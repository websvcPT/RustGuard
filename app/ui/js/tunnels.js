import { elements } from "./dom.js";
import { setTab } from "./navigation.js";
import { state } from "./state.js";
import { invoke } from "./tauri.js";
import { escapeHtml } from "./utils.js";

// Leaves editing mode and returns add-tunnel form to default state.
export function resetEditMode(shouldReturn = true) {
  const targetTab = state.returnTab || "tunnels";
  state.editingIndex = null;
  state.returnTab = "tunnels";
  elements.addPanelTitle.textContent = "Add Tunnel";
  elements.saveTunnelBtn.textContent = "Save Tunnel";
  if (shouldReturn && state.tab === "add") {
    setTab(targetTab);
  }
}

// Resets add-tunnel form fields to empty values.
export function clearTunnelForm() {
  resetEditMode(false);
  elements.tunnelNameInput.value = "";
  elements.tunnelConfigInput.value = "";
}

// Opens native file picker and pre-fills form with imported tunnel data.
export async function importTunnel(refresh) {
  try {
    const loaded = await invoke("import_tunnel_from_file");
    if (!loaded) return;
    elements.tunnelNameInput.value = loaded.name;
    elements.tunnelConfigInput.value = loaded.config;
  } catch (error) {
    console.error(error);
    window.alert(`Failed to open file dialog: ${String(error)}`);
  }
  await refresh();
}

// Validates and submits tunnel from the add/edit form.
export async function saveTunnel(refresh) {
  const name = elements.tunnelNameInput.value.trim();
  const config = elements.tunnelConfigInput.value.trim();
  if (!name || !config) return;
  try {
    if (state.editingIndex === null) {
      await invoke("add_tunnel", { name, config });
    } else {
      await invoke("update_tunnel", { index: state.editingIndex, name, config });
    }
    clearTunnelForm();
    setTab("tunnels");
  } catch (error) {
    console.error(error);
    window.alert(`Failed to save tunnel: ${String(error)}`);
  }
  await refresh();
}

// Renders tunnel cards and binds action handlers.
export function renderTunnels(refresh) {
  const tunnels = state.data?.tunnels ?? [];
  elements.tunnelList.innerHTML = "";
  if (!tunnels.length) {
    elements.tunnelList.innerHTML =
      '<div class="tunnel-card">No tunnels yet. Open <strong>Add Tunnel</strong> to create one.</div>';
    return;
  }

  tunnels.forEach((tunnel, index) => {
    const card = document.createElement("div");
    card.className = "tunnel-card";
    card.innerHTML = `
      <div class="tunnel-head">
        <div class="tunnel-name">${escapeHtml(tunnel.name)}</div>
        <div class="row tunnel-actions">
          <button class="btn" data-action="edit">Edit</button>
          <button class="btn" data-action="save">Export .conf</button>
          <button class="btn ${tunnel.active ? "" : "btn-primary"} connect-btn" data-action="toggle">${tunnel.active ? "Disconnect" : "Connect"}</button>
          <img class="status-dot" src="./assets/${tunnel.active ? "dot-green" : "dot-gray"}.png" title="${tunnel.active ? "Connected" : "Disconnected"}" alt="${tunnel.active ? "Connected" : "Disconnected"}" />
        </div>
      </div>
    `;

    card.querySelector('[data-action="toggle"]').addEventListener("click", async () => {
      try {
        await invoke("set_tunnel_active", { index, active: !tunnel.active });
      } catch (error) {
        console.error(error);
        window.alert(`Failed to change tunnel state: ${String(error)}`);
      }
      await refresh();
    });

    card.querySelector('[data-action="save"]').addEventListener("click", async () => {
      try {
        await invoke("save_tunnel_to_disk", { index });
      } catch (error) {
        console.error(error);
        window.alert(`Failed to export tunnel: ${String(error)}`);
      }
      await refresh();
    });

    card.querySelector('[data-action="edit"]').addEventListener("click", () => {
      state.editingIndex = index;
      state.returnTab = state.tab;
      elements.addPanelTitle.textContent = "Edit Tunnel";
      elements.tunnelNameInput.value = tunnel.name;
      elements.tunnelConfigInput.value = tunnel.config;
      elements.saveTunnelBtn.textContent = "Update Tunnel";
      setTab("add");
    });

    elements.tunnelList.appendChild(card);
  });
}
