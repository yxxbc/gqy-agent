import { apiRequest } from "../../core/api.js";
import { makeIconSlot } from "../../core/icons.js";
import { showToast } from "../../core/toast.js";
import { updateControlState } from "../composer/input.js";
import { closeRevisionEditor, makeCopyButton } from "../conversation/actions.js";
import { activeTurnUpdateTarget, conversationRunning, liveViewed, updateConversationChrome } from "../conversation/chrome.js";
import { createTurnStatus } from "../conversation/render.js";
import { contentAdded } from "../conversation/scroll.js";
import { createUserMessage, isSyntheticTurnContent } from "../conversation/user.js";
import { setPersonaAvatar } from "../persona.js";
import { updateQuestionDock } from "../questions.js";
import { loadSessionView } from "../sessions/view.js";
import { clearPreparingTool } from "../tools/events.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

/// 把还在跑的 live 气泡挂回重建后的时间线。
///
/// `renderConversation` 会 `replaceChildren()` 整段重建，而 live 气泡不在
/// `state.turns` 里——重建之后它就脱离了文档，后续的 assistant.delta 全写
/// 进一个看不见的节点，直到回合结束、那一轮作为持久化回合被画出来，内容才
/// 整段冒出来。自己发消息时不会中途重画，所以这个洞只在 daemon 自己发起的
/// 回合上露出来（目标续轮、后台任务唤醒）：它们的回合一落盘就触发重画。
export function reattachLiveArticles() {
  for (const live of state.liveRuns.values()) {
    if (!live.article || live.ended) continue;
    // 离屏保活的别会话气泡不能挂进当前时间线。
    if (!liveViewed(live)) continue;
    // 落库的 running 占位与直播气泡是同一轮:重挂前撤掉占位。
    removeRunningStatus(live.turnId);
    if (!live.article.isConnected) {
      elements.timeline.appendChild(live.article);
      pinQueuedMessages();
    }
    if (live.stopButton && !live.stopButton.isConnected) {
      elements.liveStopRail.appendChild(live.stopButton);
      elements.liveStopRail.hidden = false;
    }
    // 切走时被 clearQuestionDock 摘下的待答问题,切回原样归位。
    for (const question of live.questions?.values?.() || []) {
      if (question.pending && question.card && !question.card.isConnected) {
        elements.questionDock.appendChild(question.card);
      }
    }
  }
  updateQuestionDock();
  syncRunIndicator();
}

export function createLiveState(runId, options = {}) {
  return {
    runId,
    // 归属会话:切走时离屏保活、切回按它过滤重挂(retireLiveRunsForSwitch)。
    sessionId: String(options.sessionId || state.viewSessionId || ""),
    turnId: options.turnId || null,
    userText: options.userText || "",
    userAttachments: Array.isArray(options.userAttachments) ? options.userAttachments : [],
    startedAt: options.startedAt || new Date(),
    userRendered: Boolean(options.userRendered),
    article: null,
    blocks: null,
    headerStatus: null,
    stopButton: null,
    cancellationRequested: false,
    meta: null,
    endpoint: null,
    copyButton: null,
    currentText: null,
    assistantText: "",
    assistantReasoning: "",
    assets: [],
    artifacts: [],
    reasoning: null,
    reasoningParts: [],
    reasoningStarted: false,
    reasoningTitle: "",
    reasoningTimer: null,
    providerId: "",
    model: "",
    tools: new Map(),
    preparingTool: null,
    questions: new Map(),
    contextOperation: null,
    typing: null,
    typingAnimation: null,
    streamRail: null,
    ended: false,
    operation: options.operation || "create",
    inputId: options.inputId || null,
    editedContent: options.editedContent ?? null,
    redoCommitted: false
  };
}

export function isJobFollowupContent(content) {
  const raw = String(content || "");
  return isSyntheticTurnContent(raw);
}

// 排队的消息不再放输入框上方的托盘,直接画在对话末尾(用户气泡 + 「排队中」小签),
// 就是它轮到时会出现的位置。这里按 state.queuedPrompts 同步时间线里的占位:
// 少了的撤掉,多了的补上,顺序和位置(永远在最后)由 pinQueuedMessages 兜底。
export function renderQueueTray() {
  // 后台任务完成的自动跟进不是用户消息，不画。
  const prompts = (Array.isArray(state.queuedPrompts) ? state.queuedPrompts : [])
    .filter((prompt) => !isJobFollowupContent(prompt?.content) && !isJobFollowupContent(prompt?.display_content));
  if (elements.queueTray) {
    elements.queueTray.replaceChildren();
    elements.queueTray.hidden = true;
  }
  const ids = new Set(prompts.map((prompt) => String(prompt?.id)));
  for (const node of elements.timeline.querySelectorAll(".user-message.is-queued")) {
    if (!ids.has(String(node.dataset.queueId))) node.remove();
  }
  let added = null;
  for (const prompt of prompts) {
    const id = String(prompt?.id);
    if (queuedMessageNode(id)) continue;
    const node = createUserMessage(prompt?.content || "", prompt?.submitted_at || new Date(), {
      queued: true,
      queueId: id,
      attachments: prompt?.attachments
    });
    if (!node) continue;
    elements.timeline.appendChild(node);
    added = node;
  }
  pinQueuedMessages();
  if (added) contentAdded(added);
  updateControlState();
}

export function queuedMessageNode(id) {
  for (const node of elements.timeline.querySelectorAll(".user-message.is-queued")) {
    if (String(node.dataset.queueId) === String(id)) return node;
  }
  return null;
}

// 排队占位永远贴在时间线末尾,按排队顺序:直播气泡后挂进来、回合重建之后都要再钉一次
export function pinQueuedMessages() {
  for (const prompt of Array.isArray(state.queuedPrompts) ? state.queuedPrompts : []) {
    const node = queuedMessageNode(prompt?.id);
    if (node && node !== elements.timeline.lastElementChild) elements.timeline.appendChild(node);
  }
}

export async function removeQueuedPrompt(promptId) {
  if (!promptId) return;
  const target = activeTurnUpdateTarget(state.viewSessionId);
  if (!target) {
    showToast("无法确定排队消息所属的回复", "error");
    return;
  }
  try {
    await apiRequest(`/api/runs/${encodeURIComponent(target.runId)}/turns/${encodeURIComponent(target.turnId)}/queue/${encodeURIComponent(promptId)}`, { method: "DELETE" });
    state.queuedPrompts = state.queuedPrompts.filter((prompt) => String(prompt?.id) !== String(promptId));
    renderQueueTray();
  } catch (error) {
    showToast(error.message || "排队消息移除失败", "error");
    if (error.status === 404 && state.viewSessionId) await loadSessionView(state.viewSessionId, { quiet: true });
  }
}

export function disposeLiveState(live) {
  if (!live) return;
  for (const question of live.questions?.values?.() || []) {
    if (question.autoAdvanceTimer) window.clearTimeout(question.autoAdvanceTimer);
    question.autoAdvanceTimer = null;
  }
  clearPreparingTool(live);
  removeLiveStopButton(live);
  live.typingAnimation?.cancel();
  live.typingAnimation = null;
  if (live.reasoningTimer) {
    window.clearInterval(live.reasoningTimer);
    live.reasoningTimer = null;
  }
  if (live.currentText?.renderFrame) {
    window.cancelAnimationFrame(live.currentText.renderFrame);
    live.currentText.renderFrame = null;
  }
  for (const tool of live.tools?.values?.() || []) {
    if (tool.collapseTimer) window.clearTimeout(tool.collapseTimer);
    tool.collapseTimer = null;
    if (tool.outputRenderFrame) window.cancelAnimationFrame(tool.outputRenderFrame);
    tool.outputRenderFrame = null;
  }
}

export function ensureTimelineVisible() {
  elements.loadingState.hidden = true;
  elements.blockedState.hidden = true;
  elements.emptyState.hidden = true;
  elements.timeline.hidden = false;
}

export function ensureLiveUser(live, content) {
  if (!live || live.userRendered) return;
  // 离屏 live 不往当前时间线插用户消息;切回时落库回合会带上它。
  if (!liveViewed(live)) return;
  const text = String(content || live.userText || "");
  if (!text.trim() && !live.userAttachments.length) return;
  live.userText = text;
  ensureTimelineVisible();
  const message = createUserMessage(text, new Date(), {
    runId: live.runId,
    attachments: live.userAttachments
  });
  // 目标续轮等合成内容不画用户气泡(createUserMessage 返回 null),别的
  // 调用点都走 appendUserMessage 的空值兜底,这里以前直接 appendChild(null)
  // 抛 TypeError,把整段 live 装配掐断。
  if (message) {
    if (live.article?.isConnected) elements.timeline.insertBefore(message, live.article);
    else elements.timeline.appendChild(message);
  }
  live.userRendered = true;
  updateConversationChrome();
  contentAdded();
}

export function removeRunningStatus(turnId) {
  if (!turnId) return;
  const status = Array.from(elements.timeline.querySelectorAll("[data-turn-status]"))
    .find((node) => node.dataset.turnStatus === String(turnId));
  status?.remove();
}

/// 中断落定后在原位补一条「本轮已中断」状态行,取代整会话静默重拉。
/// 后端实测 cancel→run.cancelled 仅 ~12ms,之前那次 loadSessionView 把整条
/// 对话全量重渲染才是中断「不是秒停 / 感觉加载很久」的真因;这里只动这一条。
export function showInterruptedMarker(turnId, article) {
  const id = String(turnId || "");
  removeRunningStatus(turnId);
  const turn = (id && state.turns.find((item) => String(item?.id) === id)) || { id, status: "interrupted" };
  const line = createTurnStatus({ ...turn, status: "interrupted" });
  let anchor = article && article.isConnected ? article : null;
  if (!anchor && id) {
    const nodes = Array.from(elements.timeline.querySelectorAll(`[data-turn-id="${CSS.escape(id)}"]`));
    anchor = nodes.length ? nodes[nodes.length - 1] : null;
  }
  if (anchor?.parentNode) anchor.parentNode.insertBefore(line, anchor.nextSibling);
  else elements.timeline.appendChild(line);
}

export function commitRedoLive(live) {
  if (!live || live.operation !== "redo" || live.redoCommitted) return;
  live.redoCommitted = true;
  closeRevisionEditor();
  const stashKey = String(live.turnId || "");
  const previousStash = state.finishedTurnArticles.get(stashKey) || [];
  for (const entry of previousStash) {
    if (entry.kind === "final") entry.article?.remove();
  }
  const prefixSegments = previousStash.filter((entry) => entry.kind === "segment");
  if (prefixSegments.length) state.finishedTurnArticles.set(stashKey, prefixSegments);
  else state.finishedTurnArticles.delete(stashKey);
  for (const article of elements.timeline.querySelectorAll(".assistant-message")) {
    if (article.dataset.turnId === String(live.turnId || "") && article.dataset.segmentKind === "final") {
      article.remove();
    }
  }
  removeRunningStatus(live.turnId);
  if (live.inputId && live.editedContent != null) {
    const user = Array.from(elements.timeline.querySelectorAll(".user-message"))
      .find((article) => article.dataset.inputId === String(live.inputId));
    const paragraph = user?.querySelector(".user-bubble p");
    if (paragraph) paragraph.textContent = String(live.editedContent);
  }
  const turn = state.turns.find((item) => String(item?.id) === String(live.turnId));
  if (turn) {
    turn.status = "running";
    turn.assistant_content = "";
    turn.assistant_reasoning = null;
  }
  showTypingIndicator(live);
}

export function createTypingIndicator() {
  // AI 输出的「加载中」用编排点动效(用户拍板):三点走三角·顺时针→聚合→三角→
  // 逆时针→聚合→水平跳动,6s 循环。输入框那份仍是旧的匀速三点。
  const indicator = document.createElement("div");
  indicator.className = "gqy-run typing-run";
  indicator.setAttribute("aria-hidden", "true");
  const spin = document.createElement("span");
  spin.className = "mr-spin";
  for (const cls of ["mr1", "mr2", "mr3"]) {
    const dot = document.createElement("i");
    dot.className = cls;
    spin.appendChild(dot);
  }
  indicator.appendChild(spin);
  return indicator;
}

/* 运行指示器挪到了输入框那一排（`composerRunIndicator`）：气泡内那份只在
   「第一个块到达前」出现（`childElementCount > 0` 就直接 return），推理块或
   工具卡一出来就没了——而那两个阶段恰恰是最需要「它还在动」的时候。
   现在由回合状态统一驱动，见 `syncRunIndicator`。 */
export function showTypingIndicator(live) {
  if (!live || live.ended) return;
  ensureLiveArticle(live);
  syncRunIndicator();
  // 气泡里这份只管「还没开口」这一段:等待期给个落点,不然气泡是空的。
  // 整个回合期间的指示由输入框那排负责(推理、工具阶段它也在转)。
  if (live.typing || live.blocks.childElementCount > 0) return;
  const indicator = createTypingIndicator();
  live.blocks.appendChild(indicator);
  live.typing = indicator;
  contentAdded(live);
}

// 只要这个视图里有回合在跑就转，与是正文、推理还是工具无关。
export function syncRunIndicator() {
  const indicator = elements.composerRunIndicator;
  if (!indicator) return;
  indicator.hidden = !conversationRunning();
}

// 三点已挪到输入框那排，这里只保留 `is-streaming` 状态位（正文流式时的
// 样式还靠它），不再往气泡里塞节点、也不再做那段位移补间。
export function promoteTypingIndicator(live) {
  if (!live || live.ended) return;
  ensureLiveArticle(live);
  // 开口了就撤掉气泡里那份等待动画,它的语义只有「还没开口」。
  if (live.typing) {
    live.typing.remove();
    live.typing = null;
  }
  live.article.classList.add("is-streaming");
  syncRunIndicator();
}

export function clearTypingIndicator(live, { waitingOnly = false } = {}) {
  if (!live) return;
  // 气泡里那份是「还没开口」的占位，有任何内容落进来就撤。
  if (live.typing) {
    live.typing.remove();
    live.typing = null;
  }
  if (waitingOnly) {
    syncRunIndicator();
    return;
  }
  if (live.streamRail) live.streamRail.hidden = true;
  live.article?.classList.remove("is-streaming");
  syncRunIndicator();
}

/* 完成态保时序:live 渲染出的 article 按 turn 存档,重渲染时原样复用 */
export function stashLiveArticle(live, kind) {
  if (!live?.article) return;
  clearTypingIndicator(live);
  if (!live.turnId) return;
  if (!live.blocks || live.blocks.childElementCount === 0) return;
  live.article.classList.remove("live-assistant");
  live.article.dataset.segmentKind = kind;
  const key = String(live.turnId);
  const list = state.finishedTurnArticles.get(key) || [];
  // sessionId 随存:重建时的清理只能剪本会话的存档(离屏完成的轮要留到
  // 用户切回它的会话时复用)。
  list.push({ kind, article: live.article, sessionId: live.sessionId || "" });
  state.finishedTurnArticles.set(key, list);
}

export function updateLiveStopButton(live) {
  if (!live.stopButton) return;
  live.stopButton.disabled = live.ended || live.cancellationRequested;
  live.stopButton.title = live.cancellationRequested ? "正在停止" : "停止本条回复";
  live.stopButton.setAttribute("aria-label", live.stopButton.title);
}

export function removeLiveStopButton(live) {
  if (!live.stopButton) return;
  live.stopButton.remove();
  live.stopButton = null;
  elements.liveStopRail.hidden = elements.liveStopRail.childElementCount === 0;
}

export async function cancelLiveRun(live) {
  if (!live || live.ended || live.cancellationRequested) return;
  live.cancellationRequested = true;
  updateLiveStopButton(live);
  if (live.headerStatus) live.headerStatus.textContent = "正在停止";
  try {
    await apiRequest(`/api/runs/${encodeURIComponent(live.runId)}/cancel`, { method: "POST" });
  } catch (error) {
    live.cancellationRequested = false;
    updateLiveStopButton(live);
    if (live.headerStatus && !live.ended) live.headerStatus.textContent = "正在回复";
    showToast(error.message || "停止失败", "error");
    if ((error.status === 404 || error.status === 409) && state.viewSessionId) {
      await loadSessionView(state.viewSessionId, { quiet: true });
    }
  }
}

// 普通 Markdown 随内容收缩；只有需要稳定横向空间的结构撑满消息列。
// .image-gen-bubble 必须算宽块:纯生图回合没有其他宽内容,漏掉它气泡
// 会收缩成 fit-content,占位方块的 70% 宽随之塌成一丁点(08-25 实录)。
// 快递卡片挂在工具签外面(收起态也在),所以收起的工具签不算宽块时它仍要
// 自己算进来。地图卡片不在这里:它活在收起区里,展开态已经由
// `.tool-card:not(.collapsed)` 顶着,再写一条会让收起态的气泡也白撑宽。
export const WIDE_BLOCK_SELECTOR = ".markdown-body pre, .markdown-table-scroll, .conversation-media, .context-operation, img, .image-gen-bubble, .tool-card:not(.collapsed), .tool-live-progress:not([hidden]), .express-card";

export function syncBubbleWidth(article) {
  if (!article) return;
  const content = article.querySelector(".assistant-content");
  if (!content) return;
  content.classList.toggle("is-slim", !content.querySelector(WIDE_BLOCK_SELECTOR));
}

export function ensureLiveArticle(live) {
  if (live.article) return live.article;
  // 离屏 live 的气泡建成游离节点继续吃事件,切回时 reattach 挂载。
  const viewed = liveViewed(live);
  if (viewed) {
    ensureTimelineVisible();
    ensureLiveUser(live, live.userText);
    removeRunningStatus(live.turnId);
  }
  const article = document.createElement("article");
  article.className = "message assistant-message live-assistant";
  article.dataset.role = "assistant";
  article.dataset.runId = live.runId;
  if (live.turnId) article.dataset.turnId = String(live.turnId);
  const header = document.createElement("header");
  header.className = "assistant-label";
  const avatar = document.createElement("img");
  avatar.alt = "";
  avatar.setAttribute("aria-hidden", "true");
  setPersonaAvatar(avatar);
  const identity = document.createElement("div");
  const name = document.createElement("strong");
  name.textContent = state.persona.name;
  const status = document.createElement("span");
  status.className = "live-indicator";
  // 直播状态由三点弹跳/思考签表达,header 不再写「正在回复」;完成后写「刚刚」等
  status.textContent = "";
  identity.append(name, status);
  // Each running reply owns a compact stop control in its bubble corner.
  const stop = document.createElement("button");
  stop.type = "button";
  stop.className = "live-stop-button";
  stop.dataset.runId = live.runId;
  stop.appendChild(makeIconSlot("stop-square"));
  stop.addEventListener("click", () => cancelLiveRun(live));
  header.append(avatar, identity);
  if (viewed) {
    for (const existing of elements.liveStopRail.querySelectorAll(".live-stop-button")) {
      if (existing.dataset.runId === live.runId) existing.remove();
    }
    elements.liveStopRail.appendChild(stop);
    elements.liveStopRail.hidden = false;
  }
  const assistantContent = document.createElement("div");
  assistantContent.className = "assistant-content is-slim";
  const blocks = document.createElement("div");
  blocks.className = "assistant-blocks";
  assistantContent.appendChild(blocks);
  const bubble = document.createElement("div");
  bubble.className = "assistant-bubble";
  bubble.appendChild(assistantContent);
  const meta = document.createElement("div");
  meta.className = "assistant-meta";
  const endpoint = document.createElement("span");
  endpoint.className = "assistant-endpoint";
  endpoint.hidden = true;
  const metaText = document.createElement("span");
  metaText.textContent = "";
  const spacer = document.createElement("span");
  spacer.className = "meta-spacer";
  const copy = makeCopyButton(() => live.assistantText, "复制回复");
  copy.hidden = true;
  meta.append(endpoint, metaText, spacer, copy);
  const streamRail = document.createElement("div");
  streamRail.className = "assistant-stream-rail";
  streamRail.hidden = true;
  article.append(header, bubble, meta, streamRail);
  if (viewed) {
    elements.timeline.appendChild(article);
    pinQueuedMessages();
  }
  live.article = article;
  live.blocks = blocks;
  live.headerStatus = status;
  live.stopButton = stop;
  live.meta = metaText;
  live.endpoint = endpoint;
  live.copyButton = copy;
  live.streamRail = streamRail;
  updateLiveStopButton(live);
  contentAdded(live);
  return article;
}

export function breakLiveText(live) {
  live.currentText = null;
}
