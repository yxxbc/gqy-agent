import { apiRequest } from "../core/api.js";
import { createIcon } from "../core/icons.js";
import { loadBootstrap } from "./boot.js";
import { updateControlState } from "./composer/input.js";
import { consoleClose, consoleIsOpen, setConsolePanel } from "./console/panel.js";
import { clearViewSyncTimer, closeEventSource } from "./live/sse.js";
import { clearQuestionDock } from "./questions.js";
import { disposeAllLiveRuns } from "./sessions/view.js";
import { setConnectionStatus } from "./status.js";
import { elements } from "../state/elements.js";
import { state } from "../state/store.js";

/// 只有本模块用的状态（从 state/store.js 分出来的私有分片）。
const authState = {
  loginSubmitting: false
};

export function showBlockedState(unauthorized, message = "", { expired = false } = {}) {
  state.blocked = true;
  document.body.classList.toggle("is-login", Boolean(unauthorized));
  document.body.classList.toggle("is-blocked", true);
  state.viewRunningTurnId = null;
  clearViewSyncTimer();
  disposeAllLiveRuns();
  clearQuestionDock();
  closeEventSource();
  elements.loadingState.hidden = true;
  elements.timeline.hidden = true;
  elements.emptyState.hidden = true;
  elements.blockedState.hidden = false;
  elements.blockedTitle.textContent = unauthorized ? "登录顾清影" : "无法载入顾清影 WebUI";
  elements.blockedMessage.textContent = unauthorized
    ? (expired ? "登录已过期,请重新登录。" : "输入用户名和密码以继续。")
    : message || "本地服务暂时无法访问";
  elements.loginForm.hidden = !unauthorized;
  elements.registerForm.hidden = true;
  elements.setupForm.hidden = true;
  elements.retryBootstrapButton.hidden = unauthorized;
  if (unauthorized) refreshLoginHint();
  elements.loginError.textContent = "";
  elements.loginError.hidden = true;
  elements.registerError.textContent = "";
  elements.registerError.hidden = true;
  setLoginSubmitting(false);
  setRegisterSubmitting(false);
  setConnectionStatus(unauthorized ? "blocked" : "offline");
  updateControlState();
  if (unauthorized) window.requestAnimationFrame(() => elements.loginPassword.focus());
}

/// 还没建管理员账号:登录页直说「输入内置口令」;之后就是普通的用户名+密码。
export async function refreshLoginHint() {
  try {
    const status = await fetch("/api/auth/status", { cache: "no-store" }).then((response) => response.json());
    if (!document.body.classList.contains("is-login") || !elements.loginForm || elements.loginForm.hidden) return;
    if (elements.blockedMessage.textContent.startsWith("登录已过期")) return;
    if (status?.setup_pending) {
      elements.blockedMessage.textContent = "首次使用:用户名 gqy、密码 gqy 登录,然后创建管理员账号。";
      elements.loginUsername.placeholder = "gqy";
    } else {
      elements.blockedMessage.textContent = "输入用户名和密码以继续。";
      elements.loginUsername.placeholder = "用户名";
    }
  } catch (_) { /* 提示拿不到就用默认文案 */ }
}

/// 引导第 0 步:拿内置口令登进来、还没有管理员账号——先建号,建完直接以它登录。
export function showSetupAdmin() {
  state.blocked = true;
  document.body.classList.add("is-login", "is-blocked");
  elements.loadingState.hidden = true;
  elements.timeline.hidden = true;
  elements.emptyState.hidden = true;
  elements.blockedState.hidden = false;
  elements.blockedTitle.textContent = "创建管理员账号";
  elements.blockedMessage.textContent = "内置账号 gqy 只用这一次;建好账号后用它登录,别人凭邀请码注册。";
  elements.loginForm.hidden = true;
  elements.registerForm.hidden = true;
  elements.setupForm.hidden = false;
  elements.retryBootstrapButton.hidden = true;
  elements.setupError.textContent = "";
  elements.setupError.hidden = true;
  if (!elements.setupUsername.value) elements.setupUsername.value = state.account?.setup_username || "";
  setConnectionStatus("blocked");
  updateControlState();
  window.requestAnimationFrame(() => (elements.setupUsername.value ? elements.setupPassword : elements.setupUsername).focus());
}

export async function submitSetupAdmin() {
  if (state.setupSubmitting) return;
  const username = elements.setupUsername.value.trim();
  const password = elements.setupPassword.value;
  const fail = (text, focus) => { elements.setupError.textContent = text; elements.setupError.hidden = false; focus?.focus(); };
  if (!username) return fail("先起个用户名", elements.setupUsername);
  if (!password) return fail("请输入密码", elements.setupPassword);
  if (password !== elements.setupPassword2.value) return fail("两次密码不一样", elements.setupPassword2);
  elements.setupError.hidden = true;
  state.setupSubmitting = true;
  elements.setupSubmit.disabled = true;
  try {
    await apiRequest("/api/auth/setup-admin", {
      method: "POST",
      body: JSON.stringify({ username, display_name: elements.setupDisplayName.value.trim(), password }),
    });
    elements.setupPassword.value = "";
    elements.setupPassword2.value = "";
    await loadBootstrap();
  } catch (error) {
    fail(error.message || "创建失败", elements.setupUsername);
  } finally {
    state.setupSubmitting = false;
    elements.setupSubmit.disabled = false;
  }
}

/// 成员看不到管理台(供应商/密钥、共享人格、脚本、QQ、记忆库……),
/// 只留数据统计(自己的)与账号页。没开口令时人人都是管理员。
export function isAdmin() {
  return state.capabilities?.admin !== false;
}

export function applyRoleVisibility() {
  const admin = isAdmin();
  const multiUser = Boolean(state.capabilities?.multi_user);
  for (const element of document.querySelectorAll("[data-admin-only]")) element.hidden = !admin;
  for (const element of document.querySelectorAll("[data-multi-user-only]")) element.hidden = !multiUser;
  for (const element of document.querySelectorAll("[data-member-only]")) element.hidden = admin || !multiUser;
  // 成员的记忆/知识库/表情包/记账面板跟当前人格开了什么走(服务端算好的清单)。
  const dashboards = Array.isArray(state.account?.persona?.dashboards) ? state.account.persona.dashboards : [];
  for (const panel of ["memory", "kb", "memes", "ledger"]) {
    const item = elements.consoleView.querySelector(`.con-rail-item[data-console-panel="${panel}"]`);
    if (item) item.hidden = !admin && !dashboards.includes(panel);
  }
  if (!admin && consoleIsOpen() && isAdminOnlyPanel(state.consolePanel)) setConsolePanel("usage");
}

export function isAdminOnlyPanel(panel) {
  const item = elements.consoleView.querySelector(`.con-rail-item[data-console-panel="${panel}"]`);
  return Boolean(item?.hasAttribute("data-admin-only") || item?.hidden);
}

export function showRegisterForm(show) {
  elements.loginForm.hidden = show;
  elements.registerForm.hidden = !show;
  elements.blockedMessage.textContent = show ? "凭管理员发的邀请码创建账号。" : "输入用户名和密码以继续。";
  window.requestAnimationFrame(() => (show ? elements.registerInvite : elements.loginUsername).focus());
}

export function setRegisterSubmitting(submitting) {
  state.registerSubmitting = Boolean(submitting);
  for (const input of [elements.registerInvite, elements.registerUsername, elements.registerDisplayName, elements.registerPassword]) {
    input.disabled = state.registerSubmitting;
  }
  elements.registerSubmit.disabled = state.registerSubmitting;
  elements.registerSubmit.classList.toggle("is-loading", state.registerSubmitting);
  elements.registerSubmitLabel.textContent = state.registerSubmitting ? "正在注册" : "注册并登录";
}

export async function submitRegister() {
  if (state.registerSubmitting) return;
  const invite = elements.registerInvite.value.trim();
  const username = elements.registerUsername.value.trim();
  const display_name = elements.registerDisplayName.value.trim();
  const password = elements.registerPassword.value;
  const fail = (message, input) => {
    elements.registerError.textContent = message;
    elements.registerError.hidden = false;
    input?.focus();
  };
  if (!invite) return fail("请输入邀请码", elements.registerInvite);
  if (!username) return fail("请输入用户名", elements.registerUsername);
  if (!password) return fail("请输入密码", elements.registerPassword);
  elements.registerError.hidden = true;
  setRegisterSubmitting(true);
  try {
    await apiRequest("/api/auth/register", {
      method: "POST",
      body: JSON.stringify({ invite, username, display_name, password })
    });
    elements.registerPassword.value = "";
    elements.registerInvite.value = "";
    await loadBootstrap();
  } catch (error) {
    fail(error.message || "注册失败", elements.registerInvite);
  } finally {
    setRegisterSubmitting(false);
  }
}

export async function logout() {
  try {
    await apiRequest("/api/auth/logout", { method: "POST" });
  } catch (_) {
    // 令牌已失效也一样回到登录页
  }
  if (consoleIsOpen()) consoleClose();
  showBlockedState(true);
}

export function setLoginSubmitting(submitting) {
  authState.loginSubmitting = Boolean(submitting);
  elements.loginUsername.disabled = authState.loginSubmitting;
  elements.loginPassword.disabled = authState.loginSubmitting;
  elements.loginSubmit.disabled = authState.loginSubmitting;
  elements.loginSubmit.classList.toggle("is-loading", authState.loginSubmitting);
  elements.loginSubmitLabel.textContent = authState.loginSubmitting ? "正在登录" : "登录";
  const icon = elements.loginSubmit.querySelector(".icon-slot");
  if (icon) icon.replaceChildren(createIcon(authState.loginSubmitting ? "loader-circle" : "log-in"));
}

export async function submitLogin() {
  if (authState.loginSubmitting) return;
  const username = elements.loginUsername.value.trim();
  const password = elements.loginPassword.value;
  if (!username) {
    elements.loginError.textContent = "请输入用户名";
    elements.loginError.hidden = false;
    elements.loginUsername.focus();
    return;
  }
  if (!password) {
    elements.loginError.textContent = "请输入密码";
    elements.loginError.hidden = false;
    elements.loginPassword.focus();
    return;
  }
  elements.loginError.textContent = "";
  elements.loginError.hidden = true;
  setLoginSubmitting(true);
  try {
    await apiRequest("/api/auth/login", {
      method: "POST",
      body: JSON.stringify({ username, password })
    });
    elements.loginPassword.value = "";
    await loadBootstrap();
  } catch (error) {
    elements.loginError.textContent = error.status === 401
      ? "用户名或密码不正确，请重试"
      : error.message || "登录失败";
    elements.loginError.hidden = false;
    window.requestAnimationFrame(() => {
      elements.loginPassword.focus();
      elements.loginPassword.select();
    });
  } finally {
    setLoginSubmitting(false);
  }
}
