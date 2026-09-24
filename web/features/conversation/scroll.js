import { NEAR_BOTTOM_PX, PROGRAMMATIC_SCROLL_AUTO_MS, PROGRAMMATIC_SCROLL_MS } from "../../core/constants.js";
import { liveViewed } from "./chrome.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

export function updateJumpButtonOffset() {
  elements.jumpBottomButton.style.bottom = `${elements.composerDock.offsetHeight + 10}px`;
}

export function isNearBottom() {
  const distance = elements.chatScroll.scrollHeight - elements.chatScroll.scrollTop - elements.chatScroll.clientHeight;
  return distance <= NEAR_BOTTOM_PX;
}

export function isAtBottom() {
  const distance = elements.chatScroll.scrollHeight - elements.chatScroll.scrollTop - elements.chatScroll.clientHeight;
  return distance <= 2;
}

export function suspendOutputFollowing() {
  state.followOutput = false;
  elements.jumpBottomButton.hidden = false;
}

/// 同一帧里的多次滚动请求合并成一个 rAF。
///
/// 以前每次调用都 `++scrollRequestId`,后一次会把前一次已排队的 rAF 作废;
/// 流稳定时渲染回调与滚动回调挤在同一帧里,前一个请求被后一个作废、后一个
/// 又被下一条 delta 作废,滚动被连续饿死,某一帧放过去就整段下跳。现在排队
/// 的是「这一帧要不要滚」这件事本身,重复请求只是把 smooth 抬上去。
export let scrollFrame = 0;

export let scrollFrameSmooth = false;

export let scrollFrameForce = false;

export let programmaticScrollTimer = 0;

// smooth 动画会连发多条 scroll 事件,守卫不能只吃第一条——否则第二条就被
// 当成用户上滚,把「回到底部」的动画中途关掉跟随。
export let programmaticScrollSmooth = false;

export function scrollToBottom({ force = false, smooth = false } = {}) {
  if (!force && !state.followOutput) {
    elements.jumpBottomButton.hidden = false;
    return;
  }
  if (force) state.followOutput = true;
  scrollFrameSmooth = scrollFrameSmooth || smooth;
  // 已排队的非 force 请求不能把后来的 force 吞掉(点「回到底部」时若同一帧
  // 里正好有一条跟随请求在排队,它记的 force 是 false)。
  scrollFrameForce = scrollFrameForce || force;
  if (scrollFrame) return;
  scrollFrame = window.requestAnimationFrame(() => {
    const smoothNow = scrollFrameSmooth;
    const forceNow = scrollFrameForce;
    scrollFrame = 0;
    scrollFrameSmooth = false;
    scrollFrameForce = false;
    if (!forceNow && !state.followOutput) return;
    state.programmaticScroll = true;
    programmaticScrollSmooth = smoothNow;
    elements.chatScroll.scrollTo({ top: elements.chatScroll.scrollHeight, behavior: smoothNow ? "smooth" : "auto" });
    state.nearBottom = true;
    elements.jumpBottomButton.hidden = true;
    // 守卫由紧随其后的 scroll 事件解除。两种情况没有那条事件可吃:smooth
    // 期间事件被守卫吃掉、动画停下后没有下一条;auto 时视口本来就在底,
    // scrollTo 没动就不派发。两边都补兜底超时,只是长短不同。
    window.clearTimeout(programmaticScrollTimer);
    programmaticScrollTimer = window.setTimeout(() => {
      state.programmaticScroll = false;
      programmaticScrollSmooth = false;
    }, smoothNow ? PROGRAMMATIC_SCROLL_MS : PROGRAMMATIC_SCROLL_AUTO_MS);
  });
}

// anchor(可选):live 对象或 DOM 节点。离屏保活的 live(别会话)或已游离
// 的节点长内容,不该滚动当前视图。
export function contentAdded(anchor) {
  if (anchor) {
    if (anchor.nodeType) {
      if (!anchor.isConnected) return;
    } else if (anchor.runId && !liveViewed(anchor)) return;
  }
  if (state.followOutput) scrollToBottom();
  else elements.jumpBottomButton.hidden = false;
}

/// 标记「接下来这次滚动是程序发起的」:监听器看到守卫就不把它当用户上滚。
/// 非 smooth 滚动由紧随其后的那条 scroll 事件解除;没动(scrollTop 没变)
/// 就不派发事件,靠超时兜底。
export function armProgrammaticScroll() {
  state.programmaticScroll = true;
  programmaticScrollSmooth = false;
  window.clearTimeout(programmaticScrollTimer);
  programmaticScrollTimer = window.setTimeout(() => {
    state.programmaticScroll = false;
  }, PROGRAMMATIC_SCROLL_AUTO_MS);
}
