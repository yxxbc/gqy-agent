import { makeCopyButton } from "../conversation/actions.js";
import { markdownStreaming } from "../live/stream.js";
import { appendInline } from "./inline.js";

export function codeBlock(language, codeText, settled = true) {
  const wrapper = document.createElement("div");
  wrapper.className = "code-block";
  const toolbar = document.createElement("div");
  toolbar.className = "code-toolbar";
  const label = document.createElement("span");
  label.textContent = language || "代码";
  const copy = makeCopyButton(codeText, "复制代码");
  copy.className = "code-copy-button";
  toolbar.append(label, copy);
  const pre = document.createElement("pre");
  const code = document.createElement("code");
  if (language) code.className = `language-${language}`;
  code.textContent = codeText;
  // 语法高亮。纯 DOM 上色,不认识的语言/分词出岔子一律保持这份纯文本
  // (见 highlight.js);settled=false 表示围栏还没闭合,这一轮先不上色。
  window.GqyHighlight?.paint(code, language, codeText, settled);
  pre.appendChild(code);
  wrapper.append(toolbar, pre);
  // ```svg / ```html 围栏闭合后画成图,块头加「预览 / 源码」(fencepreview.js)。
  if (settled) {
    window.GqyFencePreview?.decorate({ wrapper, toolbar, pre, language, source: codeText, streaming: markdownStreaming });
  }
  return wrapper;
}

export function parseTableRow(line) {
  const text = String(line || "").trim();
  const cells = [];
  let cell = "";
  let codeFenceLength = 0;
  let hasSeparator = false;
  let endedWithSeparator = false;
  for (let index = 0; index < text.length;) {
    if (text[index] === "\\" && index + 1 < text.length) {
      cell += text.slice(index, index + 2);
      index += 2;
      endedWithSeparator = false;
      continue;
    }
    if (text[index] === "`") {
      let end = index + 1;
      while (end < text.length && text[end] === "`") end += 1;
      const runLength = end - index;
      if (!codeFenceLength) codeFenceLength = runLength;
      else if (codeFenceLength === runLength) codeFenceLength = 0;
      cell += text.slice(index, end);
      index = end;
      endedWithSeparator = false;
      continue;
    }
    if (text[index] === "|" && !codeFenceLength) {
      cells.push(cell.trim());
      cell = "";
      hasSeparator = true;
      endedWithSeparator = true;
      index += 1;
      continue;
    }
    cell += text[index];
    endedWithSeparator = false;
    index += 1;
  }
  cells.push(cell.trim());
  if (text.startsWith("|")) cells.shift();
  if (endedWithSeparator) cells.pop();
  return { cells, hasSeparator };
}

export function tableAlignments(line) {
  const row = parseTableRow(line);
  if (!row.hasSeparator || !row.cells.length) return null;
  const alignments = [];
  for (const cell of row.cells) {
    const marker = cell.match(/^(:)?-{3,}(:)?$/);
    if (!marker) return null;
    alignments.push(marker[1] && marker[2] ? "center" : marker[2] ? "right" : marker[1] ? "left" : "");
  }
  return alignments;
}

export function isTableStart(lines, index) {
  if (index + 1 >= lines.length) return false;
  const header = parseTableRow(lines[index]);
  const alignments = tableAlignments(lines[index + 1]);
  return Boolean(alignments && header.hasSeparator && header.cells.length === alignments.length);
}

export function isHorizontalRule(line) {
  const text = String(line || "").trim();
  return /^(?:\*\s*){3,}$/.test(text) || /^(?:-\s*){3,}$/.test(text) || /^(?:_\s*){3,}$/.test(text);
}

export function markdownTable(lines, startIndex) {
  const headers = parseTableRow(lines[startIndex]).cells;
  const alignments = tableAlignments(lines[startIndex + 1]);
  const wrapper = document.createElement("div");
  wrapper.className = "markdown-table-scroll";
  const table = document.createElement("table");
  const head = document.createElement("thead");
  const headRow = document.createElement("tr");
  headers.forEach((content, column) => {
    const cell = document.createElement("th");
    cell.scope = "col";
    if (alignments[column]) cell.className = `align-${alignments[column]}`;
    appendInline(cell, content);
    headRow.appendChild(cell);
  });
  head.appendChild(headRow);
  table.appendChild(head);

  const body = document.createElement("tbody");
  let index = startIndex + 2;
  while (index < lines.length && lines[index].trim()) {
    const row = parseTableRow(lines[index]);
    if (!row.hasSeparator) break;
    const tableRow = document.createElement("tr");
    for (let column = 0; column < headers.length; column += 1) {
      const cell = document.createElement("td");
      if (alignments[column]) cell.className = `align-${alignments[column]}`;
      appendInline(cell, row.cells[column] || "");
      tableRow.appendChild(cell);
    }
    body.appendChild(tableRow);
    index += 1;
  }
  if (body.children.length) table.appendChild(body);
  wrapper.appendChild(table);
  return { node: wrapper, nextIndex: index };
}

export function isMarkdownBlockStart(lines, index) {
  const line = lines[index];
  return /^\s*```/.test(line) || /^#{1,6}\s+/.test(line) || /^\s*[-*+]\s+/.test(line) || /^\s*\d+[.)]\s+/.test(line) || /^\s*>/.test(line) || isHorizontalRule(line) || isTableStart(lines, index) || /^\s*\$\$/.test(line) || /^\s*\\\[\s*$/.test(line) || Boolean(videoSourceFor(line));
}

/* ── 视频消息:整行只有一个视频 URL / 本地路径(或指向它的 markdown 链接)
   时升级为播放器。本地文件经 /api/media 流式端点(带 HTTP Range)。 ── */
export const VIDEO_SOURCE_PATTERN = /\.(mp4|m4v|webm|mov|mkv|ogv)(\?[^\s)]*)?$/i;

export function videoSourceFor(rawLine) {
  const trimmed = String(rawLine || "").trim();
  if (!trimmed || trimmed.length > 2048) return null;
  const link = trimmed.match(/^\[([^\]]*)\]\(([^)\s]+)\)$/);
  const target = link ? link[2] : trimmed;
  if (/\s/.test(target) || !VIDEO_SOURCE_PATTERN.test(target)) return null;
  if (/^https?:\/\//i.test(target)) {
    return { src: target, label: link?.[1] || target };
  }
  if (target.startsWith("/") || target.startsWith("~/")) {
    return {
      src: `/api/media?path=${encodeURIComponent(target)}`,
      label: link?.[1] || target.split("/").pop() || target,
    };
  }
  return null;
}

export function videoNode(source) {
  const card = document.createElement("div");
  card.className = "video-card";
  const shell = document.createElement("div");
  shell.className = "video-shell";
  const video = document.createElement("video");
  video.controls = true;
  video.preload = "metadata";
  video.playsInline = true;
  video.src = source.src;
  const button = document.createElement("button");
  button.type = "button";
  button.className = "vfs-btn";
  button.title = "网页全屏";
  button.setAttribute("aria-label", "网页全屏");
  button.innerHTML =
    '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"/></svg>';
  button.addEventListener("click", () => shell.classList.toggle("webfs"));
  shell.append(video, button);
  const caption = document.createElement("div");
  caption.className = "video-caption";
  caption.textContent = source.label;
  card.append(shell, caption);
  return card;
}

/* ── LaTeX 公式(KaTeX):块级 $$…$$ / \[…\],行内 $…$ / \(…\)。
   katex 未就绪或语法错误时原样降级;流式期间未闭合的定界符保持原文,
   闭合后的下一次重渲染自动升级成公式。 */
export function renderMathInto(parent, tex, displayMode) {
  const trimmed = tex.trim();
  if (trimmed && window.katex && typeof window.katex.render === "function") {
    const node = document.createElement(displayMode ? "div" : "span");
    node.className = displayMode ? "math-display" : "math-inline";
    try {
      window.katex.render(trimmed, node, { displayMode, throwOnError: false, strict: "ignore" });
      parent.appendChild(node);
      return;
    } catch (_) { /* 落到原样文本 */ }
  }
  parent.appendChild(document.createTextNode(displayMode ? `$$${tex}$$` : `$${tex}$`));
}

export function matchMathBlock(lines, index) {
  const trimmed = lines[index].trim();
  for (const [open, close] of [["$$", "$$"], ["\\[", "\\]"]]) {
    if (!trimmed.startsWith(open)) continue;
    const rest = trimmed.slice(open.length);
    if (rest.length > close.length && rest.endsWith(close)) {
      return { tex: rest.slice(0, rest.length - close.length), nextIndex: index + 1 };
    }
    const body = rest && rest !== close ? [rest] : [];
    let cursor = index + 1;
    while (cursor < lines.length) {
      const candidate = lines[cursor].trim();
      if (candidate === close || candidate.endsWith(close)) {
        if (candidate !== close) body.push(candidate.slice(0, candidate.length - close.length));
        return { tex: body.join("\n"), nextIndex: cursor + 1 };
      }
      body.push(lines[cursor]);
      cursor += 1;
    }
    return null; // 未闭合:保持原文(流式中)
  }
  return null;
}

// 字母、数字、下划线算词内字符:下划线强调两头都不能挨着它们(CommonMark 的 intraword 规则)
export function isWordChar(ch) {
  return Boolean(ch) && /[\p{L}\p{N}_]/u.test(ch);
}

// 找下划线强调的闭合位:闭合的 _ 后面不能紧跟词内字符,否则继续往后找
export function underscoreCloser(text, from, marker) {
  let end = text.indexOf(marker, from);
  while (end !== -1) {
    if (!isWordChar(text[end + marker.length])) return end;
    end = text.indexOf(marker, end + 1);
  }
  return -1;
}

export const ALERT_TYPES = {
  note: { icon: "circle-alert", label: "Note" },
  tip: { icon: "lightbulb", label: "Tip" },
  important: { icon: "sparkles", label: "Important" },
  warning: { icon: "triangle-alert", label: "Warning" },
  caution: { icon: "triangle-alert", label: "Caution" },
};
