import { renderSessionList } from "../sessions/list.js";
import { state } from "../../state/store.js";

// 顶栏没了,会话标题和「正在回复 · 工作区」那行副标题跟着没了——侧栏里
// 本来就高亮着当前会话,标题是第二份;运行状态现在由侧栏的转圈和输入框那排
// 的指示器表达,比一行小字显眼。剩下的是让侧栏重画。
export function updateConversationChrome() {
  renderSessionList();
}

// 离屏保活的 live 属于别的会话,不算「本视图在跑」。
export function liveViewed(live) {
  return !live?.sessionId || String(live.sessionId) === String(state.viewSessionId || "");
}

// 这个 turn 是否被本会话某个还在跑的 live 气泡认领:认领中的回合,
// 持久化渲染只画用户消息——checkpoint 落库的部分正文与气泡是同一份内容,
// 两边都画就是切回后正文翻倍。
export function liveClaimsTurn(turnId) {
  if (!turnId) return false;
  for (const live of state.liveRuns.values()) {
    if (!live.ended && liveViewed(live) && String(live.turnId) === String(turnId)) return true;
  }
  return false;
}

export function conversationRunning() {
  for (const live of state.liveRuns.values()) {
    if (liveViewed(live)) return true;
  }
  return Boolean(state.viewRunningTurnId);
}

export function activeTurnUpdateTarget(sessionId) {
  const runIds = state.runsBySession.get(String(sessionId || ""));
  if (!runIds) return null;
  const candidates = [...runIds]
    .map((runId) => state.liveRuns.get(String(runId)))
    .filter((live) => live && !live.ended && live.turnId);
  if (candidates.length !== 1) return null;
  return { runId: candidates[0].runId, turnId: candidates[0].turnId };
}

export function hasPendingQuestion() {
  for (const live of state.liveRuns.values()) {
    for (const question of live.questions.values()) {
      if (question.pending) return true;
    }
  }
  return false;
}
