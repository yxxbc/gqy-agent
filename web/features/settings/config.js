import { apiRequest } from "../../core/api.js";
import { showToast } from "../../core/toast.js";
import { deepClone } from "../../core/util.js";
import { loadBootstrap } from "../boot.js";
import { updateControlState } from "../composer/input.js";
import { consoleHashFor, consoleIsOpen, writeConsoleHash } from "../console/panel.js";
import { conversationRunning } from "../conversation/chrome.js";
import { refreshSessionContext } from "../goal.js";
import { renderModelMenu } from "../model-menu/menu.js";
import { applyPersona } from "../persona.js";
import { updateContext } from "../status.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

/// 只有本模块用的状态（从 state/store.js 分出来的私有分片）。
const configState = {
  configSaving: false,
  configDirty: false,
  configOriginal: null,
  promptOriginal: null
};

export function setSettingsView(view) {
  const selected = ["interface", "prompts", "providers", "models", "general", "mcp", "plugins", "advanced"].includes(view) ? view : "interface";
  state.settingsView = selected;
  elements.settingsNav.querySelectorAll("[data-settings-view]").forEach((button) => {
    const active = button.dataset.settingsView === selected;
    button.classList.toggle("active", active);
    button.setAttribute("aria-current", active ? "page" : "false");
  });
  elements.settingsPanels.forEach((panel) => {
    panel.hidden = panel.dataset.settingsPanel !== selected;
  });
  window.GqySettings?.onShow(selected);
  if (consoleIsOpen() && state.consolePanel === "settings") writeConsoleHash(consoleHashFor("settings", selected));
}

export function configValue(path, fallback = undefined) {
  let value = state.configDraft;
  for (const key of path.split(".")) {
    if (value == null || typeof value !== "object" || !(key in value)) return fallback;
    value = value[key];
  }
  return value;
}

export function setConfigValue(path, value) {
  if (!state.configDraft) return;
  const keys = path.split(".");
  let target = state.configDraft;
  for (const key of keys.slice(0, -1)) {
    if (!target[key] || typeof target[key] !== "object") target[key] = {};
    target = target[key];
  }
  target[keys[keys.length - 1]] = value;
  markConfigDirty();
}

export function markConfigDirty() {
  configState.configDirty = true;
  updateSettingsControls();
}

export function clearProviderSecretChanges() {
  for (const key of Object.keys(state.secretChanges)) {
    if (key.startsWith("providers.")) delete state.secretChanges[key];
  }
}

export function refreshProviderSecretStates() {
  for (const key of Object.keys(state.secretStates)) {
    if (key.startsWith("providers.")) delete state.secretStates[key];
  }
  state.providerSecretStates.forEach((configured, index) => {
    state.secretStates[`providers.${index}.api_key`] = Boolean(configured);
  });
}

export function updateSettingsControls() {
  const busy = state.configLoading || configState.configSaving;
  elements.reloadConfigButton.disabled = busy;
  elements.saveConfigButton.disabled = busy || !state.configLoaded || !configState.configDirty || state.invalidConfigFields.size > 0 || conversationRunning();
  elements.settingsFooter?.classList.toggle("is-dirty", Boolean(state.configLoaded && configState.configDirty));
  elements.settingsFooter?.classList.toggle("is-invalid", state.invalidConfigFields.size > 0);
  if (state.configLoading) elements.settingsStatus.textContent = "正在载入配置";
  else if (configState.configSaving) elements.settingsStatus.textContent = "正在验证并保存";
  else if (!state.configLoaded) elements.settingsStatus.textContent = "尚未载入配置";
  else if (state.invalidConfigFields.size) elements.settingsStatus.textContent = "请修正表单中的错误";
  else if (conversationRunning() && configState.configDirty) elements.settingsStatus.textContent = "回复完成后才能保存";
  else elements.settingsStatus.textContent = configState.configDirty ? "有未保存的修改" : "配置已同步";
}

export function updateAdvancedConfigEditor() {
  if (!state.configDraft || document.activeElement === elements.advancedConfigEditor) return;
  elements.advancedConfigEditor.value = JSON.stringify(state.configDraft, null, 2);
}

// 设置区的渲染在 settings.js:这里只清校验状态、同步「高级」JSON 与底栏。
export function renderConfigEditors() {
  if (!state.configLoaded || !state.configDraft) return;
  state.invalidConfigFields.clear();
  window.GqySettings?.render();
  updateAdvancedConfigEditor();
  updateSettingsControls();
}

export function mapServerSecretStates(payload) {
  const providers = state.configDraft?.providers || [];
  state.providerSecretStates = providers.map((_, index) => Boolean(payload[`providers.${index}.api_key`]));
  const states = { ...payload };
  state.secretStates = states;
  refreshProviderSecretStates();
  return states;
}

// 配置文件会省略未修改的平台默认值；草稿仍需补齐真实语义，
// 以免 WebUI 保存其他设置时覆盖通讯平台的默认策略。
export function ensurePlatformDefaults(draft) {
  if (!draft || typeof draft !== "object") return;
  draft.platforms = Object.assign({
    command_prefix: "/",
    commands: {}
  }, draft.platforms);
  const qq = Object.assign({
    enabled: false,
    reverse_ws_port: 8300,
    access_token: "",
    admin_users: [],
    allow_non_admin_host_tools: false,
    user_identification: true,
    show_group_name: true,
    conversations: [],
    plugins: {},
    asset_base_url: "",
    max_reply_chars: 3000,
  }, draft.platforms.qq);
  qq.private_chats = Object.assign({
    whitelist: [],
    allow_non_whitelist: true,
    non_whitelist_rate_limit: { max_messages: 2, window_seconds: 600 }
  }, qq.private_chats);
  qq.group_chats = Object.assign({
    whitelist: [],
    trigger_keywords: [],
    whitelist_rate_limit: { max_messages: 30, window_seconds: 60 },
    allow_non_whitelist: true,
    non_whitelist_rate_limit: { max_messages: 2, window_seconds: 600 }
  }, qq.group_chats);
  draft.platforms.qq = qq;
}

export function applyConfigPayload(payload) {
  state.configDraft = deepClone(payload?.config || {});
  ensurePlatformDefaults(state.configDraft);
  configState.configOriginal = deepClone(payload?.config || {});
  state.promptDraft = deepClone(payload?.prompts || { personas: [], identities: [] });
  configState.promptOriginal = deepClone(payload?.prompts || { personas: [], identities: [] });
  state.secretChanges = {};
  mapServerSecretStates(payload?.secret_states || {});
  configState.configDirty = false;
  state.configLoaded = true;
  state.invalidConfigFields.clear();
  if (Array.isArray(payload?.models)) state.models = payload.models;
  state.configMultimodalModels = Array.isArray(payload?.multimodal_models) ? payload.multimodal_models : [];
  const providersById = new Map(
    (Array.isArray(state.configDraft?.providers) ? state.configDraft.providers : [])
      .map((provider) => [String(provider?.id || ""), provider])
  );
  state.configInferredImageModels = state.configMultimodalModels.filter((model) => {
    const provider = providersById.get(String(model?.provider_id || ""));
    const declared = provider?.model_modalities;
    return !(declared && typeof declared === "object"
      && Object.prototype.hasOwnProperty.call(declared, String(model?.model || "")));
  });
  if (payload?.display && typeof payload.display === "object") state.display = payload.display;
  if (payload?.context && typeof payload.context === "object") state.context = payload.context;
  if (payload?.persona) applyPersona(payload.persona);
  renderConfigEditors();
  renderModelMenu();
  updateContext();
  // /api/config 给的是全局池的窗口;看着的会话钉了模型时以会话接口为准,
  // 否则打开设置页一次,上下文条就被改回全局默认模型的窗口。
  if (state.viewSessionId) refreshSessionContext(state.viewSessionId);
}

export async function loadConfigDraft() {
  if (state.configLoading || configState.configSaving) return;
  if (configState.configDirty && !window.confirm("放弃尚未保存的配置修改并重新载入？")) return;
  state.configLoading = true;
  updateSettingsControls();
  try {
    const response = await apiRequest("/api/config");
    applyConfigPayload(await response.json());
  } catch (error) {
    showToast(error.message || "配置载入失败", "error");
    elements.settingsStatus.textContent = error.message || "配置载入失败";
  } finally {
    state.configLoading = false;
    updateSettingsControls();
  }
}

export function promptStateChanged() {
  if (!configState.configOriginal || !configState.promptOriginal) return false;
  const promptKeys = ["prompt", "system_prompt_file", "system_prompt"];
  const current = Object.fromEntries(promptKeys.map((key) => [key, state.configDraft?.[key]]));
  const original = Object.fromEntries(promptKeys.map((key) => [key, configState.configOriginal?.[key]]));
  const withoutPersonaMetadata = (documents) => Object.fromEntries(
    Object.entries(documents || {}).map(([kind, items]) => [
      kind,
      (Array.isArray(items) ? items : []).map(({
        avatar_path: _avatarPath,
        board_image_path: _BoardImagePath,
        board_title: _BoardTitle,
        board_subtitle: _BoardSubtitle,
        composer_placeholder: _ComposerPlaceholder,
        starter_prompts: _StarterPrompts,
        ...document
      }) => document)
    ])
  );
  return JSON.stringify(current) !== JSON.stringify(original)
    || JSON.stringify(withoutPersonaMetadata(state.promptDraft)) !== JSON.stringify(withoutPersonaMetadata(configState.promptOriginal));
}

export function buildSecretMutations() {
  return { ...state.secretChanges };
}

export async function saveConfigDraft() {
  if (!state.configLoaded || configState.configSaving || state.configLoading || conversationRunning() || state.invalidConfigFields.size) return;
  const personaChanged = String(state.configDraft?.prompt?.active_persona || "")
    !== String(configState.configOriginal?.prompt?.active_persona || "");
  configState.configSaving = true;
  state.adminBusy = true;
  updateSettingsControls();
  updateControlState();
  try {
    const response = await apiRequest("/api/config", {
      method: "PUT",
      body: JSON.stringify({
        config: state.configDraft,
        secrets: buildSecretMutations(),
        prompts: state.promptDraft,
        reset_conversation: false
      })
    });
    applyConfigPayload(await response.json());
    if (personaChanged) await loadBootstrap();
    showToast("配置已保存");
  } catch (error) {
    showToast(error.message || "配置保存失败", "error");
    elements.settingsStatus.textContent = error.message || "配置保存失败";
  } finally {
    configState.configSaving = false;
    state.adminBusy = false;
    updateSettingsControls();
    updateControlState();
  }
}

export function applyAdvancedConfig() {
  try {
    const parsed = JSON.parse(elements.advancedConfigEditor.value);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error("配置必须是 JSON 对象");
    const oldSecretStates = new Map((state.configDraft?.providers || []).map((provider, index) => [String(provider?.id || ""), Boolean(state.providerSecretStates[index])]));
    window.GqySettings?.remapApiQuotaSecrets(state.configDraft, parsed);
    state.configDraft = parsed;
    ensurePlatformDefaults(state.configDraft);
    state.providerSecretStates = (Array.isArray(parsed.providers) ? parsed.providers : []).map((provider) => oldSecretStates.get(String(provider?.id || "")) || false);
    refreshProviderSecretStates();
    clearProviderSecretChanges();
    markConfigDirty();
    renderConfigEditors();
    showToast("完整配置已应用到草稿");
  } catch (error) {
    showToast(error.message || "JSON 无效", "error");
  }
}
