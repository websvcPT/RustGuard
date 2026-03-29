import { elements } from "./js/dom.js";
import { clearLogs, renderLogs } from "./js/logs.js";
import { setTab } from "./js/navigation.js";
import { checkForUpdates, saveSettings, renderSettings } from "./js/settings.js";
import { state } from "./js/state.js";
import { invoke } from "./js/tauri.js";
import {
  clearTunnelForm,
  importTunnel,
  renderTunnels,
  resetEditMode,
  saveTunnel,
} from "./js/tunnels.js";

if (!invoke) {
  window.alert("Tauri bridge is not available. Please launch RustGuard as a desktop app.");
}

// Renders shell-level counters and top status pill.
function renderShell() {
  const tunnels = state.data?.tunnels ?? [];
  const activeCount = tunnels.filter((t) => t.active).length;
  document.getElementById("savedCount").textContent = String(tunnels.length);
  document.getElementById("activeCount").textContent = String(activeCount);
  elements.appVersionLabel.textContent = `RustGuard ${state.data?.app_version ?? "0.0.0"}`;

  if (activeCount > 0) {
    elements.statusPill.textContent = `Connected (${activeCount})`;
    elements.statusPill.classList.remove("idle");
    elements.statusPill.classList.add("connected");
  } else {
    elements.statusPill.textContent = "Idle";
    elements.statusPill.classList.remove("connected");
    elements.statusPill.classList.add("idle");
  }
}

// Pulls fresh state from backend and re-renders all screens.
async function refresh() {
  try {
    state.data = await invoke("get_state");
    renderShell();
    renderTunnels(refresh);
    renderSettings();
    renderLogs();
  } catch (error) {
    console.error(error);
    window.alert(`Failed to load app state: ${String(error)}`);
  }
}

elements.navButtons.forEach((button) => {
  button.addEventListener("click", () => {
    if (button.dataset.tab !== "add") {
      resetEditMode(false);
    } else if (state.editingIndex === null) {
      clearTunnelForm();
    }
    setTab(button.dataset.tab);
  });
});

elements.toAddBtn.addEventListener("click", () => {
  clearTunnelForm();
  setTab("add");
});

elements.importBtn.addEventListener("click", async () => {
  await importTunnel(refresh);
});

elements.saveTunnelBtn.addEventListener("click", async () => {
  await saveTunnel(refresh);
});

elements.clearTunnelBtn.addEventListener("click", clearTunnelForm);

elements.cancelTunnelBtn.addEventListener("click", resetEditMode);

elements.clearLogsBtn.addEventListener("click", async () => {
  await clearLogs(refresh);
});

elements.autoStart.addEventListener("change", async () => {
  await saveSettings(refresh);
});

elements.startInTray.addEventListener("change", async () => {
  await saveSettings(refresh);
});

elements.checkUpdates.addEventListener("change", async () => {
  await saveSettings(refresh);
});

elements.allowMultiple.addEventListener("change", async () => {
  await saveSettings(refresh);
});

elements.checkVersionBtn.addEventListener("click", async () => {
  await checkForUpdates(refresh);
});

setTab("tunnels");
refresh();

setTimeout(async () => {
  if (!state.data?.settings?.check_updates) return;
  await checkForUpdates(refresh);
}, 30000);
