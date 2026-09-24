import { copyText } from "../conversation/actions.js";

// 认得的协议。file:// 在里面是因为模型会用它指本地路径(MCP 服务器目录之类),
// 以前不认,整条 [label](file://…) 就原样漏成 Markdown 源码。
export function validLinkUrl(value) {
  const raw = String(value || "").trim();
  if (!/^(?:https?|file):\/\//i.test(raw)) return null;
  try {
    const url = new URL(raw);
    return ["http:", "https:", "file:"].includes(url.protocol) ? url.href : null;
  } catch (_) {
    return null;
  }
}

export function isFileUrl(href) {
  return /^file:/i.test(String(href || ""));
}

export function filePathOf(href) {
  try {
    return decodeURIComponent(String(href).replace(/^file:\/\//i, "")) || String(href);
  } catch (_) {
    return String(href).replace(/^file:\/\//i, "");
  }
}

// 一个链接节点。file:// 单独一条路:浏览器从 http 页面导航到 file:// 会被安全
// 策略**静默**拦下——做成真链接的话点了什么都不会发生,比不成链更让人困惑。
// 所以本地路径改成「点一下把路径复制走」,hover 看完整路径。
export function createLink(href) {
  const link = document.createElement("a");
  link.href = href;
  link.rel = "noopener noreferrer";
  if (isFileUrl(href)) {
    const path = filePathOf(href);
    link.classList.add("path-link");
    link.title = path;
    link.addEventListener("click", (event) => {
      event.preventDefault();
      copyText(path);
    });
  } else {
    link.target = "_blank";
  }
  return link;
}

// 裸链接自动成链。模型经常直接把 URL 写进正文而不套 [](),以前这些只是纯文本,
// 点不动。识别到句尾标点要吐回去:"见 https://a.com。" 里的句号不属于地址。
export const BARE_URL_TAIL = "。，、；：！？…～\"'`,.;:!?’”»›|*_~";

export const BARE_URL_PAIRS = { ")": "(", "]": "[", "}": "{", "》": "《", "」": "「", "』": "『", "】": "【" };

export function trimUrlTail(raw) {
  let value = raw;
  while (value.length) {
    const last = value[value.length - 1];
    const opener = BARE_URL_PAIRS[last];
    if (opener) {
      // 括号只在成对时留下:GitHub/维基的地址本身就带括号。
      const opens = value.split(opener).length - 1;
      const closes = value.split(last).length - 1;
      if (closes <= opens) break;
      value = value.slice(0, -1);
      continue;
    }
    if (BARE_URL_TAIL.includes(last)) {
      value = value.slice(0, -1);
      continue;
    }
    break;
  }
  return value;
}

export function bareUrlAt(text, index) {
  // 前一个字符是字母数字时不认:避开 "xhttps://" 这类粘连。
  if (index > 0 && /[A-Za-z0-9]/.test(text[index - 1])) return null;
  // 中日韩标点一个都不能进地址。只在末尾修剪不够:「…archlinux.org、AUR」里
  // 顿号后面还跟着字母,末尾修剪碰不到它,整段会被 new URL() 当成域名的一部分
  // punycode 掉(实测变成 xn--orgaur-kr3e)。汉字本身不排除——维基那种带中文
  // 路径的地址是合法的。
  const matched = /^(?:https?|file):\/\/[^\s<>"'`\u00a0\u2000-\u206f\u3000-\u303f\uff00-\uffef]+/i.exec(
    text.slice(index)
  );
  if (!matched) return null;
  const raw = trimUrlTail(matched[0]);
  if (!raw) return null;
  const href = validLinkUrl(raw);
  return href ? { raw, href } : null;
}

export function insideAnchor(node) {
  let cursor = node;
  while (cursor) {
    if (cursor.tagName === "A") return true;
    cursor = cursor.parentElement;
  }
  return false;
}

export function appendAutoLink(parent, raw, href) {
  const link = createLink(href);
  link.classList.add("auto-link");
  link.textContent = raw;
  parent.appendChild(link);
}

// 「标题 (地址)」独占一行:模型给参考资料就是这么写的,标题是纯文本,于是以前
// 只有括号里那半截像链接,读起来像标题和地址是两码事。整行命中时标题进同一个
// <a>,点标题和点地址都能走。规则跟终端那边(src/render/link.rs)是同一套。
export const TITLE_URL_LINE = /^([ \t]*)(\S[^\n]*?)([ \t]*)([（(])[ \t]*((?:https?|file):\/\/[^\s)）]+)[ \t]*([)）])[ \t]*$/;

export function titleUrlLineAt(text, index) {
  const lineEnd = text.indexOf("\n", index);
  const line = lineEnd < 0 ? text.slice(index) : text.slice(index, lineEnd);
  const matched = TITLE_URL_LINE.exec(line);
  if (!matched) return null;
  const [, indent, title, gap, open, url, close] = matched;
  // 标题里再有地址就不是「标题 (地址)」;结尾是 ] 说明这其实是 [label](url),
  // 那条本来就有自己的分支——不拦住的话整条 Markdown 会被当成标题原样漏出来。
  if (title.includes("://") || title.endsWith("]")) return null;
  // 标题是一句话、不是一段话:句中有句号/问号/叹号/分号或长得离谱,就只让地址成链
  // (09-11 手机端实测一整段中文正文被下划线包成一个链接)。
  // 判据:中文句末标点直接算散文;英文只认「句点/问号/叹号 + 空格 + 大写或汉字」这种
  // 句界——"vs." 这类缩写后面跟小写,不算。
  if ([...title].length > 120 || /[。！？；]/.test(title) || /[.!?]\s+[A-Z\u4e00-\u9fff]/.test(title)) return null;
  const href = validLinkUrl(url);
  return href ? { length: line.length, indent, title, gap, open, url, close, href } : null;
}

export function appendTitleUrlLine(parent, hit, appendTitle) {
  if (hit.indent) parent.appendChild(document.createTextNode(hit.indent));
  const link = createLink(hit.href);
  link.classList.add("auto-link", "title-link");
  const title = document.createElement("span");
  title.className = "link-title";
  appendTitle(title, hit.title);
  link.appendChild(title);
  link.appendChild(document.createTextNode(`${hit.gap}${hit.open}`));
  const address = document.createElement("span");
  address.className = "link-url";
  address.textContent = hit.url;
  link.appendChild(address);
  link.appendChild(document.createTextNode(hit.close));
  parent.appendChild(link);
}
