import { makeIconSlot } from "../../core/icons.js";
import { showToast } from "../../core/toast.js";
import { ARTIFACT_ZOOM_MAX, artifactIconName, artifactTypeLabel, renderArtifactImage, resetArtifactImageView, zoomArtifactImage } from "./image.js";
import { artifactSupportsPreview, artifactSupportsSource, closeArtifactResourceMenu, defaultArtifactMode, dismissedArtifactsForScope, normalizeArtifact, pinnedArtifactsForScope, registerArtifact, setArtifactWorkspaceOpen, syncArtifactLayout } from "./model.js";
import { artifactLoadingNode, buildArtifactTable, loadArtifactSource, renderArtifactFailure, renderArtifactSource } from "./source.js";
import { renderMarkdown } from "../markdown/render.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

export async function renderArtifactPreview(artifact, token) {
  if (artifact.kind === "image" || artifact.mime.startsWith("image/")) {
    elements.artifactView.replaceChildren(renderArtifactImage(artifact));
    return;
  }
  if (artifact.kind === "pdf") {
    const frame = document.createElement("iframe");
    frame.className = "artifact-frame";
    frame.src = artifact.url;
    frame.title = artifact.name;
    elements.artifactView.replaceChildren(frame);
    return;
  }
  if (artifact.kind === "html") {
    const frame = document.createElement("iframe");
    frame.className = "artifact-frame";
    frame.src = artifact.url;
    frame.title = artifact.name;
    /*
     * 她写的页面要能动——图表、按钮、切换,不放开脚本这些全是死的。放开的同时
     * 靠这两样把它关在箱子里:
     *   · 不给 allow-same-origin：iframe 拿不透明源,cookie / localStorage /
     *     父页面 DOM 一律 SecurityError。别家(Claude、ChatGPT、LibreChat)给了
     *     same-origin,所以不得不再买个独立域名来隔离 cookie;我们不给,也就
     *     不需要独立域。代价是 artifact 里存不住状态,刷新即归零。
     *   · 不给 allow-popups / allow-forms / allow-top-navigation：这三个各自是
     *     一条外带通道(window.open、表单提交、top.location),**CSP 管不了,
     *     只有 sandbox 管得了**。LibreChat 的 CVE-2026-54025 就死在第三条上。
     * 出站那一半由后端的 CSP 掐(见 assets.rs 的 artifact_csp)。两道各管一半:
     * sandbox 管权限,CSP 管外泄。
     */
    frame.setAttribute("sandbox", "allow-scripts allow-modals");
    elements.artifactView.replaceChildren(frame);
    return;
  }
  if (artifact.kind === "csv") {
    const text = await loadArtifactSource(artifact);
    if (token !== state.artifactRenderToken) return;
    elements.artifactView.replaceChildren(buildArtifactTable(artifact, text));
    return;
  }
  if (artifact.kind === "markdown") {
    const text = await loadArtifactSource(artifact);
    if (token !== state.artifactRenderToken) return;
    const article = document.createElement("article");
    article.className = "markdown-body artifact-markdown";
    renderMarkdown(article, text);
    elements.artifactView.replaceChildren(article);
    return;
  }
  throw new Error("此格式不支持预览");
}

export function renderArtifactResourceMenu(artifact) {
  elements.artifactResourceMenu.replaceChildren();
  for (const item of state.artifacts) {
    const row = document.createElement("div");
    row.className = "artifact-resource-row";
    const button = document.createElement("button");
    button.type = "button";
    button.role = "menuitem";
    button.className = item.id === artifact.id ? "active" : "";
    const label = document.createElement("span");
    label.textContent = item.name;
    const type = document.createElement("small");
    type.textContent = artifactTypeLabel(item);
    button.append(makeIconSlot(artifactIconName(item)), label, type);
    if (item.id === artifact.id) button.appendChild(makeIconSlot("check"));
    button.addEventListener("click", () => {
      state.selectedArtifactId = item.id;
      state.artifactMode = defaultArtifactMode(item);
      resetArtifactImageView();
      closeArtifactResourceMenu();
      renderArtifactWorkspace();
    });
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "icon-button artifact-resource-remove";
    remove.title = "从列表移除";
    remove.setAttribute("aria-label", `从列表移除 ${item.name}`);
    remove.appendChild(makeIconSlot("x"));
    remove.addEventListener("click", (event) => {
      event.stopPropagation();
      dismissArtifact(item.id);
    });
    row.append(button, remove);
    elements.artifactResourceMenu.appendChild(row);
  }
  // 只有一个 artifact 时也要能开这个菜单——删除按钮在菜单里，禁掉就等于
  // 「最后一个删不掉」。当初禁它是因为菜单只用来切换，一个项目没得切。
  elements.artifactTitleButton.disabled = state.artifacts.length === 0;
}

/// 从列表里拿掉一个 artifact。回合产出的那些下次同步会重新长出来，所以
/// 得把 id 记进 dismissed 才删得掉。
export function dismissArtifact(id) {
  dismissedArtifactsForScope().add(id);
  pinnedArtifactsForScope().delete(id);
  state.artifactSourceCache.delete(id);
  state.artifacts = state.artifacts.filter((item) => item.id !== id);
  if (state.selectedArtifactId === id) {
    const next = state.artifacts.at(-1);
    state.selectedArtifactId = next?.id || null;
    state.artifactMode = defaultArtifactMode(next);
    resetArtifactImageView();
  }
  if (!state.artifacts.length) {
    closeArtifactResourceMenu();
    setArtifactWorkspaceOpen(false);
    elements.artifactToggleButton.hidden = true;
    elements.artifactToggleButton.classList.remove("has-new-artifact");
    return;
  }
  renderArtifactWorkspace();
  renderArtifactResourceMenu(state.artifacts.find((item) => item.id === state.selectedArtifactId));
}

export function renderArtifactWorkspace() {
  if (!state.artifactOpen) return;
  const artifact = state.artifacts.find((item) => item.id === state.selectedArtifactId) || state.artifacts.at(-1);
  if (!artifact) return;
  state.selectedArtifactId = artifact.id;
  const canPreview = artifactSupportsPreview(artifact);
  const canSource = artifactSupportsSource(artifact);
  const isImage = artifact.kind === "image" || artifact.mime.startsWith("image/");
  if ((state.artifactMode === "preview" && !canPreview) || (state.artifactMode === "source" && !canSource)) {
    state.artifactMode = defaultArtifactMode(artifact);
  }
  elements.artifactTitle.textContent = artifact.name;
  elements.artifactTitle.title = artifact.name;
  elements.artifactTypeLabel.textContent = artifactTypeLabel(artifact);
  // ?download=1 → 后端强制 attachment,markdown/pdf 也直接落盘而不是再开预览。
  elements.artifactDownloadButton.href = `${artifact.url}?download=1`;
  // 两个视图都在才需要切换器。原来这里按「是不是图片」判断,svg 一来就露馅了:
  // 它既是图片又是文本,两个视图都有,却因为 mime 是 image/* 被整组藏掉,
  // 源码根本点不到。判据换成「有没有得切」,和具体类型脱钩。
  const showPicture = isImage && state.artifactMode === "preview";
  elements.artifactPreviewButton.parentElement.hidden = !(canPreview && canSource);
  elements.artifactImageActions.hidden = !showPicture;
  elements.artifactImageExternalButton.href = showPicture ? artifact.url : "";
  elements.artifactImageZoomOutButton.disabled = !showPicture || state.artifactZoom <= 1;
  elements.artifactImageZoomInButton.disabled = !showPicture || state.artifactZoom >= ARTIFACT_ZOOM_MAX;
  elements.artifactPreviewButton.hidden = !canPreview;
  elements.artifactSourceButton.hidden = !canSource;
  elements.artifactPreviewButton.classList.toggle("active", state.artifactMode === "preview");
  elements.artifactSourceButton.classList.toggle("active", state.artifactMode === "source");
  elements.artifactPreviewButton.setAttribute("aria-pressed", String(state.artifactMode === "preview"));
  elements.artifactSourceButton.setAttribute("aria-pressed", String(state.artifactMode === "source"));
  elements.artifactCopyButton.disabled = !canSource && artifact.kind === "pdf";
  // 图片没有文本可复制,但 svg 有——同样不能只看 mime。
  elements.artifactCopyButton.hidden = isImage && !canSource;
  elements.artifactMaximizeButton.replaceChildren(makeIconSlot(state.artifactMaximized ? "minimize-2" : "maximize-2"));
  elements.artifactMaximizeButton.title = state.artifactMaximized ? "退出全屏" : "全屏显示";
  elements.artifactMaximizeButton.setAttribute("aria-label", elements.artifactMaximizeButton.title);
  renderArtifactResourceMenu(artifact);
  // 同一份内容、同一视图就不重建。回合同步、全屏切换都会走到这里,以前每次都整块重建:
  // 拖图拖到一半 stage 被换掉(像「错位」)、HTML iframe 重载丢交互状态。
  // 要强制重建(缩放归零、换了内容)的地方删掉这个记号,见 resetArtifactImageView。
  const renderKey = `${artifact.id}|${state.artifactMode}|${artifact.url}|${artifact.updated_at || ""}`;
  if (elements.artifactView.dataset.renderKey === renderKey && elements.artifactView.childElementCount) return;
  elements.artifactView.dataset.renderKey = renderKey;
  const token = ++state.artifactRenderToken;
  elements.artifactView.replaceChildren(artifactLoadingNode());
  const render = state.artifactMode === "source"
    ? renderArtifactSource(artifact, token)
    : renderArtifactPreview(artifact, token);
  render.catch((error) => {
    if (token === state.artifactRenderToken) delete elements.artifactView.dataset.renderKey;
    renderArtifactFailure(error, token);
  });
}

export async function copySelectedArtifact() {
  const artifact = state.artifacts.find((item) => item.id === state.selectedArtifactId);
  if (!artifact) return;
  try {
    if (artifactSupportsSource(artifact)) {
      await navigator.clipboard.writeText(await loadArtifactSource(artifact));
    } else if (artifact.kind === "image" && window.ClipboardItem) {
      const response = await fetch(artifact.url, { credentials: "same-origin" });
      if (!response.ok) throw new Error("图片载入失败");
      const blob = await response.blob();
      await navigator.clipboard.write([new ClipboardItem({ [blob.type]: blob })]);
    } else {
      await navigator.clipboard.writeText(artifact.url);
    }
    showToast("已复制", "success");
  } catch (error) {
    showToast(error.message || "复制失败", "error");
  }
}

export function setArtifactMode(mode) {
  const artifact = state.artifacts.find((item) => item.id === state.selectedArtifactId);
  if (!artifact || (mode === "preview" ? !artifactSupportsPreview(artifact) : !artifactSupportsSource(artifact))) return;
  state.artifactMode = mode;
  renderArtifactWorkspace();
}

export function toggleArtifactMaximized() {
  if (!state.artifactOpen) return;
  state.artifactMaximized = !state.artifactMaximized;
  syncArtifactLayout();
  renderArtifactWorkspace();
}

export function changeArtifactImageZoom(delta) {
  const artifact = state.artifacts.find((item) => item.id === state.selectedArtifactId);
  if (!artifact || !(artifact.kind === "image" || artifact.mime.startsWith("image/"))) return;
  zoomArtifactImage((state.artifactZoom || 1) + delta);
}

/// 键盘 `+` / `-` / `0`:只在图片预览开着、焦点不在输入控件里时生效,
/// 焦点落在侧栏或页面空白处才接——在聊天正文里敲 0 不该把图复位。
export function handleArtifactImageKey(event) {
  if (!state.artifactOpen || event.ctrlKey || event.metaKey || event.altKey) return;
  const target = event.target instanceof Element ? event.target : null;
  if (target?.closest("input, textarea, select, [contenteditable]")) return;
  if (target && target !== document.body && !elements.artifactWorkspace.contains(target)) return;
  if (!elements.artifactView.querySelector(".artifact-image-stage")) return;
  if (event.key === "+" || event.key === "=") zoomArtifactImage(state.artifactZoom * 1.25);
  else if (event.key === "-" || event.key === "_") zoomArtifactImage(state.artifactZoom * 0.8);
  else if (event.key === "0") zoomArtifactImage(1);
  else return;
  event.preventDefault();
}

export function artifactChipOptions() {
  return {
    normalize: (source) => normalizeArtifact(source, source?.kind || "file"),
    typeLabel: artifactTypeLabel,
    iconName: artifactIconName,
    iconSlot: makeIconSlot,
    // registerArtifact 会把它从 dismissed 里拿出来,在资源菜单里「移除」过的也能再调出。
    onOpen: (artifact) => {
      registerArtifact(artifact);
      setArtifactWorkspaceOpen(true);
    }
  };
}
