export function getFocusable(container) {
  return Array.from(container.querySelectorAll("button:not(:disabled), input:not(:disabled), textarea:not(:disabled), a[href], [tabindex]:not([tabindex='-1'])"))
    .filter((node) => !node.hidden && node.getClientRects().length > 0);
}
