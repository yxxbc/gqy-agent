import { closeSidebar } from "../sidebar.js";
import { renderSessionList } from "./list.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

/// 会话面板:会话不再常驻侧栏,收进这个浮层。
///
/// 同一个面板两种样子(CSS 按 body[data-mode] 区分):普通模式是「信匣」,
/// 一叠信笺卡片;开发模式是指令面板,Ctrl+K 呼出、方向键选、回车开。
/// 列表本身仍由 list.js 的 renderSessionList 画,这里只管开关、搜索与键盘。
const switcherState = {
  opener: null,
  cursor: -1
};

export function switcherOpen() {
  return Boolean(elements.sessionSwitcher && !elements.sessionSwitcher.hidden);
}

export function openSessionSwitcher() {
  if (!elements.sessionSwitcher || switcherOpen()) return;
  switcherState.opener = document.activeElement;
  closeSidebar();
  state.sessionFilter = "";
  elements.sessionSearch.value = "";
  syncSwitcherCopy();
  elements.sessionSwitcher.hidden = false;
  elements.sessionSwitcherButton?.setAttribute("aria-expanded", "true");
  renderSessionList();
  // 光标先落在当前会话上,回车就是「留在这里」,方向键从这里出发。
  const items = switcherItems();
  switcherState.cursor = Math.max(0, items.findIndex((item) => item.classList.contains("active")));
  paintCursor({ scroll: true });
  window.requestAnimationFrame(() => elements.sessionSearch.focus());
}

export function closeSessionSwitcher({ restoreFocus = true } = {}) {
  if (!switcherOpen()) return;
  elements.sessionSwitcher.hidden = true;
  elements.sessionSwitcherButton?.setAttribute("aria-expanded", "false");
  state.sessionMenuFor = null;
  if (state.sessionFilter) {
    state.sessionFilter = "";
    renderSessionList();
  }
  if (restoreFocus) switcherState.opener?.focus?.();
  switcherState.opener = null;
}

export function toggleSessionSwitcher() {
  if (switcherOpen()) closeSessionSwitcher();
  else openSessionSwitcher();
}

/// 标题与搜索框的文案跟着模式走。
export function syncSwitcherCopy() {
  const dev = state.sessionMode === "dev";
  if (elements.sessionSwitcherTitle) elements.sessionSwitcherTitle.textContent = dev ? "会话" : "信匣";
  if (elements.sessionSearch) elements.sessionSearch.placeholder = dev ? "跳转到会话…" : "找一封信";
}

function switcherItems() {
  return [...(elements.sessionItems?.querySelectorAll(".session-item[data-session-id]") || [])];
}

/// scroll 只在方向键移动光标时为真:列表重画后补光标不能顺手滚动,否则在
/// 信匣里滚到下面点「…」,重画一补光标(默认在当前会话,也就是第一张)就被
/// 拽回顶上。
function paintCursor({ scroll = false } = {}) {
  const items = switcherItems();
  items.forEach((item, index) => item.classList.toggle("is-cursor", index === switcherState.cursor));
  if (scroll) items[switcherState.cursor]?.scrollIntoView({ block: "nearest" });
}

/// renderSessionList 重画之后把光标补回去(列表是整个重建的)。
export function restoreSwitcherCursor() {
  if (!switcherOpen()) return;
  const count = switcherItems().length;
  switcherState.cursor = count ? Math.min(Math.max(0, switcherState.cursor), count - 1) : -1;
  paintCursor();
}

export function bindSessionSwitcher() {
  if (!elements.sessionSwitcher) return;
  // 入口上的快捷键提示按平台写:Mac 是 ⌘K,别的是 Ctrl K。
  const mac = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent || "");
  const hint = elements.sessionSwitcherButton?.querySelector(".session-entry-key");
  if (hint) hint.textContent = mac ? "⌘K" : "Ctrl K";
  elements.sessionSwitcherButton?.addEventListener("click", toggleSessionSwitcher);
  elements.sessionSwitcherScrim?.addEventListener("click", () => closeSessionSwitcher());
  elements.sessionSwitcherClose?.addEventListener("click", () => closeSessionSwitcher());
  elements.sessionSearch.addEventListener("input", () => {
    state.sessionFilter = elements.sessionSearch.value.trim();
    switcherState.cursor = 0;
    renderSessionList();
    paintCursor();
  });
  elements.sessionSwitcher.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      // 会话菜单或改名开着时,Esc 先交给它们。
      if (state.sessionMenuFor || state.sessionRenaming) return;
      event.preventDefault();
      closeSessionSwitcher();
      return;
    }
    if (event.target !== elements.sessionSearch) return;
    const items = switcherItems();
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!items.length) return;
      const step = event.key === "ArrowDown" ? 1 : -1;
      switcherState.cursor = (switcherState.cursor + step + items.length) % items.length;
      paintCursor({ scroll: true });
    } else if (event.key === "Enter" && !event.isComposing) {
      event.preventDefault();
      items[switcherState.cursor]?.querySelector(".session-item-main")?.click();
    }
  });
}
