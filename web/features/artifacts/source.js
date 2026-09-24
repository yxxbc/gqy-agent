import { makeIconSlot } from "../../core/icons.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

export async function loadArtifactSource(artifact) {
  const version = `${artifact.url}|${artifact.updated_at || ""}`;
  const cached = state.artifactSourceCache.get(artifact.id);
  if (cached?.version === version) return cached.text;
  const response = await fetch(artifact.url, { credentials: "same-origin", cache: "no-store" });
  if (!response.ok) throw new Error("文件载入失败");
  const text = await response.text();
  state.artifactSourceCache.set(artifact.id, { version, text });
  return text;
}

export function artifactLoadingNode() {
  const loading = document.createElement("div");
  loading.className = "artifact-loading";
  loading.append(makeIconSlot("loader-circle", "is-spinning"));
  return loading;
}

export function renderArtifactFailure(error, token) {
  if (token !== state.artifactRenderToken) return;
  const failure = document.createElement("div");
  failure.className = "artifact-failure";
  failure.append(makeIconSlot("circle-alert"), document.createTextNode(error?.message || "文件载入失败"));
  elements.artifactView.replaceChildren(failure);
}

/**
 * 源码视图的高亮语言。Prism 只打包了那十来门（拼装顺序写在
 * `web/vendor/prism/prism.min.js` 头部），认不出来的传空字符串，
 * `paint` 会原样留纯文本——正文缺一块颜色无所谓，缺一个字不行。
 */
export const ARTIFACT_SOURCE_LANGUAGES = {
  html: "markup", htm: "markup", xml: "markup", svg: "markup",
  css: "css", scss: "css",
  js: "javascript", mjs: "javascript", cjs: "javascript", jsx: "javascript",
  ts: "typescript", tsx: "typescript",
  json: "json", jsonl: "json",
  md: "markdown", markdown: "markdown",
  rs: "rust", py: "python", go: "go", lua: "lua", sql: "sql",
  c: "c", h: "c", cpp: "cpp", cc: "cpp", hpp: "cpp",
  sh: "bash", bash: "bash", zsh: "bash", fish: "bash",
  toml: "toml", yaml: "yaml", yml: "yaml", diff: "diff"
};

export function artifactSourceLanguage(artifact) {
  const name = String(artifact?.name || "");
  const extension = name.includes(".") ? name.split(".").pop().toLowerCase() : "";
  return ARTIFACT_SOURCE_LANGUAGES[extension] || "";
}

/** 表格最多画这么多行。再多浏览器就卡了,剩下的让她去看源码或下载。 */
export const MAX_TABLE_ROWS = 2000;

/**
 * 拆 CSV/TSV。只认最基本的那套规矩：双引号包住的字段里分隔符和换行都算正文，
 * 连着两个双引号是一个字面量引号。够读她导出的表了，不做各家方言兼容。
 */
export function parseDelimited(text, delimiter) {
  const rows = [];
  let row = [];
  let field = "";
  let quoted = false;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    if (quoted) {
      if (char !== '"') { field += char; continue; }
      if (text[index + 1] === '"') { field += '"'; index += 1; continue; }
      quoted = false;
      continue;
    }
    if (char === '"') { quoted = true; continue; }
    if (char === delimiter) { row.push(field); field = ""; continue; }
    if (char === "\r") continue;
    if (char === "\n") { row.push(field); rows.push(row); row = []; field = ""; continue; }
    field += char;
  }
  // 最后一行没有换行收尾也要算,否则整张表少一行。
  if (field.length > 0 || row.length > 0) { row.push(field); rows.push(row); }
  return rows;
}

/**
 * CSV/TSV 画成表格。**纯 DOM，一个 HTML 字符串都不产生**——面板里这份内容
 * 同样出自模型之手，和聊天正文一个待遇（理由见 highlight.js 头注释）。
 * 外壳复用 markdown-body，表格样式就不用再写一套。
 */
export function buildArtifactTable(artifact, text) {
  const tabbed = /\.tsv$/i.test(artifact.name) || artifact.mime.includes("tab-separated");
  const rows = parseDelimited(text, tabbed ? "\t" : ",").filter(
    (row) => row.length > 1 || (row[0] || "").trim() !== ""
  );
  const article = document.createElement("article");
  article.className = "markdown-body artifact-markdown";
  if (rows.length === 0) {
    const note = document.createElement("p");
    note.textContent = "这份表是空的。";
    article.appendChild(note);
    return article;
  }
  const clipped = rows.length > MAX_TABLE_ROWS + 1;
  const body = rows.slice(1, clipped ? MAX_TABLE_ROWS + 1 : rows.length);
  const columns = rows.reduce((most, row) => Math.max(most, row.length), 0);
  const table = document.createElement("table");
  const head = document.createElement("thead");
  const headRow = document.createElement("tr");
  for (let index = 0; index < columns; index += 1) {
    const cell = document.createElement("th");
    cell.textContent = rows[0][index] ?? "";
    headRow.appendChild(cell);
  }
  head.appendChild(headRow);
  const tbody = document.createElement("tbody");
  for (const row of body) {
    const line = document.createElement("tr");
    for (let index = 0; index < columns; index += 1) {
      const cell = document.createElement("td");
      cell.textContent = row[index] ?? "";
      line.appendChild(cell);
    }
    tbody.appendChild(line);
  }
  table.append(head, tbody);
  article.appendChild(table);
  if (clipped) {
    const note = document.createElement("p");
    note.className = "artifact-table-note";
    note.textContent = `表太长，只画了前 ${MAX_TABLE_ROWS} 行，共 ${rows.length - 1} 行。完整内容看源码或下载。`;
    article.appendChild(note);
  }
  return article;
}

export async function renderArtifactSource(artifact, token) {
  let text = await loadArtifactSource(artifact);
  if (token !== state.artifactRenderToken) return;
  if (artifact.kind === "json" || artifact.mime.startsWith("application/json") || /\.json$/i.test(artifact.name)) {
    try { text = JSON.stringify(JSON.parse(text), null, 2); } catch (_) {}
  }
  const source = document.createElement("div");
  source.className = "artifact-source";
  const gutter = document.createElement("div");
  gutter.className = "artifact-line-numbers";
  const lines = text.split("\n");
  for (let index = 0; index < lines.length; index += 1) {
    const number = document.createElement("span");
    number.textContent = String(index + 1);
    gutter.appendChild(number);
  }
  const pre = document.createElement("pre");
  pre.className = "artifact-code";
  const code = document.createElement("code");
  code.textContent = text;
  // 聊天正文里的代码块一直有高亮,这边却是一片纯白——同一个组件接上就是了。
  // 这份内容已经完整(不是流式),所以 settled=true,当场上色。
  window.GqyHighlight?.paint(code, artifactSourceLanguage(artifact), text, true);
  pre.appendChild(code);
  source.append(gutter, pre);
  elements.artifactView.replaceChildren(source);
}
