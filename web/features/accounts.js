import { apiRequest } from "../core/api.js";
import { asFiniteNumber, formatDateTime, formatRelativeTime } from "../core/format.js";
import { showToast } from "../core/toast.js";
import { isAdmin } from "./auth.js";
import { usageFmt, usageFmtCost } from "./console/usage.js";
import { loadPersonaCard } from "./persona-list.js";
import { elements } from "../state/elements.js";
import { state } from "../state/store.js";

/* ── 账号面板(阶段 5 多用户) ── */
export const accountState = { names: new Map(), loadSeq: 0 };

export function accountLabel(accountId) {
  if (!accountId) return "未署名";
  const entry = accountState.names.get(accountId);
  if (!entry) return accountId;
  return entry.display_name && entry.display_name !== entry.username
    ? `${entry.display_name} (${entry.username})`
    : entry.username;
}

export async function loadAccountNames() {
  const response = await apiRequest("/api/admin/accounts");
  const data = await response.json();
  accountState.names = new Map((data.accounts || []).map((account) => [account.id, account]));
  return data.accounts || [];
}

export function showAccountError(message) {
  elements.accountError.textContent = message || "";
  elements.accountError.hidden = !message;
}

export async function loadAccountPanel() {
  const seq = ++accountState.loadSeq;
  showAccountError("");
  const account = state.account || {};
  const noRow = !account.account_id;
  elements.accountUsername.value = account.username || (noRow ? "(访问密码登录)" : "");
  elements.accountDisplayName.value = account.display_name || "";
  elements.accountDisplayName.disabled = noRow;
  elements.accountCurrentPassword.disabled = noRow;
  elements.accountNewPassword.disabled = noRow;
  elements.accountSave.disabled = noRow;
  elements.accountSelfHint.textContent = noRow
    ? "用访问密码登录的是机器级管理员,密码在启动参数里改;用管理员用户名登录可以改显示名。"
    : account.admin ? "管理员" : "成员";
  elements.accountSave.disabled = false;
  elements.accountStamp.textContent = "";
  try {
    const me = await apiRequest("/api/account").then((response) => response.json());
    if (seq !== accountState.loadSeq) return;
    elements.accountProfile.value = typeof me.profile === "string" ? me.profile : "";
    accountState.profile = elements.accountProfile.value;
  } catch (_) {
    // 档案读不到就留空,保存时再报
  }
  if (!isAdmin()) {
    loadPersonaCard();
    return;
  }
  elements.inviteFresh.hidden = true;
  try {
    const [accounts, invitesResponse, usageResponse] = await Promise.all([
      loadAccountNames(),
      apiRequest("/api/admin/invites").then((response) => response.json()),
      apiRequest("/api/admin/usage/accounts?range=30d").then((response) => response.json()).catch(() => ({ accounts: [] })),
    ]);
    if (seq !== accountState.loadSeq) return;
    renderInviteRows(invitesResponse.invites || []);
    renderAccountRows(accounts, usageResponse.accounts || []);
  } catch (error) {
    if (seq !== accountState.loadSeq) return;
    elements.accountStamp.textContent = `载入失败:${error.message || error}`;
  }
}

export function renderInviteRows(invites) {
  const body = elements.inviteRows;
  body.replaceChildren();
  if (!invites.length) {
    body.innerHTML = `<tr><td colspan="5" class="acct-muted">还没有邀请码</td></tr>`;
    return;
  }
  const statusLabel = { open: "可用", used: "已使用", expired: "已过期" };
  for (const invite of invites) {
    const row = document.createElement("tr");
    const usedBy = invite.used_by ? accountLabel(invite.used_by) : "—";
    row.innerHTML = `<td>${statusLabel[invite.status] || invite.status}</td><td>${formatDateTime(invite.created_at)}</td><td>${formatDateTime(invite.expires_at)}</td><td></td><td></td>`;
    row.children[3].textContent = usedBy;
    if (invite.status !== "used") {
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "secondary-button acct-row-action";
      remove.textContent = "作废";
      remove.addEventListener("click", async () => {
        remove.disabled = true;
        try {
          await apiRequest(`/api/admin/invites/${encodeURIComponent(invite.id)}`, { method: "DELETE" });
          loadAccountPanel();
        } catch (error) {
          showToast(error.message || "作废失败", "error");
          remove.disabled = false;
        }
      });
      row.children[4].appendChild(remove);
    }
    body.appendChild(row);
  }
}

export function renderAccountRows(accounts, usage) {
  const body = elements.accountRows;
  body.replaceChildren();
  const usageById = new Map(usage.map((entry) => [entry.acct, entry]));
  for (const account of accounts) {
    const row = document.createElement("tr");
    const spent = usageById.get(account.id);
    const cells = [
      account.username,
      account.display_name,
      account.admin ? "管理员" : "成员",
      account.last_login_at ? formatRelativeTime(account.last_login_at) : "从未",
      spent ? usageFmt(asFiniteNumber(spent.total)) : "0",
      spent ? (usageFmtCost(asFiniteNumber(spent.cost)) || "—") : "—",
    ];
    cells.forEach((text, index) => {
      const cell = document.createElement("td");
      if (index >= 4) cell.className = "num";
      cell.textContent = text;
      row.appendChild(cell);
    });
    if (account.disabled) row.classList.add("acct-muted");
    const actions = document.createElement("td");
    const isSelf = state.account?.account_id === account.id;
    const toggle = document.createElement("button");
    toggle.type = "button";
    toggle.className = "secondary-button acct-row-action";
    toggle.textContent = account.disabled ? "恢复" : "停用";
    toggle.disabled = isSelf;
    toggle.addEventListener("click", () => patchAccount(account.id, { disabled: !account.disabled }, toggle));
    const reset = document.createElement("button");
    reset.type = "button";
    reset.className = "secondary-button acct-row-action";
    reset.textContent = "重设密码";
    reset.addEventListener("click", () => {
      const password = window.prompt(`给 ${account.username} 设一个新密码:`);
      if (password == null) return;
      patchAccount(account.id, { password }, reset);
    });
    actions.append(toggle, reset);
    row.appendChild(actions);
    body.appendChild(row);
  }
}

export async function patchAccount(accountId, patch, button) {
  if (button) button.disabled = true;
  try {
    await apiRequest(`/api/admin/accounts/${encodeURIComponent(accountId)}`, {
      method: "PATCH",
      body: JSON.stringify(patch)
    });
    loadAccountPanel();
  } catch (error) {
    showToast(error.message || "操作失败", "error");
    if (button) button.disabled = false;
  }
}

export async function createInvite() {
  elements.inviteCreate.disabled = true;
  try {
    const response = await apiRequest("/api/admin/invites", { method: "POST", body: JSON.stringify({}) });
    const data = await response.json();
    elements.inviteFresh.textContent = data.code || "";
    elements.inviteFresh.hidden = !data.code;
    const invitesResponse = await apiRequest("/api/admin/invites").then((r) => r.json());
    renderInviteRows(invitesResponse.invites || []);
  } catch (error) {
    showToast(error.message || "生成失败", "error");
  } finally {
    elements.inviteCreate.disabled = false;
  }
}

export async function saveAccount() {
  const patch = {};
  const displayName = elements.accountDisplayName.value.trim();
  if (displayName && displayName !== (state.account?.display_name || "")) patch.display_name = displayName;
  const newPassword = elements.accountNewPassword.value;
  if (newPassword) {
    patch.password = newPassword;
    patch.current_password = elements.accountCurrentPassword.value;
  }
  const profile = elements.accountProfile.value;
  if (profile !== (accountState.profile ?? "")) patch.profile = profile;
  if (!Object.keys(patch).length) return showAccountError("没有要保存的改动");
  elements.accountSave.disabled = true;
  try {
    const response = await apiRequest("/api/account", { method: "PATCH", body: JSON.stringify(patch) });
    const data = await response.json();
    if (data.account && state.account) {
      state.account.display_name = data.account.display_name;
    }
    elements.accountCurrentPassword.value = "";
    elements.accountNewPassword.value = "";
    if (patch.profile != null) accountState.profile = patch.profile;
    showAccountError("");
    showToast("已保存", "success");
  } catch (error) {
    showAccountError(error.message || "保存失败");
  } finally {
    elements.accountSave.disabled = false;
  }
}
