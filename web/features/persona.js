import { DEFAULT_BOARD_SUBTITLE, DEFAULT_BOARD_TITLE, DEFAULT_STARTER_PROMPTS, defaultComposerPlaceholder } from "../core/constants.js";
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

export function applyPersona(value) {
  state.persona = normalizePersona(value);
  elements.brandName.textContent = state.persona.name;
  elements.brandAvatar.alt = state.persona.name;
  setPersonaAvatar(elements.brandAvatar);
  elements.emptyKickerName.textContent = state.persona.name;
  elements.emptyTitle.textContent = state.persona.board_title;
  elements.emptySubtitle.textContent = state.persona.board_subtitle;
  elements.composerInput.placeholder = state.persona.composer_placeholder;
  const boardImageUrl = state.persona.board_image_url;
  elements.emptyVisual.hidden = !boardImageUrl;
  elements.emptyBoardImage.alt = `${state.persona.name} 看板图片`;
  if (boardImageUrl) {
    elements.emptyBoardImage.onerror = () => {
      elements.emptyBoardImage.removeAttribute("src");
      elements.emptyVisual.hidden = true;
    };
    elements.emptyBoardImage.src = `${boardImageUrl}${boardImageUrl.includes("?") ? "&" : "?"}v=${encodeURIComponent(state.persona.revision)}`;
  } else {
    elements.emptyBoardImage.removeAttribute("src");
  }
  elements.promptGrid.querySelectorAll("[data-prompt]").forEach((button, index) => {
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
