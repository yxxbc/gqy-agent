export function deepClone(value) {
  if (typeof structuredClone === "function") return structuredClone(value);
  return JSON.parse(JSON.stringify(value));
}

export function escapeText(value) {
  const span = document.createElement("span");
  span.textContent = String(value);
  return span.innerHTML;
}
