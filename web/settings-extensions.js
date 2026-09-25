"use strict";

/*
 * 设置 → 插件 →「扩展」：技能、脚本工具、MCP 服务器、pm 包，和内置插件用同一套卡片与抽屉。
 *
 * 方案见 docs/plan/2026-09-24-extensions-in-plugin-list.md。settings.js 在插件页末尾
 * 调 render(root, kit)，卡片、抽屉、开关这些零件从 kit 借（settings.js 是 IIFE，零件不外露）。
 *
 * 开关的生效方式不一样，界面上照实说：
 * - 技能、脚本：点了立即生效（后端改文件，下一轮对话按指纹重扫）；
 * - MCP：改的是配置草稿，和内置插件卡片一样，点「保存配置」才生效；
 * - pm 包：没有开关，抽屉里升级 / 卸载。安装要下载外部代码，仍走命令行。
 */
window.GqySettingsExtensions = (() => {
  const KINDS = {
    skill: { icon: "book-open", hue: 280, label: "技能" },
    script: { icon: "terminal", hue: 160, label: "脚本" },
    mcp: { icon: "server", hue: 210, label: "MCP" },
    package: { icon: "package", hue: 30, label: "pm 包" }
  };
  const SOURCE_LABEL = { persona: "人格层", global: "全局", built_in: "内置" };
  const LAYER_LABEL = { builtin: "内置", "builtin-persona": "内置（人格）", global: "全局", persona: "人格" };
  const STALE_MS = 3000;

  /* ── 来源：互联网来的（git / pm）还是自己创建的 ── */

  function originLabel(origin) {
    if (!origin) return "";
    if (origin.kind === "git") return /github\.com/.test(origin.remote || "") ? "来自 GitHub" : "来自 git 仓库";
    return { pm: "pm 包", self: "自己创建", builtin: "内置", managed: "其他程序管理" }[origin.kind] || "";
  }

  function withOrigin(origin, text) {
    const label = originLabel(origin);
    return label ? `${label} · ${text || "没有说明"}` : text;
  }

  /* 抽屉里的「来源」卡：git 来的给检查更新 / 更新，更新走它自己的方式（git + npm / uv / pip）。 */
  function originCard(origin) {
    if (!origin) return null;
    const rows = [kit.row("来源", kit.el("span.st-ext-value", { text: originLabel(origin) }))];
    if (origin.kind === "git") {
      const repo = (origin.remote || "").replace(/\.git$/, "");
      rows.push(kit.row("仓库", kit.el("span.st-ext-value", { text: repo })));
      rows.push(kit.row("当前版本", kit.el("span.st-ext-value", { text: origin.commit || "" })));
      if (origin.deps?.length) rows.push(kit.row("更新后同步依赖", kit.el("span.st-ext-value", { text: origin.deps.join("、") })));
      const status = kit.el("p.st-ext-note", { text: "" });
      const check = kit.button("检查更新", { small: true, onClick: async () => {
        status.textContent = "正在检查…";
        try {
          const { check: result } = await api("/api/extensions/origin/check", { method: "POST", body: JSON.stringify({ dir: origin.dir }) });
          status.textContent = result.has_update ? `有新版本：${result.latest.slice(0, 7)}（${result.branch} 分支）` : `已是最新（${result.branch} 分支）`;
        } catch (reason) { status.textContent = reason?.message || "检查失败"; }
      } });
      const update = kit.button("更新", { small: true, kind: "primary", onClick: async () => {
        status.textContent = "正在更新，拉代码和装依赖可能要一两分钟…";
        try {
          const { report } = await api("/api/extensions/origin/update", { method: "POST", body: JSON.stringify({ dir: origin.dir }) });
          status.textContent = report.updated ? `已更新 ${report.from} → ${report.to}\n${report.log.join("\n")}` : "已是最新，不用更新。";
          refresh();
        } catch (reason) { status.textContent = reason?.message || "更新失败"; }
      } });
      rows.push(kit.row("更新", kit.el("span.st-ext-actions", null, check, update), { hint: "拉到远端默认分支的最新提交，再按目录里的文件同步依赖；有未提交的修改时不会动" }));
      return kit.el("div", null, kit.card(rows, { title: "来源" }), status);
    }
    const hint = { self: "她用 manage_skill / manage_script 写的，或手动放进来的，没有外部来源可更新。", pm: "在「pm 包」里升级或卸载。", managed: "由安装它的程序自己管理，gqy 不替它更新。", builtin: "随 gqy 一起更新。" }[origin.kind];
    return kit.el("div", null, kit.card(rows, { title: "来源" }), hint ? kit.el("p.st-ext-note", { text: hint }) : null);
  }

  let data = null;
  let error = "";
  let fetchedAt = 0;
  let inflight = null;
  let kit = null;

  async function api(path, options) {
    const response = await window.GqyCore.apiRequest(path, options);
    return response.json();
  }

  function refresh() {
    if (inflight) return inflight;
    inflight = api("/api/extensions")
      .then((value) => { data = value; error = ""; })
      .catch((reason) => { error = reason?.message || "读取扩展失败"; })
      .finally(() => { fetchedAt = Date.now(); inflight = null; kit?.rerender("plugins"); });
    return inflight;
  }

  async function act(work, done) {
    try {
      const result = await work();
      if (done) kit.toast(typeof done === "function" ? done(result) : done, "success");
    } catch (reason) {
      kit.toast(reason?.message || "操作失败", "error");
    }
    await refresh();
  }

  function mark(kind) {
    const node = kit.el("span.st-mark.is-kind", null, kit.icon(KINDS[kind].icon));
    node.style.setProperty("--mark-hue", String(KINDS[kind].hue));
    return node;
  }

  function extCard(kind, title, description, { off = false, control = null, onOpen }) {
    const node = kit.el("div.st-plugin-card");
    node.classList.toggle("is-off", off);
    node.append(kit.el("button.st-plugin-open", { type: "button", onclick: onOpen }, mark(kind),
      kit.el("span.st-plugin-copy", null, kit.el("strong", { text: title }), kit.el("small", { text: description || "没有说明" }))));
    if (control) node.append(control);
    return node;
  }

  function section(root, title, hint, cards, addControl) {
    root.append(kit.el("h3.st-group-title", { text: title }));
    const grid = kit.el("div.st-grid.is-plugins");
    cards.forEach((node, index) => { node.style.setProperty("--i", String(index)); grid.append(node); });
    if (!cards.length) grid.append(kit.el("p.st-ext-empty", { text: "还没有。" }));
    root.append(grid, kit.el("div.st-ext-add", null, kit.el("span", { text: hint }), addControl));
  }

  function info(pairs) {
    return kit.card(pairs.filter(([, value]) => value !== null && value !== undefined && value !== "")
      .map(([label, value]) => kit.row(label, typeof value === "string" ? kit.el("span.st-ext-value", { text: value }) : value)));
  }

  /* ── 技能 ── */

  function skillCards() {
    return (data.skills || []).map((skill) => {
      const control = skill.toggle === "fixed"
        ? kit.chip("内置", "is-soft")
        : kit.toggle(skill.enabled, (value) => act(() => api("/api/extensions/skills/toggle", { method: "POST", body: JSON.stringify({ name: skill.name, enabled: value }) })), `${skill.name} 启用`);
      return extCard("skill", skill.name, withOrigin(skill.origin, skill.description), { off: !skill.enabled, control, onOpen: () => openSkill(skill) });
    });
  }

  function openSkill(skill) {
    const source = kit.el("pre.st-ext-source", { text: "正在读取…" });
    api(`/api/extensions/skills/source?name=${encodeURIComponent(skill.name)}`)
      .then((value) => { source.textContent = value.source; })
      .catch((reason) => { source.textContent = reason?.message || "读取失败"; });
    const how = { fixed: "平台内置，任何人格都开着", marker: "人格自己那一层，开关只影响这个技能", whitelist: "写进当前人格的技能白名单" }[skill.toggle];
    const footer = [kit.el("span.st-foot-spacer")];
    if (skill.source !== "built_in") {
      footer.unshift(kit.button("删除", { kind: "text", danger: true, onClick: async () => {
        if (!(await kit.confirmAction(`删除技能「${skill.name}」？文件会一起删掉，不能恢复。`, "删除"))) return;
        kit.closeDrawer();
        act(() => api(`/api/extensions/skills?name=${encodeURIComponent(skill.name)}`, { method: "DELETE" }), "已删除");
      } }));
    }
    footer.push(kit.button("完成", { kind: "primary", onClick: () => kit.closeDrawer() }));
    kit.openDrawer({ title: skill.name, subtitle: `技能 · ${SOURCE_LABEL[skill.source] || skill.source}`, width: "620px", footer,
      body: (body) => body.append(
        info([["说明", skill.description], ["状态", skill.enabled ? "开" : "关"], ["开关方式", how], ["位置", skill.path], ["来自 pm 包", skill.package]]),
        ...[originCard(skill.origin)].filter(Boolean),
        kit.card([source], { title: "SKILL.md" })) });
  }

  function openCreateSkill() {
    const draft = { name: "", description: "", body: "" };
    const errorLine = kit.el("p.st-ext-error");
    const submit = async () => {
      errorLine.textContent = "";
      try {
        await api("/api/extensions/skills", { method: "POST", body: JSON.stringify(draft) });
      } catch (reason) {
        errorLine.textContent = reason?.message || "创建失败";
        return;
      }
      dialog.close();
      kit.toast(`已新建技能「${draft.name}」`, "success");
      refresh();
    };
    const dialog = kit.openDialog({
      title: "新建技能", subtitle: "建在当前人格那一层，和她用 manage_skill 建的一样", width: "640px",
      body: (content) => content.append(kit.card([
        kit.row("名称", kit.textInput("", (value) => { draft.name = value.trim(); }, { placeholder: "小写字母、数字和连字符，例如 weekly-report", mono: true }), { hint: "技能的唯一标识，建好后不能改" }),
        kit.row("一句话说明", kit.textInput("", (value) => { draft.description = value; }, { placeholder: "什么时候该用它" }), { hint: "她靠这一句决定什么时候加载这个技能，写清楚适用场景" }),
        kit.row("正文", kit.textarea("", (value) => { draft.body = value; }, { rows: 12, mono: true, placeholder: "# 标题\n\n## 步骤\n\n1. ……" }), { block: true, hint: "Markdown。留空会生成一个模板" })
      ]), errorLine),
      actions: [kit.button("取消", { onClick: () => dialog.close() }), kit.button("创建", { kind: "primary", onClick: submit })]
    });
  }

  /* ── 脚本 ── */

  function scriptCards() {
    return (data.scripts || []).map((script) => {
      const control = kit.toggle(script.enabled, (value) => act(() => api(`/api/dash/scripts/${value ? "enable" : "disable"}`, { method: "POST", body: JSON.stringify({ id: script.id }) })), `${script.title} 启用`);
      return extCard("script", script.title || script.id, script.description, { off: !script.enabled, control, onOpen: () => openScript(script) });
    });
  }

  function openScript(script) {
    const footer = [kit.el("span.st-foot-spacer"), kit.button("完成", { kind: "primary", onClick: () => kit.closeDrawer() })];
    if (!script.builtin) {
      footer.unshift(kit.button("删除", { kind: "text", danger: true, onClick: async () => {
        if (!(await kit.confirmAction(`删除脚本「${script.id}」？脚本文件会一起删掉。`, "删除"))) return;
        kit.closeDrawer();
        act(() => api(`/api/dash/scripts/item?id=${encodeURIComponent(script.id)}`, { method: "DELETE" }), "已删除");
      } }));
    }
    kit.openDrawer({ title: script.title || script.id, subtitle: `脚本 · ${LAYER_LABEL[script.layer] || script.layer || ""}`, width: "560px", footer,
      body: (body) => body.append(info([
        ["说明", script.description], ["状态", script.enabled ? "开" : "关"], ["参数", (script.parameters || []).join("、")],
        ["文件", script.path], ["来自 pm 包", script.package]
      ]), kit.el("p.st-ext-note", { text: "改参数说明、看源码在控制台「脚本」面板。" })) });
  }

  /* ── MCP ── */

  function mcpServers() { return kit.cfg("mcp.servers", []) || []; }

  function mcpCards() {
    return (data.mcp || []).map((server) => {
      const draft = mcpServers()[server.index];
      const enabled = draft ? draft.enabled !== false : server.enabled;
      const control = kit.toggle(enabled, (value) => { kit.setCfg(`mcp.servers.${server.index}.enabled`, value); kit.rerender("plugins"); }, `${server.title} 启用`);
      const status = { ok: `${server.tools.length} 个工具`, failed: "上次连接失败", unknown: "" }[server.status];
      return extCard("mcp", server.title, withOrigin(server.origin, status || server.id), { off: !enabled, control, onOpen: () => openMcp(server) });
    });
  }

  function openMcp(server) {
    const statusText = { ok: "连接正常", failed: `上次连接失败：${server.error || ""}`, unknown: "还没连接过（没开，或还没用到）" }[server.status];
    const tools = server.tools.length
      ? kit.card(server.tools.map((tool) => kit.row(tool.name, kit.el("span.st-ext-value", { text: tool.description || "" }))), { title: `提供的工具（${server.tools.length}）` })
      : null;
    kit.openDrawer({ title: server.title, subtitle: `MCP · ${server.id}`, width: "560px",
      footer: [kit.el("span.st-foot-spacer"), kit.button("完成", { kind: "primary", onClick: () => kit.closeDrawer() })],
      body: (body) => body.append(
        info([["命令", [server.command, ...(server.args || [])].join(" ")], ["状态", statusText], ["总开关", data.mcp_enabled ? "MCP 已启用" : "MCP 总开关是关的，所有服务器都不会连接"]]),
        ...[originCard(server.origin), tools].filter(Boolean),
        kit.el("p.st-ext-note", { text: "开关改的是配置草稿，点「保存配置」生效。改命令、参数、环境变量在设置的「MCP」页。" })) });
  }

  /* ── pm 包 ── */

  function packageCards() {
    return (data.packages || []).map((pack) => extCard("package", pack.name, pack.description, { control: kit.chip(`v${pack.version}`, "is-soft"), onOpen: () => openPackage(pack) }));
  }

  function openPackage(pack) {
    const upgrade = kit.button("升级", { onClick: () => { kit.closeDrawer(); kit.toast(`正在检查 ${pack.name} 的新版本…`); act(() => api("/api/extensions/packages/upgrade", { method: "POST", body: JSON.stringify({ name: pack.name }) }), (result) => (result.updated ? `已升级 ${pack.name}：${result.from} → ${result.to}` : `${pack.name} 已是最新`)); } });
    const remove = kit.button("卸载", { kind: "text", danger: true, onClick: async () => {
      if (!(await kit.confirmAction(`卸载「${pack.name}」？它装进来的 ${pack.files.length} 个文件会被删掉。`, "卸载"))) return;
      kit.closeDrawer();
      act(() => api(`/api/extensions/packages?name=${encodeURIComponent(pack.name)}`, { method: "DELETE" }), "已卸载");
    } });
    kit.openDrawer({ title: pack.name, subtitle: `pm 包 · v${pack.version}`, width: "580px",
      footer: [remove, kit.el("span.st-foot-spacer"), upgrade, kit.button("完成", { kind: "primary", onClick: () => kit.closeDrawer() })],
      body: (body) => body.append(
        info([["说明", pack.description], ["来源", pack.reference ? `${pack.source}@${pack.reference}` : pack.source], ["commit", pack.commit ? pack.commit.slice(0, 12) : ""], ["类型", pack.kind], ["安装时间", pack.installed_at]]),
        kit.card([kit.el("pre.st-ext-source", { text: (pack.files || []).join("\n") || "（无）" })], { title: `装进来的文件（${(pack.files || []).length}）` })) });
  }

  /* ── 装配 ── */

  function render(root, parts) {
    kit = parts;
    root.append(kit.el("div.st-page-head.st-ext-head", null, kit.el("div", null, kit.el("h2", { text: "扩展" }),
      kit.el("p.st-page-desc", { text: "不改代码就能加的能力：技能、脚本工具、MCP 服务器和 pm 包。技能和脚本的开关立即生效，MCP 的开关要点「保存配置」。" }))));
    if (Date.now() - fetchedAt > STALE_MS) refresh();
    if (!data) {
      root.append(kit.el("p.st-ext-empty", { text: error || "正在读取扩展…" }));
      return;
    }
    if (error) root.append(kit.el("p.st-ext-error", { text: error }));
    section(root, "技能", data.skills_enabled ? "新技能也可以让她用 manage_skill 写。" : "技能总开关在「全局 → 工具」里是关的，这里的技能都不会给她用。",
      skillCards(), kit.button("新建技能", { iconName: "plus", small: true, onClick: openCreateSkill }));
    section(root, "脚本工具", "让她用 manage_script 注册新脚本，或在控制台「脚本」面板管理。", scriptCards(), null);
    section(root, "MCP 服务器", "在设置的「MCP」页添加和修改服务器。", mcpCards(), null);
    section(root, "pm 包", "安装要下载外部代码，用命令行：gqy pm install owner/repo。", packageCards(), null);
  }

  return { render };
})();
