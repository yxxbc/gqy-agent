import { DEFAULT_BOARD_SUBTITLE, DEFAULT_BOARD_TITLE, DEFAULT_STARTER_PROMPTS, DEV_COMPOSER_PLACEHOLDER, defaultComposerPlaceholder } from "../core/constants.js";
import { elements } from "../state/elements.js";
import { state } from "../state/store.js";

export function normalizePersona(value) {
  const name = String(value?.name || "").trim() || "GQY";
  const avatarUrl = typeof value?.avatar_url === "string" && value.avatar_url ? value.avatar_url : null;
  const boardImageUrl = typeof value?.board_image_url === "string" && value.board_image_url
    ? value.board_image_url
    : null;
  const boardTitle = String(value?.board_title || "").trim() || DEFAULT_BOARD_TITLE;
  const boardSubtitle = String(value?.board_subtitle || "").trim() || DEFAULT_BOARD_SUBTITLE;
  const composerPlaceholder =
    String(value?.composer_placeholder || "").trim() || defaultComposerPlaceholder(name);
  const configuredPrompts = Array.isArray(value?.starter_prompts) ? value.starter_prompts : [];
  const starterPrompts = DEFAULT_STARTER_PROMPTS.map((fallback, index) => String(configuredPrompts[index] || "").trim() || fallback);
  // revision 只在图片 URL 真正变化时更新:每次快照都取 Date.now() 会让
  // 头像/看板图的浏览器缓存永远击穿,每次 bootstrap 都重新下载。
  const previous = state.persona;
  const revision =
    previous && previous.avatar_url === avatarUrl && previous.board_image_url === boardImageUrl
      ? previous.revision
      : `${Date.now()}`;
  return {
    name,
    avatar_url: avatarUrl,
    board_image_url: boardImageUrl,
    board_title: boardTitle,
    board_subtitle: boardSubtitle,
    composer_placeholder: composerPlaceholder,
    starter_prompts: starterPrompts,
    revision
  };
}

export function setPersonaAvatar(image) {
  const url = state.persona?.avatar_url;
  image.hidden = !url;
  if (!url) {
    image.removeAttribute("src");
    return;
  }
  image.hidden = false;
  const separator = url.includes("?") ? "&" : "?";
  image.src = `${url}${separator}v=${encodeURIComponent(state.persona?.revision || "1")}`;
  image.onerror = () => {
    image.hidden = true;
    image.removeAttribute("src");
  };
}

/*
 * 头像取景与尺寸(09-26)。
 *
 * 取景按「焦点」记:x/y 是焦点偏离中心的百分比(±50),zoom 以焦点为原点放大。
 * 值写成 body 上的 CSS 变量,所有头像(对话、侧栏、设置页的预览)都读同一组,
 * 设置页里拖一下,整页的头像跟着动,不用逐个元素去改。
 */
export const AVATAR_SIZE_DEFAULT = 30;
const DEFAULT_FRAME = { zoom: 1, x: 0, y: 0 };

function normalizeFrame(frame) {
  const number = (value, fallback, min, max) => {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? Math.min(max, Math.max(min, parsed)) : fallback;
  };
  return {
    zoom: number(frame?.zoom, 1, 1, 4),
    x: number(frame?.x, 0, -50, 50),
    y: number(frame?.y, 0, -50, 50)
  };
}

export function avatarDisplay() {
  const display = state.account?.avatar_display || {};
  return {
    size: Number(display.size) || AVATAR_SIZE_DEFAULT,
    user: normalizeFrame(display.user || DEFAULT_FRAME),
    assistant: normalizeFrame(display.assistant || DEFAULT_FRAME)
  };
}

/// 把取景写成 CSS 变量。patch 只在设置页拖动时传,先改本地,保存另走接口。
export function applyAvatarDisplay(patch = null) {
  if (patch && state.account) {
    state.account.avatar_display = { ...(state.account.avatar_display || {}), ...patch };
  }
  const display = avatarDisplay();
  const style = document.body.style;
  for (const [prefix, frame] of [["her", display.assistant], ["me", display.user]]) {
    style.setProperty(`--${prefix}-fx`, `${50 + frame.x}%`);
    style.setProperty(`--${prefix}-fy`, `${50 + frame.y}%`);
    style.setProperty(`--${prefix}-zoom`, String(frame.zoom));
  }
  style.setProperty("--chat-avatar-size", `${display.size}px`);
  return display;
}

/// 圆形取景框:外层裁圆、描边,里面的图按 --her-* / --me-* 取景。
export function makeAvatarFrame(who) {
  const frame = document.createElement("span");
  frame.className = `avatar-frame is-${who}`;
  frame.setAttribute("aria-hidden", "true");
  const image = document.createElement("img");
  image.alt = "";
  frame.appendChild(image);
  if (who === "her") setPersonaAvatar(image);
  else setUserAvatar(image);
  return frame;
}

export function setUserAvatar(image) {
  const url = state.account?.avatar_url;
  image.hidden = !url;
  if (!url) {
    image.removeAttribute("src");
    return;
  }
  image.src = url;
  image.onerror = () => {
    image.hidden = true;
    image.removeAttribute("src");
  };
}

/// 换了/删了自己的头像:更新状态,整页的用户头像一起换。
export function setUserAvatarUrl(url) {
  if (state.account) state.account.avatar_url = url || null;
  document.body.classList.toggle("has-user-avatar", Boolean(url));
  document.querySelectorAll(".avatar-frame.is-me img").forEach(setUserAvatar);
}

/// 人格看板图(壁纸)带缓存版本号的地址;没配返回空串。
export function boardImageSrc() {
  const url = state.persona?.board_image_url;
  if (!url) return "";
  return `${url}${url.includes("?") ? "&" : "?"}v=${encodeURIComponent(state.persona.revision || "1")}`;
}

/// 占位语跟着侧栏模式走（sessions/mode.js 切换时也会调）。
export function syncComposerPlaceholder() {
  elements.composerInput.placeholder = state.sessionMode === "dev"
    ? DEV_COMPOSER_PLACEHOLDER
    : state.persona.composer_placeholder;
}

export function applyPersona(value) {
  state.persona = normalizePersona(value);
  elements.brandName.textContent = state.persona.name;
  elements.brandAvatar.alt = state.persona.name;
  setPersonaAvatar(elements.brandAvatar);
  // 侧栏「她的房间」与模式徽章上的也是她。
  if (elements.modeBadgeAvatar) setPersonaAvatar(elements.modeBadgeAvatar);
  // 月洞窗里放看板立绘;没配看板就退回头像。
  if (elements.herRoomBoard) {
    const board = boardImageSrc();
    if (board) {
      elements.herRoomBoard.hidden = false;
      elements.herRoomBoard.src = board;
      elements.herRoomBoard.classList.add("is-board");
    } else {
      elements.herRoomBoard.classList.remove("is-board");
      setPersonaAvatar(elements.herRoomBoard);
    }
  }
  if (elements.herRoomName) elements.herRoomName.textContent = state.persona.name;
  elements.emptyKickerName.textContent = state.persona.name;
  elements.emptyTitle.textContent = state.persona.board_title;
  elements.emptySubtitle.textContent = state.persona.board_subtitle;
  syncComposerPlaceholder();
  const boardImageUrl = state.persona.board_image_url;
  elements.emptyVisual.hidden = !boardImageUrl;
  elements.emptyBoardImage.alt = `${state.persona.name} 看板图片`;
  if (boardImageUrl) {
    elements.emptyBoardImage.onerror = () => {
      elements.emptyBoardImage.removeAttribute("src");
      elements.emptyVisual.hidden = true;
    };
    elements.emptyBoardImage.src = boardImageSrc();
  } else {
    elements.emptyBoardImage.removeAttribute("src");
  }
  // 开发模式那几颗是固定文案（.prompt-dev），人格配置的起手语只填普通模式那几颗。
  elements.promptGrid.querySelectorAll("[data-prompt]:not(.prompt-dev)").forEach((button, index) => {
    const prompt = state.persona.starter_prompts[index] || DEFAULT_STARTER_PROMPTS[index];
    button.dataset.prompt = prompt;
    const label = button.querySelector("span:last-child");
    if (label) label.textContent = prompt;
  });
  const refreshAssistant = (root) => root.querySelectorAll(".assistant-label").forEach((label) => {
    const name = label.querySelector("strong");
    const avatar = label.querySelector("img");
    if (name) name.textContent = state.persona.name;
    if (avatar) setPersonaAvatar(avatar);
  });
  refreshAssistant(elements.timeline);
  for (const articles of state.finishedTurnArticles.values()) {
    for (const entry of articles) refreshAssistant(entry.article);
  }
}
