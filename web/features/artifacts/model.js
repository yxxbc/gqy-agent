import { STAGE_WIDE_PX } from "../../core/constants.js";
import { layoutViewportWidth } from "../../core/ui-scale.js";
import { resetArtifactImageView } from "./image.js";
import { renderArtifactWorkspace } from "./workspace.js";
import { syncSidebarSpace } from "../sidebar.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

export function safeAssetUrl(value) {
  const raw = String(value || "").trim();
  if (!raw) return null;
  try {
    const url = new URL(raw, window.location.origin);
    if (url.origin !== window.location.origin || !url.pathname.startsWith("/api/assets/") || url.pathname === "/api/assets/") return null;
    return url.href;
  } catch (_) {
    return null;
  }
}

export function safeArtifactUrl(value) {
  const raw = String(value || "").trim();
  if (!raw) return null;
  try {
    const url = new URL(raw, window.location.origin);
    const allowed = ["/api/assets/", "/api/artifacts/"].some((prefix) => url.pathname.startsWith(prefix) && url.pathname !== prefix);
    return url.origin === window.location.origin && allowed ? url.href : null;
  } catch (_) {
    return null;
  }
}

export function artifactName(source) {
  return String(source?.name || source?.alt || "预览资源").trim() || "预览资源";
}

export function normalizeArtifact(source, fallbackKind = "file") {
  if (!source || typeof source !== "object") return null;
  const url = safeArtifactUrl(source.url);
  if (!url) return null;
  const mime = String(source.mime || "application/octet-stream").toLowerCase();
  return {
    ...source,
    id: String(source.id || url),
    url,
    name: artifactName(source),
    type_label: String(source.type_label || "").trim().toUpperCase(),
    mime,
    kind: String(source.kind || (mime.startsWith("image/") ? "image" : fallbackKind))
  };
}

export function artifactSupportsPreview(artifact) {
  // svg 的 mime 是 image/svg+xml,靠下面这条命中图片通道——`<img>` 里的 SVG
  // 浏览器强制禁脚本禁外链,既安全又白捡了缩放平移。
  return artifact?.kind === "image"
    || artifact?.mime?.startsWith("image/")
    || ["markdown", "html", "pdf", "csv"].includes(artifact?.kind);
}

export function artifactSupportsSource(artifact) {
  // svg 是图片也是文本,两个视图都要给:光能看不能读,改起来无从下手。
  return ["markdown", "html", "text", "code", "json", "csv", "svg"].includes(artifact?.kind)
    || artifact?.mime?.startsWith("text/")
    || artifact?.mime?.startsWith("application/json");
}

export function defaultArtifactMode(artifact) {
  return artifactSupportsPreview(artifact) ? "preview" : "source";
}

export function artifactWidthPixels() {
  const viewportWidth = Math.max(320, layoutViewportWidth());
  return Math.min(viewportWidth - 20, Math.max(320, viewportWidth * state.artifactWidthRatio));
}

export function syncComposerDockHeight() {
  // 非分栏的桌面浮层态里,artifact 面板是绝对定位、bottom 贴到 10px,会盖住输入框
  // 页脚(#2)。把页脚实际高度喂给 CSS,浮层的 bottom 就停在页脚上方、页脚照常可用。
  const height = elements.composerDock?.offsetHeight || 0;
  if (height) elements.mainStage.style.setProperty("--composer-dock-height", `${Math.round(height)}px`);
  // 开面板当下量的是旧布局的页脚高度(面板一开正文列变窄、页脚里模型芯片会换行
  // 变高),reflow 之后再量一次才对——否则「刚开盖住、跑一轮才正常」(#2 用户实测)。
  window.requestAnimationFrame(() => {
    const settled = elements.composerDock?.offsetHeight || 0;
    if (settled) elements.mainStage.style.setProperty("--composer-dock-height", `${Math.round(settled)}px`);
  });
}

export function syncArtifactLayout() {
  const width = artifactWidthPixels();
  elements.mainStage.style.setProperty("--artifact-width", `${Math.round(width)}px`);
  syncComposerDockHeight();
  const roomForConversation = elements.mainStage.clientWidth - width - 10;
  const split = state.artifactOpen && !state.artifactMaximized && layoutViewportWidth() > 760 && roomForConversation >= 320;
  elements.mainStage.classList.toggle("artifact-split", split);
  elements.mainStage.classList.toggle("artifact-maximized", state.artifactOpen && state.artifactMaximized);
  // 常驻任务面板的宽度闸。原先靠 .main-stage 上的容器查询,而容器查询容器会
  // 让 WebKit 在后代 replaceChildren 时归零 scrollTop(见 styles.css 注释),
  // 改成这里挂类,量的是同一个宽度。
  elements.mainStage.classList.toggle("is-wide", elements.mainStage.clientWidth >= STAGE_WIDE_PX);
  syncSidebarSpace();
}

export function closeArtifactResourceMenu() {
  elements.artifactResourceMenu.hidden = true;
  elements.artifactTitleButton.setAttribute("aria-expanded", "false");
}

export function setArtifactWorkspaceOpen(open) {
  const hasArtifacts = state.artifacts.length > 0;
  state.artifactOpen = Boolean(open && hasArtifacts);
  if (!state.artifactOpen) state.artifactMaximized = false;
  elements.artifactWorkspace.hidden = !state.artifactOpen;
  elements.artifactWorkspace.setAttribute("aria-hidden", String(!state.artifactOpen));
  elements.mainStage.classList.toggle("artifact-open", state.artifactOpen);
  closeArtifactResourceMenu();
  syncArtifactLayout();
  elements.artifactToggleButton.setAttribute("aria-pressed", String(state.artifactOpen));
  if (state.artifactOpen) {
    elements.artifactToggleButton.classList.remove("has-new-artifact");
    renderArtifactWorkspace();
  }
}

/// artifact 的归属会话。预览面板永远只画当前正在看的那个会话。
export function artifactScope() {
  return String(state.viewSessionId || state.currentSessionId || "");
}

export function pinnedArtifactsForScope() {
  const scope = artifactScope();
  let pinned = state.pinnedArtifacts.get(scope);
  if (!pinned) {
    pinned = new Map();
    state.pinnedArtifacts.set(scope, pinned);
  }
  return pinned;
}

export function dismissedArtifactsForScope() {
  const scope = artifactScope();
  let dismissed = state.dismissedArtifactIds.get(scope);
  if (!dismissed) {
    dismissed = new Set();
    state.dismissedArtifactIds.set(scope, dismissed);
  }
  return dismissed;
}

export function registerArtifact(source, { autoOpen = false } = {}) {
  const artifact = normalizeArtifact(source, source?.kind || "file");
  if (!artifact) return;
  pinnedArtifactsForScope().set(artifact.id, artifact);
  dismissedArtifactsForScope().delete(artifact.id);
  const index = state.artifacts.findIndex((item) => item.id === artifact.id);
  if (index >= 0) state.artifacts[index] = artifact;
  else state.artifacts.push(artifact);
  state.artifactSourceCache.delete(artifact.id);
  state.selectedArtifactId = artifact.id;
  state.artifactMode = defaultArtifactMode(artifact);
  resetArtifactImageView();
  elements.artifactToggleButton.hidden = false;
  if (autoOpen && layoutViewportWidth() > 760) setArtifactWorkspaceOpen(true);
  else if (!state.artifactOpen) elements.artifactToggleButton.classList.add("has-new-artifact");
  if (state.artifactOpen) renderArtifactWorkspace();
}

export function syncArtifactsFromTurns(turns) {
  let artifacts = [];
  for (const turn of turns) {
    // 只收真正的 artifact。`turn.assets` 是对话里内联显示的图片（打印/生成
    // 的图），它们已经在气泡里画出来了，再塞进 artifact 面板等于同一张图占
    // 两个位置，还会把面板自动切到图片上、盖住用户正在看的东西。
    // 要把图当 artifact 展示，走 present_artifact/create_artifact —— 那条
    // 路产出的就是 turn.artifacts。
    for (const source of Array.isArray(turn?.artifacts) ? turn.artifacts : []) {
      const artifact = normalizeArtifact(source, "file");
      if (artifact && !artifacts.some((item) => item.id === artifact.id)) artifacts.push(artifact);
    }
  }
  // 手动送进来的补在后面：它们不属于任何回合，只活在这份 state 里。
  for (const artifact of pinnedArtifactsForScope().values()) {
    if (!artifacts.some((item) => item.id === artifact.id)) artifacts.push(artifact);
  }
  const dismissed = dismissedArtifactsForScope();
  state.artifacts = artifacts.filter((item) => !dismissed.has(item.id));
  artifacts = state.artifacts;
  if (!artifacts.some((item) => item.id === state.selectedArtifactId)) {
    state.selectedArtifactId = artifacts.at(-1)?.id || null;
    state.artifactMode = defaultArtifactMode(artifacts.at(-1));
  }
  const knownIds = new Set(artifacts.map((artifact) => artifact.id));
  for (const id of state.artifactSourceCache.keys()) {
    if (!knownIds.has(id)) state.artifactSourceCache.delete(id);
  }
  elements.artifactToggleButton.hidden = artifacts.length === 0;
  if (!artifacts.length) setArtifactWorkspaceOpen(false);
  else if (state.artifactOpen) renderArtifactWorkspace();
  else if (window.location.hash.includes("artifact")) {
    // 深链 #artifact:载入后自动展开预览工作区(与 #console 同一约定)。
    window.location.hash = "";
    setArtifactWorkspaceOpen(true);
  }
}
