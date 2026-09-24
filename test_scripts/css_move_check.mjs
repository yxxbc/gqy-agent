#!/usr/bin/env node
/*
 * CSS 归并的安全检查（docs/design/2026-09-24-webui-split.md §5.3、§8 P4）。
 *
 * 把 FROM 文件里的顶层规则挪到 TO 文件末尾（TO 在层叠顺序上更靠前）。一条规则 R
 * 能挪，当且仅当：从 TO 末尾到 R 原位置之间，没有任何规则 S 满足
 *     属性重叠（含简写/长写）且 !important 相同 且 存在一对选择器优先级相等。
 * 优先级不等时胜负与先后无关。@keyframes 只查同名。
 *
 * 唯一的「不会命中同一元素」判断：两条规则的主体（最后一段复合选择器）各自要求
 * 某个类名前缀（dash-、st- …），而这两个前缀出现在互不相交的源文件里——要给元素
 * 加上某个类，那个类名就得出现在加它的代码里，所以两边不可能落在同一个元素上。
 * 其余情况一律按「可能命中」算，只会多拦、不会漏拦。
 *
 * 需要 postcss 与 @bramus/specificity（不是项目依赖）：
 *     NODE_PATH=<node_modules> node test_scripts/css_move_check.mjs web/css FROM.css TO.css [--apply] [--only=前缀,前缀]
 */
import fs from "fs";
import path from "path";
import { createRequire } from "module";

const require = createRequire(import.meta.url);
const postcss = require("postcss");
const { calculate } = require("@bramus/specificity");

const [, , dir, fromName, toName, ...flags] = process.argv;
const apply = flags.includes("--apply");
const only = (flags.find((f) => f.startsWith("--only=")) || "").slice(7).split(",").filter(Boolean);

const files = fs.readdirSync(dir).filter((f) => f.endsWith(".css")).sort();
const fromIndex = files.indexOf(fromName);
const toIndex = files.indexOf(toName);
if (fromIndex < 0 || toIndex < 0 || toIndex >= fromIndex) throw new Error("TO must come before FROM");

const EXPANDS = {
  inset: ["top", "right", "bottom", "left"],
  "place-items": ["align-items", "justify-items"],
  "place-content": ["align-content", "justify-content"],
  "place-self": ["align-self", "justify-self"],
  gap: ["row-gap", "column-gap"],
  font: ["line-height"],
  "grid-area": ["grid-row", "grid-column"],
  "grid-row": ["grid-row-start", "grid-row-end"],
  "grid-column": ["grid-column-start", "grid-column-end"],
  overflow: ["overflow-x", "overflow-y"],
};
const bare = (prop) => prop.toLowerCase().replace(/^-(webkit|moz|ms|o)-/, "");
function overlaps(a, b) {
  a = bare(a); b = bare(b);
  if (a === b || a === "all" || b === "all") return true;
  if (a.startsWith("--") || b.startsWith("--")) return false;
  if (a.startsWith(`${b}-`) || b.startsWith(`${a}-`)) return true;
  return (EXPANDS[a] || []).some((x) => overlaps(x, b)) || (EXPANDS[b] || []).some((x) => overlaps(a, x));
}

const WEB = path.resolve(dir, "..");
const sources = [];
(function scan(d) {
  for (const entry of fs.readdirSync(d, { withFileTypes: true })) {
    const p = path.join(d, entry.name);
    if (entry.isDirectory()) { if (!["vendor", "css"].includes(entry.name)) scan(p); }
    else if (/\.(js|html)$/.test(entry.name)) sources.push([p, fs.readFileSync(p, "utf8")]);
  }
})(WEB);
const nsFiles = new Map();
function filesOf(ns) {
  if (!nsFiles.has(ns)) {
    const re = new RegExp(`(?<![a-z0-9/-])${ns}-[a-z]`); // 前面是 / 的是文件路径（/dash-kb.js），不是类名
    nsFiles.set(ns, new Set(sources.filter(([, text]) => re.test(text)).map(([p]) => p)));
  }
  return nsFiles.get(ns);
}
/// 主体要求的类名前缀集合（最后一段复合选择器里的类）。拿不准就返回空集。
function subjectNamespaces(selector) {
  const out = new Set();
  for (const one of selector.split(",")) {
    const last = one.trim().split(/\s*[\s>+~]\s*/).pop() || "";
    const classes = [...last.replace(/:(not|is|where|has)\([^)]*\)/g, "").matchAll(/\.([a-z][a-z0-9]*)-[a-z0-9-]*/gi)].map((m) => m[1]);
    if (!classes.length) return new Set();
    out.add(classes); // 该选择器要求的前缀（同一元素上全部都得有）
  }
  return out;
}
function provablyDisjoint(r, s) {
  // 每一对（R 的一个选择器, S 的一个选择器）都要能证明不相交
  if (!r.ns.size || !s.ns.size) return false;
  for (const a of r.ns) for (const b of s.ns) {
    const ok = a.some((x) => b.some((y) => x !== y && ![...filesOf(x)].some((f) => filesOf(y).has(f))));
    if (!ok) return false;
  }
  return true;
}

function specs(selector) {
  try {
    return calculate(selector).map((s) => `${s.value.a},${s.value.b},${s.value.c}`);
  } catch (_) {
    return ["?"]; // 算不出来就当与一切相等
  }
}
/// 一个顶层节点的「指纹」：内部所有规则的 (优先级集合, 声明集合)，以及 keyframes 名。
function profile(node) {
  const rules = [];
  const keyframes = [];
  const visit = (n) => {
    if (n.type === "rule") {
      const decls = [];
      n.each((d) => { if (d.type === "decl") decls.push({ prop: d.prop, important: Boolean(d.important) }); });
      rules.push({ specs: new Set(specs(n.selector)), ns: subjectNamespaces(n.selector), decls });
    } else if (n.type === "atrule" && /keyframes$/i.test(n.name)) {
      keyframes.push(n.params.trim());
    } else if (n.nodes) {
      n.each(visit);
    }
  };
  visit(node);
  return { rules, keyframes };
}
function conflicts(p, q) {
  if (p.keyframes.some((k) => q.keyframes.includes(k))) return "same @keyframes name";
  for (const r of p.rules) for (const s of q.rules) {
    const sameSpec = [...r.specs].some((x) => x === "?" || s.specs.has(x) || s.specs.has("?"));
    if (!sameSpec || provablyDisjoint(r, s)) continue;
    for (const a of r.decls) for (const b of s.decls) {
      if (a.important === b.important && overlaps(a.prop, b.prop)) return `${a.prop} vs ${b.prop}`;
    }
  }
  return null;
}

const roots = files.map((f) => postcss.parse(fs.readFileSync(path.join(dir, f), "utf8"), { from: f }));
const label = (n) => (n.type === "rule" ? n.selector : `@${n.name} ${n.params}`).replace(/\s+/g, " ").slice(0, 70);
const firstClass = (n) => ((n.type === "rule" ? n.selector : n.nodes?.find((c) => c.type === "rule")?.selector) || "").match(/[a-z][a-z0-9]*/i)?.[0] || "";

const fromNodes = roots[fromIndex].nodes.filter((n) => n.type !== "comment");
const candidates = fromNodes.filter((n) => !only.length || only.includes(firstClass(n)));
const moving = new Set();
const report = [];
for (const node of fromNodes) {
  if (!candidates.includes(node)) continue;
  const p = profile(node);
  // 途经：TO 之后的整文件 + FROM 中本节点之前、且不随同挪动的节点
  const between = [];
  for (let i = toIndex + 1; i < fromIndex; i++) between.push(...roots[i].nodes.filter((n) => n.type !== "comment"));
  for (const other of fromNodes) {
    if (other === node) break;
    if (!moving.has(other)) between.push(other);
  }
  let reason = null;
  for (const s of between) {
    reason = conflicts(p, profile(s));
    if (reason) { reason = `${reason} with ${label(s)}`; break; }
  }
  if (!reason) moving.add(node);
  report.push(`${reason ? "stay" : "move"}  ${label(node)}${reason ? `  (${reason})` : ""}`);
}
console.log(report.join("\n"));
console.log(`\n${moving.size} of ${candidates.length} rules can move from ${fromName} to ${toName}`);

if (apply && moving.size) {
  // 连同紧贴在规则上方的注释一起挪；原文逐字搬运
  const take = [];
  for (const node of fromNodes) {
    if (!moving.has(node)) continue;
    let current = node;
    let prev = node.prev();
    const lead = [];
    while (prev && prev.type === "comment" && !/\n\s*\n/.test(current.raws.before || "")) {
      lead.unshift(prev);
      current = prev;
      prev = prev.prev();
    }
    for (const c of lead) { take.push(c.toString()); c.remove(); }
    take.push(node.toString());
    node.remove();
  }
  const fromPath = path.join(dir, fromName);
  const toPath = path.join(dir, toName);
  fs.writeFileSync(fromPath, roots[fromIndex].toString());
  const toText = fs.readFileSync(toPath, "utf8").replace(/\n*$/, "\n");
  fs.writeFileSync(toPath, `${toText}\n/* ── 以下由 ${fromName} 归并而来（css_move_check.mjs 验证层叠结果不变） ── */\n${take.join("\n\n")}\n`);
  console.log(`applied: ${take.length} blocks appended to ${toName}`);
}
