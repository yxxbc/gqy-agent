// Mirrors the CSS --ui-scale custom property; mobile drops it to 1 via a
// media query, so read it at runtime instead of hardcoding.
export let UI_SCALE = 1.1;

export function refreshUiScale() {
  const raw = Number.parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue("--ui-scale")
  );
  if (Number.isFinite(raw) && raw > 0) UI_SCALE = raw;
}

export const artifactTextScale = () => 1.2 / UI_SCALE;

export function layoutViewportWidth() {
  return (window.innerWidth || document.documentElement.clientWidth || 0) / UI_SCALE;
}

export function visualPixelsToLayout(value) {
  return Number(value || 0) / UI_SCALE;
}

/// 原 app.js 顶层的副作用语句，由入口在启动时按原顺序调用。
export function start() {
  refreshUiScale();

  window.addEventListener("resize", refreshUiScale);
}
