import { makeIconSlot } from "../../core/icons.js";
import { registerArtifact, safeAssetUrl, setArtifactWorkspaceOpen } from "../artifacts/model.js";
import { contentAdded } from "./scroll.js";

export function validAssetDimension(value) {
  const number = Number(value);
  return Number.isInteger(number) && number > 0 && number <= 100_000 ? number : null;
}

export function createConversationMedia(asset, { eager = false } = {}) {
  const source = asset && typeof asset === "object" ? asset : {};
  const url = safeAssetUrl(source.url);
  const mime = String(source.mime || "").trim().toLowerCase();
  const imageMime = !mime || mime.startsWith("image/");
  const width = validAssetDimension(source.width);
  const height = validAssetDimension(source.height);
  const alt = String(source.alt || "").trim() || "顾清影 生成的图片";

  const figure = document.createElement("figure");
  figure.className = "conversation-media";
  if (source.id != null) figure.dataset.assetId = String(source.id);
  const visual = document.createElement("div");
  visual.className = "conversation-media-visual";
  if (width && height) {
    const ratio = width / height;
    if (ratio >= 0.05 && ratio <= 20) {
      visual.classList.add("has-aspect");
      visual.style.aspectRatio = `${width} / ${height}`;
    }
  }
  const fallback = document.createElement("div");
  fallback.className = "conversation-media-fallback";
  fallback.appendChild(makeIconSlot("circle-alert"));
  const fallbackText = document.createElement("span");
  fallbackText.textContent = url && imageMime ? "图片载入失败" : "图片地址不可用";
  fallback.appendChild(fallbackText);

  if (url && imageMime) {
    const image = document.createElement("img");
    image.alt = alt;
    image.loading = eager ? "eager" : "lazy";
    image.decoding = "async";
    if (width) image.width = width;
    if (height) image.height = height;
    fallback.hidden = true;
    image.addEventListener("error", () => {
      image.remove();
      fallback.hidden = false;
      figure.classList.add("is-error");
      if (eager) contentAdded(figure);
    }, { once: true });
    // 只有实时流的新图(eager)加载完才跟随滚动;历史重建(刷新)的图不该在
    // 逐张加载时把视图一路拉到底——那正是「打印图片刷新后跳到 AI 输出尾部」
    // 的原因(09-12 #19)。有 aspect-ratio 占位,历史图加载也不跳。
    if (eager) image.addEventListener("load", contentAdded, { once: true });
    image.src = url;
    visual.append(image, fallback);
  } else {
    visual.appendChild(fallback);
  }

  // 图下面既不挂文件名也不挂按钮——每张图多占一行、还把气泡撑得很吵。
  // 名字(表情包是描述)和那三个按钮都跟着灯箱走(web/lightbox.js)。
  // `alt` 仍然写在 img 上,读屏和图裂时靠它。
  if (url && imageMime) {
    visual.classList.add("is-zoomable");
    visual.tabIndex = 0;
    visual.setAttribute("role", "button");
    visual.setAttribute("aria-label", `放大预览 ${alt}`);
    const openLightbox = () => {
      window.GqyLightbox?.open({
        url,
        name: alt,
        onOpenInWorkspace: () => {
          registerArtifact({ ...source, url, name: alt, kind: "image" });
          setArtifactWorkspaceOpen(true);
        },
      });
    };
    visual.addEventListener("click", openLightbox);
    visual.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        openLightbox();
      }
    });
  }
  figure.appendChild(visual);
  return figure;
}
