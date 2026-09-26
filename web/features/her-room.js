import { elements } from "../state/elements.js";
import { state } from "../state/store.js";

/// 侧栏「她的房间」里会变的两行:按时辰说的一句话,和「相识第 N 天」。
///
/// 问候按本机时间分五段,口吻是她对你说的,不是界面对用户说的。相识天数从
/// 最早的那个会话算起(两种模式都算,终端车道也算——那也是在一起的日子)。
const GREETINGS = [
  { until: 5, text: "这么晚了,还不睡吗" },
  { until: 10, text: "早呀,今天也要好好吃早饭" },
  { until: 14, text: "午安,记得歇一会儿" },
  { until: 18, text: "下午好,我一直在这儿" },
  { until: 23, text: "晚上好,今天辛苦了" },
  { until: 24, text: "这么晚了,还不睡吗" }
];
const DAY_MS = 24 * 60 * 60 * 1000;

export function greetingFor(date = new Date()) {
  const hour = date.getHours();
  return GREETINGS.find((entry) => hour < entry.until)?.text || GREETINGS[0].text;
}

function firstMetAt() {
  let earliest = Infinity;
  for (const session of state.sessions) {
    const time = Date.parse(session?.created_at || "");
    if (Number.isFinite(time) && time < earliest) earliest = time;
  }
  return Number.isFinite(earliest) ? earliest : null;
}

/// 相识天数(按自然日,第一天算 1)与起点;没有会话时为 null。首页的纪念日卡也用它。
export function daysTogether() {
  const since = firstMetAt();
  if (since == null) return null;
  const start = new Date(since);
  start.setHours(0, 0, 0, 0);
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  return { days: Math.max(1, Math.round((today - start) / DAY_MS) + 1), since: start };
}

export function syncHerRoom() {
  if (elements.herRoomStatus) elements.herRoomStatus.textContent = greetingFor();
  if (!elements.herRoomDays) return;
  // 按自然日算:昨晚认识、今早打开,就是第 2 天。
  const together = daysTogether();
  if (!together) {
    elements.herRoomDays.hidden = true;
    return;
  }
  elements.herRoomDays.textContent = `相识第 ${together.days} 天`;
  elements.herRoomDays.hidden = false;
}

/// 问候要跟着钟走:每十分钟看一眼,跨了时段就换一句。
export function startHerRoomClock() {
  syncHerRoom();
  window.setInterval(() => {
    if (!document.hidden) syncHerRoom();
  }, 10 * 60 * 1000);
}
