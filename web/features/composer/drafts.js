import { safeStorageGet, safeStorageSet } from "../../core/storage.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

/// 输入框草稿按会话各存一份。
///
/// 原来输入框里的字不跟会话走:在 A 打了一半切到 B,那段话原样出现在 B;
/// 普通/开发隔开之后,写给她的话会跟进开发模式。现在每个会话(以及两种
/// 模式各自的空白页)一个 localStorage 键,切换时先存旧的、再取新的,刷新
/// 也不丢。只存文字,附件本来就随会话切换清掉。
const PREFIX = "gqy.web.draft.";
const SAVE_DELAY_MS = 250;

const draftState = {
  boundKey: "",
  saveTimer: 0
};

// 关页面、刷新时还在防抖里的那几个字也要落盘。
window.addEventListener("pagehide", () => flushDraft());

/// 当前视图对应的草稿键:真会话用会话 id,空白页用 new-<模式>。
export function currentDraftKey() {
  if (state.viewSessionId) return `${PREFIX}${state.viewSessionId}`;
  if (state.draftMode) return `${PREFIX}new-${state.draftMode}`;
  return "";
}

function writeDraft(key, text) {
  if (!key) return;
  if (text) safeStorageSet(key, text);
  else {
    try {
      window.localStorage.removeItem(key);
    } catch (_) {
      // 存储不可用时草稿只是不持久,不影响输入。
    }
  }
}

/// 输入框内容变了(打字、发送后清空、命令回填):稍后写回当前键。
export function scheduleDraftSave() {
  if (!draftState.boundKey) draftState.boundKey = currentDraftKey();
  window.clearTimeout(draftState.saveTimer);
  draftState.saveTimer = window.setTimeout(flushDraft, SAVE_DELAY_MS);
}

export function flushDraft() {
  window.clearTimeout(draftState.saveTimer);
  draftState.saveTimer = 0;
  writeDraft(draftState.boundKey, elements.composerInput.value);
}

/// 视图换到了别的会话/空白页:存下旧草稿,换上新视图的草稿。
///
/// carry=true 用在空白页刚落地成真会话的那一刻:输入框里正是要发出去的
/// 那句话,不能被新会话的空草稿顶掉;旧的 new-<模式> 键顺手删掉,否则
/// 下次点新对话还会冒出这句已经发过的话。返回是否换了输入框内容。
export function swapDraft({ carry = false } = {}) {
  const next = currentDraftKey();
  const previous = draftState.boundKey;
  if (next === previous) return false;
  if (carry) {
    writeDraft(previous, "");
    draftState.boundKey = next;
    return false;
  }
  flushDraft();
  draftState.boundKey = next;
  const text = next ? safeStorageGet(next) || "" : "";
  if (elements.composerInput.value === text) return false;
  elements.composerInput.value = text;
  return true;
}
