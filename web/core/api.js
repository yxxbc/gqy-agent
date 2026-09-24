export class ApiError extends Error {
  constructor(message, status) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

export async function readErrorMessage(response) {
  try {
    const payload = await response.json();
    const message = payload?.error?.message;
    if (typeof message === "string" && message.trim()) return message.trim();
  } catch (_) {
    // Fall through to an HTTP status message.
  }
  return `请求失败 (${response.status})`;
}

/// 401 由登录功能处理:它在启动时登记一个回调,返回 true 表示已接手(跳去登录页)。
/// 请求层因此不需要知道登录页长什么样,也就不依赖任何功能模块。
export let apiUnauthorizedHandler = null;

export function onApiUnauthorized(handler) {
  apiUnauthorizedHandler = handler;
}

export async function apiRequest(path, options = {}) {
  const headers = new Headers(options.headers || {});
  headers.set("Accept", "application/json");
  if (options.body != null && !headers.has("Content-Type")) headers.set("Content-Type", "application/json");
  let response;
  try {
    response = await fetch(path, { ...options, headers, credentials: "same-origin" });
  } catch (_) {
    throw new ApiError("无法连接 顾清影 WebUI", 0);
  }
  if (response.status === 401 && !path.startsWith("/api/auth/") && apiUnauthorizedHandler?.()) {
    throw new ApiError("登录已过期,请重新登录", 401);
  }
  if (!response.ok) throw new ApiError(await readErrorMessage(response), response.status);
  return response;
}
