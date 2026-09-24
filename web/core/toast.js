export let toastTimer = 0;

export function showToast(message, type = "info") {
  const toast = document.createElement("div");
  toast.className = `toast${type === "error" ? " is-error" : ""}`;
  toast.textContent = String(message || "操作未完成");
  document.getElementById("toastRegion")?.replaceChildren(toast);
  if (toastTimer) window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    if (toast.isConnected) toast.remove();
  }, type === "error" ? 6000 : 3000);
}
