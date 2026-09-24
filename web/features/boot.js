import { apiRequest } from "../core/api.js";
import { asFiniteNumber } from "../core/format.js";
import { safeStorageGet } from "../core/storage.js";
import { syncUiPrefs } from "./appearance.js";
import { applyRoleVisibility, setLoginSubmitting, showBlockedState, showSetupAdmin } from "./auth.js";
import { updateControlState } from "./composer/input.js";
import { refreshVoiceButton } from "./composer/voice.js";
import { updateConversationChrome } from "./conversation/chrome.js";
import { renderConversation } from "./conversation/render.js";
import { clearViewSyncTimer, closeEventSource, connectEventSource } from "./live/sse.js";
import { renderQueueTray } from "./live/state.js";
import { renderModelMenu, updateCurrentModelDisplay } from "./model-menu/menu.js";
import { loadThinkingVariants } from "./model-menu/variants.js";
import { oobeState, openOobe } from "./oobe.js";
import { applyPersona } from "./persona.js";
import { isTerminalSession } from "./sessions/list.js";
import { findSession, trackRun } from "./sessions/runs.js";
import { applySessionView, disposeAllLiveRuns, loadSessionView } from "./sessions/view.js";
import { setConnectionStatus, updateCapabilities, updateContext, updateRuntimeUsage } from "./status.js";
import { elements } from "../state/elements.js";
import { state } from "../state/store.js";
import { clearInlineError } from "../widgets/inline-error.js";

/// 只有本模块用的状态（从 state/store.js 分出来的私有分片）。
const bootState = {
  bootId: null,
  version: null
};

export const VIEW_SESSION_KEY = "gqy.web.viewSession";

/// 页面加载后该打开哪个会话。
///
/// 不能直接用 daemon 的 `current_session`：那个指针归终端车道所有（shellhook
/// 与 CLI 用它），而终端集成会话在 WebUI 的侧栏里是隐藏的——刷新一下就掉进
/// 一个列表里根本看不到的会话，看着像「我的对话没了」。
///
/// 顺序：上次浏览的 → 当前指针（如果它在列表里可见）→ 列表第一个。
export function preferredBootSession() {
  const remembered = safeStorageGet(VIEW_SESSION_KEY);
  if (remembered && findSession(remembered) && !isTerminalSession(remembered)) return remembered;
  if (state.currentSessionId
    && findSession(state.currentSessionId)
    && !isTerminalSession(state.currentSessionId)) {
    return state.currentSessionId;
  }
  const visible = state.sessions.find((session) => !isTerminalSession(session?.session_id));
  return visible ? String(visible.session_id) : "";
}

export function applyBootstrap(snapshot) {
  if (snapshot?.account?.setup_pending) {
    state.account = snapshot.account;
    showSetupAdmin();
    return;
  }
  state.blocked = false;
  document.body.classList.remove("is-login", "is-blocked");
  clearViewSyncTimer();
  disposeAllLiveRuns();
  bootState.bootId = String(snapshot?.boot_id || "");
  state.latestEventId = Math.max(0, asFiniteNumber(snapshot?.latest_event_id));
  state.models = Array.isArray(snapshot?.models) ? snapshot.models : [];
  applyPersona(snapshot?.persona);
  state.display = snapshot?.display && typeof snapshot.display === "object" ? snapshot.display : state.display;
  state.context = snapshot?.context && typeof snapshot.context === "object" ? snapshot.context : { tokens: 0, window: null };
  state.usage = snapshot?.usage && typeof snapshot.usage === "object" ? snapshot.usage : {};
    state.capabilities = snapshot?.capabilities && typeof snapshot.capabilities === "object" ? snapshot.capabilities : {};
  state.account = snapshot?.account && typeof snapshot.account === "object" ? snapshot.account : null;
  applyRoleVisibility();
  if (state.account?.oobe_pending && !oobeState.open) window.setTimeout(() => openOobe({ reason: "first" }), 350);
  state.sessions = Array.isArray(snapshot?.sessions) ? snapshot.sessions : [];
  state.currentSessionId = typeof snapshot?.current_session_id === "string" && snapshot.current_session_id ? snapshot.current_session_id : null;
  state.sessionMenuFor = null;
  state.sessionRenaming = null;
  bootState.version = snapshot?.version ?? null;
  state.pendingSubmission = null;
  const allRuns = (Array.isArray(snapshot?.runs) ? snapshot.runs : []).filter((run) => run?.run_id && run?.session_id);
  state.runsBySession = new Map();
  for (const run of allRuns) trackRun(String(run.session_id), String(run.run_id));
  elements.loginForm.hidden = true;
  elements.registerForm.hidden = true;
  elements.setupForm.hidden = true;
  elements.retryBootstrapButton.hidden = false;
  elements.loginPassword.value = "";
  elements.loginError.textContent = "";
  elements.loginError.hidden = true;
  setLoginSubmitting(false);
  elements.versionLabel.textContent = bootState.version ? `v${bootState.version}` : "--";
  clearInlineError();
  renderModelMenu();
  updateCapabilities();
  updateContext();
  state.replayRunIds = null;
  state.replayCutoff = 0;
  const boot = preferredBootSession();
  if (boot && boot !== state.viewSessionId) state.viewSessionId = boot;
  const keepView = state.viewSessionId && state.viewSessionId !== state.currentSessionId && findSession(state.viewSessionId);
  if (keepView) {
    // 视图停留在非默认会话：全局重载不改变浏览位置，改用会话接口回填。
    state.lastEventId = state.latestEventId;
    connectEventSource(state.latestEventId);
    loadSessionView(state.viewSessionId, { quiet: true });
  } else if (state.currentSessionId && !isTerminalSession(state.currentSessionId)) {
    applySessionView({
      session_id: state.currentSessionId,
      turns: snapshot?.turns,
      queued_prompts: snapshot?.queued_prompts,
      running_turn_id: snapshot?.running_turn_id,
      runs: allRuns.filter((run) => String(run.session_id) === String(state.currentSessionId)),
      redo_candidate: snapshot?.redo_candidate
    });
    if (state.liveRuns.size === 0) {
      state.lastEventId = state.latestEventId;
      connectEventSource(state.latestEventId);
    }
  } else {
    // 单会话兜底：没有会话指针时直接使用 bootstrap 快照。指针指着隐藏的
    // 终端车道时快照里的 turns 属于那条车道，画出来就是把隐藏会话泄漏给
    // WebUI——那种情况按空状态处理。
    const hiddenLane = isTerminalSession(state.currentSessionId);
    state.viewSessionId = null;
    state.sessionModelOverride = null;
    state.sessionModelOverrideFor = "";
    updateCurrentModelDisplay();
    state.viewRunningTurnId = !hiddenLane && typeof snapshot?.running_turn_id === "string" && snapshot.running_turn_id ? snapshot.running_turn_id : null;
    state.turns = !hiddenLane && Array.isArray(snapshot?.turns) ? snapshot.turns.sort((a, b) => asFiniteNumber(a?.seq) - asFiniteNumber(b?.seq)) : [];
    state.queuedPrompts = !hiddenLane && Array.isArray(snapshot?.queued_prompts) ? snapshot.queued_prompts : [];
    state.redoCandidate = !hiddenLane && snapshot?.redo_candidate && typeof snapshot.redo_candidate === "object"
      ? snapshot.redo_candidate
      : null;
    renderConversation({ forceScroll: true });
    renderQueueTray();
    state.lastEventId = state.latestEventId;
    connectEventSource(state.latestEventId);
  }
  setConnectionStatus("connecting");
  updateRuntimeUsage();
  updateConversationChrome();
  updateControlState();
  loadThinkingVariants();
}

export async function loadBootstrap() {
  if (state.bootstrapPromise) return state.bootstrapPromise;
  state.bootstrapPromise = (async () => {
    clearViewSyncTimer();
    closeEventSource();
    state.adminBusy = false;
    state.submitting = false;
    if (!state.turns.length && state.liveRuns.size === 0) {
      elements.loadingState.hidden = false;
      elements.blockedState.hidden = true;
      elements.emptyState.hidden = true;
      elements.timeline.hidden = true;
    }
    setConnectionStatus("connecting");
    updateControlState();
    try {
      const response = await apiRequest("/api/bootstrap");
      const snapshot = await response.json();
      applyBootstrap(snapshot);
      // 认证过了才拉外观偏好:未登录时这个接口本来就该 401。
      syncUiPrefs();
      // 命令清单与麦克风状态同理:WebUI 永远要登录(09-11),页面初始化那次
      // 拿到的是 401,登录之后必须重拿,否则 /reset /compact 全都当普通消息发出去。
      if (!state.blocked) {
        window.GqyCommands?.load(apiRequest);
        refreshVoiceButton();
      }
    } catch (error) {
      showBlockedState(error.status === 401, error.message);
    }
  })();
  try {
    await state.bootstrapPromise;
  } finally {
    state.bootstrapPromise = null;
  }
}
