import { formatDateTime, formatFileSize } from "../../core/format.js";
import { makeIconSlot } from "../../core/icons.js";
import { safeAttachmentUrl } from "../composer/attachments.js";
import { makeCopyButton, makeMessageAction, openRevisionEditor } from "./actions.js";
import { validAssetDimension } from "./media.js";
import { removeQueuedPrompt } from "../live/state.js";
import { makeAvatarFrame } from "../persona.js";
import { codeBlock } from "../markdown/blocks.js";
import { appendAutoLink, appendTitleUrlLine, bareUrlAt, titleUrlLineAt, validLinkUrl } from "../markdown/links.js";

/// daemon 自己合成的轮，不是任何人敲的：后台任务唤醒、目标续轮。
///
/// 判据收口在这里——原来两处各写一遍前缀列表，加一种合成轮就得记得改两个
/// 地方，漏一个的表现是「时间线里画成用户气泡、但滚动到底又不算用户消息」。
export function isSyntheticTurnContent(raw) {
  const text = String(raw || "");
  return text.startsWith("[后台任务完成]")
    || text.startsWith("[后台命令完成]")
    || text.startsWith("[目标续轮]")
    || text.startsWith("<background-job-report>")
    || text.startsWith("<goal_round>");
}

/// `createUserMessage` 对目标续轮返回 null（那一轮在时间线里不画）。
/// 每个调用点各写一遍判空太容易漏，统一走这里。
export function appendUserMessage(parent, content, timestamp, attributes = {}) {
  const node = createUserMessage(content, timestamp, attributes);
  if (node) parent.appendChild(node);
  return node;
}

/**
 * 自己发出去的消息:只渲染代码块、行内代码和链接,别的一律原样。
 *
 * 不做完整 markdown 是有意的(09-09 用户拍板)。把 `*星号*` 变成斜体、`# 井号`
 * 变成标题,等于把人原样打进去的字改掉了——而她收到的仍是原文,两边对不上。
 * 代码块没有这个问题:``` 围栏本来就是「这段原样看」的意思;链接同理,地址
 * 文字一个字都不变,只是变成可点的。
 */
export function renderUserText(container, source) {
  const text = String(source || "");
  const lines = text.split("\n");
  const fragment = document.createDocumentFragment();
  let buffer = [];
  const flushText = () => {
    if (!buffer.length) return;
    const chunk = buffer.join("\n");
    buffer = [];
    // 围栏之间的空行不值得单独占一段。
    if (!chunk.trim()) return;
    const paragraph = document.createElement("p");
    appendUserInline(paragraph, chunk);
    fragment.appendChild(paragraph);
  };
  let index = 0;
  while (index < lines.length) {
    const fence = lines[index].match(/^\s*```\s*([\w.+-]*)\s*$/);
    if (!fence) {
      buffer.push(lines[index]);
      index += 1;
      continue;
    }
    flushText();
    index += 1;
    const body = [];
    while (index < lines.length && !/^\s*```\s*$/.test(lines[index])) {
      body.push(lines[index]);
      index += 1;
    }
    // 收尾围栏可能没打,那也照样当代码块渲染——半截的围栏更该原样看。
    index += 1;
    fragment.appendChild(codeBlock(fence[1] || "", body.join("\n")));
  }
  flushText();
  container.replaceChildren(fragment);
}

/** 行内:反引号、<url>、裸地址,其余原样。 */
export function appendUserInline(parent, source) {
  const text = String(source || "");
  let index = 0;
  let plainStart = 0;
  const flushPlain = (end) => {
    if (end > plainStart) parent.appendChild(document.createTextNode(text.slice(plainStart, end)));
  };
  while (index < text.length) {
    if (index === 0 || text[index - 1] === "\n") {
      const titled = titleUrlLineAt(text, index);
      if (titled) {
        flushPlain(index);
        appendTitleUrlLine(parent, titled, appendUserInline);
        index += titled.length;
        plainStart = index;
        continue;
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
    if (text[index] === "<") {
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
    if ("hHfF".includes(text[index])) {
      const bare = bareUrlAt(text, index);
      if (bare) {
        flushPlain(index);
        appendAutoLink(parent, bare.raw, bare.href);
        index += bare.raw.length;
        plainStart = index;
        continue;
      }
    }
    index += 1;
  }
  flushPlain(text.length);
}

export function createUserMessage(content, timestamp, attributes = {}) {
  // 系统自动触发的后台任务跟进不是真实用户输入，渲染为居中系统事件而不是用户气泡。
  const rawContent = String(content || "");
  // 目标续轮在时间线里什么都不画：输入框上方的状态行已经在说「进行中 ·
  // 第 N 轮」，对话流里每轮再来一条居中提示只是噪声，几十轮下来会把真正
  // 的内容淹掉。AI 的输出照常显示。
  if (rawContent.startsWith("[目标续轮]") || rawContent.startsWith("<goal_round>")) {
    return null;
  }
  // 目标变更通知走的是排队消息管线（步间送达、随回合持久化），但它是一次
  // 操作的回执，不是用户说的话——画成居中提示而不是用户气泡。
  if (rawContent.startsWith("[目标已变更] ")) {
    const notice = document.createElement("div");
    notice.className = "system-event is-command-result";
    if (attributes.turnId) notice.dataset.turnId = attributes.turnId;
    const label = document.createElement("span");
    label.textContent = `目标已变更：${rawContent.slice("[目标已变更] ".length)}`;
    label.title = formatDateTime(timestamp);
    notice.appendChild(label);
    return notice;
  }
  if (isSyntheticTurnContent(rawContent)) {
    const notice = document.createElement("div");
    notice.className = "system-event";
    if (attributes.turnId) notice.dataset.turnId = attributes.turnId;
    const label = document.createElement("span");
    let labelText = "";
    if (rawContent.startsWith("[后台任务完成]")) {
      labelText = rawContent.replace(/^\[后台任务完成\]\s*/, "");
    } else if (rawContent.startsWith("[后台命令完成]")) {
      const stripped = rawContent.replace(/^\[后台命令完成\]\s*/, "");
      labelText = `命令完成 ${stripped.split(" · ").slice(0, 2).join(" · ")}`;
    } else {
      const inner = (rawContent.match(/「(.*?)」/)?.[1] || "").trim();
      labelText = inner ? `任务完成 ${inner}` : "后台任务完成";
    }
    label.textContent = `⚙ ${labelText}`;
    label.title = rawContent;
    label.title = formatDateTime(timestamp);
    notice.appendChild(label);
    return notice;
  }
  const article = document.createElement("article");
  article.className = "message user-message";
  article.dataset.role = "user";
  if (attributes.turnId) article.dataset.turnId = attributes.turnId;
  if (attributes.runId) article.dataset.runId = attributes.runId;
  if (attributes.followupId) article.dataset.followupId = attributes.followupId;
  if (attributes.inputId) article.dataset.inputId = attributes.inputId;
  const bubble = document.createElement("div");
  bubble.className = "user-bubble";
  const textContent = String(content || "");
  renderUserText(bubble, textContent);
  bubble.hidden = !textContent.trim();
  const attachments = createUserAttachments(attributes.attachments);
  if (attributes.queued) {
    // 排队的消息:直接画在对话末尾,像一条已经发出去的,只是左边挂一枚「排队中」小签
    // 和一个撤下按钮。轮到它时 consumeLiveQueue 会画真的那条,这条随之撤掉。
    article.classList.add("is-queued");
    article.dataset.queueId = String(attributes.queueId || "");
    const badge = document.createElement("span");
    badge.className = "queue-badge";
    const label = document.createElement("span");
    label.textContent = "排队中";
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "queue-remove";
    remove.title = "撤回这条排队消息";
    remove.setAttribute("aria-label", "撤回这条排队消息");
    remove.appendChild(makeIconSlot("undo-2"));
    remove.addEventListener("click", () => removeQueuedPrompt(attributes.queueId));
    badge.append(label, remove);
    if (attachments) article.appendChild(attachments);
    article.append(badge, bubble, makeAvatarFrame("me"));
    return article;
  }
  const actions = document.createElement("div");
  actions.className = "message-actions";
  if (attributes.revisionTarget) {
    const edit = makeMessageAction("square-pen", "编辑最后一条消息", () => {
      openRevisionEditor(article, bubble, textContent, attributes.revisionTarget, edit);
    });
    edit.className = "edit-action";
    actions.appendChild(edit);
  }
  if (textContent.trim()) actions.appendChild(makeCopyButton(textContent, "复制消息"));
  if (attachments) article.appendChild(attachments);
  // 自己的头像:没设置时由 CSS 整个藏起来(body.has-user-avatar),设置页换图不用重画对话。
  article.append(bubble, actions, makeAvatarFrame("me"));
  return article;
}

/**
 * 附件芯片的图标。全都画成 file-text 的话，一段视频和一份 md 长得一模一样,
 * 扫一眼分不出哪个是哪个(09-09 用户实拍)。按 MIME 优先、拿不到再看扩展名。
 */
export const ATTACHMENT_EXTENSION_ICONS = {
  md: "file-markdown", markdown: "file-markdown",
  json: "file-json", jsonc: "file-json",
  pdf: "file-pdf",
  zip: "file-archive", tar: "file-archive", gz: "file-archive", xz: "file-archive",
  zst: "file-archive", "7z": "file-archive", rar: "file-archive",
  js: "file-code", mjs: "file-code", ts: "file-code", tsx: "file-code", jsx: "file-code",
  py: "file-code", rs: "file-code", go: "file-code", c: "file-code", h: "file-code",
  cpp: "file-code", hpp: "file-code", java: "file-code", rb: "file-code", php: "file-code",
  sh: "file-code", bash: "file-code", zsh: "file-code", fish: "file-code", lua: "file-code",
  toml: "file-code", yaml: "file-code", yml: "file-code", ini: "file-code", css: "file-code",
  html: "file-code", xml: "file-code", sql: "file-code", nix: "file-code",
};

export function attachmentIconName(attachment) {
  const mime = String(attachment?.mime || "").toLowerCase();
  if (attachment?.kind === "image" || mime.startsWith("image/")) return "image";
  if (mime.startsWith("video/")) return "file-video";
  if (mime.startsWith("audio/")) return "file-audio";
  if (mime === "application/pdf") return "file-pdf";
  const extension = String(attachment?.name || "").split(".").pop()?.toLowerCase() || "";
  return ATTACHMENT_EXTENSION_ICONS[extension] || "file-text";
}

export function createUserAttachments(values) {
  const attachments = Array.isArray(values) ? values : [];
  if (!attachments.length) return null;
  const list = document.createElement("div");
  list.className = "user-attachments";
  for (const attachment of attachments) {
    const url = safeAttachmentUrl(attachment?.url);
    if (!url) continue;
    const name = String(attachment?.name || "附件");
    if (attachment?.kind === "image" || String(attachment?.mime || "").startsWith("image/")) {
      const link = document.createElement("a");
      link.className = "user-attachment-image";
      link.href = url;
      link.target = "_blank";
      link.rel = "noopener noreferrer";
      link.title = name;
      // 会话里的图点开是放大预览，自己发的图没道理反而是「跳走一个新标签
      // 页」。按住 Ctrl/⌘ 或中键仍然走链接原本的行为。
      link.addEventListener("click", (event) => {
        if (event.metaKey || event.ctrlKey || event.shiftKey || event.button !== 0) return;
        if (!window.GqyLightbox) return;
        event.preventDefault();
        window.GqyLightbox.open({ url, name });
      });
      const image = document.createElement("img");
      image.src = url;
      image.alt = name;
      image.loading = "lazy";
      image.decoding = "async";
      const width = validAssetDimension(attachment?.width);
      const height = validAssetDimension(attachment?.height);
      if (width) image.width = width;
      if (height) image.height = height;
      link.appendChild(image);
      list.appendChild(link);
      continue;
    }
    // 能预览的芯片：整块是「看看是什么」，右边箭头单独负责下载。不能预览的
    // 二进制维持原样，整块就是下载链接。
    const previewable = Boolean(window.GqyPreview?.canPreview(attachment));
    const chip = document.createElement(previewable ? "div" : "a");
    chip.className = "user-attachment-file";
    if (previewable) {
      chip.classList.add("is-previewable");
      chip.tabIndex = 0;
      chip.setAttribute("role", "button");
      chip.title = `预览 ${name}`;
      const openPreview = () => window.GqyPreview.open({ ...attachment, url, name });
      chip.addEventListener("click", openPreview);
      chip.addEventListener("keydown", (event) => {
        if (event.key !== "Enter" && event.key !== " ") return;
        event.preventDefault();
        openPreview();
      });
    } else {
      chip.href = url;
      chip.setAttribute("download", "");
      chip.title = `下载 ${name}`;
    }
    chip.appendChild(makeIconSlot(attachmentIconName(attachment)));
    const copy = document.createElement("span");
    const strong = document.createElement("strong");
    strong.textContent = name;
    const small = document.createElement("small");
    small.textContent = formatFileSize(attachment?.size);
    copy.append(strong, small);
    chip.appendChild(copy);
    if (previewable) {
      const download = document.createElement("a");
      download.className = "user-attachment-download";
      download.href = url;
      download.setAttribute("download", "");
      download.title = `下载 ${name}`;
      download.setAttribute("aria-label", `下载 ${name}`);
      download.addEventListener("click", (event) => event.stopPropagation());
      download.appendChild(makeIconSlot("download"));
      chip.appendChild(download);
    } else {
      chip.appendChild(makeIconSlot("download"));
    }
    list.appendChild(chip);
  }
  return list.childElementCount ? list : null;
}
