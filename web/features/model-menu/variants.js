import { apiRequest } from "../../core/api.js";
import { THINKING_VARIANT_DEFAULT_LABEL } from "../../core/constants.js";
import { updateControlState } from "../composer/input.js";
import { updateCurrentModelDisplay } from "./menu.js";
import { state } from "../../state/store.js";

/// 只有本模块用的状态（从 state/store.js 分出来的私有分片）。
const variantsState = {
  thinkingVariantLoading: false,
  thinkingVariantLoadGeneration: 0,
  thinkingVariantError: ""
};

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
  const generation = ++variantsState.thinkingVariantLoadGeneration;
  variantsState.thinkingVariantLoading = true;
  variantsState.thinkingVariantError = "";
  updateControlState();
  try {
    const response = await apiRequest("/api/models/thinking-variants", { cache: "no-store" });
    const payload = await response.json();
    if (generation !== variantsState.thinkingVariantLoadGeneration) return;
    state.thinkingVariantModels = normalizeThinkingVariantModels(payload?.options);
    updateCurrentModelDisplay();
  } catch (error) {
    if (generation !== variantsState.thinkingVariantLoadGeneration) return;
    variantsState.thinkingVariantError = error.message || "无法载入思考档位";
  } finally {
    if (generation === variantsState.thinkingVariantLoadGeneration) {
      variantsState.thinkingVariantLoading = false;
      updateControlState();
    }
  }
}
