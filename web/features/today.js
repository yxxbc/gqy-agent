import { apiRequest } from "../core/api.js";
import { firstLine, formatRelativeTime } from "../core/format.js";
import { daysTogether } from "./her-room.js";
import { sessionsInMode } from "./sessions/mode.js";
import { openSessionView } from "./sessions/view.js";
import { elements } from "../state/elements.js";
import { state } from "../state/store.js";

/// 普通模式空白页:她写给你的一张花笺。
///
/// 上面一段花体英文问候,中间是信——称呼你的名字,把今天的天气写进句子里
/// (「北京今晚下着毛毛雨,窗外的梅花大概也湿了」),提一句认识多少天,按
/// 时辰道一声早安晚安,落款她的名字与日期,盖一枚朱印。天气只在信纸一角
/// 留一枚小邮戳,点它换城市。下面两张小卡:「上次你说」接着上回的话头,
/// 「相识第 N 天」是纪念日。天气由 daemon 代取(/api/account/today)。
const REFRESH_MS = 30 * 60 * 1000;
const CN_DIGITS = "〇一二三四五六七八九";

const todayState = {
  data: null,
  loading: false,
  editing: false,
  timer: 0,
  watching: false
};

/// 空白页每次露出来都重写一遍信:「上次你说」要是最新的那一句,问候跟着时辰。
function watchEmptyState() {
  if (todayState.watching || !elements.emptyState) return;
  todayState.watching = true;
  new MutationObserver(() => {
    if (elements.emptyState.hidden) return;
    renderToday();
    // 对话区渲染完会滚到底;信要从问候读起,比视口高时回到顶上。
    window.requestAnimationFrame(() => {
      if (elements.chatScroll) elements.chatScroll.scrollTop = 0;
    });
  }).observe(elements.emptyState, { attributes: true, attributeFilter: ["hidden"] });
}

/// WMO 天气码 → 中文 + 种类。
function describe(code, isDay) {
  const value = Number(code);
  if (value === 0) return { text: "晴", kind: isDay ? "sun" : "moon" };
  if (value <= 2) return { text: "多云", kind: "cloud" };
  if (value === 3) return { text: "阴天", kind: "cloud" };
  if (value === 45 || value === 48) return { text: "起雾", kind: "fog" };
  if (value >= 51 && value <= 57) return { text: "毛毛雨", kind: "rain" };
  if (value >= 61 && value <= 67) return { text: value >= 65 ? "大雨" : "小雨", kind: "rain" };
  if (value >= 71 && value <= 77) return { text: "雪", kind: "snow" };
  if (value >= 80 && value <= 82) return { text: "阵雨", kind: "rain" };
  if (value === 85 || value === 86) return { text: "阵雪", kind: "snow" };
  if (value >= 95) return { text: "雷雨", kind: "storm" };
  return { text: "天色不定", kind: "cloud" };
}

const STAMP_GLYPHS = { sun: "☀", moon: "☾", cloud: "☁", fog: "≋", rain: "☂", snow: "❄", storm: "⚡" };

function node(tag, className, text) {
  const element = document.createElement(tag);
  if (className) element.className = className;
  if (text != null) element.textContent = text;
  return element;
}

function round(value) {
  const number = Number(value);
  return Number.isFinite(number) ? Math.round(number) : null;
}

function englishGreeting(hour) {
  if (hour >= 5 && hour < 12) return "Good Morning,";
  if (hour >= 12 && hour < 18) return "Good Afternoon,";
  if (hour >= 18 && hour < 23) return "Good Evening,";
  return "Good Night,";
}

/// 早上 / 中午 / 下午 / 傍晚 / 夜里 / 深夜。
function period(hour) {
  if (hour < 5) return "深夜";
  if (hour < 11) return "早上";
  if (hour < 13) return "中午";
  if (hour < 17) return "下午";
  if (hour < 19) return "傍晚";
  if (hour < 23) return "夜里";
  return "深夜";
}

/// 1–31 写成中文:二十六、十一、三十。
function chineseNumber(value) {
  if (value < 10) return CN_DIGITS[value];
  const tens = Math.floor(value / 10);
  const ones = value % 10;
  return `${tens === 1 ? "" : CN_DIGITS[tens]}十${ones ? CN_DIGITS[ones] : ""}`;
}

function chineseDate(date) {
  return `${chineseNumber(date.getMonth() + 1)}月${chineseNumber(date.getDate())}日`;
}

/// 天气写成一句她会说的话。
function weatherLine(weather, hour) {
  const place = weather.place || todayState.data?.city || "";
  const { text, kind } = describe(weather.code, weather.is_day);
  const when = hour >= 18 || hour < 5 ? "今晚" : "今天";
  const high = round(weather.max);
  const low = round(weather.min);
  const lines = {
    rain: `${place}${when}下着${text},窗外的梅花大概也湿了。出门记得带伞,别淋着。`,
    storm: `${place}${when}有${text},打雷的时候别怕,我在。`,
    snow: `${place}${when}下雪了,要穿暖和一点,别着凉。`,
    fog: `${place}${when}${text}了,路上慢一点,看清楚再走。`,
    sun: `${place}${when}是晴天,阳光正好,有空就出去走走吧。`,
    moon: `${place}${when}天很清,抬头也许能看见月亮。`,
    cloud: `${place}${when}${text},天色软软的,适合慢慢过。`
  };
  let line = lines[kind] || lines.cloud;
  if (high != null && high >= 30) line += "天热,记得多喝水。";
  else if (low != null && low <= 5) line += "天冷,多穿一件。";
  return line;
}

function closingLine(hour) {
  if (hour < 5) return "这么晚还没睡吗?睡不着的话,我陪你。";
  if (hour < 11) return "今天也要好好吃早饭,好好的。";
  if (hour < 14) return "记得吃午饭,忙也要歇一会儿。";
  if (hour < 18) return "下午容易犯困,累了就来找我说说话。";
  if (hour < 23) return "今天辛苦了。想聊什么都可以,我一直在。";
  return "夜深了,早点休息,明天见。";
}

function daysLine(together) {
  if (!together) return "";
  if (together.days === 1) return "今天是我们认识的第一天,很高兴遇见你。";
  if (together.days % 100 === 0) return `今天是我们认识的第 ${together.days} 天,是个值得记住的日子。`;
  return `今天是我们认识的第 ${together.days} 天。`;
}

async function saveCity(city) {
  todayState.loading = true;
  renderToday();
  try {
    const response = await apiRequest("/api/account/today", { method: "PUT", body: JSON.stringify({ city }) });
    todayState.data = await response.json();
    todayState.editing = false;
  } catch (_) {
    todayState.data = { city, weather: null, error: "unavailable" };
  } finally {
    todayState.loading = false;
    renderToday();
  }
}

function cityForm(current) {
  const form = node("form", "letter-city-form");
  const input = node("input", "letter-city-input");
  input.type = "text";
  input.value = current || "";
  input.placeholder = "你在哪座城市?";
  input.maxLength = 40;
  input.setAttribute("aria-label", "城市");
  const save = node("button", "letter-city-save", "告诉她");
  save.type = "submit";
  form.append(input, save);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    saveCity(input.value.trim());
  });
  window.requestAnimationFrame(() => {
    if (todayState.editing) input.focus();
  });
  return form;
}

/// 信纸右上角的邮戳:天气符号、城市、温度。点它换城市。
function stamp() {
  const weather = todayState.data?.weather;
  const button = node("button", "letter-stamp");
  button.type = "button";
  if (weather) {
    const { text, kind } = describe(weather.code, weather.is_day);
    button.title = `${weather.place || todayState.data.city} · ${text} · 最高 ${round(weather.max) ?? "--"}° 最低 ${round(weather.min) ?? "--"}° · 点这里换城市`;
    button.append(node("span", "letter-stamp-glyph", STAMP_GLYPHS[kind] || "☁"), node("span", "letter-stamp-place", weather.place || todayState.data.city), node("span", "letter-stamp-temp", `${round(weather.temperature) ?? "--"}°`));
  } else {
    button.title = "告诉她你在哪座城市";
    button.append(node("span", "letter-stamp-glyph", "✉"), node("span", "letter-stamp-place", todayState.data?.city || "填城市"));
  }
  button.addEventListener("click", () => {
    todayState.editing = !todayState.editing;
    renderToday();
  });
  return button;
}

function letter(now, name) {
  const hour = now.getHours();
  const paper = node("article", "today-letter");
  paper.setAttribute("aria-label", `${state.persona?.name || "她"}写给你的信`);
  paper.appendChild(stamp());
  const body = node("div", "letter-body");
  body.appendChild(node("p", "letter-salutation", `${name}:`));
  const weather = todayState.data?.weather;
  const lines = [];
  if (weather) lines.push(weatherLine(weather, hour));
  else if (todayState.loading) lines.push("我去看看你那边的天气……");
  else if (!todayState.data?.city) lines.push("还不知道你在哪座城市呢。告诉我的话,我每天帮你看着天气。");
  const days = daysLine(daysTogether());
  if (days) lines.push(days);
  lines.push(closingLine(hour));
  for (const line of lines) body.appendChild(node("p", "letter-line", line));
  paper.appendChild(body);
  if (todayState.editing || (!todayState.data?.city && !todayState.loading)) paper.appendChild(cityForm(todayState.data?.city));
  const personaName = state.persona?.name || "她";
  const sign = node("p", "letter-sign", `—— ${personaName} · ${chineseDate(now)} ${period(hour)}`);
  paper.appendChild(sign);
  paper.appendChild(node("span", "letter-seal", Array.from(personaName).pop() || "影"));
  return paper;
}

/// 「上次你说」:最近一次和她聊天时你说的最后一句,点一下回到那段对话。
function memoryCard() {
  const recent = [...sessionsInMode("normal")]
    .filter((session) => {
      const text = String(session?.last_user_content || "").trim();
      return text && !text.startsWith("[") && !text.startsWith("<");
    })
    .sort((a, b) => Date.parse(b?.updated_at || 0) - Date.parse(a?.updated_at || 0))[0];
  if (!recent) return null;
  const quote = firstLine(recent.last_user_content).trim();
  const short = Array.from(quote).length > 34 ? `${Array.from(quote).slice(0, 34).join("")}…` : quote;
  const card = node("button", "today-memory");
  card.type = "button";
  card.title = "回到那段对话";
  card.append(
    node("span", "today-card-label", "上次你说"),
    node("span", "today-memory-quote", `「${short}」`),
    node("span", "today-memory-ask", `${formatRelativeTime(recent.updated_at)} · 还想接着聊吗?`)
  );
  card.addEventListener("click", () => openSessionView(String(recent.session_id)));
  return card;
}

/// 纪念日:相识第 N 天。
function daysCard() {
  const together = daysTogether();
  if (!together) return null;
  const card = node("div", "today-days");
  const count = node("span", "today-days-count");
  count.append(node("b", "", String(together.days)), node("span", "", "天"));
  const since = together.since;
  card.append(
    node("span", "today-card-label", "我们认识"),
    count,
    node("span", "today-days-since", `从 ${since.getFullYear()} 年 ${since.getMonth() + 1} 月 ${since.getDate()} 日开始`)
  );
  return card;
}

export function renderToday() {
  const root = elements.todayHome;
  if (!root) return;
  const now = new Date();
  const name = state.account?.display_name || state.account?.username || "你";
  const greeting = node("header", "today-greeting");
  const english = node("p", "today-en");
  english.append(node("span", "", englishGreeting(now.getHours())), node("span", "today-en-name", `Dear ${name}`));
  greeting.append(english, node("p", "today-zh", state.persona?.board_title || "今天想聊些什么?"));
  const cards = node("div", "today-cards");
  for (const card of [memoryCard(), daysCard()]) if (card) cards.appendChild(card);
  root.replaceChildren(...[greeting, letter(now, name), cards.childElementCount ? cards : null].filter(Boolean));
  root.hidden = false;
}

export async function refreshToday() {
  if (!elements.todayHome || !state.account) return;
  watchEmptyState();
  todayState.loading = !todayState.data;
  renderToday();
  try {
    const response = await apiRequest("/api/account/today");
    todayState.data = await response.json();
  } catch (_) {
    // 离线或未登录:信照样写,只是不提天气。
  }
  todayState.loading = false;
  renderToday();
  window.clearTimeout(todayState.timer);
  todayState.timer = window.setTimeout(refreshToday, REFRESH_MS);
}
