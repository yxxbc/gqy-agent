export function prettyArguments(value) {
  if (value == null) return "";
  let obj = value;
  if (typeof value === "string") {
    const trimmed = value.trim();
    if (!trimmed) return "";
    try {
      obj = JSON.parse(trimmed);
    } catch (_) {
      return value;
    }
  }
  if (obj == null) return "";
  // 顶层对象 → 「键：值」逐行,不再是裹着大括号的裸 JSON(09-12 #6:工具展开
  // 信息不该是裸 json)。嵌套值压成一行紧凑 JSON;非对象/数组回退 pretty JSON。
  if (typeof obj !== "object" || Array.isArray(obj)) {
    try {
      return JSON.stringify(obj, null, 2);
    } catch (_) {
      return String(obj);
    }
  }
  const lines = [];
  for (const [key, raw] of Object.entries(obj)) {
    let rendered;
    if (raw == null) rendered = "";
    else if (typeof raw === "object") {
      try { rendered = JSON.stringify(raw); } catch (_) { rendered = String(raw); }
    } else rendered = String(raw);
    lines.push(`${key}: ${rendered}`);
  }
  return lines.join("\n");
}

// 子代理事件名后端格式化成 `subagent:<描述>`(让并行子代理各有独立事件名,
// 见 agent::reports::tool_event_name),所以判定/取图标要认这个前缀,不能只比
// 精确名——否则子代理工具行认不出来,窥视/子过程时间线整套都不触发(09-11
// 用户报「气泡还在、渲染不对」的真因,Playwright 实测揪出)。
export function subagentToolBaseName(name) {
  const n = String(name || "");
  const at = n.search(/[:：]/);
  return at >= 0 ? n.slice(0, at) : n;
}

export function isSubagentTool(name) {
  const base = subagentToolBaseName(name);
  return base === "subagent" || base === "task";
}

export function parsedToolArguments(value) {
  if (value && typeof value === "object" && !Array.isArray(value)) return value;
  if (typeof value !== "string" || !value.trim()) return {};
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  } catch (_) {
    return {};
  }
}

export function compactLine(value, limit = 92) {
  const line = String(value || "").replace(/\s+/g, " ").trim();
  if (line.length <= limit) return line;
  return `${line.slice(0, Math.max(1, limit - 1))}…`;
}

export function compactPath(value) {
  const path = String(value || "").trim();
  if (!path) return "";
  return path.split(/[\\/]/).filter(Boolean).pop() || path;
}

export function toolSubject(name, value) {
  const args = parsedToolArguments(value);
  const toolName = String(name || "");
  if (toolName === "run_command" || toolName === "Bash") {
    const line = compactLine(args.command || args.cmd);
    const background = args.background === true || args.run_in_background === true;
    return background ? `[后台] ${line}` : line;
  }
  if (toolName === "read" || toolName === "read_file") {
    const path = compactPath(args.path);
    const offset = Number.isFinite(Number(args.offset)) && args.offset != null ? Number(args.offset) : null;
    const limit = Number.isFinite(Number(args.limit)) && args.limit != null ? Number(args.limit) : null;
    if (offset === null && limit === null) return path;
    const start = Math.max(offset ?? 1, 1);
    const page = limit !== null ? `L${start}-${start + limit - 1}` : `L${start}+`;
    return path ? `${path} (${page})` : page;
  }
  if (["edit", "artifact", "kb", "apply_patch", "apply_artifact_patch"].includes(toolName)) {
    // 唯一编辑器:patchText 里抠出文件名当副标题,不然标签恒空。
    const text = String(args.patchText || args.patch_text || "");
    const files = [...text.matchAll(/^\*\*\* (?:Add|Update|Delete) File: (.+)$/gm)].map((m) => m[1].trim());
    if (files.length === 1) return compactPath(files[0]);
    if (files.length > 1) return `${compactPath(files[0])} 等 ${files.length} 个文件`;
    return "";
  }
  if (["read", "write", "edit", "print_image", "vision_analyze"].includes(toolName)) {
    return compactPath(args.filePath || args.file_path || args.path || args.image);
  }
  if (toolName === "grep") {
    const target = compactPath(args.path);
    return compactLine(`${args.pattern || ""}${target ? ` · ${target}` : ""}`);
  }
  if (toolName === "glob") return compactLine(`${args.pattern || ""}${args.path ? ` · ${compactPath(args.path)}` : ""}`);
  if (["webfetch", "web_fetch"].includes(toolName)) return compactLine(args.url);
  if (["web_search", "search_web", "search_web_images"].includes(toolName)) return compactLine(args.query || args.q);
  if (toolName === "generate_image") return compactLine(args.prompt);
  if (isSubagentTool(toolName)) return compactLine(args.description || args.prompt);
  if (toolName === "load_skill") return compactLine(args.name);
  const preferred = ["query", "command", "path", "filePath", "url", "name", "id", "target"];
  for (const key of preferred) {
    if (typeof args[key] === "string" && args[key].trim()) return compactLine(args[key]);
  }
  return "";
}

export function formatToolDuration(milliseconds) {
  if (!Number.isFinite(milliseconds) || milliseconds < 0) return "";
  if (milliseconds < 1_000) return `${Math.max(1, Math.round(milliseconds))} ms`;
  if (milliseconds < 10_000) return `${(milliseconds / 1_000).toFixed(1)} s`;
  return `${Math.round(milliseconds / 1_000)} s`;
}

// 主题与工具显示名共享 ≥6 字符前缀时去重(如「Linux 游戏兼容性调查」+「Linux 游戏兼容性: xxx」)
export function dedupeToolSubject(title, subject) {
  const t = String(title || "").trim();
  const s = String(subject || "").trim();
  if (!t || !s) return s;
  let i = 0;
  while (i < t.length && i < s.length && t[i] === s[i]) i += 1;
  if (i < 6) return s;
  const rest = s.slice(i).replace(/^[\s:：·,，、-]+/, "");
  return rest || s;
}
