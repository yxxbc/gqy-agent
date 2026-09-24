import { apiRequest } from "../../core/api.js";
import { renderUsageRecords, renderUsageSources } from "./usage-sources.js";
import { elements } from "../../state/elements.js";

/* ───────────────────────── 控制台 · 数据统计 ─────────────────────────
   数据源:GET /api/usage/stats?range= 与 /api/usage/details。
   图表全部手写 DOM/SVG,与整站同一套 token,离线自包含。 */
export const usageState = {
  range: "1d",
  stats: null,
  loadSeq: 0,
  platformTab: null,
  // 用途筛选:按来源分桶(src → all|main|<kind>)。全局一个值时,选中某个
  // 细项会把没有该细项的另一张卡清空(08-26 审查)。
  kindFilters: new Map(),
  modelColors: new Map(),
};

export const USAGE_COLOR_VARS = ["var(--chart-1)", "var(--chart-2)", "var(--chart-4)", "var(--chart-3)"];

export const usageTip = document.createElement("div");

export function usageTipShow(html, event) {
  usageTip.innerHTML = html;
  usageTip.style.display = "block";
  usageTipMove(event);
}

export function usageTipMove(event) {
  const width = usageTip.offsetWidth;
  usageTip.style.left = `${Math.min(window.innerWidth - width - 12, event.clientX + 14)}px`;
  usageTip.style.top = `${Math.max(8, event.clientY - usageTip.offsetHeight - 12)}px`;
}

export function usageTipHide() {
  usageTip.style.display = "none";
}

// 计费估算显示:None/0 → null(不渲染);极小值给足小数位。
export function usageFmtCost(usd) {
  if (!Number.isFinite(usd) || usd <= 0) return null;
  if (usd < 0.01) return `$${usd.toFixed(4)}`;
  if (usd < 1) return `$${usd.toFixed(3)}`;
  if (usd < 100) return `$${usd.toFixed(2)}`;
  return `$${usd.toFixed(1)}`;
}

export function usageFmt(value) {
  if (value >= 1e9) return `${(value / 1e9).toFixed(2)}B`;
  if (value >= 1e6) return `${(value / 1e6).toFixed(2)}M`;
  if (value >= 1e3) return `${(value / 1e3).toFixed(1)}k`;
  return String(value);
}

export function usageSourceName(src) {
  if (src === "agent") return "智能体";
  if (src === "qq" || src === "onebot") return "QQ";
  return src;
}

// 来源内细项(后端 kinds):已含在来源合计里,只是拆出来看得见。
export function usageKindName(kind) {
  if (kind === "judge") return "主动回复判断";
  if (kind === "affection") return "好感度更新";
  if (kind === "group_join") return "入群审批";
  return kind;
}

// 明细表列窄,用短名;没有短名就退回全名。
export function usageKindShortName(kind) {
  if (kind === "judge") return "判断";
  if (kind === "affection") return "好感度";
  if (kind === "group_join") return "入群";
  return usageKindName(kind);
}

/* ── 图表色派生:跟随当前主题(含 matugen /theme.css 覆盖)──
   取 MD3 三色的"色相",明度错位+色度夹取整形成图表专用色;
   环邻 ΔE<15 时沿明度推开。内置双主题的派生结果已过校验脚本。 */
export const usageSrgbToLinear = (c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);

export const usageLinearToSrgb = (c) => (c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055);

export function usageHexToOklch(hex) {
  const match = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!match) return null;
  const n = parseInt(match[1], 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => usageSrgbToLinear(v / 255));
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const bb = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  return { L, C: Math.hypot(a, bb), H: (Math.atan2(bb, a) * 180) / Math.PI };
}

export function usageOklchToHex({ L, C, H }) {
  for (let c = C; c >= 0; c -= 0.004) {
    const h = (H * Math.PI) / 180;
    const a = c * Math.cos(h), bb = c * Math.sin(h);
    const l3 = L + 0.3963377774 * a + 0.2158037573 * bb;
    const m3 = L - 0.1055613458 * a - 0.0638541728 * bb;
    const s3 = L - 0.0894841775 * a - 1.291485548 * bb;
    const [l, m, s] = [l3 ** 3, m3 ** 3, s3 ** 3];
    const r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    const g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    const b = -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s;
    if ([r, g, b].every((v) => v >= -1e-4 && v <= 1 + 1e-4)) {
      const to255 = (v) => Math.round(Math.min(1, Math.max(0, usageLinearToSrgb(v))) * 255);
      return `#${[r, g, b].map((v) => to255(v).toString(16).padStart(2, "0")).join("")}`;
    }
  }
  return "#808080";
}

export function usageOklabDelta(a, b) {
  const rad = (H) => (H * Math.PI) / 180;
  const [aa, ab] = [a.C * Math.cos(rad(a.H)), a.C * Math.sin(rad(a.H))];
  const [ba, bb] = [b.C * Math.cos(rad(b.H)), b.C * Math.sin(rad(b.H))];
  return Math.hypot(a.L - b.L, aa - ba, ab - bb) * 100;
}

export function updateChartColors() {
  const styles = getComputedStyle(document.body);
  const read = (name) => usageHexToOklch(styles.getPropertyValue(name));
  const primary = read("--md-sys-color-primary");
  const secondary = read("--md-sys-color-secondary");
  const tertiary = read("--md-sys-color-tertiary");
  const surface = read("--md-sys-color-surface");
  if (!primary || !secondary || !tertiary || !surface) return; // 保底用 CSS 静态色
  const dark = surface.L < 0.5;
  const targetL = dark
    ? { c1: 0.60, c2: 0.66, c3: 0.55, c4: 0.64 }
    : { c1: 0.52, c2: 0.56, c3: 0.47, c4: 0.44 };
  const band = dark ? [0.49, 0.67] : [0.43, 0.62];
  const clampC = (c) => Math.min(0.17, Math.max(0.11, c));
  const colors = {
    c1: { L: targetL.c1, C: clampC(primary.C), H: primary.H },
    c2: { L: targetL.c2, C: clampC(secondary.C), H: secondary.H },
    c3: { L: targetL.c3, C: clampC(tertiary.C), H: tertiary.H },
    c4: { L: targetL.c4, C: clampC(primary.C), H: primary.H - 50 },
  };
  const ring = [["c1", "c2"], ["c2", "c4"], ["c4", "c3"], ["c3", "c1"]];
  for (let pass = 0; pass < 8; pass += 1) {
    let adjusted = false;
    for (const [xa, xb] of ring) {
      if (usageOklabDelta(colors[xa], colors[xb]) < 15) {
        const [lo, hi] = colors[xa].L <= colors[xb].L ? [xa, xb] : [xb, xa];
        colors[lo].L = Math.max(band[0], colors[lo].L - 0.025);
        colors[hi].L = Math.min(band[1], colors[hi].L + 0.025);
        adjusted = true;
      }
    }
    if (!adjusted) break;
  }
  ["c1", "c2", "c3", "c4"].forEach((key, index) =>
    document.body.style.setProperty(`--chart-${index + 1}`, usageOklchToHex(colors[key])));
  const top = dark
    ? { L: 0.72, C: Math.min(0.15, clampC(primary.C)) }
    : { L: 0.42, C: Math.min(0.16, clampC(primary.C)) };
  const base = dark
    ? { L: Math.min(0.34, surface.L + 0.06), C: 0.03 }
    : { L: Math.max(0.88, surface.L - 0.06), C: 0.025 };
  for (let i = 0; i < 5; i += 1) {
    const k = i / 4;
    document.body.style.setProperty(`--heat-${i}`, usageOklchToHex({
      L: base.L + (top.L - base.L) * k,
      C: base.C + (top.C - base.C) * k,
      H: primary.H,
    }));
  }
}

/* 色键含 provider:同名模型经不同网关(或模型名缺失)也能分色;
   同一 (provider, model) 跨栏目保持同色。 */
export function usageModelColor(provider, model) {
  const key = `${provider || ""}/${model || ""}`;
  if (!usageState.modelColors.has(key)) {
    usageState.modelColors.set(key, USAGE_COLOR_VARS[usageState.modelColors.size % USAGE_COLOR_VARS.length]);
  }
  return usageState.modelColors.get(key);
}

export function usageCacheRate(cacheRead, prompt) {
  if (!prompt) return null;
  const rate = Math.min(100, (cacheRead / prompt) * 100);
  // 两位小数;逼近满分时(>99.99)直接封顶 100——命中率是这套缓存
  // 工程的成绩单,四舍五入吃掉小数没有冲击力(验收 08-16)。
  if (rate > 99.99) return "100";
  return rate.toFixed(2);
}

export async function loadUsageStats() {
  const seq = ++usageState.loadSeq;
  elements.usageStamp.textContent = "正在载入…";
  try {
    const response = await apiRequest(`/api/usage/stats?range=${usageState.range}`);
    const data = await response.json();
    if (seq !== usageState.loadSeq) return;
    usageState.stats = data.stats;
    renderUsage();
    const now = new Date();
    const pad = (n) => String(n).padStart(2, "0");
    elements.usageStamp.textContent =
      `更新于 ${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}`;
  } catch (error) {
    if (seq !== usageState.loadSeq) return;
    elements.usageStamp.textContent = `载入失败:${error.message || error}`;
  }
}

export async function loadUsageRecords() {
  try {
    const params = new URLSearchParams({ limit: "50" });
    if (elements.usageSrcFilter.value) params.set("src", elements.usageSrcFilter.value);
    if (elements.usageModelFilter.value) params.set("model", elements.usageModelFilter.value);
    const response = await apiRequest(`/api/usage/details?${params}`);
    const data = await response.json();
    renderUsageRecords(data.records || []);
  } catch (_) {
    elements.usageRecords.innerHTML =
      `<tr class="u-day-row"><td colspan="7">明细载入失败</td></tr>`;
  }
}

/* 筛选选项来自"至今"聚合里出现过的来源与模型;保留当前选中值。 */
export function refreshUsageFilters(stats) {
  const sources = stats.sources || [];
  const fill = (select, entries) => {
    const current = select.value;
    const keepFirst = select.options[0];
    select.replaceChildren(keepFirst);
    for (const [value, label] of entries) {
      const option = document.createElement("option");
      option.value = value;
      option.textContent = label;
      select.appendChild(option);
    }
    select.value = entries.some(([value]) => value === current) ? current : "";
  };
  fill(elements.usageSrcFilter, sources.map((source) => [source.src, usageSourceName(source.src)]));
  const models = new Map();
  for (const source of sources) {
    for (const model of source.models || []) {
      if (model.model) models.set(model.model, true);
    }
  }
  fill(elements.usageModelFilter, [...models.keys()].sort().map((model) => [model, model]));
}

export function renderUsage() {
  const stats = usageState.stats;
  if (!stats) return;
  renderUsageTiles(stats);
  renderUsageHeat(stats.daily || []);
  renderUsageBars(stats);
  renderUsageSources(stats);
  refreshUsageFilters(stats);
}

export function renderUsageTiles(stats) {
  const totals = stats.totals || {};
  const prev = stats.prev_totals || null;
  const delta = (current, previous) => {
    if (!prev) return "";
    const base = previous || 0;
    if (!base) return "";
    const value = ((current || 0) / base - 1) * 100;
    const dir = value >= 0 ? "up" : "down";
    const sign = value >= 0 ? "+" : "";
    return `<span class="u-tl-right u-delta ${dir}" title="对比上一周期">${sign}${value.toFixed(0)}%</span>`;
  };
  const hit = usageCacheRate(totals.cache_read || 0, totals.prompt || 0);
  const RING_R = 15;
  const RING_C = 2 * Math.PI * RING_R;
  const icon = (path) => `<svg viewBox="0 0 24 24" aria-hidden="true">${path}</svg>`;
  const dailyAvg = usageState.range === "1d"
    ? ""
    : ` · 日均 ${(Number(totals.requests || 0) / rangeDayCount(stats)).toFixed(1)} 次`;
  const costValue = usageFmtCost(totals.cost);
  const costCoverage = Number(totals.costed_requests || 0) < Number(totals.requests || 0)
    ? `估算覆盖 ${Number(totals.costed_requests || 0).toLocaleString()}/${Number(totals.requests || 0).toLocaleString()} 次`
    : "按 models.dev 价格估算";
  elements.usageTiles.innerHTML = `
      <div class="u-tile"><div class="u-tile-label">${icon('<path d="M18 5H7l6 7-6 7h11"/>')}总消耗${delta(totals.total, prev && prev.total)}</div>
        <div class="u-tile-value">${usageFmt(totals.total || 0)}<small>tokens</small></div>
        <div class="u-tile-sub">输入 ${usageFmt(totals.prompt || 0)} · 输出 ${usageFmt(totals.completion || 0)}</div></div>
      <div class="u-tile"><div class="u-tile-label">${icon('<path d="M12 2v20"/><path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/>')}总消费${delta(totals.cost, prev && prev.cost)}</div>
        <div class="u-tile-value">${costValue ? `≈${costValue}` : "—"}</div>
        <div class="u-tile-sub">${costValue ? costCoverage : "暂无价格数据"}</div></div>
      <div class="u-tile"><div class="u-tile-label">${icon('<path d="M22 12h-4l-3 8L9 4l-3 8H2"/>')}请求数${delta(totals.requests, prev && prev.requests)}</div>
        <div class="u-tile-value">${Number(totals.requests || 0).toLocaleString()}</div>
        <div class="u-tile-sub">全部请求:对话 + 辅助${dailyAvg}</div></div>
      <div class="u-tile u-tile-flex"><div class="u-tf-main">
        <div class="u-tile-label">${icon('<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="3.5"/>')}缓存命中率</div>
        <div class="u-tile-value">${hit == null ? "—" : `${hit}<small>%</small>`}</div>
        <div class="u-tile-sub">命中 ${usageFmt(totals.cache_read || 0)} / 输入侧 ${usageFmt(totals.prompt || 0)}</div></div>
        <svg class="u-ring" viewBox="0 0 40 40" aria-hidden="true">
          <circle cx="20" cy="20" r="${RING_R}" fill="none" stroke="var(--chart-1)" stroke-opacity=".22" stroke-width="5"/>
          <circle cx="20" cy="20" r="${RING_R}" fill="none" stroke="var(--chart-3)" stroke-width="5" stroke-linecap="round"
            stroke-dasharray="${(((hit || 0) / 100) * RING_C).toFixed(1)} ${RING_C.toFixed(1)}" transform="rotate(-90 20 20)"/></svg></div>`;
}

export function rangeDayCount(stats) {
  if (usageState.range === "7d") return 7;
  if (usageState.range === "30d") return 30;
  if (usageState.range === "1d") return 1;
  const daily = stats.daily || [];
  const firstActive = daily.findIndex((day) => day.requests > 0);
  return firstActive === -1 ? 1 : Math.max(1, daily.length - firstActive);
}

export function usageParseDate(key) {
  const [year, month, day] = key.split("-").map(Number);
  return new Date(year, month - 1, day);
}

export function renderUsageHeat(daily) {
  const wrap = elements.usageHeatmap;
  const monthsEl = elements.usageHeatMonths;
  wrap.innerHTML = "";
  monthsEl.innerHTML = "";
  if (!daily.length) return;
  const max = Math.max(1, ...daily.map((day) => day.total));
  const firstDate = usageParseDate(daily[0].date);
  const lead = (firstDate.getDay() + 6) % 7;
  for (let index = 0; index < lead; index += 1) {
    const cell = document.createElement("i");
    cell.style.visibility = "hidden";
    wrap.appendChild(cell);
  }
  for (const day of daily) {
    const cell = document.createElement("i");
    cell.dataset.l = day.total === 0 ? 0 : Math.min(4, 1 + Math.floor((day.total / max) * 3.99));
    cell.addEventListener("mousemove", (event) => usageTipShow(
      `<b>${day.date}</b>
         <div class="row"><span>tokens</span><em>${usageFmt(day.total)}</em></div>
         <div class="row"><span>请求</span><em>${day.requests}</em></div>${usageFmtCost(day.cost) ? `
         <div class="row"><span>消费</span><em>≈${usageFmtCost(day.cost)}</em></div>` : ""}`, event));
    cell.addEventListener("mouseleave", usageTipHide);
    wrap.appendChild(cell);
  }
  const columns = Math.ceil((lead + daily.length) / 7);
  let previousMonth = -1;
  for (let column = 0; column < columns; column += 1) {
    const index = Math.min(Math.max(0, column * 7 - lead), daily.length - 1);
    const month = usageParseDate(daily[index].date).getMonth();
    if (month !== previousMonth) {
      const label = document.createElement("span");
      label.textContent = `${month + 1}月`;
      label.style.left = `${(column / columns) * 100}%`;
      monthsEl.appendChild(label);
      previousMonth = month;
    }
  }
  const requests = daily.reduce((sum, day) => sum + day.requests, 0);
  const tokens = daily.reduce((sum, day) => sum + day.total, 0);
  elements.usageHeatTotal.textContent =
    `共 ${requests.toLocaleString()} 次调用 · ${usageFmt(tokens)} tokens`;
}

export function renderUsageBars(stats) {
  const daily = stats.daily || [];
  let slice;
  let weekly = false;
  if (usageState.range === "1d") slice = daily.slice(-2); // 滚动 24h 跨两个日历日
  else if (usageState.range === "7d") slice = daily.slice(-7);
  else if (usageState.range === "30d") slice = daily.slice(-30);
  else {
    weekly = true;
    slice = [];
    for (let week = 0; week < Math.floor(daily.length / 7); week += 1) {
      const chunk = daily.slice(daily.length - (Math.floor(daily.length / 7) - week) * 7,
        daily.length - (Math.floor(daily.length / 7) - week - 1) * 7);
      if (!chunk.length) continue;
      const merged = { date: chunk[0].date, requests: 0, prompt: 0, completion: 0, cache_read: 0, total: 0, cost: 0 };
      for (const day of chunk) {
        merged.requests += day.requests; merged.prompt += day.prompt;
        merged.completion += day.completion; merged.cache_read += day.cache_read;
        merged.total += day.total; merged.cost += day.cost || 0;
      }
      slice.push(merged);
    }
  }
  elements.usageBarsHint.textContent = weekly ? "按周聚合 · 悬停看明细" : "悬停看明细";
  const bars = elements.usageBars;
  const xs = elements.usageBarsX;
  const ys = elements.usageBarsY;
  bars.innerHTML = ""; xs.innerHTML = ""; ys.innerHTML = "";
  const max = Math.max(...slice.map((day) => day.total), 0);
  if (!max) {
    bars.innerHTML = `<div class="u-empty" style="width:100%">该范围内没有调用记录</div>`;
    return;
  }
  const HEIGHT = 200;
  // 自适应刻度:目标 3-5 条网格线。老的固定档位在单日过亿 token 时
  // 会摆出上百条虚线和重叠标签(条纹背景 bug)。
  const rawStep = max / 4;
  const stepPow = 10 ** Math.floor(Math.log10(Math.max(1, rawStep)));
  const stepUnit = rawStep / stepPow;
  const step = (stepUnit <= 1 ? 1 : stepUnit <= 2 ? 2 : stepUnit <= 5 ? 5 : 10) * stepPow;
  const yLabel = (value, text) => {
    const label = document.createElement("span");
    label.textContent = text;
    label.style.bottom = `${(value / max) * HEIGHT}px`;
    ys.appendChild(label);
  };
  yLabel(0, "0");
  for (let value = step; value <= max; value += step) {
    const grid = document.createElement("div");
    grid.className = "u-gridline";
    grid.style.bottom = `${(value / max) * HEIGHT}px`;
    bars.appendChild(grid);
    yLabel(value, usageFmt(value));
  }
  slice.forEach((day, index) => {
    const slot = document.createElement("div");
    slot.className = "u-bar-slot";
    const column = document.createElement("div");
    column.className = "u-bar-col";
    const fresh = Math.max(0, day.prompt - day.cache_read);
    for (const [value, cls] of [[fresh, "s1"], [day.completion, "s2"], [day.cache_read, "s3"]]) {
      const segment = document.createElement("i");
      segment.className = cls;
      segment.style.height = `${Math.max(value > 0 ? 1 : 0, (value / max) * HEIGHT)}px`;
      column.appendChild(segment);
    }
    slot.appendChild(column);
    column.addEventListener("mousemove", (event) => usageTipShow(
      `<b>${day.date.slice(5)}${weekly ? " 起当周" : ""}</b>
         <div class="row"><span><i style="background:var(--chart-1)"></i>新输入</span><em>${usageFmt(fresh)}</em></div>
         <div class="row"><span><i style="background:var(--chart-2)"></i>输出</span><em>${usageFmt(day.completion)}</em></div>
         <div class="row"><span><i style="background:var(--chart-3)"></i>缓存命中</span><em>${usageFmt(day.cache_read)}</em></div>
         <div class="row"><span>请求</span><em>${day.requests}</em></div>
         <div class="row"><span>合计</span><em>${usageFmt(day.total)}</em></div>${usageFmtCost(day.cost) ? `
         <div class="row"><span>消费</span><em>≈${usageFmtCost(day.cost)}</em></div>` : ""}`, event));
    column.addEventListener("mouseleave", usageTipHide);
    bars.appendChild(slot);
    const label = document.createElement("span");
    label.textContent = weekly
      ? (index % 4 ? "" : day.date.slice(5))
      : slice.length > 16
        ? (index % 5 ? "" : day.date.slice(5))
        : slice.length === 1 ? day.date.slice(5) : day.date.slice(8);
    xs.appendChild(label);
  });
}

/// 原 app.js 顶层的副作用语句，由入口在启动时按原顺序调用。
export function start() {
  usageTip.className = "u-chart-tip";

  document.body.appendChild(usageTip);
}
