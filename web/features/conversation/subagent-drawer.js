import { apiRequest } from "../../core/api.js";
import { makeIconSlot } from "../../core/icons.js";
import { formatJobDuration } from "../jobs.js";
import { renderMarkdown } from "../markdown/render.js";
import { renderSubagentProgress, setSubagentLiveTap, subagentCountsText, subEndContent, subEndReasoning } from "./subagent.js";
import { state } from "../../state/store.js";

// ── 子代理详情抽屉 ────────────────────────────────────────────────
// 点子代理卡片 / 后台任务行上的「详情」打开:右侧滑出(手机全屏),从上到下是
// 抬头(描述、开发/档位、状态、次数、词元、耗时、模型)、完整 prompt、完整过程
// 时间线、最终结论。过程数据两个来源:
//   - 还在跑、页面上有它的实时 sink:照 sink 攒下的标记回放,之后实时标记经
//     liveTap 旁路继续流进来(不经服务器,不丢当前正在长的那段思考);
//   - 其余(已结束、刷新后、翻历史):读 /api/subagents/{id},库里是全的。
// 抬头与结论始终以接口为准,跑着时每 3 秒拉一次,跑完即停。

const POLL_MS = 3000;

let current = null;

function closeDrawer() {
  if (!current) return;
  const drawer = current;
  current = null;
  if (drawer.timer) clearInterval(drawer.timer);
  if (drawer.ticker) clearInterval(drawer.ticker);
  drawer.root.classList.remove("is-open");
  drawer.root.addEventListener("transitionend", () => drawer.root.remove(), { once: true });
  // 没有过渡(减少动态效果)时 transitionend 不来,兜底移除。
  window.setTimeout(() => drawer.root.remove(), 400);
  document.body.classList.remove("subagent-drawer-open");
  drawer.returnFocus?.focus?.({ preventScroll: true });
}

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = text;
  return node;
}

function section(title) {
  const wrap = el("section", "subagent-drawer-section");
  wrap.appendChild(el("h3", "subagent-drawer-heading", title));
  return wrap;
}

function statusWord(status, state) {
  if (status === "running") return "运行中";
  if (status === "interrupted") return "已中断";
  if (state === "error") return "失败";
  if (state === "budget_reached") return "步数用尽";
  return "已完成";
}

function durationSeconds(data) {
  const start = Date.parse(data?.created_at || "");
  if (!Number.isFinite(start)) return null;
  const end = data?.status === "running" ? Date.now() : Date.parse(data?.updated_at || "");
  if (!Number.isFinite(end)) return null;
  return Math.max(0, (end - start) / 1000);
}

function tokenFigure(data) {
  const total = Number(data?.usage?.total_tokens || 0);
  if (!total) return "";
  if (total >= 1e6) return `${(total / 1e6).toFixed(2)}M 词元`;
  if (total >= 1e3) return `${(total / 1e3).toFixed(1)}K 词元`;
  return `${total} 词元`;
}

// 从过程里找任务简介(描述、prompt、dev、档位):接口的 prompt 为空(旧数据)时兜底。
function briefFromMarkers(markers) {
  const prefix = "__subagent_brief__";
  for (const marker of markers || []) {
    if (!String(marker).startsWith(prefix)) continue;
    try {
      return JSON.parse(String(marker).slice(prefix.length));
    } catch {
      return null;
    }
  }
  return null;
}

function buildShell(title) {
  const root = el("div", "subagent-drawer-root");
  const scrim = el("div", "subagent-drawer-scrim");
  scrim.addEventListener("click", closeDrawer);
  const panel = el("aside", "subagent-drawer");
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-label", "子代理详情");
  panel.tabIndex = -1;

  const head = el("header", "subagent-drawer-head");
  const titles = el("div", "subagent-drawer-titles");
  const kicker = el("span", "subagent-drawer-kicker", "子代理");
  const name = el("h2", "subagent-drawer-title", title || "子代理");
  titles.append(kicker, name);
  const close = el("button", "subagent-drawer-close");
  close.type = "button";
  close.title = "关闭";
  close.setAttribute("aria-label", "关闭");
  close.appendChild(makeIconSlot("x"));
  close.addEventListener("click", closeDrawer);
  head.append(titles, close);

  const meta = el("div", "subagent-drawer-meta");
  const body = el("div", "subagent-drawer-body");

  const promptSection = section("任务");
  const prompt = el("div", "subagent-drawer-prompt");
  promptSection.appendChild(prompt);

  const traceSection = section("过程");
  const blocks = el("div", "sub-blocks assistant-blocks subagent-drawer-timeline");
  const traceEmpty = el("p", "subagent-drawer-empty", "加载中…");
  traceSection.append(blocks, traceEmpty);

  const resultSection = section("结论");
  const result = el("div", "subagent-drawer-result markdown-body");
  resultSection.appendChild(result);
  resultSection.hidden = true;

  body.append(promptSection, traceSection, resultSection);
  panel.append(head, meta, body);
  root.append(scrim, panel);
  return { root, panel, kicker, name, meta, prompt, promptSection, blocks, traceEmpty, resultSection, result, body };
}

function renderMeta(drawer) {
  const data = drawer.data || {};
  const brief = drawer.brief || {};
  const kicker = [brief.dev ? "开发子代理" : "子代理"];
  const tier = data.tier || brief.tier;
  if (tier) kicker.push(tier);
  drawer.ui.kicker.textContent = kicker.join(" · ");
  const title = data.description || brief.description || drawer.title;
  if (title) drawer.ui.name.textContent = title;

  const items = [];
  const status = drawer.liveRunning ? "running" : (data.status || "running");
  const chip = el("span", `subagent-drawer-status is-${status === "running" ? "running" : (data.state === "error" || status === "interrupted" ? "failed" : "done")}`);
  chip.textContent = statusWord(status, data.state);
  items.push(chip);
  const counts = subagentCountsText(drawer.sink);
  const statsCalls = Number(data?.stats?.tool_calls);
  if (Number.isFinite(statsCalls) && statsCalls > 0 && !drawer.sink.toolCount) items.push(el("span", "", `${statsCalls} 次工具`));
  if (counts) items.push(el("span", "", counts));
  else if (tokenFigure(data)) items.push(el("span", "", tokenFigure(data)));
  const seconds = durationSeconds({ ...data, status });
  if (seconds != null) items.push(el("span", "subagent-drawer-duration", formatJobDuration(seconds)));
  if (data.model) {
    const model = el("span", "subagent-drawer-model", data.model);
    model.title = data.provider_id ? `${data.provider_id} / ${data.model}` : data.model;
    items.push(model);
  }
  drawer.ui.meta.replaceChildren(...items);
}

function renderPrompt(drawer) {
  const text = String(drawer.data?.prompt || drawer.brief?.prompt || "").trim();
  drawer.ui.prompt.textContent = text || "(没有记录到 prompt)";
}

function renderResult(drawer) {
  const data = drawer.data;
  if (!data || data.status === "running" || drawer.liveRunning) {
    drawer.ui.resultSection.hidden = true;
    return;
  }
  drawer.ui.resultSection.hidden = false;
  drawer.ui.result.classList.toggle("is-error", Boolean(data.error));
  if (data.error) {
    drawer.ui.result.textContent = data.error;
  } else if (data.status === "interrupted") {
    drawer.ui.result.textContent = "子代理没有跑完就被中断了。";
  } else {
    renderMarkdown(drawer.ui.result, String(data.result || "").trim() || "(没有输出)");
  }
}

function newSink(drawer) {
  // 抽屉自己的时间线 sink:brief 已单独成段(置 true 跳过),不汇进输入框「累计」。
  drawer.sink = {
    blocks: drawer.ui.blocks, brief: true, think: null, thinkAccum: "", contentBlock: null,
    contentAccum: "", pendingCall: null, taskPeek: null, taskToken: null, peekLine: "",
    noCumulative: true, drawer: true,
  };
  drawer.ui.blocks.replaceChildren();
}

function replay(drawer, markers) {
  newSink(drawer);
  drawer.rendered = 0;
  appendMarkers(drawer, markers);
}

// 库里的过程是追加型的:轮询时只画新增的那几条,不整段重画(整段重画会把人
// 刚点开的思考/工具卡又收回去)。
function appendMarkers(drawer, markers) {
  const list = markers || [];
  if (list.length < (drawer.rendered || 0)) {
    newSink(drawer);
    drawer.rendered = 0;
  }
  drawer.brief = briefFromMarkers(list) || drawer.brief;
  for (let index = drawer.rendered || 0; index < list.length; index += 1) {
    renderSubagentProgress(drawer.sink, String(list[index]));
  }
  drawer.rendered = list.length;
  drawer.ui.traceEmpty.hidden = drawer.ui.blocks.childElementCount > 0;
  drawer.ui.traceEmpty.textContent = "没有记录到过程。";
}

async function refresh(drawer, { withTrace }) {
  if (!drawer.auditId) return;
  let data;
  try {
    data = await (await apiRequest(`/api/subagents/${encodeURIComponent(drawer.auditId)}`)).json();
  } catch (error) {
    if (current !== drawer) return;
    if (!drawer.data) {
      drawer.ui.traceEmpty.hidden = false;
      drawer.ui.traceEmpty.textContent = error?.status === 404 ? "找不到这次子代理的记录(可能已超过 7 天保留期)。" : "读取失败。";
    }
    return;
  }
  if (current !== drawer) return;
  drawer.data = data;
  if (withTrace) appendMarkers(drawer, data.trace);
  if (data.status !== "running") {
    // 实时那边也跑完了:收尾最后一块思考/正文,停轮询。
    drawer.liveRunning = false;
    subEndReasoning(drawer.sink);
    subEndContent(drawer.sink);
    if (drawer.timer) {
      clearInterval(drawer.timer);
      drawer.timer = null;
    }
  }
  renderMeta(drawer);
  renderPrompt(drawer);
  renderResult(drawer);
}

// 实时旁路与 Esc:第一次打开时才挂。不放模块顶层——这个模块和 subagent.js、
// jobs.js 之间有环,顶层就去碰 subagent.js 的变量可能撞上它还没初始化。
let wired = false;
function wireOnce() {
  if (wired) return;
  wired = true;
  // 页面上任何子代理 sink 收到标记,属于当前抽屉那一趟的就转一份进来。
  setSubagentLiveTap((sink, message) => {
    const drawer = current;
    if (!drawer || !drawer.liveRunning || sink === drawer.sink || sink.drawer) return;
    if (!sink.auditId || sink.auditId !== drawer.auditId) return;
    renderSubagentProgress(drawer.sink, message);
    drawer.ui.traceEmpty.hidden = drawer.ui.blocks.childElementCount > 0;
    renderMeta(drawer);
  });
  // 捕获阶段先于全局快捷键:抽屉开着时 Esc 只关抽屉。
  window.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || !current) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    closeDrawer();
  }, true);
}

/// 打开详情抽屉。`sink`:页面上这趟子代理的实时 sink(可无);`running`:它是否还在跑。
export function openSubagentDrawer({ auditId, sink = null, title = "", running = false }) {
  const id = String(auditId || sink?.auditId || "").trim();
  if (!id) return;
  wireOnce();
  if (current?.auditId === id) {
    current.ui.panel.focus({ preventScroll: true });
    return;
  }
  if (current) {
    const old = current;
    current = null;
    if (old.timer) clearInterval(old.timer);
    if (old.ticker) clearInterval(old.ticker);
    old.root.remove();
  }
  // 从主对话那张卡(已返回 job_id)打开的后台子代理:任务条上还有它的实时 sink,
  // 借那份,别退回轮询库。
  if (!running) {
    for (const [jobId, jobSink] of state.jobStreamSinks || []) {
      if (jobSink?.auditId === id && state.backgroundJobs.get(jobId)?.running) {
        sink = jobSink;
        running = true;
        break;
      }
    }
  }
  const ui = buildShell(title);
  const drawer = { auditId: id, title, ui, root: ui.root, data: null, brief: null, sink: null, liveRunning: Boolean(running && sink), timer: null, ticker: null, returnFocus: document.activeElement };
  current = drawer;
  newSink(drawer);
  document.body.appendChild(ui.root);
  document.body.classList.add("subagent-drawer-open");
  // 下一帧再加 is-open,滑入过渡才会发生。
  window.requestAnimationFrame(() => ui.root.classList.add("is-open"));
  ui.panel.focus({ preventScroll: true });

  if (drawer.liveRunning) {
    replay(drawer, sink.markers || []);
    renderMeta(drawer);
    renderPrompt(drawer);
    refresh(drawer, { withTrace: false });
    drawer.timer = setInterval(() => refresh(drawer, { withTrace: false }), POLL_MS);
    // 读秒:抬头的耗时跟着走。
    drawer.ticker = setInterval(() => {
      if (current === drawer && drawer.liveRunning && !document.hidden) renderMeta(drawer);
    }, 1000);
  } else {
    refresh(drawer, { withTrace: true }).then(() => {
      if (current === drawer && drawer.data?.status === "running") {
        // 页面上没有它的实时 sink(刷新后的前台子代理):只能轮询库里的过程。
        drawer.timer = setInterval(() => refresh(drawer, { withTrace: true }), POLL_MS);
      }
    });
  }
}

/// 卡片 / 任务行上的「详情」按钮。`target()` 在点击时取当下的参数(sink 状态会变)。
export function makeSubagentDetailButton(target, className = "") {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `subagent-detail-button ${className}`.trim();
  button.title = "查看子代理详情";
  button.setAttribute("aria-label", "查看子代理详情");
  button.appendChild(makeIconSlot("panel-right"));
  button.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    openSubagentDrawer(target());
  });
  return button;
}
