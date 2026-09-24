import { apiRequest } from "../core/api.js";
import { createIcon } from "../core/icons.js";
import { safeStorageGet, safeStorageSet } from "../core/storage.js";
import { artifactTextScale } from "../core/ui-scale.js";
import { procLineSetOpen } from "./conversation/proc-rail.js";
import { elements } from "../state/elements.js";
import { state } from "../state/store.js";

/// 只有本模块用的状态（从 state/store.js 分出来的私有分片）。
const appearanceState = {
  colorScheme: null,
  uiPrefs: {},
  matugenAvailable: null
};

/*
 * 外观偏好存在 daemon 那边。localStorage 按 **origin** 隔离:
 * http://127.0.0.1:8300 和 http://192.168.1.7:8300 是两个源,同一台 顾清影 换个
 * 地址进来就是另一份主题——「顾清影 长什么样」不该跟着浏览器地址栏走。
 * 本地那份仍然写:它是首帧的即时值,服务端那份要等一个来回,先按本地上色能
 * 免掉一次闪烁。窗口尺寸相关的偏好(侧栏折叠、分栏比例)故意不同步,手机和
 * 台式机本来就该不一样。
 */
export const UI_PREF_KEYS = ["theme", "colorScheme", "chatFontSize", "reasoningExpanded", "toolExpanded", "procCollapse"];

export function saveUiPref(key, value) {
  if (!UI_PREF_KEYS.includes(key)) return;
  if (appearanceState.uiPrefs[key] === value) return;
  appearanceState.uiPrefs[key] = value;
  apiRequest("/api/ui-prefs", { method: "PUT", body: JSON.stringify({ [key]: value }) }).catch(() => {});
}

/** 登录之后拉一次服务端偏好并应用。失败就维持本地那份,不打扰用户。 */
export async function syncUiPrefs() {
  let prefs;
  try {
    prefs = await (await apiRequest("/api/ui-prefs")).json();
  } catch (_) {
    return;
  }
  if (!prefs || typeof prefs !== "object") return;
  // 先记下服务端的值:下面几个 setter 会走 saveUiPref,记过就不会再发回去。
  appearanceState.uiPrefs = { ...prefs };
  if (prefs.theme) setTheme(prefs.theme);
  if (prefs.colorScheme) setColorScheme(prefs.colorScheme);
  if (prefs.chatFontSize) setChatFontSize(prefs.chatFontSize);
  if (prefs.reasoningExpanded) setReasoningExpanded(prefs.reasoningExpanded === "true");
  if (prefs.toolExpanded) setToolExpanded(prefs.toolExpanded === "true");
  if (prefs.procCollapse) setProcCollapse(prefs.procCollapse === "true");
}

export function setTheme(theme, persist = true) {
  const selected = theme === "linen" ? "linen" : "graphite";
  elements.body.dataset.theme = selected;
  document.querySelectorAll("[data-theme-choice]").forEach((button) => {
    button.classList.toggle("selected", button.dataset.themeChoice === selected);
    button.setAttribute("aria-pressed", String(button.dataset.themeChoice === selected));
  });
  const nextIcon = selected === "graphite" ? "sun" : "moon";
  for (const button of [elements.sidebarThemeButton]) {
    const slot = button.querySelector(".icon-slot");
    slot.replaceChildren(createIcon(nextIcon));
    button.title = selected === "graphite" ? "切换到晨光主题" : "切换到夜阑主题";
    button.setAttribute("aria-label", button.title);
  }
  const themeColor = document.querySelector('meta[name="theme-color"]');
  if (themeColor) themeColor.content = selected === "graphite" ? "#171821" : "#f6f0e2";
  if (persist) {
    safeStorageSet("gqy.web.theme", selected);
    saveUiPref("theme", selected);
  }
}

/*
 * 配色方案(与明暗正交):
 * - madobe  窗边预设(logo 派生 token,styles.css 内置)
 * - matugen 壁纸取色(后端 /theme.css 输出整套 MD3 token)
 * 通过禁用 /theme.css 的 <link> 切换,不改后端与 matugen 模板。
 */
export function setColorScheme(scheme, persist = true) {
  const requested = scheme === "madobe" ? "madobe" : "matugen";
  const selected = requested === "matugen" && appearanceState.matugenAvailable === false ? "madobe" : requested;
  appearanceState.colorScheme = selected;
  elements.body.dataset.colorScheme = selected;
  if (elements.matugenThemeLink) elements.matugenThemeLink.disabled = selected !== "matugen";
  document.querySelectorAll("[data-scheme-choice]").forEach((button) => {
    const active = button.dataset.schemeChoice === selected;
    button.classList.toggle("selected", active);
    button.setAttribute("aria-pressed", String(active));
    // 探测不到 matugen 输出时,「壁纸取色」整个选项不显示。
    if (button.dataset.schemeChoice === "matugen") button.hidden = appearanceState.matugenAvailable !== true;
  });
  if (persist) {
    safeStorageSet("gqy.web.colorScheme", requested);
    saveUiPref("colorScheme", requested);
  }
}

export async function probeMatugenTheme() {
  try {
    const response = await fetch("/theme.css", { method: "HEAD", cache: "no-store" });
    appearanceState.matugenAvailable = response.ok;
  } catch (_) {
    appearanceState.matugenAvailable = false;
  }
  // 无持久化记录时:matugen 可用则维持现状(matugen),否则窗边。默认值不写入存储。
  setColorScheme(safeStorageGet("gqy.web.colorScheme") || (appearanceState.matugenAvailable ? "matugen" : "madobe"), false);
}

/* 仅 WebUI 的本地显示偏好(localStorage,不写入 config) */
export const CHAT_FONT_SIZES = ["14px", "15px", "16px"];

export function setChatFontSize(size, persist = true) {
  const selected = CHAT_FONT_SIZES.includes(size) ? size : "15px";
  document.documentElement.style.setProperty("--fs-chat", selected);
  document.documentElement.style.setProperty("--fs-artifact-chat", `${Number.parseFloat(selected) * artifactTextScale()}px`);
  document.querySelectorAll("[data-chat-font]").forEach((button) => {
    const active = button.dataset.chatFont === selected;
    button.classList.toggle("active", active);
    button.setAttribute("aria-pressed", String(active));
  });
  if (persist) {
    safeStorageSet("gqy.web.chatFontSize", selected);
    saveUiPref("chatFontSize", selected);
  }
}

export function setReasoningExpanded(value, persist = true) {
  state.reasoningExpanded = Boolean(value);
  elements.reasoningExpandToggle?.setAttribute("aria-checked", String(state.reasoningExpanded));
  // 对已渲染的思考块即时生效
  document.querySelectorAll(".reasoning-block").forEach((block) => {
    block.open = state.reasoningExpanded;
  });
  if (persist) {
    safeStorageSet("gqy.web.reasoningExpanded", String(state.reasoningExpanded));
    saveUiPref("reasoningExpanded", String(state.reasoningExpanded));
  }
}

export function setToolExpanded(value, persist = true) {
  state.toolExpanded = Boolean(value);
  elements.toolExpandToggle?.setAttribute("aria-checked", String(state.toolExpanded));
  // 对已渲染的工具签即时生效
  document.querySelectorAll(".tool-card").forEach((card) => {
    card.classList.toggle("collapsed", !state.toolExpanded);
    card.querySelector(".tool-head")?.setAttribute("aria-expanded", String(state.toolExpanded));
  });
  if (persist) {
    safeStorageSet("gqy.web.toolExpanded", String(state.toolExpanded));
    saveUiPref("toolExpanded", String(state.toolExpanded));
  }
}

export function setProcCollapse(value, persist = true) {
  state.procCollapse = Boolean(value);
  elements.procCollapseToggle?.setAttribute("aria-checked", String(state.procCollapse));
  // 对已经切断的时间线即时生效:开 → 露出总结行并收起;关 → 藏掉总结行并展开
  document.querySelectorAll(".proc-line").forEach((line) => {
    if (!line.gqyProc?.closed) return;
    line.gqyProc.head.hidden = !state.procCollapse;
    procLineSetOpen(line, !state.procCollapse);
  });
  if (persist) {
    safeStorageSet("gqy.web.procCollapse", String(state.procCollapse));
    saveUiPref("procCollapse", String(state.procCollapse));
  }
}
