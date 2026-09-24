import { MAX_TOOL_OUTPUT_CHARS } from "../../core/constants.js";
import { createIcon, makeIconSlot } from "../../core/icons.js";
import { procLineRefresh, railSnapFit } from "../conversation/proc-rail.js";
import { contentAdded } from "../conversation/scroll.js";
import { renderSubagentProgress, subEndContent, subEndReasoning } from "../conversation/subagent.js";
import { dedupeToolSubject, formatToolDuration, isSubagentTool, parsedToolArguments, prettyArguments, toolSubject } from "./format.js";
import { state } from "../../state/store.js";

export function updateToolSummary(tool) {
  const details = [];
  const subject = dedupeToolSubject(tool.titleText, tool.subject);
  if (tool.commandPreview) {
    tool.commandPreview.textContent = tool.commandText || subject || "等待命令";
    tool.summary.textContent = tool.commandText || subject || "";
    return;
  }
  if (subject) details.push(subject);
  if (tool.imageCount) details.push(`${tool.imageCount} 张图片`);
  // 没有主语就空着:「无输出 / 等待输出」是旧芯片时代占摘要位的话,时间线上耗时和转圈
  // 都在状态位,这里再写字只会让人以为工具真的没输出。
  tool.summary.textContent = details.filter(Boolean).join(" · ");
}

export function scrollToolOutputToEnd(tool) {
  for (const detail of [tool.stdoutDetail, tool.stderrDetail, tool.resultDetail]) {
    if (!detail.wrapper.hidden) detail.content.scrollTop = detail.content.scrollHeight;
  }
}

export function boundedAppend(current, addition) {
  const combined = `${current || ""}${addition || ""}`;
  if (combined.length <= MAX_TOOL_OUTPUT_CHARS) return combined;
  return `[较早输出已省略]\n${combined.slice(combined.length - MAX_TOOL_OUTPUT_CHARS)}`;
}

// 持久化回合里的工具卡片（只读）。
//
// 不复用 `createTool`：那个和实时流状态强耦合（往 live.tools 注册、跟踪
// 分块输出、进度更新），拿持久化数据去喂它要伪造一个 live 对象，很脆。
// 这里只画「调了什么、给了什么参数、返回了什么」，CSS 类沿用同一套，
// 所以看起来和实时那份一致。
//
// 数据来自 `turn.tool_flow`，库里一直有——以前 API 不发，于是 WebUI 的
// 工具信息只在事件流里活过一次，切走再回来就没了。
/*
 * 工具卡上的富卡片(地图 / 快递),两种挂法:
 *
 *   outside —— 挂在工具签**外面**,收起态也看得见(待办、分享附件那一档:
 *              是给人看的交付物)。
 *   fold    —— 挂进 `.tool-body`,跟着工具签一起收起,展开才看得到。
 *
 * **地图走 fold,是隐私判断不是布局偏好**(09-13 晚用户拍板):一张地图钉的是
 * 现实里的一个点——家、常去的店。默认摊在气泡里,截图、投屏、旁边有人时全躲
 * 不掉,而它并不是每次都要看的东西。默认藏起来、要看点一下,代价小得多。
 *
 * 三处调用(回看重建、子过程回放、实时完成)走同一个函数,少一处就会出现
 * 「实时有、刷新没了」那类不一致,工具签自己踩过这个坑。
 */
export const TOOL_RICH_CARDS = [
  { selector: ".map-card", mount: "fold", module: () => window.GqyMap, matches: (m, name) => m.isMapTool(name), render: (m, output) => m.renderCard(output) },
  { selector: ".express-card", mount: "outside", module: () => window.GqyExpress, matches: (m, name) => m.isExpressTool(name), render: (m, output) => m.renderCard(output) },
];

export function toolRichCards(name, output) {
  const cards = [];
  for (const kind of TOOL_RICH_CARDS) {
    const module = kind.module();
    if (!module || !kind.matches(module, String(name || ""))) continue;
    const node = kind.render(module, String(output || ""));
    if (node) cards.push({ node, selector: kind.selector, mount: kind.mount });
  }
  return cards;
}

/** 挂到工具卡上,重画时先摘掉上一张(实时完成会重复调用)。 */
export function attachToolRichCards(card, name, output) {
  for (const { node, selector, mount } of toolRichCards(name, output)) {
    card.querySelector(selector)?.remove();
    // fold 挂进 .tool-body:收起时被 grid 0fr + overflow:hidden 一起收走。
    // 找不到 body(理论上不会)就退回挂外面——宁可露出来也别把卡片丢了。
    const fold = mount === "fold" ? card.querySelector(".tool-body") : null;
    (fold || card).appendChild(node);
  }
}

export function createPersistedToolCard(call) {
  const card = document.createElement("section");
  card.className = state.toolExpanded ? "tool-card" : "tool-card collapsed";
  const name = String(call?.name || "");
  if (name === "run_command" || name === "Bash") card.classList.add("is-command");
  if (isSubagentTool(name)) card.classList.add("is-task");
  // 图标配色来自 is-success（金）/ is-failure（红）。两个都不加会退回默认色，
  // 看起来就是「颜色不对」。
  //
  // 成败没有单独落库，但也不需要：运行时那个 ok 本来就是从输出文本算的
  // （`tool_output_succeeded`：输出是 JSON 且 success/ok 为 false 才算失败，
  // 其余一律成功），这里照抄同一条规则，两边判定必然一致。
  // 成败由后端算好（`web::dto::tool_call_succeeded`）：规则有两条——硬失败
  // 看 `tool error:` 前缀，业务失败看输出 JSON 的 success/ok。抄到这里就成
  // 了第二份真相，改一条忘另一条，同一次调用实时是红的、刷新变绿的。
  const ok = call?.ok !== false;
  card.classList.add(ok ? "is-success" : "is-failure");
  if (ok && (name === "generate_image" || name === "print_image")) {
    card.classList.add("image-tool-chip");
  }

  const head = document.createElement("button");
  head.className = "tool-head";
  head.type = "button";
  head.setAttribute("aria-expanded", String(Boolean(state.toolExpanded)));
  const icon = document.createElement("span");
  icon.className = "tool-icon";
  icon.appendChild(makeIconSlot(toolIconName(name)));
  // 与实时同构的三段：友好名（粗体）/ 技术名（小字）/ 主语摘要。
  // 只画技术名的话，用户看到的就是 archlinux_official_package_query 这种。
  const title = document.createElement("span");
  title.className = "tool-title";
  const displayName = document.createElement("strong");
  displayName.textContent = String(call?.display_name || name || "工具");
  // 子代理:显示「子代理 / 开发中」,不显裸的 `subagent:xxx`(刷新回看时历史里存的
  // display_name 是技术名,和实时的「子代理」不一致,#97 刷新后变回原始名)。任务
  // 标题走下面的 summary(toolSubject → description)。
  if (isSubagentTool(name)) {
    displayName.textContent =
      parsedToolArguments(call?.arguments)?.dev === true ? "开发中" : "子代理";
  }
  // 名字被芯片截断时,悬浮还能看全(load_tools 一次点名几个工具就会超长)。
  displayName.title = displayName.textContent;
  const realName = document.createElement("small");
  realName.className = "tool-technical-name";
  realName.textContent = name;
  const summary = document.createElement("small");
  summary.className = "tool-summary";
  summary.textContent = toolSubject(name, call?.arguments) || "";
  title.append(displayName, realName, summary);
  // 与实时那份同构：head 是 icon / title / status / chevron 四段。少了
  // status 这段，回看时卡片会比实时的窄一块，右边空一片。
  const status = document.createElement("span");
  status.className = "tool-status";
  const statusText = document.createElement("span");
  const startedMs = Number(call?.started_ms);
  const finishedMs = Number(call?.finished_ms);
  const hasSpan = Number.isFinite(startedMs) && Number.isFinite(finishedMs) && finishedMs >= startedMs;
  if (hasSpan) card.gqyTiming = { startedAt: startedMs, finishedAt: finishedMs };
  statusText.textContent = ok ? (hasSpan ? formatToolDuration(finishedMs - startedMs) || "完成" : "完成") : "失败";
  status.append(makeIconSlot(ok ? "check" : "circle-alert"), statusText);
  head.append(icon, title, status, makeIconSlot("chevron-down", "tool-chevron"));
  head.addEventListener("click", () => {
    const collapsed = card.classList.toggle("collapsed");
    head.setAttribute("aria-expanded", String(!collapsed));
    railSnapFit(card);
  });

  const body = document.createElement("div");
  body.className = "tool-body";
  // 文件编辑:把 patchText 参数画成 diff(增删配色),而不是摊一坨补丁 JSON。
  // patchText 随 tool_flow 落库,回看/刷新走同一份。渲不出(解析失败)再退回原始参数。
  const diffView = window.GqyDiff?.renderFromCall?.(call) || null;
  if (diffView) {
    body.appendChild(diffView);
  } else {
    const argumentText = prettyArguments(call?.arguments);
    if (argumentText) {
      const detail = createToolDetail("参数", true);
      detail.content.textContent = argumentText;
      detail.wrapper.hidden = false;
      body.appendChild(detail.wrapper);
    }
  }
  const output = String(call?.output || "");
  // 编辑成功时,结果就是 `{ok:true, files:[…]}` 这类样板,和上面的 diff 重复——藏掉;
  // 失败时结果是报错原文,留着(diffView 存在=是编辑工具且解析出了补丁)。
  const hideEditOutput = diffView && ok;
  if (output && !hideEditOutput) {
    const detail = createToolDetail("结果", true);
    detail.content.textContent = output;
    detail.wrapper.hidden = false;
    body.appendChild(detail.wrapper);
  }
  // 子代理:回看/刷新时把落库的子过程标记流回放成时间线(#9)。放在参数/结果之前,
  // 和实时展开态一个样。用一个一次性 sink 走同款 renderSubagentProgress。
  if (isSubagentTool(name) && Array.isArray(call?.sub_trace) && call.sub_trace.length) {
    const subBlocks = document.createElement("div");
    subBlocks.className = "sub-blocks assistant-blocks";
    const sink = {
      blocks: subBlocks, brief: false, think: null, thinkAccum: "", contentBlock: null,
      contentAccum: "", pendingCall: null, taskPeek: null, taskToken: null, peekLine: "",
    };
    for (const marker of call.sub_trace) renderSubagentProgress(sink, String(marker));
    subEndReasoning(sink);
    subEndContent(sink);
    body.insertBefore(subBlocks, body.firstChild);
    card.classList.add("is-task");
  }
  const fold = document.createElement("div");
  fold.className = "tool-fold";
  fold.appendChild(body);
  card.append(head, fold);
  // 待办列表挂在签外面,收起态也看得见——那是给人看的产出,不是调试信息。
  const todos = window.GqyTodos?.isTodoTool(name) ? window.GqyTodos.render(output) : null;
  if (todos) card.appendChild(todos);
  // 分享附件同理:文件卡片是交付物,直接出现在气泡里,点击即下载。
  const shared = window.GqyShared?.isShareTool(name) ? window.GqyShared.renderCard(output) : null;
  if (shared) card.appendChild(shared);
  // 地图/快递卡片同理:坐标与物流轨迹是产出,不是工具日志。
  attachToolRichCards(card, name, output);
  return card;
}

export function createToolDetail(labelText, preformatted = false) {
  const wrapper = document.createElement("div");
  wrapper.className = "tool-detail";
  wrapper.hidden = true;
  const label = document.createElement("span");
  label.className = "tool-detail-label";
  label.textContent = labelText;
  const content = document.createElement(preformatted ? "pre" : "p");
  wrapper.append(label, content);
  return { wrapper, content, raw: "" };
}

export function updateToolStatus(tool, status, iconName, statusClass = "") {
  tool.statusText.textContent = status;
  tool.statusIcon.replaceChildren(createIcon(iconName));
  tool.statusIcon.classList.toggle("is-spinning", iconName === "loader-circle");
  tool.card.classList.remove("is-success", "is-failure");
  if (statusClass) tool.card.classList.add(statusClass);
  procLineRefresh(tool.card.closest(".proc-line"));
}

export function renderCommandOutputPreview(tool) {
  const preview = tool.pendingOutputPreview;
  const panel = tool.commandOutputPreview;
  if (!panel || !preview || !Array.isArray(preview.lines)) return;
  const wasFollowing = panel.hidden || panel.scrollHeight - panel.scrollTop - panel.clientHeight <= 2;
  const previousScrollTop = panel.scrollTop;
  const children = [];
  if (preview.omitted) {
    const omitted = document.createElement("span");
    omitted.className = "tool-command-output-omitted";
    omitted.textContent = "⋮ 已省略较早输出";
    children.push(omitted);
  }
  for (const line of preview.lines) {
    const row = document.createElement("span");
    row.className = `tool-command-output-line${line?.stream === "stderr" ? " is-stderr" : ""}`;
    row.textContent = String(line?.text || "");
    children.push(row);
  }
  panel.replaceChildren(...children);
  panel.hidden = children.length === 0;
  if (!panel.hidden) panel.scrollTop = wasFollowing ? panel.scrollHeight : previousScrollTop;
}

export function scheduleCommandOutputPreview(tool, preview) {
  if (!tool?.commandOutputPreview || !preview || typeof preview !== "object") return;
  tool.pendingOutputPreview = preview;
  if (tool.outputRenderFrame) return;
  tool.outputRenderFrame = window.requestAnimationFrame(() => {
    tool.outputRenderFrame = null;
    renderCommandOutputPreview(tool);
    contentAdded(tool.card);
  });
}

// 工具家族图标(验收清单):终端=$、网络=地球仪、编辑=笔、记忆=大脑……
// 未列家族回落扳手。全部 lucide 线稿,无 emoji。
export function toolIconName(name) {
  const n = String(name || "");
  if (["run_command", "Bash", "job_status", "job_stop"].includes(n)) return "terminal";
  if (["web_search", "web_fetch", "search_web", "webfetch", "read_url_content"].includes(n)) return "globe";
  // agy 原生工具(antigravity 中转)。
  if (["view_file", "list_dir"].includes(n)) return "file-text";
  if (["write_to_file", "replace_file_content"].includes(n)) return "square-pen";
  if (["find_by_name", "grep_search"].includes(n)) return "search";
  if (["manage_task", "invoke_subagent", "define_subagent", "manage_subagents"].includes(n)) return "bot";
  if (n.startsWith("browser_") || n === "call_mcp_tool") return "wrench";
  if (n === "search_web_images") return "image-search";
  if (["edit", "artifact", "kb", "apply_patch", "apply_artifact_patch"].includes(n)) return "square-pen";
  if (["recall_memories", "recall_past_events", "remember_fact", "search_evicted_context"].includes(n)) return "brain";
  if (["create_goal", "get_goal", "update_goal"].includes(n)) return "target";
  if (n === "todowrite" || n === "todoupdate") return "list-todo";
  if (isSubagentTool(n)) return "bot";
  if (n.includes("knowledge_base")) return "book-open";
  if (n === "ask_question") return "circle-help";
  if (n === "generate_image") return "paintbrush";
  if (["analyze_image", "vision_analyze", "print_image"].includes(n)) return "image";
  if (n.includes("meme")) return "smile";
  if (n.includes("alarm")) return "alarm-clock";
  if (n === "read_clipboard") return "clipboard";
  if (n === "get_weather") return "cloud-sun";
  if (["calculator", "scientific_calculator", "calculate_hash", "get_exchange_rate", "decode_encoded_text"].includes(n)) return "calculator";
  if (n === "read" || n === "read_file") return "file-text";
  if (n === "glob" || n === "grep") return "search";
  if (n === "trash_path") return "trash-2";
  if (n === "load_tools") return "package";
  if (n.includes("skill")) return "puzzle";
  if (n.startsWith("aur_") || n.startsWith("archlinux") || n.startsWith("archwiki") || n === "install_aur_package") return "arch";
  if (n.startsWith("online_man")) return "package";
  if (n === "usage_query") return "chart-column";
  if (["draw_tarot_card", "draw_zhouyi_hexagram", "draw_fortune_lot"].includes(n)) return "sparkles";
  if (["create_artifact", "read_artifact", "present_artifact"].includes(n)) return "file-text";
  return "wrench";
}

/// 生图占位气泡的点阵动画(A 方案,08-22 定稿):随机位置/大小/时长的
/// 小块点阵若隐若现,同屏最多 3 块;出图/失败/离屏即停,定时器不外泄。
export function startImageGenDots(bubble) {
  const spawn = () => {
    if (!bubble.isConnected) {
      stopImageGenDots(bubble);
      return;
    }
    if (bubble.querySelectorAll(".dot-patch").length >= 3) return;
    const patch = document.createElement("span");
    patch.className = "dot-patch";
    const size = 60 + Math.random() * 90;
    patch.style.width = `${size}px`;
    patch.style.height = `${size}px`;
    patch.style.left = `${Math.random() * 78}%`;
    patch.style.top = `${Math.random() * 78}%`;
    patch.style.animationDuration = `${(2.2 + Math.random() * 1.6).toFixed(2)}s`;
    patch.addEventListener("animationend", () => patch.remove());
    bubble.appendChild(patch);
  };
  spawn();
  window.setTimeout(spawn, 500);
  bubble.gqyDotsTimer = window.setInterval(spawn, 700);
}

export function stopImageGenDots(bubble) {
  if (bubble?.gqyDotsTimer) {
    window.clearInterval(bubble.gqyDotsTimer);
    bubble.gqyDotsTimer = null;
  }
}
