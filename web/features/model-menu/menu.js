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

/// 只有本模块用的状态（从 state/store.js 分出来的私有分片）。
const menuState = {
  modelSelectionSubmitting: false,
  stagedModelKeys: null,
  stagedFollowGlobal: false,
  stagedVariants: null,
  expandedLevelKey: null,
  groupExpanded: null,
  modelQuery: "",
  modelMenuTouched: false,
  modelMenuError: "",
  sessionModelOverrideToken: 0
};

/// 过滤框节点(只建一次,见 ensureModelMenuSearch)。
let modelMenuSearch = null;

export function openModelMenu() {
  if (elements.modelButton.disabled || state.models.length === 0) return;
  resetModelMenuStaging();
  // 每次打开都从干净状态开始:过滤框清空,分节展开状态按当前选中重算。
  menuState.modelQuery = "";
  menuState.groupExpanded = null;
  if (modelMenuSearch) modelMenuSearch.input.value = "";
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
    menuState.stagedModelKeys = null;
    menuState.stagedFollowGlobal = false;
    menuState.modelMenuTouched = false;
    menuState.modelMenuError = "";
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
  if (elements.modelMenu.hidden || menuState.modelSelectionSubmitting) return;
  // 菜单开着且用户尚未改动暂存选择时，同步为最新覆盖状态。
  if (!menuState.modelMenuTouched && menuState.stagedModelKeys instanceof Set) {
    const fresh = viewSessionModelOverride();
    const freshFollow = !fresh;
    const freshKeys = new Set((fresh || []).map(modelKey));
    const unchanged = menuState.stagedFollowGlobal === freshFollow
      && menuState.stagedModelKeys.size === freshKeys.size
      && [...freshKeys].every((key) => menuState.stagedModelKeys.has(key));
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
  const token = ++menuState.sessionModelOverrideToken;
  if (!target) {
    setSessionModelOverride("", null);
    return;
  }
  try {
    const response = await apiRequest(`/api/sessions/${encodeURIComponent(target)}/models`);
    const payload = await response.json();
    if (token !== menuState.sessionModelOverrideToken || state.viewSessionId !== target) return;
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
  menuState.stagedFollowGlobal = !override;
  menuState.stagedModelKeys = new Set((override || []).map(modelKey));
  // 思考档位以前是另一个按钮、另一个浮层,即点即写。现在它和模型选择合成
  // 一个面板,就得跟模型选择一样先暂存,由同一个「确认」一起提交——否则同一
  // 个面板里一半改动立刻生效、一半要按确认,「取消」也说不清取消的是什么。
  menuState.stagedVariants = new Map(
    state.thinkingVariantModels.map((model) => [modelKey(model), model.selected ?? null])
  );
  menuState.expandedLevelKey = null;
  menuState.modelMenuTouched = false;
  menuState.modelMenuError = "";
}

/// 某个模型可选的档位;没有可配置档位的模型返回空数组(那一行就不长小片)。
export function variantOptionsFor(key) {
  const entry = state.thinkingVariantModels.find((model) => modelKey(model) === key);
  return entry ? entry.variants : [];
}

export function stagedVariantFor(key) {
  if (menuState.stagedVariants instanceof Map && menuState.stagedVariants.has(key)) {
    return menuState.stagedVariants.get(key);
  }
  const entry = state.thinkingVariantModels.find((model) => modelKey(model) === key);
  return entry ? entry.selected ?? null : null;
}

export function modelMenuStaging() {
  if (menuState.stagedModelKeys instanceof Set) {
    return { follow: menuState.stagedFollowGlobal, keys: menuState.stagedModelKeys };
  }
  const override = viewSessionModelOverride();
  return { follow: !override, keys: new Set((override || []).map(modelKey)) };
}

/// 按供应商分组,保持模型表原有的顺序(供应商按首次出现,组内按目录顺序)。
/// `query` 非空时只留命中的模型(模型名 / 供应商名 / 供应商 id 里含它就命中)。
function groupModelsByProvider(models, query = "") {
  const groups = new Map();
  for (const model of models) {
    if (!model || typeof model !== "object") continue;
    const id = String(model.provider_id || "");
    const name = String(model.provider_name || id || "未命名供应商");
    if (query) {
      const haystack = `${String(model.model || "")} ${name} ${id}`.toLowerCase();
      if (!haystack.includes(query)) continue;
    }
    if (!groups.has(id)) {
      groups.set(id, { id, name, models: [] });
    }
    groups.get(id).models.push(model);
  }
  return [...groups.values()];
}

/// 某个供应商节是否展开。默认展开「有选中/已激活模型」的节与唯一的那个节
/// (30 多家、上千条模型全铺开等于没有列表);用户点过的状态记在
/// menuState.groupExpanded 里,重画/重开不丢。
function modelGroupExpanded(id, group, soleGroup = false) {
  if (menuState.groupExpanded instanceof Map && menuState.groupExpanded.has(id)) {
    return menuState.groupExpanded.get(id);
  }
  if (soleGroup) return true;
  const staging = modelMenuStaging();
  const keys = staging.follow ? new Set(activeModels().map(modelKey)) : staging.keys;
  return group.models.some((model) => keys.has(modelKey(model)));
}

function toggleModelGroup(id, group) {
  if (!(menuState.groupExpanded instanceof Map)) menuState.groupExpanded = new Map();
  menuState.groupExpanded.set(id, !modelGroupExpanded(id, group));
  renderModelMenu();
  // 节头是重画出来的,焦点跟着回来——不然键盘用户点一下节就掉到 body 上。
  window.requestAnimationFrame(() => {
    const header = [...elements.modelMenu.querySelectorAll(".model-menu-group")]
      .find((node) => node.dataset.provider === id);
    header?.focus();
  });
}

/// 一条模型(可带思考档位小片)。所有节点挂在 `parent` 上——分组后 parent 是
/// 节内容容器,不再是整个列表。
function appendModelEntry(parent, model, staging, globalKeys) {
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
  copy.append(name);
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
    parent.appendChild(button);
    return;
  }
  const row = document.createElement("div");
  row.className = "model-menu-row";
  const chip = document.createElement("button");
  chip.type = "button";
  chip.className = "model-level-chip";
  chip.setAttribute("aria-expanded", String(menuState.expandedLevelKey === key));
  chip.title = `思考程度：${thinkingVariantLabel(stagedVariantFor(key))}`;
  const chipText = document.createElement("span");
  chipText.textContent = thinkingVariantLabel(stagedVariantFor(key), true);
  chip.append(chipText, makeIconSlot("chevron-down"));
  chip.addEventListener("click", (event) => {
    event.stopPropagation();
    if (menuState.expandedLevelKey === key) closeLevelMenu();
    else openLevelMenu(key, chip, model.model);
  });
  row.append(button, chip);
  parent.appendChild(row);
}

/// 列表本体(含「跟随全局」与各供应商节)。过滤框输入时只重画它——输入框与
/// 页脚留在原地,焦点不丢。
function buildModelList(staging, globalKeys) {
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

  // 按供应商分组:节头是品牌图标 + 供应商名 + 条数,点开才铺模型(09-26 用户
  // 要求)。默认只展开有选中模型的节——30 多家、上千条模型全铺开等于没有列表;
  // 过滤时全部展开(用户就是在找命中项)。
  const groups = groupModelsByProvider(state.models, menuState.modelQuery);
  for (const group of groups) {
    const expanded = menuState.modelQuery
      ? true
      : modelGroupExpanded(group.id, group, groups.length === 1);
    const section = document.createElement("section");
    section.className = "model-menu-section";
    // 节 = 一个 aria group;节头是组里的 menuitem,aria-expanded 表示展开。
    section.setAttribute("role", "group");
    section.setAttribute("aria-label", group.name);
    const header = document.createElement("button");
    header.type = "button";
    header.className = "model-menu-group";
    header.setAttribute("role", "menuitem");
    header.dataset.provider = group.id;
    header.setAttribute("aria-expanded", String(expanded));
    // 品牌图标来自 provider-icons.js;认不出的供应商只显示名字(名字就在旁边)。
    const brand = window.GqyProviderIcons?.providerMark({ id: group.id, display_name: group.name }, "is-small");
    if (brand) header.appendChild(brand);
    const groupName = document.createElement("strong");
    groupName.textContent = group.name;
    const groupCount = document.createElement("small");
    groupCount.textContent = String(group.models.length);
    header.append(groupName, groupCount, makeIconSlot("chevron-down"));
    header.addEventListener("click", () => toggleModelGroup(group.id, group));
    section.appendChild(header);
    if (expanded) {
      const body = document.createElement("div");
      body.className = "model-menu-group-models";
      for (const model of group.models) appendModelEntry(body, model, staging, globalKeys);
      section.appendChild(body);
    }
    list.appendChild(section);
  }

  if (!groups.length) {
    const empty = document.createElement("p");
    empty.className = "model-menu-empty";
    empty.textContent = menuState.modelQuery ? "没有匹配的模型" : "还没有可用模型";
    list.appendChild(empty);
  }
  return list;
}

/// 页脚(反馈 + 取消/确认)。
function buildModelFooter() {
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
  return footer;
}

/// 过滤框(菜单顶部,固定不滚动)。节点只建一次:输入时只换列表,输入框与
/// 页脚留在原地,焦点和光标不丢。
function ensureModelMenuSearch() {
  if (!modelMenuSearch) {
    const wrap = document.createElement("div");
    wrap.className = "model-menu-search";
    const input = document.createElement("input");
    input.type = "search";
    input.placeholder = "过滤模型或供应商";
    input.setAttribute("aria-label", "过滤模型");
    input.autocomplete = "off";
    input.spellcheck = false;
    input.addEventListener("input", () => {
      menuState.modelQuery = input.value.trim().toLowerCase();
      refreshModelMenuList();
    });
    wrap.appendChild(input);
    modelMenuSearch = { wrap, input };
  }
  return modelMenuSearch;
}

export function renderModelMenu() {
  // 重画整张列表会把滚动位置清零。展开档位、选档位都要重画,不记住就
  // 每次都弹回顶部,而用户正看着列表中间某一行。
  const scrollTop = elements.modelMenu.querySelector(".model-menu-list")?.scrollTop ?? 0;
  elements.modelMenu.replaceChildren();
  const staging = modelMenuStaging();
  const globalKeys = new Set(activeModels().map(modelKey));
  const search = ensureModelMenuSearch();
  const list = buildModelList(staging, globalKeys);
  const footer = buildModelFooter();
  elements.modelMenu.append(search.wrap, list, footer);
  if (scrollTop) list.scrollTop = scrollTop;
  // 展开/收起档位会改变菜单高度，位置要跟着重算。
  positionModelMenu();
  updateModelMenuState();
  updateCurrentModelDisplay();
  refreshLiveEndpointVisibility();
  updateControlState();
}

/// 过滤框输入:只换掉列表本体,输入框/页脚原地不动。
function refreshModelMenuList() {
  const list = elements.modelMenu.querySelector(".model-menu-list");
  if (!list) return;
  const staging = modelMenuStaging();
  const globalKeys = new Set(activeModels().map(modelKey));
  list.replaceWith(buildModelList(staging, globalKeys));
  updateModelMenuState();
  positionModelMenu();
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
    button.disabled = state.blocked || menuState.modelSelectionSubmitting;
    const check = button.querySelector(".check-slot");
    if (check) check.replaceChildren(...(checked ? [createIcon("check")] : []));
  });
  const feedback = elements.modelMenu.querySelector(".model-menu-feedback");
  if (feedback) {
    const following = staging.follow || staging.keys.size === 0;
    feedback.textContent = menuState.modelMenuError
      || (following ? "跟随全局激活模型池" : `已选择 ${formatInteger(staging.keys.size)} 个模型（仅本会话）`);
    feedback.classList.toggle("is-error", Boolean(menuState.modelMenuError));
  }
  const confirm = elements.modelMenu.querySelector(".model-confirm");
  if (confirm) {
    confirm.textContent = menuState.modelSelectionSubmitting ? "正在应用" : "确认";
    confirm.disabled = menuState.modelSelectionSubmitting || state.blocked;
  }
  const cancel = elements.modelMenu.querySelector(".model-cancel");
  if (cancel) cancel.disabled = menuState.modelSelectionSubmitting;
}

export function chooseFollowGlobal() {
  if (!(menuState.stagedModelKeys instanceof Set) || menuState.modelSelectionSubmitting) return;
  menuState.stagedFollowGlobal = true;
  menuState.stagedModelKeys = new Set();
  menuState.modelMenuTouched = true;
  menuState.modelMenuError = "";
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
  menuState.expandedLevelKey = key;
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
  menuState.expandedLevelKey = null;
  elements.modelMenu
    .querySelectorAll('.model-level-chip[aria-expanded="true"]')
    .forEach((chip) => chip.setAttribute("aria-expanded", "false"));
}

export function stageVariant(key, variant) {
  if (!(menuState.stagedVariants instanceof Map) || menuState.modelSelectionSubmitting) return;
  menuState.stagedVariants.set(key, variant);
  closeLevelMenu();
  menuState.modelMenuTouched = true;
  menuState.modelMenuError = "";
  renderModelMenu();
}

export function toggleStagedModel(key) {
  if (!(menuState.stagedModelKeys instanceof Set) || menuState.modelSelectionSubmitting) return;
  if (menuState.stagedFollowGlobal) {
    // 退出跟随模式：以当前显示的全局激活池为起点继续多选。
    menuState.stagedFollowGlobal = false;
    menuState.stagedModelKeys = new Set(activeModels().map(modelKey));
  }
  if (menuState.stagedModelKeys.has(key)) menuState.stagedModelKeys.delete(key);
  else menuState.stagedModelKeys.add(key);
  menuState.modelMenuTouched = true;
  menuState.modelMenuError = "";
  updateModelMenuState();
}

/// 把面板里改过的思考档位一次写回。档位是**全局按模型**存的偏好,和会话
/// 的模型选择不是一个作用域,所以是两次请求;这里先写档位——它失败了就整个
/// 确认中止,不会出现「模型换了但档位没跟上」的半套状态。
export async function commitStagedVariants() {
  if (!(menuState.stagedVariants instanceof Map)) return;
  const updates = [];
  for (const model of state.thinkingVariantModels) {
    const key = modelKey(model);
    if (!menuState.stagedVariants.has(key)) continue;
    const desired = menuState.stagedVariants.get(key);
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
  if (!(menuState.stagedModelKeys instanceof Set) || menuState.modelSelectionSubmitting) return;
  const sessionId = String(state.viewSessionId || state.currentSessionId || "");
  if (!sessionId) {
    menuState.modelMenuError = "当前视图没有可设置的会话";
    updateModelMenuState();
    return;
  }
  const follow = menuState.stagedFollowGlobal || menuState.stagedModelKeys.size === 0;
  const selected = follow ? [] : state.models.filter((model) => menuState.stagedModelKeys.has(modelKey(model)));
  if (!follow && selected.length === 0) {
    menuState.modelMenuError = "所选模型已不可用，请重新选择";
    updateModelMenuState();
    return;
  }
  menuState.modelSelectionSubmitting = true;
  menuState.modelMenuError = "";
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
    menuState.modelSelectionSubmitting = false;
    closeModelMenu();
    setSessionModelOverride(sessionId, payload?.model_override);
    // 换了模型池,窗口大小也跟着换;不拉的话上下文条要到跑完一轮才纠正。
    refreshSessionContext(sessionId);
    showToast(follow ? "本会话已恢复跟随全局" : "本会话模型已更新（下一轮生效）");
  } catch (error) {
    menuState.modelMenuError = error.message || "模型设置未保存";
    showInlineError(error.message);
    showToast(error.message, "error");
  } finally {
    menuState.modelSelectionSubmitting = false;
    updateControlState();
    if (applied) window.requestAnimationFrame(() => elements.modelButton.focus());
    else {
      updateModelMenuState();
      window.requestAnimationFrame(() => elements.modelMenu.querySelector(".model-confirm")?.focus());
    }
  }
}
