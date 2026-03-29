import { elements } from "./dom.js";
import { state } from "./state.js";
import { invoke } from "./tauri.js";
import { escapeHtml } from "./utils.js";

// Clears backend logs and reloads the log panel.
export async function clearLogs(refresh) {
  try {
    await invoke("clear_logs");
  } catch (error) {
    console.error(error);
    window.alert(`Failed to clear logs: ${String(error)}`);
  }
  await refresh();
}

// Renders log history lines.
export function renderLogs() {
  const logs = state.data?.logs ?? [];
  elements.logsEl.innerHTML = logs
    .map((entry) => `<div class="log-line">${escapeHtml(entry)}</div>`)
    .join("");
}
