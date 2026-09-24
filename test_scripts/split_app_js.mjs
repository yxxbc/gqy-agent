#!/usr/bin/env node
/*
 * 把 IIFE 形式的 web/app.js 拆成 ES 模块（docs/design/2026-09-24-webui-split.md §8 P2）。
 *
 * 需要 acorn / acorn-walk（不是项目依赖，临时装在任意目录即可）：
 *     NODE_PATH=<装了 acorn 的 node_modules> node test_scripts/split_app_js.mjs web/app.js split.json
 *
 * split.json：
 *     { "boundaries": [[条目名, 模块路径], ...],   // 按源码顺序，条目归最近的前一个边界
 *       "overrides": { 条目名: 模块路径 },         // 不在所属区段里的零散条目
 *       "entry": "app.js" }
 *
 * 做的事与守的约束：
 *   - 条目原文照搬（含紧贴其上的注释），去掉 IIFE 的一层缩进；模板字符串里的行不动。
 *   - 每个模块导出自己全部的顶层名字，按引用补 import。
 *   - 非声明语句（定时器、监听）收进所在模块的 `start()`，入口按原顺序调用。
 *   - 被重新赋值的 `let` 必须和所有赋值者同模块（ES 模块的导入绑定是只读的）。
 *   - 初始化时就求值的跨模块引用只许指向 core/ 与 state/（无环层，保证先求值），
 *     否则循环依赖下会撞 TDZ。违反任何一条直接报错退出，不写文件。
 */
import fs from "fs";
import path from "path";
import { createRequire } from "module";

const require = createRequire(import.meta.url);
const acorn = require("acorn");
const walk = require("acorn-walk");

const [, , sourcePath, configPath] = process.argv;
const webRoot = path.dirname(sourcePath);
const src = fs.readFileSync(sourcePath, "utf8");
const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
const ast = acorn.parse(src, { ecmaVersion: "latest", sourceType: "script", locations: true });
const body = ast.body[0].expression.callee.body.body;

// ── 条目与名字 ──
const DECL = new Set(["FunctionDeclaration", "VariableDeclaration", "ClassDeclaration"]);
const items = body.map((node, index) => {
  const names = [];
  if (node.type === "FunctionDeclaration" || node.type === "ClassDeclaration") names.push(node.id.name);
  if (node.type === "VariableDeclaration") {
    for (const decl of node.declarations) {
      walk.fullAncestor(decl.id, (n, _s, anc) => {
        const parent = anc[anc.length - 2];
        if (n.type === "Identifier" && !(parent?.type === "Property" && parent.key === n && !parent.shorthand)) names.push(n.name);
      });
    }
  }
  return { index, node, names, decl: DECL.has(node.type) };
});
const owner = new Map();
for (const item of items) for (const name of item.names) owner.set(name, item);
const lets = new Set(items.filter((i) => i.node.kind === "let").flatMap((i) => i.names));

// ── 归属 ──
const boundaries = new Map(config.boundaries);
let current = null;
for (const item of items) {
  const first = item.names[0];
  if (first && boundaries.has(first)) current = boundaries.get(first);
  item.module = (first && config.overrides[first]) || current;
  if (item.node.type === "ExpressionStatement" && item.node.expression.type === "Literal") {
    item.drop = true; // "use strict"：模块本来就是严格模式
    item.module = config.entry;
    continue;
  }
  if (!item.module) throw new Error(`no module for item at line ${item.node.loc.start.line}`);
}

// ── 引用分析 ──
function isReference(n, anc) {
  const parent = anc[anc.length - 2];
  if (!parent) return true;
  if (parent.type === "MemberExpression" && parent.property === n && !parent.computed) return false;
  if (parent.type === "Property" && parent.key === n && !parent.computed && !parent.shorthand) return false;
  if ((parent.type === "MethodDefinition" || parent.type === "PropertyDefinition") && parent.key === n && !parent.computed) return false;
  if (parent.type === "LabeledStatement" || parent.type === "BreakStatement" || parent.type === "ContinueStatement") return false;
  return true;
}
const FUNCTIONS = new Set(["FunctionExpression", "ArrowFunctionExpression", "FunctionDeclaration", "ClassBody"]);
for (const item of items) {
  item.refs = new Set();
  item.evalRefs = new Set();
  item.assigns = new Set();
  walk.fullAncestor(item.node, (n, _s, anc) => {
    if (n.type === "Identifier" && owner.has(n.name) && !item.names.includes(n.name) && isReference(n, anc)) {
      item.refs.add(n.name);
      // 声明初始化时就求值（不在任何函数体里）的引用
      const inFunction = anc.slice(1, -1).some((a) => FUNCTIONS.has(a.type));
      if (item.decl && item.node.type !== "FunctionDeclaration" && !inFunction) item.evalRefs.add(n.name);
    }
    if (n.type === "AssignmentExpression" && n.left.type === "Identifier" && lets.has(n.left.name)) item.assigns.add(n.left.name);
    if (n.type === "UpdateExpression" && n.argument.type === "Identifier" && lets.has(n.argument.name)) item.assigns.add(n.argument.name);
  });
}

// ── 约束检查 ──
const problems = [];
const moduleOf = (name) => owner.get(name).module;
for (const item of items) {
  for (const name of item.assigns) {
    if (moduleOf(name) !== item.module) problems.push(`let ${name} (${moduleOf(name)}) is assigned from ${item.module}`);
  }
  for (const name of item.evalRefs) {
    const target = moduleOf(name);
    if (target !== item.module && !/^(core|state)\//.test(target)) {
      problems.push(`${item.names.join(",")} in ${item.module} reads ${name} (${target}) at load time`);
    }
  }
  if (/^(core|state)\//.test(item.module)) {
    for (const name of item.refs) {
      const target = moduleOf(name);
      const rank = (m) => (m.startsWith("core/") ? 0 : m.startsWith("state/") ? 1 : 9);
      if (rank(target) > rank(item.module)) problems.push(`${item.names[0] ?? "stmt"} in ${item.module} depends on ${name} (${target})`);
    }
  }
}
if (problems.length) {
  console.error(problems.join("\n"));
  process.exit(1);
}

// ── 文本搬运 ──
const quasis = [];
walk.full(ast, (n) => { if (n.type === "TemplateElement") quasis.push([n.start, n.end]); });
const insideQuasi = (offset) => quasis.some(([a, b]) => offset > a && offset <= b);
function dedent(start, end) {
  let out = "";
  let offset = start;
  for (const line of src.slice(start, end).split("\n")) {
    out += (!insideQuasi(offset) && line.startsWith("  ") ? line.slice(2) : line) + "\n";
    offset += line.length + 1;
  }
  return out.replace(/\n$/, "");
}

const modules = new Map();
let previousEnd = body[0].start;
for (const item of items) {
  // 上一个条目结尾之后的第一个换行起，到本条目所在行首：这段是紧贴它的注释与空行。
  const lineStart = src.lastIndexOf("\n", item.node.start - 1) + 1;
  const gapBreak = src.indexOf("\n", previousEnd);
  const leadStart = gapBreak >= 0 && gapBreak < lineStart ? gapBreak + 1 : lineStart;
  previousEnd = item.node.end;
  if (item.drop) continue;
  const lead = dedent(leadStart, lineStart).replace(/^\n+/, "").replace(/\n+$/, "");
  let text = dedent(lineStart, item.node.end);
  if (item.decl && item.module !== config.entry) {
    text = text.replace(/^(\s*)(async function|function|const|let|class)\b/, "$1export $2");
  }
  const mod = modules.get(item.module) || { items: [], statements: [] };
  (item.decl || item.module === config.entry ? mod.items : mod.statements).push({ lead: lead.trim() ? lead : "", text, item });
  modules.set(item.module, mod);
}

// ── 生成 ──
const order = [...modules.keys()];
const startOrder = [];
for (const item of items) {
  if (!item.decl && !item.drop && item.module !== config.entry && !startOrder.includes(item.module)) startOrder.push(item.module);
}
function importPath(from, to) {
  let rel = path.posix.relative(path.posix.dirname(from), to);
  if (!rel.startsWith(".")) rel = `./${rel}`;
  return rel;
}
for (const [file, mod] of modules) {
  const needed = new Map();
  const all = [...mod.items, ...mod.statements];
  for (const { item } of all) {
    for (const name of item.refs) {
      const target = moduleOf(name);
      if (target === file) continue;
      if (!needed.has(target)) needed.set(target, new Set());
      needed.get(target).add(name);
    }
  }
  if (file === config.entry) {
    for (const target of startOrder) {
      if (!needed.has(target)) needed.set(target, new Set());
      needed.get(target).add(`start as start_${target.replace(/[^a-z0-9]/gi, "_")}`);
    }
  }
  const imports = [...needed.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([target, names]) => `import { ${[...names].sort().join(", ")} } from "${importPath(file, target)}";`);
  const parts = [];
  const header = config.headers?.[file];
  if (header) parts.push(header.trimEnd());
  if (imports.length) parts.push(imports.join("\n"));
  for (const { lead, text } of mod.items) parts.push((lead ? `${lead}\n` : "") + text);
  if (mod.statements.length) {
    const inner = mod.statements.map(({ lead, text }) => (lead ? `${lead}\n` : "") + text)
      .join("\n\n").split("\n").map((l) => (l ? `  ${l}` : l)).join("\n");
    parts.push(`/// 原 app.js 顶层的副作用语句，由入口在启动时按原顺序调用。\nexport function start() {\n${inner}\n}`);
  }
  if (file === config.entry) {
    const starts = startOrder.map((m) => `start_${m.replace(/[^a-z0-9]/gi, "_")}();`).join("\n");
    const idx = parts.findIndex((p) => /^initialize\(\);/m.test(p));
    if (idx >= 0) parts.splice(idx, 0, starts);
  }
  const out = parts.join("\n\n") + "\n";
  acorn.parse(out, { ecmaVersion: "latest", sourceType: "module" });
  const dest = path.join(webRoot, file);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.writeFileSync(dest, out);
}
const lines = (f) => fs.readFileSync(path.join(webRoot, f), "utf8").split("\n").length;
for (const f of order) console.log(`${String(lines(f)).padStart(6)}  ${f}`);
