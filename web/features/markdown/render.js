import { createIcon } from "../../core/icons.js";
import { ALERT_TYPES, codeBlock, isHorizontalRule, isMarkdownBlockStart, isTableStart, markdownTable, matchMathBlock, renderMathInto, videoNode, videoSourceFor } from "./blocks.js";
import { appendInline } from "./inline.js";

export function renderMarkdown(container, source) {
  const lines = String(source || "").replace(/\r\n?/g, "\n").split("\n");
  const fragment = document.createDocumentFragment();
  let index = 0;
  while (index < lines.length) {
    const line = lines[index];
    if (!line.trim()) {
      index += 1;
      continue;
    }
    const fence = line.match(/^\s*```\s*([\w.+-]*)\s*$/);
    if (fence) {
      const codeLines = [];
      index += 1;
      while (index < lines.length && !/^\s*```\s*$/.test(lines[index])) {
        codeLines.push(lines[index]);
        index += 1;
      }
      // 收尾围栏还没到 = 这块代码正流式写着,内容随时会变,先不上色。
      const closed = index < lines.length;
      if (closed) index += 1;
      const language = /^[\w.+-]{1,40}$/.test(fence[1] || "") ? fence[1] : "";
      fragment.appendChild(codeBlock(language, codeLines.join("\n"), closed));
      continue;
    }
    const video = videoSourceFor(line);
    if (video) {
      fragment.appendChild(videoNode(video));
      index += 1;
      continue;
    }
    if (/^\s*(\$\$|\\\[)/.test(line)) {
      const math = matchMathBlock(lines, index);
      if (math) {
        const wrapper = document.createElement("div");
        wrapper.className = "math-block";
        renderMathInto(wrapper, math.tex, true);
        fragment.appendChild(wrapper);
        index = math.nextIndex;
        continue;
      }
    }
    if (isTableStart(lines, index)) {
      const rendered = markdownTable(lines, index);
      fragment.appendChild(rendered.node);
      index = rendered.nextIndex;
      continue;
    }
    if (isHorizontalRule(line)) {
      fragment.appendChild(document.createElement("hr"));
      index += 1;
      continue;
    }
    const heading = line.match(/^(#{1,6})\s+(.+)$/);
    if (heading) {
      const level = Math.min(6, heading[1].length + 1);
      const node = document.createElement(`h${level}`);
      appendInline(node, heading[2]);
      fragment.appendChild(node);
      index += 1;
      continue;
    }
    const unordered = line.match(/^\s*[-*+]\s+(.+)$/);
    if (unordered) {
      const list = document.createElement("ul");
      let hasTask = false;
      while (index < lines.length) {
        const itemMatch = lines[index].match(/^\s*[-*+]\s+(.+)$/);
        if (!itemMatch) break;
        const item = document.createElement("li");
        const task = itemMatch[1].match(/^\[([ xX])\]\s+(.*)$/);
        if (task) {
          hasTask = true;
          item.className = "task-list-item";
          const checkbox = document.createElement("input");
          checkbox.type = "checkbox";
          checkbox.checked = task[1].toLowerCase() === "x";
          checkbox.disabled = true;
          const content = document.createElement("span");
          appendInline(content, task[2]);
          item.append(checkbox, content);
        } else {
          appendInline(item, itemMatch[1]);
        }
        list.appendChild(item);
        index += 1;
      }
      if (hasTask) list.classList.add("task-list");
      fragment.appendChild(list);
      continue;
    }
    const ordered = line.match(/^\s*\d+[.)]\s+(.+)$/);
    if (ordered) {
      const list = document.createElement("ol");
      while (index < lines.length) {
        const itemMatch = lines[index].match(/^\s*\d+[.)]\s+(.+)$/);
        if (!itemMatch) break;
        const item = document.createElement("li");
        appendInline(item, itemMatch[1]);
        list.appendChild(item);
        index += 1;
      }
      fragment.appendChild(list);
      continue;
    }
    if (/^\s*>/.test(line)) {
      const quoteLines = [];
      while (index < lines.length) {
        const quote = lines[index].match(/^\s*>\s?(.*)$/);
        if (!quote) break;
        quoteLines.push(quote[1]);
        index += 1;
      }
      const blockquote = document.createElement("blockquote");
      const alertMatch = quoteLines[0]?.match(/^\s*\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*(.*)$/i);
      if (alertMatch) {
        const kind = alertMatch[1].toLowerCase();
        blockquote.className = `markdown-alert markdown-alert-${kind}`;
        const title = document.createElement("p");
        title.className = "markdown-alert-title";
        const cfg = ALERT_TYPES[kind];
        if (cfg?.icon) {
          title.appendChild(createIcon(cfg.icon, "markdown-alert-icon"));
        }
        const label = document.createElement("span");
        label.textContent = cfg?.label || alertMatch[1];
        title.appendChild(label);
        blockquote.appendChild(title);
        if (alertMatch[2].trim()) {
          quoteLines[0] = alertMatch[2];
        } else {
          quoteLines.shift();
          while (quoteLines.length > 0 && !quoteLines[0].trim()) {
            quoteLines.shift();
          }
        }
      }
      const cleanedLines = quoteLines.map((l) => l.replace(/^#{1,6}\s+(.+)$/, "**$1**"));
      if (alertMatch) {
        if (cleanedLines.length > 0) {
          const content = document.createElement("div");
          content.className = "markdown-alert-content";
          appendInline(content, cleanedLines.join("\n"));
          blockquote.appendChild(content);
        }
      } else {
        appendInline(blockquote, cleanedLines.join("\n"));
      }
      fragment.appendChild(blockquote);
      continue;
    }
    const paragraphLines = [line];
    index += 1;
    while (index < lines.length && lines[index].trim() && !isMarkdownBlockStart(lines, index)) {
      paragraphLines.push(lines[index]);
      index += 1;
    }
    const paragraph = document.createElement("p");
    appendInline(paragraph, paragraphLines.join("\n"));
    fragment.appendChild(paragraph);
  }
  container.replaceChildren(fragment);
  // 独占一行的链接升级成卡片。这里只是排队:流式期间每来一段都会重渲染,
  // 真正的抓取要等最后一次渲染安顿下来(见 linkcards.js 的防抖)。
  window.GqyLinkCards?.scan(container);
  // 没闭合的围栏这一轮空着,等这块正文不再变了再补上色(同样是防抖)。
  window.GqyHighlight?.settle(container);
}
