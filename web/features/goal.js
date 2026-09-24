import { apiRequest } from "../core/api.js";
import { asFiniteNumber, formatTime, formatTokens } from "../core/format.js";
import { makeIconSlot } from "../core/icons.js";
import { showToast } from "../core/toast.js";
import { runSessionId } from "./sessions/runs.js";
import { loadSessionView } from "./sessions/view.js";
import { updateContext } from "./status.js";
import { elements } from "../state/elements.js";
import { state } from "../state/store.js";

/// 只有本模块用的状态（从 state/store.js 分出来的私有分片）。
const goalState = {
  stageTodos: null,
  goal: null,
  goalGeneration: 0,
  stageTodosGeneration: 0
};

/// 常驻任务面板：当前会话的待办。
///
/// 两条更新路径。进会话/刷新走 `GET /api/sessions/{id}/todos`——工具事件
/// 只在 `todowrite` 跑的那一刻发生一次,不问一次就只有空面板；回合里 AI
/// 改了待办则直接吃 `tool.finished` 的输出,不必再往返一趟。
export function renderStageTodos(todos) {
  goalState.stageTodos = todos?.length ? todos : null;
  const panel = elements.stageTodos;
  panel.replaceChildren();
  const card = goalState.stageTodos ? window.GqyTodos?.renderList(goalState.stageTodos) : null;
  if (!card) {
    panel.hidden = true;
    return;
  }
  panel.appendChild(card);
  panel.hidden = false;
}

export const GOAL_PHASE_LABELS = Object.freeze({
  active: "进行中",
  paused: "已暂停",
  blocked: "受阻",
  complete: "已完成",
});

/// 目标状态行。
///
/// 目标是会话级的长期状态，不该只在对话流里闪一条消息就没了——那条消息会
/// 被后面几十轮顶到看不见的地方。贴在输入框上方，随状态刷新，能直接操作。
export function renderGoalBar() {
  const bar = elements.goalBar;
  bar.replaceChildren();
  const goal = goalState.goal;
  // 完成的目标不再占位：那一行的作用是「它还在做这件事」，做完了就该让开。
  // 想回顾结果，AI 的结案陈词就在对话流里。
  if (!goal || goal.phase === "complete") {
    bar.hidden = true;
    return;
  }
  bar.hidden = false;
  bar.dataset.phase = String(goal.phase || "");

  const mark = document.createElement("span");
  mark.className = "goal-bar-mark";
  mark.appendChild(makeIconSlot("target"));

  // 一行装下：目标 + 状态。轮数上限不显示——256 是防跑飞的兜底，不是进度
  // 条的分母，写出来只会让人以为要跑 256 轮。
  const objective = document.createElement("strong");
  objective.className = "goal-bar-objective";
  objective.textContent = String(goal.objective || "");
  objective.title = "点击修改目标";
  objective.tabIndex = 0;
  objective.setAttribute("role", "button");
  const startEdit = () => beginGoalEdit(objective, goal);
  objective.addEventListener("click", startEdit);
  objective.addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      startEdit();
    }
  });

  const meta = document.createElement("small");
  meta.className = "goal-bar-meta";
  // active 但没武装 = 目标还在、只是不会自己往前跑了（被打断过或重启过）。
  const phase = goal.phase === "active" && !goal.armed
    ? "已停下"
    : GOAL_PHASE_LABELS[goal.phase] || goal.phase;
  meta.textContent = `${phase} · 第 ${goal.rounds_started} 轮`;
  if (goal.blocked_message) meta.title = goal.blocked_message;

  const actions = document.createElement("span");
  actions.className = "goal-bar-actions";
  // 按钮跟着阶段变：暂停的目标不该还挂着「暂停」。
  // 编辑排在最前：点文字也能改，但一个明确的按钮才看得出「这行可以改」。
  const edit = document.createElement("button");
  edit.type = "button";
  edit.className = "goal-bar-button";
  edit.title = "修改目标";
  edit.setAttribute("aria-label", "修改目标");
  edit.append(makeIconSlot("square-pen"));
  edit.addEventListener("click", startEdit);
  actions.appendChild(edit);
  const buttons = goal.phase === "active" && goal.armed
    ? [["pause", "暂停", "pause"], ["clear", "清除", "x"]]
    : [["resume", "继续", "play"], ["clear", "清除", "x"]];
  for (const [action, label, icon] of buttons) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "goal-bar-button";
    button.title = label;
    button.setAttribute("aria-label", label);
    button.append(makeIconSlot(icon));
    button.addEventListener("click", () => runGoalAction(action));
    actions.appendChild(button);
  }
  bar.append(mark, objective, meta, actions);
}

/// 就地改目标：点一下文字变输入框，回车提交，Esc 放弃。
export function beginGoalEdit(node, goal) {
  // 多行文本框(09-12 用户报单行不好写不好看):自动撑高,回车提交、
  // Shift+回车换行、Esc 放弃。
  const input = document.createElement("textarea");
  input.className = "goal-bar-edit";
  input.rows = 1;
  input.value = String(goal.objective || "");
  input.setAttribute("aria-label", "修改目标");
  const autosize = () => {
    input.style.height = "auto";
    input.style.height = `${Math.min(input.scrollHeight, 220)}px`;
  };
  // `finish` 会被回车和失焦各触发一次——提交时把输入框换掉，那一下又会
  // 触发 blur。没有这个闸就会连发两次 edit。
  let settled = false;
  const finish = (commit) => {
    if (settled) return;
    settled = true;
    const next = input.value.trim();
    if (commit && next && next !== goal.objective) runGoalAction(`edit ${next}`);
    else renderGoalBar();
  };
  input.addEventListener("keydown", (event) => {
    event.stopPropagation();
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      finish(true);
    } else if (event.key === "Escape") {
      event.preventDefault();
      finish(false);
    }
  });
  input.addEventListener("input", autosize);
  input.addEventListener("blur", () => finish(true));
  node.replaceWith(input);
  input.focus();
  input.select();
  autosize();
}

export async function runGoalAction(action) {
  try {
    const response = await apiRequest("/api/goal", {
      method: "POST",
      body: JSON.stringify({ session_id: state.viewSessionId, input: action }),
    });
    // 服务端把「拒绝」也当成一次成功的命令执行（HTTP 200 + 一段说明文字），
    // 所以不能只看 HTTP 状态——不弹出来的话，改目标失败时状态行只是悄悄
    // 变回原样，看着像点了没反应。
    const text = String((await response.json())?.text || "");
    if (/^(用法|\/goal |本会话)/.test(text)) showToast(text.split("\n")[0], "error");
    // edit 命中正在跑的续轮时，daemon 会掐掉旧轮、按新目标重开一轮——
    // 中断和新气泡就是时间线上的反馈，这里只补一个轻量确认。
    else if (action.startsWith("edit ")) showToast(`目标已变更：${text.split("\n")[1] || ""}`);
  } catch (error) {
    showToast(error?.message || "目标操作失败", "error");
  }
  loadGoal(state.viewSessionId);
}

/// `/pop`（无参数）的轮次多选器：列出可弹出的轮次（最旧在前，与按数量
/// 弹出同一口径），勾选后按 turn_ids 弹出。
export async function openPopPicker() {
  const sessionId = state.viewSessionId;
  if (!sessionId) return;
  let turns = [];
  try {
    const response = await apiRequest(`/api/sessions/${encodeURIComponent(sessionId)}/poppable`);
    turns = (await response.json())?.turns || [];
  } catch (error) {
    showToast(error.message || "读取可弹出轮次失败", "error");
    return;
  }
  const list = elements.popDialogList;
  list.replaceChildren();
  elements.popDialogAll.checked = false;
  if (!turns.length) {
    const empty = document.createElement("div");
    empty.className = "pop-dialog-empty";
    empty.textContent = "当前上下文没有可弹出的轮次";
    list.appendChild(empty);
  }
  const boxes = [];
  for (const turn of turns) {
    const row = document.createElement("label");
    row.className = "pop-dialog-row";
    const box = document.createElement("input");
    box.type = "checkbox";
    box.value = String(turn?.turn_id || "");
    const preview = document.createElement("span");
    preview.className = "pop-row-preview";
    preview.textContent = String(turn?.preview || "").trim() || "（空消息）";
    const meta = document.createElement("span");
    meta.className = "pop-row-meta";
    const tokens = asFiniteNumber(turn?.tokens);
    meta.textContent = [formatTime(turn?.timestamp), tokens ? formatTokens(tokens) : ""]
      .filter(Boolean)
      .join(" · ");
    row.append(box, preview, meta);
    list.appendChild(row);
    boxes.push(box);
  }
  const refresh = () => {
    const selected = boxes.filter((box) => box.checked).length;
    elements.popConfirmButton.disabled = selected === 0;
    elements.popConfirmButton.textContent = selected ? `弹出所选（${selected}）` : "弹出所选";
    elements.popDialogAll.checked = boxes.length > 0 && selected === boxes.length;
  };
  // onchange 直接赋值而不是 addEventListener：每次打开都重建列表，
  // 累加监听器会让旧闭包一直陪跑。
  boxes.forEach((box) => { box.onchange = refresh; });
  elements.popDialogAll.onchange = () => {
    boxes.forEach((box) => { box.checked = elements.popDialogAll.checked; });
    refresh();
  };
  elements.popConfirmButton.onclick = async () => {
    const turnIds = boxes.filter((box) => box.checked).map((box) => box.value);
    if (!turnIds.length) return;
    elements.popConfirmButton.disabled = true;
    try {
      const response = await apiRequest("/api/conversation/pop", {
        method: "POST",
        body: JSON.stringify({ session_id: sessionId, turn_ids: turnIds }),
      });
      const removed = (await response.json())?.result?.turns || 0;
      elements.popDialog.close();
      await loadSessionView(sessionId, { quiet: true });
      showToast(`已从上下文弹出 ${removed} 轮`);
    } catch (error) {
      elements.popConfirmButton.disabled = false;
      showToast(error.message || "弹出失败", "error");
    }
  };
  refresh();
  if (typeof elements.popDialog.showModal === "function") elements.popDialog.showModal();
  else elements.popDialog.setAttribute("open", "");
}

// 命令回执的锚点回合。优先锚到正在流式输出的那一轮：它落盘后 id 不变，
// 回执就一直钉在它后面；只认「最后一个已落盘回合」的话，运行中敲的命令
// 会因为这一轮还没落盘而没有锚点，被顶到时间线最前面。
export function commandAnchorTurnId() {
  const live = [...state.liveRuns.values()].find((entry) => entry && !entry.ended && entry.turnId);
  if (live) return String(live.turnId);
  return state.turns.length ? String(state.turns[state.turns.length - 1]?.id || "") : "";
}

export async function refreshSessionContext(sessionId) {
  const scope = String(sessionId || "");
  if (!scope) return;
  const generation = (state.contextGeneration = (state.contextGeneration || 0) + 1);
  try {
    const response = await apiRequest(`/api/sessions/${encodeURIComponent(scope)}/context`);
    const payload = await response.json();
    // 用户可能在响应回来之前又切走了：旧响应不许覆盖新会话的数字。
    if (generation !== state.contextGeneration || state.viewSessionId !== scope) return;
    state.context.tokens = Math.max(0, asFiniteNumber(payload?.context_tokens));
    state.context.window = payload?.context_window == null
      ? null
      : Math.max(0, asFiniteNumber(payload.context_window));
    updateContext();
  } catch (_) {
    // 拉不到就保持现状，等 run 事件里的增量。
  }
}

export async function loadGoal(sessionId) {
  const scope = String(sessionId || "");
  if (!scope) {
    goalState.goal = null;
    renderGoalBar();
    return;
  }
  const generation = ++goalState.goalGeneration;
  try {
    const response = await apiRequest(`/api/sessions/${encodeURIComponent(scope)}/goal`);
    const payload = await response.json();
    if (generation !== goalState.goalGeneration) return;
    goalState.goal = payload?.goal || null;
  } catch (_) {
    if (generation !== goalState.goalGeneration) return;
    goalState.goal = null;
  }
  renderGoalBar();
}

export async function loadStageTodos(sessionId) {
  const scope = String(sessionId || "");
  if (!scope) {
    renderStageTodos(null);
    return;
  }
  const generation = ++goalState.stageTodosGeneration;
  try {
    const response = await apiRequest(`/api/sessions/${encodeURIComponent(scope)}/todos`);
    const payload = await response.json();
    if (generation !== goalState.stageTodosGeneration) return;
    renderStageTodos(window.GqyTodos?.normalize(payload?.todos) || null);
  } catch (_) {
    // 面板是附带信息,拿不到就空着,不打扰对话。
    if (generation === goalState.stageTodosGeneration) renderStageTodos(null);
  }
}

/// 新版 todowrite 输出不含清单本体,实时卡片与舞台面板改从会话 API 取。
/// 拿不到就静默放弃——面板是附带信息,不打扰对话。
export async function attachLiveTodoPanel(tool, live, sameSession) {
  const scope = runSessionId(live.runId);
  if (!scope) return;
  try {
    const response = await apiRequest(`/api/sessions/${encodeURIComponent(scope)}/todos`);
    const payload = await response.json();
    const todos = window.GqyTodos?.normalize(payload?.todos) || null;
    if (sameSession) renderStageTodos(todos);
    const panel = todos ? window.GqyTodos.renderList(todos) : null;
    tool.card.querySelector(".todo-panel")?.remove();
    if (panel) tool.card.appendChild(panel);
  } catch (_) {}
}
