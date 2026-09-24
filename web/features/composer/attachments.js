import { apiRequest } from "../../core/api.js";
import { MAX_ATTACHMENTS } from "../../core/constants.js";
import { formatFileSize } from "../../core/format.js";
import { makeIconSlot } from "../../core/icons.js";
import { showToast } from "../../core/toast.js";
import { updateControlState } from "./input.js";
import { updateJumpButtonOffset } from "../conversation/scroll.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

export function safeAttachmentUrl(value) {
  const raw = String(value || "").trim();
  if (!raw) return null;
  try {
    const url = new URL(raw, window.location.origin);
    if (url.origin !== window.location.origin || !url.pathname.startsWith("/api/attachments/") || url.pathname === "/api/attachments/") return null;
    return url.href;
  } catch (_) {
    return null;
  }
}

export function attachmentSessionId() {
  return String(state.viewSessionId || state.currentSessionId || "");
}

export function renderComposerAttachments() {
  const tray = elements.attachmentTray;
  tray.replaceChildren();
  tray.hidden = state.composerAttachments.length === 0;
  for (const item of state.composerAttachments) {
    const isImage = item.kind === "image" && item.previewUrl;
    const entry = document.createElement("div");
    entry.className = `attachment-item ${isImage ? "is-image" : "is-file"} is-${item.status}`;
    entry.title = item.status === "error" ? `${item.name}: ${item.error || "上传失败"}` : item.name;
    if (isImage) {
      const image = document.createElement("img");
      image.src = item.previewUrl;
      image.alt = "";
      const fallback = document.createElement("span");
      fallback.className = "attachment-image-fallback";
      fallback.hidden = true;
      fallback.appendChild(makeIconSlot("circle-alert"));
      image.addEventListener("load", () => { fallback.hidden = true; }, { once: true });
      image.addEventListener("error", () => {
        image.hidden = true;
        fallback.hidden = false;
      }, { once: true });
      entry.append(image, fallback);
    } else {
      const icon = document.createElement("span");
      icon.className = "attachment-file-icon";
      const nameParts = String(item.name || "").split(".");
      const extension = nameParts.length > 1 ? nameParts.pop().toUpperCase() : "FILE";
      icon.textContent = extension.slice(0, 4);
      entry.appendChild(icon);
      const copy = document.createElement("span");
      copy.className = "attachment-item-copy";
      const name = document.createElement("strong");
      name.textContent = item.name;
      name.title = item.name;
      const meta = document.createElement("small");
      if (item.status === "uploading") meta.textContent = `上传中 ${Math.round(item.progress || 0)}%`;
      else if (item.status === "error") meta.textContent = item.error || "上传失败";
      else meta.textContent = formatFileSize(item.size);
      copy.append(name, meta);
      entry.appendChild(copy);
    }
    if (item.status === "uploading") {
      const spinner = makeIconSlot("loader-circle", "attachment-spinner is-spinning");
      entry.appendChild(spinner);
    } else if (item.status === "error") {
      const retry = document.createElement("button");
      retry.type = "button";
      retry.className = "attachment-action";
      retry.title = "重试上传";
      retry.setAttribute("aria-label", `重试上传 ${item.name}`);
      retry.appendChild(makeIconSlot("refresh-cw"));
      retry.addEventListener("click", () => uploadComposerAttachment(item));
      entry.appendChild(retry);
    }
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "attachment-action attachment-remove";
    remove.title = "移除附件";
    remove.setAttribute("aria-label", `移除附件 ${item.name}`);
    remove.appendChild(makeIconSlot("x"));
    remove.addEventListener("click", () => removeComposerAttachment(item));
    entry.appendChild(remove);
    tray.appendChild(entry);
  }
  window.requestAnimationFrame(updateJumpButtonOffset);
}

export function uploadComposerAttachment(item) {
  if (!item?.file || !item.sessionId) return;
  item.status = "uploading";
  item.progress = 0;
  item.error = "";
  renderComposerAttachments();
  updateControlState();
  const request = new XMLHttpRequest();
  item.request = request;
  request.open("POST", `/api/attachments?session_id=${encodeURIComponent(item.sessionId)}`);
  request.setRequestHeader("Accept", "application/json");
  request.setRequestHeader("Content-Type", item.file.type || "application/octet-stream");
  request.setRequestHeader("X-GQY-Filename", encodeURIComponent(item.file.name));
  request.upload.addEventListener("progress", (event) => {
    if (!event.lengthComputable || item.request !== request) return;
    item.progress = Math.min(99, Math.round((event.loaded / event.total) * 100));
    renderComposerAttachments();
  });
  request.addEventListener("load", () => {
    if (item.request !== request) return;
    item.request = null;
    let payload = null;
    try { payload = JSON.parse(request.responseText || "null"); } catch (_) {}
    if (request.status >= 200 && request.status < 300 && payload?.id) {
      const uploadedPreview = payload.kind === "image" ? safeAttachmentUrl(payload.url) : null;
      if (uploadedPreview && item.previewUrl?.startsWith("blob:")) URL.revokeObjectURL(item.previewUrl);
      Object.assign(item, payload, {
        previewUrl: uploadedPreview || item.previewUrl,
        status: "ready",
        progress: 100,
        error: ""
      });
    } else {
      item.status = "error";
      item.error = payload?.error?.message || `上传失败 (${request.status || "网络错误"})`;
    }
    renderComposerAttachments();
    updateControlState();
  });
  request.addEventListener("error", () => {
    if (item.request !== request) return;
    item.request = null;
    item.status = "error";
    item.error = "无法连接上传服务";
    renderComposerAttachments();
    updateControlState();
  });
  request.send(item.file);
}

export function collectTransferFiles(transfer) {
  const files = [];
  const seen = new Set();
  const add = (file) => {
    if (!(file instanceof File)) return;
    const key = `${file.name}\0${file.size}\0${file.lastModified}\0${file.type}`;
    if (seen.has(key)) return;
    seen.add(key);
    files.push(file);
  };
  for (const item of Array.from(transfer?.items || [])) {
    if (item.kind === "file") add(item.getAsFile());
  }
  for (const file of Array.from(transfer?.files || [])) add(file);
  return files;
}

export function addComposerFiles(files) {
  if (!state.capabilities?.attachments) return;
  const incoming = Array.isArray(files) ? files : Array.from(files || []);
  if (!incoming.length) return;
  const available = Math.max(0, MAX_ATTACHMENTS - state.composerAttachments.length);
  if (incoming.length > available) {
    showToast(`每条消息最多添加 ${MAX_ATTACHMENTS} 个附件，已忽略 ${incoming.length - available} 个`, "error");
  }
  const accepted = incoming.slice(0, available);
  for (const file of accepted) {
    if (!(file instanceof File) || file.size <= 0) {
      showToast(`${file?.name || "附件"} 是空文件`, "error");
      continue;
    }
    const image = file.type.startsWith("image/");
    const item = {
      localId: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
      file,
      sessionId: attachmentSessionId(),
      name: file.name,
      mime: file.type,
      kind: image ? "image" : "text",
      size: file.size,
      status: "uploading",
      progress: 0,
      previewUrl: image ? URL.createObjectURL(file) : "",
      request: null,
      error: ""
    };
    state.composerAttachments.push(item);
    uploadComposerAttachment(item);
  }
  renderComposerAttachments();
  updateControlState();
}

export function removeComposerAttachment(item, deleteRemote = true) {
  item.request?.abort();
  item.request = null;
  state.composerAttachments = state.composerAttachments.filter((candidate) => candidate !== item);
  if (item.previewUrl) URL.revokeObjectURL(item.previewUrl);
  if (deleteRemote && item.id && item.sessionId) {
    apiRequest(`/api/attachments/${encodeURIComponent(item.id)}?session_id=${encodeURIComponent(item.sessionId)}`, { method: "DELETE" }).catch(() => {});
  }
  renderComposerAttachments();
  updateControlState();
}

export function clearComposerAttachments(deleteRemote = true) {
  for (const item of [...state.composerAttachments]) removeComposerAttachment(item, deleteRemote);
  elements.attachmentInput.value = "";
}

export function committedComposerAttachments() {
  const attachments = state.composerAttachments.filter((item) => item.status === "ready").map((item) => ({
    id: item.id,
    url: item.url,
    name: item.name,
    mime: item.mime,
    kind: item.kind,
    size: item.size,
    width: item.width || 0,
    height: item.height || 0
  }));
  clearComposerAttachments(false);
  return attachments;
}
