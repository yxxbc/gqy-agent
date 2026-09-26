import { apiRequest } from "../core/api.js";
import { BRAILLE_FRAMES } from "../core/constants.js";
import { makeIconSlot } from "../core/icons.js";
import { showToast } from "../core/toast.js";
import { conversationRunning } from "./conversation/chrome.js";
import { setReasoningPeek } from "./conversation/reasoning.js";
import { jobStreamSink } from "./conversation/render.js";
import { updateJumpButtonOffset } from "./conversation/scroll.js";
import { renderSubagentProgress } from "./conversation/subagent.js";
import { connectEventSource } from "./live/sse.js";
import { loadSessionView } from "./sessions/view.js";
import { elements } from "../state/elements.js";
import { state } from "../state/store.js";

/// 只有本模块用的状态（从 state/store.js 分出来的私有分片）。
const jobsState = {
  jobsStripOpen: localStorage.getItem("gqy.web.jobsStripOpen") === "1",
  commandLogs: new Map(),
  commandPeekLine: new Map(),
  commandPeekTimers: new Map()
};

export function jobStatusDisplay(status) {
  const value = String(status || "");
  if (value === "stopped") return "已中断";
  if (value === "timed_out") return "已超时";
  if (value === "exited(signal)") return "异常退出";
  if (value === "exited(0)") return "完成";
  const match = value.match(/^exited\((-?\d+)\)$/);
  return match ? `退出码 ${match[1]}` : value;
}

export function visibleBackgroundJobs() {
  // 会话隔离: 状态条只显示当前查看会话的任务(无会话标记的旧任务保持可见)。
  return Array.from(state.backgroundJobs.values()).filter(
    (job) => !job.session_id || !state.viewSessionId || job.session_id === state.viewSessionId
  );
}

// 盲文点阵转圈 spinner(09-12 用户指定):一个全局 ticker 刷所有 .job-braille
// 的字符,避免每行各自 CSS 动画在任务条重建时被打回起点。
// 空心盲文点阵转圈(和会话列表 BRAILLE_FRAMES 同款),不是之前那组实心的
// ⣾⣽⣻…(09-12 #2 用户指出实心不对)。
export const JOB_BRAILLE = BRAILLE_FRAMES;

export let jobBrailleFrame = 0;

export function makeJobSpinner() {
  // 左侧标记槽:默认点阵 spinner,鼠标悬浮时原地换成展开/收起箭头
  //(09-12 #8b 用户要求,和子代理一样)。展开态箭头旋转 180°。
  const slot = document.createElement("span");
  slot.className = "job-chip-marker-slot";
  const s = document.createElement("span");
  s.className = "job-chip-marker job-braille";
  s.textContent = JOB_BRAILLE[jobBrailleFrame];
  slot.appendChild(s);
  slot.appendChild(makeIconSlot("chevron-down", "job-chip-chevron"));
  return slot;
}

// 后台命令没有实时进度流,展开那行时拉日志尾巴看输出(09-12 用户报「命令无法
// 点击展开看输出」);运行中每 1.5s 轮询一次,退出即停。
export function commandLogPanel(jobId) {
  let entry = jobsState.commandLogs.get(jobId);
  if (!entry) {
    const panel = document.createElement("div");
    panel.className = "job-stream-panel job-log-panel";
    const pre = document.createElement("pre");
    pre.className = "job-log-pre";
    pre.textContent = "…";
    panel.appendChild(pre);
    entry = { panel, pre, timer: null };
    jobsState.commandLogs.set(jobId, entry);
  }
  return entry;
}

export async function refreshCommandLog(jobId) {
  const entry = jobsState.commandLogs.get(jobId);
  if (!entry) return;
  try {
    // apiRequest 返回的是 Response,得再 .json()(09-12 #8a 命令永远「暂无输出」
    // 的真凶:直接把 Response 当 JSON 用,data.log 恒为 undefined)。
    const resp = await apiRequest(`/api/jobs/${encodeURIComponent(jobId)}/log`);
    const data = await resp.json();
    const atBottom = entry.pre.scrollTop + entry.pre.clientHeight >= entry.pre.scrollHeight - 8;
    entry.pre.textContent = data?.log || "(暂无输出)";
    if (atBottom) entry.pre.scrollTop = entry.pre.scrollHeight;
    if (!data?.running && entry.timer) {
      clearInterval(entry.timer);
      entry.timer = null;
    }
  } catch {
    entry.pre.textContent = "(读取日志失败)";
  }
}

// 后台命令的窥视(#120):轮询日志尾行,取最后一条非空行喂给状态行窥视。命令没有
// 进度流,但输出全在日志里,尾行就是「它现在在干嘛」。行会随任务条重建而换元素,
// 所以 timer 里每次都从当前 DOM 找回该 job 的窥视 span。
export function trackCommandPeek(jobId) {
  if (jobsState.commandPeekTimers.has(jobId)) return;
  const tick = async () => {
    const job = state.backgroundJobs.get(jobId);
    const running = job && job.running;
    try {
      const resp = await apiRequest(`/api/jobs/${encodeURIComponent(jobId)}/log`);
      const data = await resp.json();
      const lines = String(data?.log || "").split("\n").map((l) => l.trimEnd()).filter(Boolean);
      const last = lines.length ? lines[lines.length - 1] : "";
      if (last) {
        jobsState.commandPeekLine.set(jobId, last);
        const el = elements.jobsStrip?.querySelector(`.job-chip[data-job-id="${CSS.escape(jobId)}"] .job-chip-peek > span`);
        if (el) setReasoningPeek(el, last);
      }
      if (data?.running === false) stop();
    } catch { /* 忽略,下次再试 */ }
    if (!running) stop();
  };
  const stop = () => {
    const t = jobsState.commandPeekTimers.get(jobId);
    if (t) clearInterval(t);
    jobsState.commandPeekTimers.delete(jobId);
  };
  tick();
  jobsState.commandPeekTimers.set(jobId, setInterval(tick, 1500));
}

export function renderJobsStrip() {
  const strip = elements.jobsStrip;
  if (!strip) return;
  const jobs = visibleBackgroundJobs();
  // 并行任务数首次达到收缩阈值(≥3)时自动收起成「后台任务 ×N」一行(#11):
  // 从 <3 跨到 ≥3 的那一刻强制收起(刷新时 prev=0 也算跨越),之后用户手动展开保留。
  const prevJobCount = state.prevJobCount || 0;
  state.prevJobCount = jobs.length;
  if (jobs.length >= 3 && prevJobCount < 3) jobsState.jobsStripOpen = false;
  if (!jobs.length) {
    strip.hidden = true;
    strip.replaceChildren();
    updateJumpButtonOffset();
    return;
  }
  const fragment = document.createDocumentFragment();
  const collapsible = jobs.length >= 3;
  if (collapsible) {
    // 合并行做成和单行一样的 job-chip 外观(09-12 用户报):braille spinner +
    // 「后台任务 ×N」+ 展开箭头,不再是另一种带 ▸ 前缀的按钮。
    const toggle = document.createElement("div");
    toggle.className = jobsState.jobsStripOpen ? "job-chip is-toggle is-open" : "job-chip is-toggle";
    toggle.setAttribute("role", "button");
    toggle.setAttribute("aria-expanded", String(jobsState.jobsStripOpen));
    const label = document.createElement("span");
    label.className = "job-chip-label";
    label.textContent = `后台任务 ×${jobs.length}`;
    toggle.append(makeJobSpinner(), label);
    toggle.addEventListener("click", () => {
      jobsState.jobsStripOpen = !jobsState.jobsStripOpen;
      // 收起「后台任务 ×N」合并行时,把里面所有已展开的状态行 + 思考/工具卡
      // 一并收起(09-12 #15),不留展开残留。
      if (!jobsState.jobsStripOpen) {
        state.expandedJobs.clear();
        for (const sink of state.jobStreamSinks.values()) {
          sink.panel?.querySelectorAll("details[open]").forEach((d) => { d.open = false; });
        }
      }
      localStorage.setItem("gqy.web.jobsStripOpen", jobsState.jobsStripOpen ? "1" : "0");
      renderJobsStrip();
    });
    fragment.appendChild(toggle);
  }
  const showRows = !collapsible || jobsState.jobsStripOpen;
  for (const job of showRows ? jobs : []) {
    const jid = String(job.job_id);
    const isSubagent = job.kind === "subagent";
    const row = document.createElement("div");
    row.className = "job-chip is-expandable";
    row.dataset.jobId = jid;

    const label = document.createElement("span");
    label.className = "job-chip-label";
    const kindWord = isSubagent ? (job.dev ? "开发中" : "子代理") : "命令";
    label.textContent = `${kindWord} ${job.job_id} · ${job.title}`;
    label.title = label.textContent;

    // 行窥视:跑到工具显示工具、跑到思考窥思考,单行滚动刷新(仅子代理有进度流,
    // 命令没有进度流所以窥视留空)。标题保持完整、不被窥视替换。
    const peekSlot = document.createElement("span");
    peekSlot.className = "job-chip-peek reasoning-peek";
    const peek = document.createElement("span");
    peekSlot.appendChild(peek);

    const token = document.createElement("span");
    token.className = "job-chip-token";

    const time = document.createElement("span");
    time.className = "job-chip-time";
    const seconds = job.running
      ? Math.max(0, Math.round(job.runtime_seconds + (Date.now() - job.receivedAt) / 1000))
      : job.runtime_seconds;
    time.textContent = formatJobDuration(seconds);

    const stop = document.createElement("button");
    stop.type = "button";
    stop.className = "job-chip-stop";
    stop.textContent = "✕";
    stop.title = "停止该后台任务";
    stop.addEventListener("click", async (event) => {
      event.stopPropagation();
      try {
        await apiRequest(`/api/jobs/${encodeURIComponent(jid)}`, { method: "DELETE" });
      } catch (error) {
        showToast(error.message || "停止失败", "error");
      }
    });

    // 布局(09-12 #2):节点 · 标题 · token 秒数 · <淡出过渡> 窥视(撑开右对齐) · ✕。
    // 标题贴左 hug、token/时间紧跟其后,窥视占满余下空间、左侧淡出滚动,不再让标题
    // flex 撑开把窥视顶到最右留下大空档(#12)。展开箭头合进左侧标记槽。
    row.append(makeJobSpinner(), label, token, time, peekSlot, stop);

    if (isSubagent) {
      const sink = jobStreamSink(jid);
      sink.taskPeek = peek;
      sink.taskToken = token;
      if (sink.peekLine) setReasoningPeek(peek, sink.peekLine);
      if (sink.tokenText) token.textContent = sink.tokenText;
    } else {
      // 后台命令没有进度流,但有输出日志(#120):把日志尾行当窥视,轮询刷新;
      // 先用已缓存的尾行填上(重建行时不闪)。
      if (jobsState.commandPeekLine?.has(jid)) setReasoningPeek(peek, jobsState.commandPeekLine.get(jid));
      if (job.running) trackCommandPeek(jid, peek);
    }

    const expanded = state.expandedJobs.has(jid);
    row.classList.toggle("is-open", expanded);
    row.setAttribute("aria-expanded", String(expanded));
    row.addEventListener("click", (event) => {
      if (event.target.closest(".job-chip-stop")) return;
      if (state.expandedJobs.has(jid)) {
        state.expandedJobs.delete(jid);
        // 收起状态行时,把里面已展开的思考/工具卡也一并收起(09-12 #5),
        // 下次展开是收起态,而不是保留上次的展开。
        const sink = state.jobStreamSinks.get(jid);
        if (sink?.panel) {
          sink.panel.querySelectorAll("details[open]").forEach((d) => { d.open = false; });
        }
      } else {
        state.expandedJobs.add(jid);
      }
      renderJobsStrip();
    });

    const wrap = document.createElement("div");
    wrap.className = "job-chip-wrap";
    wrap.appendChild(row);
    if (expanded) {
      if (isSubagent) {
        wrap.appendChild(jobStreamSink(jid).panel);
      } else {
        const entry = commandLogPanel(jid);
        wrap.appendChild(entry.panel);
        refreshCommandLog(jid);
        if (job.running && !entry.timer) {
          entry.timer = setInterval(() => refreshCommandLog(jid), 1500);
        }
      }
    } else if (!isSubagent) {
      const entry = jobsState.commandLogs.get(jid);
      if (entry?.timer) {
        clearInterval(entry.timer);
        entry.timer = null;
      }
    }
    fragment.appendChild(wrap);
  }
  strip.replaceChildren(fragment);
  strip.hidden = false;
  updateJumpButtonOffset();
}

export function formatJobDuration(seconds) {
  const value = Math.max(0, Math.floor(seconds));
  if (value >= 3600) return `${Math.floor(value / 3600)}h ${String(Math.floor((value % 3600) / 60)).padStart(2, "0")}m`;
  if (value >= 60) return `${Math.floor(value / 60)}m ${String(value % 60).padStart(2, "0")}s`;
  return `${value}s`;
}

export async function seedJobsStrip() {
  try {
    // apiRequest 返回 Response,得再 .json()(与 #8a 命令日志同一坑:直接把
    // Response 当 JSON,data.jobs 恒为 undefined → 刷新后一个后台任务都存不进,
    // 状态行整条消失。job.started 只在开跑那一刻发,刷新后不重放,全靠这里补拉)。
    const data = await (await apiRequest("/api/jobs")).json();
    state.backgroundJobs.clear();
    for (const job of data?.jobs || []) {
      const jid = String(job.job_id);
      state.backgroundJobs.set(jid, { ...job, receivedAt: Date.now() });
      // 刷新后子代理展开区是空的(子过程只在内存里,#9)。补拉这个任务到目前为止的
      // 原始标记流回放进它的 sink,展开就能看到之前的思考/工具/正文;之后的实时进度
      // 继续往同一个 sink 追加。每个 sink 只回放一次。
      if (job.kind === "subagent") seedJobTrace(jid);
    }
    renderJobsStrip();
  } catch {
    /* daemon may predate the jobs API */
  }
}

export async function seedJobTrace(jid) {
  const sink = jobStreamSink(jid);
  if (sink.__replayed) return;
  sink.__replayed = true;
  try {
    const data = await (await apiRequest(`/api/jobs/${encodeURIComponent(jid)}/trace`)).json();
    for (const marker of data?.trace || []) renderSubagentProgress(sink, String(marker));
    if ((data?.trace || []).length) renderJobsStrip();
  } catch {
    sink.__replayed = false; /* 拉失败下次再试 */
  }
}

// 回到前台补一刀(09-12 #9:手机切到别的程序再切回,后台期间任务完成了却不刷新;
// #3:刷新/断连回来状态行没了)。手机后台久了系统会掐断 SSE 且不自动重连,所以:
// 连接死了就按 lastEventId 重连、补拉后台任务;当前没有在跑的直播时静默补同步一次
// 会话,追回后台期间错过的完成事件(有直播在跑就不动,免得打断流式重挂)。
export let lastVisibleResync = 0;

/// 原 app.js 顶层的副作用语句，由入口在启动时按原顺序调用。
export function start() {
  setInterval(() => {
    if (document.hidden) return;
    const nodes = elements.jobsStrip?.querySelectorAll(".job-braille");
    if (!nodes || !nodes.length) return;
    jobBrailleFrame = (jobBrailleFrame + 1) % JOB_BRAILLE.length;
    const frame = JOB_BRAILLE[jobBrailleFrame];
    nodes.forEach((node) => {
      node.textContent = frame;
    });
  }, 110);

  setInterval(() => {
    if (document.hidden) return;
    const visible = visibleBackgroundJobs();
    if (!visible.length) return;
    // 只更新计时文本：全量重建会重启 CSS 旋转动画，导致 spinner 每秒瞬移回原点。
    let missing = false;
    for (const job of visible) {
      const row = elements.jobsStrip?.querySelector(`.job-chip[data-job-id="${CSS.escape(String(job.job_id))}"]`);
      if (!row) {
        missing = true;
        continue;
      }
      const time = row.querySelector(".job-chip-time");
      if (!time) continue;
      const seconds = Math.max(0, Math.round(job.runtime_seconds + (Date.now() - job.receivedAt) / 1000));
      time.textContent = formatJobDuration(seconds);
    }
    if (missing && (jobsState.jobsStripOpen || visible.length < 3)) renderJobsStrip();
  }, 1000);

  setTimeout(seedJobsStrip, 800);

  document.addEventListener("visibilitychange", () => {
    if (document.hidden || state.blocked) return;
    const src = state.eventSource;
    const dead = !src || src.readyState === EventSource.CLOSED;
    if (dead) connectEventSource(state.lastEventId || 0);
    seedJobsStrip();
    // 只有 SSE 真的断过(切走太久被系统掐了)才补同步会话:SSE 一直连着就没漏事件,
    // 没必要重建整个对话。长对话整段 loadSessionView 很重,每次切回前台都重建正是
    // 「滚动中切回来渲染丢失/卡死」的诱因(09-12 #13:visibilitychange 无条件重建)。
    if (!dead) return;
    const now = Date.now();
    if (now - lastVisibleResync < 1500) return;
    lastVisibleResync = now;
    if (!conversationRunning() && state.viewSessionId && !state.viewLoading) {
      loadSessionView(state.viewSessionId, { quiet: true });
    }
  });
}
