import { MAX_ATTACHMENTS, MAX_CONTENT_CHARS } from "../../core/constants.js";
import { formatInteger } from "../../core/format.js";
import { createIcon } from "../../core/icons.js";
import { layoutViewportWidth } from "../../core/ui-scale.js";
import { syncComposerDockHeight } from "../artifacts/model.js";
import { revisionEligible } from "../conversation/actions.js";
import { conversationRunning, hasPendingQuestion } from "../conversation/chrome.js";
import { updateJumpButtonOffset } from "../conversation/scroll.js";
import { syncRunIndicator } from "../live/state.js";
import { updateModelMenuState } from "../model-menu/menu.js";
import { updateSettingsControls } from "../settings/config.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";
import { scheduleDraftSave } from "./drafts.js";

export function countCharacters(value) {
  return Array.from(String(value || "")).length;
}

// 触屏设备(手机/平板):没有悬停、指针粗。回车语义与自动聚焦都按它分岔。
export function isTouchComposer() {
  return window.matchMedia("(hover: none), (pointer: coarse)").matches;
}

// 触屏设备上程序化聚焦会弹出软键盘挡住内容，只在桌面端自动聚焦
export function focusComposerIfDesktop() {
  if (isTouchComposer()) return;
  elements.composerInput.focus();
}

export function resizeComposer() {
  const input = elements.composerInput;
  input.style.height = "auto";
  input.style.height = `${Math.min(input.scrollHeight, layoutViewportWidth() <= 760 ? 120 : 146)}px`;
  const count = countCharacters(input.value);
  elements.characterCount.textContent = `${formatInteger(count)} / 20,000`;
  elements.characterCount.hidden = count < 18_000;
  elements.characterCount.classList.toggle("is-error", count > MAX_CONTENT_CHARS);
  // 打字、发送后清空、命令回填都会走到这里:草稿随之写回当前会话的键。
  scheduleDraftSave();
  updateControlState();
  // 输入框多行增高时,artifact 浮层的让位高度跟着更新(#2)。
  if (state.artifactOpen) syncComposerDockHeight();
  window.requestAnimationFrame(updateJumpButtonOffset);
}

export function updateControlState() {
  syncRunIndicator();
  const running = conversationRunning();
  const busy = state.adminBusy || state.submitting;
  const locked = state.blocked || state.adminBusy;
  const inputCount = countCharacters(elements.composerInput.value.trim());
  const attachmentUploading = state.composerAttachments.some((item) => item.status === "uploading");
  const attachmentError = state.composerAttachments.some((item) => item.status === "error");
  const attachmentReady = state.composerAttachments.some((item) => item.status === "ready");

  elements.composerInput.disabled = locked;
  elements.composerForm.classList.toggle("is-disabled", locked);
  elements.attachButton.disabled = locked || state.submitting || !state.capabilities?.attachments || state.composerAttachments.length >= MAX_ATTACHMENTS;
  elements.micButton.disabled = locked || state.submitting;
  elements.newChatButton.disabled = state.blocked || busy || state.sessionBusy || state.viewLoading;
  // 会话级模型覆盖允许在回复进行中调整，下一轮生效。
  elements.modelButton.disabled = state.blocked || state.models.length === 0;
  elements.promptGrid.querySelectorAll("button").forEach((button) => {
    button.disabled = state.blocked || running || busy;
  });
  updateModelMenuState();

  elements.sendButton.classList.remove("is-cancel");
  elements.sendButton.querySelector(".icon-slot").replaceChildren(createIcon("arrow-up"));
  elements.sendButton.title = running ? "加入队列" : "发送消息";
  elements.sendButton.setAttribute("aria-label", elements.sendButton.title);
  elements.sendButton.disabled = state.blocked || state.adminBusy || state.submitting || hasPendingQuestion()
    || (inputCount === 0 && !attachmentReady) || inputCount > MAX_CONTENT_CHARS || attachmentUploading || attachmentError;
  // 语音与发送合并成同一个位置(用户):gqy voice 可用、且没有输入、且不在排队/运行时
  // 显麦克风(点了走语音),否则显发送。voice 不可用就永远是发送。
  const hasDraft = inputCount > 0 || attachmentReady;
  // 有话要说时输入框换一圈流动的描边(普通)/亮起左侧竖线(开发),见 40-composer.css。
  elements.composerForm.classList.toggle("has-draft", hasDraft && !locked);
  const showMic = state.voiceEnabled === true && !hasDraft && !running && !state.submitting;
  elements.micButton.hidden = !showMic;
  elements.sendButton.hidden = showMic;
  document.querySelectorAll(".edit-action, .redo-action").forEach((button) => {
    button.disabled = !revisionEligible();
  });

  if (state.blocked) elements.composerState.textContent = "未授权";
  // 被问问题时不再在输入框页脚重复「等待回答」——问题卡自己就写着,页脚这份多余
  // 且被模型芯片/速度挤成竖排(#3)。留空即可。
  else if (hasPendingQuestion()) elements.composerState.textContent = "";
  else if (attachmentUploading) elements.composerState.textContent = "正在上传";
  else if (attachmentError) elements.composerState.textContent = "附件上传失败";
  else if (busy) elements.composerState.textContent = state.submitting ? (running ? "正在加入队列" : "正在发送") : "正在处理";
  else if (inputCount > MAX_CONTENT_CHARS) elements.composerState.textContent = "消息不能超过 20,000 个字符";
  else elements.composerState.textContent = "";
  elements.composerState.classList.toggle("is-error", inputCount > MAX_CONTENT_CHARS || attachmentError);
  updateSettingsControls();
}
