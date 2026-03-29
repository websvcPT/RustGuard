import { elements } from "./dom.js";
import { state } from "./state.js";
import { invoke } from "./tauri.js";

function showToast(message, tone = "success") {
  const toast = document.createElement("div");
  toast.className = `toast toast-${tone}`;
  toast.textContent = message;
  elements.toastContainer.appendChild(toast);
  requestAnimationFrame(() => {
    toast.classList.add("visible");
  });
  const timer = setTimeout(() => {
    toast.classList.remove("visible");
    setTimeout(() => toast.remove(), 180);
  }, 3000);
  state.toastTimers.push(timer);
}

function setCheckVersionButtonBusy(isBusy) {
  state.checkingUpdates = isBusy;
  elements.checkVersionBtn.disabled = isBusy;
  elements.checkVersionBtn.classList.toggle("loading", isBusy);
  elements.checkVersionBtn.textContent = isBusy
    ? "Checking..."
    : "Check for updates";
}

function renderUpdateStatus() {
  const status = state.data?.update_status;
  if (!status) return;
  let message = status.message ?? "No update check has been performed yet.";
  if (status.update_available && status.latest_release_url) {
    message += ` Open: ${status.latest_release_url}`;
  }
  elements.updateStatusMessage.textContent = message;
}

// Persists settings toggles to backend storage.
export async function saveSettings(refresh) {
  try {
    await invoke("update_settings", {
      autoStart: elements.autoStart.checked,
      startInTray: elements.startInTray.checked,
      checkUpdates: elements.checkUpdates.checked,
      allowMultipleTunnels: elements.allowMultiple.checked,
    });
    showToast("Settings saved.");
  } catch (error) {
    console.error(error);
    window.alert(`Failed to save settings: ${String(error)}`);
  }
  await refresh();
}

// Runs a manual update check and updates UI state with loading feedback.
export async function checkForUpdates(refresh) {
  if (state.checkingUpdates) return;
  setCheckVersionButtonBusy(true);
  try {
    await invoke("check_for_updates");
    showToast("Update check completed.");
  } catch (error) {
    console.error(error);
    window.alert(`Failed to check for updates: ${String(error)}`);
  } finally {
    setCheckVersionButtonBusy(false);
  }
  await refresh();
}

// Applies settings and app metadata values to their controls.
export function renderSettings() {
  const settings = state.data?.settings;
  if (!settings) return;
  elements.autoStart.checked = settings.auto_start;
  elements.startInTray.checked = settings.start_in_tray;
  elements.checkUpdates.checked = settings.check_updates;
  elements.allowMultiple.checked = settings.allow_multiple_tunnels;
  elements.currentVersionValue.textContent = state.data.app_version;
  elements.settingsFolder.textContent = state.data.settings_folder;
  renderUpdateStatus();
  if (!state.checkingUpdates) {
    setCheckVersionButtonBusy(false);
  }
}
