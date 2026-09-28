"use strict";

/*
 * 选中文字右键菜单:解释 · 翻译 · 引用追问 · 搜索 · 复制。
 * 计划:docs/plan-is-true/2026-09-14/selection-menu.md(2026-09-14 裁定:中性说明、知识库与网页两栏、
 * 不留历史、侧栏里的选区先不管)。
 *
 * - 只在聊天消息正文(.markdown-body / .user-bubble)里、选区非空时拦截右键;其他地方与
 *   Shift + 右键一律走浏览器原生菜单。替掉原生菜单不能把退路也拿走,所以菜单自带「复制」。
 * - 解释 / 翻译:POST /api/selection/assist,NDJSON 流式。上下文由后端按 turn_id 取,前端不传。
 * - 搜索:知识库(/api/dash/kb/search)在前、网页(/api/selection/web-search)在后。
 * - 结果只进浮窗:不进对话、不进记忆、不留历史。关浮窗即中断请求。
 * - 手机:长按菜单拦不住也不该拦,选区停稳 300ms 后在选区下方浮一条工具条(上方是系统菜单)。
 *
 * 2026-09-28 用户反馈那一轮:
 * - **选区高亮自己画**(`.sel-mark`):浏览器给原生选区画的底色会随焦点、重绘、浏览器的不同
 *   而消失,而用户正是靠它认「我刚选的是哪几个字」。浮窗/菜单在的一天,高亮就在一天。
 * - **思考要有动静**:模型在想的这几秒里浮窗里挂主对话同款的「正在思考」签(三点弹跳 + 标题流光),
 *   后端把 reasoning 增量也推过来,正文一到就收尾成「已思考」。
 * - **语言一对按钮**:翻译浮窗里「英文 / 中文」各一个,当前那个亮着,点另一个原地重来。
 * - **不再自动关,所以「钉住」按钮撤下**(用户 09-28:浮窗停留就是钉住了):点别处不关,
 *   关浮窗只有 ✕ 和 Esc。
 * - **尺寸随回复伸缩**:内容多长浮窗多长,这一侧放不下就改用更宽的另一侧,让字往上长,
 *   而不是把结果切在肚子里。
 *
 * 菜单与浮窗挂 document.body、fixed 定位:聊天区外层有 overflow 裁剪。
 * 单独成文件:app.js 已经上万行(与 contextpanel.js / todos.js 同构)。
 */
window.GqySelectionMenu = (() => {
  const ACTIONS = [
    { key: "explain", label: "解释", needsModel: true },
    { key: "translate", label: "翻译", needsModel: true },
    { key: "quote", label: "引用追问" },
    { key: "search", label: "搜索" },
  ];
  const MAX_CHARS = 2000;
  const BODY_SELECTOR = ".markdown-body, .user-bubble";

  let ctx = null;
  let menu = null;
  let toolbar = null;
  let marks = null; // { layer, nodes, range } 自己画的选区高亮
  let current = null; // { text, turnId, rect, range }
  const popovers = []; // { node, head, body, foot, controller, side, anchor, manual }
  let selectionTimer = 0;

  function el(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text != null) node.textContent = text;
    return node;
  }
  function button(className, text, onClick, title) {
    const node = el("button", className, text);
    node.type = "button";
    if (onClick) node.addEventListener("click", onClick);
    if (title) node.title = title;
    return node;
  }
  const elementOf = (node) => (node?.nodeType === Node.ELEMENT_NODE ? node : node?.parentElement) || null;

  function mount(options) {
    ctx = options;
    if (!ctx?.root) return;
    ctx.root.addEventListener("contextmenu", onContextMenu);
    ctx.root.addEventListener("scroll", onRootScroll, { passive: true, capture: true });
    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown);
    window.addEventListener("resize", onWindowResize, { passive: true });
    // 页面整体滚一下(移动端浏览器工具栏收放、外部容器滚动)高亮也要跟着挪:矩形取自 Range,
    // 重画一次就是新位置。菜单只在聊天区自己滚动时关,所以那条监听单独留着。
    window.addEventListener("scroll", paintMarks, { passive: true, capture: true });
    if (window.matchMedia("(hover: none), (pointer: coarse)").matches) {
      document.addEventListener("selectionchange", onSelectionChange);
    }
  }

  /// 选区必须整段落在同一条消息的正文里,才算我们的地盘。
  function readSelection() {
    const selection = window.getSelection();
    if (!selection || selection.isCollapsed || !selection.rangeCount) return null;
    const text = selection.toString().trim();
    if (!text) return null;
    const range = selection.getRangeAt(0);
    const start = elementOf(range.startContainer);
    const end = elementOf(range.endContainer);
    if (!start || !end || !ctx.root.contains(start) || !ctx.root.contains(end)) return null;
    const startBody = start.closest(BODY_SELECTOR);
    const endBody = end.closest(BODY_SELECTOR);
    if (!startBody || !endBody) return null;
    const article = start.closest("article.message");
    if (!article || article !== end.closest("article.message")) return null;
    return {
      text,
      turnId: article.dataset.turnId || "",
      rect: range.getBoundingClientRect(),
      range: range.cloneRange(),
    };
  }

  // ---------------- 选区高亮 ----------------

  /*
   * 用 Range 的矩形自己画一条高亮,不看浏览器的脸色。原生高亮只在「文档焦点、DOM 没被重绘、
   * 浏览器乐意」时才画得出来:点上菜单项、她那边正在流式重画、换一个浏览器,都可能是空白。
   * 高亮跟着菜单/浮窗活,关了就撤。鼠标滚轮一动 Rect 就过期,重画即可(DOM 被换掉的
   * 话 Range 会失效,这时只能撤掉——比指着一处错地方强)。
   */

  function setMarkRange(range) {
    if (!marks) {
      marks = { layer: el("div", "sel-mark-layer"), nodes: [], range: null };
      document.body.appendChild(marks.layer);
    }
    marks.range = range ? range.cloneRange() : null;
    paintMarks();
  }

  function paintMarks() {
    if (!marks) return;
    const range = marks.range;
    const alive = range?.startContainer?.isConnected && range?.endContainer?.isConnected;
    const rects = alive && !range.collapsed ? [...range.getClientRects()] : [];
    marks.layer.replaceChildren();
    marks.nodes = rects
      .filter((rect) => rect.width > 0 && rect.height > 0)
      .map((rect) => {
        const node = el("div", "sel-mark");
        node.style.left = `${rect.left}px`;
        node.style.top = `${rect.top}px`;
        node.style.width = `${rect.width}px`;
        node.style.height = `${rect.height}px`;
        marks.layer.appendChild(node);
        return node;
      });
  }

  function clearMarks() {
    marks?.layer.remove();
    marks = null;
  }

  /// 菜单与浮窗都撤了,高亮才撤:它标的是「你刚才选的是哪几个字」,浮窗还在看就该还在。
  function syncMarks() {
    if (!menu && !popovers.length) clearMarks();
  }

  function onRootScroll() {
    closeMenu();
    syncMarks();
    paintMarks();
  }

  function onWindowResize() {
    closeMenu();
    syncMarks();
    paintMarks();
    for (const pop of popovers) fitPopover(pop);
  }

  function onContextMenu(event) {
    if (event.shiftKey) return;
    const picked = readSelection();
    if (!picked) return;
    event.preventDefault();
    current = picked;
    setMarkRange(picked.range);
    openMenu(event.clientX, event.clientY);
  }

  function openMenu(x, y) {
    // 直接换掉旧菜单,不走 closeMenu:那条路会顺手撤掉高亮,而高亮正是刚点起来的那条。
    menu?.remove();
    menu = el("div", "sel-menu");
    menu.setAttribute("role", "menu");
    const tooLong = current.text.length > MAX_CHARS;
    for (const action of ACTIONS) {
      const item = menuItem(action.label, () => runAction(action.key));
      if (action.needsModel && tooLong) {
        item.disabled = true;
        item.title = `选中的文字超过 ${MAX_CHARS} 字`;
      }
      menu.appendChild(item);
    }
    menu.append(el("div", "sel-menu-sep"), menuItem("复制", copySelection), el("div", "sel-menu-hint", "Shift + 右键:浏览器菜单"));
    // 按下按钮会让部分浏览器收起选区;选区文字已经记在 current 里,这里只防闪。
    menu.addEventListener("pointerdown", (event) => event.preventDefault());
    document.body.appendChild(menu);
    const { width, height } = menu.getBoundingClientRect();
    menu.style.left = `${Math.max(8, Math.min(x, window.innerWidth - width - 8))}px`;
    menu.style.top = `${y + height + 8 > window.innerHeight ? Math.max(8, y - height) : y}px`;
    menu.querySelector(".sel-menu-item:not(:disabled)")?.focus({ preventScroll: true });
  }

  function menuItem(label, onClick) {
    const item = button("sel-menu-item", label, () => {
      closeMenu();
      onClick();
    });
    item.setAttribute("role", "menuitem");
    return item;
  }

  function closeMenu() {
    menu?.remove();
    menu = null;
  }

  /// 点别处只关菜单。浮窗不自动关(09-28 用户裁定:浮窗停留就是钉住了),要关只有 ✕ 和 Esc。
  function onPointerDown(event) {
    if (!menu || menu.contains(event.target)) return;
    closeMenu();
    syncMarks();
  }

  function onKeyDown(event) {
    if (menu) {
      const items = [...menu.querySelectorAll(".sel-menu-item:not(:disabled)")];
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const index = items.indexOf(document.activeElement);
        const step = event.key === "ArrowDown" ? 1 : -1;
        items[(index + step + items.length) % items.length]?.focus();
      } else if (event.key === "Escape") {
        event.preventDefault();
        closeMenu();
        syncMarks();
      }
      return;
    }
    // Esc 关最后一个浮窗:没有「钉住」之后,后开的那个就是眼下在看的那个。
    if (event.key !== "Escape" || !popovers.length) return;
    event.preventDefault();
    closePopover(popovers[popovers.length - 1]);
  }

  function runAction(key) {
    hideToolbar();
    if (!current) return;
    if (key === "quote") quote(current.text);
    else if (key === "search") openSearch(current);
    else openAssist(key, current);
    // 菜单项一按就关了菜单。动作没开出浮窗(复制 / 引用追问)时,高亮跟着菜单一起撤;
    // 开出了浮窗的,高亮留到浮窗关掉那一刻(见 closePopover 的 syncMarks)。
    syncMarks();
  }

  // ---------------- 引用追问 / 复制 ----------------

  function quote(text, extra = "") {
    const input = ctx.composer;
    if (!input) return;
    const block = text.split("\n").map((line) => `> ${line}`).join("\n");
    const draft = input.value.replace(/\s+$/, "");
    input.value = `${draft ? `${draft}\n\n` : ""}${block}\n${extra ? `\n${extra}\n` : ""}\n`;
    input.dispatchEvent(new Event("input", { bubbles: true }));
    ctx.resizeComposer?.();
    input.focus();
    input.setSelectionRange(input.value.length, input.value.length);
    window.getSelection()?.removeAllRanges();
  }

  async function copyText(text, message = "已复制") {
    try {
      await navigator.clipboard.writeText(text);
      ctx.toast?.(message);
    } catch (_) {
      ctx.toast?.("复制失败:浏览器没给剪贴板权限", "error");
    }
  }

  function copySelection() {
    if (current) copyText(current.text);
  }

  // ---------------- 浮窗 ----------------

  function createPopover(title, picked) {
    // 浮窗各自独立:同时开着两个是允许的(对比两种译法、解释配搜索),关哪个由用户点 ✕ 决定。
    const node = el("section", "sel-pop");
    node.setAttribute("role", "dialog");
    node.setAttribute("aria-label", title);
    const head = el("header", "sel-pop-head");
    const excerpt = picked.text.replace(/\s+/g, " ");
    const quoteNode = el("span", "sel-pop-quote", excerpt.length > 60 ? `${excerpt.slice(0, 60)}…` : excerpt);
    quoteNode.title = picked.text;
    const pop = { node, head, body: null, foot: null, controller: null, side: "below", anchor: picked.rect, manual: false };
    head.append(el("strong", null, title), quoteNode, button("sel-icon", "✕", () => closePopover(pop), "关闭"));
    pop.body = el("div", "sel-pop-body");
    pop.foot = el("footer", "sel-pop-foot");
    node.append(head, pop.body, pop.foot);
    document.body.appendChild(node);
    popovers.push(pop);
    placePopover(pop);
    makeDraggable(pop, head);
    return pop;
  }

  /// 按住标题栏拖动浮窗(标题栏里的按钮照常点)。手机宽度下浮窗是底部面板,不拖。
  function makeDraggable(pop, handle) {
    handle.addEventListener("pointerdown", (event) => {
      if (event.button !== 0 || event.target.closest("button")) return;
      if (window.matchMedia("(max-width: 640px)").matches) return;
      event.preventDefault();
      const node = pop.node;
      const rect = node.getBoundingClientRect();
      const offsetX = event.clientX - rect.left;
      const offsetY = event.clientY - rect.top;
      // 原先可能是贴在选区上方(用 bottom 定位),拖动统一换成 top/left。
      // 位置是用户亲手摆的,此后不再自动伸缩,只按视口留边收上限。
      const startMax = window.innerHeight - rect.top - 8;
      pop.manual = true;
      node.style.bottom = "";
      node.style.top = `${rect.top}px`;
      node.style.left = `${rect.left}px`;
      node.classList.add("is-dragging");
      handle.setPointerCapture(event.pointerId);
      const move = (moveEvent) => {
        const left = Math.min(Math.max(8, moveEvent.clientX - offsetX), window.innerWidth - rect.width - 8);
        // 标题栏始终留在视口里,拖到底部时内容区收矮而不是整个跑出屏幕。
        const top = Math.min(Math.max(8, moveEvent.clientY - offsetY), window.innerHeight - 48);
        node.style.left = `${left}px`;
        node.style.top = `${top}px`;
        node.style.maxHeight = `${Math.max(48, Math.min(startMax, window.innerHeight - top - 8))}px`;
      };
      const end = () => {
        node.classList.remove("is-dragging");
        handle.removeEventListener("pointermove", move);
        handle.removeEventListener("pointerup", end);
        handle.removeEventListener("pointercancel", end);
      };
      handle.addEventListener("pointermove", move);
      handle.addEventListener("pointerup", end);
      handle.addEventListener("pointercancel", end);
    });
  }

  /// 贴在选区下方,下方放不下且上方更宽就翻到上方;手机宽度由 CSS 改成底部面板。
  function placePopover(pop) {
    const gap = 8;
    const rect = pop.anchor;
    const node = pop.node;
    const width = Math.min(400, window.innerWidth - 16);
    node.style.width = `${width}px`;
    node.style.left = `${Math.min(Math.max(8, rect.left), window.innerWidth - width - 8)}px`;
    const below = window.innerHeight - rect.bottom - gap;
    const above = rect.top - gap;
    pop.side = below >= 240 || below >= above ? "below" : "above";
    applyPopoverSide(pop);
    fitPopover(pop);
  }

  /// 按 pop.side 把浮窗贴在选区的那一侧。
  function applyPopoverSide(pop) {
    const gap = 8;
    const rect = pop.anchor;
    const node = pop.node;
    if (pop.side === "above") {
      node.style.top = "";
      node.style.bottom = `${window.innerHeight - rect.top + gap}px`;
    } else {
      node.style.bottom = "";
      node.style.top = `${rect.bottom + gap}px`;
    }
  }

  /// 内容多长浮窗多长(09-28 用户要求:大小随回复伸缩)。上限是贴的那一侧还剩多少空白;
  /// 这一侧放不下、另一侧明显更宽裕时换一边,让字往上长,而不是把结果切在肚子里。
  /// 拖过的(pop.manual)不自动改位置——那是用户亲手摆的。
  function fitPopover(pop) {
    const node = pop.node;
    if (pop.manual || node.classList.contains("is-dragging")) return;
    // 手机宽度下 CSS 把它按成底部面板,这里不掺和。
    if (window.matchMedia("(max-width: 640px)").matches) return;
    const rect = pop.anchor;
    if (!rect || !node.isConnected) return;
    const margin = 8;
    const gap = 8;
    const desired = pop.head.offsetHeight + pop.foot.offsetHeight + pop.body.scrollHeight + 2;
    const space = {
      below: Math.max(120, window.innerHeight - rect.bottom - gap - margin),
      above: Math.max(120, rect.top - gap - margin),
    };
    // 换边的门槛:另一边要宽裕出 80px 才值当跳一下,否则内容在阈值附近会来回翻。
    if (desired > space[pop.side] && space[otherSide(pop.side)] > space[pop.side] + 80) {
      pop.side = otherSide(pop.side);
      applyPopoverSide(pop);
    }
    node.style.maxHeight = `${Math.max(120, Math.min(desired, space[pop.side]))}px`;
  }

  const otherSide = (side) => (side === "above" ? "below" : "above");

  function closePopover(pop) {
    pop.controller?.abort();
    pop.node.remove();
    const index = popovers.indexOf(pop);
    if (index >= 0) popovers.splice(index, 1);
    syncMarks();
  }

  function statusLine(text) {
    const node = el("div", "sel-status");
    node.append(el("span", "sel-spinner"), document.createTextNode(text));
    return node;
  }

  // ---------------- 解释 / 翻译 ----------------

  /// 汉字占多数就译成英文,否则译成中文;浮窗里可以切。
  function autoTarget(text) {
    const compact = text.replace(/\s+/g, "");
    const cjk = (compact.match(/[㐀-鿿豈-﫿]/g) || []).length;
    return compact && cjk * 2 >= compact.length ? "en" : "zh";
  }

  const LANGUAGES = [
    { code: "zh", label: "中文" },
    { code: "en", label: "英文" },
  ];

  function openAssist(kind, picked, targetLang) {
    const title = kind === "explain" ? "解释" : "翻译";
    const pop = createPopover(title, picked);
    pop.assist = { kind, picked, title };
    if (kind === "translate") {
      pop.lang = targetLang || autoTarget(picked.text);
      pop.head.insertBefore(languageSwitch(pop), pop.head.lastElementChild);
    }
    if (!picked.turnId) pop.body.appendChild(el("div", "sel-note", "这条消息还没落库,这次不带对话上下文。"));
    pop.output = el("div", "sel-output markdown-body");
    pop.body.appendChild(pop.output);
    startAssist(pop);
    return pop;
  }

  /// 翻译的目标语言:两个都摆在标题栏上,当前那个亮着;点另一个原地重来,浮窗不跳位置。
  function languageSwitch(pop) {
    const bar = el("div", "sel-switch");
    for (const { code, label } of LANGUAGES) {
      const chip = button("sel-chip", label, () => {
        if (code === pop.lang) return;
        pop.lang = code;
        for (const item of bar.children) {
          const active = item.dataset.lang === code;
          item.classList.toggle("is-active", active);
          item.setAttribute("aria-pressed", String(active));
        }
        restartAssist(pop);
      }, `译成${label}`);
      chip.dataset.lang = code;
      chip.classList.toggle("is-active", code === pop.lang);
      chip.setAttribute("aria-pressed", String(code === pop.lang));
      bar.appendChild(chip);
    }
    return bar;
  }

  /// 思考签:借用主对话「正在思考」那一套(三点弹跳 + 标题流光,样式在
  /// 24-reasoning-media.css),想完了收尾成「已思考」并挂上秒数。思考正文折叠在签里,
  /// 「显示 → 思考」设成 hidden 时只留签、不留正文。
  function thinkBlock() {
    const node = el("details", "reasoning-block sel-think is-live");
    const summary = el("summary", null);
    const icon = el("span", "reasoning-icon");
    icon.append(el("i"), el("i"), el("i"));
    const title = el("span", "reasoning-title", "正在思考");
    const status = el("span", "reasoning-live-status");
    summary.append(icon, title, status);
    const text = el("div", "reasoning-text sel-think-text");
    node.append(summary, text);
    if (ctx.reasoningHidden?.()) text.remove();
    return { node, title, status, text, raw: "", startedAt: performance.now(), done: false, frame: 0 };
  }

  function finalizeThink(pop) {
    const think = pop.think;
    if (!think || think.done) return;
    think.done = true;
    think.node.classList.remove("is-live");
    think.title.textContent = "已思考";
    think.status.textContent = `${((performance.now() - think.startedAt) / 1000).toFixed(1)}s`;
    if (think.frame) window.cancelAnimationFrame(think.frame);
    think.frame = 0;
    think.text.textContent = think.raw;
  }

  /// 一次旁路请求的全过程。切语言、重试都走 restartAssist:同一个浮窗,原地重来。
  function startAssist(pop) {
    const { kind, picked } = pop.assist;
    pop.controller?.abort();
    pop.text = "";
    pop.finished = false;
    pop.frame = 0;
    pop.think?.node.remove();
    if (pop.think?.frame) window.cancelAnimationFrame(pop.think.frame);
    pop.think = null;
    pop.status?.remove();
    pop.status = statusLine(kind === "explain" ? "正在解释…" : "正在翻译…");
    pop.body.insertBefore(pop.status, pop.output);
    pop.output.replaceChildren();
    renderAssistFoot(pop);

    pop.controller = new AbortController();
    streamAssist(
      {
        session_id: ctx.getSessionId(),
        turn_id: picked.turnId || null,
        action: kind,
        text: picked.text,
        target_lang: pop.lang || null,
      },
      pop.controller.signal,
      {
        reasoning(chunk) {
          const think = (pop.think ??= thinkBlock());
          if (!pop.think.node.isConnected) {
            pop.body.insertBefore(think.node, pop.status);
            // 签是长出来的,浮窗得跟着长:不然状态行会被挤到折叠线以下看不见。
            think.node.addEventListener("toggle", () => fitPopover(pop));
            fitPopover(pop);
          }
          think.raw += chunk;
          if (!think.frame) {
            think.frame = window.requestAnimationFrame(() => {
              think.frame = 0;
              think.text.textContent = think.raw;
            });
          }
        },
        delta(chunk) {
          pop.text += chunk;
          pop.status?.remove();
          pop.status = null;
          finalizeThink(pop);
          if (!pop.frame) pop.frame = window.requestAnimationFrame(() => paintAssist(pop));
        },
        done(event) {
          if (!pop.text && event?.text) pop.text = String(event.text);
          pop.status?.remove();
          pop.status = null;
          finalizeThink(pop);
          if (pop.frame) window.cancelAnimationFrame(pop.frame);
          paintAssist(pop);
          if (!pop.text) pop.output.textContent = "模型没有返回内容";
          pop.finished = true;
          renderAssistFoot(pop);
          fitPopover(pop);
        },
        error(message) {
          pop.status?.remove();
          pop.status = null;
          finalizeThink(pop);
          pop.body.appendChild(el("div", "sel-note is-error", message));
          pop.finished = Boolean(pop.text);
          renderAssistFoot(pop);
          fitPopover(pop);
        },
      }
    );
  }

  function restartAssist(pop) {
    startAssist(pop);
  }

  function paintAssist(pop) {
    pop.frame = 0;
    ctx.renderMarkdown(pop.output, pop.text);
    fitPopover(pop);
  }

  /// 页脚:复制 / 重试 / 转成追问。复制与追问要有内容才点得动。
  function renderAssistFoot(pop) {
    const ready = pop.finished && Boolean(pop.text);
    const copy = button("sel-btn", "复制", () => copyText(pop.text));
    const retry = button("sel-btn", "重试", () => restartAssist(pop));
    const ask = button("sel-btn", "转成追问", () => {
      quote(pop.assist.picked.text, `${pop.assist.title}:\n${pop.text}`);
      closePopover(pop);
    }, "把选中文字和这段结果一起放进输入框");
    copy.disabled = !ready;
    ask.disabled = !ready;
    pop.foot.replaceChildren(copy, retry, ask);
  }

  async function streamAssist(payload, signal, handlers) {
    let response;
    try {
      response = await ctx.apiRequest("/api/selection/assist", {
        method: "POST",
        body: JSON.stringify(payload),
        signal,
      });
    } catch (error) {
      if (!signal.aborted) handlers.error(error?.message || "请求失败");
      return;
    }
    const reader = response.body?.getReader();
    if (!reader) {
      handlers.error("浏览器不支持流式读取");
      return;
    }
    const decoder = new TextDecoder();
    let buffer = "";
    let finished = false;
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        let index;
        while ((index = buffer.indexOf("\n")) >= 0) {
          const line = buffer.slice(0, index).trim();
          buffer = buffer.slice(index + 1);
          if (!line) continue;
          let event;
          try {
            event = JSON.parse(line);
          } catch (_) {
            continue;
          }
          if (event.type === "delta") handlers.delta(String(event.text || ""));
          else if (event.type === "reasoning") handlers.reasoning?.(String(event.text || ""));
          else if (event.type === "done") {
            finished = true;
            handlers.done(event);
          } else if (event.type === "error") {
            finished = true;
            handlers.error(String(event.message || "请求失败"));
          }
        }
      }
    } catch (error) {
      if (!signal.aborted) handlers.error(error?.message || "连接中断");
      return;
    }
    if (!finished && !signal.aborted) handlers.error("连接提前结束,没有收到结果");
  }

  // ---------------- 搜索 ----------------

  function openSearch(picked) {
    const pop = createPopover("搜索", picked);
    const query = picked.text.replace(/\s+/g, " ").slice(0, 200);
    const kb = searchSection(pop.body, "知识库");
    const web = searchSection(pop.body, "网页");
    pop.controller = new AbortController();
    const { signal } = pop.controller;
    // 两栏是异步到的,谁到谁把浮窗量一次;不到就是它自己的错误提示撑着高度。
    ctx.apiRequest(`/api/dash/kb/search?q=${encodeURIComponent(query)}&limit=5`, { signal })
      .then((response) => response.json())
      .then((data) => {
        renderKb(kb, data);
        fitPopover(pop);
      })
      .catch((error) => {
        if (signal.aborted) return;
        sectionError(kb, error);
        fitPopover(pop);
      });
    ctx.apiRequest(`/api/selection/web-search?q=${encodeURIComponent(query)}`, { signal })
      .then((response) => response.json())
      .then((data) => {
        renderWeb(web, data);
        fitPopover(pop);
      })
      .catch((error) => {
        if (signal.aborted) return;
        sectionError(web, error);
        fitPopover(pop);
      });
    pop.foot.append(
      button("sel-btn", "复制关键词", () => copyText(query)),
      button("sel-btn", "引用追问", () => {
        quote(picked.text);
        closePopover(pop);
      })
    );
  }

  function searchSection(parent, label) {
    const section = el("section", "sel-section");
    const body = el("div", "sel-section-body");
    body.appendChild(statusLine("正在搜索…"));
    section.append(el("div", "sel-section-head", label), body);
    parent.appendChild(section);
    return body;
  }

  function sectionError(body, error) {
    body.replaceChildren(el("div", "sel-note is-error", error?.message || "搜索失败"));
  }

  function renderKb(body, data) {
    body.replaceChildren();
    const results = Array.isArray(data?.results) ? data.results : [];
    if (!results.length) {
      body.appendChild(el("div", "sel-empty", "知识库里没有匹配"));
      return;
    }
    for (const item of results.slice(0, 5)) {
      const path = String(item.path || item.rel_path || item.file || item.name || item.title || "");
      const snippet = String(item.snippet || item.excerpt || item.preview || item.content || item.text || "")
        .replace(/\s+/g, " ")
        .slice(0, 140);
      // 侧栏临时视图(file-links 步 2)建好之前,点一下先复制路径。
      const row = button("sel-result", null, () => copyText(path, "已复制文件路径"), "复制路径");
      row.appendChild(el("span", "sel-result-title", path || "(无路径)"));
      if (snippet) row.appendChild(el("span", "sel-result-snippet", snippet));
      body.appendChild(row);
    }
  }

  function renderWeb(body, data) {
    body.replaceChildren();
    const output = String(data?.output || "").trim();
    if (!output) {
      body.appendChild(el("div", "sel-empty", "网页搜索没有结果"));
      return;
    }
    let parsed = null;
    try {
      parsed = JSON.parse(output);
    } catch (_) {
      parsed = null;
    }
    const results = Array.isArray(parsed?.results) ? parsed.results : null;
    if (!results) {
      const markdown = el("div", "sel-output markdown-body");
      ctx.renderMarkdown(markdown, output);
      body.appendChild(markdown);
      return;
    }
    for (const item of results.slice(0, 6)) {
      const url = String(item.url || item.link || "");
      const title = String(item.title || url || "(无标题)");
      const snippet = String(item.snippet || item.content || item.description || "").replace(/\s+/g, " ").slice(0, 140);
      // 只放行 http(s) 链接:结果来自外部搜索服务,是不可信数据。
      const safe = /^https?:\/\//i.test(url);
      const row = safe ? el("a", "sel-result") : el("div", "sel-result");
      if (safe) {
        row.href = url;
        row.target = "_blank";
        row.rel = "noopener noreferrer";
      }
      row.appendChild(el("span", "sel-result-title", title));
      if (snippet) row.appendChild(el("span", "sel-result-snippet", snippet));
      if (url) row.appendChild(el("span", "sel-result-url", url));
      body.appendChild(row);
    }
  }

  // ---------------- 手机工具条 ----------------

  function onSelectionChange() {
    window.clearTimeout(selectionTimer);
    selectionTimer = window.setTimeout(() => {
      const picked = readSelection();
      if (!picked) {
        // 手机上高亮也一样自己画:系统长按菜单把原生高亮压暗之后,还得看得见选了什么。
        hideToolbar();
        clearMarks();
        return;
      }
      current = picked;
      setMarkRange(picked.range);
      showToolbar(picked.rect);
    }, 300);
  }

  function showToolbar(rect) {
    if (!toolbar) {
      toolbar = el("div", "sel-toolbar");
      toolbar.setAttribute("role", "toolbar");
      for (const action of ACTIONS) toolbar.appendChild(button("sel-toolbar-item", action.label, () => runAction(action.key)));
      toolbar.appendChild(button("sel-toolbar-item", "复制", () => {
        copySelection();
        hideToolbar();
      }));
      // 点工具条别让系统先把选区收掉。
      toolbar.addEventListener("pointerdown", (event) => event.preventDefault());
      document.body.appendChild(toolbar);
    }
    toolbar.hidden = false;
    const { width, height } = toolbar.getBoundingClientRect();
    const below = rect.bottom + 10;
    toolbar.style.top = `${below + height < window.innerHeight - 8 ? below : Math.max(8, rect.top - height - 10)}px`;
    toolbar.style.left = `${Math.min(Math.max(8, rect.left + rect.width / 2 - width / 2), window.innerWidth - width - 8)}px`;
  }

  function hideToolbar() {
    if (toolbar) toolbar.hidden = true;
  }

  return { mount };
})();
