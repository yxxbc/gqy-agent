"use strict";

/*
 * 悬浮窗的页面逻辑(src/pet/web/app.js,编译进二进制)。
 *
 * 三件事:
 *   1. 把鼠标动作翻成指令发回 Rust 侧(window.ipc.postMessage):
 *      拖动 / 双击开 WebUI / 右键菜单 / 关闭;
 *   2. 有模型就加载 Live2D(资源走 gqy-pet:// 协议,见 src/pet/assets.rs),
 *      加载中先显示静态立绘,失败就留在立绘上并把原因回报给 Rust;
 *   3. 按 daemon 推来的状态(idle / thinking / speaking)做表现:
 *      状态点、人物外的光,以及模型的嘴型(能驱动就驱动,不能就只是光)。
 *
 * 模型、参数名这些都不猜:模型给什么用什么,给不了就退回立绘。
 */

const send = (message) => {
  try {
    window.ipc?.postMessage(JSON.stringify(message));
  } catch (_) {}
};

/* Rust 侧经 initialization script 注入:模型清单的 URL(没有模型就是 null)。 */
const CONFIG = window.__PET__ || {};

const stage = document.getElementById("stage");
const portrait = document.getElementById("portrait");
const dot = document.getElementById("dot");

let petState = "idle";
let live2d = null; // { app, model }

// ---------------- 指令 ----------------

/* 拖动交给系统原生拖动:按住往下走 4px 才算拖,免得双击被当成两次拖动。 */
let pressed = false;
let dragging = false;
let startX = 0;
let startY = 0;

document.addEventListener("pointerdown", (event) => {
  if (event.button !== 0 || event.target.closest("#menu, #close")) return;
  pressed = true;
  dragging = false;
  startX = event.clientX;
  startY = event.clientY;
});

document.addEventListener("pointermove", (event) => {
  if (!pressed || dragging) return;
  if (Math.hypot(event.clientX - startX, event.clientY - startY) < 4) return;
  dragging = true;
  document.body.classList.add("is-dragging");
  send({ cmd: "drag" });
});

const release = () => {
  pressed = false;
  dragging = false;
  document.body.classList.remove("is-dragging");
};
window.addEventListener("pointerup", release);
window.addEventListener("pointercancel", release);
// 原生拖动一开始浏览器就收不到 pointerup 了:鼠标出窗或失焦也要复位。
window.addEventListener("blur", release);
document.addEventListener("pointerleave", release);

document.addEventListener("dblclick", (event) => {
  if (event.target.closest("#menu, #close")) return;
  send({ cmd: "open_webui" });
});

document.getElementById("close").addEventListener("click", () => send({ cmd: "close" }));
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") send({ cmd: "close" });
});

// ---------------- 右键菜单 ----------------

const menu = document.getElementById("menu");
const item = (act) => menu.querySelector(`[data-act="${act}"]`);
const PET_STATE = CONFIG.config || { on_top: true, scale: 1 };

const refreshMenu = () => {
  item("on_top").querySelector(".check").textContent = PET_STATE.on_top ? "✓" : "";
  item("bigger").disabled = PET_STATE.scale >= 2;
  item("smaller").disabled = PET_STATE.scale <= 0.5;
};

document.addEventListener("contextmenu", (event) => {
  event.preventDefault();
  refreshMenu();
  menu.hidden = false;
  // 菜单不许跑出窗口:窗口只有人物那么大,贴边出去就等于点不到。
  const box = menu.getBoundingClientRect();
  menu.style.left = `${Math.max(4, Math.min(event.clientX, window.innerWidth - box.width - 4))}px`;
  menu.style.top = `${Math.max(4, Math.min(event.clientY, window.innerHeight - box.height - 4))}px`;
});

menu.addEventListener("click", (event) => {
  const act = event.target.closest("button")?.dataset.act;
  if (!act) return;
  menu.hidden = true;
  if (act === "on_top") {
    PET_STATE.on_top = !PET_STATE.on_top;
    send({ cmd: "set_on_top", on: PET_STATE.on_top });
  } else if (act === "bigger" || act === "smaller") {
    const step = act === "bigger" ? 0.1 : -0.1;
    // 一位小数:反复加减不该攒出 0.7000000000000001
    PET_STATE.scale = Math.round(Math.min(2, Math.max(0.5, PET_STATE.scale + step)) * 10) / 10;
    send({ cmd: "set_scale", scale: PET_STATE.scale });
  } else if (act === "webui") {
    send({ cmd: "open_webui" });
  } else if (act === "close") {
    send({ cmd: "close" });
  }
});

// 点别处收菜单。capture 阶段听:菜单里的按钮自己会先把 click 处理掉。
document.addEventListener(
  "pointerdown",
  (event) => {
    if (!menu.hidden && !menu.contains(event.target)) menu.hidden = true;
  },
  true
);

// ---------------- 状态表现 ----------------

const STATE_CLASS = { idle: "", thinking: "is-thinking", speaking: "is-speaking" };

window.gqyPet = {
  setState(name) {
    petState = name;
    document.body.dataset.state = name;
    dot.className = STATE_CLASS[name] ?? "";
  },
};

/* 说话时让嘴动。**只认 Cubism 的标准参数名**:用户模型的自定义参数名(这份模型是
   中文标注的)猜不出来,猜错的代价是人物乱动,不如不动。模型没绑这个参数时
   `setParameterValueById` 会抛,这里吞掉并把它关掉。 */
let mouthParam = CONFIG.model_url ? "ParamMouthOpenY" : null;

function driveMouth(core) {
  if (!mouthParam) return;
  const open =
    petState === "speaking" ? 0.3 + 0.5 * Math.abs(Math.sin(performance.now() / 85)) : 0;
  try {
    core.setParameterValueById(mouthParam, open);
  } catch (_) {
    send({ cmd: "model_note", message: `模型没有 ${mouthParam} 参数,嘴型不动` });
    mouthParam = null;
  }
}

// ---------------- Live2D ----------------

const VENDOR = [
  "gqy-pet://app/vendor/live2dcubismcore.min.js",
  "gqy-pet://app/vendor/pixi.min.js",
  "gqy-pet://app/vendor/cubism4.min.js",
];

function loadScript(url) {
  return new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = url;
    script.onload = () => resolve();
    script.onerror = () => reject(new Error(`加载失败: ${url}`));
    document.head.appendChild(script);
  });
}

/* 让模型铺满窗口:等比缩放到放得下,再贴底居中(人物站在窗口下沿)。 */
function fit(model, app) {
  const { width, height } = app.renderer.screen;
  const inner = model.internalModel;
  const scale = Math.min(width / inner.width, height / inner.height);
  model.scale.set(scale);
  model.x = (width - inner.width * scale) / 2;
  model.y = height - inner.height * scale;
}

async function startLive2d() {
  for (const url of VENDOR) await loadScript(url);
  const app = new PIXI.Application({
    // pixi v7 没有 `transparent` 这个选项(那是 v6 的),画布默认是**不透明黑**——
    // 透明窗里那就是一块黑板。要的是把画布底调的 alpha 归零。
    backgroundAlpha: 0,
    antialias: true,
    autoDensity: true,
    resolution: window.devicePixelRatio || 1,
    resizeTo: window,
  });
  document.getElementById("live2d").appendChild(app.view);

  const model = await PIXI.live2d.Live2DModel.from(CONFIG.model_url, {
    autoInteract: false,
    autoUpdate: true,
  });
  app.stage.addChild(model);
  fit(model, app);
  window.addEventListener("resize", () => fit(model, app));

  // 待机动作:清单里的动作组按文件名命名(idle → "Idle",见 src/pet/model.rs)。
  // 动作本身循环就循环,不循环的播完再接一次。
  try {
    model.motion("Idle");
    model.on("motionFinish", () => model.motion("Idle"));
  } catch (error) {
    send({ cmd: "model_note", message: `待机动作没跑起来: ${error}` });
  }

  // 每帧驱动:嘴型要写在模型 update 之前,pixi-live2d-display 提供了这个钩子。
  const manager = model.internalModel;
  try {
    manager.on("beforeModelUpdate", () => driveMouth(manager.coreModel));
  } catch (error) {
    send({ cmd: "model_note", message: `拿不到逐帧钩子,嘴型不动: ${error}` });
  }

  live2d = { app, model, mouth: true };
  document.body.classList.add("has-model");
  return { app, model };
}

if (CONFIG.model_url) {
  startLive2d()
    .then(({ app, model }) => {
      const motionCount = model.internalModel.motionManager.definitions
        ? Object.keys(model.internalModel.motionManager.definitions).length
        : 0;
      const expressionCount = model.internalModel.motionManager.expressionManager
        ? model.internalModel.motionManager.expressionManager.definitions?.length || 0
        : 0;
      send({
        cmd: "model_ready",
        motions: motionCount,
        expressions: expressionCount,
        width: Math.round(model.internalModel.width),
        height: Math.round(model.internalModel.height),
        // 这两个值放在一起报:窗口要是又变成不透明,一眼能看出是画布底还是页面底
        // (用户 09-28 报过一次「背景不透明」,当时只能靠猜)。
        alpha: app.renderer.background.alpha,
        body: getComputedStyle(document.body).backgroundColor,
      });
    })
    .catch((error) => {
      // 退回静态立绘:窗口还在,只是不会动。
      document.body.classList.remove("has-model");
      send({ cmd: "model_failed", message: String(error?.message || error) });
    });
}

/* 加载完报一声:立绘自然宽度 > 0 说明内联的图真的解码出来了,
   而不是一块空白窗。Rust 侧把它写进日志。 */
window.addEventListener("load", () => {
  send({ cmd: "ready", width: portrait.naturalWidth || 0 });
});
