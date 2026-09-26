import { apiRequest } from "../../core/api.js";
import { asFiniteNumber, cacheSuffix, effectiveUsageTotal, formatTokens, formatUsageMeta, generationSpeedValue } from "../../core/format.js";
import { makeIconSlot } from "../../core/icons.js";
import { showToast } from "../../core/toast.js";
import { focusComposerIfDesktop, updateControlState } from "../composer/input.js";
import { consoleIsOpen } from "../console/panel.js";
import { liveViewed, updateConversationChrome } from "../conversation/chrome.js";
import { procLineBreak } from "../conversation/proc-rail.js";
import { contentAdded, scrollToBottom } from "../conversation/scroll.js";
import { appendUserMessage } from "../conversation/user.js";
import { refreshViewSnapshot } from "./sse.js";
import { breakLiveText, clearTypingIndicator, disposeLiveState, ensureLiveArticle, ensureLiveUser, removeLiveStopButton, renderQueueTray, showInterruptedMarker, showTypingIndicator, stashLiveArticle, syncBubbleWidth } from "./state.js";
import { finalizeLiveReasoning, rerenderLiveHtmlFences } from "./stream.js";
import { endPendingQuestions } from "../questions.js";
import { renderSessionList } from "../sessions/list.js";
import { trackRun } from "../sessions/runs.js";
import { createLiveForRun, loadSessionView } from "../sessions/view.js";
import { setComposerUsage, updateContext, updateRuntimeUsage } from "../status.js";
import { updateToolStatus, updateToolSummary } from "../tools/cards.js";
import { clearPreparingTool } from "../tools/events.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

export function appendRunNotice(live, message, error = false) {
  ensureLiveArticle(live);
  clearTypingIndicator(live);
  breakLiveText(live);
  const notice = document.createElement("div");
  notice.className = `run-notice${error ? " is-error" : ""}`;
  notice.append(makeIconSlot(error ? "circle-alert" : "circle-stop"));
  const text = document.createElement("span");
  text.textContent = String(message || "");
  notice.appendChild(text);
  procLineBreak(live.blocks);
  live.blocks.appendChild(notice);
  return notice;
}

/// 失败原因的中文说法。键是 `run.failed` 事件里服务端给的 failure_kind
/// （见 `llm/openai_compatible/errors.rs` 的 `classify_failure`）；认不出的键
/// 保留服务端原文——别把未知失败说成已知的。
const FAILURE_REASONS = {
  content_policy: "上游内容策略拦下了这条内容，没法接着往下写",
  rate_limit: "上游限流了（429），过一会儿再试",
  authentication: "上游登录态失效了（401/403），得重新登录",
  transport_timeout: "上游卡住一直没有输出，已被终止",
  transport_connect: "连不上上游",
  transport_request: "上游连接中断",
  endpoint_unavailable: "这个模型暂时不可用",
  endpoint_incompatible: "上游不接受这条请求",
  invalid_request: "上游认为请求有问题",
  status: "上游返回了错误",
};

function failureNoticeText(data) {
  const kind = String(data?.failure_kind || "");
  return FAILURE_REASONS[kind] || String(data?.message || "本轮运行失败");
}

/// 半截失败后的「继续」：给同一会话发一条短请求，让模型接着把没说完的写完。
/// 发的是普通用户消息（时间线上看得见），不做隐藏合成轮——用户按了什么、她
/// 收到了什么，两边对得上，出问题也好查。
const CONTINUE_PROMPT = "继续把上面没说完的说完，不要重复已经写过的内容。";

function appendContinueAction(live, notice) {
  if (!notice) return;
  const button = document.createElement("button");
  button.type = "button";
  button.className = "run-notice-action";
  button.textContent = "继续";
  button.addEventListener("click", async () => {
    if (button.disabled) return;
    button.disabled = true;
    try {
      await startContinueRun(live.sessionId || state.viewSessionId, CONTINUE_PROMPT);
    } catch (error) {
      button.disabled = false;
      // 409 = 会话里刚起了新的一轮;别把调度问题说成用户该重来一遍。
      showToast(
        error?.status === 409 ? "会话里刚开了新的一轮，稍等一下再点继续" : error?.message || "继续失败",
        "error"
      );
    }
  });
  notice.appendChild(button);
}

async function startContinueRun(sessionId, content) {
  const body = { content };
  if (sessionId) body.session_id = sessionId;
  const response = await apiRequest("/api/turns", { method: "POST", body: JSON.stringify(body) });
  const payload = await response.json();
  const queuedPrompt = payload?.queued ? payload.prompt : null;
  if (queuedPrompt) {
    if (!state.queuedPrompts.some((prompt) => String(prompt?.id) === String(queuedPrompt?.id))) {
      state.queuedPrompts.push(queuedPrompt);
    }
    renderQueueTray();
    return;
  }
  const runId = String(payload?.run_id || "");
  if (!runId) throw new Error("服务未返回运行标识");
  if (sessionId) trackRun(sessionId, runId);
  const live = createLiveForRun(runId, content);
  live.userText = content;
  ensureLiveUser(live, content);
  showTypingIndicator(live);
  scrollToBottom({ force: true, smooth: true });
  updateRuntimeUsage();
  updateConversationChrome();
  renderSessionList();
}

export function markUnfinishedTools(live) {
  for (const tool of live.tools.values()) {
    if (tool.finished) continue;
    tool.finished = true;
    tool.finishedAt = performance.now();
    updateToolStatus(tool, "已中断", "circle-alert", "is-failure");
    updateToolSummary(tool);
    if (tool.liveProgress) {
      if (tool.liveProgress.textContent.trim()) tool.liveProgress.classList.add("is-error");
      else tool.liveProgress.hidden = true;
      tool.progressDetail.wrapper.hidden = !tool.progressDetail.raw;
      syncBubbleWidth(live.article);
    }
    if (!state.toolExpanded) {
      tool.card.classList.add("collapsed");
      tool.head.setAttribute("aria-expanded", "false");
    }
  }
}

export function setLiveEndpoint(live, providerId, model) {
  const values = [providerId, model].map((value) => String(value || "").trim()).filter(Boolean);
  live.providerId = String(providerId || "");
  live.model = String(model || "");
  if (!live.endpoint) return;
  live.endpoint.textContent = values.join(" / ");
  live.endpoint.hidden = !state.display?.show_mixed_model_endpoint || values.length === 0;
}

export function consumeLiveQueue(live, data) {
  finalizeLiveReasoning(live);
  procLineBreak(live.blocks);
  setLiveEndpoint(live, data?.provider_id, data?.model);
  if (live.headerStatus) live.headerStatus.textContent = "";
  // followup 插在步与步之间时,前一段末尾不再打「已完成」那条带背景的小字
  // (09-12 用户报没必要):中间段没有独立用量可报,留空并隐藏那行。
  if (live.meta) {
    live.meta.textContent = "";
    live.meta.hidden = true;
  }

  const ids = new Set((Array.isArray(data?.prompt_ids) ? data.prompt_ids : []).map(String));
  const consumed = state.queuedPrompts.filter((prompt) => ids.has(String(prompt?.id)));
  state.queuedPrompts = state.queuedPrompts.filter((prompt) => !ids.has(String(prompt?.id)));
  for (const prompt of consumed) {
    appendUserMessage(elements.timeline, prompt?.content || "", prompt?.submitted_at || new Date(), {
      turnId: live.turnId,
      runId: live.runId,
      followupId: prompt?.id,
      attachments: prompt?.attachments
    });
  }
  renderQueueTray();

  stashLiveArticle(live, "segment");
  removeLiveStopButton(live);
  live.article = null;
  live.blocks = null;
  live.headerStatus = null;
  live.meta = null;
  live.endpoint = null;
  live.copyButton = null;
  live.streamRail = null;
  live.typingAnimation = null;
  live.currentText = null;
  live.assistantText = "";
  live.assistantReasoning = "";
  live.reasoning = null;
  live.reasoningParts = [];
  live.reasoningStarted = false;
  live.reasoningTitle = "";
  live.tools = new Map();
  live.questions = new Map();
  live.contextOperation = null;
  showTypingIndicator(live);
  contentAdded(live);
}

export function updateLocalTurnFromLive(live, terminalStatus, data) {
  const status = terminalStatus === "completed" ? "completed" : "interrupted";
  let turn = live.turnId ? state.turns.find((item) => String(item?.id) === String(live.turnId)) : null;
  if (!turn && (live.userText || live.userAttachments.length)) {
    turn = {
      id: live.turnId || `local-${live.runId}`,
      seq: state.turns.length ? Math.max(...state.turns.map((item) => asFiniteNumber(item?.seq))) + 1 : 1,
      status,
      active_context: true,
      user_content: live.userText,
      assistant_content: live.assistantText,
      assistant_reasoning: live.assistantReasoning || null,
      provider_id: data?.provider_id || live.providerId || null,
      model: data?.model || live.model || null,
      user_timestamp: new Date().toISOString(),
      assistant_timestamp: new Date().toISOString(),
      token_total: effectiveUsageTotal(data?.usage),
      token_usage_estimated: Boolean(data?.usage_estimated),
      question_exchanges: [],
      followups: [],
      assets: [...live.assets],
      artifacts: [...live.artifacts],
      attachments: [...live.userAttachments]
    };
    state.turns.push(turn);
  } else if (turn) {
    turn.status = status;
    if (live.assistantText.trim()) turn.assistant_content = live.assistantText;
    if (live.assistantReasoning.trim()) turn.assistant_reasoning = live.assistantReasoning;
    if (data?.provider_id || live.providerId) turn.provider_id = data?.provider_id || live.providerId;
    if (data?.model || live.model) turn.model = data?.model || live.model;
    if (live.assets.length) turn.assets = [...live.assets];
    if (live.artifacts.length) turn.artifacts = [...live.artifacts];
    turn.assistant_timestamp = new Date().toISOString();
    if (terminalStatus === "completed") {
      turn.token_total = effectiveUsageTotal(data?.usage);
      turn.token_usage_estimated = Boolean(data?.usage_estimated);
    }
  }
}

// 回合内一次模型请求结束(chat.round_usage):立即刷新气泡计量与上下文
// 条,不等 run 完结。usage 是刚结束请求的用量,其 prompt+completion 即
// 当前上下文占用;turn_* 是回合累计。回合结束后 finishLiveRun 会用权威
// 数字覆盖这里的中间值。
export function handleRoundUsage(live, data) {
  if (live.meta) {
    const usage = formatUsageMeta({
      turnTotal: asFiniteNumber(data?.turn_total),
      turnPrompt: data?.turn_prompt,
      turnCached: data?.turn_cache_read,
      estimated: data?.estimated,
      generationTokens: data?.turn_generation_tokens,
      generationMs: data?.turn_generation_ms
    });
    if (usage) live.meta.textContent = usage;
  }
  // 输入框下方那个「累计」逐请求刷新(#131:以前只有 run.completed 才刷,子代理跑
  // 完的花销要等整回合结束才体现)。后端现在每个主回合都带会话实时累计。
  state.cumulativeBase = {
    total: asFiniteNumber(data?.cumulative_tokens),
    prompt: asFiniteNumber(data?.cumulative_prompt_tokens),
    cached: asFiniteNumber(data?.cumulative_cache_read_tokens),
  };
  // 已跑完的子代理:基线确实涨上来把它算进去了才摘掉那份估算(见 absorbDoneSubagents,
  // 躲开后端竞态导致的掉数)。
  absorbDoneSubagents(state.cumulativeBase.total);
  refreshComposerCumulative({
    speed: generationSpeedValue(data?.turn_generation_tokens, data?.turn_generation_ms),
  });
  const round = data?.usage;
  const contextTokens = asFiniteNumber(round?.prompt_tokens, 0) + asFiniteNumber(round?.completion_tokens, 0);
  if (contextTokens > 0) {
    state.context.tokens = contextTokens;
    updateContext();
  }
}

// 输入框「累计」的合成:基线(后端每回合 / 收尾给的会话实时累计)+ 正在跑的子代理
// 的实时估算之和(#131)。子代理还没落库的花销靠估算先顶上、跑完由下个主回合的
// 基线接管;并行子代理各自更新自己那一份,这里只求个和、按 rAF 合并刷,不会鬼畜抖。
export function composerCumulativeTokens() {
  const base = state.cumulativeBase || null;
  if (!base || !(base.total > 0)) return null;
  let extra = 0;
  for (const v of state.liveSubagentTokens?.values() || []) extra += asFiniteNumber(v?.tokens);
  const total = base.total + extra;
  return { total, prompt: base.prompt, cached: base.cached };
}

// 收尾:把「已跑完」的子代理估算从合成里摘掉——但只在权威基线确实已经把它算进来
// 之后才摘(基线比标记完成时涨了至少估算的一半)。否则会撞上后端竞态:子代理刚跑完、
// 它的用量还没落库进会话累计,唤醒回合的 round_usage 先带了一个不含它的基线过来,
// 这会儿摘掉估算 = 累计瞬间掉一大块(#131 后台子代理实测到的掉数)。等基线真涨上来
// 再摘,既不掉也不会和基线重复计。
export function absorbDoneSubagents(newBaseTotal) {
  for (const [id, entry] of state.liveSubagentTokens) {
    if (!entry?.done) continue;
    const grewBy = asFiniteNumber(newBaseTotal) - asFiniteNumber(entry.baseAtDone);
    if (grewBy >= asFiniteNumber(entry.tokens) * 0.5) state.liveSubagentTokens.delete(id);
  }
}

export function refreshComposerCumulative(opts = {}) {
  const cum = composerCumulativeTokens();
  const payload = {};
  if ("speed" in opts) payload.speed = opts.speed;
  payload.cumulative = cum
    ? `${formatTokens(cum.total)}${cacheSuffix(cum.cached, cum.prompt)}`
    : null;
  setComposerUsage(payload);
}

// 「≈1.2k」「498.9K」「1.2万」这类计数文本抠成数值(带 k/m/b/万 单位)。是估算,精度
// 到单位,足够撑「累计」逐步涨,收尾由后端权威基线纠正。抠不出返回 null。
export function tokensFromCount(text) {
  const m = String(text || "").match(/([\d.]+)\s*([kKmMbB万]?)/);
  if (!m) return null;
  let n = parseFloat(m[1]);
  if (!Number.isFinite(n)) return null;
  const unit = (m[2] || "").toLowerCase();
  if (unit === "k") n *= 1e3;
  else if (unit === "m") n *= 1e6;
  else if (unit === "b") n *= 1e9;
  else if (m[2] === "万") n *= 1e4;
  return Math.round(n);
}

export function finishLiveRun(kind, data, live) {
  if (!live || live.ended) return;
  const runId = live.runId;
  if (live.operation === "redo" && kind !== "completed") {
    live.ended = true;
    disposeLiveState(live);
    state.liveRuns.delete(runId);
    state.replayRunIds?.delete(runId);
    state.terminalRunIds.add(runId);
    showToast(kind === "failed" ? String(data?.message || "重新生成失败") : "重新生成已取消", "error");
    if (state.viewSessionId) loadSessionView(state.viewSessionId, { quiet: true });
    updateConversationChrome();
    updateControlState();
    return;
  }
  live.ended = true;
  clearPreparingTool(live);
  clearTypingIndicator(live);
  finalizeLiveReasoning(live);
  if (live.currentText?.renderFrame) {
    window.cancelAnimationFrame(live.currentText.renderFrame);
    live.currentText.renderFrame = null;
    live.currentText.element.__liveRaw = live.currentText.raw;
  }
  rerenderLiveHtmlFences(live);
  procLineBreak(live.blocks);
  setLiveEndpoint(live, data?.provider_id, data?.model);
  removeLiveStopButton(live);
  state.terminalRunIds.add(runId);
  if (state.terminalRunIds.size > 30) state.terminalRunIds.delete(state.terminalRunIds.values().next().value);

  if (kind === "completed") {
    if (live.headerStatus) live.headerStatus.textContent = "";
    if (live.meta) {
      const usage = formatUsageMeta({
        turnTotal: effectiveUsageTotal(data?.usage),
        turnPrompt: data?.usage?.prompt_tokens,
        turnCached: data?.usage?.cache_read_tokens,
        estimated: data?.usage_estimated,
        cumulative: data?.cumulative_tokens,
        cumulativePrompt: data?.cumulative_prompt_tokens,
        cumulativeCached: data?.cumulative_cache_read_tokens,
        generationTokens: data?.usage?.generation_tokens,
        generationMs: data?.usage?.generation_ms
      });
      live.meta.textContent = usage || "已完成";
    }
    // 输入框下方信息行:最新一轮的输出速度 + 会话累计 token(#99/#131)。收尾时
    // 会话累计是权威值(所有子代理都跑完、子会话都记好了),直接当基线,把中途的
    // 子代理实时估算清空(已被基线接管)。
    state.cumulativeBase = {
      total: asFiniteNumber(data?.cumulative_tokens),
      prompt: asFiniteNumber(data?.cumulative_prompt_tokens),
      cached: asFiniteNumber(data?.cumulative_cache_read_tokens),
    };
    // 只摘「已跑完且基线确实涨上来把它算进去」的子代理估算;仍在跑的后台子代理会活过
    // 父回合,别清;唤醒回合竞态下基线还没含它时也别清(见 absorbDoneSubagents,#131)。
    absorbDoneSubagents(state.cumulativeBase.total);
    refreshComposerCumulative({
      speed: generationSpeedValue(data?.usage?.generation_tokens, data?.usage?.generation_ms),
    });
  } else if (kind === "cancelled") {
    markUnfinishedTools(live);
    endPendingQuestions(live, "本轮已停止，无法再提交回答");
    // 停止状态只由时间线的「本轮已中断」一处表达,气泡内通知与 header/meta 不再重复
    if (live.headerStatus) live.headerStatus.textContent = "";
    if (live.meta) live.meta.textContent = "";
  } else {
    markUnfinishedTools(live);
    endPendingQuestions(live, "本轮已结束，无法再提交回答");
    const partial = Boolean(String(live.assistantText || "").trim());
    const notice = appendRunNotice(
      live,
      `${failureNoticeText(data)}${partial ? "。已写出的部分保留着。" : ""}`,
      true
    );
    if (partial) appendContinueAction(live, notice);
    if (live.headerStatus) live.headerStatus.textContent = "运行失败";
    if (live.meta) live.meta.textContent = "";
  }

  // 离屏 live 属于别的会话:state.turns 是当前视图的,不能往里塞。
  if (liveViewed(live)) updateLocalTurnFromLive(live, kind, data);
  // 刚起步就被掐掉的轮（目标编辑打断最常见）：气泡里什么都没有，留着就是
  // 一个空壳。丢弃它，让下面的静默重拉用落库的中断轮接管。
  const emptyCancelled = kind === "cancelled"
    && !String(live.assistantText || "").trim()
    && !(live.reasoningParts && live.reasoningParts.length)
    && !(live.tools && live.tools.size);
  const cancelledInView = kind === "cancelled" && data?.session_id
    && String(data.session_id) === String(state.viewSessionId || "");
  const markerTurnId = live.turnId;
  const markerArticle = (!emptyCancelled && live.article?.isConnected) ? live.article : null;
  if (emptyCancelled) {
    disposeLiveState(live);
    state.liveRuns.delete(runId);
  } else {
    stashLiveArticle(live, "final");
  }
  if (cancelledInView) {
    // 中断轮已落库（含部分输出与状态）。以前这里整会话静默重拉，把「本轮已中断」
    // 标记捎带渲染出来——但那次全量重渲染在长对话里就是中断「特别高延迟 / 感觉
    // 加载很久」的由来。改成只在原位补这一条状态行（后端 cancel 事件仅 ~12ms）。
    showInterruptedMarker(markerTurnId, markerArticle);
    // 紧跟着的那次 120ms 后台快照别再整会话重渲染一遍(上面已画对)。
    state.suppressPostCancelRender = true;
  }
  if (kind === "completed" || kind === "cancelled") {
    // 上下文条跟着正在看的会话走（没有视图时退回终端车道）。
    // cancelled 也要刷新：被中断的轮次已经持久化进上下文。
    const updatesGlobalContext = !data?.session_id
      || String(data.session_id) === String(state.viewSessionId || state.currentSessionId || "");
    if (updatesGlobalContext) {
      if (data?.context_tokens != null) state.context.tokens = Math.max(0, asFiniteNumber(data.context_tokens));
      state.context.window = data?.context_window == null ? state.context.window : Math.max(0, asFiniteNumber(data.context_window));
    }
    const usage = data?.usage && typeof data.usage === "object" ? data.usage : null;
    if (usage) {
      state.usage.last_usage = usage;
      state.usage.last_conversation_usage = usage;
      state.usage.requests = asFiniteNumber(state.usage.requests) + 1;
      state.usage.prompt_tokens = asFiniteNumber(state.usage.prompt_tokens) + asFiniteNumber(usage.prompt_tokens);
      state.usage.completion_tokens = asFiniteNumber(state.usage.completion_tokens) + asFiniteNumber(usage.completion_tokens);
      state.usage.total_tokens = asFiniteNumber(state.usage.total_tokens) + effectiveUsageTotal(usage);
      state.usage.cache_read_tokens = asFiniteNumber(state.usage.cache_read_tokens) + asFiniteNumber(usage.cache_read_tokens, 0);
      state.usage.cache_write_tokens = asFiniteNumber(state.usage.cache_write_tokens) + asFiniteNumber(usage.cache_write_tokens, 0);
    }
  }
  state.liveRuns.delete(runId);
  state.replayRunIds?.delete(runId);
  state.pendingSubmission = null;
  updateContext();
  updateRuntimeUsage(data?.usage || null, Boolean(data?.usage_estimated));
  updateConversationChrome();
  updateControlState();
  contentAdded(live);
  if (state.liveRuns.size === 0) {
    window.requestAnimationFrame(() => {
      if (!state.blocked && !consoleIsOpen()) focusComposerIfDesktop();
    });
    window.setTimeout(() => {
      if (state.liveRuns.size === 0) refreshViewSnapshot();
    }, 120);
  }
}
