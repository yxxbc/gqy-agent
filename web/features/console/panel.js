import { apiRequest } from "../../core/api.js";
import { loadAccountPanel } from "../accounts.js";
import { isAdmin, isAdminOnlyPanel } from "../auth.js";
import { loadUsageRecords, loadUsageStats, updateChartColors, usageState, usageTipHide } from "./usage.js";
import { loadConfigDraft } from "../settings/config.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

// 控制台位置写进 URL hash:#console/<面板> 与 #console/settings/<子页>。
// 刷新、分享链接都能回到同一页;老的裸 #console 仍开数据统计。
export function consoleHashFor(panel, view) {
  return (panel === "settings" || panel === "platforms") && view ? `#console/${panel}/${view}` : `#console/${panel}`;
}

export function writeConsoleHash(hash) {
  const target = hash || `${window.location.pathname}${window.location.search}`;
  if ((hash && window.location.hash === hash) || (!hash && !window.location.hash)) return;
  window.history.replaceState(null, "", target); // 不用 location.hash=,那会留个孤零零的 # 并滚动
}

export function parseConsoleHash() {
  const match = /^#console(?:\/([a-z-]+))?(?:\/([a-z-]+))?$/.exec(window.location.hash || "");
  if (!match) return null;
  let panel = match[1] || "usage";
  let view = match[2] || "";
  // 旧深链：QQ 的消息记录、群管、设置分页都搬进了平台页。
  const legacy = { qq: "qq-history", groups: "qq-groups" }[panel] || (panel === "settings" && view === "qq" ? "qq-settings" : "");
  if (legacy) {
    panel = "platforms";
    view = legacy;
  }
  // 面板清单只有 index.html 一份,这里查 DOM 而不是再抄一遍。
  const known = Boolean(elements.consoleView.querySelector(`.con-panel[data-console-panel="${panel}"]`));
  const allowed = known && (isAdmin() || !isAdminOnlyPanel(panel));
  return { panel: allowed ? panel : "usage", view: allowed ? view : "" };
}

export function consoleOpen(panel = "usage") {
  elements.consoleView.hidden = false;
  elements.consoleView.setAttribute("aria-hidden", "false");
  setConsolePanel(panel);
}

export function consoleClose() {
  elements.consoleView.hidden = true;
  elements.consoleView.setAttribute("aria-hidden", "true");
  usageTipHide();
  writeConsoleHash("");
}

export function consoleIsOpen() {
  return !elements.consoleView.hidden;
}

/// 切控制台标签页。数据统计的图表要等真正显示了才量得到尺寸,配置也是进了
/// 设置页才拉——都放在这里,免得开个控制台把两边的请求都打出去。
export function setConsolePanel(panel) {
  if (!isAdmin() && isAdminOnlyPanel(panel)) panel = "usage";
  state.consolePanel = panel;
  if (panel === "account") loadAccountPanel();
  for (const item of elements.consoleView.querySelectorAll(".con-rail-item[data-console-panel]")) {
    item.classList.toggle("active", item.dataset.consolePanel === panel);
  }
  for (const pane of elements.consoleView.querySelectorAll(".con-panel[data-console-panel]")) {
    pane.hidden = pane.dataset.consolePanel !== panel;
  }
  if (panel === "usage") {
    updateChartColors();
    loadUsageStats();
    loadUsageRecords();
  } else {
    usageTipHide();
  }
  if (panel === "settings" && !state.configLoaded && !state.configLoading) loadConfigDraft();
  // 插件 dashboard 面板各自独立文件,首次进入挂载、之后只刷新。
  if (window.GqyDash?.has(panel)) window.GqyDash.open(panel);
  if (panel === "platforms") setPlatformView(state.platformView.platform, state.platformView.tab);
  placeSettingsFooter();
  writeConsoleHash(consoleHashFor(panel, consoleViewFor(panel)));
}

export function consoleViewFor(panel) {
  if (panel === "settings") return state.settingsView;
  if (panel === "platforms") return `${state.platformView.platform}-${state.platformView.tab}`;
  return "";
}

/// 通讯平台页。每个平台一行，分页二选一：settingsPage 用设置页的渲染器
/// (GqySettings 的页名)，dash 用看板(GqyDash 的面板名)。
/// 加平台：index.html 加一个平台按钮和 platform-body，再在这里登记分页。
/// 平台 id 里不能有 "-"，深链用它分隔平台与分页(#console/platforms/qq-groups)。
export const PLATFORMS = {
  qq: {
    tabs: {
      settings: { settingsPage: "qq" },
      history: { dash: "qq" },
      groups: { dash: "groups" }
    }
  }
};

export function parsePlatformView(view) {
  const [platform, tab] = String(view || "").split("-");
  return { platform, tab };
}

export function setPlatformView(platform, tab) {
  const known = PLATFORMS[platform] ? platform : Object.keys(PLATFORMS)[0];
  const tabs = PLATFORMS[known].tabs;
  const selectedTab = tabs[tab] ? tab : Object.keys(tabs)[0];
  state.platformView = { platform: known, tab: selectedTab };
  const panel = elements.consoleView.querySelector('.con-panel[data-console-panel="platforms"]');
  if (!panel) return;
  for (const button of panel.querySelectorAll("[data-platform]")) {
    const active = button.dataset.platform === known;
    button.classList.toggle("active", active);
    button.setAttribute("aria-current", active ? "page" : "false");
  }
  for (const body of panel.querySelectorAll("[data-platform-body]")) {
    const current = body.dataset.platformBody === known;
    body.hidden = !current;
    if (!current) continue;
    for (const button of body.querySelectorAll("[data-platform-tab]")) {
      const active = button.dataset.platformTab === selectedTab;
      button.classList.toggle("active", active);
      button.setAttribute("aria-current", active ? "page" : "false");
    }
    for (const pane of body.querySelectorAll("[data-platform-pane]")) {
      pane.hidden = pane.dataset.platformPane !== selectedTab;
    }
  }
  const target = tabs[selectedTab];
  if (target.settingsPage) {
    if (!state.configLoaded && !state.configLoading) loadConfigDraft();
    window.GqySettings?.onShow(target.settingsPage);
  }
  if (target.dash) window.GqyDash?.open(target.dash);
  placeSettingsFooter();
  if (consoleIsOpen() && state.consolePanel === "platforms") writeConsoleHash(consoleHashFor("platforms", consoleViewFor("platforms")));
}

/// 保存栏只有一个：设置页，或者平台页的设置分页。哪边在显示就挪到哪边，
/// 草稿与脏状态照旧只有一份，两处改的是同一份配置。
export function placeSettingsFooter() {
  const footer = elements.settingsFooter;
  if (!footer) return;
  const { platform, tab } = state.platformView;
  const onPlatformSettings = state.consolePanel === "platforms" && Boolean(PLATFORMS[platform]?.tabs[tab]?.settingsPage);
  const host = elements.consoleView.querySelector(`.con-panel[data-console-panel="${onPlatformSettings ? "platforms" : "settings"}"]`);
  if (host && footer.parentElement !== host) host.append(footer);
}

export function bindConsoleEvents() {
  elements.consoleButton.addEventListener("click", () => consoleOpen());
  elements.consoleBack.addEventListener("click", () => consoleClose());
  elements.conRailToggle.addEventListener("click", () =>
    elements.consoleView.classList.toggle("rail-collapsed"));
  for (const item of elements.consoleView.querySelectorAll(".con-rail-item[data-console-panel]")) {
    item.addEventListener("click", () => setConsolePanel(item.dataset.consolePanel));
  }
  elements.usageRangeSeg.addEventListener("click", (event) => {
    const button = event.target.closest("button");
    if (!button) return;
    elements.usageRangeSeg.querySelectorAll("button").forEach((other) =>
      other.classList.toggle("on", other === button));
    usageState.range = button.dataset.range;
    loadUsageStats();
  });
  elements.usageRefresh.addEventListener("click", () => {
    updateChartColors();
    loadUsageStats();
    loadUsageRecords();
  });
  elements.usageClear.addEventListener("click", async () => {
    // 本页(卡片/热力/每日/模型明细/最近调用)全部派生自 usage-history.jsonl,
    // 删它就是清空整页;usage.json 只喂聊天界面的会话累计,不在本页上。
    if (!window.confirm("清空数据统计？\n\n本页所有数据（总消耗、热力图、每日 token、模型明细、最近调用）都会归零，且不可恢复。")) {
      return;
    }
    elements.usageClear.disabled = true;
    try {
      await apiRequest("/api/usage/clear", { method: "POST" });
      usageState.kindFilters.clear();
      usageState.platformTab = null;
      loadUsageStats();
      loadUsageRecords();
    } catch (error) {
      elements.usageStamp.textContent = `清空失败:${error.message || error}`;
    } finally {
      elements.usageClear.disabled = false;
    }
  });
  elements.usageSrcFilter.addEventListener("change", () => loadUsageRecords());
  elements.usageModelFilter.addEventListener("change", () => loadUsageRecords());
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    const fullscreenVideo = document.querySelector(".video-shell.webfs");
    if (fullscreenVideo) {
      fullscreenVideo.classList.remove("webfs");
      return;
    }
    if (consoleIsOpen()) consoleClose();
  });
}
