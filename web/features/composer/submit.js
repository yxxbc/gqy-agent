import { ApiError, apiRequest } from "../../core/api.js";
import { MAX_CONTENT_CHARS } from "../../core/constants.js";
import { showToast } from "../../core/toast.js";
import { loadBootstrap } from "../boot.js";
import { committedComposerAttachments } from "./attachments.js";
import { countCharacters, resizeComposer, updateControlState } from "./input.js";
import { activeTurnUpdateTarget, conversationRunning, hasPendingQuestion, updateConversationChrome } from "../conversation/chrome.js";
import { renderConversation } from "../conversation/render.js";
import { scrollToBottom } from "../conversation/scroll.js";
import { commandAnchorTurnId, loadGoal, openPopPicker } from "../goal.js";
import { scheduleViewSync } from "../live/sse.js";
import { cancelLiveRun, ensureLiveUser, renderQueueTray, showTypingIndicator } from "../live/state.js";
import { renderSessionList } from "../sessions/list.js";
import { trackRun, viewSessionEntry } from "../sessions/runs.js";
import { beginRunReplay, createLiveForRun, loadSessionView } from "../sessions/view.js";
import { updateRuntimeUsage } from "../status.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";
import { clearInlineError, showInlineError } from "../../widgets/inline-error.js";

/// 只有本模块用的状态（从 state/store.js 分出来的私有分片）。
const submitState = {
  commandRunning: false
};

export async function submitTurn() {
  if (state.adminBusy || state.submitting || state.blocked) return;
  if (hasPendingQuestion()) return;
  const sessionId = state.viewSessionId;
  const queueing = conversationRunning();
  const updateTarget = queueing ? activeTurnUpdateTarget(sessionId) : null;
  // 只有确定了追加目标才走 /api/queue;否则(在跑但目标不唯一/还没定,常见于
  // 子代理执行中——手机端尤甚)改走 /api/turns,由后端按会话排进当前在跑的轮
  // (09-12 #10:手机端子代理执行时新消息/followup 发不出)。
  const canQueue = queueing && !!updateTarget;
  const content = elements.composerInput.value.trim();
  // 命中命令表就当命令执行，不当消息发。不命中的 `/xxx` 照常发给模型
  // ——与 REPL 同一语义（slash_commands::parse_repl_input）。
  if (window.GqyCommands?.match(content)) {
    window.GqyCommands.hide();
    // 同一条命令不能重入。命令往往要等服务端干完活（/reset 要清库、/compact
    // 要重算上下文），这期间用户看不出回车生效没有，很自然会再敲一次。
    if (submitState.commandRunning) return;
    submitState.commandRunning = true;
    // **先**清输入框，再去跑。原来是跑完才清，命令跑多久输入框就挂着原文
    // 多久——看着就像回车没反应，于是连按几次、连触发几次。
    elements.composerInput.value = "";
    resizeComposer();
    updateControlState();
    let handled = false;
    try {
      handled = await window.GqyCommands.tryRun(content, {
        apiRequest,
        sessionId: state.viewSessionId,
        mode: viewSessionEntry()?.mode === "dev" ? "dev" : "normal",
        redraw: renderConversation,
        // 目标状态行不在对话流里，重绘对话动不到它。
        reloadGoal: () => loadGoal(state.viewSessionId),
        toast: (text) => showToast(text),
        // /stop：停掉当前视图里正在跑的回复；返回空串表示没有在跑的。
        stopRun: async () => {
          const live = [...state.liveRuns.values()].find((entry) => entry && !entry.ended);
          if (!live) return "";
          await cancelLiveRun(live);
          return live.cancellationRequested ? "已请求停止当前回复" : "";
        },
        // 命令改了服务端状态（/reset 清空历史）时用它重拉，光重绘不够。
        reload: async () => {
          if (state.viewSessionId && state.viewSessionId !== state.currentSessionId) {
            await loadSessionView(state.viewSessionId, { quiet: true });
          } else {
            await loadBootstrap();
          }
        },
        // 敲命令那一刻排在最后的回合（含还在流式输出的）。回执插在它之后，
        // 之后来的新回合就不会把回执顶下去。
        anchorTurnId: commandAnchorTurnId(),
        // /pop、/compact 这类要重排上下文的命令不能插在运行中的回合上。
        // 只看当前查看的会话:别的会话在跑不该挡这里的 /reset /compact /pop(09-10 沙盒实测)
        isRunning: () => conversationRunning(),
        // /pop 无参数时的轮次多选器。
        openPopPicker: () => openPopPicker(),
      });
    } finally {
      submitState.commandRunning = false;
      updateControlState();
    }
    if (handled) return;
    // 命令表里有、却没被处理：把原文还给用户，别让它凭空消失。
    elements.composerInput.value = content;
    resizeComposer();
  }
  const readyAttachments = state.composerAttachments.filter((item) => item.status === "ready");
  const attachmentIds = readyAttachments.map((item) => item.id);
  const sentAttachments = readyAttachments.map((item) => ({
    id: item.id,
    url: item.url,
    name: item.name,
    mime: item.mime,
    kind: item.kind,
    size: item.size,
    width: item.width || 0,
    height: item.height || 0
  }));
  const count = countCharacters(content);
  if (!content && !attachmentIds.length) {
    elements.composerState.textContent = "消息不能为空";
    elements.composerState.classList.add("is-error");
    return;
  }
  if (count > MAX_CONTENT_CHARS) {
    elements.composerState.textContent = "消息不能超过 20,000 个字符";
    elements.composerState.classList.add("is-error");
    return;
  }
  state.submitting = true;
  if (!queueing) state.pendingSubmission = { content, attachments: sentAttachments };
  clearInlineError();
  updateControlState();
  try {
    const body = canQueue
      ? { content, run_id: updateTarget.runId, turn_id: updateTarget.turnId, attachment_ids: attachmentIds }
      : { content, attachment_ids: attachmentIds };
    if (sessionId) body.session_id = sessionId;
    const response = await apiRequest(canQueue ? "/api/queue" : "/api/turns", {
      method: "POST",
      body: JSON.stringify(body)
    });
    const payload = await response.json();
    const queuedPrompt = canQueue ? payload : payload?.queued ? payload.prompt : null;
    if (queuedPrompt) {
      if (!state.queuedPrompts.some((prompt) => String(prompt?.id) === String(queuedPrompt?.id))) {
        state.queuedPrompts.push(queuedPrompt);
      }
      state.pendingSubmission = null;
      elements.composerInput.value = "";
      committedComposerAttachments();
      resizeComposer();
      renderQueueTray();
      // 自己发的消息就该看着它:哪怕之前上滚过,也回到底部
      scrollToBottom({ force: true, smooth: true });
      if (!queueing) {
        // 服务端发现该会话已有 turn 在运行并自动转排队：同步该 run 的 live 状态。
        const runningRunId = String(payload?.run_id || "");
        if (runningRunId && sessionId) {
          trackRun(sessionId, runningRunId);
          if (!state.liveRuns.has(runningRunId) && !state.terminalRunIds.has(runningRunId)) {
            createLiveForRun(runningRunId);
            beginRunReplay();
          }
        } else {
          state.viewRunningTurnId = String(payload?.running_turn_id || "") || state.viewRunningTurnId;
          scheduleViewSync();
        }
        renderSessionList();
        updateConversationChrome();
      }
      return;
    }
    const runId = String(payload?.run_id || "");
    if (!runId) throw new ApiError("服务未返回运行标识", response.status);
    if (state.terminalRunIds.has(runId)) {
      if (sessionId) await loadSessionView(sessionId, { quiet: true });
      else await loadBootstrap();
    } else {
      if (sessionId) trackRun(sessionId, runId);
      const live = createLiveForRun(runId, content);
      live.userText = content;
      live.userAttachments = sentAttachments;
      ensureLiveUser(live, content);
      showTypingIndicator(live);
      elements.composerInput.value = "";
      committedComposerAttachments();
      resizeComposer();
      // 自己发的消息就该看着它:哪怕之前上滚过,也回到底部
      scrollToBottom({ force: true, smooth: true });
      updateRuntimeUsage();
      updateConversationChrome();
      renderSessionList();
    }
  } catch (error) {
    if (!queueing) state.pendingSubmission = null;
    // 409 = 后端认为这个会话已经在跑，而前端以为没有。原文案（「正在同步」
    // ＋「请重新发送」）把机器的调度问题说成用户该重来一遍，而且说了两遍。
    // 现在只留一条，说清楚发生了什么。
    // 排队请求 409 = 盯着的那条轮已经跑完/被顶替,会话此刻空闲。别再弹
    // 「再发一次」让用户重来——直接改走 /api/turns 起一条新轮,消息不丢
    // (/api/turns 会自动排队或新建,09-12 用户报「排队消息却提示要等」)。
    if (canQueue && error.status === 409) {
      try {
        const body = { content, attachment_ids: attachmentIds };
        if (sessionId) body.session_id = sessionId;
        const retry = await apiRequest("/api/turns", { method: "POST", body: JSON.stringify(body) });
        const payload = await retry.json();
        const qp = payload?.queued ? payload.prompt : null;
        if (qp && !state.queuedPrompts.some((p) => String(p?.id) === String(qp?.id))) {
          state.queuedPrompts.push(qp);
        }
        elements.composerInput.value = "";
        committedComposerAttachments();
        resizeComposer();
        renderQueueTray();
        if (sessionId) await loadSessionView(sessionId, { quiet: true });
        else await loadBootstrap();
        return;
      } catch (retryError) {
        showToast(retryError.message || "发送失败", "error");
      }
    } else if (error.status === 409) {
      showToast("这条没发出去：会话刚开始新的一轮，再发一次", "error");
    } else {
      showInlineError(error.message);
      showToast(error.message, "error");
    }
    if (error.status === 409) {
      if (sessionId) await loadSessionView(sessionId, { quiet: true });
      else await loadBootstrap();
    }
  } finally {
    state.submitting = false;
    updateControlState();
  }
}
