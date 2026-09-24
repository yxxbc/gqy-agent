import { COMMAND_OUTPUT_PREVIEW_ROWS, MAX_TOOL_OUTPUT_CHARS } from "../../core/constants.js";
import { asFiniteNumber } from "../../core/format.js";
import { makeIconSlot } from "../../core/icons.js";
import { artifactIconName } from "../artifacts/image.js";
import { normalizeArtifact, registerArtifact, safeAssetUrl, setArtifactWorkspaceOpen } from "../artifacts/model.js";
import { artifactChipOptions } from "../artifacts/workspace.js";
import { createConversationMedia } from "../conversation/media.js";
import { attachSubBrief, procLineAttach, procLineBreak, railSnapFit } from "../conversation/proc-rail.js";
import { reasoningPeekText } from "../conversation/reasoning.js";
import { contentAdded } from "../conversation/scroll.js";
import { buildSubagentBrief, renderSubagentProgress, subEndReasoning, subScrollContainer } from "../conversation/subagent.js";
import { attachLiveTodoPanel, renderStageTodos } from "../goal.js";
import { refreshComposerCumulative } from "../live/run.js";
import { breakLiveText, clearTypingIndicator, ensureLiveArticle, syncBubbleWidth } from "../live/state.js";
import { finalizeLiveReasoning } from "../live/stream.js";
import { runSessionId } from "../sessions/runs.js";
import { attachToolRichCards, boundedAppend, createToolDetail, scheduleCommandOutputPreview, scrollToolOutputToEnd, startImageGenDots, stopImageGenDots, toolIconName, updateToolStatus, updateToolSummary } from "./cards.js";
import { compactLine, formatToolDuration, isSubagentTool, parsedToolArguments, prettyArguments, toolSubject } from "./format.js";
import { state } from "../../state/store.js";

export function createTool(live, data, opts = {}) {
  ensureLiveArticle(live);
  clearTypingIndicator(live, { waitingOnly: true });
  breakLiveText(live);
  finalizeLiveReasoning(live);
  live.contextOperation = null;
  const toolId = String(data?.tool_id || `${live.runId}_tool_unknown_${live.tools.size + 1}`);
  if (live.tools.has(toolId)) return live.tools.get(toolId);
  const card = document.createElement("section");
  card.className = state.toolExpanded ? "tool-card" : "tool-card collapsed";
  card.dataset.toolId = toolId;
  const isCommand = ["run_command", "Bash"].includes(String(data?.name || ""));
  if (isCommand) card.classList.add("is-command");
  const isTask =
    isSubagentTool(data?.name) ||
    /^(subagent|task)[:：]/i.test(String(data?.display_name || ""));
  if (isTask) {
    card.classList.add("is-task");
    // 前台子代理:运行时自动展开那块四行活区域,子过程实时流入(用户拍板);
    // 跑完(tool.finished)再收起成一行。head 的 aria-expanded 也置真。
    card.classList.remove("collapsed");
  }
  const subjectText = toolSubject(data?.name, data?.arguments);
  const commandArguments = isCommand ? parsedToolArguments(data?.arguments) : null;
  const commandText = isCommand ? String(commandArguments?.command || commandArguments?.cmd || "").trim() : "";
  const head = document.createElement("button");
  head.className = "tool-head";
  head.type = "button";
  head.setAttribute("aria-expanded", String(Boolean(state.toolExpanded)));
  const icon = document.createElement("span");
  icon.className = "tool-icon";
  const toolName = String(data?.name || "");
  // 生图/打图走 GPT 式点阵占位气泡,芯片隐藏(失败时再露出来给细节)。
  const isImageTool = toolName === "generate_image" || toolName === "print_image";
  if (isImageTool) card.classList.add("image-tool-chip");
  icon.appendChild(makeIconSlot(toolIconName(toolName)));
  const title = document.createElement("span");
  title.className = "tool-title";
  const displayName = document.createElement("strong");
  displayName.textContent = String(data?.display_name || data?.name || "工具");
  // 开发模式子代理显示「开发中」而非「子代理」,和普通子代理区分开(09-11)。
  if (isTask && parsedToolArguments(data?.arguments)?.dev === true) {
    displayName.textContent = "开发中";
  }
  displayName.title = displayName.textContent;
  const realName = document.createElement("small");
  realName.className = "tool-technical-name";
  realName.textContent = String(data?.name || "");
  const summary = document.createElement("small");
  summary.className = "tool-summary";
  title.append(displayName, realName, summary);
  const status = document.createElement("span");
  status.className = "tool-status";
  const statusIcon = makeIconSlot("loader-circle", "is-spinning");
  const statusText = document.createElement("span");
  statusText.textContent = "运行中";
  status.append(statusIcon, statusText);
  const chevron = makeIconSlot("chevron-down", "tool-chevron");
  // 子代理:标题行里放一条单行窥视(和「已思考」标题右侧尾巴同款),收起态
  // 显示子代理当前在做什么;不再用带底色的方块(那读起来像独立 tag,09-11)。
  let taskPeek = null;
  let taskToken = null;
  if (isTask) {
    const peekSlot = document.createElement("span");
    peekSlot.className = "reasoning-peek tool-peek";
    taskPeek = document.createElement("span");
    peekSlot.appendChild(taskPeek);
    // 前台子代理行也带 token 消耗 + 读秒(09-12 #6,与后台任务条同口径)。
    // token 由 renderSubagentProgress 解析 stats 后写进 taskToken;读秒由全局
    // ticker 按 data-task-start 更新,卡片进入 is-success/is-failure 即定格。
    taskToken = document.createElement("span");
    taskToken.className = "job-chip-token tool-task-token";
    const seconds = document.createElement("span");
    seconds.className = "tool-task-seconds";
    seconds.dataset.taskStart = String(performance.now());
    seconds.textContent = "0s";
    // 布局(#2):子代理·title · token 秒数 · <淡出过渡> 窥视(撑开)。token/秒数紧跟标题,
    // 窥视占满余下、左侧淡出,不再夹在标题和 token 之间把标题顶开。
    head.append(icon, title, taskToken, seconds, peekSlot, status, chevron);
  } else {
    head.append(icon, title, status, chevron);
  }
  let commandPreview = null;
  let commandOutputPreview = null;
  if (isCommand) {
    commandPreview = document.createElement("pre");
    commandPreview.className = "tool-command-preview";
    commandPreview.textContent = commandText || subjectText || "等待命令";
    commandOutputPreview = document.createElement("div");
    commandOutputPreview.className = "tool-command-output-preview";
    commandOutputPreview.setAttribute("aria-label", "最近命令输出");
    commandOutputPreview.style.setProperty("--command-output-lines", String(COMMAND_OUTPUT_PREVIEW_ROWS));
    commandOutputPreview.hidden = true;
  }
  const body = document.createElement("div");
  body.className = "tool-body";
  const argumentsDetail = createToolDetail("参数", true);
  const progressDetail = createToolDetail("进度");
  const stdoutDetail = createToolDetail("命令输出", true);
  const stderrDetail = createToolDetail("错误输出", true);
  stderrDetail.wrapper.classList.add("is-stderr");
  const resultDetail = createToolDetail("结果", true);
  // 文件编辑:patchText 参数画成 diff,而不是摊一坨补丁 JSON(实时与刷新回看同一份)。
  const diffView = window.GqyDiff?.renderFromCall?.({ name: data?.name, arguments: data?.arguments }) || null;
  const argumentText = diffView ? "" : prettyArguments(data?.arguments);
  if (argumentText) {
    argumentsDetail.raw = argumentText;
    argumentsDetail.content.textContent = argumentText;
    argumentsDetail.wrapper.hidden = false;
  }
  body.append(argumentsDetail.wrapper, progressDetail.wrapper, stdoutDetail.wrapper, stderrDetail.wrapper, resultDetail.wrapper);
  if (diffView) body.insertBefore(diffView, argumentsDetail.wrapper);
  // 子代理:收起看标题行的窥视,展开看下面的「子过程时间线」——子代理自己的
  // 思考与工具流,和主智能体的过程区同款渲染(09-11 用户要求)。不再用方块。
  let liveProgress = null;
  let subBlocks = null;
  let briefBuilt = false;
  if (isTask) {
    // 子过程时间线的承载容器:proc-line 挂进这里(和主对话过程区同构)。
    subBlocks = document.createElement("div");
    subBlocks.className = "sub-blocks assistant-blocks";
    body.insertBefore(subBlocks, body.firstChild);
    // 子代理的任务简介放在展开区最上方,美化呈现,不再让人去读裸 JSON 参数
    //(09-12 #6):标题=description,正文=prompt(整段保留换行)。裸参数那栏
    // 对子代理收起来(信息都在简介里了)。
    const taskArgs = parsedToolArguments(data?.arguments);
    const brief = buildSubagentBrief(taskArgs.description, taskArgs.prompt);
    if (brief) {
      attachSubBrief(subBlocks, brief);
      argumentsDetail.wrapper.hidden = true;
      briefBuilt = true;
    }
    const fold = document.createElement("div");
    fold.className = "tool-fold";
    fold.appendChild(body);
    card.append(head, fold);
    if (taskPeek) taskPeek.textContent = reasoningPeekText(subjectText || "正在启动子代理…");
  } else {
    card.append(head);
    if (commandPreview) card.appendChild(commandPreview);
    if (commandOutputPreview) card.appendChild(commandOutputPreview);
    const fold = document.createElement("div");
    fold.className = "tool-fold";
    fold.appendChild(body);
    card.appendChild(fold);
  }
  const tool = {
    id: toolId,
    name: String(data?.name || ""),
    card,
    head,
    body,
    status,
    statusIcon,
    statusText,
    summary,
    commandPreview,
    commandOutputPreview,
    commandText,
    artifactPreview: null,
    pendingOutputPreview: null,
    outputRenderFrame: null,
    argumentsDetail,
    progressDetail,
    stdoutDetail,
    stderrDetail,
    resultDetail,
    isTask,
    liveProgress,
    taskPeek,
    taskToken,
    brief: briefBuilt,
    blocks: subBlocks,
    think: null,
    thinkAccum: "",
    pendingCall: null,
    titleText: String(data?.display_name || data?.name || "工具"),
    subject: subjectText,
    startedAt: performance.now(),
    finishedAt: null,
    imageCount: 0,
    isImageTool,
    imagePlaceholder: null,
    finished: false,
    collapseTimer: null
  };
  head.addEventListener("click", () => {
    const collapsed = card.classList.toggle("collapsed");
    head.setAttribute("aria-expanded", String(!collapsed));
    // 收起子代理状态行时,把里面已展开的思考/工具也一并收起,下次展开是干净的
    // 收起态(#5),不然收起只是把外层折了、里面还留着上次的展开。
    if (collapsed) {
      card.querySelectorAll(".sub-blocks details[open]").forEach((d) => {
        d.open = false;
      });
      card.querySelectorAll(".sub-blocks .tool-card:not(.collapsed)").forEach((inner) => {
        inner.classList.add("collapsed");
        const innerHead = inner.querySelector(".tool-head");
        if (innerHead) innerHead.setAttribute("aria-expanded", "false");
      });
    }
    railSnapFit(card);
    syncBubbleWidth(live.article);
    if (!collapsed) {
      window.requestAnimationFrame(() => {
        scrollToolOutputToEnd(tool);
        // 展开从「当前进行中」看起,而不是从顶部(#8)。滚到底 = 最新那一步。
        const sc = subScrollContainer(tool);
        if (sc) { sc.__pinnedUp = false; sc.scrollTop = sc.scrollHeight; }
        contentAdded();
      });
    }
  });
  updateToolSummary(tool);
  card.gqyTiming = tool;
  live.tools.set(toolId, tool);
  // 顶替「准备 xx」占位签时不重放淡入:占位签已经平滑滑入,这里只是原地
  // 把文字换成正式工具名,再滑一次会显得整行错位(#17,只在会发 preparing
  // 的中转线后端出现)。
  if (opts.staticEnter) card.style.animation = "none";
  procLineAttach(live.blocks, card);
  if (isImageTool) {
    const bubble = document.createElement("div");
    bubble.className = "image-gen-bubble";
    const label = document.createElement("span");
    label.className = "image-gen-label";
    label.textContent = toolName === "print_image" ? "正在加载图片" : "正在生成图片";
    if (subjectText) bubble.title = subjectText;
    bubble.appendChild(label);
    procLineBreak(live.blocks);
    live.blocks.appendChild(bubble);
    startImageGenDots(bubble);
    tool.imagePlaceholder = bubble;
  }
  syncBubbleWidth(live.article);
  contentAdded(live);
  return tool;
}

export function ensureTool(live, data) {
  const toolId = String(data?.tool_id || "");
  return (toolId && live.tools.get(toolId)) || createTool(live, data);
}

// The backend sends the phase text; the local map is only a fallback for a
// daemon older than this asset.
export function preparingToolLabel(name, phase) {
  if (phase) return String(phase);
  if (["edit", "artifact", "kb", "apply_patch", "apply_artifact_patch"].includes(name)) return "准备编辑";
  if (name === "run_command") return "准备执行";
  if (name === "ask_question") return "准备问题";
  return "准备工具";
}

export function clearPreparingTool(live) {
  if (!live?.preparingTool) return;
  live.preparingTool.remove();
  live.preparingTool = null;
  stopPreparingTimer(live);
  contentAdded(live);
}

export function stopPreparingTimer(live) {
  if (!live?.preparingTimer) return;
  window.clearInterval(live.preparingTimer);
  live.preparingTimer = null;
}

/// 准备窗口结束：秒表归零，下一批重新计。
///
/// 只在**工具真的跑完**或新一轮思考开始时调用,不在 `tool.started` 时调用
/// ——批量调用里第二个工具的准备提示紧接着第一个的开工到来,那还是同一个
/// 等待窗口,归零的话屏幕上的秒数来回横跳(与 REPL 的
/// `tool_preparing_since` 同一套语义)。
export function resetPreparingWindow(live) {
  if (!live) return;
  live.preparingSince = null;
  clearPreparingTool(live);
}

export function renderPreparingLabel(live) {
  const tag = live?.preparingTool;
  if (!tag) return;
  const label = tag.querySelector(".tool-preparing-label");
  if (!label) return;
  const base = tag.dataset.phaseLabel || "";
  const elapsed = live.preparingSince == null
    ? ""
    : formatToolDuration(performance.now() - live.preparingSince);
  label.textContent = elapsed ? `${base} · ${elapsed}` : base;
}

export function handleToolPreparing(live, data) {
  const name = String(data?.tool_name || "");
  if (!name) return;
  ensureLiveArticle(live);
  clearTypingIndicator(live, { waitingOnly: true });
  finalizeLiveReasoning(live);
  // 窗口起点只认第一次——批量里换了工具不重新计时。
  if (live.preparingSince == null) live.preparingSince = performance.now();
  if (live.preparingTool?.dataset.toolName === name) return;
  clearPreparingTool(live);
  const tag = document.createElement("div");
  tag.className = "tool-preparing-tag";
  tag.dataset.toolName = name;
  tag.dataset.phaseLabel = preparingToolLabel(name, data?.phase);
  const label = document.createElement("span");
  label.className = "tool-preparing-label";
  tag.append(makeIconSlot("loader-circle", "is-spinning"), label);
  procLineAttach(live.blocks, tag);
  live.preparingTool = tag;
  renderPreparingLabel(live);
  live.preparingTimer = window.setInterval(() => renderPreparingLabel(live), 200);
  syncBubbleWidth(live.article);
  contentAdded(live);
}

export function handleToolEvent(name, live, data) {
  if (name === "tool.preparing") {
    handleToolPreparing(live, data);
    return;
  }
  if (name === "tool.started") {
    // 只撤标签,不清 `preparingSince`：同一批里下一个工具的准备提示紧接着
    // 到来,那还是同一个等待窗口。
    const morphing = !!live.preparingTool;
    clearPreparingTool(live);
    createTool(live, data, { staticEnter: morphing });
    return;
  }
  const tool = ensureTool(live, data);
  if (name === "tool.image") {
    const asset = data?.asset && typeof data.asset === "object" ? data.asset : null;
    if (asset && safeAssetUrl(asset.url)) {
      const assetId = String(asset.id || asset.url);
      if (!live.assets.some((item) => String(item?.id || item?.url) === assetId)) {
        ensureLiveArticle(live);
        clearTypingIndicator(live, { waitingOnly: true });
        breakLiveText(live);
        finalizeLiveReasoning(live);
        live.contextOperation = null;
        live.assets.push(asset);
        const media = createConversationMedia(asset, { eager: true });
        if (tool.imagePlaceholder) {
          stopImageGenDots(tool.imagePlaceholder);
          tool.imagePlaceholder.replaceWith(media);
          tool.imagePlaceholder = null;
        } else {
          procLineBreak(live.blocks);
          live.blocks.appendChild(media);
        }
        // 不自动进 artifact:图片已经在气泡里画出来了,再塞进面板等于同一张
        // 图占两个位置,还会把面板自动切过去盖住用户正在看的东西——表情包
        // 也会。要在工作区看，气泡上有「在预览工作区打开」按钮。
        syncBubbleWidth(live.article);
        tool.imageCount += 1;
      }
    } else if (data?.error) {
      const message = String(data.error);
      tool.progressDetail.raw = message;
      tool.progressDetail.content.textContent = message;
      tool.progressDetail.wrapper.hidden = Boolean(tool.liveProgress);
      if (tool.liveProgress) {
        tool.liveProgress.textContent = message;
        tool.liveProgress.hidden = false;
      }
    }
    updateToolSummary(tool);
  } else if (name === "tool.artifact") {
    const artifact = normalizeArtifact(data?.artifact, "file");
    if (artifact) {
      registerArtifact(artifact, { autoOpen: true });
      if (!live.artifacts) live.artifacts = [];
      const index = live.artifacts.findIndex((item) => String(item?.id) === artifact.id);
      if (index >= 0) live.artifacts[index] = artifact;
      else live.artifacts.push(artifact);
      // 实时回合同样画到气泡底部,与刷新后 createAssistantMessage 那份同构。
      const liveContent = live.article?.querySelector(".assistant-content");
      if (liveContent) {
        window.GqyArtifactChips?.sync(liveContent, live.artifacts, artifactChipOptions());
        syncBubbleWidth(live.article);
      }
      if (!tool.artifactPreview) {
        tool.artifactPreview = document.createElement("button");
        tool.artifactPreview.type = "button";
        tool.artifactPreview.className = "tool-artifact-preview";
        tool.card.insertBefore(tool.artifactPreview, tool.body);
        tool.artifactPreview.addEventListener("click", () => {
          const current = state.artifacts.find((item) => item.id === tool.artifactPreview.dataset.artifactId);
          if (!current) return;
          state.selectedArtifactId = current.id;
          setArtifactWorkspaceOpen(true);
        });
      }
      tool.artifactPreview.dataset.artifactId = artifact.id;
      const artifactLabel = document.createElement("span");
      artifactLabel.textContent = artifact.name;
      tool.artifactPreview.replaceChildren(
        makeIconSlot(artifactIconName(artifact)),
        artifactLabel,
        makeIconSlot("panel-right")
      );
      tool.subject = artifact.name;
    } else if (data?.error) {
      tool.progressDetail.raw = String(data.error);
      tool.progressDetail.content.textContent = tool.progressDetail.raw;
      tool.progressDetail.wrapper.hidden = false;
    }
    updateToolSummary(tool);
  } else if (name === "tool.progress" && tool.isTask) {
    // 子代理:标题行单行窥视 + 展开后的子过程时间线,不再用带底色的方块。
    // (实时 token 汇进「累计」的逻辑统一在 renderSubagentProgress 的 stats 分支里,
    // 前台工具卡与后台任务条同源,见 #131。)
    renderSubagentProgress(tool, String(data?.message || ""));
    if (!tool.finished) updateToolStatus(tool, "运行中", "loader-circle");
  } else if (name === "tool.progress") {
    let message = String(data?.message || "");
    // 文件编辑(edit/kb/artifact):diff 卡已由 patchText 参数在建卡时画好,「准备修改」
    // 这类阶段签、`__patch_preview__` 预览等中间进度都是噪点,一律丢弃,只留 diff + 结果。
    if (message.startsWith("__patch_preview__") || window.GqyDiff?.isEditTool?.(tool.name)) return;
    // 阶段签(「准备修改」这类)只描述过程,不是结果:工具失败后不该留在卡片上
    // 当错误说明(09-11 手机端实测 edit 被沙盒拒后还挂着「准备修改」)。
    tool.lastProgressWasPhase = message.startsWith("__tool_phase__");
    if (message.startsWith("__tool_phase__")) {
      message = message.slice("__tool_phase__".length).replace(/^~\s*/, "").trim();
    } else if (message.startsWith("__subagent_stats__")) {
      message = message.slice("__subagent_stats__".length).trim();
    } else if (message.startsWith("__subagent_detach__")) {
      message = message.slice("__subagent_detach__".length).trim();
    }
    // 任何持续汇报进度的工具(插件子代理如兼容性调查)都惰性获得实时进度面板,
    // 不再仅限内置 task 工具
    if (!tool.liveProgress && !tool.finished && message) {
      tool.liveProgress = document.createElement("div");
      tool.liveProgress.className = "tool-live-progress";
      // body 在普通/命令卡里包在 .tool-fold 里,不是 card 的直接子节点,直接
      // card.insertBefore(_, body) 会抛 NotFoundError(编辑工具的「准备修改」阶段
      // 一直在悄悄抛,live 进度面板从来没真出现过)。挂到 body 顶部即可。
      if (tool.body.parentNode === tool.card) {
        tool.card.insertBefore(tool.liveProgress, tool.body);
      } else {
        tool.body.insertBefore(tool.liveProgress, tool.body.firstChild);
      }
    }
    tool.progressDetail.raw = message;
    tool.progressDetail.content.textContent = message;
    tool.progressDetail.wrapper.hidden = !message || Boolean(tool.liveProgress);
    if (tool.liveProgress && message) {
      tool.liveProgress.textContent = message;
      tool.liveProgress.hidden = false;
      syncBubbleWidth(live.article);
    }
    if (!tool.subject && message) tool.subject = compactLine(message);
    updateToolStatus(tool, "运行中", "loader-circle");
    updateToolSummary(tool);
  } else if (name === "tool.output") {
    const detail = data?.stream === "stderr" ? tool.stderrDetail : tool.stdoutDetail;
    detail.raw = boundedAppend(detail.raw, String(data?.output || ""));
    detail.content.textContent = detail.raw;
    detail.wrapper.hidden = !detail.raw;
    if (!tool.card.classList.contains("collapsed")) detail.content.scrollTop = detail.content.scrollHeight;
    scheduleCommandOutputPreview(tool, data?.preview);
    updateToolSummary(tool);
  } else if (name === "tool.finished") {
    tool.finished = true;
    tool.finishedAt = performance.now();
    // 子代理跑完了,把最后停在「正在思考」的那块思考收尾成「已思考」(#4/#7)——
    // subEndReasoning 平时只在下一个工具调用到来时触发,子代理以思考结尾就没人收。
    // 跑完把那块四行活区域平滑收起成一行(用户拍板:运行时展开、完成后收起,可再点开)。
    if (tool.isTask) {
      subEndReasoning(tool);
      tool.card.classList.add("collapsed");
      tool.head.setAttribute("aria-expanded", "false");
      railSnapFit(tool.card);
      // 子代理跑完:它的实时估算先「冻住」保留(别立刻抽走,否则基线还没把它算进来
      // 之前累计会掉一下),等下个主回合的权威基线接管时再删(见 handleRoundUsage)。
      const doneId = String(data?.tool_id || tool.id || "");
      const entry = doneId && state.liveSubagentTokens.get(doneId);
      if (entry) { entry.done = true; entry.baseAtDone = asFiniteNumber(state.cumulativeBase?.total); refreshComposerCumulative(); }
    }
    const output = String(data?.output || "");
    tool.resultDetail.raw = output.length > MAX_TOOL_OUTPUT_CHARS ? `[较早输出已省略]\n${output.slice(-MAX_TOOL_OUTPUT_CHARS)}` : output;
    tool.resultDetail.content.textContent = tool.resultDetail.raw;
    // 子代理的最终输出要显示出来(#6:用户要看 AI 的最终输出,上批误删了)。
    // 编辑工具成功时结果是 `{ok:true,files:[…]}` 样板,和 diff 卡重复——藏掉;失败留报错。
    const hideEditOutput = Boolean(data?.ok) && window.GqyDiff?.isEditTool?.(tool.name)
      && tool.body.querySelector(".diff-view");
    tool.resultDetail.wrapper.hidden = !tool.resultDetail.raw || Boolean(hideEditOutput);
    if (tool.commandPreview && tool.resultDetail.raw) {
      tool.stdoutDetail.wrapper.hidden = true;
      tool.stderrDetail.wrapper.hidden = true;
    }
    const ok = Boolean(data?.ok);
    resetPreparingWindow(live);
    // 只刷正在看的那个会话——后台会话的 todowrite 不该改屏幕上这块面板。
    // 08-21 token-diet:新版 todowrite 输出是一行文本(不再回显整表 JSON),
    // parse 不出来时改从会话 todos API 取当前清单;旧 JSON 输出走原路。
    if (ok && window.GqyTodos?.isTodoTool(tool.name)) {
      const parsed = window.GqyTodos.parse(output);
      const sameSession = runSessionId(live.runId) === String(state.viewSessionId || "");
      if (parsed) {
        if (sameSession) renderStageTodos(parsed);
        // 与回看那份同构（`createPersistedToolCard`）：待办列表挂在签外面。
        // 只在这里画会让实时和刷新后长得不一样,那正是工具签之前踩过的坑。
        const todos = window.GqyTodos.renderList(parsed);
        tool.card.querySelector(".todo-panel")?.remove();
        if (todos) tool.card.appendChild(todos);
      } else {
        attachLiveTodoPanel(tool, live, sameSession);
      }
    }
    // 分享附件同坑同修:实时完成时也要挂,否则只有刷新后才能看到卡片。
    if (ok && window.GqyShared?.isShareTool(tool.name)) {
      const shared = window.GqyShared.renderCard(output);
      tool.card.querySelector(".shared-attachment")?.remove();
      if (shared) tool.card.appendChild(shared);
    }
    if (ok) attachToolRichCards(tool.card, tool.name, output);
    scheduleCommandOutputPreview(tool, data?.preview);
    if (tool.imagePlaceholder) {
      stopImageGenDots(tool.imagePlaceholder);
      // 失败不留空气泡(08-22 用户反馈):撤占位、露芯片,错误细节在芯片里。
      tool.imagePlaceholder.remove();
      tool.imagePlaceholder = null;
    }
    if (tool.isImageTool && !ok) {
      tool.card.classList.remove("image-tool-chip");
    }
    // 时间线上成功不打勾不写「完成」,右侧就是耗时;失败才写字
    updateToolStatus(tool, ok ? formatToolDuration(tool.finishedAt - tool.startedAt) || "完成" : "失败", ok ? "check" : "circle-alert", ok ? "is-success" : "is-failure");
    updateToolSummary(tool);
    if (tool.liveProgress) {
      if (ok || tool.lastProgressWasPhase) tool.liveProgress.hidden = true;
      else tool.liveProgress.classList.add("is-error");
      tool.progressDetail.wrapper.hidden = !tool.progressDetail.raw;
      syncBubbleWidth(live.article);
    }
    if (!state.toolExpanded) {
      tool.card.classList.add("collapsed");
      tool.head.setAttribute("aria-expanded", "false");
    }
  }
  contentAdded(live);
}
