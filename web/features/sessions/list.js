import { apiRequest } from "../../core/api.js";
import { BRAILLE_FRAMES } from "../../core/constants.js";
import { firstLine, formatRelativeTime } from "../../core/format.js";
import { makeIconSlot } from "../../core/icons.js";
import { visualPixelsToLayout } from "../../core/ui-scale.js";
import { showToast } from "../../core/toast.js";
import { updateConversationChrome } from "../conversation/chrome.js";
import { scrollToBottom } from "../conversation/scroll.js";
import { requestClearConversation } from "../session-mode.js";
import { deriveConversationDetails, findSession, multiSessionEnabled, sessionDisplayName, sessionHasRuns } from "./runs.js";
import { deleteSession, openSessionView, refreshSessions } from "./view.js";
import { sessionsInMode } from "./mode.js";
import { closeSessionSwitcher, restoreSwitcherCursor } from "./switcher.js";
import { renderStatusBar } from "./statusbar.js";
import { syncHerRoom } from "../her-room.js";
import { closeSidebar } from "../sidebar.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

/// 只有本模块用的状态（从 state/store.js 分出来的私有分片）。
const listState = {
  sessionDragId: null,
  brailleFrame: 0
};

export function closeSessionMenu() {
  if (!state.sessionMenuFor) return;
  state.sessionMenuFor = null;
  renderSessionList();
}

export function toggleSessionMenu(sessionId) {
  state.sessionMenuFor = state.sessionMenuFor === sessionId ? null : sessionId;
  renderSessionList();
  if (!state.sessionMenuFor) return;
  const menu = placeSessionMenu();
  if (menu) window.requestAnimationFrame(() => menu.querySelector("button")?.focus());
}

/// 会话菜单钉在视口上,贴着「…」按钮弹出。
///
/// 菜单不放进会话卡片,挂在会话面板根节点上。放在卡片里时,信笺悬停上浮的
/// transform 会让卡片自成层叠上下文、并成为 fixed 的参照系:菜单的 z-index
/// 只在这张卡片里算数,被后面的卡片盖住,鼠标一离开卡片还会整块跳位;面板
/// 的滚动容器也会把它裁掉。挂到面板根上就只和面板比高低,放不下就往上翻。
/// 列表每次重画都会重建菜单,所以重画后也要再摆一次。
export function placeSessionMenu() {
  removeSessionMenu();
  if (!state.sessionMenuFor) return null;
  const item = elements.sessionItems.querySelector(`.session-item[data-session-id="${CSS.escape(state.sessionMenuFor)}"]`);
  const anchor = item?.querySelector(".session-menu-button");
  const session = findSession(state.sessionMenuFor);
  if (!anchor || !session) return null;
  const menu = buildSessionMenu(session, state.sessionMenuFor === "default");
  (elements.sessionSwitcher || document.body).appendChild(menu);
  // 外壳有 zoom(--ui-scale):矩形是视觉像素,fixed 的坐标要换回布局像素。
  const rect = anchor.getBoundingClientRect();
  const button = {
    top: visualPixelsToLayout(rect.top),
    right: visualPixelsToLayout(rect.right),
    bottom: visualPixelsToLayout(rect.bottom)
  };
  const width = menu.offsetWidth;
  const height = menu.offsetHeight;
  const viewportWidth = visualPixelsToLayout(document.documentElement.clientWidth);
  const viewportHeight = visualPixelsToLayout(document.documentElement.clientHeight);
  const left = Math.max(8, Math.min(button.right - width, viewportWidth - width - 8));
  const below = button.bottom + 4;
  const top = below + height > viewportHeight - 8 ? Math.max(8, button.top - height - 4) : below;
  menu.style.left = `${left}px`;
  menu.style.top = `${top}px`;
  // 万一祖先带了 transform,fixed 的参照系就是那个祖先而不是视口。
  // 量一下实际落点,差多少补多少。
  const placed = menu.getBoundingClientRect();
  const driftX = visualPixelsToLayout(placed.left) - left;
  const driftY = visualPixelsToLayout(placed.top) - top;
  if (Math.abs(driftX) > 0.5 || Math.abs(driftY) > 0.5) {
    menu.style.left = `${left - driftX}px`;
    menu.style.top = `${top - driftY}px`;
  }
  return menu;
}

function removeSessionMenu() {
  for (const menu of document.querySelectorAll(".session-menu")) menu.remove();
}

export function beginSessionRename(sessionId) {
  state.sessionRenaming = sessionId;
  renderSessionList();
}

export function cancelSessionRename() {
  state.sessionRenaming = null;
  renderSessionList();
}

export async function commitSessionRename(sessionId, value) {
  if (state.sessionRenaming !== sessionId) return;
  state.sessionRenaming = null;
  const session = findSession(sessionId);
  const name = String(value || "").trim();
  if (!session || !name || name === String(session.name || "").trim()) {
    renderSessionList();
    return;
  }
  try {
    await apiRequest(`/api/sessions/${encodeURIComponent(sessionId)}`, {
      method: "PATCH",
      body: JSON.stringify({ name })
    });
    session.name = name;
    showToast("会话已重命名");
  } catch (error) {
    showToast(error.message || "重命名失败", "error");
  }
  renderSessionList();
  if (sessionId === state.viewSessionId) updateConversationChrome();
}

export function buildSessionMenu(session, isDefault) {
  const id = String(session?.session_id || "");
  const menu = document.createElement("div");
  menu.className = "session-menu";
  menu.setAttribute("role", "menu");
  menu.setAttribute("aria-label", `会话操作：${sessionDisplayName(session)}`);
  // 终端集成会话是固定入口:不可改名、不可删除、不可被顶替,
  // 菜单只留「清空对话」;其余会话不再提供「设为默认」。
  const actions = [];
  if (!isDefault) actions.push({ label: "重命名", handler: () => beginSessionRename(id) });
  // 清空对本来只给默认会话（它不能改名/删除，拿这个顶位），可普通会话一样
  // 需要「留着会话、只丢历史」——删掉重建会连模型/工作目录覆盖一起丢。
  actions.push({ label: "清空对话", handler: () => requestClearConversation(id) });
  if (!isDefault) actions.push({ label: "删除", danger: true, handler: () => deleteSession(id) });
  for (const action of actions) {
    const button = document.createElement("button");
    button.type = "button";
    button.setAttribute("role", "menuitem");
    if (action.danger) button.classList.add("is-danger");
    button.textContent = action.label;
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      closeSessionMenu();
      action.handler();
    });
    menu.appendChild(button);
  }
  return menu;
}

// 终端集成会话（固定 id "default"）不在侧栏列出：它是 shellhook 那条车道，
// 由终端驱动，在 WebUI 的会话列表里既不该被误点进去、更不该被误删。真要看
// 它的历史，用 REPL 的 /session 切过去。
export function isTerminalSession(sessionId) {
  return String(sessionId || "") === "default";
}

export function buildSessionItem(session) {
  const id = String(session?.session_id || "");
  const isView = Boolean(id) && (id === state.viewSessionId || id === state.switchingToSessionId);
  // 终端集成会话固定为 id "default",不再跟随可变的全局指针。
  const isDefault = id === "default";
  const item = document.createElement("div");
  item.className = `session-item${isView ? " active" : ""}`;
  item.dataset.sessionId = id;
  // 花笺右下角那枚小印:她名字的最后一个字(信匣样式里用)。
  item.dataset.seal = Array.from(String(state.persona?.name || "影")).pop() || "影";

  const renaming = state.sessionRenaming === id;
  // 侧栏拖拽排序(组内):HTML5 DnD,drop 时全量提交新顺序。
  if (!renaming) attachSessionDrag(item, session, id);
  const main = document.createElement(renaming ? "div" : "button");
  main.className = `session-item-main${renaming ? " is-renaming" : ""}`;
  if (!renaming) {
    main.type = "button";
    main.title = isView ? sessionDisplayName(session) : `查看「${sessionDisplayName(session)}」`;
    main.addEventListener("click", () => {
      closeSessionSwitcher({ restoreFocus: false });
      openSessionView(id);
    });
  }
  // 行首那一格只放状态指示器。模式图标搬去了分组标题——同一组里每行都
  // 画一遍相同的图标，重复十几次也说不出新东西，还占着状态该用的位置。
  // 空着的时候格子仍在，文字左缘不会因为有没有指示器而移位。
  const lead = document.createElement("span");
  lead.className = "session-lead";
  if (sessionHasRuns(id)) {
    const spinner = document.createElement("span");
    spinner.className = "session-run-spinner";
    spinner.title = "有回复正在运行";
    spinner.textContent = BRAILLE_FRAMES[listState.brailleFrame % BRAILLE_FRAMES.length];
    lead.appendChild(spinner);
  } else if (state.unreadSessions.has(id)) {
    const dot = document.createElement("span");
    dot.className = "session-unread-dot";
    dot.title = "有未读的新回复";
    lead.appendChild(dot);
  }
  main.appendChild(lead);

  const copy = document.createElement("span");
  copy.className = "session-copy";
  if (renaming) {
    const input = document.createElement("input");
    input.className = "session-rename-input";
    input.type = "text";
    input.value = String(session?.name || "");
    input.maxLength = 200;
    input.setAttribute("aria-label", "会话名称");
    input.addEventListener("click", (event) => event.stopPropagation());
    input.addEventListener("keydown", (event) => {
      event.stopPropagation();
      if (event.key === "Enter") {
        event.preventDefault();
        commitSessionRename(id, input.value);
      } else if (event.key === "Escape") {
        event.preventDefault();
        cancelSessionRename();
      }
    });
    input.addEventListener("blur", () => {
      if (state.sessionRenaming === id) commitSessionRename(id, input.value);
    });
    copy.appendChild(input);
    window.requestAnimationFrame(() => {
      input.focus();
      input.select();
    });
  } else {
    const titleRow = document.createElement("span");
    titleRow.className = "session-title-row";
    const title = document.createElement("strong");
    title.textContent = sessionDisplayName(session);
    titleRow.appendChild(title);
    if (isDefault) {
      const badge = document.createElement("span");
      badge.className = "session-default-badge";
      badge.textContent = "默认";
      badge.title = "CLI 与快捷入口的默认会话";
      titleRow.appendChild(badge);
    }
    copy.appendChild(titleRow);
    // 信笺卡片上的落款日期与首句(开发模式的面板里画成一行等宽的时间)。
    const when = session?.updated_at || session?.created_at;
    if (when) {
      const date = document.createElement("time");
      date.className = "session-date";
      date.dateTime = String(when);
      date.textContent = formatRelativeTime(when);
      copy.appendChild(date);
    }
    const snippet = firstLine(session?.last_user_content || "");
    if (snippet) {
      const line = document.createElement("span");
      line.className = "session-snippet";
      line.textContent = snippet;
      copy.appendChild(line);
    }
  }

  // Gemini-style list rows: name only; details live in the hover tooltip.
  if (!renaming) {
    const snippet = firstLine(session?.last_user_content || "");
    const sandbox = String(session?.sandbox || "").trim();
    const details = [snippet, sandbox ? `sandbox: ${sandbox}` : ""].filter(Boolean).join("\n");
    if (details) {
      main.title = `${sessionDisplayName(session)}\n${details}`;
    }
  }

  main.appendChild(copy);
  item.appendChild(main);

  const trailing = document.createElement("span");
  trailing.className = "session-trailing";

  const menuButton = document.createElement("button");
  menuButton.type = "button";
  menuButton.className = "session-menu-button";
  menuButton.title = "会话操作";
  menuButton.setAttribute("aria-label", `会话操作：${sessionDisplayName(session)}`);
  menuButton.setAttribute("aria-haspopup", "menu");
  menuButton.setAttribute("aria-expanded", String(state.sessionMenuFor === id));
  menuButton.appendChild(makeIconSlot("ellipsis"));
  menuButton.addEventListener("click", (event) => {
    event.stopPropagation();
    toggleSessionMenu(id);
  });
  trailing.appendChild(menuButton);
  item.appendChild(trailing);
  return item;
}

export function buildFallbackSessionItem() {
  const details = deriveConversationDetails();
  const item = document.createElement("div");
  item.className = "session-item active";
  const main = document.createElement("button");
  main.type = "button";
  main.className = "session-item-main";
  main.title = details.title;
  main.appendChild(makeIconSlot("message-circle"));
  const copy = document.createElement("span");
  copy.className = "session-copy";
  const title = document.createElement("strong");
  title.textContent = details.title;
  const snippet = document.createElement("small");
  snippet.className = "session-snippet";
  snippet.textContent = details.snippet;
  snippet.title = details.snippet;
  copy.append(title, snippet);
  main.appendChild(copy);
  main.addEventListener("click", () => {
    closeSidebar();
    scrollToBottom({ force: true, smooth: true });
  });
  item.appendChild(main);
  const trailing = document.createElement("span");
  trailing.className = "session-trailing";
  const time = document.createElement("span");
  time.className = "session-time";
  time.textContent = details.timestamp ? formatRelativeTime(details.timestamp) : "";
  trailing.appendChild(time);
  item.appendChild(trailing);
  return item;
}

export function renderSessionList() {
  if (!elements.sessionItems) return;
  if (state.sessionRenaming && elements.sessionItems.querySelector(".session-rename-input")) return;
  // 整列重建会让滚动容器先塌成空的,scrollTop 被夹回 0——在信匣里滚到下面
  // 点「…」,列表一重画就跳回顶上,菜单也就挂到了看不见的按钮上。先记下再还原。
  const scrollTop = elements.sessionList?.scrollTop || 0;
  elements.sessionItems.replaceChildren();
  if (!multiSessionEnabled() || state.sessions.length === 0) {
    elements.sessionItems.appendChild(buildFallbackSessionItem());
    removeSessionMenu();
    return;
  }
  // 侧栏只列当前模式的会话（左上角开关切换，模式创建时定死）。终端集成会话
  // 不列出——它是 shellhook 那条车道,由终端驱动,WebUI 里既不该被误点进去
  // 也不该被误删;要看它的历史用 REPL 的 /session 切过去。
  const all = sessionsInMode(state.sessionMode);
  const visible = filterSessions(all, state.sessionFilter);
  if (!visible.length) {
    elements.sessionItems.appendChild(all.length ? buildNoMatchHint() : buildModeEmptyHint(state.sessionMode));
  } else {
    for (const session of visible) elements.sessionItems.appendChild(buildSessionItem(session));
  }
  if (elements.sessionList) elements.sessionList.scrollTop = scrollTop;
  syncSessionEntry(all);
  renderStatusBar();
  restoreSwitcherCursor();
  placeSessionMenu();
}

/// 面板里的搜索:名字或最后一句里含关键字就留下,大小写不敏感。
function filterSessions(sessions, query) {
  const needle = String(query || "").trim().toLowerCase();
  if (!needle) return sessions;
  return sessions.filter((session) => [sessionDisplayName(session), session?.last_user_content || ""]
    .some((text) => String(text).toLowerCase().includes(needle)));
}

/// 侧栏的会话入口:数量 + 有没有未读(面板收起来了,未读得在入口上看得见)。
function syncSessionEntry(sessions) {
  if (!elements.sessionEntryCount) return;
  elements.sessionEntryCount.textContent = String(sessions.length);
  const unread = sessions.some((session) => state.unreadSessions.has(String(session.session_id)));
  elements.sessionEntryUnread.hidden = !unread;
  elements.sessionEntryLabel.textContent = state.sessionMode === "dev" ? "会话" : "信匣";
  // 相识天数从会话列表里算,列表一变就跟着更新。
  syncHerRoom();
}

function buildNoMatchHint() {
  const hint = document.createElement("p");
  hint.className = "session-mode-empty";
  hint.textContent = "没有匹配的会话";
  return hint;
}

/// 一个计时器喂所有转圈。
///
/// 每个转圈各起一个 interval 的话,列表一重画就要收拾一批计时器,漏一个就
/// 是一个永远跑下去的定时器;而且各自起跑点不同,几行并排时相位乱跳。
/// 共用一个帧号还有个好处:重画时新建的元素直接落在当前帧上,不会从头闪。
export function startBrailleTicker() {
  window.setInterval(() => {
    if (document.hidden) return;
    const spinners = document.querySelectorAll(".session-run-spinner");
    if (!spinners.length) return;
    listState.brailleFrame = (listState.brailleFrame + 1) % BRAILLE_FRAMES.length;
    const glyph = BRAILLE_FRAMES[listState.brailleFrame];
    for (const spinner of spinners) spinner.textContent = glyph;
  }, 90);
}

export function clearSessionDropMarkers() {
  if (!elements.sessionItems) return;
  for (const el of elements.sessionItems.querySelectorAll(".drop-before, .drop-after")) {
    el.classList.remove("drop-before", "drop-after");
  }
}

export function attachSessionDrag(item, session, id) {
  item.draggable = true;
  item.addEventListener("dragstart", (event) => {
    listState.sessionDragId = id;
    item.classList.add("is-dragging");
    event.dataTransfer.effectAllowed = "move";
    try { event.dataTransfer.setData("text/plain", id); } catch (_) { /* 老内核 */ }
  });
  item.addEventListener("dragend", () => {
    listState.sessionDragId = null;
    item.classList.remove("is-dragging");
    clearSessionDropMarkers();
  });
  item.addEventListener("dragover", (event) => {
    const dragId = listState.sessionDragId;
    if (!dragId || dragId === id) return;
    // 只在同一分组(普通/dev)内排序,跨组语义(改会话模式)不存在。
    const dragging = findSession(dragId);
    if (!dragging || (dragging?.mode === "dev") !== (session?.mode === "dev")) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
    const rect = item.getBoundingClientRect();
    const before = event.clientY < rect.top + rect.height / 2;
    clearSessionDropMarkers();
    item.classList.add(before ? "drop-before" : "drop-after");
  });
  item.addEventListener("dragleave", (event) => {
    if (event.relatedTarget && item.contains(event.relatedTarget)) return;
    item.classList.remove("drop-before", "drop-after");
  });
  item.addEventListener("drop", (event) => {
    const dragId = listState.sessionDragId;
    if (!dragId || dragId === id) return;
    event.preventDefault();
    const before = item.classList.contains("drop-before");
    clearSessionDropMarkers();
    listState.sessionDragId = null;
    commitSessionReorder(dragId, id, before);
  });
}

export async function commitSessionReorder(dragId, targetId, before) {
  const list = state.sessions;
  const from = list.findIndex((s) => String(s?.session_id) === String(dragId));
  if (from < 0) return;
  const [moved] = list.splice(from, 1);
  let to = list.findIndex((s) => String(s?.session_id) === String(targetId));
  if (to < 0) {
    list.splice(from, 0, moved);
    return;
  }
  list.splice(before ? to : to + 1, 0, moved);
  renderSessionList();
  // 全量提交当前顺序(两组按数组序混排;后端按序重写 sort_key,分组是
  // 前端展示层的事)。终端车道会话不参与。
  const ids = list
    .filter((s) => !isTerminalSession(s?.session_id))
    .map((s) => String(s.session_id));
  state.lastReorderIds = ids.join("\n");
  try {
    await apiRequest("/api/sessions/order", {
      method: "PUT",
      body: JSON.stringify({ session_ids: ids })
    });
  } catch (error) {
    showToast(error.message || "排序保存失败", "error");
    refreshSessions();
  }
}

function buildModeEmptyHint(mode) {
  const hint = document.createElement("p");
  hint.className = "session-mode-empty";
  hint.textContent = mode === "dev" ? "还没有开发会话" : "还没有对话";
  return hint;
}
