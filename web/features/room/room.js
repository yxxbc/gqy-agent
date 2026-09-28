// 多方聊天室（方案稿 docs/design/2026-09-27-chat-room.md）。
//
// 房间就是一个会话；打开它时 /api/sessions/{id}/turns 多带一个 `room`
// 字段，sessions/view.js 把它放进 state.viewRoom。对话区的绘制与输入框的
// 提交由这里接管：本模块启动时把 renderRoom / submitRoom 挂到 store 上，
// renderConversation 与 submitTurn 只看回调在不在，不反过来 import 本模块。
//
// 发言事件（room.*）由 live/sse.js 转成 window 上的 `gqy:room-event`。
import { apiRequest } from "../../core/api.js";
import { showToast } from "../../core/toast.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";
import { createUserMessage } from "../conversation/user.js";
import { renderMarkdown } from "../markdown/render.js";
import { makeAvatarFrame } from "../persona.js";
import { openSessionView, refreshSessions } from "../sessions/view.js";

/// 只有本模块用的状态：正在流式输出的发言（run_id → { participant_id, text }）。
const roomState = {
  live: new Map()
};

/// 人格参与者用自己的头像；中转线优先显示供应商品牌图标，未知供应商回退到首字母。
const RELAY_COLORS = {
  Claude: "#d97757",
  Codex: "#3e8e7e",
  Gemini: "#5b7fd6",
  Cline: "#8a6fd1"
};

function currentRoom() {
  const room = state.viewRoom;
  if (!room || String(room.room_id) !== String(state.viewSessionId)) return null;
  return room;
}

function participantOf(room, participantId) {
  return (room?.participants || []).find((item) => String(item.participant_id) === String(participantId)) || null;
}

function makeRoomAvatar(participant) {
  if (participant?.kind === "persona") return makeAvatarFrame("her");
  const frame = document.createElement("span");
  frame.className = "avatar-frame room-avatar";
  frame.setAttribute("aria-hidden", "true");
  const label = String(participant?.label || "?");
  frame.style.setProperty("--room-avatar-color", RELAY_COLORS[label] || "var(--md-sys-color-secondary)");
  const brand = window.GqyProviderIcons?.providerMark(
    { id: participant?.provider_id, display_name: label },
    "room-avatar-brand"
  );
  if (brand) frame.appendChild(brand);
  else frame.textContent = Array.from(label)[0] || "?";
  return frame;
}

function buildParticipantMessage(room, participantId, content, { live = false } = {}) {
  const participant = participantOf(room, participantId);
  const article = document.createElement("article");
  article.className = `message assistant-message room-message${live ? " is-live" : ""}`;
  article.dataset.role = "assistant";
  article.dataset.participantId = String(participantId || "");
  const header = document.createElement("header");
  header.className = "assistant-label";
  const identity = document.createElement("div");
  const name = document.createElement("strong");
  name.textContent = participant?.label || "未知参与者";
  identity.appendChild(name);
  if (participant?.model) {
    const model = document.createElement("span");
    model.className = "room-model";
    model.textContent = participant.model;
    identity.appendChild(model);
  }
  header.append(makeRoomAvatar(participant), identity);
  const body = document.createElement("div");
  body.className = "assistant-content";
  const blocks = document.createElement("div");
  blocks.className = "assistant-blocks";
  const markdown = document.createElement("div");
  markdown.className = "markdown-body";
  if (String(content || "").trim()) renderMarkdown(markdown, content);
  else {
    const typing = document.createElement("span");
    typing.className = "room-typing";
    typing.textContent = "正在想…";
    markdown.appendChild(typing);
  }
  blocks.appendChild(markdown);
  body.appendChild(blocks);
  article.append(header, body);
  return article;
}

function buildNotice(content) {
  const notice = document.createElement("div");
  notice.className = "system-event room-notice";
  const label = document.createElement("span");
  label.textContent = String(content || "");
  notice.appendChild(label);
  return notice;
}

function buildMessageNode(room, message) {
  const kind = String(message?.speaker_kind || "");
  if (kind === "user") return createUserMessage(message.content, message.created_at);
  if (kind === "participant") return buildParticipantMessage(room, message.participant_id, message.content);
  return buildNotice(message.content);
}

async function stopRoom() {
  const room = currentRoom();
  if (!room) return;
  try {
    await apiRequest(`/api/rooms/${encodeURIComponent(room.room_id)}/stop`, { method: "POST" });
  } catch (error) {
    showToast(error.message || "停不下来", "error");
  }
}

async function skipRoomSpeaker() {
  const room = currentRoom();
  if (!room) return;
  try {
    await apiRequest(`/api/rooms/${encodeURIComponent(room.room_id)}/skip`, { method: "POST" });
  } catch (error) {
    showToast(error.message || "跳过失败", "error");
  }
}

function manageRoomParticipants(room) {
  if (!room || room.messages?.length || room.status?.running) return;
  openRoomDialog(room);
}

function buildStatusBar(room) {
  const status = room.status || {};
  const bar = document.createElement("div");
  bar.className = "room-status";
  bar.id = "roomStatusBar";
  const names = (room.participants || []).map((item) => item.label).join("、");
  const text = document.createElement("span");
  if (status.running) {
    const speaking = participantOf(room, status.speaking);
    const pending = Number(status.pending_rounds) || 0;
    text.textContent = speaking
      ? `${speaking.label} ${status.skipping ? "正在跳过" : "正在说"}${pending ? `，后面还排着 ${pending} 轮` : ""}`
      : "准备开始这一轮…";
    if (speaking && !status.skipping) {
      const skip = document.createElement("button");
      skip.type = "button";
      skip.className = "secondary-button room-skip";
      skip.textContent = "跳过";
      skip.addEventListener("click", skipRoomSpeaker);
      bar.appendChild(skip);
    }
    const stop = document.createElement("button");
    stop.type = "button";
    stop.className = "secondary-button room-stop";
    stop.textContent = "停止";
    stop.addEventListener("click", stopRoom);
    bar.prepend(text);
    bar.appendChild(stop);
  } else {
    text.textContent = `聊天室：${names}。你说一句，大家按顺序各回一次。`;
    bar.appendChild(text);
    if (!room.messages?.length) {
      const manage = document.createElement("button");
      manage.type = "button";
      manage.className = "secondary-button room-manage-participants";
      manage.textContent = "管理发言人";
      manage.addEventListener("click", () => manageRoomParticipants(room));
      bar.appendChild(manage);
    }
  }
  return bar;
}

function scrollToEnd() {
  window.requestAnimationFrame(() => {
    elements.chatScroll.scrollTop = elements.chatScroll.scrollHeight;
  });
}

export function renderRoom() {
  const room = currentRoom();
  if (!room) return;
  elements.loadingState.hidden = true;
  elements.blockedState.hidden = true;
  elements.emptyState.hidden = true;
  elements.timeline.hidden = false;
  elements.timeline.replaceChildren();
  for (const message of room.messages || []) {
    const node = buildMessageNode(room, message);
    if (node) elements.timeline.appendChild(node);
  }
  for (const [runId, live] of roomState.live) {
    const node = buildParticipantMessage(room, live.participant_id, live.text, { live: true });
    node.dataset.runId = runId;
    elements.timeline.appendChild(node);
  }
  elements.timeline.appendChild(buildStatusBar(room));
  scrollToEnd();
}

function refreshStatusBar() {
  const room = currentRoom();
  const old = document.getElementById("roomStatusBar");
  if (!room || !old) return;
  old.replaceWith(buildStatusBar(room));
}

function updateLiveNode(room, runId) {
  const live = roomState.live.get(runId);
  const existing = elements.timeline.querySelector(`.room-message.is-live[data-run-id="${CSS.escape(runId)}"]`);
  const node = buildParticipantMessage(room, live.participant_id, live.text, { live: true });
  node.dataset.runId = runId;
  if (existing) existing.replaceWith(node);
  else document.getElementById("roomStatusBar")?.before(node);
  scrollToEnd();
}

function handleRoomEvent(name, data) {
  const room = currentRoom();
  if (!room || String(data?.room_id) !== String(room.room_id)) return;
  if (name === "room.message") {
    const message = data.message;
    if (!message || (room.messages || []).some((item) => item.message_id === message.message_id)) return;
    room.messages = [...(room.messages || []), message];
    if (message.run_id) roomState.live.delete(String(message.run_id));
    renderRoom();
  } else if (name === "room.delta") {
    const runId = String(data.run_id || "");
    const live = roomState.live.get(runId) || { participant_id: data.participant_id, text: "" };
    live.text += String(data.delta || "");
    roomState.live.set(runId, live);
    updateLiveNode(room, runId);
  } else if (name === "room.delta_reset") {
    const live = roomState.live.get(String(data.run_id || ""));
    if (live) {
      live.text = "";
      updateLiveNode(room, String(data.run_id));
    }
  } else if (name === "room.status") {
    room.status = data.status || {};
    // 发言结束但没产出消息（被取消）时，残留的流式气泡要收掉。
    const speakingRun = room.status.run_id ? String(room.status.run_id) : "";
    let pruned = false;
    for (const runId of [...roomState.live.keys()]) {
      if (runId !== speakingRun) {
        roomState.live.delete(runId);
        pruned = true;
      }
    }
    if (pruned) renderRoom();
    else refreshStatusBar();
  } else if (name === "room.participants") {
    if (Array.isArray(data.participants)) room.participants = data.participants;
    renderRoom();
  }
}

/// 输入框在房间里按发送：返回 true 表示已处理。
export async function submitRoom() {
  const room = currentRoom();
  if (!room) return false;
  const content = elements.composerInput.value.trim();
  if (!content) return true;
  if (state.composerAttachments.length) {
    showToast("聊天室暂时不支持附件", "error");
    return true;
  }
  elements.composerInput.value = "";
  elements.composerInput.dispatchEvent(new Event("input"));
  try {
    await apiRequest(`/api/rooms/${encodeURIComponent(room.room_id)}/messages`, {
      method: "POST",
      body: JSON.stringify({ content })
    });
  } catch (error) {
    elements.composerInput.value = content;
    elements.composerInput.dispatchEvent(new Event("input"));
    showToast(error.message || "发送失败", "error");
  }
  return true;
}

// —— 建房对话框 ——

function buildCandidateRow(candidate, index, selectedIds, managing) {
  const row = document.createElement("label");
  row.className = "pop-dialog-row room-candidate";
  const check = document.createElement("input");
  check.type = "checkbox";
  check.checked = selectedIds ? selectedIds.has(candidate.participant_id) : true;
  check.dataset.index = String(index);
  const copy = document.createElement("span");
  copy.className = "room-candidate-copy";
  const title = document.createElement("strong");
  title.textContent = candidate.label;
  copy.appendChild(title);
  row.append(check, makeRoomAvatar(candidate), copy);
  if (Array.isArray(candidate.models) && candidate.models.length) {
    const select = document.createElement("select");
    select.className = "room-candidate-model";
    select.dataset.index = String(index);
    select.setAttribute("aria-label", `${candidate.label} 用哪个模型`);
    for (const model of candidate.models) {
      const option = document.createElement("option");
      option.value = model;
      option.textContent = model;
      option.selected = model === candidate.model;
      select.appendChild(option);
    }
    select.disabled = Boolean(managing && candidate.isCurrent);
    if (candidate.unavailable) select.title = "供应商已停用；可以移除该发言人";
    else if (select.disabled) select.title = "已有成员的模型保持不变";
    select.addEventListener("click", (event) => event.stopPropagation());
    row.appendChild(select);
  } else {
    const hint = document.createElement("span");
    hint.className = "room-model";
    hint.textContent = candidate.kind === "persona" ? "用她当前的模型" : "";
    row.appendChild(hint);
  }
  return row;
}

function ensureDialog() {
  let dialog = document.getElementById("roomDialog");
  if (dialog) return dialog;
  dialog = document.createElement("dialog");
  dialog.className = "reset-dialog pop-dialog room-dialog";
  dialog.id = "roomDialog";
  dialog.setAttribute("aria-labelledby", "roomDialogTitle");
  const form = document.createElement("form");
  form.method = "dialog";
  const header = document.createElement("header");
  const headerCopy = document.createElement("div");
  const title = document.createElement("h2");
  title.id = "roomDialogTitle";
  title.textContent = "新建聊天室";
  const intro = document.createElement("p");
  intro.id = "roomDialogIntro";
  intro.textContent = "勾选谁进来。你每说一句，大家按这里的顺序各回一次。房间里只聊天，不开工具。";
  headerCopy.append(title, intro);
  header.appendChild(headerCopy);
  const name = document.createElement("input");
  name.type = "text";
  name.id = "roomDialogName";
  name.className = "room-dialog-name";
  name.placeholder = "房间名（可不填）";
  name.maxLength = 80;
  const list = document.createElement("div");
  list.className = "pop-dialog-list";
  list.id = "roomDialogList";
  const footer = document.createElement("footer");
  const cancel = document.createElement("button");
  cancel.className = "secondary-button";
  cancel.type = "submit";
  cancel.value = "cancel";
  cancel.textContent = "取消";
  const confirm = document.createElement("button");
  confirm.className = "primary-button";
  confirm.type = "button";
  confirm.id = "roomDialogConfirm";
  confirm.textContent = "进入聊天室";
  footer.append(cancel, confirm);
  form.append(header, name, list, footer);
  dialog.appendChild(form);
  document.body.appendChild(dialog);
  return dialog;
}

async function openRoomDialog(room = null) {
  let candidates = [];
  try {
    const response = await apiRequest("/api/rooms/candidates");
    candidates = (await response.json())?.candidates || [];
  } catch (error) {
    showToast(error.message || "读取参与者失败", "error");
    return;
  }
  const dialog = ensureDialog();
  const managing = Boolean(room);
  const selectedIds = managing
    ? new Set((room.participants || []).map((participant) => String(participant.participant_id)))
    : null;
  if (managing) {
    const existing = new Map((room.participants || []).map((participant) => [
      String(participant.participant_id),
      participant
    ]));
    for (const candidate of candidates) {
      const participant = existing.get(String(candidate.participant_id));
      if (participant) {
        candidate.model = participant.model;
        candidate.isCurrent = true;
        if (participant.model && !candidate.models.includes(participant.model)) {
          candidate.models.push(participant.model);
        }
      }
    }
    const availableIds = new Set(candidates.map((candidate) => String(candidate.participant_id)));
    for (const participant of room.participants || []) {
      if (availableIds.has(String(participant.participant_id))) continue;
      candidates.push({
        kind: participant.kind,
        participant_id: participant.participant_id,
        provider_id: participant.provider_id,
        label: participant.label,
        model: participant.model,
        models: participant.model ? [participant.model] : [],
        isCurrent: true,
        unavailable: true
      });
    }
  }
  dialog.querySelector("#roomDialogTitle").textContent = managing ? "管理发言人" : "新建聊天室";
  dialog.querySelector("#roomDialogIntro").textContent = managing
    ? "首条消息发出前可以增删发言人，已有成员的模型保持不变。"
    : "勾选谁进来。你每说一句，大家按这里的顺序各回一次。房间里只聊天，不开工具。";
  dialog.querySelector("#roomDialogName").hidden = managing;
  const list = dialog.querySelector("#roomDialogList");
  list.replaceChildren(...candidates.map((candidate, index) => buildCandidateRow(candidate, index, selectedIds, managing)));
  if (!managing && candidates.length < 2) {
    const hint = document.createElement("p");
    hint.className = "pop-dialog-empty";
    hint.textContent = "还没有启用的 CLI 中转线。先在「供应商和模型」里启用 Claude Code、Codex、Antigravity 或 Cline，再来拉它们进群。";
    list.appendChild(hint);
  }
  const confirm = dialog.querySelector("#roomDialogConfirm");
  confirm.textContent = managing ? "保存成员" : "进入聊天室";
  confirm.onclick = async () => {
    const selectedCandidates = candidates.filter((candidate, index) =>
      list.querySelector(`input[data-index="${index}"]`)?.checked
    );
    const orderedCandidates = managing
      ? (() => {
          const selectedById = new Map(selectedCandidates.map((candidate) => [
            String(candidate.participant_id),
            candidate
          ]));
          const currentIds = new Set((room.participants || []).map((participant) =>
            String(participant.participant_id)
          ));
          return [
            ...(room.participants || [])
              .map((participant) => selectedById.get(String(participant.participant_id)))
              .filter(Boolean),
            ...selectedCandidates.filter((candidate) => !currentIds.has(String(candidate.participant_id)))
          ];
        })()
      : selectedCandidates;
    const participants = orderedCandidates.map((candidate) => {
      const index = candidates.indexOf(candidate);
      return {
        kind: candidate.kind,
        provider_id: candidate.provider_id,
        model: list.querySelector(`select[data-index="${index}"]`)?.value || candidate.model
      };
    });
    if (!participants.length) {
      showToast("至少保留一位发言人", "error");
      return;
    }
    if (participants.length > 6) {
      showToast("最多选择 6 位发言人", "error");
      return;
    }
    confirm.disabled = true;
    try {
      if (managing) {
        const response = await apiRequest(`/api/rooms/${encodeURIComponent(room.room_id)}/participants`, {
          method: "PUT",
          body: JSON.stringify({
            expected_participant_ids: (room.participants || []).map((participant) => participant.participant_id),
            participants
          })
        });
        const payload = await response.json();
        if (payload?.room) {
          room.participants = payload.room.participants || room.participants;
          room.messages = payload.room.messages || room.messages;
          room.status = payload.room.status || room.status;
          renderRoom();
        }
      } else {
        const response = await apiRequest("/api/rooms", {
          method: "POST",
          body: JSON.stringify({ name: dialog.querySelector("#roomDialogName").value, participants })
        });
        const payload = await response.json();
        await refreshSessions();
        await openSessionView(String(payload?.session?.session_id || ""));
      }
      dialog.close();
    } catch (error) {
      showToast(error.message || (managing ? "更新发言人失败" : "建房失败"), "error");
    } finally {
      confirm.disabled = false;
    }
  };
  dialog.showModal();
}

export async function openCreateRoomDialog() {
  await openRoomDialog();
}

export function initRoom() {
  state.roomRenderer = renderRoom;
  state.roomSubmit = submitRoom;
  window.addEventListener("gqy:room-event", (event) => handleRoomEvent(event.detail?.name, event.detail?.data));
  const button = elements.newRoomButton;
  button?.addEventListener("click", openCreateRoomDialog);
}
