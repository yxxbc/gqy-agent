import { asFiniteNumber, formatInteger, formatTokens } from "../core/format.js";
import { elements } from "../state/elements.js";
import { state } from "../state/store.js";

/// 只有本模块用的状态（从 state/store.js 分出来的私有分片）。
const statusState = {
  connection: "connecting"
};

export function setConnectionStatus(status) {
  statusState.connection = status;
  const definitions = {
    online: { sidebar: "在线", className: "" },
    connecting: { sidebar: "重连中", className: "is-connecting" },
    offline: { sidebar: "离线", className: "is-offline" },
    blocked: { sidebar: "未授权", className: "is-blocked" }
  };
  const selected = definitions[status] || definitions.connecting;
  elements.sidebarConnectionStatus.textContent = selected.sidebar;
  elements.sidebarStatusDot.classList.remove("is-connecting", "is-offline", "is-blocked");
  if (selected.className) elements.sidebarStatusDot.classList.add(selected.className);
}

export function updateContext() {
  const tokens = Math.max(0, asFiniteNumber(state.context?.tokens));
  const windowSize = state.context?.window == null ? null : Math.max(0, asFiniteNumber(state.context.window));
  if (elements.contextNumbers) {
    elements.contextNumbers.textContent = windowSize ? `${formatTokens(tokens)} / ${formatTokens(windowSize)}` : `${formatTokens(tokens)} / --`;
  }
  const percent = windowSize > 0 ? Math.min(100, Math.max(0, (tokens / windowSize) * 100)) : 0;
  // 上下文占用画成一个小圆环(比长条优雅,用户反馈原展示不美观):r=9,周长≈56.55,
  // 按占用比例设 dashoffset;高/临界用配色区分。
  if (elements.contextRing) {
    const circ = 2 * Math.PI * 9;
    elements.contextRing.style.strokeDasharray = `${circ.toFixed(2)}`;
    elements.contextRing.style.strokeDashoffset = `${(circ * (1 - percent / 100)).toFixed(2)}`;
  }
  if (elements.contextTrack) {
    elements.contextTrack.setAttribute("aria-label", windowSize ? `上下文使用 ${Math.round(percent)}%,点击查看分项` : `上下文 ${formatInteger(tokens)} tokens,点击查看分项`);
    elements.contextTrack.classList.toggle("is-high", percent >= 75 && percent < 90);
    elements.contextTrack.classList.toggle("is-critical", percent >= 90);
  }
  // 分项弹窗开着时跟着重算,换了会话就关掉(contextpanel.js)。
  window.GqyContextPanel?.contextChanged();
}

// 输入框下方信息行的「每秒 toks」「累计」:取最新一轮的样本,回合结束/round_usage 时更新。
export function setComposerUsage({ speed, cumulative } = {}) {
  // undefined = 不动这一项(只想刷累计时别把速度顺手藏了);null/"" = 清空隐藏。
  if (elements.composerSpeed && speed !== undefined) {
    if (speed) {
      elements.composerSpeedValue.textContent = speed;
      elements.composerSpeed.hidden = false;
    } else {
      elements.composerSpeed.hidden = true;
    }
  }
  if (elements.composerCumulative && cumulative !== undefined) {
    if (cumulative) {
      elements.composerCumulativeValue.textContent = cumulative;
      elements.composerCumulative.hidden = false;
    } else {
      elements.composerCumulative.hidden = true;
    }
  }
}

export function updateRuntimeUsage() {}

export function updateCapabilities() {
  const values = [
    ["会话", state.capabilities?.multi_conversation ? "多会话" : "当前单一对话"],
    ["附件", state.capabilities?.attachments ? "可用" : "不可用"],
    ["消息队列", state.capabilities?.queue ? "可用" : "不可用"]
  ];
  elements.capabilityList.replaceChildren();
  for (const [name, value] of values) {
    const row = document.createElement("div");
    const term = document.createElement("dt");
    const description = document.createElement("dd");
    term.textContent = name;
    description.textContent = value;
    row.append(term, description);
    elements.capabilityList.appendChild(row);
  }
}
