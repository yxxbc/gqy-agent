import { apiRequest } from "../core/api.js";
import { showToast } from "../core/toast.js";
import { escapeText } from "../core/util.js";
import { loadBootstrap } from "./boot.js";
import { openOobe } from "./oobe.js";
import { elements } from "../state/elements.js";

export async function loadPersonaCard() {
  if (!elements.personaList) return;
  try {
    const data = await apiRequest("/api/account/personas").then((response) => response.json());
    renderPersonaList(data);
  } catch (error) {
    elements.personaList.innerHTML = `<p class="u-hint">载入失败:${escapeText(error.message || error)}</p>`;
  }
}

/// 账号页人格卡的一行摘要:插件数,脚本/技能勾了明细才报数(null = 全开)。
/// 记忆对新建的人格常开,只有旧人格关着时才提一句。
export function personaSummary(persona) {
  const parts = [`${(persona.plugins || []).length} 个插件`];
  if (Array.isArray(persona.scripts)) parts.push(`${persona.scripts.length} 个脚本`);
  if (Array.isArray(persona.skills)) parts.push(`${persona.skills.length} 个技能`);
  if (persona.memory === false) parts.unshift("记忆关");
  return parts.join(" · ");
}

export function renderPersonaList(data) {
  const list = elements.personaList;
  list.replaceChildren();
  elements.personaCreate.hidden = data.member_personas === false;
  const rows = [{ slug: null, name: "GQY", description: "管理员发布的共享人格", shared: true }, ...(data.personas || [])];
  for (const persona of rows) {
    const row = document.createElement("div");
    row.className = "persona-row";
    const active = (data.active || null) === (persona.slug || null);
    row.classList.toggle("is-active", active);
    if (persona.avatar_url || persona.shared) {
      const image = document.createElement("img");
      image.src = persona.shared ? "/assets/gqy-logo.png" : `${persona.avatar_url}&v=${Date.now()}`;
      image.alt = "";
      row.appendChild(image);
    } else {
      const initial = document.createElement("div");
      initial.className = "persona-initial";
      initial.textContent = String(persona.name || "?").slice(0, 1);
      row.appendChild(initial);
    }
    const text = document.createElement("div");
    const title = document.createElement("b");
    title.textContent = persona.name + (active ? "(当前)" : "");
    const sub = document.createElement("small");
    sub.textContent = persona.description || (persona.shared ? "" : personaSummary(persona));
    text.append(title, sub);
    row.appendChild(text);
    const actions = document.createElement("div");
    actions.className = "persona-row-actions";
    if (!active) {
      const use = document.createElement("button");
      use.type = "button";
      use.className = "secondary-button acct-row-action";
      use.textContent = "使用";
      use.addEventListener("click", async () => {
        use.disabled = true;
        try {
          await apiRequest("/api/account/active-persona", { method: "PUT", body: JSON.stringify({ slug: persona.slug, oobe_done: true }) });
          await loadBootstrap();
          loadPersonaCard();
          showToast(`新会话将使用 ${persona.name}`, "success");
        } catch (error) {
          showToast(error.message || "切换失败", "error");
          use.disabled = false;
        }
      });
      actions.appendChild(use);
    }
    if (!persona.shared) {
      const edit = document.createElement("button");
      edit.type = "button";
      edit.className = "secondary-button acct-row-action";
      edit.textContent = "编辑";
      edit.addEventListener("click", async () => {
        try {
          const detail = await apiRequest("/api/account/personas").then((response) => response.json());
          const full = (detail.personas || []).find((item) => item.slug === persona.slug) || persona;
          // 提示词不在列表里:按 slug 再取一次文件内容
          const promptResponse = await apiRequest(`/api/account/personas/${encodeURIComponent(persona.slug)}/prompt`);
          full.prompt = (await promptResponse.json()).prompt || "";
          openOobe({ reason: "edit", persona: full });
        } catch (error) {
          showToast(error.message || "载入失败", "error");
        }
      });
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "secondary-button acct-row-action";
      remove.textContent = "删除";
      remove.addEventListener("click", async () => {
        if (!window.confirm(`删除人格「${persona.name}」?记忆一起删,会话保留。`)) return;
        try {
          await apiRequest(`/api/account/personas/${encodeURIComponent(persona.slug)}`, { method: "DELETE" });
          await loadBootstrap();
          loadPersonaCard();
        } catch (error) {
          showToast(error.message || "删除失败", "error");
        }
      });
      actions.append(edit, remove);
    }
    row.appendChild(actions);
    list.appendChild(row);
  }
}
