# Copilot Instructions for RustGuard

## Build, test, and lint

- Build (dev): `cargo build --manifest-path app/Cargo.toml`
- Run app locally: `cargo run --manifest-path app/Cargo.toml`
- Run Tauri app (GUI): `cargo tauri dev --manifest-path app/Cargo.toml`
- Build (release): `cargo build --release --manifest-path app/Cargo.toml`
- Format check (CI): `cargo fmt --all --check --manifest-path app/Cargo.toml`
- Lint (CI, warnings are errors): `cargo clippy --all-targets --all-features --manifest-path app/Cargo.toml -- -D warnings`
- Test suite (CI): `cargo test --all-targets --manifest-path app/Cargo.toml`
- Single test: `cargo test --manifest-path app/Cargo.toml state_round_trip -- --exact`

Packaging scripts used by release automation:

- Linux artifacts: `./scripts/build_linux.sh <version>` (builds `target/release/rustguard`, then resets `app/Cargo.toml` version to `0.0.0`)
- Windows artifacts (PowerShell): `./scripts/build_windows.ps1 <version>` (builds `target/release/rustguard.exe`, then resets `app/Cargo.toml` version to `0.0.0`)

## High-level architecture

- App is a single Rust crate in `app/` using Tauri:
  - Backend commands/state in `app/src/main.rs`
  - Frontend UI in `app/ui/` (`index.html`, `styles.css`, `app.js`)
- `AppRuntime` owns runtime state (`PersistedState`, logs, state/tunnel paths) and exposes command handlers for frontend actions.
- Persistence is JSON-based (`state.json`) via `load_state`/`save_state`; tunnels and settings are stored together.
- Tunnel activation flows through `set_tunnel_active` → `apply_tunnel_action`.
  - On Linux, `wg-quick up/down <config-path>` is executed.
  - On non-Linux targets, tunnel state is marked/logged but native control is intentionally not implemented.
- Window/config metadata is defined in `app/tauri.conf.json`; frontend is loaded from `app/ui`.
- CI and release structure:
  - `.github/workflows/ci.yml`: fmt + clippy + tests on push/PR.
  - `.github/workflows/version-tag.yml`: semantic tag creation from commit messages on `master`.
  - `.github/workflows/release.yml`: builds Linux/Windows artifacts on tags and publishes release.
  - `.github/workflows/pages.yml`: deploys static `website/` content to GitHub Pages.

## Key repository conventions

- Keep app identity/pathing consistent with existing constants and docs:
  - App ID: `net.websvc.rustguard`
  - Data dir suffix is `rustaguard` (spelled this way in both code and README); treat as compatibility-sensitive.
- Root-level layout is split: application crate/assets are under `app/`, while release scripts and packaging outputs stay at repo root.
- Settings and tunnels are persisted under the same app data root; avoid introducing alternate storage locations without migration handling.
- UI structure is tab-driven in `app/ui/app.js` with a light theme in `app/ui/styles.css`.
- Logs are user-visible in-app (`Vec<String>`), so operational errors should be surfaced as log entries rather than silently ignored.
- CI lint policy is strict (`-D warnings`); code changes should stay clippy-clean across all targets/features used in CI.
- Version tagging relies on commit message/body markers `(MAJOR)` and `(MINOR)` in `version-tag.yml`; preserve this when changing release/version workflows.
