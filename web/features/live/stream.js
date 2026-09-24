import { procLineAttach, procLineBreak } from "../conversation/proc-rail.js";
import { normalizeReasoningTitle, reasoningHidden, setReasoningPeek, splitReasoningText } from "../conversation/reasoning.js";
import { createReasoningBlock } from "../conversation/render.js";
import { contentAdded } from "../conversation/scroll.js";
import { breakLiveText, clearTypingIndicator, ensureLiveArticle, promoteTypingIndicator, showTypingIndicator, syncBubbleWidth } from "./state.js";
import { renderMarkdown } from "../markdown/render.js";
import { resetPreparingWindow } from "../tools/events.js";

/// 正在画流式中间态。围栏预览据此把 html 这类「重建一次就重载一次」的活性预览推迟到
/// 回合结束那次重画——流式每帧整段重建,iframe 会一帧一闪。
export let markdownStreaming = false;

/// 流式渲染时把没闭合的行内标记先补上:模型正在输出 `` `sudo pacman -Syu` ``,
/// 闭合反引号没到之前整段按普通文字排版,一到就换成代码样式——每次这么
/// 一换,那一行前后的字全部重排,看起来就是「字在跳」(09-10 沙盒逐帧取证)。
/// 只补三样:未闭合的围栏代码块、行内反引号、`**` 粗体;单个 `*`/`_` 与列表
/// 和数学冲突,不碰。回合结束后按落库原文重画,这里的补丁不进任何存档。
export function stabilizeStreamingMarkdown(raw) {
  const text = String(raw || "");
  if (!text) return text;
  const lines = text.split("\n");
  let fenceOpen = false;
  let tailStart = 0;
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (/^\s*(```|~~~)/.test(line)) {
      fenceOpen = !fenceOpen;
      // 围栏一关,尾巴从它后面算:围栏里的反引号不参与行内配对
      if (!fenceOpen) tailStart = index + 1;
    } else if (!fenceOpen && !line.trim()) tailStart = index + 1;
  }
  if (fenceOpen) return `${text}\n\`\`\``;
  const tail = lines.slice(tailStart).join("\n");
  let patched = text;
  const backticks = (tail.match(/`/g) || []).length;
  if (backticks % 2 === 1) patched += "`";
  const bolds = (tail.match(/\*\*/g) || []).length;
  if (bolds % 2 === 1) patched += "**";
  return patched;
}

/// 流式中间态的渲染入口:打上 markdownStreaming,围栏预览据此推迟活性内容。
/// 原文挂在元素上,回合结束时 rerenderLiveHtmlFences 按它补画一次。
export function renderStreamingMarkdown(block) {
  block.element.__liveRaw = block.raw;
  markdownStreaming = true;
  try {
    renderMarkdown(block.element, stabilizeStreamingMarkdown(block.raw));
  } finally {
    markdownStreaming = false;
  }
}

/// 流式期间 ```html / ```mermaid 围栏只占位(见 codeBlock),回合结束补画成沙箱预览。
/// 只重画含这两种围栏的块:其余块流式结果与终稿同构,不必再换一遍 DOM。
export function rerenderLiveHtmlFences(live) {
  for (const element of live.blocks?.querySelectorAll(".live-text-block") || []) {
    const raw = element.__liveRaw;
    if (typeof raw === "string" && /^\s*```\s*(html?|mermaid)\s*$/im.test(raw)) renderMarkdown(element, raw);
  }
}

export function scheduleMarkdownRender(block) {
  if (block.renderFrame) return;
  block.renderFrame = window.requestAnimationFrame(() => {
    block.renderFrame = null;
    renderStreamingMarkdown(block);
    contentAdded(block.element);
  });
}

export function appendAssistantDelta(live, delta) {
  const text = String(delta || "");
  if (!text) return;
  ensureLiveArticle(live);
  const startsText = !live.currentText;
  if (!live.currentText) {
    finalizeLiveReasoning(live);
    const element = document.createElement("div");
    element.className = "markdown-body live-text-block";
    const block = { element, raw: "", renderFrame: null };
    procLineBreak(live.blocks);
    live.blocks.appendChild(element);
    syncBubbleWidth(live.article);
    live.currentText = block;
    live.contextOperation = null;
    if (live.assistantText && !/\s$/.test(live.assistantText)) live.assistantText += "\n\n";
  }
  live.currentText.raw += text;
  live.assistantText += text;
  live.copyButton.hidden = !live.assistantText.trim();
  if (startsText) {
    renderStreamingMarkdown(live.currentText);
    promoteTypingIndicator(live);
  } else {
    scheduleMarkdownRender(live.currentText);
  }
  contentAdded(live);
}

export function resetSupersededGeneration(live) {
  if (live.currentText?.renderFrame) window.cancelAnimationFrame(live.currentText.renderFrame);
  live.currentText?.element?.remove();
  live.currentText = null;
  for (const reasoning of live.reasoningParts || []) reasoning.element?.remove();
  if (live.reasoningTimer) window.clearInterval(live.reasoningTimer);
  live.reasoningTimer = null;
  live.reasoning = null;
  live.reasoningParts = [];
  live.reasoningStarted = false;
  live.reasoningTitle = "";
  live.reasoningClockStart = null;
  live.assistantText = "";
  live.assistantReasoning = "";
  if (live.copyButton) live.copyButton.hidden = true;
  clearTypingIndicator(live);
  showTypingIndicator(live);
}

export function ensureLiveReasoning(live) {
  ensureLiveArticle(live);
  clearTypingIndicator(live, { waitingOnly: true });
  if (live.reasoning) return live.reasoning;
  breakLiveText(live);
  live.contextOperation = null;
  const reasoning = createReasoningBlock("", "正在思考", true);
  // 计时从 reasoning.start 事件算起,而不是签出现的时刻(签是惰性创建的)
  if (live.reasoningClockStart != null) reasoning.startedAt = live.reasoningClockStart;
  reasoning.pendingTitle = normalizeReasoningTitle(live.reasoningTitle);
  if (!reasoningHidden()) procLineAttach(live.blocks, reasoning.element);
  live.reasoning = reasoning;
  live.reasoningParts.push(reasoning);
  if (live.reasoningTimer) window.clearInterval(live.reasoningTimer);
  const updateProgress = () => {
    if (!reasoning.liveStatus || reasoning.startedAt == null) return;
    const elapsed = Math.max(0, Math.floor((performance.now() - reasoning.startedAt) / 1000));
    reasoning.liveStatus.textContent = `${elapsed}s`;
  };
  updateProgress();
  live.reasoningTimer = window.setInterval(updateProgress, 1000);
  return reasoning;
}

export function collectLiveReasoning(live) {
  return (live.reasoningParts || [])
    .map((part) => String(part.raw || "").trim())
    .filter(Boolean)
    .join("\n\n");
}

export function finalizeLiveReasoning(live) {
  const reasoning = live.reasoning;
  if (!reasoning) return;
  if (live.reasoningTimer) {
    window.clearInterval(live.reasoningTimer);
    live.reasoningTimer = null;
  }
  const parsed = splitReasoningText(reasoning.raw);
  const title = "已思考";
  reasoning.raw = parsed.body;
  reasoning.finished = true;
  if (!reasoning.raw.trim() && title === "已思考") {
    reasoning.element.remove();
  } else {
    reasoning.element.classList.remove("is-live");
    reasoning.title.textContent = title;
    reasoning.body.textContent = reasoning.raw;
    if (reasoning.progress) reasoning.progress.remove();
    if (reasoning.liveStatus) {
      if (reasoning.startedAt != null) {
        reasoning.liveStatus.textContent = `${((performance.now() - reasoning.startedAt) / 1000).toFixed(1)}s`;
      } else {
        reasoning.liveStatus.remove();
      }
    }
  }
  live.reasoning = null;
  live.reasoningTitle = "";
  live.reasoningStarted = false;
  live.reasoningClockStart = null;
  live.assistantReasoning = collectLiveReasoning(live);
}

export function handleReasoningEvent(name, live, data) {
  if (name === "reasoning.start" || name === "reasoning.part_start") {
    // 惰性创建:只记状态,签等第一段真实思考文本(reasoning.delta)到达才出现,
    // 避免不输出思考的模型挂着空的「正在思考」签和空面板
    finalizeLiveReasoning(live);
    resetPreparingWindow(live);
    live.reasoningStarted = true;
    live.reasoningClockStart = performance.now();
    breakLiveText(live);
    return;
  }
  if (name === "reasoning.reset") {
    if (live.reasoning) {
      live.reasoning.raw = "";
      live.reasoning.body.textContent = "";
      live.reasoning.pendingTitle = "";
    }
    return;
  }
  if (name === "reasoning.title") {
    live.reasoningTitle = String(data?.title || "").trim();
    // 只更新已存在的签;没有思考文本就不为标题单独建签
    if (live.reasoning) live.reasoning.pendingTitle = normalizeReasoningTitle(live.reasoningTitle);
    return;
  }
  if (name === "reasoning.delta") {
    const delta = String(data?.delta || "");
    if (!delta) return;
    if (!live.reasoning && !delta.trim()) return;
    const reasoning = ensureLiveReasoning(live);
    reasoning.raw += delta;
    reasoning.body.textContent = reasoning.raw;
    // 窥视槽只放尾巴:换行折成空格,取最后 160 字,够撑满一行还不至于每个 delta 都重排一大段
    setReasoningPeek(reasoning.peek, reasoning.raw);
    live.assistantReasoning = collectLiveReasoning(live);
    contentAdded(live);
    return;
  }
  if (name === "reasoning.part_end") {
    finalizeLiveReasoning(live);
  }
}
