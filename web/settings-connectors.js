"use strict";

/*
 * 通讯平台 → iMessage（以及以后的连接器平台）。
 *
 * 连接器平台的配置在 platforms.connectors.<平台>，和 QQ 设置页用同一份配置草稿、同一个
 * 保存栏；这里只多一张「连接状态」卡，读 /api/connectors。settings.js 是 IIFE，卡片、开关、
 * 密钥框这些零件从它传进来的 kit 借（和 settings-extensions.js 一样）。
 *
 * 草稿里原本没有这一节时不主动建：只有真改了哪一项才写进去，免得打开页面就把一整节
 * 默认值写进配置文件。
 */
window.GqySettingsConnectors = (() => {
  const PLATFORMS = {
    imessage: {
      title: "iMessage",
      description: "经 iMessage 连接器接入（macOS）。连接器只负责收发，会话、指令、模型、记忆都在这里。改动保存后生效。",
      handlePlaceholder: "手机号或 Apple ID 邮箱，回车添加",
      setup: "装连接器：scripts/imessage/install.sh，然后把下面的口令填进 ~/.gqy/config/imessage.json 的 token，并把 enabled 改成 true。"
    }
  };
  const STALE_MS = 5000;

  let kit = null;
  let status = null;
  let error = "";
  let fetchedAt = 0;
  let inflight = null;
  let revealedToken = null;

  function refresh() {
    if (inflight) return inflight;
    inflight = window.GqyCore.apiRequest("/api/connectors")
      .then((response) => response.json())
      .then((value) => { status = value; error = ""; })
      .catch((reason) => { error = reason?.message || "读取连接状态失败"; })
      .finally(() => { fetchedAt = Date.now(); inflight = null; kit?.rerender(); });
    return inflight;
  }

  const base = (platform) => `platforms.connectors.${platform}`;
  const read = (platform, key, fallback) => kit.cfg(`${base(platform)}.${key}`, fallback);
  const write = (platform, key, value) => kit.setCfg(`${base(platform)}.${key}`, value);

  function since(seconds) {
    const minutes = Math.max(0, Math.floor((Date.now() / 1000 - seconds) / 60));
    if (minutes < 1) return "刚刚";
    if (minutes < 60) return `${minutes} 分钟前`;
    if (minutes < 1440) return `${Math.floor(minutes / 60)} 小时前`;
    return `${Math.floor(minutes / 1440)} 天前`;
  }

  const CAPABILITY_LABELS = { reaction_in: "收点按回应", reaction_out: "发点按回应", image_out: "发图", audio_out: "发语音", file_out: "发文件", group: "群聊" };

  function statusCard(platform, meta) {
    const { el, card, row, chip, button, empty } = kit;
    const reload = button("刷新", { kind: "text", small: true, iconName: "refresh-cw", onClick: () => refresh() });
    if (error) return card([empty(error)], { title: "连接状态", actions: reload });
    if (!status) return card([empty("正在读取……")], { title: "连接状态", actions: reload });
    const connections = (status.connected || []).filter((item) => item.platform === platform);
    if (!connections.length) {
      return card([row("未连接", null, { hint: meta.setup })], { title: "连接状态", actions: reload });
    }
    const rows = connections.map((item) => {
      const abilities = Object.entries(item.capabilities || {}).filter(([, on]) => on).map(([key]) => chip(CAPABILITY_LABELS[key] || key));
      const name = [item.connector, item.version && `v${item.version}`].filter(Boolean).join(" ");
      return row(`已连接${item.account ? ` · ${item.account}` : ""}`, el("div.st-chips", null, ...abilities), { hint: `${name || "连接器"} · ${since(item.connected_at)}连上` });
    });
    return card(rows, { title: "连接状态", actions: reload });
  }

  function randomToken() {
    const bytes = new Uint8Array(24);
    crypto.getRandomValues(bytes);
    return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  }

  function tokenCard(platform) {
    const { el, card, row, button, secretControl, setSecret, textInput } = kit;
    const key = `${base(platform)}.token`;
    const rows = [row("口令", secretControl(key), { hint: "连接器连进来要带的口令，必填。空口令一律拒绝：沙盒里的成员会话也能连本机端口。" })];
    const generate = button("生成新口令", { small: true, iconName: "key", onClick: () => {
      revealedToken = randomToken();
      setSecret(key, revealedToken);
      kit.rerender();
    } });
    rows.push(row("生成", generate, { hint: "生成后要保存配置，并把同一个口令填进连接器配置。口令只在这里显示这一次。" }));
    if (revealedToken) {
      const shown = textInput(revealedToken, null, { mono: true, ariaLabel: "新口令" });
      shown.readOnly = true;
      shown.addEventListener("focus", () => shown.select());
      rows.push(row("新口令", shown, { hint: "点一下全选，复制到 ~/.gqy/config/imessage.json 的 token。" }));
    }
    return card(rows, { title: "接入", description: "连接器地址：ws://127.0.0.1:<网页端口>/api/connector/ws?platform=" + platform });
  }

  function contactsCard(platform, meta) {
    const { el, card, row, button, toggle, textInput, chipList, empty } = kit;
    const contacts = () => JSON.parse(JSON.stringify(read(platform, "contacts", []) || []));
    const save = (list) => write(platform, "contacts", list);
    const list = contacts();
    const rows = list.map((contact, index) => {
      const update = (mutate) => { const next = contacts(); mutate(next[index]); save(next); };
      const name = textInput(contact.name || "", (value) => update((item) => { item.name = value.trim(); }), { placeholder: "名字", ariaLabel: "联系人名字", width: "12rem" });
      const handles = chipList(contact.handles || [], (values) => update((item) => { item.handles = values; }), { placeholder: meta.handlePlaceholder, mono: true, ariaLabel: "账号" });
      const owner = toggle(Boolean(contact.owner), (value) => update((item) => { if (value) item.owner = true; else delete item.owner; }), "是我本人");
      const remove = button("删除", { kind: "text", small: true, danger: true, onClick: async () => {
        if (!(await kit.confirmAction(`删除联系人「${contact.name || "未命名"}」？`, "删除"))) return;
        const next = contacts();
        next.splice(index, 1);
        save(next);
        kit.rerender();
      } });
      return el("div", null,
        row("名字", el("div.st-inline-form", null, name, remove), { hint: "也是会话名的一部分（imessage-名字），改名等于换一个对话" }),
        row("账号", handles, { block: true, hint: "同一个人的多个账号合成一个对话；11 位国内手机号自动补 +86" }),
        row("是我本人", owner, { hint: "记忆与终端、网页共享，写入算你自己的" }));
    });
    const add = button("添加联系人", { iconName: "plus", small: true, onClick: () => {
      const next = contacts();
      next.push({ name: "", handles: [] });
      save(next);
      kit.rerender();
    } });
    return card([...(rows.length ? rows : [empty("还没有联系人。名单外的人发来的消息不会回。")]), add], { title: "联系人", description: "只回名单里的人的私聊。群聊不支持。" });
  }

  const BEHAVIOR_FIELDS = [
    { path: "owner_host_tools", label: "本人可用宿主工具", hint: "允许在聊天里让她跑命令、读写文件。默认关：手机丢了或账号被盗时，别人不能借聊天窗口操作电脑", kind: "toggle", default: false },
    { path: "max_bubbles", label: "最多拆成几条", hint: "按空行拆气泡，段落多于上限时均衡合并", kind: "number", integer: true, min: 1, max: 20, unit: "条", default: 6 },
    { path: "bubble_pause_seconds", label: "气泡间停顿上限", hint: "按长度停 0.5 秒到这个上限，像在打字；0 = 连着发", kind: "number", min: 0, max: 10, step: 0.5, unit: "秒", default: 2 },
    { path: "memory_write_enabled", label: "允许写记忆", kind: "toggle", default: true }
  ];

  function behaviorCard(platform) {
    const bindingFor = (field) => kit.pathBinding(`${base(platform)}.${field.path}`);
    return kit.card(kit.fieldRows(BEHAVIOR_FIELDS, bindingFor), { title: "回复" });
  }

  function render(root, platform, nextKit) {
    kit = nextKit;
    const meta = PLATFORMS[platform];
    if (!meta) return;
    const { el, toggle } = kit;
    const enabled = Boolean(read(platform, "enabled", false));
    const switchLabel = el("span", { text: enabled ? "已启用" : "未启用" });
    const enabledSwitch = toggle(enabled, (value) => {
      write(platform, "enabled", value);
      switchLabel.textContent = value ? "已启用" : "未启用";
      root.classList.toggle("is-platform-off", !value);
    });
    root.append(el("div.st-page-head", null,
      el("div", null, el("h2", { text: meta.title }), el("p.st-page-desc", { text: meta.description })),
      el("label.st-head-switch", null, switchLabel, enabledSwitch)));
    root.classList.toggle("is-platform-off", !enabled);
    root.append(statusCard(platform, meta), tokenCard(platform), contactsCard(platform, meta), behaviorCard(platform));
    if (!inflight && Date.now() - fetchedAt > STALE_MS) refresh();
  }

  return { render };
})();
