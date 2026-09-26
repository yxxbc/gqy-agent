"use strict";

/*
 * 寄信卡片：`send_letter` 的结果画成一只信封，点开是浮层里铺开的花笺。
 *
 * 与地图、快递同机制：挂在 features/tools/cards.js 的 TOOL_RICH_CARDS 上，
 * 回看重建 / 子过程回放 / 实时完成三条路共用这一份画法，少挂一处就会出现
 * 「实时有、刷新没了」。挂法用 outside——信封是给人看的交付物，工具签收起
 * 时也该看得见。
 *
 * 信封的四片折页在中心交汇（左右两折从两个角指向中心、下折从底边翻上来、
 * 上折最后压上），折痕用 drop-shadow 画；拆印 → 翻盖 → 铺纸 → 落字 → 落印
 * 那串动效先在桌面的 demo 里调好，再搬到这里。
 *
 * 信封上的名字读侧栏品牌位（人格名），蜡印取名字最后一个字——换人格、改
 * 名字都不用改这份代码。
 */

window.GqyLetter = (() => {
  const BRANCH_SVG = `
    <svg viewBox="0 0 120 60" aria-hidden="true">
      <path d="M4 54 C26 46 42 34 60 24 C78 14 96 10 116 12"></path>
      <path d="M60 24 C58 34 60 42 66 50"></path>
      <path d="M96 12 C92 22 90 30 92 40"></path>
      <g transform="translate(60 22)"><circle r="2.2" cx="0" cy="-3"></circle><circle r="2.2" cx="2.8" cy="-0.9"></circle><circle r="2.2" cx="1.8" cy="2.4"></circle><circle r="2.2" cx="-1.8" cy="2.4"></circle><circle r="2.2" cx="-2.8" cy="-0.9"></circle></g>
      <g transform="translate(96 10) scale(.8)"><circle r="2.2" cx="0" cy="-3"></circle><circle r="2.2" cx="2.8" cy="-0.9"></circle><circle r="2.2" cx="1.8" cy="2.4"></circle><circle r="2.2" cx="-1.8" cy="2.4"></circle><circle r="2.2" cx="-2.8" cy="-0.9"></circle></g>
      <g transform="translate(28 44) scale(.62)"><circle r="2.2" cx="0" cy="-3"></circle><circle r="2.2" cx="2.8" cy="-0.9"></circle><circle r="2.2" cx="1.8" cy="2.4"></circle><circle r="2.2" cx="-1.8" cy="2.4"></circle><circle r="2.2" cx="-2.8" cy="-0.9"></circle></g>
    </svg>`;
  const MARK_SVG = `
    <svg viewBox="0 0 140 140" aria-hidden="true">
      <circle cx="70" cy="40" r="12"></circle><circle cx="96" cy="58" r="12"></circle>
      <circle cx="88" cy="88" r="12"></circle><circle cx="52" cy="88" r="12"></circle>
      <circle cx="44" cy="58" r="12"></circle><circle cx="70" cy="66" r="5"></circle>
      <path d="M8 128 C36 118 52 100 70 86" fill="none" stroke="#8a5f66" stroke-width="2.4"></path>
    </svg>`;
  const SPRIG_SVG = `
    <svg class="letter-sprig" viewBox="0 0 52 14" aria-hidden="true">
      <path d="M2 12 C16 10 28 7 50 3"></path>
      <circle cx="18" cy="8" r="2.6"></circle><circle cx="34" cy="5" r="2.2"></circle>
    </svg>`;

  function isLetterTool(name) {
    return String(name || "") === "send_letter";
  }

  function personaName() {
    const name = document.getElementById("brandName")?.textContent?.trim();
    return name || "她";
  }

  function parse(output) {
    let data;
    try {
      data = JSON.parse(String(output || ""));
    } catch {
      return null;
    }
    if (!data || typeof data !== "object" || data.ok === false) return null;
    const lines = String(data.body || "")
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
    if (!lines.length) return null;
    return {
      salutation: String(data.salutation || "").trim(),
      lines,
      signature: String(data.signature || "").trim()
    };
  }

  function dateLabel(now = new Date()) {
    const months = ["一","二","三","四","五","六","七","八","九","十","十一","十二"];
    const days = ["","一","二","三","四","五","六","七","八","九","十","十一","十二","十三","十四",
      "十五","十六","十七","十八","十九","二十","廿一","廿二","廿三","廿四","廿五","廿六","廿七",
      "廿八","廿九","三十","卅一"];
    return `${months[now.getMonth()]}月${days[now.getDate()]}`;
  }

  function envelopeMarkup() {
    const name = personaName();
    const seal = Array.from(name).slice(-1)[0] || "影";
    return `
      <span class="letter-env-paper"></span>
      <span class="letter-env-frame"></span>
      <span class="letter-env-pocket"></span>
      <span class="letter-env-branch" aria-hidden="true">${BRANCH_SVG}</span>
      <span class="letter-env-address">${name} 手缄</span>
      <span class="letter-env-seal">${seal}</span>
      <span class="letter-env-flap"></span>`;
  }

  function renderCard(output) {
    const letter = parse(output);
    if (!letter) return null;
    const card = document.createElement("div");
    card.className = "letter-card";
    const envelope = document.createElement("button");
    envelope.type = "button";
    envelope.className = "letter-envelope";
    envelope.setAttribute("aria-label", "打开信封");
    envelope.innerHTML = envelopeMarkup();
    const hint = document.createElement("p");
    hint.className = "letter-hint";
    hint.textContent = "点开，是给你的";
    envelope.addEventListener("click", () => open(letter, envelope, hint));
    card.append(envelope, hint);
    return card;
  }

  /* ── 浮层：信封从卡片里飞进中央，拆印 → 翻盖 → 铺纸 → 落字 → 落印 ── */
  let veil = null;
  let parts = null;
  let current = null;
  let phase = "idle";
  const timers = [];
  const later = (ms, fn) => timers.push(setTimeout(fn, ms));
  const reduced = () => window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
  const ms = (value) => (reduced() ? 0 : value);

  function ensureVeil() {
    if (veil) return parts;
    veil = document.createElement("div");
    veil.className = "letter-veil";
    veil.hidden = true;
    veil.innerHTML = `
      <div class="letter-veil-glow" aria-hidden="true"></div>
      <div class="letter-veil-motes" aria-hidden="true"></div>
      <article class="letter-paper" hidden>
        <span class="letter-paper-mark" aria-hidden="true">${MARK_SVG}</span>
        <p class="letter-kicker is-line">${SPRIG_SVG}<span>见字如面</span>${SPRIG_SVG.replace(
          'class="letter-sprig"',
          'class="letter-sprig is-flip"'
        )}</p>
        <p class="letter-salutation is-line"></p>
        <div class="letter-body"></div>
        <p class="letter-sign is-line"></p>
        <span class="letter-stamp"></span>
      </article>
      <div class="letter-actions" hidden>
        <button type="button" class="letter-replay">再看一遍</button>
        <button type="button" class="letter-close">收好</button>
      </div>
      <button type="button" class="letter-x" aria-label="关闭">✕</button>`;
    const motes = veil.querySelector(".letter-veil-motes");
    for (let i = 0; i < 14; i++) {
      const mote = document.createElement("span");
      mote.className = "letter-mote";
      mote.style.cssText =
        `left:${Math.random() * 100}%;animation-duration:${9 + Math.random() * 9}s;` +
        `animation-delay:${-Math.random() * 14}s;` +
        `transform:scale(${(0.6 + Math.random()).toFixed(2)})`;
      motes.appendChild(mote);
    }
    parts = {
      root: veil,
      paper: veil.querySelector(".letter-paper"),
      kicker: veil.querySelector(".letter-kicker"),
      salutation: veil.querySelector(".letter-salutation"),
      body: veil.querySelector(".letter-body"),
      sign: veil.querySelector(".letter-sign"),
      stamp: veil.querySelector(".letter-stamp"),
      actions: veil.querySelector(".letter-actions")
    };
    veil.querySelector(".letter-replay").addEventListener("click", replayOpen);
    veil.querySelector(".letter-close").addEventListener("click", close);
    veil.querySelector(".letter-x").addEventListener("click", close);
    veil.addEventListener("click", (event) => {
      if (event.target === veil || event.target.classList.contains("letter-veil-glow")) close();
    });
    window.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && phase !== "idle") close();
    });
    document.body.appendChild(veil);
    return parts;
  }

  function fillLetter(ui, letter) {
    const name = personaName();
    ui.salutation.textContent = letter.salutation || "致你：";
    ui.body.replaceChildren();
    for (const line of letter.lines) {
      const node = document.createElement("p");
      node.className = "is-line";
      node.textContent = line;
      ui.body.appendChild(node);
    }
    ui.sign.textContent = letter.signature || `—— ${name} · ${dateLabel()}`;
    ui.stamp.textContent = Array.from(name).slice(-1)[0] || "影";
  }

  function targetSize() {
    const width = Math.min(380, Math.round(innerWidth * 0.78));
    return { width, height: Math.round((width * 190) / 300) };
  }

  function open(letter, envelope, hint) {
    if (phase !== "idle") return;
    phase = "flying";
    const ui = ensureVeil();
    const home = envelope.parentElement;
    fillLetter(ui, letter);
    current = { letter, envelope, hint, home };
    const first = envelope.getBoundingClientRect();
    envelope.classList.add("is-flying");
    Object.assign(envelope.style, {
      left: `${first.left}px`,
      top: `${first.top}px`,
      width: `${first.width}px`,
      height: `${first.height}px`
    });
    document.body.appendChild(envelope);
    hint.hidden = true;
    ui.root.hidden = false;
    requestAnimationFrame(() => ui.root.classList.add("is-shown"));
    requestAnimationFrame(() => {
      const size = targetSize();
      Object.assign(envelope.style, {
        left: `${Math.round((innerWidth - size.width) / 2)}px`,
        top: `${Math.round((innerHeight - size.height) / 2 - 8)}px`,
        width: `${size.width}px`,
        height: `${size.height}px`
      });
    });
    later(ms(700), beginOpen);
  }

  function beginOpen() {
    phase = "opening";
    const ui = parts;
    const envelope = current.envelope;
    const lines = [ui.kicker, ui.salutation, ...ui.body.querySelectorAll(".is-line"), ui.sign];
    ui.paper.hidden = false;
    envelope.querySelector(".letter-env-seal")?.classList.add("is-broken");
    later(ms(200), () => envelope.classList.add("is-open"));
    later(ms(640), () => {
      envelope.classList.add("is-gone");
      ui.paper.classList.add("is-open");
    });
    lines.forEach((line, index) => later(ms(1100 + index * 130), () => line.classList.add("is-in")));
    later(ms(2450), () => ui.stamp.classList.add("is-pressed"));
    later(ms(2700), () => {
      ui.actions.hidden = false;
      phase = "open";
    });
  }

  function close() {
    if (phase === "idle" || phase === "closing") return;
    phase = "closing";
    const ui = parts;
    const { envelope, hint, home } = current || {};
    timers.splice(0).forEach(clearTimeout);
    ui.root.classList.remove("is-shown");
    ui.paper.classList.remove("is-open");
    later(380, () => {
      ui.root.hidden = true;
      ui.actions.hidden = true;
      ui.paper.hidden = true;
      ui.paper.querySelectorAll(".is-in").forEach((line) => line.classList.remove("is-in"));
      ui.stamp.classList.remove("is-pressed");
      if (envelope) {
        envelope.classList.remove("is-flying", "is-open", "is-gone");
        envelope.removeAttribute("style");
        envelope.querySelector(".letter-env-seal")?.classList.remove("is-broken");
        if (home) home.insertBefore(envelope, home.firstChild);
      }
      if (hint) hint.hidden = false;
      phase = "idle";
    });
  }

  function replayOpen() {
    if (!current || (phase !== "open" && phase !== "opening")) return;
    const { letter, envelope, hint } = current;
    close();
    later(ms(760), () => {
      if (phase !== "idle") return;
      open(letter, envelope, hint);
    });
  }

  return { isLetterTool, renderCard };
})();
