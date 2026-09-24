import { apiRequest } from "../../core/api.js";
import { asFiniteNumber } from "../../core/format.js";
import { safeStorageSet } from "../../core/storage.js";
import { showToast } from "../../core/toast.js";
import { showBlockedState } from "../auth.js";
import { VIEW_SESSION_KEY, loadBootstrap } from "../boot.js";
import { clearComposerAttachments } from "../composer/attachments.js";
import { focusComposerIfDesktop, updateControlState } from "../composer/input.js";
import { closeRevisionEditor } from "../conversation/actions.js";
import { updateConversationChrome } from "../conversation/chrome.js";
import { reasoningHidden } from "../conversation/reasoning.js";
import { renderConversation } from "../conversation/render.js";
import { scrollToBottom } from "../conversation/scroll.js";
import { subScrollContainer } from "../conversation/subagent.js";
import { refreshSessionContext } from "../goal.js";
import { renderJobsStrip } from "../jobs.js";
import { clearViewSyncTimer, connectEventSource, scheduleViewSync } from "../live/sse.js";
import { createLiveState, disposeLiveState, ensureLiveArticle, renderQueueTray, showTypingIndicator } from "../live/state.js";
import { appendAssistantDelta, handleReasoningEvent } from "../live/stream.js";
import { refreshSessionModelOverride, setSessionModelOverride, updateCurrentModelDisplay } from "../model-menu/menu.js";
import { isTerminalSession, renderSessionList } from "./list.js";
import { findSession, sessionDisplayName } from "./runs.js";
import { closeSidebar } from "../sidebar.js";
import { handleToolEvent } from "../tools/events.js";
import { isSubagentTool } from "../tools/format.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

/// 只有本模块用的状态（从 state/store.js 分出来的私有分片）。
const viewState = {
  viewLoadGeneration: 0
};

export async function refreshSessions() {
  try {
    const response = await apiRequest("/api/sessions");
    const payload = await response.json();
    state.sessions = Array.isArray(payload?.sessions) ? payload.sessions : [];
    renderSessionList();
    updateConversationChrome();
  } catch (_) {
    // 后续 SSE 或 bootstrap 会补齐会话列表。
  }
}

export function setSessionBusy(value) {
  state.sessionBusy = Boolean(value);
  updateControlState();
}

export async function createSession(mode) {
  if (state.blocked || state.sessionBusy || state.adminBusy || state.submitting) return;
  setSessionBusy(true);
  try {
    const response = await apiRequest("/api/sessions", {
      method: "POST",
      body: JSON.stringify(mode === "dev" ? { mode: "dev" } : {})
    });
    const payload = await response.json();
    const record = payload?.session && typeof payload.session === "object" ? payload.session : null;
    const sessionId = String(record?.session_id || "");
    if (sessionId && !findSession(sessionId)) {
      state.sessions.unshift(record);
      renderSessionList();
    }
    if (sessionId) await loadSessionView(sessionId);
    focusComposerIfDesktop();
  } catch (error) {
    showToast(error.message || "新建会话失败", "error");
  } finally {
    setSessionBusy(false);
  }
}

export async function openSessionView(sessionId, { userInitiated = true } = {}) {
  if (!sessionId) return;
  if (sessionId === state.viewSessionId && !state.viewLoading) {
    closeSidebar();
    scrollToBottom({ force: true, smooth: true });
    return;
  }
  await loadSessionView(sessionId, { userInitiated });
}

export async function loadSessionView(sessionId, { quiet = false, userInitiated = false } = {}) {
  if (!sessionId || (quiet && sessionId !== state.viewSessionId) || (state.viewLoading && !userInitiated)) return;
  // 命令回执是会话内的临时记录，换会话就清掉——否则会串到别的会话里。
  // 回执按会话记账（commands.js），切走再切回来仍在原位，这里不再清空。
  if (state.unreadSessions.delete(sessionId)) renderSessionList();
  const generation = ++viewState.viewLoadGeneration;
  state.viewLoading = true;
  // 先切后加载:用户点标签的一刻立刻高亮目标会话、收起侧栏、给对话区铺一层
  // 加载动画,大会话拉取期间不再像卡在旧会话上(09-12 用户报)。真正的视图
  // 由下面 applySessionView 拉回后应用。
  if (userInitiated && sessionId !== state.viewSessionId) {
    state.switchingToSessionId = sessionId;
    renderSessionList();
    closeSidebar();
    elements.conversationStage?.classList.add("is-switching");
  }
  try {
    const response = await apiRequest(`/api/sessions/${encodeURIComponent(sessionId)}/turns`);
    const payload = await response.json();
    if (generation !== viewState.viewLoadGeneration) return;
    applySessionView(payload);
    if (!quiet) closeSidebar();
  } catch (error) {
    if (generation !== viewState.viewLoadGeneration) return;
    if (error.status === 401) showBlockedState(true);
    else if (error.status === 404) {
      showToast("会话不存在", "error");
      refreshSessions();
      if (sessionId === state.viewSessionId) window.setTimeout(() => openFallbackSessionView(sessionId), 0);
    } else showToast(error.message || "载入会话失败", "error");
  } finally {
    if (generation === viewState.viewLoadGeneration) {
      state.viewLoading = false;
      state.switchingToSessionId = "";
      elements.conversationStage?.classList.remove("is-switching");
      updateControlState();
    }
  }
}

export function disposeAllLiveRuns() {
  for (const live of state.liveRuns.values()) disposeLiveState(live);
  state.liveRuns.clear();
  elements.liveStopRail.replaceChildren();
  elements.liveStopRail.hidden = true;
}

// 切换会话不再销毁还在跑的直播状态:事件环只留 4096 条,长回复从 0 重放
// 必撞 resync,已渲染的内容就永远回不来了。改成离屏保活——DOM 游离但事件
// 照常写入,切回来原样重挂(reattachLiveArticles)。只清掉已结束的残壳。
export function retireLiveRunsForSwitch() {
  for (const [runId, live] of [...state.liveRuns.entries()]) {
    if (live.ended) {
      disposeLiveState(live);
      state.liveRuns.delete(runId);
    }
  }
  // 停止栏与问题坞都是全局元素,先清空;切回时按会话重挂。
  elements.liveStopRail.replaceChildren();
  elements.liveStopRail.hidden = true;
}

export function applySessionView(payload) {
  const sessionId = String(payload?.session_id || "");
  if (!sessionId) return;
  if (state.viewSessionId && state.viewSessionId !== sessionId && state.composerAttachments.length) {
    clearComposerAttachments(true);
  }
  retireLiveRunsForSwitch();
  clearViewSyncTimer();
  state.viewSessionId = sessionId;
  // 记住浏览位置，刷新后回到这里而不是跳去终端车道（见 preferredBootSession）。
  if (!isTerminalSession(sessionId)) safeStorageSet(VIEW_SESSION_KEY, sessionId);
  if (state.sessionModelOverrideFor !== sessionId) {
    // 会话切换：先按"跟随全局"显示，再异步取回该会话的覆盖池。
    state.sessionModelOverride = null;
    state.sessionModelOverrideFor = "";
    updateCurrentModelDisplay();
    refreshSessionModelOverride(sessionId);
  }
  // 上下文条跟着看的会话走：不拉的话它一直显示上一个会话的数字，
  // 直到这个会话跑完一轮才被 run 事件纠正。
  refreshSessionContext(sessionId);
  state.turns = Array.isArray(payload?.turns)
    ? payload.turns.sort((a, b) => asFiniteNumber(a?.seq) - asFiniteNumber(b?.seq))
    : [];
  state.queuedPrompts = Array.isArray(payload?.queued_prompts) ? payload.queued_prompts : [];
  state.redoCandidate = payload?.redo_candidate && typeof payload.redo_candidate === "object"
    ? payload.redo_candidate
    : null;
  closeRevisionEditor();
  state.pendingSubmission = null;
  const runs = (Array.isArray(payload?.runs) ? payload.runs : []).filter((run) => run?.run_id);
  if (runs.length) state.runsBySession.set(sessionId, new Set(runs.map((run) => String(run.run_id))));
  else state.runsBySession.delete(sessionId);
  state.viewRunningTurnId = !runs.length && typeof payload?.running_turn_id === "string" && payload.running_turn_id
    ? payload.running_turn_id
    : null;
  renderConversation({ forceScroll: true });
  renderQueueTray();
  renderJobsStrip();
  restoreLiveRuns(runs);
  updateConversationChrome();
  updateControlState();
  scheduleViewSync();
}

export function findUnclaimedRunningTurn() {
  const claimed = new Set();
  for (const live of state.liveRuns.values()) {
    if (live.turnId) claimed.add(String(live.turnId));
  }
  return state.turns.find((turn) => turn?.status === "running" && !claimed.has(String(turn?.id))) || null;
}

export function createLiveForRun(runId, userText = "", options = {}) {
  const { claimTurn = true, operation = "create", turnId = null, inputId = null } = options;
  const existing = state.liveRuns.get(runId);
  if (existing) return existing;
  const redo = operation === "redo";
  const runningTurn = redo || userText || !claimTurn ? null : findUnclaimedRunningTurn();
  const live = createLiveState(runId, {
    sessionId: options.sessionId,
    turnId: turnId || runningTurn?.id || null,
    userText: userText || runningTurn?.user_content || "",
    userAttachments: runningTurn?.attachments || [],
    startedAt: runningTurn?.user_timestamp || new Date(),
    userRendered: redo || Boolean(runningTurn),
    operation,
    inputId,
    editedContent: options.editedContent
  });
  state.liveRuns.set(runId, live);
  return live;
}

export function beginRunReplay(runIds = null) {
  // 事件环形缓冲已滚过上限时,after=0 必然触发 resync_required →
  // bootstrap → 又 replay 的循环:短窗口内连续吃到 resync 就放弃从头
  // 重放,live 状态由 bootstrap 快照兜底,增量从当前事件 id 继续。
  const now = Date.now();
  if (state.replayResyncCount >= 2 && now - state.replayResyncAt < 15000) {
    state.replayRunIds = null;
    connectEventSource(state.lastEventId);
    return;
  }
  // 只重放传入的 run(全新空壳);离屏保活的 live 已吃过这些事件,再放
  // 一遍正文就翻倍了。
  state.replayRunIds = runIds ? new Set(runIds) : new Set(state.liveRuns.keys());
  state.replayCutoff = Math.max(state.lastEventId, state.replayCutoff, state.latestEventId);
  state.lastEventId = 0;
  connectEventSource(0);
}

// 把落库的这一回合(含回合中途检查点写下的子代理子过程)按实时事件的**同一套
// handler** 回放进一个 live run:刷新/切会话重连时用它给 live 气泡「播种」,让后续
// 实时事件无缝接上,不再另起空壳、也不再画重复卡(#5b 重连渲染重做)。用真 handler
// 回放而不是自己搭 DOM,是为了让 live.tools/live.blocks/正文累计等内部状态和正常
// 流式时完全一致——尤其正在跑的那次子代理调用不喂 tool.finished,留着让实时续。
export function seedLiveFromPersistedTurn(live, turn) {
  ensureLiveArticle(live);
  // 播种是回放历史,不该喂「累计」的实时子代理估算(否则已完成子代理会和后端基线
  // 重复计;#131)。置旗让 tool.progress 里那段 liveSubagentTokens 更新跳过。
  state.seedingLive = true;
  try {
    seedLiveRounds(live, turn);
  } finally {
    state.seedingLive = false;
  }
  // 播种是一次性灌进一大坨,子过程区停在顶部;若不拉到底,后续实时更新的
  // subStickBottom 会「测得改前不在底」→ 从此不再跟随(#159 刷新后不自动向下滚)。
  // 排在播种自身的 rAF 之后再拉一次底,让在跑的子代理接着贴底跟随。
  window.requestAnimationFrame(() => {
    for (const tool of live.tools.values()) {
      if (tool?.isTask && !tool.finished && tool.blocks) {
        const c = subScrollContainer(tool);
        if (c) c.scrollTop = c.scrollHeight;
      }
    }
  });
}

export function seedLiveRounds(live, turn) {
  const rounds = Array.isArray(turn?.tool_flow) ? turn.tool_flow : [];
  for (const round of rounds) {
    const reasoning = String(round?.assistant_reasoning || "");
    if (reasoning.trim() && !reasoningHidden()) {
      handleReasoningEvent("reasoning.start", live, {});
      handleReasoningEvent("reasoning.delta", live, { delta: reasoning });
      handleReasoningEvent("reasoning.part_end", live, {});
    }
    const content = String(round?.assistant_content || "");
    if (content.trim()) appendAssistantDelta(live, content);
    for (const call of Array.isArray(round?.calls) ? round.calls : []) {
      handleToolEvent("tool.started", live, {
        tool_id: call?.id, name: call?.name,
        display_name: call?.display_name, arguments: call?.arguments,
      });
      if (isSubagentTool(call?.name) && Array.isArray(call?.sub_trace)) {
        for (const marker of call.sub_trace) {
          handleToolEvent("tool.progress", live, {
            tool_id: call?.id, name: call?.name, message: String(marker),
          });
        }
      }
      const output = String(call?.output || "");
      // 有真实输出 = 这次调用已完成才收尾;检查点里在跑的那次 output 是空的(或
      // 派生时的占位「(tool result unavailable)」),不收尾——让它保持运行态,实时
      // 事件到了继续更新同一张卡。
      if (output && output !== "(tool result unavailable)") {
        handleToolEvent("tool.finished", live, {
          tool_id: call?.id, name: call?.name, output, ok: call?.ok !== false,
        });
      }
    }
  }
}

export function restoreLiveRuns(runs) {
  // 只有全新空壳需要事件重放;离屏保活切回来的 live 内容都在,重放反而
  // 会把正文写两遍。
  const fresh = new Set();
  let seededConnect = false;
  // 正在跑的那条回合:create 的 runs 不带 turn_id,靠回合状态兜底认它。
  const runningTurn = state.turns.find((turn) => turn?.status === "running");
  for (const run of runs) {
    const runId = String(run?.run_id || "");
    if (!runId || state.terminalRunIds.has(runId)) continue;
    const kept = state.liveRuns.has(runId);
    const turnId = String(run?.turn_id || "") || (runningTurn ? String(runningTurn.id) : "");
    const turn = turnId ? state.turns.find((t) => String(t?.id) === turnId) : null;
    const live = createLiveForRun(runId, "", {
      operation: String(run?.operation || "create"),
      turnId: turnId || null,
      inputId: String(run?.input_id || "") || null
    });
    if (live.operation === "redo" && state.turns.some((turn) => {
      return String(turn?.id) === String(live.turnId) && turn?.status === "running";
    })) {
      live.redoCommitted = true;
    }
    // 这条重连回合已被 renderConversation 按落库快照画成了持久气泡,而且快照里有回合
    // 中途检查点写下的内容(#5a 起,子代理子过程也在)。这种情况把内容「播种」进
    // live 气泡、删掉那张持久气泡,而不是另起一个空壳叠上去(#5b:刷新后一个空
    // 「开发中」壳压在有内容的持久泡旁边);也不从 0 重放服务端事件(环缓冲早滚过
    // →resync→bootstrap 死循环,几十秒空白还停不掉——#3)。改增量续上。
    const canSeed = !kept && live.operation !== "redo" && turn && turn.status === "running"
      && ((Array.isArray(turn.tool_flow) && turn.tool_flow.length)
        || String(turn.assistant_content || "").trim());
    if (live.operation === "redo") {
      // redo 走原路(它自己会提交/重挂)。
    } else if (canSeed) {
      const persisted = [...elements.timeline.querySelectorAll(
        `article.assistant-message[data-turn-id="${turnId}"]`
      )].find((n) => !n.classList.contains("live-assistant"));
      ensureLiveArticle(live);
      seedLiveFromPersistedTurn(live, turn);
      showTypingIndicator(live);
      if (persisted) {
        // 把 live 气泡挪到持久气泡原位再删持久气泡,保持时间线顺序。
        if (persisted.parentNode === elements.timeline && live.article) {
          elements.timeline.insertBefore(live.article, persisted);
        }
        persisted.remove();
      }
      seededConnect = true;
    } else {
      // 立刻把气泡建出来,不等下一个事件。停止按钮和等待动效就都回来了。
      ensureLiveArticle(live);
      showTypingIndicator(live);
      if (!kept) fresh.add(runId);
    }
  }
  if (fresh.size) {
    beginRunReplay(fresh);
  } else if (seededConnect) {
    // 播种过、没有需要从 0 重放的空壳:仍要连上事件流看后续与收尾(applySessionView
    // 只在 liveRuns 为空时连,这里已非空)。从当前最新事件增量续上,不撞 resync。
    state.replayRunIds = null;
    state.lastEventId = Math.max(state.lastEventId, state.latestEventId);
    connectEventSource(state.lastEventId);
  }
}

export async function openFallbackSessionView(excludedSessionId) {
  const excluded = String(excludedSessionId || "");
  if (state.viewSessionId !== excluded) return;
  // deleteSession() 和 session.deleted 事件会各来一次，且到达可能有先后：
  // 只防并发的旗标挡不住"第一次兜底完成后第二次才到"的时序，两边各建一个
  // 新会话，删一个凭空多出两个。按被删会话 id 上一次性闩锁：同一场删除，
  // 兜底只发生一次。
  if (state.fallbackInFlight || state.fallbackDoneFor === excluded) return;
  state.fallbackInFlight = true;
  state.fallbackDoneFor = excluded;
  try {
    await openFallbackSessionViewInner(excluded);
  } finally {
    state.fallbackInFlight = false;
  }
}

export async function openFallbackSessionViewInner(excluded) {
  // 终端集成会话不能当兜底：它在侧栏里是隐藏的，掉进去看着就像「我的对话
  // 全没了」。一个可见会话都不剩时走 loadBootstrap()，让空状态兜底。
  const fallback = state.currentSessionId
    && state.currentSessionId !== excluded
    && !isTerminalSession(state.currentSessionId)
    ? state.currentSessionId
    : String(state.sessions.find((session) => {
        const id = String(session?.session_id || "");
        return id !== excluded && !isTerminalSession(id);
      })?.session_id || "");
  if (fallback) {
    await loadSessionView(fallback);
    return;
  }
  // 本地列表空了先跟服务端对一次：删最后一个会话时顶替的新会话由服务端建
  // （session.created 先于 session.deleted 广播，DELETE 回执里也带着），这里
  // 通常已经在列表里；SSE 掉过事件才会走到这一步。不这么对一次的话，每个
  // 开着该会话的页面都会自己 POST 一个，删一个多出两个（09-10 复现）。
  await refreshSessions();
  const refreshed = String(state.sessions.find((session) => {
    const id = String(session?.session_id || "");
    return id !== excluded && !isTerminalSession(id);
  })?.session_id || "");
  if (refreshed) {
    await loadSessionView(refreshed);
    return;
  }
  // 一个可见会话都不剩：直接新建一个顶上。落进空状态的话，用户面对的是一个
  // 不在侧栏里的「幽灵视图」，在里面打字实际写进隐藏的终端集成车道。
  // 不走 createSession()——删除流程还举着 sessionBusy，它会直接返回。
  try {
    const response = await apiRequest("/api/sessions", {
      method: "POST",
      body: JSON.stringify({}),
    });
    const record = (await response.json())?.session;
    const sessionId = String(record?.session_id || "");
    if (sessionId) {
      if (!findSession(sessionId)) {
        state.sessions.unshift(record);
        renderSessionList();
      }
      await loadSessionView(sessionId);
      return;
    }
  } catch (_) {
    // 新建失败（离线等）：退回空状态兜底，至少不落进隐藏车道。
  }
  await loadBootstrap();
}

export async function deleteSession(sessionId) {
  const session = findSession(sessionId);
  if (!window.confirm(`删除会话「${sessionDisplayName(session)}」？此操作无法撤销。`)) return;
  if (state.sessionBusy) return;
  setSessionBusy(true);
  try {
    const response = await apiRequest(`/api/sessions/${encodeURIComponent(sessionId)}`, { method: "DELETE" });
    showToast("会话已删除");
    state.sessions = state.sessions.filter((item) => String(item?.session_id) !== String(sessionId));
    // 删的是最后一个会话时，服务端已经建好顶替的那个并随回执带回；事件
    // 到达有先后，这里直接收进列表，兜底就不会再去新建。
    const replacement = (await response.json().catch(() => null))?.fallback;
    const replacementId = String(replacement?.session_id || "");
    if (replacementId && !findSession(replacementId)) state.sessions.unshift(replacement);
    renderSessionList();
    if (sessionId === state.viewSessionId) await openFallbackSessionView(sessionId);
  } catch (error) {
    showToast(error.message || "删除失败", "error");
  } finally {
    setSessionBusy(false);
  }
}

export function handleSessionEvent(name, data) {
  if (name === "session.reordered") {
    // 发起端已乐观重排(lastReorderIds 一致就不用刷);其它客户端拉一次。
    const ids = Array.isArray(data?.session_ids) ? data.session_ids.map(String).join("\n") : "";
    if (ids && ids !== state.lastReorderIds) refreshSessions();
    return;
  }
  const sessionId = String(data?.session_id || "");
  if (!sessionId) return;
  if (name === "session.created") {
    if (data?.platform) return;
    if (!findSession(sessionId)) {
      state.sessions.unshift({
        session_id: sessionId,
        name: String(data?.name || ""),
        kind: "",
        sandbox: "",
        mode: data?.mode === "dev" ? "dev" : "normal",
        created_at: null,
        updated_at: new Date().toISOString(),
        turn_count: 0,
        last_user_content: ""
      });
      renderSessionList();
    }
  } else if (name === "session.renamed") {
    const target = findSession(sessionId);
    if (target) target.name = String(data?.name || "");
    renderSessionList();
    if (sessionId === state.viewSessionId) updateConversationChrome();
  } else if (name === "session.deleted") {
    state.sessions = state.sessions.filter((item) => String(item?.session_id) !== sessionId);
    renderSessionList();
    if (sessionId === state.viewSessionId && !state.bootstrapPromise && !state.viewLoading) {
      openFallbackSessionView(sessionId);
    }
  } else if (name === "session.updated") {
    const target = findSession(sessionId);
    if (target && Object.prototype.hasOwnProperty.call(data || {}, "sandbox")) {
      target.sandbox = String(data?.sandbox || "");
    }
    if (Object.prototype.hasOwnProperty.call(data || {}, "model_override") && sessionId === state.viewSessionId) {
      setSessionModelOverride(sessionId, data.model_override);
    }
    renderSessionList();
    if (sessionId === state.viewSessionId) updateConversationChrome();
  } else if (name === "session.current_changed") {
    // 每视图独立浏览：默认会话只影响侧栏「默认」徽标，不再跟随切换。
    state.currentSessionId = sessionId;
    renderSessionList();
  }
}
