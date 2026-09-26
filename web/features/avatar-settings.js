import { apiRequest } from "../core/api.js";
import { makeIconSlot } from "../core/icons.js";
import { showToast } from "../core/toast.js";
import { applyAvatarDisplay, avatarDisplay, makeAvatarFrame, setPersonaAvatar, setUserAvatar, setUserAvatarUrl } from "./persona.js";
import { state } from "../state/store.js";

/// 账号页的「头像」卡:她和我各自的取景(拖动、滚轮/滑块缩放)、对话里的头像
/// 尺寸,下方一段迷你对话实时预览。
///
/// 所有头像读的是 body 上同一组 CSS 变量(persona.js 的 applyAvatarDisplay),
/// 所以这里拖一下,预览、侧栏、对话流里的头像同时跟着动;保存防抖后发给
/// /api/account/avatar-display,按账号落盘。
const SAVE_DELAY_MS = 400;
const MAX_UPLOAD_BYTES = 4 * 1024 * 1024;
const MIN_ZOOM = 1;
const MAX_ZOOM = 4;

const cardState = {
  built: false,
  who: "her",
  saveTimer: 0,
  pending: {},
  refs: {}
};

function node(tag, className, text) {
  const element = document.createElement(tag);
  if (className) element.className = className;
  if (text != null) element.textContent = text;
  return element;
}

function button(label, icon, onClick, className = "secondary-button") {
  const element = node("button", className);
  element.type = "button";
  if (icon) element.appendChild(makeIconSlot(icon));
  element.appendChild(node("span", "", label));
  element.addEventListener("click", onClick);
  return element;
}

function displayKey(who) {
  return who === "her" ? "assistant" : "user";
}

function currentFrame(who = cardState.who) {
  const display = avatarDisplay();
  return who === "her" ? display.assistant : display.user;
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

/// 本地立刻生效,服务端防抖保存。value 为 null 表示恢复默认。
function updateDisplay(key, value) {
  applyAvatarDisplay({ [key]: value });
  cardState.pending[key] = value;
  window.clearTimeout(cardState.saveTimer);
  cardState.saveTimer = window.setTimeout(saveDisplay, SAVE_DELAY_MS);
  paintControls();
}

async function saveDisplay() {
  const body = cardState.pending;
  cardState.pending = {};
  if (!Object.keys(body).length) return;
  try {
    const response = await apiRequest("/api/account/avatar-display", { method: "PUT", body: JSON.stringify(body) });
    const payload = await response.json();
    // 拖动还在继续时别拿旧回执覆盖新值。
    if (state.account && !Object.keys(cardState.pending).length) {
      state.account.avatar_display = payload?.avatar_display || state.account.avatar_display;
      applyAvatarDisplay();
    }
  } catch (error) {
    showToast(error.message || "头像设置保存失败", "error");
  }
}

function setFrame(patch) {
  const next = { ...currentFrame(), ...patch };
  next.zoom = clamp(next.zoom, MIN_ZOOM, MAX_ZOOM);
  next.x = clamp(next.x, -50, 50);
  next.y = clamp(next.y, -50, 50);
  updateDisplay(displayKey(cardState.who), next);
}

/// 拖动:手往右拖,图跟着往右走,露出的是左边——焦点往左移。放大后同样的
/// 手势对应更小的焦点位移,手感才跟得上。
function bindEditor(stage) {
  let drag = null;
  stage.addEventListener("pointerdown", (event) => {
    if (!hasImage(cardState.who)) return;
    const frame = currentFrame();
    drag = { id: event.pointerId, x: event.clientX, y: event.clientY, start: frame, size: stage.clientWidth || 1 };
    stage.setPointerCapture(event.pointerId);
    stage.classList.add("is-dragging");
  });
  stage.addEventListener("pointermove", (event) => {
    if (!drag || event.pointerId !== drag.id) return;
    const scale = 100 / drag.size / drag.start.zoom;
    setFrame({
      x: drag.start.x - (event.clientX - drag.x) * scale,
      y: drag.start.y - (event.clientY - drag.y) * scale
    });
  });
  const end = (event) => {
    if (!drag || event.pointerId !== drag.id) return;
    drag = null;
    stage.classList.remove("is-dragging");
  };
  stage.addEventListener("pointerup", end);
  stage.addEventListener("pointercancel", end);
  stage.addEventListener("wheel", (event) => {
    if (!hasImage(cardState.who)) return;
    event.preventDefault();
    setFrame({ zoom: currentFrame().zoom * Math.exp(-event.deltaY * 0.0015) });
  }, { passive: false });
  // 键盘:方向键挪焦点,+/- 缩放。
  stage.addEventListener("keydown", (event) => {
    const frame = currentFrame();
    const step = event.shiftKey ? 8 : 2;
    const moves = { ArrowLeft: { x: frame.x - step }, ArrowRight: { x: frame.x + step }, ArrowUp: { y: frame.y - step }, ArrowDown: { y: frame.y + step } };
    if (moves[event.key]) setFrame(moves[event.key]);
    else if (event.key === "+" || event.key === "=") setFrame({ zoom: frame.zoom + 0.1 });
    else if (event.key === "-") setFrame({ zoom: frame.zoom - 0.1 });
    else return;
    event.preventDefault();
  });
}

function hasImage(who) {
  return who === "her" ? Boolean(state.persona?.avatar_url) : Boolean(state.account?.avatar_url);
}

async function uploadAvatar(file) {
  if (!file) return;
  if (file.size > MAX_UPLOAD_BYTES) {
    showToast("头像不能超过 4 MiB", "error");
    return;
  }
  const { upload } = cardState.refs;
  upload.disabled = true;
  try {
    const response = await apiRequest("/api/account/avatar", {
      method: "PUT",
      headers: { "Content-Type": file.type || "application/octet-stream" },
      body: file
    });
    const payload = await response.json();
    setUserAvatarUrl(payload?.avatar_url || null);
    // 新图从正中开始取景,旧图的焦点对新图没有意义。
    updateDisplay("user", null);
    showToast("头像已更新");
  } catch (error) {
    showToast(error.message || "头像上传失败", "error");
  } finally {
    upload.disabled = false;
    paint();
  }
}

async function removeAvatar() {
  try {
    await apiRequest("/api/account/avatar", { method: "DELETE" });
    setUserAvatarUrl(null);
    updateDisplay("user", null);
    showToast("头像已移除");
  } catch (error) {
    showToast(error.message || "移除失败", "error");
  }
  paint();
}

function build(root) {
  const refs = cardState.refs;
  const head = node("div", "u-card-head");
  head.append(node("h3", "", "头像"), node("span", "u-hint", "拖动取景,滚轮或滑块缩放,改动即时生效"));

  const tabs = node("div", "avatar-tabs");
  tabs.setAttribute("role", "tablist");
  refs.tabs = ["her", "me"].map((who) => {
    const tab = node("button", "avatar-tab");
    tab.type = "button";
    tab.setAttribute("role", "tab");
    tab.dataset.who = who;
    tab.addEventListener("click", () => {
      cardState.who = who;
      paint();
    });
    tabs.appendChild(tab);
    return tab;
  });

  const stage = node("div", "avatar-editor");
  stage.tabIndex = 0;
  stage.setAttribute("role", "img");
  refs.herFrame = makeAvatarFrame("her");
  refs.meFrame = makeAvatarFrame("me");
  refs.empty = node("span", "avatar-editor-empty", "还没有头像");
  stage.append(refs.herFrame, refs.meFrame, refs.empty);
  bindEditor(stage);
  refs.stage = stage;

  const zoom = node("input", "avatar-range");
  zoom.type = "range";
  zoom.min = String(MIN_ZOOM);
  zoom.max = String(MAX_ZOOM);
  zoom.step = "0.01";
  zoom.setAttribute("aria-label", "缩放");
  zoom.addEventListener("input", () => setFrame({ zoom: Number(zoom.value) }));
  refs.zoom = zoom;

  const size = node("input", "avatar-range");
  size.type = "range";
  size.min = "24";
  size.max = "44";
  size.step = "1";
  size.setAttribute("aria-label", "对话里的头像大小");
  size.addEventListener("input", () => updateDisplay("size", Number(size.value)));
  refs.size = size;
  refs.sizeValue = node("output", "avatar-range-value");
  refs.zoomValue = node("output", "avatar-range-value");

  const picker = node("input");
  picker.type = "file";
  picker.accept = "image/png,image/jpeg,image/webp,image/gif";
  picker.hidden = true;
  picker.addEventListener("change", () => {
    uploadAvatar(picker.files?.[0]);
    picker.value = "";
  });
  refs.upload = button("上传头像", "user-plus", () => picker.click());
  refs.remove = button("移除", "trash-2", removeAvatar, "secondary-button is-danger");
  refs.reset = button("重置取景", "rotate-ccw", () => updateDisplay(displayKey(cardState.who), null));
  refs.herHint = node("p", "avatar-hint", "她的头像图片在「设置 › 人格 › 看板」里更换,这里只调取景。");

  const labelled = (label, input, output) => {
    const row = node("label", "avatar-control");
    row.append(node("span", "", label), input, output);
    return row;
  };
  const controls = node("div", "avatar-controls");
  const actions = node("div", "avatar-actions");
  actions.append(refs.upload, refs.remove, refs.reset, picker);
  controls.append(tabs, labelled("缩放", zoom, refs.zoomValue), labelled("对话头像大小", size, refs.sizeValue), actions, refs.herHint);

  const editorRow = node("div", "avatar-editor-row");
  editorRow.append(stage, controls);

  // 实时预览:用和对话流同一组变量,所见即所得。
  const preview = node("div", "avatar-preview");
  preview.setAttribute("aria-label", "预览");
  const herRow = node("div", "avatar-preview-row is-her");
  const herCopy = node("div", "avatar-preview-copy");
  refs.previewHerName = node("strong", "avatar-preview-name");
  herCopy.append(refs.previewHerName, node("p", "", "今天也辛苦啦。晚饭想吃点什么?"));
  herRow.append(makeAvatarFrame("her"), herCopy);
  const meRow = node("div", "avatar-preview-row is-me");
  meRow.append(node("p", "avatar-preview-bubble", "想吃点热乎的,你帮我挑一家吧"), makeAvatarFrame("me"));
  preview.append(node("span", "avatar-preview-label", "预览"), herRow, meRow);
  refs.preview = preview;

  root.replaceChildren(head, editorRow, preview);
  cardState.built = true;
}

function paintControls() {
  const refs = cardState.refs;
  if (!cardState.built) return;
  const frame = currentFrame();
  const display = avatarDisplay();
  refs.zoom.value = String(frame.zoom);
  refs.zoomValue.textContent = `${frame.zoom.toFixed(2)}×`;
  refs.size.value = String(display.size);
  refs.sizeValue.textContent = `${display.size}px`;
}

function paint() {
  const refs = cardState.refs;
  if (!cardState.built) return;
  const who = cardState.who;
  const herName = state.persona?.name || "她";
  const myName = state.account?.display_name || state.account?.username || "我";
  refs.tabs[0].textContent = herName;
  refs.tabs[1].textContent = myName;
  for (const tab of refs.tabs) {
    const active = tab.dataset.who === who;
    tab.classList.toggle("is-active", active);
    tab.setAttribute("aria-selected", String(active));
  }
  refs.herFrame.hidden = who !== "her";
  refs.meFrame.hidden = who !== "me";
  const present = hasImage(who);
  refs.empty.hidden = present;
  refs.stage.classList.toggle("is-empty", !present);
  refs.stage.setAttribute("aria-label", `${who === "her" ? herName : myName}的头像取景`);
  refs.zoom.disabled = !present;
  refs.reset.disabled = !present;
  refs.upload.hidden = who !== "me";
  refs.upload.querySelector("span:last-child").textContent = state.account?.avatar_url ? "更换头像" : "上传头像";
  refs.remove.hidden = who !== "me" || !state.account?.avatar_url;
  refs.herHint.hidden = who !== "her";
  refs.previewHerName.textContent = herName;
  // 图片地址可能变过(换人格、刚上传):卡里的几张图重新取一次。
  document.querySelectorAll("#avatarCard .avatar-frame.is-her img").forEach(setPersonaAvatar);
  document.querySelectorAll("#avatarCard .avatar-frame.is-me img").forEach(setUserAvatar);
  paintControls();
}

/// 账号面板打开时调:第一次搭骨架,之后按当前人格与账号刷新。
export function syncAvatarCard() {
  const root = document.getElementById("avatarCard");
  if (!root) return;
  if (!cardState.built) build(root);
  paint();
}
