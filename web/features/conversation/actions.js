import { ApiError, apiRequest } from "../../core/api.js";
import { MAX_CONTENT_CHARS } from "../../core/constants.js";
import { makeIconSlot } from "../../core/icons.js";
import { showToast } from "../../core/toast.js";
import { countCharacters, updateControlState } from "../composer/input.js";
import { conversationRunning, hasPendingQuestion, updateConversationChrome } from "./chrome.js";
import { cancelLiveRun } from "../live/state.js";
import { renderSessionList } from "../sessions/list.js";
import { trackRun } from "../sessions/runs.js";
import { createLiveForRun, loadSessionView } from "../sessions/view.js";
import { state } from "../../state/store.js";

/// 只有本模块用的状态（从 state/store.js 分出来的私有分片）。
const actionsState = {
  revisionSubmitting: false,
  revisionEditor: null
};

export async function copyText(text) {
  const value = String(text || "");
  if (!value) return false;
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value);
      showToast("已复制");
      return true;
    }
  } catch (_) {
    // Use the selection fallback below.
  }
  const textarea = document.createElement("textarea");
  textarea.value = value;
  textarea.setAttribute("readonly", "");
  textarea.style.position = "fixed";
  textarea.style.left = "-9999px";
  textarea.style.top = "0";
  document.body.appendChild(textarea);
  textarea.select();
  textarea.setSelectionRange(0, textarea.value.length);
  let copied = false;
  try {
    copied = document.execCommand("copy");
  } catch (_) {
    copied = false;
  }
  textarea.remove();
  showToast(copied ? "已复制" : "复制失败", copied ? "info" : "error");
  return copied;
}

export function makeCopyButton(textProvider, label = "复制") {
  const button = document.createElement("button");
  button.type = "button";
  button.title = label;
  button.setAttribute("aria-label", label);
  button.appendChild(makeIconSlot("copy"));
  button.addEventListener("click", () => copyText(typeof textProvider === "function" ? textProvider() : textProvider));
  return button;
}

export function makeMessageAction(icon, label, handler) {
  const button = document.createElement("button");
  button.type = "button";
  button.title = label;
  button.setAttribute("aria-label", label);
  button.appendChild(makeIconSlot(icon));
  button.addEventListener("click", handler);
  return button;
}

export function revisionEligible(candidate = state.redoCandidate) {
  if (!candidate || !state.capabilities?.redo) return false;
  // AI 输出中也允许改上一条 prompt(09-12 用户报):submitRedo 会先掐掉正在跑
  // 的那轮再重发,所以这里不再拿 conversationRunning() 挡着。
  return !state.blocked && !state.viewLoading && !state.resyncing
    && !state.submitting && !actionsState.revisionSubmitting
    && !state.adminBusy && !state.sessionBusy && !hasPendingQuestion()
    && state.queuedPrompts.length === 0;
}

export function closeRevisionEditor({ restoreFocus = false } = {}) {
  const editor = actionsState.revisionEditor;
  if (!editor) return;
  editor.form.remove();
  editor.bubble.hidden = editor.wasHidden;
  actionsState.revisionEditor = null;
  if (restoreFocus) editor.opener?.focus();
}

export function openRevisionEditor(article, bubble, content, candidate, opener) {
  if (!revisionEligible(candidate)) return;
  closeRevisionEditor();
  const form = document.createElement("form");
  form.className = "revision-editor";
  form.setAttribute("aria-label", "编辑最后一条消息");
  const textarea = document.createElement("textarea");
  textarea.value = String(content || "");
  textarea.maxLength = MAX_CONTENT_CHARS;
  textarea.setAttribute("aria-label", "消息内容");
  const error = document.createElement("div");
  error.className = "revision-editor-error";
  error.setAttribute("role", "alert");
  error.hidden = true;
  const footer = document.createElement("div");
  footer.className = "revision-editor-footer";
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.textContent = "取消";
  const submit = document.createElement("button");
  submit.type = "submit";
  submit.textContent = "发送";
  footer.append(cancel, submit);
  form.append(textarea, error, footer);
  const wasHidden = bubble.hidden;
  bubble.hidden = true;
  article.insertBefore(form, article.querySelector(".message-actions"));
  actionsState.revisionEditor = { form, textarea, error, submit, bubble, wasHidden, opener, candidate };
  cancel.addEventListener("click", () => closeRevisionEditor({ restoreFocus: true }));
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const draft = textarea.value.trim();
    if (!draft && !article.querySelector(".user-attachments")) {
      error.textContent = "消息不能为空";
      error.hidden = false;
      return;
    }
    if (countCharacters(draft) > MAX_CONTENT_CHARS) {
      error.textContent = "消息不能超过 20,000 个字符";
      error.hidden = false;
      return;
    }
    await submitRedo(candidate, draft);
  });
  textarea.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      closeRevisionEditor({ restoreFocus: true });
    } else if ((event.ctrlKey || event.metaKey) && event.key === "Enter" && !event.isComposing) {
      event.preventDefault();
      form.requestSubmit();
    }
  });
  window.requestAnimationFrame(() => {
    textarea.focus();
    textarea.setSelectionRange(textarea.value.length, textarea.value.length);
    form.scrollIntoView({ block: "nearest" });
  });
}

export async function submitRedo(candidate, editedContent = null) {
  if (!revisionEligible(candidate)) return;
  const sessionId = state.viewSessionId;
  if (!sessionId) return;
  actionsState.revisionSubmitting = true;
  const editor = actionsState.revisionEditor;
  if (editor) {
    editor.form.setAttribute("aria-busy", "true");
    editor.textarea.disabled = true;
    editor.submit.disabled = true;
    editor.error.hidden = true;
  }
  updateControlState();
  // AI 还在输出时改 prompt:先掐掉正在跑的那轮(redo 后端遇到 session_has_runs
  // 会 409),等它收尾再重发。最多等 ~4s,到点就交给下面的 409 重试兜底。
  if (conversationRunning()) {
    for (const live of [...state.liveRuns.values()]) {
      if (live && !live.ended) {
        try { await cancelLiveRun(live); } catch { /* 尽力而为 */ }
      }
    }
    for (let i = 0; i < 40 && conversationRunning(); i += 1) {
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
  try {
    const body = {
      expected_revision: candidate.revision,
      input_id: candidate.input_id
    };
    if (editedContent != null) body.content = editedContent;
    const response = await apiRequest(
      `/api/sessions/${encodeURIComponent(sessionId)}/turns/${encodeURIComponent(candidate.turn_id)}/redo`,
      { method: "POST", body: JSON.stringify(body) }
    );
    const payload = await response.json();
    const runId = String(payload?.run_id || "");
    if (!runId) throw new ApiError("服务未返回运行标识", response.status);
    trackRun(sessionId, runId);
    createLiveForRun(runId, "", {
      claimTurn: false,
      operation: "redo",
      turnId: candidate.turn_id,
      inputId: candidate.input_id,
      editedContent
    });
    state.redoCandidate = null;
    renderSessionList();
    updateConversationChrome();
  } catch (error) {
    if (editor && actionsState.revisionEditor === editor) {
      editor.error.textContent = error.status === 409 ? "会话已变化，请重新操作" : error.message;
      editor.error.hidden = false;
    }
    showToast(error.status === 409 ? "会话状态已更新" : error.message, "error");
    if (error.status === 409) await loadSessionView(sessionId, { quiet: true });
  } finally {
    actionsState.revisionSubmitting = false;
    if (editor && actionsState.revisionEditor === editor) {
      editor.form.removeAttribute("aria-busy");
      editor.textarea.disabled = false;
      editor.submit.disabled = false;
    }
    updateControlState();
  }
}
