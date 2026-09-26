"use strict";

/*
 * 上下文分项弹窗:输入框下方的上下文圆环点开后看到的分项占用。
 * 计划:docs/plan-is-true/2026-09-14/context-panel.md;接口:GET /api/sessions/{id}/context/breakdown。
 *
 * 口径跟后端走,前端不另算:
 * - 顶部总数有实测用实测,否则估算;分项一律是 o200k 估算。
 * - 实测 − 估算合计单列「分词器差异」,不按比例摊进各分项。
 * - 中转后端(claude-code / codex / agy)的差额是「CLI 自带(推算)」。
 * - 「上下文走势」是每回合结束时的占用,数据由挂载方给(getContextHistory),不参与合计。
 *
 * 单独成文件:app.js 已经上万行(与 todos.js / diff.js 同构)。弹窗挂在 dock 上而不是
 * .composer 里——后者 overflow:hidden 会把它裁掉,和模型菜单同一个原因。
 */
window.GqyContextPanel = (() => {
  const LABELS = {
    system: "系统提示词",
    tools_full: "工具 · 常驻",
    tools_stub: "工具 · stub",
    mcp: "MCP 工具",
    skills: "技能",
    summary: "压缩摘要",
    fossil: "化石化瞬态",
    messages: "消息历史",
  };
  const LIST_ORDER = ["messages", "mcp", "tools_stub", "tools_full", "skills", "fossil", "system", "summary"];
  const BAR_ORDER = ["system", "tools_full", "tools_stub", "mcp", "skills", "summary", "fossil", "messages"];
  const HELP = {
    system: "system 消息与预设对话",
    tools_full: "以完整 schema 发给模型的工具定义",
    tools_stub: "懒工具常驻的「真名 + 摘要 + 宽松参数壳」,完整契约用 load_tools 才展开",
    mcp: "MCP 服务器登记进来的工具定义",
    skills: "load_skill 取回的技能正文",
    summary: "压缩产生的摘要行",
    fossil: "runtime、联想记忆、提醒等发送过的瞬态内容,落库后逐字节回放",
    messages: "用户消息、她的回复、工具调用与结果",
    diff: "供应商实测总数 − 本地 o200k 估算合计。分项只能按估算拆,差额不摊进各项",
    cli: "单次请求实测 − 顾清影 发过去部分的估算。CLI 自己的系统提示词与原生工具 顾清影 看不到,这是推算,含分词器差异",
    buffer: "到自动压缩水位就开始压缩,这段实际用不到",
    deferred: "load_tools 能展开但还没展开的完整契约;展开前不占上下文",
  };
  const BACKEND_LABELS = { claude_code: "claude-code", codex: "codex", antigravity: "agy" };
  // 走势图:最多画最近 max 轮;预测看最近 recent 轮的增量;算出来超过 far 轮就不报数了。
  const TREND = { max: 48, recent: 5, far: 500, w: 240, h: 56 };

  let ctx = null;
  let pop = null;
  let open = false;
  let data = null;
  let loadedSession = "";
  let loadError = "";
  let seq = 0;
  let refreshTimer = 0;
  // idle | confirm | running | done | empty | failed
  let compact = { phase: "idle", from: null, message: "" };

  function el(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text != null) node.textContent = text;
    return node;
  }
  const fmt = (value) => ctx.formatTokens(Math.abs(Number(value) || 0));
  const signed = (value) => `${value > 0 ? "+" : value < 0 ? "−" : ""}${fmt(value)}`;
  const pct = (value, windowSize) => (windowSize > 0 ? `${((Math.abs(value) / windowSize) * 100).toFixed(1)}%` : "");
  const usedOf = (payload) => payload.measured_tokens ?? payload.estimate_tokens;

  function mount(options) {
    ctx = options;
    pop = options.pop;
    if (!ctx?.trigger || !pop) return;
    ctx.trigger.addEventListener("click", () => (open ? close() : show()));
    document.addEventListener("keydown", (event) => {
      if (!open || event.key !== "Escape") return;
      if (compact.phase === "confirm") {
        compact.phase = "idle";
        render();
        return;
      }
      close({ restoreFocus: true });
    });
    document.addEventListener("pointerdown", (event) => {
      if (!open || pop.contains(event.target) || ctx.trigger.contains(event.target)) return;
      close();
    });
    window.addEventListener("resize", position, { passive: true });
  }

  function show() {
    open = true;
    pop.hidden = false;
    ctx.trigger.setAttribute("aria-expanded", "true");
    if (compact.phase !== "running") compact = { phase: "idle", from: null, message: "" };
    render();
    position();
    load();
    window.requestAnimationFrame(() => pop.querySelector(".ctx-close")?.focus({ preventScroll: true }));
  }

  function close({ restoreFocus = false } = {}) {
    if (!open) return;
    open = false;
    pop.hidden = true;
    ctx.trigger.setAttribute("aria-expanded", "false");
    window.clearTimeout(refreshTimer);
    if (compact.phase === "confirm") compact.phase = "idle";
    if (restoreFocus) ctx.trigger.focus();
  }

  /// 同模型菜单:按圆环实际位置算,右边对齐圆环、浮在上方,夹回视口。手机上是
  /// 底部面板,位置全交给 CSS。
  function position() {
    if (!open) return;
    if (window.matchMedia("(max-width: 640px)").matches) {
      pop.style.right = "";
      pop.style.bottom = "";
      pop.style.maxHeight = "";
      return;
    }
    const dock = ctx.dock.getBoundingClientRect();
    const button = ctx.trigger.getBoundingClientRect();
    const gap = 8;
    const margin = 8;
    const width = pop.offsetWidth * ctx.uiScale();
    const rightGap = Math.min(dock.right - button.right, dock.right - margin - width);
    pop.style.right = `${Math.max(0, ctx.toLayout(rightGap))}px`;
    pop.style.bottom = `${ctx.toLayout(dock.bottom - button.top + gap)}px`;
    pop.style.maxHeight = `${Math.min(660, ctx.toLayout(Math.max(200, button.top - gap - margin)))}px`;
  }

  async function load() {
    const sessionId = String(ctx.getSessionId() || "");
    const mine = ++seq;
    loadError = "";
    if (loadedSession !== sessionId) data = null;
    render();
    if (!sessionId) {
      loadError = "还没有会话";
      render();
      return;
    }
    try {
      const response = await ctx.apiRequest(`/api/sessions/${encodeURIComponent(sessionId)}/context/breakdown`);
      const payload = await response.json();
      if (mine !== seq) return;
      if (response.ok === false) throw new Error(payload?.error || `HTTP ${response.status}`);
      data = payload;
      loadedSession = sessionId;
    } catch (error) {
      if (mine !== seq) return;
      loadError = error?.message || "统计失败";
    }
    if (mine !== seq) return;
    render();
    position();
  }

  /// app.js 的 updateContext 每次刷新圆环都会叫这里:开着就跟着重算(防抖),
  /// 换了会话就关掉,不让旧会话的分项挂在新会话的圆环上。
  function contextChanged() {
    if (!open) return;
    const sessionId = String(ctx.getSessionId() || "");
    if (loadedSession && sessionId !== loadedSession) {
      close();
      return;
    }
    if (compact.phase === "running" || ctx.isRunning?.()) return;
    window.clearTimeout(refreshTimer);
    refreshTimer = window.setTimeout(load, 800);
  }

  function render() {
    if (!pop || !open) return;
    pop.replaceChildren(renderHead(), renderBody(), renderFoot());
  }

  function help(text) {
    const node = el("span", "ctx-help", "?");
    node.title = text;
    node.setAttribute("aria-label", text);
    return node;
  }

  /// share:这一行在同组里的相对分量(0–1),画成名字后面的一截小条;
  /// 按组内最大值归一,不按窗口——否则几十 token 的项永远是一条看不见的线。
  function row({ name, sub, tokens, numText, windowSize, color, dotClass, dim, zero, helpText, share = null }) {
    const node = el("div", `ctx-row${dim ? " is-dim" : ""}${zero ? " is-zero" : ""}`);
    const dot = el("i", `ctx-dot${dotClass ? ` ${dotClass}` : ""}`);
    if (color) dot.style.background = color;
    const label = el("span", "ctx-name", name);
    if (sub) label.appendChild(el("small", null, sub));
    if (helpText) label.appendChild(help(helpText));
    const meter = el("span", "ctx-meter");
    if (share != null && share > 0) {
      const fill = el("i");
      fill.style.width = `${Math.max(4, Math.min(100, share * 100))}%`;
      if (color) fill.style.background = color;
      meter.appendChild(fill);
    }
    node.append(
      dot,
      label,
      meter,
      el("span", "ctx-num", numText ?? fmt(tokens)),
      el("span", "ctx-pct", tokens == null ? "" : pct(tokens, windowSize))
    );
    return node;
  }

  function groupLabel(left, right) {
    const node = el("div", "ctx-group-label");
    node.append(el("span", null, left), el("span", null, right || ""));
    return node;
  }

  function renderHead() {
    const head = el("header", "ctx-head");
    const top = el("div", "ctx-head-row");
    top.appendChild(el("span", "ctx-title", "上下文"));
    if (data) {
      const measured = data.measured_tokens != null;
      const badge = el("span", `ctx-badge ${measured ? "is-measured" : "is-estimate"}`, measured ? "实测" : "估算");
      badge.title = measured
        ? "上一回合最后一次请求,供应商报告的真实占用"
        : "本地 o200k 估算:刚压缩完、回合被打断或供应商没报用量时没有实测";
      top.appendChild(badge);
      const backend = BACKEND_LABELS[data.backend?.kind];
      if (backend) top.appendChild(el("span", "ctx-badge", backend));
    }
    const closeButton = el("button", "ctx-close", "✕");
    closeButton.type = "button";
    closeButton.setAttribute("aria-label", "关闭");
    closeButton.addEventListener("click", () => close({ restoreFocus: true }));
    top.appendChild(closeButton);
    head.appendChild(top);

    const fallback = ctx.getContext?.() || {};
    const used = data ? usedOf(data) : Number(fallback.tokens) || 0;
    const windowSize = data ? data.window : fallback.window;
    const hero = el("div", "ctx-hero");
    if (windowSize) hero.appendChild(renderDonut(data, used, windowSize));
    const stats = el("div", "ctx-stats");
    const total = el("div", "ctx-total");
    total.append(
      el("span", "ctx-used", fmt(used)),
      el("span", "ctx-win", windowSize ? `/ ${fmt(windowSize)}${data?.window_assumed ? " · 按配置" : ""}` : "/ 窗口未知")
    );
    stats.appendChild(total);
    if (windowSize) {
      const trim = Number(data?.thresholds?.trim_at_ratio) || 0.8;
      const left = Math.round(windowSize * trim) - used;
      stats.appendChild(el("span", `ctx-runway${left <= 0 ? " is-over" : ""}`,
        left > 0 ? `距自动压缩还有 ${fmt(left)}` : "已到自动压缩水位"));
    }
    hero.appendChild(stats);
    head.appendChild(hero);
    return head;
  }

  /// 头部的环形图:分项按顺序首尾相接,缓冲区是末段的淡色,两根刻度标出
  /// 自动压缩与强制压缩。环心是占比。和输入框里那块小表盘是同一个读法。
  function renderDonut(payload, used, windowSize) {
    const NS = "http://www.w3.org/2000/svg";
    const R = 26;
    const C = 2 * Math.PI * R;
    const svg = document.createElementNS(NS, "svg");
    svg.setAttribute("viewBox", "0 0 64 64");
    svg.setAttribute("class", "ctx-donut");
    svg.setAttribute("aria-hidden", "true");
    const ring = document.createElementNS(NS, "g");
    ring.setAttribute("transform", "rotate(-90 32 32)");
    const arc = (className, from, length, color, title) => {
      const circle = document.createElementNS(NS, "circle");
      circle.setAttribute("cx", "32");
      circle.setAttribute("cy", "32");
      circle.setAttribute("r", String(R));
      circle.setAttribute("class", className);
      circle.style.strokeDasharray = `${Math.max(0, length).toFixed(2)} ${C.toFixed(2)}`;
      circle.style.strokeDashoffset = `${(-from).toFixed(2)}`;
      if (color) circle.style.stroke = color;
      if (title) {
        const label = document.createElementNS(NS, "title");
        label.textContent = title;
        circle.appendChild(label);
      }
      ring.appendChild(circle);
    };
    const trim = Number(payload?.thresholds?.trim_at_ratio) || 0.8;
    const force = Number(payload?.thresholds?.compact_force_ratio) || 0.9;
    arc("ctx-donut-track", 0, C);
    arc("ctx-donut-buffer", trim * C, (1 - trim) * C, null, "自动压缩缓冲");
    let offset = 0;
    if (payload) {
      for (const key of BAR_ORDER) {
        const value = payload.categories?.[key] || 0;
        if (!value) continue;
        const length = (value / windowSize) * C;
        arc("ctx-donut-seg", offset, length, `var(--ctx-cat-${key})`, `${LABELS[key]} ${fmt(value)}`);
        offset += length;
      }
      const extra = used - payload.estimate_tokens;
      if (extra > 0) {
        const length = (extra / windowSize) * C;
        arc("ctx-donut-seg is-extra", offset, length, null, payload.backend?.kind !== "native" ? "CLI 自带(推算)" : "分词器差异");
      }
    } else {
      arc("ctx-donut-seg", 0, (used / windowSize) * C, "var(--accent)");
    }
    for (const [ratio, className] of [[trim, "is-trim"], [force, "is-force"]]) {
      const angle = ratio * 2 * Math.PI;
      const tick = document.createElementNS(NS, "line");
      tick.setAttribute("x1", String(32 + (R - 7) * Math.cos(angle)));
      tick.setAttribute("y1", String(32 + (R - 7) * Math.sin(angle)));
      tick.setAttribute("x2", String(32 + (R + 7) * Math.cos(angle)));
      tick.setAttribute("y2", String(32 + (R + 7) * Math.sin(angle)));
      tick.setAttribute("class", `ctx-donut-tick ${className}`);
      ring.appendChild(tick);
    }
    svg.appendChild(ring);
    const center = document.createElementNS(NS, "text");
    center.setAttribute("x", "32");
    center.setAttribute("y", "32");
    center.setAttribute("class", "ctx-donut-pct");
    center.textContent = pct(used, windowSize);
    svg.appendChild(center);
    return svg;
  }

  function renderBody() {
    const body = el("div", "ctx-body");
    if (loadError) {
      body.appendChild(el("div", "ctx-note is-error", `统计失败:${loadError}`));
      return body;
    }
    if (!data) {
      const note = el("div", "ctx-note ctx-loading");
      note.append(el("span", "ctx-spinner"), document.createTextNode("正在统计…"));
      body.appendChild(note);
      return body;
    }
    const relay = data.backend?.kind && data.backend.kind !== "native";
    const windowSize = data.window;
    const used = usedOf(data);

    if (relay) {
      body.appendChild(el("div", "ctx-note",
        "分项只覆盖 顾清影 发给 CLI 的部分。CLI 自己的系统提示词与原生工具看不到,那一行是推算(含分词器差异)。"));
    }
    if (!windowSize) {
      body.appendChild(el("div", "ctx-note", "窗口大小未知,算不出占比与剩余。在 设置 → 模型 里为这个模型填上下文窗口。"));
    }

    body.appendChild(groupLabel("在上下文里", "分项为估算 · o200k"));
    const largest = Math.max(1, ...LIST_ORDER.map((key) => data.categories?.[key] || 0));
    const zeros = [];
    for (const key of LIST_ORDER) {
      const value = data.categories?.[key] || 0;
      // 为 0 的分项折成一行:一整列灰掉的 0 只是在挤真正有量的那几行。
      if (!value) {
        zeros.push(LABELS[key]);
        continue;
      }
      body.appendChild(row({
        name: LABELS[key],
        tokens: value,
        windowSize,
        color: `var(--ctx-cat-${key})`,
        helpText: HELP[key],
        share: value / largest,
      }));
    }
    if (zeros.length) body.appendChild(el("div", "ctx-zero-line", `为 0:${zeros.join("、")}`));
    if (relay && data.backend.cli_overhead_tokens != null) {
      const value = data.backend.cli_overhead_tokens;
      body.appendChild(row({
        name: "CLI 自带", sub: "推算", tokens: value, numText: value < 0 ? signed(value) : fmt(value),
        windowSize, dotClass: "is-cli", dim: true, helpText: HELP.cli,
      }));
    } else if (!relay && data.measured_tokens != null) {
      const diff = data.measured_tokens - data.estimate_tokens;
      if (diff !== 0) {
        body.appendChild(row({
          name: "分词器差异", tokens: diff, numText: signed(diff), windowSize, dotClass: "is-diff", dim: true, helpText: HELP.diff,
        }));
      }
    }

    if (windowSize) {
      const trim = Number(data.thresholds?.trim_at_ratio) || 0.8;
      const buffer = Math.round(windowSize * (1 - trim));
      body.appendChild(groupLabel("窗口余量"));
      body.appendChild(row({ name: "自动压缩缓冲", tokens: buffer, windowSize, dotClass: "is-buffer", dim: true, helpText: HELP.buffer }));
      body.appendChild(row({ name: "剩余空间", tokens: Math.max(0, windowSize - used - buffer), windowSize, dotClass: "is-free", dim: true }));
    }

    if (data.deferred_tools_tokens > 0) {
      body.appendChild(groupLabel("不在上下文里"));
      const deferred = row({ name: "未加载工具", sub: "完整契约", numText: "—", tokens: null, windowSize, dotClass: "is-free", dim: true,
        helpText: `${HELP.deferred}(约 ${fmt(data.deferred_tools_tokens)})` });
      body.appendChild(deferred);
    }

    body.appendChild(renderTrend(windowSize, used));
    return body;
  }

  /// 「上下文走势」:本会话每回合结束时的上下文大小,旧 → 新。数据来自挂载方
  /// 的 getContextHistory(口径见 app.js 的 contextHistory),弹窗不碰模块状态。
  /// 压缩不单独标注,那一轮的线自然落下去。
  function renderTrend(windowSize, used) {
    const section = el("section", "ctx-trend");
    const points = (ctx.getContextHistory?.() || []).slice(-TREND.max);
    section.appendChild(groupLabel("上下文走势", points.length >= 2 ? `最近 ${points.length} 轮` : ""));
    if (points.length < 2) {
      section.appendChild(el("div", "ctx-trend-note is-empty", "再聊几轮就能看到走势"));
      return section;
    }
    const trimAt = windowSize ? windowSize * (Number(data.thresholds?.trim_at_ratio) || 0.8) : 0;
    const forceAt = windowSize ? windowSize * (Number(data.thresholds?.compact_force_ratio) || 0.9) : 0;
    section.appendChild(renderSparkline(points, trimAt, forceAt));
    const forecast = forecastText(points, used, trimAt);
    if (forecast) section.appendChild(el("div", "ctx-trend-note", forecast));
    return section;
  }

  /// 画布:viewBox 固定、preserveAspectRatio=none 横向拉满,线宽靠 CSS 的
  /// vector-effect 保持不变形。末点圆点是叠在上面的 HTML,免得被拉成椭圆。
  function renderSparkline(points, trimAt, forceAt) {
    const NS = "http://www.w3.org/2000/svg";
    const { w, h } = TREND;
    const svgNode = (tag, attrs) => {
      const node = document.createElementNS(NS, tag);
      for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, String(value));
      return node;
    };
    const peak = Math.max(1, ...points.map((point) => point.tokens));
    let top = peak * 1.15;
    // 峰值过了自动压缩水位的一半,才把纵轴拉到看得见两条水位线;否则大窗口里
    // 几万 token 会被压成贴底的一条线,走势反而看不出来。
    if (trimAt && peak >= trimAt * 0.5) top = Math.max(top, forceAt * 1.04);
    const x = (index) => (index / (points.length - 1)) * w;
    const y = (value) => h - (Math.min(value, top) / top) * h;
    const last = points[points.length - 1];

    const svg = svgNode("svg", { viewBox: `0 0 ${w} ${h}`, preserveAspectRatio: "none", class: "ctx-trend-svg", role: "img" });
    svg.setAttribute("aria-label", `上下文走势,最近 ${points.length} 轮,最新 ${fmt(last.tokens)}`);
    const line = points.map((point, index) => `${index ? "L" : "M"}${x(index).toFixed(1)} ${y(point.tokens).toFixed(1)}`).join(" ");
    svg.appendChild(svgNode("path", { d: `${line} L${w} ${h} L0 ${h} Z`, class: "ctx-trend-area" }));
    for (const [value, className] of [[trimAt, "is-trim"], [forceAt, "is-force"]]) {
      if (!value || value > top) continue;
      const at = y(value).toFixed(1);
      svg.appendChild(svgNode("line", { x1: 0, x2: w, y1: at, y2: at, class: `ctx-trend-rule ${className}` }));
    }
    svg.appendChild(svgNode("path", { d: line, class: "ctx-trend-line" }));
    // 每轮一条透明竖条接住悬停,<title> 给出这一轮的数字。
    const step = w / (points.length - 1);
    points.forEach((point, index) => {
      const from = Math.max(0, x(index) - step / 2);
      const to = Math.min(w, x(index) + step / 2);
      const hit = svgNode("rect", { x: from.toFixed(1), y: 0, width: (to - from).toFixed(1), height: h, class: "ctx-trend-hit" });
      const title = svgNode("title", {});
      title.textContent = `第 ${point.seq ?? index + 1} 轮 · ${fmt(point.tokens)}`;
      hit.appendChild(title);
      svg.appendChild(hit);
    });

    const chart = el("div", "ctx-trend-chart");
    const dot = el("i", "ctx-trend-dot");
    dot.style.left = "100%";
    dot.style.top = `${((y(last.tokens) / h) * 100).toFixed(1)}%`;
    chart.append(svg, dot);
    return chart;
  }

  /// 预测只看最近几轮的平均增长,压缩造成的回落(负增量)不算——否则刚压完
  /// 一次就会得出「没有增长」。当前值和头部用同一个数(实测优先),两处说法一致。
  function forecastText(points, used, trimAt) {
    if (!trimAt) return "";
    if (used >= trimAt) return "已到自动压缩水位";
    const recent = points.slice(-(TREND.recent + 1));
    const deltas = [];
    for (let index = 1; index < recent.length; index += 1) {
      const delta = recent[index].tokens - recent[index - 1].tokens;
      if (delta >= 0) deltas.push(delta);
    }
    const average = deltas.length ? deltas.reduce((sum, delta) => sum + delta, 0) / deltas.length : 0;
    if (average <= 0) return "最近几轮没有增长";
    const rounds = Math.max(1, Math.ceil((trimAt - used) / average));
    if (rounds > TREND.far) return "最近几轮几乎没有增长";
    return `按最近的速度,约 ${rounds} 轮后自动压缩`;
  }

  function renderFoot() {
    const foot = el("footer", "ctx-foot");
    if (compact.phase === "confirm") {
      const confirm = el("div", "ctx-confirm");
      confirm.appendChild(el("div", null,
        "用摘要替换较早的历史,最近的回合原样保留。压缩后缓存前缀会重建一次,下一轮输入按全价计。"));
      const actions = el("div", "ctx-confirm-actions");
      const cancel = el("button", "ctx-btn is-ghost", "取消");
      cancel.type = "button";
      cancel.addEventListener("click", () => {
        compact.phase = "idle";
        render();
      });
      const ok = el("button", "ctx-btn is-primary", "确定压缩");
      ok.type = "button";
      ok.addEventListener("click", runCompact);
      actions.append(cancel, ok);
      confirm.appendChild(actions);
      foot.appendChild(confirm);
      window.requestAnimationFrame(() => ok.focus({ preventScroll: true }));
    }

    const line = el("div", "ctx-foot-row");
    const trim = Number(data?.thresholds?.trim_at_ratio) || 0.8;
    const force = Number(data?.thresholds?.compact_force_ratio) || 0.9;
    line.appendChild(el("span", "ctx-thresholds", `到 ${Math.round(trim * 100)}% 自动压缩 · ${Math.round(force * 100)}% 强制`));
    if (compact.phase === "done" && data && compact.from != null) {
      line.appendChild(el("span", "ctx-result", `已压缩 ${fmt(compact.from)} → ${fmt(usedOf(data))}`));
    } else if (compact.phase === "empty" || compact.phase === "failed") {
      line.appendChild(el("span", `ctx-result${compact.phase === "failed" ? " is-error" : ""}`, compact.message));
    }

    const running = Boolean(ctx.isRunning?.());
    const button = el("button", "ctx-btn");
    button.type = "button";
    if (compact.phase === "running") {
      button.disabled = true;
      button.append(el("span", "ctx-spinner"), document.createTextNode("压缩中…"));
    } else {
      button.textContent = "立即压缩";
      button.disabled = running || compact.phase === "confirm" || !ctx.getSessionId();
      if (running) button.title = "回合进行中,两轮之间才能压缩";
      button.addEventListener("click", () => {
        compact = { phase: "confirm", from: null, message: "" };
        render();
      });
    }
    line.appendChild(button);
    foot.appendChild(line);
    return foot;
  }

  async function runCompact() {
    const sessionId = String(ctx.getSessionId() || "");
    compact = { phase: "running", from: data ? usedOf(data) : null, message: "" };
    render();
    try {
      const response = await ctx.apiRequest("/api/conversation/compact", {
        method: "POST",
        body: JSON.stringify({ session_id: sessionId }),
      });
      const payload = await response.json();
      if (response.ok === false) throw new Error(payload?.error || `HTTP ${response.status}`);
      // compact_now 返回空 = 整段对话都还在保留的尾巴以内,没有更早的回合可折(见 commands.js /compact)。
      if (payload?.result?.compacted === true) {
        compact.phase = "done";
        await ctx.onCompacted?.(sessionId);
        await load();
      } else {
        compact = { phase: "empty", from: null, message: "对话都还在保留的尾巴里,没有更早的回合可折叠" };
        render();
      }
    } catch (error) {
      compact = { phase: "failed", from: null, message: `压缩失败:${error?.message || "未知错误"}` };
      render();
    }
  }

  return { mount, contextChanged, close };
})();
