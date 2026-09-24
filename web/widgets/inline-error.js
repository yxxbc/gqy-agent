import { elements } from "../state/elements.js";

export function showInlineError(message) {
  const text = String(message || "操作未完成").trim();
  elements.errorRegion.textContent = text;
  elements.errorRegion.hidden = !text;
}

export function clearInlineError() {
  elements.errorRegion.textContent = "";
  elements.errorRegion.hidden = true;
}
