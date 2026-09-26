import { apiRequest } from "../core/api.js";
import { showToast } from "../core/toast.js";
import { loadBootstrap } from "./boot.js";
import { focusComposerIfDesktop, updateControlState } from "./composer/input.js";
import { conversationRunning } from "./conversation/chrome.js";
import { findSession, multiSessionEnabled, sessionDisplayName, sessionHasRuns } from "./sessions/runs.js";
import { enterDraftView, refreshSessions } from "./sessions/view.js";
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

/// 「清空对话」要清的会话,null 表示当前正看着的会话。会话菜单是按卡片弹的,
/// 在别的卡片上点「清空对话」,清的得是那张卡片,不是当前会话。
let resetTargetId = null;

export function openResetDialog(sessionId = null) {
  resetTargetId = sessionId && sessionId !== state.viewSessionId ? sessionId : null;
  const target = resetTargetId ? findSession(resetTargetId) : null;
  elements.resetDialogTitle.textContent = target ? `清空「${sessionDisplayName(target)}」？` : "清空当前会话？";
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

export function requestClearConversation(sessionId = null) {
  if (sessionId && sessionId !== state.viewSessionId) {
    if (state.adminBusy) return;
    if (sessionHasRuns(sessionId)) {
      showToast("这个会话还有回复在运行");
      return;
    }
    if (!(Number(findSession(sessionId)?.turn_count) > 0)) {
      showToast("这个会话没有可清除的记录");
      return;
    }
    openResetDialog(sessionId);
    return;
  }
  if (conversationRunning() || state.adminBusy || state.submitting) return;
  if (!hasHistory()) {
    showToast("当前会话没有可清除的记录");
    return;
  }
  openResetDialog();
}

export async function resetConversation() {
  const otherSession = resetTargetId;
  if (state.adminBusy || (!otherSession && (conversationRunning() || state.submitting))) return;
  state.adminBusy = true;
  elements.resetConfirmButton.disabled = true;
  elements.resetCancelButton.disabled = true;
  elements.resetConfirmButton.textContent = "正在清除";
  updateControlState();
  try {
    const sessionId = otherSession || state.viewSessionId;
    if (!sessionId) throw new Error("无法确定要清除的会话");
    await apiRequest("/api/conversation/reset", {
      method: "POST",
      body: JSON.stringify({ session_id: sessionId })
    });
    if (elements.resetDialog.open) elements.resetDialog.close("confirmed");
    if (otherSession) {
      // 清的是别的会话:当前视图不动,只刷新列表上的首句与轮数。
      showToast("会话已清空");
      await refreshSessions();
    } else {
      await loadBootstrap();
      focusComposerIfDesktop();
    }
  } catch (error) {
    showInlineError(error.message);
    showToast(error.message, "error");
    if (error.status === 409 && !otherSession) await loadBootstrap();
  } finally {
    state.adminBusy = false;
    elements.resetConfirmButton.disabled = false;
    elements.resetCancelButton.disabled = false;
    elements.resetConfirmButton.textContent = "清空记录";
    updateControlState();
  }
}
