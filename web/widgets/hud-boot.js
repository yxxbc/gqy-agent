/// 开发模式的切入动画:贾维斯式 HUD 启动序列。
///
/// 替掉原来「她化作粒子飘走」的那一段:进工作台不该是她离开的画面,而是
/// 系统上线——能量环由内向外展开、扫描线扫过、网格亮起,一行等宽字打出
/// 「DEV MODE」再闪一下 ONLINE。全程 CSS 动画(92-dev-hud.css),这里只负责
/// 搭骨架、到点拆掉;不接事件,用户按键或点击就提前收尾。
const LIFETIME_MS = 1950;
const HURRY_MS = 200;

let active = null;

function reducedMotion() {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
}

function svgNode(tag, attributes) {
  const node = document.createElementNS("http://www.w3.org/2000/svg", tag);
  for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, value);
  return node;
}

function buildRings() {
  const svg = svgNode("svg", { class: "hud-boot-rings", viewBox: "0 0 220 220", "aria-hidden": "true" });
  svg.append(
    svgNode("circle", { class: "ring-outer", cx: "110", cy: "110", r: "104" }),
    svgNode("circle", { class: "ring-mid", cx: "110", cy: "110", r: "80" }),
    svgNode("circle", { class: "ring-inner", cx: "110", cy: "110", r: "56" }),
    svgNode("circle", { class: "ring-core", cx: "110", cy: "110", r: "18" })
  );
  return svg;
}

export function cancelHudBoot() {
  active?.remove();
}

/// 在 host(正文区)里播一次。status 是底下那行小字,比如会话数。
export function playHudBoot(host, { status = "ONLINE" } = {}) {
  if (!host || reducedMotion() || document.hidden) return;
  cancelHudBoot();
  const veil = document.createElement("div");
  veil.className = "hud-boot";
  veil.setAttribute("aria-hidden", "true");
  const core = document.createElement("div");
  core.className = "hud-boot-core";
  core.appendChild(buildRings());
  const text = document.createElement("div");
  text.className = "hud-boot-text";
  const title = document.createElement("span");
  title.className = "hud-boot-title";
  title.textContent = "GQY · DEV MODE";
  const line = document.createElement("span");
  line.className = "hud-boot-status";
  line.textContent = status;
  text.append(title, line);
  const grid = document.createElement("div");
  grid.className = "hud-boot-grid";
  const scan = document.createElement("div");
  scan.className = "hud-boot-scan";
  veil.append(grid, scan, core, text);
  host.appendChild(veil);

  let timer = 0;
  const remove = () => {
    window.clearTimeout(timer);
    window.removeEventListener("keydown", hurry, true);
    window.removeEventListener("pointerdown", hurry, true);
    veil.remove();
    if (active === handle) active = null;
  };
  function hurry() {
    veil.classList.add("is-leaving");
    window.clearTimeout(timer);
    timer = window.setTimeout(remove, HURRY_MS);
  }
  const handle = { remove };
  active = handle;
  window.addEventListener("keydown", hurry, true);
  window.addEventListener("pointerdown", hurry, true);
  timer = window.setTimeout(remove, LIFETIME_MS);
}
