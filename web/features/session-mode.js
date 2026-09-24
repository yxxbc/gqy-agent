import { apiRequest } from "../core/api.js";
import { makeIconSlot } from "../core/icons.js";
import { showToast } from "../core/toast.js";
import { loadBootstrap } from "./boot.js";
import { focusComposerIfDesktop, updateControlState } from "./composer/input.js";
import { conversationRunning } from "./conversation/chrome.js";
import { findSession, multiSessionEnabled } from "./sessions/runs.js";
import { createSession } from "./sessions/view.js";
import { closeSidebar } from "./sidebar.js";
import { elements } from "../state/elements.js";
import { state } from "../state/store.js";
import { showInlineError } from "../widgets/inline-error.js";

export function hasHistory() {
  for (const live of state.liveRuns.values()) {
    if (live.userRendered) return true;
  }
  return state.turns.length > 0 || Boolean(elements.timeline.querySelector(".user-message"));
}

export function openResetDialog() {
  if (typeof elements.resetDialog.showModal === "function") elements.resetDialog.showModal();
  else elements.resetDialog.setAttribute("open", "");
  window.requestAnimationFrame(() => elements.resetCancelButton.focus());
}

export function openModeChooser() {
  if (state.modeChooserOpen) return;
  state.modeChooserOpen = true;
  updateControlState();
  const overlay = document.createElement("div");
  overlay.className = "mode-chooser-overlay";
  overlay.id = "modeChooserOverlay";
  const panel = document.createElement("div");
  panel.className = "mode-chooser";
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-label", "选择新会话模式");
  const title = document.createElement("strong");
  title.textContent = "新会话";
  const hint = document.createElement("small");
  hint.textContent = "选择模式后开始对话；会话模式创建后不可更改";
  panel.append(title, hint);
  const options = [
    { id: "normal", label: "普通模式", icon: "message-circle", desc: "人格、记忆、全部工具" },
    { id: "dev", label: "开发模式", icon: "code", desc: "极简提示词与编码工具，记忆独立" }
  ];
  for (const option of options) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "mode-chooser-option";
    button.dataset.mode = option.id;
    button.appendChild(makeIconSlot(option.icon));
    const copy = document.createElement("span");
    copy.className = "mode-chooser-copy";
    const label = document.createElement("strong");
    label.textContent = option.label;
    const desc = document.createElement("small");
    desc.textContent = option.desc;
    copy.append(label, desc);
    button.appendChild(copy);
    button.addEventListener("click", () => {
      closeModeChooser();
      closeSidebar();
      createSession(option.id);
    });
    panel.appendChild(button);
  }
  overlay.addEventListener("click", (event) => {
    if (event.target === overlay) closeModeChooser();
  });
  overlay.appendChild(panel);
  document.body.appendChild(overlay);
  const onKey = (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      closeModeChooser();
    }
  };
  state.modeChooserKeyHandler = onKey;
  document.addEventListener("keydown", onKey, true);
  window.requestAnimationFrame(() => panel.querySelector("button")?.focus());
}

export function closeModeChooser() {
  if (!state.modeChooserOpen) return;
  state.modeChooserOpen = false;
  if (state.modeChooserKeyHandler) {
    document.removeEventListener("keydown", state.modeChooserKeyHandler, true);
    state.modeChooserKeyHandler = null;
  }
  document.getElementById("modeChooserOverlay")?.remove();
  updateControlState();
}

export function activeSessionMode() {
  const session = findSession(state.viewSessionId);
  return session?.mode === "dev" ? "dev" : "normal";
}

export function requestNewConversation() {
  if (multiSessionEnabled()) {
    openModeChooser();
    return;
  }
  closeSidebar();
  if (!hasHistory()) {
    focusComposerIfDesktop();
    return;
  }
  if (conversationRunning() || state.adminBusy || state.submitting) return;
  openResetDialog();
}

export function requestClearConversation() {
  if (conversationRunning() || state.adminBusy || state.submitting) return;
  if (!hasHistory()) {
    showToast("当前会话没有可清除的记录");
    return;
  }
  openResetDialog();
}

export async function resetConversation() {
  if (conversationRunning() || state.adminBusy || state.submitting) return;
  state.adminBusy = true;
  elements.resetConfirmButton.disabled = true;
  elements.resetCancelButton.disabled = true;
  elements.resetConfirmButton.textContent = "正在清除";
  updateControlState();
  try {
    if (!state.viewSessionId) throw new Error("无法确定要清除的会话");
    await apiRequest("/api/conversation/reset", {
      method: "POST",
      body: JSON.stringify({ session_id: state.viewSessionId })
    });
    if (elements.resetDialog.open) elements.resetDialog.close("confirmed");
    await loadBootstrap();
    focusComposerIfDesktop();
  } catch (error) {
    showInlineError(error.message);
    showToast(error.message, "error");
    if (error.status === 409) await loadBootstrap();
  } finally {
    state.adminBusy = false;
    elements.resetConfirmButton.disabled = false;
    elements.resetCancelButton.disabled = false;
    elements.resetConfirmButton.textContent = "清空记录";
    updateControlState();
  }
}
