import { BRAILLE_FRAMES } from "../../core/constants.js";
import { sessionsInMode } from "./mode.js";
import { multiSessionEnabled, sessionDisplayName, sessionHasRuns } from "./runs.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

/// 开发模式的底部状态栏,tmux 的那一条。
///
/// 会话是 tmux 的「窗口」:`0:名字*`,* 是当前,在跑的挂盲文转圈(和 REPL、
/// 侧栏同一个 BRAILLE_FRAMES,由 list.js 的 startBrailleTicker 统一换帧),跑完
/// 没看的挂 !。Alt+1…9 直接跳。右侧照抄输入框那边的模型名与上下文数字(观察
/// DOM 文本,不另算一份),再加一只时钟。回调由调用方注入,避免和 view.js 互相引用。
const statusState = {
  onOpen: null,
  onNew: null,
  onPalette: null,
  onNormal: null,
  onMenu: null,
  mirrors: null,
  clockTimer: 0
};

export function bindStatusBar(callbacks) {
  Object.assign(statusState, callbacks);
  // 模型名与上下文数字变了,右侧跟着变。
  const observer = new MutationObserver(() => paintMirrors());
  for (const source of [elements.modelLabel, elements.contextNumbers]) {
    if (source) observer.observe(source, { childList: true, characterData: true, subtree: true });
  }
  document.addEventListener("keydown", (event) => {
    if (state.sessionMode !== "dev" || !event.altKey || event.ctrlKey || event.metaKey) return;
    const digit = /^Digit([1-9])$/.exec(event.code);
    if (!digit) return;
    const target = sessionsInMode("dev")[Number(digit[1]) - 1];
    if (!target) return;
    event.preventDefault();
    statusState.onOpen?.(String(target.session_id));
  });
}

function node(tag, className, text) {
  const element = document.createElement(tag);
  if (className) element.className = className;
  if (text != null) element.textContent = text;
  return element;
}

function button(className, text, title, onClick) {
  const element = node("button", className, text);
  element.type = "button";
  if (title) {
    element.title = title;
    element.setAttribute("aria-label", title);
  }
  element.addEventListener("click", onClick);
  return element;
}

function buildWindow(session, index, activeId) {
  const id = String(session.session_id);
  const active = id === activeId;
  const running = sessionHasRuns(id);
  const unread = !running && state.unreadSessions.has(id);
  const name = sessionDisplayName(session);
  const tab = button(`tsb-window${active ? " is-active" : ""}${running ? " is-running" : ""}${unread ? " is-unread" : ""}`, null,
    `${name}${index < 9 ? ` (Alt+${index + 1})` : ""}`, () => statusState.onOpen?.(id));
  tab.setAttribute("role", "tab");
  tab.setAttribute("aria-selected", String(active));
  tab.append(node("span", "tsb-index", `${index}:`), node("span", "tsb-name", name));
  if (running) tab.appendChild(node("span", "tsb-flag session-run-spinner", BRAILLE_FRAMES[0]));
  else tab.appendChild(node("span", "tsb-flag", active ? "*" : unread ? "!" : ""));
  return tab;
}

function paintMirrors() {
  const mirrors = statusState.mirrors;
  if (!mirrors) return;
  mirrors.model.textContent = elements.modelLabel?.textContent?.trim() || "--";
  mirrors.context.textContent = `ctx ${elements.contextNumbers?.textContent?.trim() || "--"}`;
}

function paintClock() {
  const clock = statusState.mirrors?.clock;
  if (!clock) return;
  const now = new Date();
  clock.textContent = `${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`;
}

export function renderStatusBar() {
  const bar = elements.devStatusbar;
  if (!bar) return;
  const show = state.sessionMode === "dev" && multiSessionEnabled();
  bar.hidden = !show;
  document.body.classList.toggle("has-statusbar", show);
  window.clearInterval(statusState.clockTimer);
  if (!show) {
    bar.replaceChildren();
    statusState.mirrors = null;
    return;
  }

  const mode = button("tsb-mode", "DEV", "回到她身边(普通模式)", () => statusState.onNormal?.());
  const menu = button("tsb-menu", "☰", "菜单:控制台、主题、分享文件", (event) => statusState.onMenu?.(event.currentTarget));

  const windows = node("div", "tsb-windows");
  windows.setAttribute("role", "tablist");
  windows.setAttribute("aria-label", "开发会话");
  const activeId = state.switchingToSessionId || state.viewSessionId || "";
  const sessions = sessionsInMode("dev");
  sessions.forEach((session, index) => windows.appendChild(buildWindow(session, index, activeId)));
  // 草稿页也占一个窗口位:「现在在一个还没落地的新窗口里」。
  if (state.draftMode === "dev") {
    const draft = node("span", "tsb-window is-active is-draft");
    draft.append(node("span", "tsb-index", `${sessions.length}:`), node("span", "tsb-name", "新会话"), node("span", "tsb-flag", "*"));
    windows.appendChild(draft);
  }
  windows.appendChild(button("tsb-new", "+", "新会话", () => statusState.onNew?.()));

  const right = node("div", "tsb-right");
  const mac = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent || "");
  const palette = button("tsb-seg tsb-palette", mac ? "⌘K" : "Ctrl K", "搜索、改名与整理会话", () => statusState.onPalette?.());
  const model = node("span", "tsb-seg tsb-model");
  const context = node("span", "tsb-seg tsb-context");
  const clock = node("span", "tsb-seg tsb-clock");
  right.append(palette, model, context, clock);
  statusState.mirrors = { model, context, clock };

  bar.replaceChildren(mode, menu, windows, right);
  paintMirrors();
  paintClock();
  statusState.clockTimer = window.setInterval(paintClock, 20_000);

  // 只横向把当前窗口挪进视野。
  const current = windows.querySelector(".tsb-window.is-active");
  if (current && current.offsetLeft + current.offsetWidth > windows.clientWidth) {
    windows.scrollLeft = current.offsetLeft + current.offsetWidth - windows.clientWidth;
  }
}
