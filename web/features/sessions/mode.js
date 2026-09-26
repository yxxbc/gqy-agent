import { safeStorageGet, safeStorageSet } from "../../core/storage.js";
import { isTerminalSession } from "./list.js";
import { multiSessionEnabled } from "./runs.js";
import { boardImageSrc, syncComposerPlaceholder } from "../persona.js";
import { elements } from "../../state/elements.js";
import { playHudBoot } from "../../widgets/hud-boot.js";
import { playParticleVeil } from "../../widgets/particle-veil.js";
import { setPetals } from "../../widgets/petals.js";
import { syncSwitcherCopy } from "./switcher.js";
import { state } from "../../state/store.js";

/// 侧栏左上角的「普通 | 开发」开关。
///
/// 两种会话创建时模式就定死了，这里把它们当成两间隔开的屋子：侧栏只列当前
/// 模式的会话，新对话也落在当前模式里，页面外观跟着 body[data-mode] 切换。
/// 记住上次在哪间屋子，刷新后回到原处（见 boot.js 的 preferredBootSession）。
export const SESSION_MODE_KEY = "gqy.web.sessionMode";

export function normalizeMode(mode) {
  return mode === "dev" ? "dev" : "normal";
}

export function sessionMode(session) {
  return normalizeMode(session?.mode);
}

export function storedSessionMode() {
  return normalizeMode(safeStorageGet(SESSION_MODE_KEY));
}

/// 当前模式下侧栏可见的会话（终端车道永远不列）。
export function sessionsInMode(mode) {
  const target = normalizeMode(mode);
  return state.sessions.filter(
    (session) => !isTerminalSession(session?.session_id) && sessionMode(session) === target
  );
}

/// 只有本模块用的状态。
const modeState = {
  introPlayed: false
};

export function setActiveMode(mode) {
  const next = normalizeMode(mode);
  const changed = next !== state.sessionMode;
  state.sessionMode = next;
  safeStorageSet(SESSION_MODE_KEY, next);
  syncModeChrome();
  // 页面第一次落定模式(刷新)也算一次进场;之后只在真的换了模式时播。
  const intro = !modeState.introPlayed;
  modeState.introPlayed = true;
  if (changed || intro) playModeVeil(next);
}

/// 进普通模式:她的壁纸由粒子聚成整图;进开发模式:贾维斯式 HUD 启动序列。
/// 刷新落在哪个模式,就播哪个模式的进场。
function playModeVeil(mode) {
  if (!multiSessionEnabled()) return;
  if (mode === "dev") {
    const count = sessionsInMode("dev").length;
    playHudBoot(elements.mainStage, { status: `ONLINE · ${count} SESSION${count === 1 ? "" : "S"}` });
    return;
  }
  const imageUrl = boardImageSrc();
  if (imageUrl) playParticleVeil(elements.mainStage, { imageUrl });
}

export function syncModeChrome() {
  const mode = normalizeMode(state.sessionMode);
  document.body.dataset.mode = mode;
  // 落梅只属于她的房间;开发模式、单会话部署都不飘。
  setPetals(elements.mainStage, mode === "normal" && multiSessionEnabled());
  syncComposerPlaceholder();
  syncSwitcherCopy();
  const buttons = elements.modeSwitch?.querySelectorAll("[data-mode]") || [];
  for (const button of buttons) {
    const active = button.dataset.mode === mode;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-checked", String(active));
    button.tabIndex = active ? 0 : -1;
  }
  if (elements.modeSwitch) {
    // 单会话部署没有「另一间屋子」，开关不出现。
    elements.modeSwitch.hidden = !multiSessionEnabled();
    elements.modeSwitch.dataset.active = mode;
  }
}
