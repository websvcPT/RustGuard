use auto_launch::AutoLaunch;
use rfd::FileDialog;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
#[cfg(unix)]
use std::io::IsTerminal;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
#[cfg(not(target_os = "linux"))]
use tauri::tray::MouseButton;
use tauri::tray::TrayIconBuilder;
#[cfg(not(target_os = "linux"))]
use tauri::tray::TrayIconEvent;
use tauri::{Manager, State, WindowEvent};

/// Stable application identifier used in UI metadata and persisted logs.
const APP_ID: &str = "net.websvc.rustguard";
/// Stable tray identifier used to update icon state after tunnel changes.
const TRAY_ID: &str = "rustguard-tray";
/// Linux interface names in WireGuard/wg-quick should stay under 15 chars.
const MAX_TUNNEL_NAME_LEN: usize = 15;
/// GitHub API endpoint for RustGuard releases.
const RELEASES_API_URL: &str =
    "https://api.github.com/repos/websvcPT/RustGuard/releases?per_page=50";
/// Interval used to refresh sudo credentials after first successful authentication.
const SUDO_KEEPALIVE_INTERVAL_SECS: u64 = 60;
/// Tracks whether a sudo-authenticated session has been established in this process.
static SUDO_AUTHENTICATED: AtomicBool = AtomicBool::new(false);
/// Indicates whether a sudo keepalive thread is already active.
static SUDO_KEEPALIVE_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct AppSettings {
    auto_start: bool,
    start_in_tray: bool,
    check_updates: bool,
    allow_multiple_tunnels: bool,
}

impl Default for AppSettings {
    /// Returns default user settings for a first-time installation.
    fn default() -> Self {
        Self {
            auto_start: false,
            start_in_tray: false,
            check_updates: true,
            allow_multiple_tunnels: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Tunnel {
    name: String,
    config: String,
    active: bool,
}

impl Default for Tunnel {
    /// Returns an empty disconnected tunnel value for migration safety.
    fn default() -> Self {
        Self {
            name: String::new(),
            config: String::new(),
            active: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct PersistedState {
    settings: AppSettings,
    tunnels: Vec<Tunnel>,
}

#[derive(Debug, Serialize)]
struct FrontendState {
    app_id: String,
    app_version: String,
    settings_folder: String,
    settings: AppSettings,
    tunnels: Vec<Tunnel>,
    logs: Vec<String>,
    update_status: UpdateStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct UpdateStatus {
    latest_stable_version: Option<String>,
    latest_release_url: Option<String>,
    update_available: bool,
    last_checked_unix: Option<u64>,
    message: String,
    last_error: Option<String>,
}

impl Default for UpdateStatus {
    /// Returns the initial "not checked" update-check status.
    fn default() -> Self {
        Self {
            latest_stable_version: None,
            latest_release_url: None,
            update_available: false,
            last_checked_unix: None,
            message: String::from("No update check has been performed yet."),
            last_error: None,
        }
    }
}

#[derive(Debug)]
struct AppRuntime {
    state: PersistedState,
    logs: Vec<String>,
    state_file: PathBuf,
    tunnel_dir: PathBuf,
    allow_app_exit: bool,
    update_status: UpdateStatus,
}

impl AppRuntime {
    /// Initializes runtime state, storage paths, and startup logs.
    fn new() -> Self {
        let data_dir = app_data_dir();
        let tunnel_dir = data_dir.join("tunnels");
        let _ = fs::create_dir_all(&tunnel_dir);
        let state_file = data_dir.join("state.json");
        let (state, state_migrated) = load_state_with_migration(&state_file);
        let logs = vec![format!("Initialized {APP_ID}")];

        let mut runtime = Self {
            state,
            logs,
            state_file,
            tunnel_dir,
            allow_app_exit: false,
            update_status: UpdateStatus::default(),
        };
        if state_migrated {
            runtime.logs.push(String::from(
                "Migrated settings/state schema to current application version.",
            ));
            runtime.save_state();
        }
        runtime
    }

    /// Persists in-memory state to disk and records persistence errors in logs.
    fn save_state(&mut self) {
        if let Err(err) = save_state(&self.state_file, &self.state) {
            self.logs
                .push(format!("Failed to save settings/state: {err}"));
        }
    }

    /// Counts currently active tunnels for status labels and tray icon updates.
    fn active_tunnel_count(&self) -> usize {
        self.state
            .tunnels
            .iter()
            .filter(|tunnel| tunnel.active)
            .count()
    }

    /// Indicates if at least one tunnel is currently active.
    fn has_active_tunnels(&self) -> bool {
        self.active_tunnel_count() > 0
    }

    /// Converts backend runtime data into the frontend payload shape.
    fn frontend_state(&self) -> FrontendState {
        FrontendState {
            app_id: APP_ID.to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            settings_folder: app_data_dir().display().to_string(),
            settings: self.state.settings.clone(),
            tunnels: self.state.tunnels.clone(),
            logs: self.logs.clone(),
            update_status: self.update_status.clone(),
        }
    }

    /// Opens a file picker and returns a parsed tunnel payload for the add form.
    fn import_tunnel_from_file(&mut self) -> Option<ImportedTunnel> {
        let path = FileDialog::new()
            .add_filter("WireGuard config", &["conf", "txt"])
            .pick_file()?;

        match fs::read_to_string(&path) {
            Ok(config) => {
                let fallback = String::from("imported-tunnel");
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(str::to_owned)
                    .unwrap_or(fallback);
                self.logs
                    .push(format!("Loaded tunnel from {}", path.display()));
                Some(ImportedTunnel { name, config })
            }
            Err(err) => {
                self.logs.push(format!("Failed to import tunnel: {err}"));
                None
            }
        }
    }

    /// Adds a new tunnel after validating required input fields.
    fn add_tunnel(&mut self, name: String, config: String) -> Result<(), String> {
        let name = name.trim();
        let cfg = config.trim();
        if let Err(err) = validate_tunnel_payload(name, cfg, &self.state.tunnels, None) {
            self.logs.push(format!("Failed to add tunnel: {err}"));
            return Err(err);
        }

        self.state.tunnels.push(Tunnel {
            name: name.to_owned(),
            config: cfg.to_owned(),
            active: false,
        });
        self.logs.push(format!("Added tunnel '{name}'"));
        self.save_state();
        Ok(())
    }

    /// Updates an existing tunnel and supports renaming while preserving state consistency.
    fn update_tunnel(&mut self, index: usize, name: String, config: String) -> Result<(), String> {
        let name = name.trim();
        let cfg = config.trim();
        if let Err(err) = validate_tunnel_payload(name, cfg, &self.state.tunnels, Some(index)) {
            self.logs.push(format!("Failed to update tunnel: {err}"));
            return Err(err);
        }

        let old_tunnel = self
            .state
            .tunnels
            .get(index)
            .cloned()
            .ok_or_else(|| String::from("Invalid tunnel index"))?;

        if old_tunnel.active {
            let down_msg = apply_tunnel_action(
                &old_tunnel.name,
                &old_tunnel.config,
                "down",
                &self.tunnel_dir,
            )?;
            self.logs.push(down_msg);
        }

        self.state.tunnels[index].name = name.to_owned();
        self.state.tunnels[index].config = cfg.to_owned();

        if old_tunnel.active {
            match apply_tunnel_action(name, cfg, "up", &self.tunnel_dir) {
                Ok(up_msg) => {
                    self.logs.push(up_msg);
                    self.state.tunnels[index].active = true;
                }
                Err(err) => {
                    self.state.tunnels[index] = old_tunnel.clone();
                    if let Err(recover_err) = apply_tunnel_action(
                        &old_tunnel.name,
                        &old_tunnel.config,
                        "up",
                        &self.tunnel_dir,
                    ) {
                        self.logs.push(format!(
                            "Failed to recover tunnel '{}' after update failure: {recover_err}",
                            old_tunnel.name
                        ));
                    }
                    self.logs.push(format!(
                        "Failed to update active tunnel '{}': {err}",
                        old_tunnel.name
                    ));
                    return Err(err);
                }
            }
        } else {
            self.state.tunnels[index].active = false;
        }

        let old_config_path = self.tunnel_dir.join(format!("{}.conf", old_tunnel.name));
        if old_tunnel.name != name && old_config_path.exists() {
            let _ = fs::remove_file(old_config_path);
        }

        self.logs.push(format!(
            "Updated tunnel '{}'{}.",
            old_tunnel.name,
            if old_tunnel.name != name {
                format!(" -> '{name}'")
            } else {
                String::new()
            }
        ));
        self.save_state();
        Ok(())
    }

    /// Writes a tunnel config to the managed tunnel directory and optional export path.
    fn save_tunnel_to_disk(&mut self, index: usize) -> Result<(), String> {
        let tunnel = self
            .state
            .tunnels
            .get(index)
            .cloned()
            .ok_or_else(|| String::from("Invalid tunnel index"))?;
        let path = self.tunnel_dir.join(format!("{}.conf", tunnel.name));
        fs::write(&path, tunnel.config.as_bytes()).map_err(|e| e.to_string())?;
        secure_file_permissions(&path)?;
        self.logs.push(format!(
            "Saved tunnel '{}' to {}",
            tunnel.name,
            path.display()
        ));

        if let Some(export_path) = FileDialog::new()
            .set_file_name(format!("{}.conf", tunnel.name).as_str())
            .save_file()
        {
            match fs::write(&export_path, tunnel.config.as_bytes()) {
                Ok(_) => self.logs.push(format!(
                    "Exported tunnel '{}' to {}",
                    tunnel.name,
                    export_path.display()
                )),
                Err(err) => self
                    .logs
                    .push(format!("Failed to export tunnel '{}': {err}", tunnel.name)),
            }
        }

        Ok(())
    }

    /// Connects or disconnects a tunnel and updates persisted state on success.
    fn set_tunnel_active(&mut self, index: usize, active: bool) -> Result<(), String> {
        if index >= self.state.tunnels.len() {
            return Err(String::from("Invalid tunnel index"));
        }

        if active && !self.state.settings.allow_multiple_tunnels {
            for (idx, tunnel) in self.state.tunnels.iter_mut().enumerate() {
                if idx != index && tunnel.active {
                    let msg = apply_tunnel_action(
                        &tunnel.name,
                        &tunnel.config,
                        "down",
                        &self.tunnel_dir,
                    )?;
                    self.logs.push(msg);
                    tunnel.active = false;
                }
            }
        }

        let tunnel = &mut self.state.tunnels[index];
        let action = if active { "up" } else { "down" };
        match apply_tunnel_action(&tunnel.name, &tunnel.config, action, &self.tunnel_dir) {
            Ok(msg) => {
                tunnel.active = active;
                self.logs.push(msg);
                self.save_state();
                Ok(())
            }
            Err(err) => {
                let message = format!("Tunnel '{}' failed: {err}", tunnel.name);
                self.logs.push(message.clone());
                Err(err)
            }
        }
    }

    /// Applies settings from the frontend and saves them to state storage.
    fn update_settings(
        &mut self,
        auto_start: bool,
        start_in_tray: bool,
        check_updates: bool,
        allow_multiple_tunnels: bool,
    ) -> Result<(), String> {
        let previous = self.state.settings.clone();
        let should_sync_autostart = previous.auto_start != auto_start
            || (auto_start && previous.start_in_tray != start_in_tray);
        self.state.settings.auto_start = auto_start;
        self.state.settings.start_in_tray = start_in_tray;
        self.state.settings.check_updates = check_updates;
        self.state.settings.allow_multiple_tunnels = allow_multiple_tunnels;

        if should_sync_autostart {
            if let Err(err) = sync_autostart(&self.state.settings) {
                self.state.settings = previous;
                self.logs
                    .push(format!("Failed to apply startup settings: {err}"));
                return Err(err);
            }
        }

        self.logs.push(String::from("Settings updated."));
        self.save_state();
        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct ImportedTunnel {
    name: String,
    config: String,
}

#[tauri::command]
/// Returns the full state payload needed to render the frontend.
fn get_state(runtime: State<'_, Mutex<AppRuntime>>) -> Result<FrontendState, String> {
    let runtime = runtime.lock().map_err(|e| e.to_string())?;
    Ok(runtime.frontend_state())
}

#[tauri::command]
/// Imports a tunnel from disk and returns parsed values for form prefill.
fn import_tunnel_from_file(
    runtime: State<'_, Mutex<AppRuntime>>,
) -> Result<Option<ImportedTunnel>, String> {
    let mut runtime = runtime.lock().map_err(|e| e.to_string())?;
    Ok(runtime.import_tunnel_from_file())
}

#[tauri::command]
/// Creates a new tunnel entry from user-provided name and configuration.
fn add_tunnel(
    name: String,
    config: String,
    runtime: State<'_, Mutex<AppRuntime>>,
) -> Result<(), String> {
    let mut runtime = runtime.lock().map_err(|e| e.to_string())?;
    runtime.add_tunnel(name, config)
}

#[tauri::command]
/// Updates an existing tunnel by index, including tunnel rename support.
fn update_tunnel(
    index: usize,
    name: String,
    config: String,
    runtime: State<'_, Mutex<AppRuntime>>,
) -> Result<(), String> {
    let mut runtime = runtime.lock().map_err(|e| e.to_string())?;
    runtime.update_tunnel(index, name, config)
}

#[tauri::command]
/// Changes tunnel connection state (connect/disconnect) for a given tunnel index.
fn set_tunnel_active(
    index: usize,
    active: bool,
    app: tauri::AppHandle,
    runtime: State<'_, Mutex<AppRuntime>>,
) -> Result<(), String> {
    let mut runtime = runtime.lock().map_err(|e| e.to_string())?;
    runtime.set_tunnel_active(index, active)?;
    let has_active_tunnels = runtime.has_active_tunnels();
    drop(runtime);
    sync_tray_icon(&app, has_active_tunnels)
}

#[tauri::command]
/// Saves the selected tunnel config to disk and optional user-selected location.
fn save_tunnel_to_disk(index: usize, runtime: State<'_, Mutex<AppRuntime>>) -> Result<(), String> {
    let mut runtime = runtime.lock().map_err(|e| e.to_string())?;
    runtime.save_tunnel_to_disk(index)
}

#[tauri::command]
/// Updates app settings from frontend controls.
fn update_settings(
    auto_start: bool,
    start_in_tray: bool,
    check_updates: bool,
    allow_multiple_tunnels: bool,
    runtime: State<'_, Mutex<AppRuntime>>,
) -> Result<(), String> {
    let mut runtime = runtime.lock().map_err(|e| e.to_string())?;
    runtime.update_settings(
        auto_start,
        start_in_tray,
        check_updates,
        allow_multiple_tunnels,
    )
}

#[tauri::command]
/// Checks the upstream repository for a newer stable (non-RC) version.
fn check_for_updates(runtime: State<'_, Mutex<AppRuntime>>) -> Result<UpdateStatus, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let mut runtime = runtime.lock().map_err(|e| e.to_string())?;
    let status = check_for_updates_internal(&current_version)?;
    let message = status.message.clone();
    runtime.update_status = status.clone();
    runtime
        .logs
        .push(format!("Update check completed: {}", message));
    Ok(status)
}

#[tauri::command]
/// Clears log history and appends a confirmation entry.
fn clear_logs(runtime: State<'_, Mutex<AppRuntime>>) -> Result<(), String> {
    let mut runtime = runtime.lock().map_err(|e| e.to_string())?;
    runtime.logs.clear();
    runtime.logs.push(String::from("Logs cleared."));
    Ok(())
}

/// Resolves the compatibility-sensitive app data directory path per OS.
fn app_data_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Some(dir) = dirs::data_dir() {
            return dir.join("WebSVC").join("rustaguard");
        }
    }

    #[cfg(target_os = "linux")]
    if let Some(home) = linux_launcher_user_home() {
        return home.join(".websvc").join("rustaguard");
    }

    if let Some(home) = dirs::home_dir() {
        return home.join(".websvc").join("rustaguard");
    }

    PathBuf::from(".websvc/rustaguard")
}

/// Resolves the original launching user's home when running as root through pkexec/sudo.
#[cfg(target_os = "linux")]
fn linux_launcher_user_home() -> Option<PathBuf> {
    if let Ok(uid) = env::var("PKEXEC_UID") {
        if let Some(home) = passwd_home_by_uid(&uid) {
            return Some(home);
        }
    }

    if let Ok(user) = env::var("SUDO_USER") {
        if let Some(home) = passwd_home_by_name(&user) {
            return Some(home);
        }
    }

    None
}

/// Looks up a user's home directory in `/etc/passwd` by username.
#[cfg(target_os = "linux")]
fn passwd_home_by_name(user_name: &str) -> Option<PathBuf> {
    let passwd = fs::read_to_string("/etc/passwd").ok()?;
    for line in passwd.lines() {
        let mut fields = line.split(':');
        let name = fields.next()?;
        let _passwd = fields.next()?;
        let _uid = fields.next()?;
        let _gid = fields.next()?;
        let _gecos = fields.next()?;
        let home = fields.next()?;
        if name == user_name {
            return Some(PathBuf::from(home));
        }
    }
    None
}

/// Looks up a user's home directory in `/etc/passwd` by numeric uid.
#[cfg(target_os = "linux")]
fn passwd_home_by_uid(uid: &str) -> Option<PathBuf> {
    let passwd = fs::read_to_string("/etc/passwd").ok()?;
    for line in passwd.lines() {
        let mut fields = line.split(':');
        let _name = fields.next()?;
        let _passwd = fields.next()?;
        let entry_uid = fields.next()?;
        let _gid = fields.next()?;
        let _gecos = fields.next()?;
        let home = fields.next()?;
        if entry_uid == uid {
            return Some(PathBuf::from(home));
        }
    }
    None
}

/// Loads persisted state and reports whether canonical migration is required.
fn load_state_with_migration(path: &Path) -> (PersistedState, bool) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    match fs::read_to_string(path) {
        Ok(content) => {
            let Ok(raw_json) = serde_json::from_str::<serde_json::Value>(&content) else {
                return (PersistedState::default(), false);
            };
            let state: PersistedState =
                serde_json::from_value(raw_json.clone()).unwrap_or_default();
            let normalized_json = serde_json::to_value(&state).unwrap_or(serde_json::Value::Null);
            (state, raw_json != normalized_json)
        }
        Err(_) => (PersistedState::default(), false),
    }
}

/// Serializes and writes state as pretty JSON to disk.
fn save_state(path: &Path, state: &PersistedState) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let content = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

/// Validates tunnel payload and enforces uniqueness among saved tunnel names.
fn validate_tunnel_payload(
    name: &str,
    config: &str,
    tunnels: &[Tunnel],
    current_index: Option<usize>,
) -> Result<(), String> {
    if name.is_empty() || config.is_empty() {
        return Err(String::from("Tunnel name and configuration are required"));
    }

    validate_tunnel_name(name)?;

    if tunnels
        .iter()
        .enumerate()
        .any(|(idx, tunnel)| Some(idx) != current_index && tunnel.name.eq_ignore_ascii_case(name))
    {
        return Err(format!("Tunnel name '{name}' already exists"));
    }

    Ok(())
}

/// Ensures tunnel names match wg-quick compatible interface naming constraints.
fn validate_tunnel_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(String::from("Tunnel name cannot be empty"));
    }

    if name.len() > MAX_TUNNEL_NAME_LEN {
        return Err(format!(
            "Tunnel name cannot exceed {MAX_TUNNEL_NAME_LEN} characters"
        ));
    }

    let valid = name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '=' | '+' | '.' | '-'));

    if !valid {
        return Err(String::from(
            "Tunnel name may only contain: a-z, A-Z, 0-9, _, =, +, ., -",
        ));
    }

    Ok(())
}

/// Restricts managed tunnel config permissions to owner-only access when supported.
fn secure_file_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
}

/// Queries GitHub releases and returns update information for stable versions only.
fn check_for_updates_internal(current_version: &str) -> Result<UpdateStatus, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let releases = client
        .get(RELEASES_API_URL)
        .header("User-Agent", "RustGuard")
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json::<Vec<GitHubRelease>>()
        .map_err(|e| e.to_string())?;

    let latest_stable = latest_stable_release(&releases);

    let mut status = UpdateStatus {
        last_checked_unix: Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs(),
        ),
        ..UpdateStatus::default()
    };

    let current_stable = Version::parse(current_version)
        .ok()
        .filter(|v| v.pre.is_empty());

    if let Some((latest, release)) = latest_stable {
        status.latest_stable_version = Some(latest.to_string());
        status.latest_release_url = Some(release.html_url.clone());
        status.update_available = current_stable
            .as_ref()
            .map(|current| latest > *current)
            .unwrap_or(true);
        status.message = if status.update_available {
            format!("A newer stable version is available: {}", latest)
        } else {
            format!("You are running the latest stable version: {}", latest)
        };
        status.last_error = None;
        return Ok(status);
    }

    status.message = String::from("No stable release found in repository releases.");
    status.last_error = Some(String::from("No stable release found."));
    Ok(status)
}

/// Finds the latest non-draft, non-prerelease stable semantic version from GitHub releases.
fn latest_stable_release(releases: &[GitHubRelease]) -> Option<(Version, &GitHubRelease)> {
    releases
        .iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|release| {
            let normalized = release.tag_name.trim_start_matches('v').trim();
            Version::parse(normalized)
                .ok()
                .filter(|version| version.pre.is_empty())
                .map(|version| (version, release))
        })
        .max_by(|(left, _), (right, _)| left.cmp(right))
}

/// Returns the tray icon image variant based on active tunnel state.
fn tray_icon(active: bool) -> Image<'static> {
    if active {
        tauri::include_image!("Icon/rustguard_tray_active.png")
    } else {
        tauri::include_image!("Icon/rustguard_tray_idle.png")
    }
}

/// Applies the active/disconnected tray icon according to tunnel state.
fn sync_tray_icon(app: &tauri::AppHandle, active: bool) -> Result<(), String> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_icon(Some(tray_icon(active)))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Restores and focuses the primary application window.
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Exits the process from tray menu flow after allowing close events.
fn exit_app_from_tray(app: &tauri::AppHandle) {
    if let Ok(mut runtime) = app.state::<Mutex<AppRuntime>>().lock() {
        runtime.allow_app_exit = true;
        runtime
            .logs
            .push(String::from("Exiting RustGuard from tray menu."));
    }
    app.exit(0);
}

/// Creates tray icon and menu handlers for open/exit actions.
fn create_tray(app: &tauri::AppHandle, active: bool) -> Result<(), String> {
    let open_item = MenuItem::with_id(app, "open", "Open RustGuard", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let exit_item =
        MenuItem::with_id(app, "exit", "Exit", true, None::<&str>).map_err(|e| e.to_string())?;
    let menu = Menu::with_items(app, &[&open_item, &exit_item]).map_err(|e| e.to_string())?;

    let mut tray_builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("RustGuard")
        .icon(tray_icon(active))
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "exit" => exit_app_from_tray(app),
            _ => {}
        });

    #[cfg(target_os = "linux")]
    {
        // Linux tray backends often do not emit click/double-click events reliably.
        // Open the tray menu on left click so "Open RustGuard" remains reachable quickly.
        tray_builder = tray_builder.show_menu_on_left_click(true);
    }

    #[cfg(not(target_os = "linux"))]
    {
        tray_builder = tray_builder
            .show_menu_on_left_click(false)
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } = event
                {
                    show_main_window(tray.app_handle());
                }
            });
    }

    tray_builder.build(app).map_err(|e| e.to_string())?;

    Ok(())
}

/// Logs tray initialization errors to the in-app logs.
fn log_tray_init_error(app: &tauri::AppHandle, err: &str) {
    if let Ok(mut runtime) = app.state::<Mutex<AppRuntime>>().lock() {
        runtime
            .logs
            .push(format!("Failed to initialize system tray: {err}"));
    }
}

/// Defers tray creation to the main loop to avoid early GTK startup timing issues.
fn schedule_tray_initialization(app: tauri::AppHandle, active: bool) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(250));
        let app_for_init = app.clone();
        if let Err(err) = app.run_on_main_thread(move || {
            if let Err(tray_err) = create_tray(&app_for_init, active) {
                log_tray_init_error(&app_for_init, &tray_err);
            }
        }) {
            log_tray_init_error(&app, &err.to_string());
        }
    });
}

/// Enables/disables startup registration using platform-specific auto-launch hooks.
fn sync_autostart(settings: &AppSettings) -> Result<(), String> {
    let executable = env::current_exe().map_err(|e| e.to_string())?;
    let executable = executable
        .to_str()
        .ok_or_else(|| String::from("Executable path contains invalid UTF-8"))?;
    let args: &[&str] = if settings.start_in_tray {
        &["--start-in-tray"]
    } else {
        &[]
    };

    #[cfg(target_os = "linux")]
    {
        let launcher = AutoLaunch::new("RustGuard", executable, args);
        if settings.auto_start {
            return launcher.enable().map_err(|e| e.to_string());
        }
        launcher.disable().map_err(|e| e.to_string())
    }

    #[cfg(target_os = "windows")]
    {
        let launcher = AutoLaunch::new("RustGuard", executable, args);
        if settings.auto_start {
            return launcher.enable().map_err(|e| e.to_string());
        }
        return launcher.disable().map_err(|e| e.to_string());
    }

    #[cfg(target_os = "macos")]
    {
        let launcher = AutoLaunch::new("RustGuard", executable, true, args);
        if settings.auto_start {
            return launcher.enable().map_err(|e| e.to_string());
        }
        return launcher.disable().map_err(|e| e.to_string());
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let _ = settings;
        Ok(())
    }
}

/// Executes tunnel up/down behavior and writes config files before command dispatch.
fn apply_tunnel_action(
    name: &str,
    config: &str,
    action: &str,
    tunnel_dir: &Path,
) -> Result<String, String> {
    let path = tunnel_dir.join(format!("{}.conf", name));
    fs::write(&path, config.as_bytes()).map_err(|e| e.to_string())?;
    secure_file_permissions(&path)?;

    if cfg!(target_os = "linux") {
        let output = run_wg_action_with_elevation(action, &path)?;

        if output.status.success() {
            return Ok(format!("Tunnel '{name}' {action} succeeded"));
        }

        let details = command_output_details(&output);
        if details.is_empty() {
            return Err(format!(
                "wg-quick {action} failed with status {}",
                output.status
            ));
        }

        return Err(format!(
            "wg-quick {action} failed with status {}.\nwg-quick output:\n{details}",
            output.status
        ));
    }

    Ok(format!(
        "Tunnel '{name}' marked as '{}' (native WireGuard control for this OS is not configured yet)",
        action
    ))
}

/// Runs wg-quick using root if available, otherwise tries desktop elevation prompts.
fn run_wg_action_with_elevation(action: &str, path: &Path) -> Result<Output, String> {
    if let Some(status) = run_wg_quick_as_root(action, path)? {
        return Ok(status);
    }

    if let Some(status) = run_wg_quick_with_sudo(action, path)? {
        return Ok(status);
    }

    if let Some(status) = run_wg_quick_with_pkexec(action, path)? {
        return Ok(status);
    }

    Err(String::from(
        "No privilege elevation tool available. Install/configure sudo or pkexec.",
    ))
}

/// Starts a background sudo keepalive loop once authentication succeeds.
fn start_sudo_keepalive_if_needed() {
    if SUDO_KEEPALIVE_STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    thread::spawn(|| loop {
        thread::sleep(Duration::from_secs(SUDO_KEEPALIVE_INTERVAL_SECS));
        if !SUDO_AUTHENTICATED.load(Ordering::Relaxed) {
            continue;
        }
        if !command_exists("sudo") {
            continue;
        }
        let Ok(status) = Command::new("sudo").arg("-n").arg("-v").status() else {
            continue;
        };
        if !status.success() {
            SUDO_AUTHENTICATED.store(false, Ordering::Relaxed);
        }
    });
}

/// Executes wg-quick directly when already running with root privileges.
fn run_wg_quick_as_root(action: &str, path: &Path) -> Result<Option<Output>, String> {
    if command_exists("id") {
        let id_output = Command::new("id")
            .arg("-u")
            .output()
            .map_err(|e| e.to_string())?;
        if id_output.status.success() && String::from_utf8_lossy(&id_output.stdout).trim() == "0" {
            let output = Command::new("wg-quick")
                .arg(action)
                .arg(path)
                .output()
                .map_err(|e| e.to_string())?;
            return Ok(Some(output));
        }
    }
    Ok(None)
}

/// Executes wg-quick through sudo, prompting once and then reusing session credentials.
fn run_wg_quick_with_sudo(action: &str, path: &Path) -> Result<Option<Output>, String> {
    if !command_exists("sudo") {
        return Ok(None);
    }

    let status = Command::new("sudo")
        .arg("-n")
        .arg("-v")
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        SUDO_AUTHENTICATED.store(true, Ordering::Relaxed);
        start_sudo_keepalive_if_needed();
        let output = Command::new("sudo")
            .arg("-n")
            .arg("wg-quick")
            .arg(action)
            .arg(path)
            .output()
            .map_err(|e| e.to_string())?;
        return Ok(Some(output));
    }

    if env::var("DISPLAY").is_ok() || env::var("WAYLAND_DISPLAY").is_ok() {
        let askpass_candidates = [
            "/usr/bin/ssh-askpass",
            "/usr/lib/ssh/ssh-askpass",
            "/usr/libexec/ssh-askpass",
            "/usr/bin/ksshaskpass",
        ];
        if let Some(askpass_path) = askpass_candidates
            .iter()
            .find(|candidate| Path::new(candidate).exists())
        {
            let output = Command::new("sudo")
                .arg("-A")
                .arg("wg-quick")
                .arg(action)
                .arg(path)
                .env("SUDO_ASKPASS", askpass_path)
                .output()
                .map_err(|e| e.to_string())?;
            if output.status.success() {
                SUDO_AUTHENTICATED.store(true, Ordering::Relaxed);
                start_sudo_keepalive_if_needed();
            }
            return Ok(Some(output));
        }
    }

    #[cfg(unix)]
    if std::io::stdin().is_terminal() {
        let output = Command::new("sudo")
            .arg("wg-quick")
            .arg(action)
            .arg(path)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            SUDO_AUTHENTICATED.store(true, Ordering::Relaxed);
            start_sudo_keepalive_if_needed();
        }
        return Ok(Some(output));
    }

    Ok(None)
}

/// Falls back to pkexec when sudo cannot prompt (typical app-launch context).
fn run_wg_quick_with_pkexec(action: &str, path: &Path) -> Result<Option<Output>, String> {
    if !command_exists("pkexec") {
        return Ok(None);
    }

    let output = Command::new("pkexec")
        .arg("wg-quick")
        .arg(action)
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(Some(output))
}

/// Converts command stdout/stderr into a readable error details string.
fn command_output_details(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Returns whether an executable exists in PATH.
fn command_exists(command: &str) -> bool {
    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path_var).any(|path_dir| path_dir.join(command).exists())
}

/// Bootstraps Tauri, registers commands, and starts the desktop event loop.
fn main() {
    let cli_start_in_tray = env::args().any(|arg| arg == "--start-in-tray");

    tauri::Builder::default()
        .manage(Mutex::new(AppRuntime::new()))
        .setup(move |app| {
            let startup_handle = app.handle().clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(30));
                let should_check = startup_handle
                    .state::<Mutex<AppRuntime>>()
                    .lock()
                    .map(|runtime| runtime.state.settings.check_updates)
                    .unwrap_or(false);
                if !should_check {
                    return;
                }

                let current_version = env!("CARGO_PKG_VERSION").to_string();
                match check_for_updates_internal(&current_version) {
                    Ok(status) => {
                        if let Ok(mut runtime) = startup_handle.state::<Mutex<AppRuntime>>().lock()
                        {
                            runtime.update_status = status.clone();
                            runtime.logs.push(format!(
                                "Startup update check completed: {}",
                                status.message
                            ));
                        }
                    }
                    Err(err) => {
                        if let Ok(mut runtime) = startup_handle.state::<Mutex<AppRuntime>>().lock()
                        {
                            runtime.update_status.message =
                                String::from("Failed to check updates at startup.");
                            runtime.update_status.last_error = Some(err.clone());
                            runtime.update_status.last_checked_unix = Some(
                                SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            );
                            runtime
                                .logs
                                .push(format!("Startup update check failed: {err}"));
                        }
                    }
                }
            });

            let has_active_tunnels = app
                .state::<Mutex<AppRuntime>>()
                .lock()
                .map(|runtime| runtime.has_active_tunnels())
                .unwrap_or(false);

            schedule_tray_initialization(app.handle().clone(), has_active_tunnels);

            // Start hidden only when explicitly launched with --start-in-tray
            // (e.g., autostart flow). Manual app launches should open the window.
            let should_start_hidden = cli_start_in_tray;

            if should_start_hidden {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let should_prevent = window
                    .app_handle()
                    .state::<Mutex<AppRuntime>>()
                    .lock()
                    .map(|runtime| !runtime.allow_app_exit)
                    .unwrap_or(true);

                if should_prevent {
                    api.prevent_close();
                    let _ = window.hide();
                    if let Ok(mut runtime) = window.app_handle().state::<Mutex<AppRuntime>>().lock()
                    {
                        runtime.logs.push(String::from(
                            "Window hidden to system tray. Use tray menu to exit.",
                        ));
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            import_tunnel_from_file,
            add_tunnel,
            update_tunnel,
            set_tunnel_active,
            save_tunnel_to_disk,
            update_settings,
            check_for_updates,
            clear_logs
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    /// Verifies persisted state is correctly serialized and deserialized.
    fn state_round_trip() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("state.json");
        let state = PersistedState {
            settings: AppSettings {
                auto_start: true,
                start_in_tray: true,
                check_updates: false,
                allow_multiple_tunnels: true,
            },
            tunnels: vec![Tunnel {
                name: String::from("wg0"),
                config: String::from("[Interface]\nPrivateKey=abc"),
                active: false,
            }],
        };

        save_state(&path, &state).expect("save state");
        let (loaded, _) = load_state_with_migration(&path);

        assert_eq!(loaded.settings.auto_start, state.settings.auto_start);
        assert_eq!(loaded.settings.start_in_tray, state.settings.start_in_tray);
        assert_eq!(loaded.tunnels.len(), 1);
        assert_eq!(loaded.tunnels[0].name, "wg0");
    }

    #[test]
    /// Ensures the data path preserves the historical `rustaguard` suffix.
    fn linux_data_path_suffix_is_expected() {
        let dir = app_data_dir();
        let path = dir.to_string_lossy();
        assert!(path.contains("rustaguard"));
    }

    #[test]
    /// Validates supported and unsupported tunnel interface names.
    fn tunnel_name_validation_matches_expected_charset_and_length() {
        assert!(validate_tunnel_name("wg0").is_ok());
        assert!(validate_tunnel_name("nd-rog1-vdf").is_ok());
        assert!(validate_tunnel_name("name_with+ok.1").is_ok());
        assert!(validate_tunnel_name("too-long-interface-name").is_err());
        assert!(validate_tunnel_name("bad name").is_err());
        assert!(validate_tunnel_name("bad/slash").is_err());
    }

    #[test]
    /// Ensures tunnel names remain unique across add/update operations.
    fn tunnel_name_uniqueness_validation() {
        let tunnels = vec![
            Tunnel {
                name: String::from("wg0"),
                config: String::from("[Interface]\nPrivateKey=abc"),
                active: false,
            },
            Tunnel {
                name: String::from("office"),
                config: String::from("[Interface]\nPrivateKey=xyz"),
                active: false,
            },
        ];

        let duplicate =
            validate_tunnel_payload("WG0", "[Interface]\nPrivateKey=qwe", &tunnels, None);
        assert!(duplicate.is_err());

        let update_self =
            validate_tunnel_payload("office", "[Interface]\nPrivateKey=qwe", &tunnels, Some(1));
        assert!(update_self.is_ok());
    }

    #[test]
    /// Ignores release tags that are pre-release candidates when finding latest stable.
    fn latest_stable_release_ignores_rc_versions() {
        let releases = vec![
            GitHubRelease {
                tag_name: String::from("v1.0.0-rc.10"),
                html_url: String::from("https://example.invalid/rc"),
                draft: false,
                prerelease: true,
            },
            GitHubRelease {
                tag_name: String::from("v1.0.0"),
                html_url: String::from("https://example.invalid/stable"),
                draft: false,
                prerelease: false,
            },
        ];
        let (version, release) = latest_stable_release(&releases).expect("stable release");
        assert_eq!(version, Version::parse("1.0.0").expect("version parse"));
        assert_eq!(release.html_url, "https://example.invalid/stable");
    }

    #[test]
    /// Migrates raw state JSON by adding defaults and pruning deprecated keys.
    fn load_state_migrates_settings_schema() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("state.json");
        let raw = serde_json::json!({
            "settings": {
                "auto_start": true,
                "check_updates": false,
                "deprecated_toggle": true
            },
            "tunnels": [],
            "legacy_field": "remove-me"
        });
        fs::write(&path, serde_json::to_string_pretty(&raw).expect("json"))
            .expect("write raw state");

        let (loaded, migrated) = load_state_with_migration(&path);
        assert!(migrated);
        assert!(loaded.settings.auto_start);
        assert!(!loaded.settings.check_updates);
        assert!(!loaded.settings.start_in_tray);
        assert!(!loaded.settings.allow_multiple_tunnels);
    }
}
