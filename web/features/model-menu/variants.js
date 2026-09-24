import { apiRequest } from "../../core/api.js";
import { THINKING_VARIANT_DEFAULT_LABEL } from "../../core/constants.js";
import { updateControlState } from "../composer/input.js";
import { updateCurrentModelDisplay } from "./menu.js";
import { state } from "../../state/store.js";

export function thinkingVariantLabel(variant, short = false) {
  if (variant == null) return short ? THINKING_VARIANT_DEFAULT_LABEL : "模型默认";
  return String(variant);
}

export function normalizeThinkingVariantModels(value) {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    const providerId = String(item?.provider_id || "").trim();
    const model = String(item?.model || "").trim();
    if (!providerId || !model) return [];
    const variants = Array.from(new Set(
      (Array.isArray(item?.variants) ? item.variants : [])
        .map((variant) => String(variant).trim())
        .filter(Boolean)
    ));
    const selected = typeof item?.selected === "string" && variants.includes(item.selected)
      ? item.selected
      : null;
    return [{ provider_id: providerId, model, variants, selected }];
  });
}

export async function loadThinkingVariants() {
  const generation = ++state.thinkingVariantLoadGeneration;
  state.thinkingVariantLoading = true;
  state.thinkingVariantError = "";
  updateControlState();
  try {
    const response = await apiRequest("/api/models/thinking-variants", { cache: "no-store" });
    const payload = await response.json();
    if (generation !== state.thinkingVariantLoadGeneration) return;
    state.thinkingVariantModels = normalizeThinkingVariantModels(payload?.options);
    updateCurrentModelDisplay();
  } catch (error) {
    if (generation !== state.thinkingVariantLoadGeneration) return;
    state.thinkingVariantError = error.message || "无法载入思考档位";
  } finally {
    if (generation === state.thinkingVariantLoadGeneration) {
      state.thinkingVariantLoading = false;
      updateControlState();
    }
  }
}
