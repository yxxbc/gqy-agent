export function asFiniteNumber(value, fallback = 0) {
  const number = Number(value);
  return Number.isFinite(number) ? number : fallback;
}

export function formatInteger(value) {
  const number = Math.max(0, asFiniteNumber(value));
  try {
    return new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 0 }).format(number);
  } catch (_) {
    return String(Math.round(number));
  }
}

// 缓存命中率只以输入为分母：输出 token 要到下一轮才进入输入，把它算进
// 分母会让同样的缓存效果随回复变长而显得越来越差。三家供应商的用量字段
// 也都是这么定义的（DeepSeek 直接把 prompt 劈成 hit+miss）。
// 缓存命中率显示口径(09-11 用户拍板,与终端 render::usage::cache_percent 同规矩):
// 只有 >99.9 才显示成 100;99.1–99.9 留一位小数;99.0 及以下取整(99 不写 99.0)。
export function formatCachePercent(hit, total) {
  if (hit <= 0 || total <= 0) return null;
  const raw = Math.min(100, (hit / total) * 100);
  const roundedOne = Math.round(raw * 10) / 10;
  if (roundedOne >= 100) return "100";
  if (roundedOne > 99) return roundedOne.toFixed(1);
  return String(Math.round(raw));
}

export function cacheSuffix(cached, prompt) {
  const hit = asFiniteNumber(cached, 0);
  const total = asFiniteNumber(prompt, 0);
  const label = formatCachePercent(hit, total);
  return label == null ? "" : `（C${label}%）`;
}

// 输出速度:回合层测的「首块到末块」时长与对应 completion tokens,两者
// 任一为零就是没测到,不显示——和缓存率同一条规矩。
export function formatGenerationSpeed(tokens, millis) {
  const count = asFiniteNumber(tokens, 0);
  const duration = asFiniteNumber(millis, 0);
  if (count <= 0 || duration <= 0) return "";
  const rate = (count * 1000) / duration;
  return `每秒 ${rate >= 10 ? formatInteger(Math.round(rate)) : rate.toFixed(1)} toks`;
}

// 只取速度数字(给输入框下方信息行的「每秒 __ toks」用,模板已带「每秒/toks」)。
export function generationSpeedValue(tokens, millis) {
  const count = asFiniteNumber(tokens, 0);
  const duration = asFiniteNumber(millis, 0);
  if (count <= 0 || duration <= 0) return null;
  const rate = (count * 1000) / duration;
  return rate >= 10 ? formatInteger(Math.round(rate)) : rate.toFixed(1);
}

export function formatUsageMeta({ turnTotal, turnPrompt, turnCached, estimated, cumulative, cumulativePrompt, cumulativeCached, generationTokens, generationMs }) {
  const parts = [];
  const speed = formatGenerationSpeed(generationTokens, generationMs);
  if (speed) parts.push(speed);
  if (asFiniteNumber(turnTotal) > 0) {
    parts.push(`本轮${estimated ? "约 " : " "}${formatTokens(turnTotal)}${cacheSuffix(turnCached, turnPrompt)}`);
  }
  if (asFiniteNumber(cumulative) > 0) {
    parts.push(`累计 ${formatTokens(cumulative)}${cacheSuffix(cumulativeCached, cumulativePrompt)}`);
  }
  return parts.join(" · ");
}

export function formatTokens(value) {
  const number = Math.max(0, asFiniteNumber(value));
  if (number < 1000) return formatInteger(number);
  const useMillions = number >= 1_000_000;
  const amount = number / (useMillions ? 1_000_000 : 1000);
  const digits = amount >= 100 ? 0 : amount >= 10 ? 1 : 1;
  const suffix = useMillions ? "M" : "k";
  try {
    return `${new Intl.NumberFormat("zh-CN", { maximumFractionDigits: digits }).format(amount)}${suffix}`;
  } catch (_) {
    return `${amount.toFixed(digits)}${suffix}`;
  }
}

export function parseDate(value) {
  if (value == null || value === "") return null;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
}

export function formatTime(value) {
  const date = parseDate(value);
  if (!date) return "";
  try {
    return new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit", hour12: false }).format(date);
  } catch (_) {
    return date.toLocaleTimeString?.() || "";
  }
}

export function formatDateTime(value) {
  const date = parseDate(value);
  if (!date) return "";
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      hour12: false
    }).format(date);
  } catch (_) {
    return date.toLocaleString?.() || "";
  }
}

export function formatRelativeTime(value) {
  const date = parseDate(value);
  if (!date) return "";
  const difference = Date.now() - date.getTime();
  if (difference >= 0 && difference < 60_000) return "刚刚";
  if (difference >= 0 && difference < 3_600_000) return `${Math.max(1, Math.floor(difference / 60_000))} 分钟前`;
  const now = new Date();
  if (date.toDateString() === now.toDateString()) return formatTime(date);
  try {
    return new Intl.DateTimeFormat("zh-CN", { month: "numeric", day: "numeric" }).format(date);
  } catch (_) {
    return date.toLocaleDateString?.() || "";
  }
}

export function dayKey(value) {
  const date = parseDate(value);
  if (!date) return "unknown";
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
}

export function formatDayLabel(value) {
  const date = parseDate(value);
  if (!date) return "较早";
  const today = new Date();
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (date.toDateString() === today.toDateString()) return "今天";
  if (date.toDateString() === yesterday.toDateString()) return "昨天";
  try {
    return new Intl.DateTimeFormat("zh-CN", { year: "numeric", month: "long", day: "numeric" }).format(date);
  } catch (_) {
    return date.toLocaleDateString?.() || "较早";
  }
}

export function firstLine(value) {
  return String(value || "").split(/\r?\n/, 1)[0].trim();
}

export function modelMark(model) {
  const source = String(model?.provider_name || model?.provider_id || model?.model || "").trim();
  if (!source) return "--";
  const words = source.split(/[\s._/-]+/).filter(Boolean);
  const mark = words.length > 1 ? `${words[0][0] || ""}${words[1][0] || ""}` : source.slice(0, 2);
  return mark.toLocaleUpperCase("en-US");
}

export function modelKey(model) {
  return JSON.stringify([String(model?.provider_id || ""), String(model?.model || "")]);
}

export function effectiveUsageTotal(usage) {
  if (!usage || typeof usage !== "object") return 0;
  const explicit = asFiniteNumber(usage.total_tokens, 0);
  return explicit > 0 ? explicit : asFiniteNumber(usage.prompt_tokens, 0) + asFiniteNumber(usage.completion_tokens, 0);
}

export function formatFileSize(value) {
  const bytes = Math.max(0, asFiniteNumber(value));
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10 * 1024 ? 1 : 0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
