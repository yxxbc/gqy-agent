import { asFiniteNumber, dayKey, formatInteger } from "../../core/format.js";
import { accountLabel, accountState, loadAccountNames } from "../accounts.js";
import { isAdmin } from "../auth.js";
import { usageCacheRate, usageFmt, usageFmtCost, usageKindName, usageKindShortName, usageModelColor, usageSourceName, usageState, usageTipHide, usageTipShow } from "./usage.js";
import { elements } from "../../state/elements.js";

export function renderUsageSources(stats) {
  const container = elements.usageSources;
  container.innerHTML = "";
  const sources = stats.sources || [];
  if (!sources.length) {
    container.innerHTML = `<div class="u-card"><div class="u-empty">该范围内没有调用记录</div></div>`;
    return;
  }
  const agent = sources.find((source) => source.src === "agent");
  const platforms = sources.filter((source) => source.src !== "agent");
  if (isAdmin() && Array.isArray(stats.accounts) && stats.accounts.length > 1) {
    container.appendChild(buildUsageAccountsCard(stats.accounts));
  }
  if (agent) {
    container.appendChild(buildUsageSourceCard(
      "模型消耗明细 · 智能体",
      "终端 / WebUI / 定时任务 / 子代理 · 悬停环形图或表行看联动",
      agent,
      stats,
      null,
    ));
  }
  if (platforms.length) {
    if (!usageState.platformTab || !platforms.some((source) => source.src === usageState.platformTab)) {
      usageState.platformTab = platforms[0].src;
    }
    const active = platforms.find((source) => source.src === usageState.platformTab) || platforms[0];
    container.appendChild(buildUsageSourceCard(
      "模型消耗明细 · 通讯平台",
      "按平台分页 · 同一模型全页同色",
      active,
      stats,
      platforms,
    ));
  }
}

/// 总表按人拆(管理员):空账号 = 管理员自己 + 终端 + 通讯平台。名字来自
/// 成员表,没拉到之前先显示 id。
export function buildUsageAccountsCard(accounts) {
  const card = document.createElement("div");
  card.className = "u-card";
  card.innerHTML = `<div class="u-card-head"><h3>按人拆分</h3><span class="u-hint">成员的 WebUI 会话各记各的 · 未署名 = 管理员/终端/通讯平台</span></div>`;
  const scroll = document.createElement("div");
  scroll.className = "u-table-scroll";
  const table = document.createElement("table");
  table.className = "u-table";
  table.innerHTML = `<thead><tr><th>账号</th><th class="num">调用</th><th class="num">输入</th><th class="num">输出</th><th class="num">合计</th><th class="num">消费</th></tr></thead>`;
  const body = document.createElement("tbody");
  for (const entry of accounts) {
    const row = document.createElement("tr");
    const cells = [
      accountLabel(entry.acct),
      formatInteger(entry.requests),
      usageFmt(asFiniteNumber(entry.prompt)),
      usageFmt(asFiniteNumber(entry.completion)),
      usageFmt(asFiniteNumber(entry.total)),
      usageFmtCost(asFiniteNumber(entry.cost)) || "—",
    ];
    cells.forEach((text, index) => {
      const cell = document.createElement("td");
      if (index > 0) cell.className = "num";
      cell.textContent = text;
      row.appendChild(cell);
    });
    body.appendChild(row);
  }
  table.appendChild(body);
  scroll.appendChild(table);
  card.appendChild(scroll);
  if (!accountState.names.size && isAdmin()) loadAccountNames().then(() => renderUsageSources(usageState.stats)).catch(() => {});
  return card;
}

export function buildUsageSourceCard(title, hint, source, stats, platformTabs) {
  const card = document.createElement("div");
  card.className = "u-card";
  const head = document.createElement("div");
  head.className = "u-card-head";
  head.innerHTML = `<h3>${title}</h3><span class="u-hint">${hint}</span>`;
  if (platformTabs && platformTabs.length) {
    const seg = document.createElement("div");
    seg.className = "con-segmented";
    seg.style.marginLeft = "auto";
    for (const platform of platformTabs) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = usageSourceName(platform.src);
      button.classList.toggle("on", platform.src === source.src);
      button.addEventListener("click", () => {
        usageState.platformTab = platform.src;
        renderUsageSources(usageState.stats);
      });
      seg.appendChild(button);
    }
    head.appendChild(seg);
  }
  const filterKinds = (source.kinds || []).filter((kind) => Number(kind.total || 0) > 0);
  if (filterKinds.length) {
    const seg = document.createElement("div");
    seg.className = "con-segmented";
    seg.style.marginLeft = platformTabs && platformTabs.length ? "8px" : "auto";
    const active = usageState.kindFilters.get(source.src) || "all";
    const choices = [["all", "全部"], ["main", "其它"]];
    for (const kind of filterKinds) choices.push([kind.kind, usageKindName(kind.kind)]);
    for (const [value, label] of choices) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = label;
      button.title = value === "main"
        ? "未标注用途的调用:主线回复,以及好感度、入群审批等尚未打标的辅助调用"
        : label;
      button.classList.toggle("on", active === value);
      button.addEventListener("click", () => {
        usageState.kindFilters.set(source.src, value);
        renderUsageSources(usageState.stats);
      });
      seg.appendChild(button);
    }
    head.appendChild(seg);
  }
  card.appendChild(head);

  const body = document.createElement("div");
  body.className = "u-model-body";
  const donutWrap = document.createElement("div");
  donutWrap.className = "u-donut-wrap";
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 120 120");
  const center = document.createElement("div");
  center.className = "u-donut-center";
  donutWrap.appendChild(svg);
  donutWrap.appendChild(center);
  body.appendChild(donutWrap);

  const scroll = document.createElement("div");
  scroll.className = "u-table-scroll";
  const table = document.createElement("table");
  table.className = "u-table u-models-table";
  table.innerHTML = `<thead><tr><th>模型</th><th class="num">占比</th><th class="num">请求</th>
      <th class="num">输入</th><th class="num">输出</th><th class="num">消费</th><th>缓存命中</th></tr></thead>`;
  const tbody = document.createElement("tbody");
  const tfoot = document.createElement("tfoot");
  table.appendChild(tbody);
  table.appendChild(tfoot);
  scroll.appendChild(table);
  body.appendChild(scroll);
  card.appendChild(body);

  // 细项(主动回复判断等)摊进饼图与模型表:每个模型先扣掉细项占用的部分
  // 作为"主线",细项再各自成段——细项只当页脚摆着就看不出它吃掉了多少。
  const kinds = (source.kinds || []).filter((kind) => Number(kind.total || 0) > 0);
  const kindShare = new Map();
  for (const kind of kinds) {
    for (const model of kind.models || []) {
      const key = `${model.provider}\u0000${model.model}`;
      const prev = kindShare.get(key) || { total: 0, requests: 0, prompt: 0, completion: 0, cost: 0, cache_read: 0 };
      kindShare.set(key, {
        total: prev.total + Number(model.total || 0),
        requests: prev.requests + Number(model.requests || 0),
        prompt: prev.prompt + Number(model.prompt || 0),
        completion: prev.completion + Number(model.completion || 0),
        cache_read: prev.cache_read + Number(model.cache_read || 0),
        cost: prev.cost + Number(model.cost || 0),
      });
    }
  }
  const models = [];
  for (const model of source.models || []) {
    const taken = kindShare.get(`${model.provider}\u0000${model.model}`);
    if (!taken) {
      models.push(model);
      continue;
    }
    const rest = {
      ...model,
      total: Number(model.total || 0) - taken.total,
      requests: Number(model.requests || 0) - taken.requests,
      prompt: Number(model.prompt || 0) - taken.prompt,
      completion: Number(model.completion || 0) - taken.completion,
      cache_read: Number(model.cache_read || 0) - taken.cache_read,
      cost: Number(model.cost || 0) - taken.cost,
    };
    if (rest.total > 0 || rest.requests > 0) models.push(rest);
  }
  for (const kind of kinds) {
    for (const model of kind.models || []) {
      if (!Number(model.total || 0)) continue;
      models.push({ ...model, kindId: kind.kind, kindLabel: usageKindName(kind.kind) });
    }
  }
  models.sort((a, b) => Number(b.total || 0) - Number(a.total || 0));
  // 筛选只在有细项的卡上生效;合计随筛选重算,否则占比会拿全量当分母。
  const filter = filterKinds.length
    ? usageState.kindFilters.get(source.src) || "all"
    : "all";
  const visible = models.filter((model) => {
    if (filter === "all") return true;
    if (filter === "main") return !model.kindLabel;
    return model.kindId === filter;
  });
  const aggregate = filter === "all"
    ? source
    : visible.reduce((sum, model) => ({
        requests: sum.requests + Number(model.requests || 0),
        prompt: sum.prompt + Number(model.prompt || 0),
        completion: sum.completion + Number(model.completion || 0),
        cache_read: sum.cache_read + Number(model.cache_read || 0),
        total: sum.total + Number(model.total || 0),
        cost: sum.cost + Number(model.cost || 0),
      }), { requests: 0, prompt: 0, completion: 0, cache_read: 0, total: 0, cost: 0 });
  const requests = Number(aggregate.requests || 0);
  const defCenter = `<div><b>${usageFmt(aggregate.total || 0)}</b><small>token 合计</small>
      <span class="u-donut-sub">${requests.toLocaleString()} 次请求</span></div>`;
  center.innerHTML = defCenter;
  if (!visible.length || !aggregate.total) {
    tbody.innerHTML = `<tr><td colspan="7"><div class="u-empty">暂无记录</div></td></tr>`;
    return card;
  }

  const globalShare = stats.totals && stats.totals.total
    ? Math.round((aggregate.total / stats.totals.total) * 100)
    : null;
  const sourceHit = usageCacheRate(aggregate.cache_read || 0, aggregate.prompt || 0);
  tfoot.innerHTML = `<tr><td>合计${globalShare == null ? "" :
    ` <small style="color:var(--text-faint);font-weight:400">占全局 ${globalShare}%</small>`}</td>
      <td></td><td class="num">${requests}</td>
      <td class="num">${usageFmt(aggregate.prompt || 0)}</td>
      <td class="num">${usageFmt(aggregate.completion || 0)}</td>
      <td class="num">${usageFmtCost(aggregate.cost) ? `≈${usageFmtCost(aggregate.cost)}` : "—"}</td>
      <td>${sourceHit == null ? "" : `<span class="u-cache-pill">${sourceHit}%</span>`}</td></tr>`;

  const RADIUS = 44;
  const CIRCUM = 2 * Math.PI * RADIUS;
  // 单段就是完整圆环;分段间隙只在真的有多段时存在,且不超过最小段
  // 的一半,防止小切片被间隙吃掉。
  const minShare = Math.min(...visible.map((model) => model.total / aggregate.total));
  const GAP = visible.length > 1 ? Math.min(3, Math.max(0.5, (minShare * CIRCUM) / 2)) : 0;
  let accumulated = 0;
  visible.forEach((model, index) => {
    const share = model.total / aggregate.total;
    const baseName = model.model || "(未标模型)";
    const modelName = model.kindLabel ? `${baseName} · ${model.kindLabel}` : baseName;
    const color = usageModelColor(model.provider, model.model);
    const hit = usageCacheRate(model.cache_read || 0, model.prompt || 0);
    const row = document.createElement("tr");
    // 细项行:同模型同色(全页同色规则),用虚线点与徽章区分用途。
    const dot = model.kindLabel
      ? `<i class="u-dot u-dot-kind" style="background:${color}"></i>`
      : `<i class="u-dot" style="background:${color}"></i>`;
    row.innerHTML = `<td class="u-model-name"><b>${dot}${baseName}${
        model.kindLabel ? `<span class="u-kind-tag">${model.kindLabel}</span>` : ""
      }</b>
          <small><i class="u-dot" style="visibility:hidden"></i>${model.provider || "—"}</small></td>
        <td class="num">${Math.round(share * 100)}%</td>
        <td class="num">${model.requests}</td>
        <td class="num">${usageFmt(model.prompt || 0)}</td>
        <td class="num">${usageFmt(model.completion || 0)}</td>
        <td class="num">${usageFmtCost(model.cost) ? `≈${usageFmtCost(model.cost)}` : "—"}</td>
        <td>${hit == null ? "—" : `<span class="u-cache-pill">${hit}%</span>`}</td>`;
    tbody.appendChild(row);

    const length = Math.max(0.5, share * CIRCUM - GAP);
    const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    circle.setAttribute("cx", "60");
    circle.setAttribute("cy", "60");
    circle.setAttribute("r", String(RADIUS));
    circle.setAttribute("fill", "none");
    circle.setAttribute("stroke", color);
    circle.setAttribute("stroke-width", "18");
    if (model.kindLabel) circle.setAttribute("stroke-opacity", "0.5");
    circle.setAttribute("stroke-dasharray", `${length.toFixed(2)} ${(CIRCUM - length).toFixed(2)}`);
    circle.setAttribute("stroke-dashoffset", `${(-(accumulated * CIRCUM + GAP / 2)).toFixed(2)}`);
    circle.setAttribute("transform", "rotate(-90 60 60)");
    circle.addEventListener("mousemove", (event) => {
      circle.setAttribute("stroke-width", "21");
      row.classList.add("hl");
      center.innerHTML = `<div><b>${Math.round(share * 100)}%</b><small>${modelName}</small></div>`;
      usageTipShow(
        `<b>${modelName}</b>
           <div class="row"><span>占比</span><em>${Math.round(share * 100)}%</em></div>
           <div class="row"><span>请求</span><em>${model.requests}</em></div>
           <div class="row"><span>输入</span><em>${usageFmt(model.prompt || 0)}</em></div>
           <div class="row"><span>输出</span><em>${usageFmt(model.completion || 0)}</em></div>${usageFmtCost(model.cost) ? `
           <div class="row"><span>消费</span><em>≈${usageFmtCost(model.cost)}</em></div>` : ""}
           <div class="row"><span>缓存命中</span><em>${hit == null ? "—" : `${hit}%`}</em></div>`, event);
    });
    circle.addEventListener("mouseleave", () => {
      circle.setAttribute("stroke-width", "18");
      row.classList.remove("hl");
      center.innerHTML = defCenter;
      usageTipHide();
    });
    row.addEventListener("mouseenter", () => circle.setAttribute("stroke-width", "21"));
    row.addEventListener("mouseleave", () => circle.setAttribute("stroke-width", "18"));
    svg.appendChild(circle);
    accumulated += share;
  });
  return card;
}

export function renderUsageRecords(records) {
  const tbody = elements.usageRecords;
  tbody.innerHTML = "";
  if (!records.length) {
    tbody.innerHTML = `<tr class="u-day-row"><td colspan="8">还没有任何调用记录</td></tr>`;
    return;
  }
  const today = new Date();
  const dayKey = (date) =>
    `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
  const todayKey = dayKey(today);
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  const yesterdayKey = dayKey(yesterday);
  let currentDay = null;
  for (const record of records) {
    const date = new Date(record.ts * 1000);
    const key = dayKey(date);
    if (key !== currentDay) {
      currentDay = key;
      const label = key === todayKey ? "今天" : key === yesterdayKey ? "昨天" : "";
      const row = document.createElement("tr");
      row.className = "u-day-row";
      row.innerHTML = `<td colspan="8">${label ? `${label} · ` : ""}${key.slice(5)}</td>`;
      tbody.appendChild(row);
    }
    const pad = (n) => String(n).padStart(2, "0");
    const hit = usageCacheRate(record.cache_read || 0, record.prompt || 0);
    const row = document.createElement("tr");
    row.innerHTML = `<td class="time">${pad(date.getHours())}:${pad(date.getMinutes())}</td>
        <td><span class="u-src-pill">${usageSourceName(record.src || "agent")}</span></td>
        <td class="u-model-name"><b>${record.model || "(未标模型)"}</b><small>${record.provider || "—"}</small></td>
        <td class="num">${usageFmt(record.prompt || 0)}</td>
        <td class="num">${usageFmt(record.completion || 0)}</td>
        <td class="num">${usageFmtCost(record.cost) ? `≈${usageFmtCost(record.cost)}` : "—"}</td>
        <td>${hit == null ? "—" : `<span class="u-cache-pill">${hit}%</span>`}</td>
        <td><span class="u-type-pill ${record.kind ? "t-kind" : record.aux ? "t-aux" : "t-chat"}">${
        record.kind ? usageKindShortName(record.kind) : record.aux ? "辅助" : "对话"
      }</span></td>`;
    tbody.appendChild(row);
  }
}
