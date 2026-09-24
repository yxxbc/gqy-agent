import { visualPixelsToLayout } from "../../core/ui-scale.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

export function artifactIconName(artifact) {
  if (artifact?.kind === "image" || artifact?.mime?.startsWith("image/")) return "image";
  if (artifact?.kind === "markdown") return "file-markdown";
  if (artifact?.kind === "json") return "file-json";
  if (artifact?.kind === "code" || artifact?.kind === "html") return "file-code";
  if (artifact?.kind === "csv") return "layout-grid";
  return "file-text";
}

export function artifactTypeLabel(artifact) {
  if (artifact?.type_label) return artifact.type_label;
  if (artifact?.kind === "markdown") return "MD";
  if (artifact?.kind === "json") return "JSON";
  if (artifact?.kind === "html") return "HTML";
  if (artifact?.kind === "code") return "CODE";
  if (artifact?.kind === "pdf") return "PDF";
  if (artifact?.kind === "image") return String(artifact.mime || "IMAGE").split("/").pop().toUpperCase();
  return "FILE";
}

export const ARTIFACT_ZOOM_MAX = 4;

export function artifactImageTransform() {
  return `translate(${state.artifactPanX}px, ${state.artifactPanY}px) scale(${state.artifactZoom})`;
}

/// 缩放 / 平移归零,并作废 renderArtifactWorkspace 的「同一视图不重建」记号——
/// 否则状态归零了、画面上的图还停在旧变换里。
export function resetArtifactImageView() {
  state.artifactZoom = 1;
  state.artifactPanX = 0;
  state.artifactPanY = 0;
  delete elements.artifactView.dataset.renderKey;
}

/// 指针在 stage 里的布局坐标。clientX 与 getBoundingClientRect 都是屏上像素,
/// `.app-shell` 带 `zoom: var(--ui-scale)`,差值除 UI_SCALE 才和 offsetLeft 同一套单位。
export function artifactStagePoint(stage, event) {
  const rect = stage.getBoundingClientRect();
  return {
    x: visualPixelsToLayout(event.clientX - rect.left),
    y: visualPixelsToLayout(event.clientY - rect.top)
  };
}

/// 以 anchor(stage 内布局坐标,缺省取 stage 中心)为不动点缩放。
///
/// 原点在图的左上角(styles.css `transform-origin: 0 0`),屏上位置 = 图框 + pan + zoom·q。
/// 让 anchor 下那个 q 缩放前后不动,就是 pan' = pan + (anchor − 图框 − pan)·(1 − 新/旧)。
/// 以前原点是 `center top`、滚轮不补偿 pan:放大时图往下长,指针下的内容跑开——todo 里的「错位」。
export function zoomArtifactImage(nextZoom, anchor = null) {
  const stage = elements.artifactView.querySelector(".artifact-image-stage");
  const image = stage?.querySelector("img");
  const previous = state.artifactZoom || 1;
  const zoom = Math.min(ARTIFACT_ZOOM_MAX, Math.max(1, Number(nextZoom) || 1));
  if (zoom <= 1) {
    state.artifactPanX = 0;
    state.artifactPanY = 0;
  } else if (stage && image) {
    const point = anchor || { x: stage.clientWidth / 2, y: stage.clientHeight / 2 };
    const ratio = 1 - zoom / previous;
    state.artifactPanX += (point.x - image.offsetLeft - state.artifactPanX) * ratio;
    state.artifactPanY += (point.y - image.offsetTop - state.artifactPanY) * ratio;
  }
  state.artifactZoom = zoom;
  if (image) {
    image.style.transform = artifactImageTransform();
    stage.classList.toggle("is-zoomed", zoom > 1);
  }
  updateArtifactImageControls();
}

export function renderArtifactImage(artifact) {
  const stage = document.createElement("div");
  stage.className = "artifact-image-stage";
  const image = document.createElement("img");
  image.src = artifact.url;
  image.alt = artifact.name;
  image.draggable = false;
  image.style.transform = artifactImageTransform();
  stage.classList.toggle("is-zoomed", state.artifactZoom > 1);
  stage.addEventListener("wheel", (event) => {
    event.preventDefault();
    zoomArtifactImage(state.artifactZoom * (event.deltaY < 0 ? 1.12 : 0.89), artifactStagePoint(stage, event));
  }, { passive: false });
  // 双击:适应 ↔ 原始尺寸,以双击点为锚。图本身比面板小(适应即原始)时放大两倍,不然双击没反应。
  stage.addEventListener("dblclick", (event) => {
    event.preventDefault();
    if (state.artifactZoom > 1) {
      zoomArtifactImage(1);
      return;
    }
    const actual = image.offsetWidth ? image.naturalWidth / image.offsetWidth : 0;
    zoomArtifactImage(actual > 1.05 ? actual : 2, artifactStagePoint(stage, event));
  });

  // 平移。位移只除 UI_SCALE、**不除 zoom**:transform 是 translate() 在 scale() 前,
  // translate 不被放大(docs/plan-is-true/2026-09-14/webui-delivery.md §1 验证推理)。
  let pan = null;
  let frame = 0;
  const applyPan = () => {
    frame = 0;
    if (!pan) return;
    state.artifactPanX = pan.originX + visualPixelsToLayout(pan.clientX - pan.startX);
    state.artifactPanY = pan.originY + visualPixelsToLayout(pan.clientY - pan.startY);
    image.style.transform = artifactImageTransform();
  };
  stage.addEventListener("pointerdown", (event) => {
    if (state.artifactZoom <= 1 || event.button !== 0) return;
    event.preventDefault();
    stage.classList.add("is-dragging");
    stage.setPointerCapture(event.pointerId);
    pan = {
      startX: event.clientX,
      startY: event.clientY,
      originX: state.artifactPanX,
      originY: state.artifactPanY,
      clientX: event.clientX,
      clientY: event.clientY
    };
  });
  // 高回报率鼠标一帧能来好几次 pointermove:只记最新坐标,每帧写一次 transform。
  stage.addEventListener("pointermove", (event) => {
    if (!pan) return;
    pan.clientX = event.clientX;
    pan.clientY = event.clientY;
    if (!frame) frame = window.requestAnimationFrame(applyPan);
  });
  const finishPan = () => {
    if (!pan) return;
    // 还没画的最后一帧当场补上,松手的位置就是停下的位置。
    if (frame) {
      window.cancelAnimationFrame(frame);
      applyPan();
    }
    pan = null;
    stage.classList.remove("is-dragging");
  };
  stage.addEventListener("pointerup", finishPan);
  stage.addEventListener("pointercancel", finishPan);
  // 捕获被抢走(系统手势、弹窗、元素被移出文档)时不会有 up/cancel,不听这条就卡在拖拽态。
  stage.addEventListener("lostpointercapture", finishPan);
  stage.appendChild(image);
  return stage;
}

export function updateArtifactImageControls() {
  const artifact = state.artifacts.find((item) => item.id === state.selectedArtifactId);
  if (!(artifact?.kind === "image" || artifact?.mime?.startsWith("image/"))) return;
  elements.artifactImageZoomOutButton.disabled = state.artifactZoom <= 1;
  elements.artifactImageZoomInButton.disabled = state.artifactZoom >= ARTIFACT_ZOOM_MAX;
}
