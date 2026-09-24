import { apiRequest } from "../../core/api.js";
import { EVENT_NAMES, RUN_EVENTS } from "../../core/constants.js";
import { asFiniteNumber } from "../../core/format.js";
import { showToast } from "../../core/toast.js";
import { showBlockedState } from "../auth.js";
import { loadBootstrap } from "../boot.js";
import { updateControlState } from "../composer/input.js";
import { liveViewed, updateConversationChrome } from "../conversation/chrome.js";
import { jobStreamSink, renderConversation } from "../conversation/render.js";
import { renderSubagentProgress } from "../conversation/subagent.js";
import { loadGoal } from "../goal.js";
import { renderJobsStrip } from "../jobs.js";
import { handleContextEvent } from "./context.js";
import { consumeLiveQueue, finishLiveRun, handleRoundUsage, refreshComposerCumulative } from "./run.js";
import { commitRedoLive, ensureLiveUser, removeRunningStatus, renderQueueTray, showTypingIndicator } from "./state.js";
import { appendAssistantDelta, handleReasoningEvent, resetSupersededGeneration } from "./stream.js";
import { createQuestion, markQuestionAnswered, markQuestionClosed } from "../questions.js";
import { renderSessionList } from "../sessions/list.js";
import { runSessionId, trackRun, untrackRun } from "../sessions/runs.js";
import { createLiveForRun, handleSessionEvent, loadSessionView, refreshSessions, restoreLiveRuns } from "../sessions/view.js";
import { setConnectionStatus, updateContext } from "../status.js";
import { clearPreparingTool, handleToolEvent } from "../tools/events.js";
import { state } from "../../state/store.js";

export function clearViewSyncTimer() {
  if (!state.viewSyncTimer) return;
  window.clearTimeout(state.viewSyncTimer);
  state.viewSyncTimer = null;
}

export function scheduleViewSync() {
  clearViewSyncTimer();
  if (!state.viewRunningTurnId || state.blocked) return;
  state.viewSyncTimer = window.setTimeout(() => {
    state.viewSyncTimer = null;
    refreshViewSnapshot();
  }, 1_000);
}

export async function refreshViewSnapshot() {
  const sessionId = state.viewSessionId;
  if (!sessionId || state.blocked || state.viewLoading || state.resyncing) {
    scheduleViewSync();
    return;
  }
  try {
    const response = await apiRequest(`/api/sessions/${encodeURIComponent(sessionId)}/turns`);
    const payload = await response.json();
    if (state.viewSessionId !== sessionId || state.viewLoading) return;
    const runs = (Array.isArray(payload?.runs) ? payload.runs : []).filter((run) => run?.run_id);
    if (runs.length) state.runsBySession.set(sessionId, new Set(runs.map((run) => String(run.run_id))));
    else if (state.liveRuns.size === 0) state.runsBySession.delete(sessionId);
    state.viewRunningTurnId = !runs.length && typeof payload?.running_turn_id === "string" && payload.running_turn_id
      ? payload.running_turn_id
      : null;
    if (state.liveRuns.size === 0) {
      const nextTurns = Array.isArray(payload?.turns)
        ? payload.turns.sort((a, b) => asFiniteNumber(a?.seq) - asFiniteNumber(b?.seq))
        : state.turns;
      const turnsChanged = JSON.stringify(nextTurns) !== JSON.stringify(state.turns);
      const nextCandidate = payload?.redo_candidate && typeof payload.redo_candidate === "object"
        ? payload.redo_candidate
        : null;
      const candidateChanged = JSON.stringify(nextCandidate) !== JSON.stringify(state.redoCandidate);
      state.turns = nextTurns;
      state.queuedPrompts = Array.isArray(payload?.queued_prompts) ? payload.queued_prompts : state.queuedPrompts;
      state.redoCandidate = nextCandidate;
      // 刚中断那次不必整会话重渲染(#1):原位补的「本轮已中断」+ 留在原地的直播
      // 气泡已经把最终态画对了,重渲染只是把同样的东西再拼一遍——长对话里这一下
      // 就是中断残留的卡顿。只吞这一次纯 turns 变更;redo 候选变了照常渲染。
      const suppress = state.suppressPostCancelRender && !candidateChanged;
      state.suppressPostCancelRender = false;
      if ((turnsChanged || candidateChanged) && !suppress) renderConversation();
      renderQueueTray();
      restoreLiveRuns(runs);
    }
    renderSessionList();
    updateConversationChrome();
    updateControlState();
  } catch (error) {
    if (error.status === 401) {
      showBlockedState(true);
      return;
    }
    if (error.status === 404) {
      state.viewRunningTurnId = null;
      refreshSessions();
      return;
    }
  } finally {
    scheduleViewSync();
  }
}

export async function ensureActiveTurnUser(live, turnId) {
  if (!live || live.userRendered || !turnId) return;
  // 离屏 live 不补渲用户消息(下面拉的是当前视图的 turns,张冠李戴)。
  if (!liveViewed(live)) return;
  const existing = state.turns.find((turn) => String(turn?.id) === String(turnId));
  if (existing) {
    live.userText = String(existing.user_content || "");
    live.userRendered = true;
    updateConversationChrome();
    return;
  }
  const sessionId = state.viewSessionId;
  try {
    const response = await apiRequest(`/api/sessions/${encodeURIComponent(sessionId)}/turns`);
    const payload = await response.json();
    if (state.viewSessionId !== sessionId || state.liveRuns.get(live.runId) !== live || live.userRendered) return;
    const turn = Array.isArray(payload?.turns) ? payload.turns.find((item) => String(item?.id) === String(turnId)) : null;
    if (!turn) return;
    live.userText = String(turn.user_content || "");
    live.userAttachments = Array.isArray(turn.attachments) ? turn.attachments : [];
    ensureLiveUser(live, live.userText);
  } catch (_) {
    // The stream can continue; a later view refresh will recover the user turn.
  }
}

export function handleRunEvent(name, data) {
  const runId = String(data?.run_id || "");
  if (!runId) return;
  const sessionId = typeof data?.session_id === "string" && data.session_id ? data.session_id : runSessionId(runId);
  const terminal = name === "run.completed" || name === "run.cancelled" || name === "run.failed";
  if (name === "run.started" && sessionId) trackRun(sessionId, runId);
  // 正在看的会话不算未读——用户就在现场看着它跑完。
  if (terminal && sessionId && sessionId !== state.viewSessionId) {
    if (!state.unreadSessions.has(sessionId)) {
      state.unreadSessions.add(sessionId);
      renderSessionList();
    }
  }

  let live = state.liveRuns.get(runId);
  if (!live && !terminal && !state.terminalRunIds.has(runId) && sessionId && sessionId === state.viewSessionId) {
    // 视图会话里出现的新 run（本端发起、他端发起或重放）都会挂上 live 块。
    // run.started 意味着全新的 turn，不去认领时间线里已有的 running turn。
    live = createLiveForRun(runId, "", {
      sessionId,
      claimTurn: name !== "run.started",
      operation: String(data?.operation || "create"),
      turnId: String(data?.turn_id || "") || null,
      inputId: String(data?.input_id || "") || null
    });
    if (live.turnId && state.viewRunningTurnId === String(live.turnId)) state.viewRunningTurnId = null;
  }

  if (name === "run.started") {
    if (live) {
      live.operation = String(data?.operation || live.operation || "create");
      live.turnId = String(data?.turn_id || live.turnId || "") || null;
      live.inputId = String(data?.input_id || live.inputId || "") || null;
    }
    if (live && !live.ended && live.operation !== "redo") showTypingIndicator(live);
    renderSessionList();
    updateConversationChrome();
    updateControlState();
    return;
  }
  if (terminal) {
    // 一轮跑完，目标的轮次/阶段可能都变了（模型自己报了完成或受阻）。
    if (sessionId && sessionId === state.viewSessionId) loadGoal(sessionId);
    untrackRun(runId);
    if (live) {
      finishLiveRun(name.slice("run.".length), data, live);
    } else {
      state.terminalRunIds.add(runId);
      if (state.terminalRunIds.size > 30) state.terminalRunIds.delete(state.terminalRunIds.values().next().value);
      if (name === "run.completed" && data?.session_id && String(data.session_id) === String(state.viewSessionId || state.currentSessionId || "")) {
        if (data?.context_tokens != null) state.context.tokens = Math.max(0, asFiniteNumber(data.context_tokens));
        state.context.window = data?.context_window == null ? state.context.window : Math.max(0, asFiniteNumber(data.context_window));
        updateContext();
      }
      renderSessionList();
    }
    return;
  }
  if (!live) return;

  if (name === "turn.started") {
    live.turnId = String(data?.turn_id || "");
    if (live.article) live.article.dataset.turnId = live.turnId;
    if (String(data?.operation || "") === "redo") {
      live.operation = "redo";
      live.inputId = String(data?.input_id || live.inputId || "") || null;
      if (typeof data?.display_content === "string") live.editedContent = data.display_content;
    }
    if (state.viewRunningTurnId === live.turnId) state.viewRunningTurnId = null;
    removeRunningStatus(live.turnId);
    if (live.operation === "redo") commitRedoLive(live);
    else ensureActiveTurnUser(live, live.turnId);
  } else if (name === "assistant.delta") appendAssistantDelta(live, data?.delta);
  else if (name === "chat.round_usage") handleRoundUsage(live, data);
  else if (name === "generation.superseded") resetSupersededGeneration(live);
  else if (name.startsWith("reasoning.")) handleReasoningEvent(name, live, data);
  else if (name === "queue.consumed") consumeLiveQueue(live, data);
  else if (name.startsWith("tool.")) handleToolEvent(name, live, data);
  else if (name === "question.requested") {
    clearPreparingTool(live);
    createQuestion(live, data);
  }
  else if (name === "question.answered") {
    const question = live.questions.get(String(data?.question_id || ""));
    if (question) markQuestionAnswered(question, data?.answers);
  } else if (name === "question.closed") {
    const question = live.questions.get(String(data?.question_id || ""));
    if (question) markQuestionClosed(question);
  } else if (name.startsWith("context.")) handleContextEvent(name, live, data);
}

export function eventShouldBeHandled(name, data, eventId) {
  if (name === "resync_required") {
    if (eventId > 0) state.lastEventId = eventId;
    return true;
  }
  if (eventId > 0 && eventId <= state.lastEventId) return false;
  if (eventId > 0) state.lastEventId = eventId;
  if (state.replayRunIds && eventId > 0 && eventId <= state.replayCutoff) {
    // 重放窗口内只重建正在恢复的 run，其余事件已经反映在快照里。
    if (!RUN_EVENTS.has(name)) return false;
    return state.replayRunIds.has(String(data?.run_id || ""));
  }
  if (state.replayRunIds && eventId > state.replayCutoff) state.replayRunIds = null;
  return true;
}

export function handleSseEvent(name, event) {
  // 登录态没了(blocked)时,一律不再处理 SSE 事件:否则一边弹登录界面、一边还
  // 触发 loadBootstrap/「正在重新同步」的会话重载提示,两个提示重复(09-12 #18)。
  // showBlockedState 已经关了 SSE,这里挡住任何残留在途的事件。
  if (state.blocked) return;
  let data;
  try {
    data = event.data ? JSON.parse(event.data) : {};
  } catch (_) {
    showToast("收到无法解析的事件，正在重新同步", "error");
    loadBootstrap();
    return;
  }
  const eventId = Math.max(0, asFiniteNumber(event.lastEventId));
  if (!eventShouldBeHandled(name, data, eventId)) return;
  if (name === "resync_required") {
    if (state.replayRunIds) {
      state.replayResyncCount += 1;
      state.replayResyncAt = Date.now();
    } else {
      state.replayResyncCount = 0;
    }
    if (!state.resyncing) {
      state.resyncing = true;
      loadBootstrap().finally(() => {
        state.resyncing = false;
      });
    }
    return;
  }
  if (name.startsWith("session.")) {
    handleSessionEvent(name, data);
    return;
  }
  if (name === "queue.added") {
    const prompt = data?.prompt;
    if (queueEventTargetsView(data) && prompt && !state.queuedPrompts.some((item) => String(item?.id) === String(prompt?.id))) {
      state.queuedPrompts.push(prompt);
      renderQueueTray();
    }
    return;
  }
  if (name === "job.started") {
    const job = data?.job;
    if (job?.job_id) {
      state.backgroundJobs.set(String(job.job_id), { ...job, receivedAt: Date.now() });
      renderJobsStrip();
    }
    return;
  }
  if (name === "job.progress") {
    const jobId = String(data?.job_id || "");
    const message = String(data?.message || "");
    if (jobId && message) {
      // 后台子代理的实时进度:喂给该 job 的子过程流(与前台子代理工具行同款
      // 解析后渲进该 job 的子过程时间线(展开时可见,持久累积)。
      renderSubagentProgress(jobStreamSink(jobId), message);
    }
    return;
  }
  if (name === "job.finished") {
    const jobId = String(data?.job_id || "");
    // 后台子代理跑完:它的实时估算先「冻住」(别立刻抽走,否则基线还没算进它之前
    // 累计会掉一下),等下个主回合权威基线接管时再删(#131,与前台同款)。
    const entry = state.liveSubagentTokens.get("job:" + jobId);
    if (entry) { entry.done = true; entry.baseAtDone = asFiniteNumber(state.cumulativeBase?.total); refreshComposerCumulative(); }
    state.expandedJobs.delete(jobId);
    state.jobStreamSinks.delete(jobId);
    if (state.backgroundJobs.delete(jobId)) renderJobsStrip();
    return;
  }
  if (name === "job.acknowledged") {
    const jobId = String(data?.job_id || "");
    state.expandedJobs.delete(jobId);
    state.jobStreamSinks.delete(jobId);
    if (state.backgroundJobs.delete(jobId)) renderJobsStrip();
    return;
  }
  if (name === "queue.removed") {
    if (queueEventTargetsView(data)) {
      state.queuedPrompts = state.queuedPrompts.filter((prompt) => String(prompt?.id) !== String(data?.prompt_id));
      renderQueueTray();
    }
    return;
  }
  if (name === "conversation.reset" || name === "conversation.pop" || name === "conversation.compacted") {
    const sessionId = typeof data?.session_id === "string" ? data.session_id : "";
    // 清空/压缩/pop 会重排或清零会话累计;把「不下调」用的基线与子代理估算一并清了,
    // 让它按重载后的权威值重新起算(#131:否则 max 会把清零前的旧高值锁住)。
    if (!sessionId || sessionId === state.viewSessionId) {
      state.cumulativeBase = null;
      state.liveSubagentTokens.clear();
    }
    if (sessionId && sessionId !== state.viewSessionId) {
      refreshSessions();
    } else if (!state.viewSessionId || state.viewSessionId === state.currentSessionId) {
      loadBootstrap();
    } else {
      loadSessionView(state.viewSessionId, { quiet: true });
      refreshSessions();
    }
    return;
  }
  handleRunEvent(name, data);
}

export function queueEventTargetsView(data) {
  const explicit = typeof data?.session_id === "string" && data.session_id ? data.session_id : "";
  if (explicit) return explicit === state.viewSessionId;
  const runId = String(data?.run_id || "");
  if (runId) {
    if (state.liveRuns.has(runId)) return true;
    const sessionId = runSessionId(runId);
    if (sessionId) return sessionId === state.viewSessionId;
  }
  const turnId = String(data?.turn_id || "");
  if (turnId) {
    if (state.viewRunningTurnId && turnId === state.viewRunningTurnId) return true;
    for (const live of state.liveRuns.values()) {
      if (String(live.turnId || "") === turnId) return true;
    }
    return state.turns.some((turn) => String(turn?.id) === turnId && turn?.status === "running");
  }
  return false;
}

export function closeEventSource() {
  if (state.eventSource) {
    state.eventSource.close();
    state.eventSource = null;
  }
  if (state.healthTimer) {
    window.clearTimeout(state.healthTimer);
    state.healthTimer = null;
  }
}

export async function refineConnectionHealth(source) {
  if (state.eventSource !== source || source.readyState === EventSource.OPEN) return;
  try {
    const response = await fetch("/api/health", { cache: "no-store", credentials: "same-origin" });
    if (!response.ok) throw new Error("health check failed");
    if (state.eventSource === source && source.readyState !== EventSource.OPEN) setConnectionStatus("connecting");
  } catch (_) {
    if (state.eventSource === source && source.readyState !== EventSource.OPEN) setConnectionStatus("offline");
  }
}

export function connectEventSource(after) {
  closeEventSource();
  if (state.blocked) return;
  const source = new EventSource(`/api/events?after=${encodeURIComponent(Math.max(0, asFiniteNumber(after)))}`);
  state.eventSource = source;
  source.onopen = () => {
    if (state.eventSource !== source) return;
    setConnectionStatus("online");
    if (state.healthTimer) window.clearTimeout(state.healthTimer);
    state.healthTimer = null;
  };
  source.onerror = () => {
    if (state.eventSource !== source) return;
    setConnectionStatus("connecting");
    if (state.healthTimer) window.clearTimeout(state.healthTimer);
    state.healthTimer = window.setTimeout(() => refineConnectionHealth(source), 1200);
  };
  for (const name of EVENT_NAMES) source.addEventListener(name, (event) => handleSseEvent(name, event));
}
