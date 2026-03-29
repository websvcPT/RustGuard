import { elements } from "./dom.js";
import { state } from "./state.js";

const TITLES = {
  tunnels: ["Tunnels", "Manage your saved WireGuard tunnels."],
  add: ["Add Tunnel", "Import or paste a WireGuard configuration."],
  settings: ["Settings", "Adjust app behavior and tunnel policy."],
  logs: ["Logs", "Operational events and error details."],
};

// Switches visible content panel and updates page heading/subheading.
export function setTab(tab) {
  state.tab = tab;
  elements.navButtons.forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.tab === tab);
  });
  elements.tabPanels.forEach((panel) => {
    panel.classList.toggle("hidden", panel.id !== `tab-${tab}`);
  });
  [elements.tabTitle.textContent, elements.tabSubtitle.textContent] = TITLES[tab];
}
