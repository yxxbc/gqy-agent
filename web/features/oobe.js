import { apiRequest } from "../core/api.js";
import { showToast } from "../core/toast.js";
import { accountState, loadAccountPanel } from "./accounts.js";
import { isAdmin } from "./auth.js";
import { loadBootstrap } from "./boot.js";
import { focusComposerIfDesktop } from "./composer/input.js";
import { consoleIsOpen } from "./console/panel.js";
import { elements } from "../state/elements.js";
import { state } from "../state/store.js";

/* ── 欢迎引导 / 成员人格(阶段 8) ── */
export const oobeState = { open: false, step: 1, mode: "private", editing: null, avatarFile: null, boardFile: null, plugins: [], busy: false, reason: "first" };

export function oobeShowError(message) {
  elements.oobeError.textContent = message || "";
  elements.oobeError.hidden = !message;
}

export function oobeSetStep(step) {
  oobeState.step = step;
  for (const pane of elements.oobePanes.querySelectorAll(".oobe-pane")) {
    const active = Number(pane.dataset.oobeStep) === step;
    pane.classList.toggle("is-active", active);
    if (active) {
      pane.style.animation = "none";
      void pane.offsetWidth; // 重新触发入场动画
      pane.style.animation = "";
    }
  }
  for (const item of elements.oobeSteps.querySelectorAll("li")) {
    const n = Number(item.dataset.step);
    item.classList.toggle("on", n === step);
    item.classList.toggle("done", n < step);
  }
  const last = step === 3;
  elements.oobeBack.hidden = step === 1 || step === 4;
  elements.oobeNext.hidden = step === 4;
  // 首启是「先跳过」(跳过建号引导);新建/编辑人格是「取消」(直接关掉不保存)——
  // 之前这两种模式下这颗键整个藏了,于是新建人格没有任何退出口(用户 #164)。
  elements.oobeSkip.hidden = step === 4;
  elements.oobeSkip.textContent = oobeState.reason === "first" ? "先跳过" : "取消";
  elements.oobeNextLabel.textContent = last ? (oobeState.editing ? "保存" : "开始聊天") : "下一步";
  oobeShowError("");
  if (step === 1) window.requestAnimationFrame(() => elements.oobeName.focus());
  if (step === 3) window.requestAnimationFrame(() => elements.oobeProfile.focus());
}

export function oobeSetMode(mode) {
  oobeState.mode = mode;
  for (const option of elements.oobe.querySelectorAll(".oobe-option")) {
    const on = option.dataset.personaMode === mode;
    option.classList.toggle("is-on", on);
    option.setAttribute("aria-checked", on ? "true" : "false");
  }
  elements.oobePersonaForm.hidden = mode !== "private";
}

export function oobeRenderPlugins(options, enabled) {
  elements.oobePlugins.replaceChildren();
  const on = new Set(enabled || options.map((option) => option.id));
  for (const option of options) {
    const label = document.createElement("label");
    label.className = "oobe-plugin";
    const input = document.createElement("input");
    input.type = "checkbox";
    input.value = option.id;
    input.checked = on.has(option.id);
    const text = document.createElement("span");
    const title = document.createElement("b");
    title.textContent = option.label || option.id;
    text.appendChild(title);
    text.append(option.hint || "");
    label.append(input, text);
    elements.oobePlugins.appendChild(label);
  }
  if (!options.length) elements.oobePlugins.innerHTML = `<p class="u-hint">没有可选的功能。</p>`;
}

/// 脚本/技能这类「逐个勾」的块:没有条目就整块藏起来;enabled 为 null = 全勾。
export function oobeRenderChecklist(wrapId, containerId, items, enabled) {
  const wrap = document.getElementById(wrapId);
  const container = document.getElementById(containerId);
  container.replaceChildren();
  wrap.hidden = !items.length;
  const on = enabled ? new Set(enabled) : null;
  for (const item of items) {
    const label = document.createElement("label");
    label.className = "oobe-plugin";
    const input = document.createElement("input");
    input.type = "checkbox";
    input.value = item.id;
    input.checked = on ? on.has(item.id) : true;
    const text = document.createElement("span");
    const title = document.createElement("b");
    title.textContent = item.label || item.id;
    text.appendChild(title);
    text.append(item.hint || "");
    label.append(input, text);
    container.appendChild(label);
  }
}

/// 块藏着(没东西可勾)= null = 全部;摆出来了就按勾选发明细。
export function oobeSelectedChecklist(wrapId, containerId) {
  if (document.getElementById(wrapId).hidden) return null;
  return [...document.querySelectorAll(`#${containerId} input:checked`)].map((input) => input.value);
}

export function oobeRenderScripts(scripts, enabled) {
  oobeRenderChecklist("oobeScriptsWrap", "oobeScripts", scripts, enabled);
}

export function oobeRenderSkills(skills, enabled) {
  oobeRenderChecklist("oobeSkillsWrap", "oobeSkills", skills, enabled);
}

export function oobeSelectedScripts() {
  return oobeSelectedChecklist("oobeScriptsWrap", "oobeScripts");
}

export function oobeSelectedSkills() {
  return oobeSelectedChecklist("oobeSkillsWrap", "oobeSkills");
}

export function oobeSelectedPlugins() {
  return [...elements.oobePlugins.querySelectorAll("input:checked")].map((input) => input.value);
}

export function previewImageFile(file, image) {
  if (!file) return;
  const url = URL.createObjectURL(file);
  image.onload = () => URL.revokeObjectURL(url);
  image.src = url;
  image.hidden = false;
}

/// reason: first(注册后)/create(账号页新建)/edit(改一个已有的)
export async function openOobe({ reason = "first", persona = null } = {}) {
  if (oobeState.open || isAdmin()) return;
  oobeState.open = true;
  oobeState.reason = reason;
  oobeState.editing = persona ? persona.slug : null;
  oobeState.avatarFile = null;
  oobeState.boardFile = null;
  elements.oobe.hidden = false;
  document.body.classList.add("is-oobe");
  elements.oobeName.value = persona?.name || "";
  elements.oobeDesc.value = persona?.description || "";
  elements.oobePrompt.value = "";
  elements.oobeAvatarPreview.hidden = true;
  elements.oobeAvatarPreview.removeAttribute("src");
  elements.oobeProfile.value = "";
  oobeSetMode("private");
  elements.oobe.querySelector(".oobe-choice").hidden = reason !== "first";
  let options = [];
  let scripts = [];
  let skills = [];
  try {
    const data = await apiRequest("/api/account/personas").then((response) => response.json());
    options = data.plugins || [];
    scripts = data.scripts || [];
    skills = data.skills || [];
    if (data.shared?.name) elements.oobeSharedName.textContent = data.shared.name;
    elements.oobeSharedHint.textContent = data.shared?.maintainer
      ? `${data.shared.maintainer} 维护的预置人格,不可修改`
      : "预置人格,不可修改";
    if (data.shared?.name) elements.oobeSharedName.textContent = data.shared.name;
    elements.oobeSharedHint.textContent = data.shared?.maintainer
      ? `${data.shared.maintainer} 维护的预置人格,不可修改`
      : "预置人格,不可修改";
    elements.oobeProfile.value = data.prompt || "";
    if (data.member_personas === false && reason !== "first") {
      showToast("管理员关闭了成员自建人格", "error");
      closeOobe();
      return;
    }
    if (data.member_personas === false) oobeSetMode("shared");
    if (persona) {
      elements.oobePrompt.value = persona.prompt || "";
      if (persona.avatar_url) { elements.oobeAvatarPreview.src = `${persona.avatar_url}&v=${Date.now()}`; elements.oobeAvatarPreview.hidden = false; }
    }
  } catch (error) {
    oobeShowError(error.message || "载入失败");
  }
  oobeRenderPlugins(options, persona ? persona.plugins : null);
  oobeRenderScripts(scripts, persona ? persona.scripts : null);
  oobeRenderSkills(skills, persona ? persona.skills : null);
  oobeSetStep(1);
}

export function closeOobe() {
  oobeState.open = false;
  elements.oobe.hidden = true;
  document.body.classList.remove("is-oobe");
}

export async function uploadPersonaImage(slug, file, board) {
  if (!file) return;
  await apiRequest(`/api/account/personas/${encodeURIComponent(slug)}/image${board ? "?board=1" : ""}`, {
    method: "PUT",
    headers: { "Content-Type": file.type || "application/octet-stream" },
    body: file,
  });
}

export async function oobeFinish() {
  if (oobeState.busy) return;
  oobeState.busy = true;
  elements.oobeNext.disabled = true;
  elements.oobeNext.classList.add("is-loading");
  try {
    let slug = null;
    let displayName = "GQY";
    if (oobeState.mode === "private") {
      const name = elements.oobeName.value.trim();
      const prompt = elements.oobePrompt.value.trim();
      if (!name) { oobeSetStep(1); throw new Error("先起个名字"); }
      const body = {
        name, prompt,
        description: elements.oobeDesc.value.trim(),
        plugins: oobeSelectedPlugins(),
        scripts: oobeSelectedScripts(),
        skills: oobeSelectedSkills(),
        activate: true,
      };
      let persona;
      if (oobeState.editing) {
        const response = await apiRequest(`/api/account/personas/${encodeURIComponent(oobeState.editing)}`, { method: "PUT", body: JSON.stringify(body) });
        persona = (await response.json()).persona;
      } else {
        const response = await apiRequest("/api/account/personas", { method: "POST", body: JSON.stringify(body) });
        persona = (await response.json()).persona;
      }
      slug = persona.slug;
      displayName = persona.name;
      await uploadPersonaImage(slug, oobeState.avatarFile, false);
    }
    const profile = elements.oobeProfile.value;
    await apiRequest("/api/account", { method: "PATCH", body: JSON.stringify({ profile }) });
    await apiRequest("/api/account/active-persona", { method: "PUT", body: JSON.stringify({ slug, oobe_done: true }) });
    accountState.profile = profile;
    await loadBootstrap();
    if (oobeState.editing) {
      // 编辑现有人格=直接保存关闭,不走 onboarding 的「已准备好」庆祝页(#146:
      // 编辑不该重新进 OOBE 的那套开场/收尾)。
      closeOobe();
      if (consoleIsOpen()) loadAccountPanel();
      showToast(`${displayName} 已更新`, "success");
    } else {
      elements.oobeDoneTitle.textContent = `${displayName} 准备好了`;
      elements.oobeDoneText.textContent = oobeState.mode === "private"
        ? "接下来的会话用这个人格。改设定、换头像在控制台的账号页。"
        : "你用的是共享的 顾清影;想要自己的人格,随时在账号页里创建。";
      const avatar = oobeState.avatarFile ? URL.createObjectURL(oobeState.avatarFile) : (slug ? `/api/persona/avatar?scope=${encodeURIComponent(slug)}` : "/assets/gqy-logo.png");
      elements.oobeDoneAvatar.onerror = () => { elements.oobeDoneAvatar.hidden = true; };
      elements.oobeDoneAvatar.src = avatar;
      elements.oobeDoneAvatar.hidden = false;
      oobeSetStep(4);
      window.setTimeout(() => {
        closeOobe();
        if (consoleIsOpen()) loadAccountPanel();
        else if (state.sessions.length) focusComposerIfDesktop();
      }, 1400);
    }
  } catch (error) {
    oobeShowError(error.message || "保存失败");
  } finally {
    oobeState.busy = false;
    elements.oobeNext.disabled = false;
    elements.oobeNext.classList.remove("is-loading");
  }
}

export function bindOobeEvents() {
  for (const option of elements.oobe.querySelectorAll(".oobe-option")) {
    option.addEventListener("click", () => oobeSetMode(option.dataset.personaMode));
  }
  elements.oobeAvatarInput.addEventListener("change", () => {
    oobeState.avatarFile = elements.oobeAvatarInput.files?.[0] || null;
    previewImageFile(oobeState.avatarFile, elements.oobeAvatarPreview);
  });
  elements.oobeBack.addEventListener("click", () => oobeSetStep(Math.max(1, oobeState.step - 1)));
  elements.oobeNext.addEventListener("click", () => {
    if (oobeState.step === 1 && oobeState.mode === "private") {
      if (!elements.oobeName.value.trim()) return oobeShowError("先起个名字");
    }
    if (oobeState.step === 1 && oobeState.mode === "shared") return oobeSetStep(3);
    if (oobeState.step < 3) return oobeSetStep(oobeState.step + 1);
    oobeFinish();
  });
  elements.oobeSkip.addEventListener("click", async () => {
    // 新建/编辑人格模式:这颗是「取消」,直接关掉、什么都不动(#164)。
    if (oobeState.reason !== "first") {
      closeOobe();
      return;
    }
    try {
      await apiRequest("/api/account/active-persona", { method: "PUT", body: JSON.stringify({ slug: null, oobe_done: true }) });
    } catch (_) {}
    closeOobe();
    showToast("随时可以在账号页里创建自己的人格", "info");
  });
}
