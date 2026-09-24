import { makeIconSlot } from "../../core/icons.js";
import { procLineBreak } from "../conversation/proc-rail.js";
import { contentAdded } from "../conversation/scroll.js";
import { breakLiveText, clearTypingIndicator, ensureLiveArticle, syncBubbleWidth } from "./state.js";
import { finalizeLiveReasoning } from "./stream.js";
import { boundedAppend } from "../tools/cards.js";

export function createContextOperation(live, kind) {
  ensureLiveArticle(live);
  clearTypingIndicator(live, { waitingOnly: true });
  breakLiveText(live);
  finalizeLiveReasoning(live);
  const block = document.createElement("section");
  block.className = "context-operation";
  const title = document.createElement("strong");
  title.append(makeIconSlot("refresh-cw"), document.createElement("span"));
  title.lastChild.textContent = kind === "compact" ? "正在整理上下文" : "正在释放旧上下文";
  const output = document.createElement("pre");
  output.hidden = true;
  block.append(title, output);
  const operation = { kind, block, title: title.lastChild, output, raw: "" };
  procLineBreak(live.blocks);
  live.blocks.appendChild(block);
  syncBubbleWidth(live.article);
  live.contextOperation = operation;
  contentAdded(live);
  return operation;
}

export function handleContextEvent(name, live, data) {
  if (name === "context.compact_start") createContextOperation(live, "compact");
  else if (name === "context.compact_delta") {
    const operation = live.contextOperation?.kind === "compact" ? live.contextOperation : createContextOperation(live, "compact");
    operation.raw = boundedAppend(operation.raw, String(data?.delta || ""));
    operation.output.textContent = operation.raw;
    operation.output.hidden = !operation.raw;
  } else if (name === "context.compact_end") {
    if (live.contextOperation?.kind === "compact") live.contextOperation.title.textContent = "上下文已整理";
    live.contextOperation = null;
  } else if (name === "context.pop_start") createContextOperation(live, "pop");
  else if (name === "context.pop_end") {
    if (live.contextOperation?.kind === "pop") live.contextOperation.title.textContent = "旧上下文已释放";
    live.contextOperation = null;
  } else if (name === "context.error") {
    const operation = live.contextOperation || createContextOperation(live, "compact");
    operation.block.classList.add("is-error");
    operation.title.textContent = "上下文整理未完成";
    operation.raw = String(data?.message || "上下文维护失败");
    operation.output.textContent = operation.raw;
    operation.output.hidden = false;
    live.contextOperation = null;
  }
  contentAdded(live);
}
