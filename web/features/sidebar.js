import { safeStorageSet } from "../core/storage.js";
import { layoutViewportWidth } from "../core/ui-scale.js";
import { artifactWidthPixels, syncArtifactLayout } from "./artifacts/model.js";
import { elements } from "../state/elements.js";
import { state } from "../state/store.js";

export function closeSidebar() {
  elements.sidebar.classList.remove("open");
  elements.sidebarScrim.classList.remove("visible");
  elements.sidebarScrim.tabIndex = -1;
}

export function setSidebarCollapsed(collapsed, { automatic = false } = {}) {
  state.sidebarCollapsed = Boolean(collapsed);
  state.sidebarAutoCollapsed = Boolean(automatic && collapsed);
  elements.appShell?.classList.toggle("is-sidebar-collapsed", state.sidebarCollapsed);
  if (elements.sidebarExpandButton) elements.sidebarExpandButton.hidden = !state.sidebarCollapsed;
  if (elements.sidebarCollapseButton) elements.sidebarCollapseButton.hidden = state.sidebarCollapsed;
  if (state.sidebarCollapsed) closeSidebar();
  if (!automatic) safeStorageSet("gqy.web.sidebarCollapsed", String(state.sidebarCollapsed));
  syncArtifactLayout?.();
}

export function syncSidebarSpace() {
  if (layoutViewportWidth() <= 760) {
    if (state.sidebarAutoCollapsed) setSidebarCollapsed(false, { automatic: true });
    return;
  }
  const shellWidth = elements.appShell.clientWidth;
  const sidebarWidth = Number.parseFloat(getComputedStyle(elements.appShell).getPropertyValue("--sidebar-width")) || 252;
  const artifactWidth = state.artifactOpen && !state.artifactMaximized ? artifactWidthPixels() + 26 : 0;
  const availableWhenExpanded = shellWidth - sidebarWidth - artifactWidth;
  if (!state.sidebarCollapsed && availableWhenExpanded < 360) {
    setSidebarCollapsed(true, { automatic: true });
  } else if (state.sidebarAutoCollapsed && availableWhenExpanded >= 420) {
    setSidebarCollapsed(false, { automatic: true });
  }
}

export function openSidebar(opener = document.activeElement) {
  state.sidebarOpener = opener;
  elements.sidebar.classList.add("open");
  elements.sidebarScrim.classList.add("visible");
  elements.sidebarScrim.tabIndex = 0;
}
