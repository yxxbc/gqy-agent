import { closeArtifactResourceMenu, setArtifactWorkspaceOpen } from "./artifacts/model.js";
import { toggleArtifactMaximized } from "./artifacts/workspace.js";
import { consoleIsOpen } from "./console/panel.js";
import { closeModelMenu } from "./model-menu/menu.js";
import { requestNewConversation } from "./session-mode.js";
import { closeSessionMenu } from "./sessions/list.js";
import { closeSettings, settingsIsOpen } from "./settings/panel.js";
import { closeSidebar } from "./sidebar.js";
import { elements } from "../state/elements.js";
import { state } from "../state/store.js";

/// 光标是不是已经在某个能打字的地方。
///
/// `contenteditable` 也算——artifact 的源码视图和将来的富文本都是它,漏判
/// 会让 `/` 快捷键在用户正打字时抢走焦点。
export function typingSomewhere() {
  const node = document.activeElement;
  if (!node) return false;
  if (node.isContentEditable) return true;
  const tag = node.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

export function handleGlobalKeydown(event) {
  // `/` 直接跳到输入框(YouTube 那套)。只聚焦,不把斜杠本身送进去——
  // 快捷键是「跳过去」,不是「替我打一个字」;真要发命令,落到输入框之后
  // 再敲一次 `/` 就行,那一下会正常触发命令菜单。
  if (event.key === "/"
    && !event.ctrlKey && !event.metaKey && !event.altKey
    && !typingSomewhere()
    && !state.blocked
    && !consoleIsOpen()
    && !window.GqyLightbox?.isOpen()
    && !elements.resetDialog.open
    && !elements.composerInput.disabled) {
    event.preventDefault();
    elements.composerInput.focus();
    const at = elements.composerInput.value.length;
    elements.composerInput.setSelectionRange(at, at);
    return;
  }
  if (event.key === "Escape") {
    if (elements.resetDialog.open) return;
    if (!elements.artifactResourceMenu.hidden) {
      event.preventDefault();
      closeArtifactResourceMenu();
      elements.artifactTitleButton.focus();
      return;
    }
    if (state.sessionMenuFor) {
      event.preventDefault();
      closeSessionMenu();
      return;
    }
    if (!elements.modelMenu.hidden) {
      event.preventDefault();
      closeModelMenu({ restoreFocus: true });
      return;
    }
    if (settingsIsOpen()) {
      event.preventDefault();
      closeSettings();
      return;
    }
    if (state.artifactOpen) {
      event.preventDefault();
      if (state.artifactMaximized) {
        toggleArtifactMaximized();
        return;
      }
      setArtifactWorkspaceOpen(false);
      elements.artifactToggleButton.focus();
      return;
    }
    if (elements.sidebar.classList.contains("open")) {
      event.preventDefault();
      closeSidebar();
      state.sidebarOpener?.focus?.();
    }
  }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k" && !event.shiftKey && !event.altKey) {
    event.preventDefault();
    requestNewConversation();
  }
}
