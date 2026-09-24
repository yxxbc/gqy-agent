import { state } from "../../state/store.js";

/*
 * display.reasoning 只决定后端产生什么(摘要/完整/不产生);
 * WebUI 是否渲染仅以「有没有思考内容」为准,hidden 时若仍收到文本则不渲染(保底)。
 * 默认展开/收起由本地偏好 gqy.web.reasoningExpanded 决定,与 summary/full 无关。
 */
export function reasoningHidden() {
  return state.display?.reasoning === "hidden";
}

export function normalizeReasoningTitle(value) {
  const title = String(value || "").trim().replace(/^[*#\s]+|[*#\s]+$/g, "");
  if (!title || /^正在(?:思考)?(?:\.{3}|…+)?$/u.test(title)) return "";
  return title;
}

export function splitReasoningText(value) {
  const raw = String(value || "").trim();
  const bold = raw.match(/^\*\*([^\n*]{1,160})\*\*(?:\r?\n){0,2}([\s\S]*)$/);
  if (bold) return { title: normalizeReasoningTitle(bold[1]), body: bold[2].trim() };
  const heading = raw.match(/^#{1,6}\s+([^\n]{1,160})(?:\r?\n)+([\s\S]*)$/);
  if (heading) return { title: normalizeReasoningTitle(heading[1]), body: heading[2].trim() };
  return { title: "", body: raw };
}

// 窥视槽只放尾巴:换行折成空格,取最后 160 字,够撑满一行还不至于每个 delta 都重排一大段
export function reasoningPeekText(text) {
  return String(text || "").replace(/\s+/g, " ").trimEnd().slice(-160);
}

// 写入窥视文字并量一下:放得下就左对齐紧跟着时间;放不下才切到尾部可见 + 左侧渐隐
export function setReasoningPeek(peek, text) {
  if (!peek) return;
  peek.textContent = reasoningPeekText(text);
  const slot = peek.parentElement;
  if (slot) slot.classList.toggle("is-overflow", peek.scrollWidth > slot.clientWidth + 1);
}
