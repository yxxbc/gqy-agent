import { firstLine } from "../../core/format.js";
import { state } from "../../state/store.js";

export function newestLiveRun() {
  let latest = null;
  for (const live of state.liveRuns.values()) latest = live;
  return latest;
}

export function deriveConversationDetails() {
  const live = newestLiveRun();
  if (state.turns.length === 0) {
    const liveUser = live?.userText || state.pendingSubmission?.content || "";
    if (!liveUser) return { title: "新对话", snippet: "尚未开始", timestamp: null };
    return { title: firstLine(liveUser) || "新对话", snippet: firstLine(liveUser), timestamp: new Date() };
  }
  const firstTurn = state.turns[0];
  const lastTurn = state.turns[state.turns.length - 1];
  const followups = Array.isArray(lastTurn?.followups) ? lastTurn.followups : [];
  const lastFollowup = followups[followups.length - 1];
  const assistant = String(lastTurn?.assistant_content || "").trim();
  const liveContent = live ? String(live.userText || "").trim() : "";
  const snippet = firstLine(liveContent || assistant || lastFollowup?.content || lastTurn?.user_content || "");
  const timestamp = liveContent ? live?.startedAt : lastTurn?.assistant_timestamp || lastFollowup?.submitted_at || lastTurn?.user_timestamp;
  return {
    title: firstLine(firstTurn?.user_content) || "当前对话",
    snippet: snippet || (lastTurn?.status === "running" ? "正在回复" : "对话已开始"),
    timestamp
  };
}

export function multiSessionEnabled() {
  return Boolean(state.capabilities?.multi_conversation);
}

export function sessionDisplayName(session) {
  const name = firstLine(session?.name || "");
  return name || "新会话";
}

export function findSession(sessionId) {
  const id = String(sessionId || "");
  return state.sessions.find((session) => String(session?.session_id) === id) || null;
}

export function viewSessionEntry() {
  return state.viewSessionId ? findSession(state.viewSessionId) : null;
}

export function trackRun(sessionId, runId) {
  const session = String(sessionId || "");
  const run = String(runId || "");
  if (!session || !run) return;
  let runs = state.runsBySession.get(session);
  if (!runs) {
    runs = new Set();
    state.runsBySession.set(session, runs);
  }
  runs.add(run);
}

export function untrackRun(runId) {
  const run = String(runId || "");
  for (const [sessionId, runs] of state.runsBySession) {
    if (runs.delete(run) && runs.size === 0) state.runsBySession.delete(sessionId);
  }
}

export function runSessionId(runId) {
  const run = String(runId || "");
  if (!run) return "";
  for (const [sessionId, runs] of state.runsBySession) {
    if (runs.has(run)) return sessionId;
  }
  return "";
}

export function sessionHasRuns(sessionId) {
  return (state.runsBySession.get(String(sessionId || ""))?.size || 0) > 0;
}
