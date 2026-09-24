import { makeIconSlot } from "../../core/icons.js";
import { attachSubBrief, procLineAttach, procLineBreak } from "./proc-rail.js";
import { setReasoningPeek } from "./reasoning.js";
import { createReasoningBlock } from "./render.js";
import { formatJobDuration } from "../jobs.js";
import { refreshComposerCumulative, tokensFromCount } from "../live/run.js";
import { renderMarkdown } from "../markdown/render.js";
import { createPersistedToolCard } from "../tools/cards.js";
import { toolSubject } from "../tools/format.js";
import { state } from "../../state/store.js";

// ── 子代理进度:把中转来的标记流解析成结构化事件 ───────────────────
// 子代理内部的思考/工具活动经父回合的 tool.progress 通道以标记串上来。
// Summary 档只有纯文本行(用于标题窥视);Full 档带 __subtool_call__ /
// __subtool_result__ / __subagent_reasoning__(用于展开后的子过程时间线)。
export const SUBAGENT_MARKERS = {
  reasoning: "__subagent_reasoning__",
  content: "__subagent_content__",
  call: "__subtool_call__",
  result: "__subtool_result__",
  stats: "__subagent_stats__",
  detach: "__subagent_detach__",
  brief: "__subagent_brief__"
};

// 子代理任务简介 DOM(展开区最上方):标题 + 整段 prompt。前台从工具参数直接建;
// 后台经 __subagent_brief__ marker 建(后台事件流里没有参数,09-12 #9)。
export function buildSubagentBrief(title, prompt) {
  const t = String(title || "").trim();
  const p = String(prompt || "").trim();
  if (!t && !p) return null;
  // prompt 做成默认收起的可展开 tag:子代理自动展开活区域时整段 prompt 会刷屏,
  // 收成一行「任务标题」,想看再点开(用户反馈)。
  const brief = document.createElement("details");
  brief.className = "subagent-brief";
  const summary = document.createElement("summary");
  summary.className = "subagent-brief-title";
  // 节点:和时间线其它步同一列、坐在细线上(它是时间线的开头,不再是分离的一块)。
  // 平时显 📋,鼠标悬浮时原地换成展开箭头(不在右侧另起一个,用户要求)。
  const marker = document.createElement("span");
  marker.className = "subagent-brief-marker";
  marker.append(
    makeIconSlot("clipboard", "subagent-brief-icon"),
    makeIconSlot("chevron-right", "subagent-brief-chevron"),
  );
  const label = document.createElement("span");
  label.className = "subagent-brief-name";
  label.textContent = t || "任务 prompt";
  summary.append(marker, label);
  brief.appendChild(summary);
  if (p) {
    const body = document.createElement("div");
    body.className = "subagent-brief-prompt";
    body.textContent = p;
    brief.appendChild(body);
  }
  return brief;
}

export function parseSubagentEvent(message) {
  const text = String(message || "");
  if (text.startsWith(SUBAGENT_MARKERS.reasoning)) {
    // 不要 trim:逐 token 的 reasoning delta 前后的空格是词间空格,trim 掉就成了
    // 「Actuallythetails」这种连成一坨(09-12 #8 思考内容没空格没换行的真因)。
    return { kind: "reasoning", text: text.slice(SUBAGENT_MARKERS.reasoning.length) };
  }
  if (text.startsWith(SUBAGENT_MARKERS.content)) {
    // 同 reasoning:不 trim,逐 token 的正文 delta 词间空格要留住。
    return { kind: "content", text: text.slice(SUBAGENT_MARKERS.content.length) };
  }
  if (text.startsWith(SUBAGENT_MARKERS.call)) {
    try {
      const payload = JSON.parse(text.slice(SUBAGENT_MARKERS.call.length));
      const args = typeof payload.args === "string" ? payload.args : JSON.stringify(payload.args ?? {});
      return { kind: "call", name: String(payload.name || ""), display: String(payload.display || ""), args, subject: toolSubject(payload.name, args) };
    } catch {
      return { kind: "plain", text: text.slice(SUBAGENT_MARKERS.call.length).trim() };
    }
  }
  if (text.startsWith(SUBAGENT_MARKERS.result)) {
    try {
      const payload = JSON.parse(text.slice(SUBAGENT_MARKERS.result.length));
      const args = typeof payload.args === "string" ? payload.args : JSON.stringify(payload.args ?? {});
      return { kind: "result", name: String(payload.name || ""), display: String(payload.display || ""), args, ok: payload.ok !== false, output: String(payload.output ?? "") };
    } catch {
      return { kind: "plain", text: text.slice(SUBAGENT_MARKERS.result.length).trim() };
    }
  }
  if (text.startsWith(SUBAGENT_MARKERS.stats)) return { kind: "stats", text: text.slice(SUBAGENT_MARKERS.stats.length).trim() };
  if (text.startsWith(SUBAGENT_MARKERS.brief)) {
    try {
      const p = JSON.parse(text.slice(SUBAGENT_MARKERS.brief.length));
      return { kind: "brief", description: String(p.description || ""), prompt: String(p.prompt || "") };
    } catch {
      return { kind: "plain", text: "" };
    }
  }
  if (text.startsWith(SUBAGENT_MARKERS.detach)) return { kind: "plain", text: text.slice(SUBAGENT_MARKERS.detach.length).trim() };
  return { kind: "plain", text: text.trim() };
}

export function subagentPeekLine(ev) {
  if (ev.kind === "reasoning") return ev.text;
  if (ev.kind === "content") return ev.text;
  if (ev.kind === "call") return `调用 ${ev.name}${ev.subject ? " · " + ev.subject : ""}`;
  if (ev.kind === "result") return `${ev.name} ${ev.ok ? "完成" : "出错"}`;
  return ev.text || "";
}

// 子过程时间线:子代理自己的思考与工具流,复用主对话同一套渲染——proc-line
// 细线时间线 + createReasoningBlock(思考:累加、可展开、有窥视、动画)+
// createPersistedToolCard(完成的工具卡,与主流工具卡同构)。sink.blocks 承载
// proc-line;sink.think 是当前正累加的思考块。
export function subEndReasoning(sink) {
  if (sink.think) {
    // 冲掉未触发的 rAF,把最终全文渲一遍,再释放帧句柄给下一个思考块。
    if (sink.thinkFrame) {
      window.cancelAnimationFrame(sink.thinkFrame);
      sink.thinkFrame = null;
    }
    const finalText = sink.think.__acc != null ? sink.think.__acc : sink.thinkAccum;
    sink.think.body.textContent = finalText || "";
    setReasoningPeek(sink.think.peek, finalText || "");
    sink.think.title.textContent = "已思考";
    sink.think.element.classList.remove("is-live");
    // 冻结读秒(09-12 #4:子过程思考读秒一直停在 0s)。startedAt 是创建时的
    // performance.now();收尾时算出最终耗时定格,ticker 靠 is-live 判活,收尾即停。
    const ls = sink.think.liveStatus;
    if (ls && sink.think.startedAt != null) {
      ls.textContent = `${((performance.now() - sink.think.startedAt) / 1000).toFixed(1)}s`;
    }
    sink.think = null;
    sink.thinkAccum = "";
  }
}

// 子代理正文段收尾:把当前正在累加的正文块定格(内容留在时间线里),
// 下一段正文会另起一块,中间穿插思考/工具卡——和主对话的交错渲染同构。
export function subEndContent(sink) {
  if (sink.contentBlock) {
    // 收尾时把最终全文渲一遍(可能有帧还没触发),再释放帧句柄,让下一段正文能重新调度。
    if (sink.contentFrame) {
      window.cancelAnimationFrame(sink.contentFrame);
      sink.contentFrame = null;
    }
    renderMarkdown(sink.contentBlock, sink.contentBlock.__subAcc || sink.contentAccum || "");
    sink.contentBlock = null;
    sink.contentAccum = "";
  }
}

// 子过程时间线增长时自动滚到底(09-12 #7:展开后 timeline 继续长不自动滚)。
// 滚的是最近的可滚容器(前台=.sub-blocks 本身,后台=外层 .job-stream-panel);
// 只有用户本来就贴着底才跟随,往上翻了就不抢。
export function subScrollContainer(sink) {
  const el = sink && sink.blocks;
  if (!el) return null;
  let c = el;
  while (c && c !== document.body) {
    const style = window.getComputedStyle(c);
    if (/(auto|scroll)/.test(style.overflowY) && c.scrollHeight > c.clientHeight + 1) break;
    c = c.parentElement;
  }
  if (!c || c === document.body) c = el;
  return c;
}

// 贴底跟随:先量「改内容之前是不是贴着底」,mutate 完只在原本贴底时才拉回底。
// 不靠区分程序/用户滚动(那套 pinnedUp 会被程序自己的归位清掉、导致往上翻又被拽回 #139),
// 就一条:你原本在底我才跟,你往上翻了(改前就不在底)我一步都不动。
export function subStickBottom(sink, mutate) {
  const c = subScrollContainer(sink);
  const atBottom = c ? (c.scrollHeight - c.scrollTop - c.clientHeight) < 30 : false;
  mutate();
  if (c && atBottom) c.scrollTop = c.scrollHeight;
}

export function subAutoScroll(sink) {
  // 内容已经加完了才调它(工具卡/结果那种低频路径):当前离底 <30 就跟,否则不动。
  const c = subScrollContainer(sink);
  if (!c) return;
  if (c.scrollHeight - c.scrollTop - c.clientHeight < 30) c.scrollTop = c.scrollHeight;
}

// 往子过程区加一个块(思考块头/工具卡)必须走「加之前先量在不在底,加完只在原本
// 贴底时才拉回底」——直接 procLineAttach 会把容器撑高却不滚,一次没滚就把整条贴底
// 跟随链打断,之后逐 token 的 subStickBottom 全测得「改前不在底」再不跟(#159/#160,
// 前台后台同此)。subAutoScroll 是「加完再量」,块一高就已经离底 >30px 也修不回来。
export function subAttach(sink, el) {
  subStickBottom(sink, () => procLineAttach(sink.blocks, el));
}

export function renderSubagentProgress(sink, message) {
  const ev = parseSubagentEvent(message);
  if (ev.kind === "stats") {
    // stats 文本形如「工具调用 3 次　消耗词元 ≈1.2k」/「tool calls: 3　token cost: 1.2k」,
    // 每步更新一次。抠出 token 数(可能带 ≈ 前缀),喂给任务条那行的 token 显示(09-12 item 4)。
    const m = ev.text.match(/(?:词元|cost)\s*[：:]?\s*(≈?\s*[\d.]+\s*[kKmMbB万]?)/);
    if (m) {
      sink.tokenText = m[1].replace(/\s+/g, "");
      if (sink.taskToken) sink.taskToken.textContent = sink.tokenText;
      // 前台工具卡 / 后台任务条(job)都从这里过:把这次子代理的实时 token 估算按
      // usageKey 汇进输入框那个「累计」(#131,后台子代理同样接上)。回放/播种时不接
      // (那是历史,会和后端基线重复计)。
      const key = sink.usageKey || (sink.id != null ? sink.id : null);
      const n = tokensFromCount(sink.tokenText);
      if (key != null && n != null && !state.seedingLive) {
        // 保留已有的 done/baseAtDone:子代理收尾还可能再来一条 stats,别把完成态覆盖没了。
        const prev = state.liveSubagentTokens.get(key);
        state.liveSubagentTokens.set(key, { tokens: n, done: prev?.done || false, baseAtDone: prev?.baseAtDone });
        refreshComposerCumulative();
      }
    }
    return;
  }
  if (!sink.blocks) return;
  if (ev.kind === "brief") {
    // 后台子代理展开区顶部补任务简介(09-12 #9;前台已在 createTool 里建好并置
    // sink.brief,不会重复)。插在子过程时间线容器之前。
    if (!sink.brief) {
      const brief = buildSubagentBrief(ev.description, ev.prompt);
      if (brief) {
        attachSubBrief(sink.blocks, brief);
        sink.brief = true;
      }
    }
    return;
  }
  if (ev.kind === "content") {
    // 空 delta 直接丢:否则会造一个空正文块并 procLineBreak 切断时间线(「串」的根)。
    if (!ev.text) return;
    // 子代理正文逐 token 增量(#6:光有 timeline,正文没流出来)。先收思考,再把
    // 正文累加到一个活的正文块;新起一段正文时切断当前时间线,正文落在段间,
    // 之后的工具/思考会另起一条 proc-line——和主对话交错渲染同构。
    if (!sink.contentBlock) {
      subStickBottom(sink, () => {
        subEndReasoning(sink);
        procLineBreak(sink.blocks);
        const div = document.createElement("div");
        div.className = "sub-content markdown-body";
        sink.blocks.appendChild(div);
        sink.contentBlock = div;
        sink.contentAccum = "";
      });
    }
    sink.contentAccum += ev.text;
    // markdown 渲染按 rAF 合并:逐 token 全量重解析太费,一帧渲一次就够顺。
    // 累加文本挂在块元素上,帧触发时读它当前值(而非调度那刻的旧值),避免同一
    // 帧内后到的 token 被丢。
    const block = sink.contentBlock;
    block.__subAcc = sink.contentAccum;
    if (!sink.contentFrame) {
      sink.contentFrame = window.requestAnimationFrame(() => {
        sink.contentFrame = null;
        subStickBottom(sink, () => renderMarkdown(block, block.__subAcc || ""));
      });
    }
    sink.peekLine = sink.contentAccum;
    if (sink.taskPeek) setReasoningPeek(sink.taskPeek, sink.contentAccum);
    return;
  }
  if (ev.kind === "reasoning") {
    // 空 delta 直接丢:否则会造一个空思考块(「串」尤其是思考的根)。
    if (!ev.text) return;
    // 思考逐 token 增量,累加到一个活的思考块(不能覆盖,否则只剩最后一个 token)。
    subEndContent(sink);
    if (!sink.think) {
      sink.think = createReasoningBlock("", "正在思考", true);
      sink.thinkAccum = "";
      // 读秒 ticker 靠这个起点更新(见 subEndReasoning 上方的 setInterval)。
      if (sink.think.liveStatus && sink.think.startedAt != null) {
        sink.think.liveStatus.dataset.subStart = String(sink.think.startedAt);
      }
      subAttach(sink, sink.think.element);
    }
    sink.thinkAccum += ev.text;
    sink.think.raw = sink.thinkAccum;
    // 正文体逐 token 全量重写 textContent,展开态下每个 token 都重排,长思考会卡死
    // (#114:打开正在思考的行特别卡)。按 rAF 合并:一帧只写一次当前全文。
    const think = sink.think;
    think.__acc = sink.thinkAccum;
    if (!sink.thinkFrame) {
      sink.thinkFrame = window.requestAnimationFrame(() => {
        sink.thinkFrame = null;
        subStickBottom(sink, () => {
          think.body.textContent = think.__acc || "";
          setReasoningPeek(think.peek, think.__acc || "");
          // 行窥视也合进这一帧:每 token 各测一次 scrollWidth 会引发同步重排,连带把
          // 已完成的「已思考」行窥视一起抖(#3 疯狂抖动)。一帧只测一次。
          if (sink.taskPeek) setReasoningPeek(sink.taskPeek, think.__acc || "");
        });
      });
    }
    sink.peekLine = sink.thinkAccum;
    return;
  }
  if (ev.kind === "call") {
    subEndReasoning(sink);
    subEndContent(sink);
    // Full 档:call 先记着,result 到了再落一张完成卡(带 args + output)。
    sink.pendingCall = { name: ev.name, display: ev.display, args: ev.args, subject: ev.subject };
    // 窥视也用友好显示名(#7:展开是「运行命令」,窥视却还是裸的 run_command)。
    const callLabel = ev.display || ev.name;
    sink.peekLine = ev.subject ? callLabel + " · " + ev.subject : "调用 " + callLabel;
    if (sink.taskPeek) setReasoningPeek(sink.taskPeek, sink.peekLine);
    return;
  }
  if (ev.kind === "result") {
    subEndReasoning(sink);
    subEndContent(sink);
    const call = sink.pendingCall || { name: ev.name, display: ev.display, args: ev.args };
    sink.pendingCall = null;
    const card = createPersistedToolCard({ name: call.name, display_name: call.display, arguments: call.args != null ? call.args : ev.args, output: ev.output, ok: ev.ok });
    subAttach(sink, card);
    sink.peekLine = (call.display || call.name) + " " + (ev.ok ? "完成" : "出错");
    if (sink.taskPeek) setReasoningPeek(sink.taskPeek, sink.peekLine);
    return;
  }
  if (ev.kind === "plain" && ev.text) {
    // Summary 档没有结构化标记(WebUI 回合强制 Full,一般走不到这):只有
    // `工具 #N：名字 · 主语 运行中/ok/err`。运行中不落卡(没 args/output),
    // ok/err 时落一张完成卡。
    const match = ev.text.match(/^(?:工具|tool)\s*#(\d+)[:：]?\s*(.*)$/i);
    if (!match) {
      sink.peekLine = ev.text;
      if (sink.taskPeek) setReasoningPeek(sink.taskPeek, ev.text);
      return;
    }
    const rest = match[2].trim();
    const running = /(?:运行中|running)$/i.test(rest);
    const errored = /(?:\berr\b|错误|失败)$/i.test(rest);
    const finished = !running && /(?:\bok\b|\berr\b|完成|失败|错误)$/i.test(rest);
    const label = rest.replace(/\s*(?:运行中|running|ok|err)$/i, "").trim();
    sink.peekLine = label || rest;
    if (sink.taskPeek) setReasoningPeek(sink.taskPeek, sink.peekLine);
    if (finished) {
      subEndReasoning(sink);
      const at = label.indexOf(" · ");
      const nm = at >= 0 ? label.slice(0, at) : label;
      const subj = at >= 0 ? label.slice(at + 3) : "";
      const card = createPersistedToolCard({ name: nm, arguments: subj, output: "", ok: !errored });
      subAttach(sink, card);
    }
  }
}

/// 原 app.js 顶层的副作用语句，由入口在启动时按原顺序调用。
export function start() {
  // 子过程时间线里「正在思考」的读秒 ticker:主对话那份有各自的 live 计时器,
  // 子过程这份没有,所以读秒永远停在 0s。这个全局 ticker 按 is-live 更新所有
  // 子过程思考块的读秒(用 dataset.subStart 存的起点)。
  setInterval(() => {
    if (document.hidden) return;
    const nodes = document.querySelectorAll(".sub-blocks .reasoning-block.is-live .reasoning-live-status[data-sub-start]");
    for (const ls of nodes) {
      const start = Number(ls.dataset.subStart);
      if (!Number.isFinite(start)) continue;
      ls.textContent = `${Math.max(0, Math.floor((performance.now() - start) / 1000))}s`;
    }
    // 前台子代理行的读秒(09-12 #6):跑着时逐秒走,卡片进入成功/失败即定格。
    for (const el of document.querySelectorAll(".tool-card.is-task .tool-task-seconds[data-task-start]")) {
      const start = Number(el.dataset.taskStart);
      if (!Number.isFinite(start)) continue;
      const card = el.closest(".tool-card");
      const done = card && (card.classList.contains("is-success") || card.classList.contains("is-failure"));
      const secs = (performance.now() - start) / 1000;
      if (done) {
        el.textContent = formatJobDuration(secs);
        delete el.dataset.taskStart;
      } else {
        el.textContent = formatJobDuration(secs);
      }
    }
  }, 1000);
}
