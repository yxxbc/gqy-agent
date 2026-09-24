import { isWordChar, renderMathInto, underscoreCloser } from "./blocks.js";
import { appendAutoLink, appendTitleUrlLine, bareUrlAt, createLink, insideAnchor, titleUrlLineAt, validLinkUrl } from "./links.js";

export function appendInline(parent, source, depth = 0) {
  const text = String(source || "");
  if (depth > 8) {
    parent.appendChild(document.createTextNode(text));
    return;
  }
  let index = 0;
  let plainStart = 0;
  const flushPlain = (end) => {
    if (end > plainStart) parent.appendChild(document.createTextNode(text.slice(plainStart, end)));
  };
  while (index < text.length) {
    if ((index === 0 || text[index - 1] === "\n") && !insideAnchor(parent)) {
      const titled = titleUrlLineAt(text, index);
      if (titled) {
        flushPlain(index);
        appendTitleUrlLine(parent, titled, (node, source) =>
          appendInline(node, source, depth + 1));
        index += titled.length;
        plainStart = index;
        continue;
      }
    }
    if (text[index] === "\\" && text[index + 1] === "(") {
      const end = text.indexOf("\\)", index + 2);
      if (end > index + 1) {
        flushPlain(index);
        renderMathInto(parent, text.slice(index + 2, end), false);
        index = end + 2;
        plainStart = index;
        continue;
      }
    }
    if (text[index] === "\\" && index + 1 < text.length && "\\`*_[]|~$".includes(text[index + 1])) {
      flushPlain(index);
      parent.appendChild(document.createTextNode(text[index + 1]));
      index += 2;
      plainStart = index;
      continue;
    }
    if (text[index] === "$") {
      if (text[index + 1] === "$") {
        const end = text.indexOf("$$", index + 2);
        if (end > index + 1) {
          flushPlain(index);
          renderMathInto(parent, text.slice(index + 2, end), false);
          index = end + 2;
          plainStart = index;
          continue;
        }
      } else {
        // 行内 $…$:内容非空、不跨行、两端非空格,右 $ 后不紧跟数字(避开价格写法)。
        const end = text.indexOf("$", index + 1);
        const inner = end > index ? text.slice(index + 1, end) : "";
        if (
          end > index + 1
          && inner.length
          && !inner.includes("\n")
          && !/^\s/.test(inner)
          && !/\s$/.test(inner)
          && !/^\d/.test(text.slice(end + 1))
        ) {
          flushPlain(index);
          renderMathInto(parent, inner, false);
          index = end + 1;
          plainStart = index;
          continue;
        }
      }
    }
    if (text[index] === "\n") {
      flushPlain(index);
      parent.appendChild(document.createElement("br"));
      index += 1;
      plainStart = index;
      continue;
    }
    if (text[index] === "`") {
      const end = text.indexOf("`", index + 1);
      if (end > index + 1) {
        flushPlain(index);
        const code = document.createElement("code");
        code.textContent = text.slice(index + 1, end);
        parent.appendChild(code);
        index = end + 1;
        plainStart = index;
        continue;
      }
    }
    if (text[index] === "[") {
      const labelEnd = text.indexOf("](", index + 1);
      const urlEnd = labelEnd >= 0 ? text.indexOf(")", labelEnd + 2) : -1;
      if (labelEnd > index + 1 && urlEnd > labelEnd + 2) {
        const href = validLinkUrl(text.slice(labelEnd + 2, urlEnd));
        if (href) {
          flushPlain(index);
          const link = createLink(href);
          appendInline(link, text.slice(index + 1, labelEnd), depth + 1);
          parent.appendChild(link);
          index = urlEnd + 1;
          plainStart = index;
          continue;
        }
      }
    }
    // <https://…> 与裸链接。放在 ` 与 [](…) 之后:行内代码和 md 链接先被吃掉,
    // 这里看不到它们的内容。已经在 <a> 里(md 链接的标签)就不再套一层。
    if (text[index] === "<" && !insideAnchor(parent)) {
      const end = text.indexOf(">", index + 1);
      const href = end > index + 1 ? validLinkUrl(text.slice(index + 1, end)) : null;
      if (href) {
        flushPlain(index);
        appendAutoLink(parent, text.slice(index + 1, end), href);
        index = end + 1;
        plainStart = index;
        continue;
      }
    }
    if ("hHfF".includes(text[index]) && !insideAnchor(parent)) {
      const bare = bareUrlAt(text, index);
      if (bare) {
        flushPlain(index);
        appendAutoLink(parent, bare.raw, bare.href);
        index += bare.raw.length;
        plainStart = index;
        continue;
      }
    }
    if (text.startsWith("~~", index)) {
      const end = text.indexOf("~~", index + 2);
      if (end > index + 2 && text.slice(index + 2, end).trim()) {
        flushPlain(index);
        const deletion = document.createElement("del");
        appendInline(deletion, text.slice(index + 2, end), depth + 1);
        parent.appendChild(deletion);
        index = end + 2;
        plainStart = index;
        continue;
      }
    }
    const strongMarker = text.startsWith("**", index) ? "**" : text.startsWith("__", index) ? "__" : null;
    if (strongMarker && !(strongMarker === "__" && isWordChar(text[index - 1]))) {
      const end = strongMarker === "__" ? underscoreCloser(text, index + 2, "__") : text.indexOf(strongMarker, index + 2);
      if (end > index + 2 && text.slice(index + 2, end).trim()) {
        flushPlain(index);
        const strong = document.createElement("strong");
        appendInline(strong, text.slice(index + 2, end), depth + 1);
        parent.appendChild(strong);
        index = end + 2;
        plainStart = index;
        continue;
      }
    }
    if (text[index] === "*" || (text[index] === "_" && !isWordChar(text[index - 1]))) {
      const marker = text[index];
      const end = marker === "_" ? underscoreCloser(text, index + 1, "_") : text.indexOf(marker, index + 1);
      if (end > index + 1 && text.slice(index + 1, end).trim()) {
        flushPlain(index);
        const emphasis = document.createElement("em");
        appendInline(emphasis, text.slice(index + 1, end), depth + 1);
        parent.appendChild(emphasis);
        index = end + 1;
        plainStart = index;
        continue;
      }
    }
    index += 1;
  }
  flushPlain(text.length);
}
