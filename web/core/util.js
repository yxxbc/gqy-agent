export function deepClone(value) {
  if (typeof structuredClone === "function") return structuredClone(value);
  return JSON.parse(JSON.stringify(value));
}

export function escapeText(value) {
  const span = document.createElement("span");
  span.textContent = String(value);
  return span.innerHTML;
}

/// 按点路径（"a.b.c"）读写嵌套对象。设置页的配置草稿全靠它们。
export function getPath(object, path, fallback) {
  let value = object;
  for (const key of String(path).split(".")) {
    if (value == null || typeof value !== "object" || !(key in value)) return fallback;
    value = value[key];
  }
  return value;
}

export function setPath(object, path, value) {
  const keys = String(path).split(".");
  let target = object;
  for (const key of keys.slice(0, -1)) {
    if (!target[key] || typeof target[key] !== "object") target[key] = {};
    target = target[key];
  }
  target[keys[keys.length - 1]] = value;
}

export function deletePath(object, path) {
  const keys = String(path).split(".");
  let target = object;
  for (const key of keys.slice(0, -1)) {
    if (!target?.[key] || typeof target[key] !== "object") return;
    target = target[key];
  }
  delete target[keys[keys.length - 1]];
}
