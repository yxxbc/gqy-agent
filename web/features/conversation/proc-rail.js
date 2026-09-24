import { makeIconSlot } from "../../core/icons.js";
import { formatToolDuration } from "../tools/format.js";
import { state } from "../../state/store.js";

/* ─── 过程时间线 ───
 * 连续的思考块和工具签串成一条时间线(.proc-line):一根 1px 细线穿过图标列的中心,
 * 图标处断开,图标就是节点。正文、媒体、任何不是思考/工具的东西一出现,就把当前
 * 时间线「切断」——后面再来工具就另起一条。
 * 「过程自动收起」开着时,切断那一刻收成一行总结(Worked for 5.4 s · 3 tools);
 * 关着就保持展开,也不出总结行。运行中(还没切断)永远没有总结行。
 * 细线是独立元素,起点和终点跟着可见节点走,ResizeObserver 一触发就重算,
 * 高度交给 CSS transition——新出一行,线就平滑长到那个图标,不是瞬间跳。
 */
export function procLineCreate(isStatic) {
  const line = document.createElement("div");
  line.className = "proc-line is-live is-open";
  if (isStatic) line.classList.add("is-static");
  const rail = document.createElement("i");
  rail.className = "proc-rail";
  rail.setAttribute("aria-hidden", "true");
  const head = document.createElement("button");
  head.type = "button";
  head.className = "proc-head";
  head.hidden = true;
  const node = document.createElement("span");
  node.className = "proc-node";
  node.appendChild(makeIconSlot("chevron-right", "proc-chevron"));
  const summary = document.createElement("span");
  summary.className = "proc-summary";
  head.append(node, summary);
  head.addEventListener("click", () => procLineSetOpen(line, !line.classList.contains("is-open")));
  const wrap = document.createElement("div");
  wrap.className = "proc-wrap";
  const inner = document.createElement("div");
  const steps = document.createElement("div");
  steps.className = "proc-steps";
  inner.appendChild(steps);
  wrap.appendChild(inner);
  line.append(rail, head, wrap);
  line.gqyProc = { rail, head, summary, steps, closed: false, batchStart: -Infinity };
  const fit = () => procLineFit(line);
  if (typeof ResizeObserver === "function") new ResizeObserver(fit).observe(line);
  window.requestAnimationFrame(fit);
  return line;
}

export const PROC_NODE_SELECTOR = ":scope > .tool-head > .tool-icon, :scope > summary > .reasoning-icon, :scope > summary > .subagent-brief-marker, :scope.tool-preparing-tag > .icon-slot";

export function procLineFit(line) {
  const proc = line.gqyProc;
  if (!proc || !line.isConnected) return;
  const nodes = [];
  if (!proc.head.hidden) nodes.push(proc.head.querySelector(".proc-node"));
  if (line.classList.contains("is-open") || proc.folding) {
    for (const step of proc.steps.children) {
      const node = step.querySelector(PROC_NODE_SELECTOR);
      // 隐藏的签(生图签藏着)没有 offsetParent,不算节点
      if (node && node.offsetParent) nodes.push(node);
    }
  }
  if (!nodes.length) {
    proc.rail.style.height = "0px";
    return;
  }
  // app 壳 zoom 1.1 下 getBoundingClientRect 是缩放后的坐标,style 里的 px 是缩放前的,
  // 用容器自己的 rect 宽 / offsetWidth 反推缩放比。
  const box = line.getBoundingClientRect();
  const zoom = line.offsetWidth ? box.width / line.offsetWidth : 1;
  const center = (node) => {
    const rect = node.getBoundingClientRect();
    return (rect.top - box.top + rect.height / 2) / zoom;
  };
  const first = center(nodes[0]);
  let last = center(nodes[nodes.length - 1]);
  // 开合动画进行中:内层在被裁剪,线的终点不能超过当前可见底边,否则内容收完了线还拖在外面
  const clip = proc.steps.parentElement.getBoundingClientRect();
  last = Math.min(last, (clip.bottom - box.top) / zoom);
  proc.rail.style.top = `${first}px`;
  proc.rail.style.height = `${Math.max(0, last - first)}px`;
}

// 展开/收起时间线里某一项(思考块/工具卡)是瞬间的,但 proc-rail 靠 ResizeObserver
// + 0.45s transition 平滑跟随,不同步就抖一下(09-12 #3)。让细线立即贴合、这次不过渡。
// 展开/收起时把细线重贴一次内容(#8/#9):内容用 grid-rows fold 平滑展开(~0.3s),
// 而 .proc-rail 已去掉自己的 transition,靠 proc-line 的 ResizeObserver 在动画每一帧
// 重量节点位置、瞬时跟着内容长/缩。这里再补一次即时 fit 兜底(有些位移不改 proc-line
// 高度、ResizeObserver 不触发),不再跑 360ms rAF 循环(那是长页面卡死/崩溃的隐患)。
export function railSnapFit(el) {
  const line = el?.closest?.(".proc-line");
  if (line?.gqyProc) procLineFit(line);
}

export function procLineSetOpen(line, open) {
  line.classList.toggle("is-open", open);
  const proc = line.gqyProc;
  if (!proc) return;
  proc.head.setAttribute("aria-expanded", String(open));
  // 收起「Worked for」整条时间线时,把里面已展开的思考块/工具卡(含子代理里的)
  // 一并收起(#10),下次展开是干净收起态,不保留上次翻开的。
  if (!open) {
    proc.steps.querySelectorAll("details[open]").forEach((d) => { d.open = false; });
    proc.steps.querySelectorAll(".tool-card:not(.collapsed)").forEach((c) => {
      c.classList.add("collapsed");
      const innerHead = c.querySelector(".tool-head");
      if (innerHead) innerHead.setAttribute("aria-expanded", "false");
    });
  }
  // 开合期间线逐帧跟裁剪边走,不自己再走一遍 transition(两条曲线叠起来就是线拖在内容后面)。
  // 收起时节点仍算数,只是被裁剪边钳住;裁剪到头线也就到头了。
  proc.rail.style.transition = "none";
  proc.folding = true;
  window.clearTimeout(proc.foldTimer);
  proc.foldTimer = window.setTimeout(() => {
    proc.folding = false;
    proc.rail.style.transition = "";
    procLineFit(line);
  }, 420);
  // ResizeObserver 是这一帧布局完才回调,线会慢内容一帧;开合期间每帧自己量一次,
  // 读 rect 会拿到过渡当前值,写回去落在同一帧里。
  const tick = () => {
    if (!proc.folding) return;
    procLineFit(line);
    window.requestAnimationFrame(tick);
  };
  window.requestAnimationFrame(tick);
  procLineFit(line);
}

// 把思考块 / 工具签挂进当前时间线;没有开着的就新起一条
export function procLineAttach(blocks, element, isStatic = false) {
  if (!blocks || !element) return null;
  let line = blocks.lastElementChild;
  if (!line?.classList?.contains("proc-line") || line.gqyProc?.closed) {
    line = procLineCreate(isStatic);
    blocks.appendChild(line);
  }
  if (!isStatic) {
    // 快模型一口气吐几个调用:不压着后来的行等,而是让 250ms 窗口内到的行共用
    // 同一条淡入时间轴(负延迟对齐到窗口起点),几行像一批一起浮起来;窗口过了
    // 再开新一批。动画还是那条曲线,只是不会一行一行各自蹦。
    const proc = line.gqyProc;
    const now = performance.now();
    if (now - proc.batchStart > 250) proc.batchStart = now;
    const offset = now - proc.batchStart;
    if (offset > 0) element.style.animationDelay = `-${Math.round(offset)}ms`;
  }
  line.gqyProc.steps.appendChild(element);
  return line;
}

// 子代理任务简介作为「时间线的开头」插进去(用户:和 timeline 统一,不再是分离的一块)。
// 建一条 proc-line(若无),把 brief 放成第一个 step——它的 .subagent-brief-marker
// 会被 PROC_NODE_SELECTOR 认作节点,细线从它这里起头。
export function attachSubBrief(blocks, brief) {
  if (!blocks || !brief) return;
  let line = blocks.lastElementChild;
  if (!line?.classList?.contains("proc-line") || line.gqyProc?.closed) {
    line = procLineCreate(false);
    blocks.appendChild(line);
  }
  line.gqyProc.steps.insertBefore(brief, line.gqyProc.steps.firstChild);
  procLineFit(line);
}

// 正文/媒体来了:把当前时间线切断
export function procLineBreak(blocks) {
  const line = blocks?.lastElementChild;
  if (!line?.classList?.contains("proc-line") || line.gqyProc?.closed) return;
  const proc = line.gqyProc;
  proc.closed = true;
  line.classList.remove("is-live");
  procLineRefresh(line);
  // 子过程时间线(前台/后台子代理展开区)恒展开,不做 procCollapse 折叠:那块本就是
  // 限高滚动的紧凑区,折成「Thought / N tools」摘要既多余又会冒出一条怪「Thought」
  // 顶在 prompt 上面(#2)。只有主对话的过程区才折。
  if (state.procCollapse && !blocks.classList?.contains("sub-blocks")) {
    proc.head.hidden = false;
    procLineSetOpen(line, false);
  }
}

// 总结行文字:Worked for 5.4 s · 3 tools · 1 thought · 1 err(回看的没有耗时)
export function procLineRefresh(line) {
  const proc = line?.gqyProc;
  if (!proc?.closed) return;
  const tools = proc.steps.querySelectorAll(":scope > .tool-card").length;
  const thoughts = proc.steps.querySelectorAll(":scope > .reasoning-block").length;
  const errs = proc.steps.querySelectorAll(":scope > .tool-card.is-failure").length;
  // 「Worked for」= 第一个工具开跑到最后一个工具跑完。实时用 performance.now,
  // 回看用落库的 Unix 毫秒,差值同一口径,刷新前后数字一致。
  let first = Infinity;
  let last = -Infinity;
  for (const card of proc.steps.querySelectorAll(":scope > .tool-card")) {
    const timing = card.gqyTiming;
    if (!timing || timing.startedAt == null || timing.finishedAt == null) continue;
    first = Math.min(first, timing.startedAt);
    last = Math.max(last, timing.finishedAt);
  }
  const elapsed = Number.isFinite(first) && Number.isFinite(last) ? formatToolDuration(last - first) : "";
  const parts = [];
  const strong = (text) => {
    const b = document.createElement("b");
    b.textContent = text;
    return b;
  };
  const plain = (text, className = "") => {
    const span = document.createElement("span");
    if (className) span.className = className;
    span.textContent = text;
    return span;
  };
  if (tools) {
    const count = `${tools} tool${tools > 1 ? "s" : ""}`;
    if (elapsed) {
      parts.push(strong(`Worked for ${elapsed}`));
      parts.push(plain(count));
    } else {
      parts.push(strong(count));
    }
    if (thoughts) parts.push(plain(`${thoughts} thought${thoughts > 1 ? "s" : ""}`));
    if (errs) parts.push(plain(`${errs} err${errs > 1 ? "s" : ""}`, "proc-err"));
  } else {
    parts.push(strong("Thought"));
  }
  proc.summary.replaceChildren();
  parts.forEach((part, index) => {
    if (index) proc.summary.appendChild(plain(" · ", "proc-dot"));
    proc.summary.appendChild(part);
  });
}
