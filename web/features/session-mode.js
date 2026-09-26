import { apiRequest } from "../core/api.js";
import { showToast } from "../core/toast.js";
import { loadBootstrap } from "./boot.js";
import { focusComposerIfDesktop, updateControlState } from "./composer/input.js";
import { conversationRunning } from "./conversation/chrome.js";
import { multiSessionEnabled } from "./sessions/runs.js";
import { enterDraftView } from "./sessions/view.js";
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

export function activeSessionMode() {
  return state.sessionMode === "dev" ? "dev" : "normal";
}

/// 新对话直接落在侧栏开关选中的模式里，先是草稿页，发第一条消息才建会话。
export function requestNewConversation() {
  if (multiSessionEnabled()) {
    closeSidebar();
    if (state.draftMode !== activeSessionMode()) enterDraftView(activeSessionMode());
    focusComposerIfDesktop();
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
