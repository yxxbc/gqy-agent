import { apiRequest } from "../../core/api.js";
import { formatInteger, modelKey, modelMark } from "../../core/format.js";
import { createIcon, makeIconSlot } from "../../core/icons.js";
import { showToast } from "../../core/toast.js";
import { UI_SCALE, visualPixelsToLayout } from "../../core/ui-scale.js";
import { updateControlState } from "../composer/input.js";
import { refreshSessionContext } from "../goal.js";
import { normalizeThinkingVariantModels, thinkingVariantLabel } from "./variants.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";
import { clearInlineError, showInlineError } from "../../widgets/inline-error.js";

export function openModelMenu() {
  if (elements.modelButton.disabled || state.models.length === 0) return;
  resetModelMenuStaging();
  renderModelMenu();
  elements.modelMenu.hidden = false;
  elements.modelButton.setAttribute("aria-expanded", "true");
  positionModelMenu();
  refreshSessionModelOverride();
  const selected = elements.modelMenu.querySelector(".model-menu-item.selected:not(:disabled)");
  const first = elements.modelMenu.querySelector(".model-menu-item:not(:disabled)");
  window.requestAnimationFrame(() => (selected || first)?.focus());
}

/// 菜单不在按钮的父元素里(`.composer` 会把它裁掉,见 index.html),所以
/// 位置得自己算：贴按钮左边、浮在按钮上方,再夹回 dock 的可视范围内。
export function positionModelMenu() {
  if (elements.modelMenu.hidden) return;
  const dock = elements.composerDock.getBoundingClientRect();
  const button = elements.modelButton.getBoundingClientRect();
  const gap = 8;
  const margin = 8;
  const width = elements.modelMenu.offsetWidth * UI_SCALE;
  const left = Math.min(
    Math.max(margin, button.left),
    Math.max(margin, window.innerWidth - width - margin)
  );
  elements.modelMenu.style.left = `${visualPixelsToLayout(left - dock.left)}px`;
  elements.modelMenu.style.bottom = `${visualPixelsToLayout(dock.bottom - button.top + gap)}px`;
  // 上方剩多少就开多高,顶不出视口。
  const room = visualPixelsToLayout(Math.max(160, button.top - gap - margin));
  elements.modelMenu.style.maxHeight = `${Math.min(420, room)}px`;
}

export function closeModelMenu({ restoreFocus = false, discard = true } = {}) {
  if (elements.modelMenu.hidden) return;
  closeLevelMenu();
  elements.modelMenu.hidden = true;
  elements.modelButton.setAttribute("aria-expanded", "false");
  if (discard) {
    state.stagedModelKeys = null;
    state.stagedFollowGlobal = false;
    state.modelMenuTouched = false;
    state.modelMenuError = "";
  }
  if (restoreFocus) elements.modelButton.focus();
}

export function activeModels() {
  return state.models.filter((model) => model?.active);
}

export function normalizeModelOverride(value) {
  if (!Array.isArray(value)) return null;
  const models = value
    .map((item) => ({ provider_id: String(item?.provider_id || ""), model: String(item?.model || "") }))
    .filter((item) => item.provider_id && item.model);
  return models.length ? models : null;
}

export function viewSessionModelOverride() {
  return state.viewSessionId && state.sessionModelOverrideFor === state.viewSessionId
    ? state.sessionModelOverride
    : null;
}

export function describeOverrideModel(entry) {
  const key = modelKey(entry);
  return state.models.find((model) => modelKey(model) === key) || entry;
}

export function setSessionModelOverride(sessionId, override) {
  state.sessionModelOverrideFor = String(sessionId || "");
  state.sessionModelOverride = normalizeModelOverride(override);
  updateCurrentModelDisplay();
  if (elements.modelMenu.hidden || state.modelSelectionSubmitting) return;
  // 菜单开着且用户尚未改动暂存选择时，同步为最新覆盖状态。
  if (!state.modelMenuTouched && state.stagedModelKeys instanceof Set) {
    const fresh = viewSessionModelOverride();
    const freshFollow = !fresh;
    const freshKeys = new Set((fresh || []).map(modelKey));
    const unchanged = state.stagedFollowGlobal === freshFollow
      && state.stagedModelKeys.size === freshKeys.size
      && [...freshKeys].every((key) => state.stagedModelKeys.has(key));
    if (!unchanged) {
      const hadFocus = elements.modelMenu.contains(document.activeElement);
      resetModelMenuStaging();
      renderModelMenu();
      if (hadFocus) {
        const focusTarget = elements.modelMenu.querySelector(".model-menu-item.selected:not(:disabled)")
          || elements.modelMenu.querySelector(".model-menu-item:not(:disabled)");
        focusTarget?.focus();
      }
      return;
    }
  }
  updateModelMenuState();
}

export async function refreshSessionModelOverride(sessionId = state.viewSessionId) {
  const target = String(sessionId || "");
  const token = ++state.sessionModelOverrideToken;
  if (!target) {
    setSessionModelOverride("", null);
    return;
  }
  try {
    const response = await apiRequest(`/api/sessions/${encodeURIComponent(target)}/models`);
    const payload = await response.json();
    if (token !== state.sessionModelOverrideToken || state.viewSessionId !== target) return;
    setSessionModelOverride(target, payload?.model_override);
  } catch (_) {
    // 静默失败：顶栏回退显示全局池，下次打开菜单会再次刷新。
  }
}

export function updateCurrentModelDisplay() {
  // 设置页摘要始终反映全局激活池。
  const active = activeModels();
  if (active.length === 0) {
    elements.settingsModelMark.textContent = "--";
    elements.settingsModelName.textContent = state.models.length ? "未选择模型" : "未配置模型";
    elements.settingsModelProvider.textContent = "--";
  } else if (active.length > 1) {
    elements.settingsModelMark.textContent = "MX";
    elements.settingsModelName.textContent = "混合模型";
    elements.settingsModelProvider.textContent = `${active.length} 个活动端点`;
  } else {
    elements.settingsModelMark.textContent = modelMark(active[0]);
    elements.settingsModelName.textContent = String(active[0].model || "");
    elements.settingsModelProvider.textContent = String(active[0].provider_name || active[0].provider_id || "");
  }

  // 顶栏反映当前会话生效的模型池：有覆盖显示覆盖，否则跟随全局。
  const override = viewSessionModelOverride();
  const pool = override ? override.map(describeOverrideModel) : active;
  const scope = override ? "本会话固定" : "跟随全局";
  if (pool.length === 0) {
    elements.modelLabel.textContent = state.models.length ? "未选择模型" : "未配置模型";
    elements.modelLabel.title = `${elements.modelLabel.textContent}（${scope}）`;
    return;
  }
  if (pool.length > 1) {
    const title = pool.map((model) => `${model.provider_name || model.provider_id || ""} · ${model.model || ""}`).join("\n");
    elements.modelLabel.textContent = `混合模型 · ${pool.length}`;
    elements.modelLabel.title = `${scope}\n${title}`;
    return;
  }
  const selected = pool[0];
  // 档位并进按钮文字——它原本有自己的按钮,合并后这里是唯一能看到它的地方。
  const level = state.thinkingVariantModels.find((model) => modelKey(model) === modelKey(selected))?.selected;
  const name = String(selected.model || "");
  elements.modelLabel.textContent = level == null ? name : `${name} · ${thinkingVariantLabel(level, true)}`;
  elements.modelLabel.title = `${selected.provider_name || selected.provider_id || ""} · ${selected.model || ""}（${scope}）`;
}

export function refreshLiveEndpointVisibility() {
  for (const live of state.liveRuns.values()) {
    if (!live.endpoint) continue;
    const values = [live.providerId, live.model].map((value) => String(value || "").trim()).filter(Boolean);
    live.endpoint.hidden = !state.display?.show_mixed_model_endpoint || values.length === 0;
  }
}

export function resetModelMenuStaging() {
  const override = viewSessionModelOverride();
  state.stagedFollowGlobal = !override;
  state.stagedModelKeys = new Set((override || []).map(modelKey));
  // 思考档位以前是另一个按钮、另一个浮层,即点即写。现在它和模型选择合成
  // 一个面板,就得跟模型选择一样先暂存,由同一个「确认」一起提交——否则同一
  // 个面板里一半改动立刻生效、一半要按确认,「取消」也说不清取消的是什么。
  state.stagedVariants = new Map(
    state.thinkingVariantModels.map((model) => [modelKey(model), model.selected ?? null])
  );
  state.expandedLevelKey = null;
  state.modelMenuTouched = false;
  state.modelMenuError = "";
}

/// 某个模型可选的档位;没有可配置档位的模型返回空数组(那一行就不长小片)。
export function variantOptionsFor(key) {
  const entry = state.thinkingVariantModels.find((model) => modelKey(model) === key);
  return entry ? entry.variants : [];
}

export function stagedVariantFor(key) {
  if (state.stagedVariants instanceof Map && state.stagedVariants.has(key)) {
    return state.stagedVariants.get(key);
  }
  const entry = state.thinkingVariantModels.find((model) => modelKey(model) === key);
  return entry ? entry.selected ?? null : null;
}

export function modelMenuStaging() {
  if (state.stagedModelKeys instanceof Set) {
    return { follow: state.stagedFollowGlobal, keys: state.stagedModelKeys };
  }
  const override = viewSessionModelOverride();
  return { follow: !override, keys: new Set((override || []).map(modelKey)) };
}

export function renderModelMenu() {
  // 重画整张列表会把滚动位置清零。展开档位、选档位都要重画,不记住就
  // 每次都弹回顶部,而用户正看着列表中间某一行。
  const scrollTop = elements.modelMenu.querySelector(".model-menu-list")?.scrollTop ?? 0;
  elements.modelMenu.replaceChildren();
  const staging = modelMenuStaging();
  const globalKeys = new Set(activeModels().map(modelKey));
  const list = document.createElement("div");
  list.className = "model-menu-list";
  list.setAttribute("role", "group");
  list.setAttribute("aria-label", "可用模型");

  const follow = document.createElement("button");
  follow.type = "button";
  follow.className = "model-menu-item model-menu-follow";
  follow.setAttribute("role", "menuitemcheckbox");
  follow.setAttribute("aria-checked", String(staging.follow));
  follow.classList.toggle("selected", staging.follow);
  const followCopy = document.createElement("span");
  followCopy.className = "model-menu-copy";
  const followName = document.createElement("strong");
  followName.textContent = "跟随全局";
  const followHint = document.createElement("small");
  followHint.textContent = "使用全局激活模型池";
  followCopy.append(followName, followHint);
  const followCheck = document.createElement("span");
  followCheck.className = "icon-slot check-slot";
  followCheck.setAttribute("aria-hidden", "true");
  if (staging.follow) followCheck.appendChild(createIcon("check"));
  follow.append(followCopy, followCheck);
  follow.addEventListener("click", chooseFollowGlobal);
  list.appendChild(follow);

  for (const model of state.models) {
    if (!model || typeof model !== "object") continue;
    const button = document.createElement("button");
    button.type = "button";
    button.className = "model-menu-item";
    button.setAttribute("role", "menuitemcheckbox");
    button.dataset.modelKey = modelKey(model);
    const checked = staging.follow ? globalKeys.has(button.dataset.modelKey) : staging.keys.has(button.dataset.modelKey);
    const selected = checked && !staging.follow;
    button.setAttribute("aria-checked", String(checked));
    button.classList.toggle("selected", selected);
    button.classList.toggle("from-global", checked && staging.follow);

    const copy = document.createElement("span");
    copy.className = "model-menu-copy";
    const name = document.createElement("strong");
    name.textContent = String(model.model || "");
    const provider = document.createElement("small");
    provider.textContent = String(model.provider_name || model.provider_id || "");
    copy.append(name, provider);
    const check = document.createElement("span");
    check.className = "icon-slot check-slot";
    check.setAttribute("aria-hidden", "true");
    if (checked) check.appendChild(createIcon("check"));
    button.append(copy, check);
    button.addEventListener("click", () => toggleStagedModel(button.dataset.modelKey));

    // 档位小片和展开的档位行都得在这个按钮外面——按钮里套按钮是非法嵌套,
    // 浏览器会把内层拎出去,点击就落到外层的「选中模型」上。
    const key = button.dataset.modelKey;
    const variants = variantOptionsFor(key);
    if (!variants.length) {
      list.appendChild(button);
      continue;
    }
    const row = document.createElement("div");
    row.className = "model-menu-row";
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "model-level-chip";
    chip.setAttribute("aria-expanded", String(state.expandedLevelKey === key));
    chip.title = `思考程度：${thinkingVariantLabel(stagedVariantFor(key))}`;
    const chipText = document.createElement("span");
    chipText.textContent = thinkingVariantLabel(stagedVariantFor(key), true);
    chip.append(chipText, makeIconSlot("chevron-down"));
    chip.addEventListener("click", (event) => {
      event.stopPropagation();
      if (state.expandedLevelKey === key) closeLevelMenu();
      else openLevelMenu(key, chip, model.model);
    });
    row.append(button, chip);
    list.appendChild(row);
  }

  const footer = document.createElement("footer");
  footer.className = "model-menu-footer";
  footer.setAttribute("role", "none");
  const feedback = document.createElement("span");
  feedback.className = "model-menu-feedback";
  feedback.setAttribute("role", "status");
  feedback.setAttribute("aria-live", "polite");
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.className = "model-cancel";
  cancel.setAttribute("role", "menuitem");
  cancel.textContent = "取消";
  cancel.addEventListener("click", () => closeModelMenu({ restoreFocus: true }));
  const confirm = document.createElement("button");
  confirm.type = "button";
  confirm.className = "model-confirm";
  confirm.setAttribute("role", "menuitem");
  confirm.textContent = "确认";
  confirm.addEventListener("click", confirmModelSelection);
  footer.append(feedback, cancel, confirm);
  elements.modelMenu.append(list, footer);
  if (scrollTop) list.scrollTop = scrollTop;
  // 展开/收起档位会改变菜单高度，位置要跟着重算。
  positionModelMenu();
  updateModelMenuState();
  updateCurrentModelDisplay();
  refreshLiveEndpointVisibility();
  updateControlState();
}

export function updateModelMenuState() {
  const staging = modelMenuStaging();
  const globalKeys = new Set(activeModels().map(modelKey));
  elements.modelMenu.querySelectorAll(".model-menu-item").forEach((button) => {
    const isFollowItem = button.classList.contains("model-menu-follow");
    const key = button.dataset.modelKey || "";
    const checked = isFollowItem
      ? staging.follow
      : (staging.follow ? globalKeys.has(key) : staging.keys.has(key));
    button.classList.toggle("selected", checked && (isFollowItem || !staging.follow));
    button.classList.toggle("from-global", !isFollowItem && checked && staging.follow);
    button.setAttribute("aria-checked", String(checked));
    button.disabled = state.blocked || state.modelSelectionSubmitting;
    const check = button.querySelector(".check-slot");
    if (check) check.replaceChildren(...(checked ? [createIcon("check")] : []));
  });
  const feedback = elements.modelMenu.querySelector(".model-menu-feedback");
  if (feedback) {
    const following = staging.follow || staging.keys.size === 0;
    feedback.textContent = state.modelMenuError
      || (following ? "跟随全局激活模型池" : `已选择 ${formatInteger(staging.keys.size)} 个模型（仅本会话）`);
    feedback.classList.toggle("is-error", Boolean(state.modelMenuError));
  }
  const confirm = elements.modelMenu.querySelector(".model-confirm");
  if (confirm) {
    confirm.textContent = state.modelSelectionSubmitting ? "正在应用" : "确认";
    confirm.disabled = state.modelSelectionSubmitting || state.blocked;
  }
  const cancel = elements.modelMenu.querySelector(".model-cancel");
  if (cancel) cancel.disabled = state.modelSelectionSubmitting;
}

export function chooseFollowGlobal() {
  if (!(state.stagedModelKeys instanceof Set) || state.modelSelectionSubmitting) return;
  state.stagedFollowGlobal = true;
  state.stagedModelKeys = new Set();
  state.modelMenuTouched = true;
  state.modelMenuError = "";
  updateModelMenuState();
}

/// 档位选项做成独立浮层,挂在 composer-dock 上。
///
/// 内联铺开会把下面的模型整体往下顶,列表本来就长,一展开就更难找；浮层
/// 又不能放进 `.model-menu`——那个为了圆角开了 overflow: hidden,列表自己
/// 还滚动,浮层会被切掉。所以和模型菜单平级,自己算位置。
export function openLevelMenu(key, chip, modelName) {
  const variants = variantOptionsFor(key);
  if (!variants.length) return;
  state.expandedLevelKey = key;
  const menu = elements.modelLevelMenu;
  menu.replaceChildren();
  menu.setAttribute("aria-label", `${modelName} 的思考程度`);
  for (const variant of [null, ...variants]) {
    const staged = stagedVariantFor(key) === variant;
    const option = document.createElement("button");
    option.type = "button";
    option.className = "model-level-option";
    option.setAttribute("role", "radio");
    option.setAttribute("aria-checked", String(staged));
    option.classList.toggle("selected", staged);
    option.textContent = thinkingVariantLabel(variant);
    option.title = variant == null ? "使用模型默认设置" : String(variant);
    option.addEventListener("click", (event) => {
      event.stopPropagation();
      stageVariant(key, variant);
    });
    menu.appendChild(option);
  }
  menu.hidden = false;
  chip.setAttribute("aria-expanded", "true");
  positionLevelMenu(chip);
}

export function positionLevelMenu(chip) {
  const menu = elements.modelLevelMenu;
  if (menu.hidden) return;
  const dock = elements.composerDock.getBoundingClientRect();
  const anchor = chip.getBoundingClientRect();
  const margin = 8;
  const width = menu.offsetWidth * UI_SCALE;
  const height = menu.offsetHeight * UI_SCALE;
  // 贴小片右缘往左展开,竖直方向和小片对齐;上下都夹回视口。
  const left = Math.min(
    Math.max(margin, anchor.right - width),
    Math.max(margin, window.innerWidth - width - margin)
  );
  const top = Math.min(
    Math.max(margin, anchor.top - 4),
    Math.max(margin, window.innerHeight - height - margin)
  );
  menu.style.left = `${visualPixelsToLayout(left - dock.left)}px`;
  menu.style.top = `${visualPixelsToLayout(top - dock.top)}px`;
}

export function closeLevelMenu() {
  if (elements.modelLevelMenu.hidden) return;
  elements.modelLevelMenu.hidden = true;
  state.expandedLevelKey = null;
  elements.modelMenu
    .querySelectorAll('.model-level-chip[aria-expanded="true"]')
    .forEach((chip) => chip.setAttribute("aria-expanded", "false"));
}

export function stageVariant(key, variant) {
  if (!(state.stagedVariants instanceof Map) || state.modelSelectionSubmitting) return;
  state.stagedVariants.set(key, variant);
  closeLevelMenu();
  state.modelMenuTouched = true;
  state.modelMenuError = "";
  renderModelMenu();
}

export function toggleStagedModel(key) {
  if (!(state.stagedModelKeys instanceof Set) || state.modelSelectionSubmitting) return;
  if (state.stagedFollowGlobal) {
    // 退出跟随模式：以当前显示的全局激活池为起点继续多选。
    state.stagedFollowGlobal = false;
    state.stagedModelKeys = new Set(activeModels().map(modelKey));
  }
  if (state.stagedModelKeys.has(key)) state.stagedModelKeys.delete(key);
  else state.stagedModelKeys.add(key);
  state.modelMenuTouched = true;
  state.modelMenuError = "";
  updateModelMenuState();
}

/// 把面板里改过的思考档位一次写回。档位是**全局按模型**存的偏好,和会话
/// 的模型选择不是一个作用域,所以是两次请求;这里先写档位——它失败了就整个
/// 确认中止,不会出现「模型换了但档位没跟上」的半套状态。
export async function commitStagedVariants() {
  if (!(state.stagedVariants instanceof Map)) return;
  const updates = [];
  for (const model of state.thinkingVariantModels) {
    const key = modelKey(model);
    if (!state.stagedVariants.has(key)) continue;
    const desired = state.stagedVariants.get(key);
    if (desired === (model.selected ?? null)) continue;
    updates.push({ provider_id: model.provider_id, model: model.model, selected: desired });
  }
  if (!updates.length) return;
  const response = await apiRequest("/api/models/thinking-variants", {
    method: "PUT",
    body: JSON.stringify({ updates })
  });
  const payload = await response.json();
  state.thinkingVariantModels = normalizeThinkingVariantModels(payload?.options);
}

export async function confirmModelSelection() {
  if (!(state.stagedModelKeys instanceof Set) || state.modelSelectionSubmitting) return;
  const sessionId = String(state.viewSessionId || state.currentSessionId || "");
  if (!sessionId) {
    state.modelMenuError = "当前视图没有可设置的会话";
    updateModelMenuState();
    return;
  }
  const follow = state.stagedFollowGlobal || state.stagedModelKeys.size === 0;
  const selected = follow ? [] : state.models.filter((model) => state.stagedModelKeys.has(modelKey(model)));
  if (!follow && selected.length === 0) {
    state.modelMenuError = "所选模型已不可用，请重新选择";
    updateModelMenuState();
    return;
  }
  state.modelSelectionSubmitting = true;
  state.modelMenuError = "";
  clearInlineError();
  updateModelMenuState();
  let applied = false;
  try {
    await commitStagedVariants();
    const response = await apiRequest(`/api/sessions/${encodeURIComponent(sessionId)}/models`, {
      method: "PUT",
      body: JSON.stringify({
        models: selected.map((model) => ({
          provider_id: String(model.provider_id || ""),
          model: String(model.model || "")
        }))
      })
    });
    const payload = await response.json();
    applied = true;
    state.modelSelectionSubmitting = false;
    closeModelMenu();
    setSessionModelOverride(sessionId, payload?.model_override);
    // 换了模型池,窗口大小也跟着换;不拉的话上下文条要到跑完一轮才纠正。
    refreshSessionContext(sessionId);
    showToast(follow ? "本会话已恢复跟随全局" : "本会话模型已更新（下一轮生效）");
  } catch (error) {
    state.modelMenuError = error.message || "模型设置未保存";
    showInlineError(error.message);
    showToast(error.message, "error");
  } finally {
    state.modelSelectionSubmitting = false;
    updateControlState();
    if (applied) window.requestAnimationFrame(() => elements.modelButton.focus());
    else {
      updateModelMenuState();
      window.requestAnimationFrame(() => elements.modelMenu.querySelector(".model-confirm")?.focus());
    }
  }
}
