import { asFiniteNumber, formatTokens, formatUsageMeta, generationSpeedValue } from "../../core/format.js";
import { createIcon, makeIconSlot } from "../../core/icons.js";
import { syncArtifactsFromTurns } from "../artifacts/model.js";
import { artifactChipOptions } from "../artifacts/workspace.js";
import { makeCopyButton, makeMessageAction, submitRedo } from "./actions.js";
import { liveClaimsTurn, updateConversationChrome } from "./chrome.js";
import { createConversationMedia } from "./media.js";
import { procLineAttach, procLineBreak, railSnapFit } from "./proc-rail.js";
import { reasoningHidden, reasoningPeekText, setReasoningPeek, splitReasoningText } from "./reasoning.js";
import { armProgrammaticScroll, isNearBottom } from "./scroll.js";
import { appendUserMessage } from "./user.js";
import { loadGoal, loadStageTodos } from "../goal.js";
import { refreshComposerCumulative } from "../live/run.js";
import { WIDE_BLOCK_SELECTOR, reattachLiveArticles } from "../live/state.js";
import { renderMarkdown } from "../markdown/render.js";
import { makeAvatarFrame } from "../persona.js";
import { clearQuestionDock } from "../questions.js";
import { setComposerUsage } from "../status.js";
import { createPersistedToolCard } from "../tools/cards.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

// 后台子代理的子过程流:一个 job 一份,持久存在 state.jobStreamSinks 里
// (任务条整条重建时面板 DOM 也不丢),点开对应任务条那行时挂到它下面。
export function jobStreamSink(jobId) {
  let sink = state.jobStreamSinks.get(jobId);
  if (!sink) {
    const panel = document.createElement("div");
    panel.className = "job-stream-panel";
    const blocks = document.createElement("div");
    blocks.className = "sub-blocks assistant-blocks";
    panel.appendChild(blocks);
    // taskPeek / taskToken 由 renderJobsStrip 每次重建时挂到当前那行的窥视/
    // token 元素上(09-12 用户要回行窥视:跑到工具显示工具、跑到思考窥思考;
    // token 每步更新)。标题本身仍保持完整、不被窥视替换。
    sink = { panel, blocks, taskPeek: null, taskToken: null, tokenText: "", think: null, thinkAccum: "", pendingCall: null, peekLine: "", usageKey: "job:" + jobId };
    state.jobStreamSinks.set(jobId, sink);
  }
  return sink;
}

export function createReasoningBlock(text, title = "已思考", live = false, summaryOnly = false) {
  const details = document.createElement("details");
  details.className = "reasoning-block";
  details.classList.toggle("is-summary", summaryOnly);
  details.classList.toggle("is-live", live);
  details.open = state.reasoningExpanded === true;
  const summary = document.createElement("summary");
  const atom = makeIconSlot("atom", "reasoning-icon");
  if (live) for (let index = 0; index < 3; index += 1) atom.appendChild(document.createElement("i"));
  const titleNode = document.createElement("span");
  titleNode.className = "reasoning-title";
  titleNode.textContent = title || (live ? "正在思考" : "已思考");
  const chevron = makeIconSlot("chevron-right", "reasoning-chevron");
  summary.append(atom, titleNode);
  let liveStatus = null;
  let progress = null;
  if (live) {
    liveStatus = document.createElement("span");
    liveStatus.className = "reasoning-live-status";
    liveStatus.textContent = "0s";
    summary.appendChild(liveStatus);
    progress = document.createElement("div");
    progress.className = "reasoning-progress";
    progress.setAttribute("role", "progressbar");
    progress.setAttribute("aria-label", "思考进度");
    progress.setAttribute("aria-valuetext", "正在思考");
    const progressFill = document.createElement("i");
    progressFill.setAttribute("aria-hidden", "true");
    progress.appendChild(progressFill);
  }
  // 思考内容收着的时候,标题行右边那片空白放思考的尾巴:正在想就跟着滚,想完了
  // 也留着(回看那份同样有),展开时才让位。尾部对齐,新字从右边推进来,旧字从左边淡出。
  const slot = document.createElement("span");
  slot.className = "reasoning-peek";
  const peek = document.createElement("span");
  slot.appendChild(peek);
  summary.appendChild(slot);
  // 此时还没挂进文档量不到宽度;先写字,挂上后由 fit/下一次 delta 再量
  peek.textContent = reasoningPeekText(text);
  window.requestAnimationFrame(() => setReasoningPeek(peek, text));
  summary.appendChild(chevron);
  const body = document.createElement("div");
  body.className = "reasoning-text";
  body.textContent = String(text || "");
  details.append(summary);
  if (progress) details.appendChild(progress);
  details.appendChild(body);
  const block = {
    element: details,
    title: titleNode,
    liveStatus,
    progress,
    body,
    peek,
    raw: String(text || ""),
    pendingTitle: "",
    summaryOnly,
    partOpen: false,
    startedAt: live ? performance.now() : null,
    finished: !live,
    userToggled: false,
    ignoreNextToggle: false
  };
  details.addEventListener("toggle", () => {
    if (block.ignoreNextToggle) {
      block.ignoreNextToggle = false;
      return;
    }
    block.userToggled = true;
    railSnapFit(details);
  });
  return block;
}

export function createAssistantMessage({
  content = "",
  reasoning = "",
  reasoningTitle = "已思考",
  // 工具轮次（持久化回合用）。实时那份由事件流按到达顺序往 blocks 里插，
  // 推理、正文、工具卡是交错的；这里从 turn.tool_flow 重建同样的顺序。
  toolRounds = [],
  // 已回答的问题(#5b):按时序落在它对应的 ask_question 工具位上,不再被整
  // 堆到助手消息之前(刷新后问答卡跑到正文前面就是这么来的)。ask_question 在
  // tool_flow 里就是一个调用,这里遇到它就用第 N 个 exchange 顶替那张裸工具卡。
  questionExchanges = [],
  assets = [],
  // 这一轮产出的 artifact,画成气泡底部的 chip(artifactchips.js)。
  artifacts = [],
  timestamp = null,
  tokenTotal = 0,
  tokenPrompt = 0,
  tokenCached = 0,
  tokenEstimated = false,
  // 刷新后也要有的「累计」与「每秒」(09-11):累计由 renderConversation 按顺序算好
  cumulative = null,
  generationTokens = 0,
  generationMs = 0,
  providerId = "",
  model = "",
  activeContext = true,
  turnId = null,
  muted = false,
  segmentKind = "final",
  redoTarget = null
} = {}) {
  const article = document.createElement("article");
  article.className = `message assistant-message${muted ? " is-muted" : ""}`;
  article.dataset.role = "assistant";
  if (turnId) article.dataset.turnId = turnId;
  article.dataset.segmentKind = segmentKind;
  const header = document.createElement("header");
  header.className = "assistant-label";
  const avatar = makeAvatarFrame("her");
  const identity = document.createElement("div");
  const name = document.createElement("strong");
  name.textContent = state.persona.name;
  identity.append(name);
  header.append(avatar, identity);
  const assistantContent = document.createElement("div");
  assistantContent.className = "assistant-content";
  const blocks = document.createElement("div");
  blocks.className = "assistant-blocks";
  // 逐轮重建:每一轮是「思考 → 正文 → 这轮调的工具」,轮次之间按顺序排,
  // 最后才是本回合的最终思考与回答。把所有工具堆到最前面是错的——那样
  // 一个十轮的回合会先甩出二十个工具卡,中间说了什么全看不见了。
  //
  // 卡片必须挂在 blocks 里:样式表是 `.assistant-blocks > .tool-card`,
  // 挂在外面选择器不命中,会退化成一行裸文本。
  const exchangeQueue = Array.isArray(questionExchanges) ? [...questionExchanges] : [];
  for (const round of Array.isArray(toolRounds) ? toolRounds : []) {
    const roundReasoning = String(round?.assistant_reasoning || "");
    if (roundReasoning.trim() && !reasoningHidden()) {
      const parsed = splitReasoningText(roundReasoning);
      procLineAttach(blocks, createReasoningBlock(parsed.body, "已思考", false).element, true);
    }
    const roundContent = String(round?.assistant_content || "");
    if (roundContent.trim()) {
      const markdown = document.createElement("div");
      markdown.className = "markdown-body";
      renderMarkdown(markdown, roundContent);
      procLineBreak(blocks);
      blocks.appendChild(markdown);
    }
    for (const call of Array.isArray(round?.calls) ? round.calls : []) {
      // ask_question 这一步:用它对应的已回答卡顶替裸工具卡,落在原时序位。
      if (String(call?.name || "") === "ask_question" && exchangeQueue.length) {
        procLineBreak(blocks);
        blocks.appendChild(createAnsweredQuestionCard(exchangeQueue.shift()));
        continue;
      }
      procLineAttach(blocks, createPersistedToolCard(call), true);
      // share_file 的富预览(播放器/图片/下载条)重建:实时靠 tool.finished
      // 的输出渲染,刷新/切换后从落库的 tool_flow 输出里复原同一份。
      if (window.GqyShared?.isShareTool(String(call?.name || ""))) {
        const shared = window.GqyShared.renderCard(String(call?.output || ""));
        if (shared) {
          procLineBreak(blocks);
          blocks.appendChild(shared);
        }
      }
    }
  }
  if (String(reasoning || "").trim() && !reasoningHidden()) {
    const parsed = splitReasoningText(reasoning);
    procLineAttach(blocks, createReasoningBlock(parsed.body, "已思考", false).element, true);
  }
  if (String(content || "").trim()) {
    const markdown = document.createElement("div");
    markdown.className = "markdown-body";
    renderMarkdown(markdown, content);
    procLineBreak(blocks);
    blocks.appendChild(markdown);
  }
  // tool_flow 里没找到对应 ask_question 调用的已回答卡(边角情形)兜底补在末尾,
  // 总比丢掉强;正常情形上面已按位插完,这里为空。
  for (const exchange of exchangeQueue) {
    procLineBreak(blocks);
    blocks.appendChild(createAnsweredQuestionCard(exchange));
  }
  for (const asset of Array.isArray(assets) ? assets : []) {
    procLineBreak(blocks);
    blocks.appendChild(createConversationMedia(asset));
  }
  // 回合以工具收尾(没有最终正文)时,最后那条时间线也要切断,否则总结行永远不出
  procLineBreak(blocks);
  assistantContent.appendChild(blocks);
  assistantContent.classList.toggle("is-slim", !blocks.querySelector(WIDE_BLOCK_SELECTOR));
  window.GqyArtifactChips?.sync(assistantContent, artifacts, artifactChipOptions());
  article.append(header, assistantContent);

  const meta = document.createElement("div");
  meta.className = "assistant-meta";
  if (state.display?.show_mixed_model_endpoint && (String(providerId || "").trim() || String(model || "").trim())) {
    const endpoint = document.createElement("span");
    endpoint.className = "assistant-endpoint";
    endpoint.textContent = [providerId, model].map((value) => String(value || "").trim()).filter(Boolean).join(" / ");
    meta.appendChild(endpoint);
  }
  // 刷新后的回合也带「累计」与「每秒」:累计按会话里到这一轮为止的顺序求和(与
  // run.completed 事件里 daemon 算的口径一致),速度用落库的样本。
  const usageText = formatUsageMeta({
    turnTotal: tokenTotal,
    turnPrompt: tokenPrompt,
    turnCached: tokenCached,
    estimated: tokenEstimated,
    cumulative: cumulative?.total,
    cumulativePrompt: cumulative?.prompt,
    cumulativeCached: cumulative?.cached,
    generationTokens: generationTokens,
    generationMs: generationMs
  });
  if (usageText) {
    const token = document.createElement("span");
    token.textContent = usageText;
    meta.appendChild(token);
  }
  if (!activeContext) {
    const contextBadge = document.createElement("span");
    contextBadge.className = "context-state-badge";
    contextBadge.textContent = "已移出当前上下文";
    meta.appendChild(contextBadge);
  }
  const copyValue = String(content || "").trim() || String(reasoning || "");
  if (copyValue || redoTarget) {
    const spacer = document.createElement("span");
    spacer.className = "meta-spacer";
    meta.appendChild(spacer);
    if (redoTarget) {
      const redo = makeMessageAction("refresh-cw", "重新生成回复", () => submitRedo(redoTarget));
      redo.className = "redo-action";
      meta.appendChild(redo);
    }
    if (copyValue) meta.appendChild(makeCopyButton(copyValue, "复制回复"));
  }
  if (meta.childNodes.length) article.appendChild(meta);
  return article;
}

export function setAssistantRedoAction(article, candidate) {
  const meta = article?.querySelector(".assistant-meta");
  if (!meta) return;
  meta.querySelector(".redo-action")?.remove();
  if (!candidate) return;
  const redo = makeMessageAction("refresh-cw", "重新生成回复", () => submitRedo(candidate));
  redo.className = "redo-action";
  const copy = meta.querySelector("button:last-child");
  if (copy) meta.insertBefore(redo, copy);
  else meta.appendChild(redo);
}

export function createAnsweredQuestionCard(exchange, compact = true) {
  const card = document.createElement("section");
  card.className = "answered-question-card";
  if (compact) card.classList.add("is-compact");
  const header = document.createElement("header");
  // 去掉左边那个大对钩(#143 用户嫌大):「已回答」二字已经表达状态了。
  const copy = document.createElement("div");
  const status = document.createElement("small");
  status.textContent = "已回答";
  const title = document.createElement("strong");
  const questions = Array.isArray(exchange?.questions) ? exchange.questions : [];
  title.textContent = questions.length === 1 ? String(questions[0]?.header || "补充确认") : `${questions.length} 项补充确认`;
  copy.append(status, title);
  header.append(copy);
  const list = document.createElement("dl");
  list.className = "answered-question-list";
  const answers = Array.isArray(exchange?.answers) ? exchange.answers : [];
  questions.forEach((question, index) => {
    const row = document.createElement("div");
    const term = document.createElement("dt");
    term.textContent = String(question?.question || question?.header || `问题 ${index + 1}`);
    const description = document.createElement("dd");
    const selected = Array.isArray(answers[index]) ? answers[index] : [];
    description.textContent = selected.map(String).join("、") || "未记录";
    row.append(term, description);
    list.appendChild(row);
  });
  card.append(header, list);
  return card;
}

export function createPersistedQuestion(exchange, turnId) {
  const wrapper = document.createElement("article");
  wrapper.className = "persisted-question-wrap";
  if (turnId) wrapper.dataset.turnId = turnId;
  wrapper.appendChild(createAnsweredQuestionCard(exchange));
  return wrapper;
}

export function createTurnStatus(turn) {
  const status = document.createElement("div");
  status.className = "turn-status-line";
  status.dataset.turnStatus = String(turn?.id || "");
  // 也标上 turn-id：命令回执按「锚点回合的最后一个 [data-turn-id] 节点」
  // 插入，不标的话回执会插在这条状态行**之前**，时间顺序看着是乱的。
  if (turn?.id) status.dataset.turnId = String(turn.id);
  const isInterrupted = turn?.status === "interrupted";
  status.classList.toggle("is-interrupted", isInterrupted);
  status.appendChild(makeIconSlot(isInterrupted ? "circle-alert" : "loader-circle"));
  const text = document.createElement("span");
  text.textContent = isInterrupted ? "本轮已中断" : "本轮正在运行";
  status.appendChild(text);
  if (asFiniteNumber(turn?.token_total) > 0) {
    const usage = document.createElement("span");
    usage.textContent = `${turn.token_usage_estimated ? "约 " : ""}${formatTokens(turn.token_total)} tokens`;
    status.appendChild(usage);
  }
  if (turn?.active_context === false) {
    const context = document.createElement("span");
    context.className = "context-state-badge";
    context.textContent = "已移出当前上下文";
    status.appendChild(context);
  }
  return status;
}

export function renderPersistedTurn(turn) {
  const turnId = String(turn?.id || "");
  const candidate = state.redoCandidate && String(state.redoCandidate.turn_id) === turnId
    ? state.redoCandidate
    : null;
  appendUserMessage(elements.timeline, turn?.user_content || "", turn?.user_timestamp, {
    turnId,
    inputId: turnId,
    revisionTarget: candidate && String(candidate.input_id) === turnId ? candidate : null,
    attachments: turn?.attachments
  });

  /*
   * 本页会话内完成的 turn:优先复用 live 流式渲染出的 article(含按时序排列的
   * 思考签 / 工具签 / 正文块),避免用扁平的「单 reasoning + 正文」重建而丢失时序。
   * 历史重载(后端快照没有 parts 顺序)才退回扁平重建。
   */
  const stash = turnId && turn?.status !== "running" ? state.finishedTurnArticles.get(turnId) : null;
  const claimed = turn?.status === "running" && liveClaimsTurn(turnId);
  let stashIndex = 0;
  const takeStash = (kind) => {
    if (!stash || stashIndex >= stash.length || stash[stashIndex].kind !== kind) return null;
    return stash[stashIndex++].article;
  };

  // 已回答的问题卡:live 存档里原位保留;快照重建时**不再**整堆甩在助手消息
  // 之前(#5b:刷新后问答卡跑到正文前面),而是交给下面的 createAssistantMessage
  // 按 ask_question 的时序位插进 blocks。只有在没有最终助手块可挂时才在这里兜底。
  const persistedExchanges = (!stash && !claimed && Array.isArray(turn?.question_exchanges))
    ? turn.question_exchanges
    : [];

  const followups = Array.isArray(turn?.followups) ? turn.followups : [];
  for (const followup of followups) {
    const precedingContent = String(followup?.preceding_assistant_content || "");
    const precedingReasoning = String(followup?.preceding_assistant_reasoning || "");
    const stashedSegment = takeStash("segment");
    if (stashedSegment) {
      elements.timeline.appendChild(stashedSegment);
    } else if (!claimed && (precedingContent.trim() || precedingReasoning.trim())) {
      elements.timeline.appendChild(createAssistantMessage({
        content: precedingContent,
        reasoning: precedingReasoning,
        providerId: followup?.provider_id,
        model: followup?.model,
        timestamp: followup?.submitted_at,
        turnId,
        segmentKind: "segment",
        activeContext: turn?.active_context !== false
      }));
    }
    appendUserMessage(elements.timeline, followup?.content || "", followup?.submitted_at, {
      turnId,
      followupId: String(followup?.id || ""),
      inputId: String(followup?.id || ""),
      revisionTarget: candidate && String(candidate.input_id) === String(followup?.id || "") ? candidate : null,
      attachments: followup?.attachments
    });
  }
  let leftoverSegment;
  while ((leftoverSegment = takeStash("segment"))) elements.timeline.appendChild(leftoverSegment);

  // 这一轮调过的工具。`stash` 存在说明刚在本端实时渲染过，实时卡片还在
  // 原位，不要再画一遍。卡片要交给助手消息放进它的 `assistant-blocks`
  // 里——挂在外面样式选择器不命中，会退化成一行裸文本。
  const persistedToolRounds = stash
    ? []
    : (Array.isArray(turn?.tool_flow) ? turn.tool_flow : []);

  const assistantContent = String(turn?.assistant_content || "");
  const assistantReasoning = String(turn?.assistant_reasoning || "");
  const assets = turn?.status === "running" ? [] : (Array.isArray(turn?.assets) ? turn.assets : []);
  const artifacts = turn?.status === "running" ? [] : (Array.isArray(turn?.artifacts) ? turn.artifacts : []);
  const stashedFinal = takeStash("final");
  if (stashedFinal) {
    stashedFinal.classList.toggle("is-muted", turn?.active_context === false);
    stashedFinal.dataset.segmentKind = "final";
    setAssistantRedoAction(stashedFinal, candidate);
    elements.timeline.appendChild(stashedFinal);
  } else if (
    !claimed
    && (assistantContent.trim()
      || assistantReasoning.trim()
      || assets.length
      || artifacts.length
      || persistedToolRounds.length
      || persistedExchanges.length)
  ) {
    elements.timeline.appendChild(createAssistantMessage({
      content: assistantContent,
      reasoning: assistantReasoning,
      toolRounds: persistedToolRounds,
      questionExchanges: persistedExchanges,
      providerId: turn?.provider_id,
      model: turn?.model,
      assets,
      artifacts,
      timestamp: turn?.assistant_timestamp,
      tokenTotal: turn?.token_total,
      tokenPrompt: turn?.token_prompt,
      tokenCached: turn?.token_cache_read,
      tokenEstimated: Boolean(turn?.token_usage_estimated),
      cumulative: state.cumulativeByTurn?.get(turnId) || null,
      generationTokens: turn?.generation_tokens,
      generationMs: turn?.generation_ms,
      activeContext: turn?.active_context !== false,
      turnId,
      segmentKind: "final",
      redoTarget: candidate,
      muted: turn?.active_context === false
    }));
  }
  if ((turn?.status === "running" && !claimed) || turn?.status === "interrupted") elements.timeline.appendChild(createTurnStatus(turn));
  else if (!stashedFinal && !assistantContent.trim() && !assistantReasoning.trim() && (asFiniteNumber(turn?.token_total) > 0 || turn?.active_context === false)) {
    const metadata = createTurnStatus({ ...turn, status: "completed" });
    metadata.querySelector("span:nth-child(2)").textContent = "本轮已完成";
    metadata.querySelector(".icon-slot").replaceChildren(createIcon("check"));
    elements.timeline.appendChild(metadata);
  }
}

export function renderConversation({ forceScroll = false } = {}) {
  elements.loadingState.hidden = true;
  elements.blockedState.hidden = true;
  clearQuestionDock();
  // 每条回合的「累计」=会话里到它为止的顺序求和(与 run.completed 里 daemon 报的口径一致)
  state.cumulativeByTurn = new Map();
  {
    let total = 0;
    let prompt = 0;
    let cached = 0;
    for (const turn of state.turns) {
      total += asFiniteNumber(turn?.token_total);
      prompt += asFiniteNumber(turn?.token_prompt);
      cached += asFiniteNumber(turn?.token_cache_read);
      state.cumulativeByTurn.set(String(turn?.id || ""), { total, prompt, cached });
    }
  }
  // 刷新/切会话后,输入框下方信息行按最后一轮回填(速度 + 累计),不然刷新就空了(#99)。
  {
    const lastTurn = state.turns[state.turns.length - 1];
    const lastCum = lastTurn ? state.cumulativeByTurn.get(String(lastTurn.id || "")) : null;
    // 回填给「累计」定基线,重连后若正跑子代理,refreshComposerCumulative 有基线可加。
    // 但**只在没有基线、或候选更高时才用它**:按落库回合求和会漏算子代理子会话,每秒
    // 轮询若照它下调,会把实时事件维护的、含子代理的权威累计压低——后台子代理跑完后
    // 累计瞬间掉一大块正是这么来的(#131)。可信度更高的 bootstrap 会话累计(含子代理)
    // 也纳入比较,取最高的当基线。/reset 等清零场景由 conversation.* 事件另行清基线。
    const cand = lastCum && lastCum.total > 0
      ? { total: lastCum.total, prompt: lastCum.prompt, cached: lastCum.cached }
      : null;
    const ctxTotal = asFiniteNumber(state.context?.cumulative_tokens);
    const ctx = ctxTotal > 0
      ? { total: ctxTotal, prompt: asFiniteNumber(state.context?.cumulative_prompt_tokens), cached: asFiniteNumber(state.context?.cumulative_cache_read_tokens) }
      : null;
    const best = [state.cumulativeBase, ctx, cand]
      .filter((c) => c && c.total > 0)
      .reduce((a, b) => (!a || b.total > a.total ? b : a), null);
    state.cumulativeBase = best;
    setComposerUsage({
      speed: lastTurn ? generationSpeedValue(lastTurn.generation_tokens, lastTurn.generation_ms) : null,
    });
    refreshComposerCumulative();
  }
  // 回合运行期间每秒轮询都可能整段重建（refreshViewSnapshot）。用户正往回
  // 翻历史时不能每秒被拽回底部：只有明确导航（换会话/启动）或用户本来就
  // 跟着输出走时才滚到底，否则原地恢复滚动位置。
  const keepScroll = !forceScroll && !state.followOutput;
  const previousScrollTop = elements.chatScroll.scrollTop;
  // replaceChildren 让 scrollHeight 瞬间塌掉,浏览器把 scrollTop 钳到 0 并派发
  // 一条 scroll 事件;这条事件先于下面的 rAF 到达监听器。不守卫的话监听器
  // 把「跳到顶」当成用户上滚,关掉跟随——后台任务完成的通知落库触发整段
  // 重建时就是这么把自动滚动弄丢的(之后 AI 继续输出也不再往下走)。
  armProgrammaticScroll();
  elements.timeline.replaceChildren();
  const turns = [...state.turns].sort((left, right) => asFiniteNumber(left?.seq) - asFiniteNumber(right?.seq));
  state.turns = turns;
  syncArtifactsFromTurns(turns);
  loadStageTodos(state.viewSessionId);
  loadGoal(state.viewSessionId);
  if (state.finishedTurnArticles.size) {
    const knownTurnIds = new Set(turns.map((turn) => String(turn?.id)));
    for (const [key, list] of [...state.finishedTurnArticles.entries()]) {
      // 别的会话离屏完成的存档不在本会话的 turns 里,不能因此被剪掉。
      const foreign = list.some((entry) => entry.sessionId && String(entry.sessionId) !== String(state.viewSessionId || ""));
      if (!foreign && !knownTurnIds.has(key)) state.finishedTurnArticles.delete(key);
    }
  }
  if (turns.length === 0) {
    elements.timeline.hidden = true;
    elements.emptyState.hidden = false;
  } else {
    elements.emptyState.hidden = true;
    elements.timeline.hidden = false;
    // 不再插日期分隔条：它在回执/流式气泡之间来回跳位置，信息量又低
    // （悬停消息时间戳就有完整日期）。
    for (const turn of turns) renderPersistedTurn(turn);
  }
  // 命令回执不是回合，不在 state.turns 里；timeline 每次重建都要补回来。
  window.GqyCommands?.renderNotices(elements.timeline, state.viewSessionId);
  reattachLiveArticles();
  // 落盘回合数为 0 不等于屏幕上没内容：回执和正在流式输出的气泡都不在
  // state.turns 里。只按 turns 判空的话，运行中一次重绘就把画面整个换成
  // 欢迎页，气泡瞬间蒸发。
  if (elements.timeline.childElementCount > 0) {
    elements.emptyState.hidden = true;
    elements.timeline.hidden = false;
  }
  if (keepScroll) {
    // 同步恢复（不等下一帧），重建就不会闪一下再跳回来。上方内容高度
    // 变化仍可能让视口偏移，先接受这个近似。
    armProgrammaticScroll();
    elements.chatScroll.scrollTop = previousScrollTop;
    state.nearBottom = isNearBottom();
    elements.jumpBottomButton.hidden = false;
  } else {
    state.nearBottom = true;
    state.followOutput = true;
    elements.jumpBottomButton.hidden = true;
    // 先同步钉到底:replaceChildren 之后 scrollTop 被钳成 0,只等下一帧再滚
    // 的话会画出一帧顶部——手机上每轮结束整段重建都闪一下(09-10 沙盒实测
    // 采样到 scrollTop 291→0→291)。rAF 那次是布局稳定后的最终校正。
    armProgrammaticScroll();
    elements.chatScroll.scrollTop = elements.chatScroll.scrollHeight;
    window.requestAnimationFrame(() => {
      armProgrammaticScroll();
      elements.chatScroll.scrollTop = elements.chatScroll.scrollHeight;
      // 重建前后都可能有 scroll 事件进监听器,跟随位在这里再钉一次。
      state.followOutput = true;
      state.nearBottom = true;
      elements.jumpBottomButton.hidden = true;
    });
  }
  updateConversationChrome();
}
