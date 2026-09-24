import { consoleClose, consoleIsOpen, consoleOpen } from "../console/panel.js";
import { closeModelMenu } from "../model-menu/menu.js";
import { state } from "../../state/store.js";

// 设置以前是从右侧滑出来的抽屉,自带遮罩和焦点陷阱。现在它是控制台的一个
// 标签页——控制台本来就是个整页视图,设置这么大一坨挂在抽屉里,和「数据统计」
// 各占一套导航,没道理。这两个函数保留下来当入口,内部转成开控制台。
export function openSettings(opener = document.activeElement) {
  state.settingsOpener = opener;
  closeModelMenu();
  consoleOpen("settings");
}

export function closeSettings({ restoreFocus = true } = {}) {
  if (!settingsIsOpen()) return;
  consoleClose();
  if (restoreFocus && state.settingsOpener instanceof HTMLElement) state.settingsOpener.focus();
  state.settingsOpener = null;
}

export function settingsIsOpen() {
  return consoleIsOpen() && state.consolePanel === "settings";
}
