/// 进普通模式的粒子幕:把她的壁纸拆成粒子,从四散处聚拢成整图。
/// (去开发模式那一段原来是吹散,已换成 hud-boot.js 的 HUD 启动序列。)
///
/// 画在宿主元素里的一层 canvas 上,pointer-events: none,不挡任何操作;
/// 用户按键或点击就提前收尾。系统要求减少动效、页面在后台时直接跳过。
///
/// 粒子阶段在 CSS 像素分辨率的离屏 ImageData 上逐块写像素,再整张放大
/// 画到主 canvas(几千次 fillRect 换 fillStyle 太慢);聚拢完成后淡入
/// 原图的高清版本,「粒子 → 完整图片」这一步才看得到清晰的收束。

const MAX_PARTICLES = 7000;
const ASSEMBLE_MS = 1500;
const SHARPEN_MS = 450;
const HOLD_MS = 350;
const FADE_MS = 700;

let active = null;

function reducedMotion() {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
}

function easeOutCubic(t) {
  return 1 - (1 - t) ** 3;
}

function loadImage(url) {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.decoding = "async";
    image.onload = () => resolve(image);
    image.onerror = reject;
    image.src = url;
  });
}

/// object-fit: cover 的几何:图铺满 w×h,居中裁切。
function coverRect(image, width, height) {
  const scale = Math.max(width / image.naturalWidth, height / image.naturalHeight);
  const drawWidth = image.naturalWidth * scale;
  const drawHeight = image.naturalHeight * scale;
  return { x: (width - drawWidth) / 2, y: (height - drawHeight) / 2, width: drawWidth, height: drawHeight };
}

function sampleParticles(image, width, height) {
  const step = Math.max(4, Math.ceil(Math.sqrt((width * height) / MAX_PARTICLES)));
  const sampler = document.createElement("canvas");
  sampler.width = width;
  sampler.height = height;
  const context = sampler.getContext("2d", { willReadFrequently: true });
  const rect = coverRect(image, width, height);
  context.drawImage(image, rect.x, rect.y, rect.width, rect.height);
  const pixels = context.getImageData(0, 0, width, height).data;
  const particles = [];
  const reach = Math.max(width, height) * 0.7;
  for (let y = 0; y < height; y += step) {
    for (let x = 0; x < width; x += step) {
      const offset = (Math.min(height - 1, y + (step >> 1)) * width + Math.min(width - 1, x + (step >> 1))) * 4;
      const angle = Math.random() * Math.PI * 2;
      const distance = reach * (0.35 + Math.random() * 0.65);
      particles.push({
        tx: x,
        ty: y,
        // 从四散处飞回原位;上方的先到,像墨从上往下晕开。
        dx: Math.cos(angle) * distance,
        dy: Math.sin(angle) * distance,
        delay: Math.random() * 0.35 + (y / height) * 0.15,
        r: pixels[offset],
        g: pixels[offset + 1],
        b: pixels[offset + 2]
      });
    }
  }
  return { particles, step, rect };
}

function drawParticles(buffer, width, height, particles, step, progress) {
  const data = buffer.data;
  data.fill(0);
  const span = 0.6;
  for (const particle of particles) {
    const local = Math.min(1, Math.max(0, (progress - particle.delay) / span));
    // travel 从 1(远处)走到 0(原位)。
    const travel = 1 - easeOutCubic(local);
    const alpha = Math.min(1, local * 1.6);
    if (alpha <= 0) continue;
    const size = Math.max(2, Math.round(step * (0.35 + 0.65 * local)));
    const px = Math.round(particle.tx + particle.dx * travel);
    const py = Math.round(particle.ty + particle.dy * travel);
    if (px >= width || py >= height || px + size <= 0 || py + size <= 0) continue;
    const a = Math.round(alpha * 255);
    const x0 = Math.max(0, px);
    const y0 = Math.max(0, py);
    const x1 = Math.min(width, px + size);
    const y1 = Math.min(height, py + size);
    for (let y = y0; y < y1; y += 1) {
      let index = (y * width + x0) * 4;
      for (let x = x0; x < x1; x += 1) {
        data[index] = particle.r;
        data[index + 1] = particle.g;
        data[index + 2] = particle.b;
        data[index + 3] = a;
        index += 4;
      }
    }
  }
}

export function cancelParticleVeil() {
  active?.finish(true);
}

let pendingVisible = null;

/// 在 host 里播一次粒子幕:聚成整图。
///
/// 页面在后台(后台打开的标签页刷新)时先记下,等切到前台那一刻再播;
/// 期间又来一次就只保留最新的。
export function playParticleVeil(host, options = {}) {
  if (!host || !options.imageUrl || reducedMotion()) return;
  if (pendingVisible) {
    document.removeEventListener("visibilitychange", pendingVisible);
    pendingVisible = null;
  }
  if (!document.hidden) {
    runParticleVeil(host, options);
    return;
  }
  pendingVisible = () => {
    if (document.hidden) return;
    document.removeEventListener("visibilitychange", pendingVisible);
    pendingVisible = null;
    runParticleVeil(host, options);
  };
  document.addEventListener("visibilitychange", pendingVisible);
}

async function runParticleVeil(host, { imageUrl }) {
  cancelParticleVeil();
  const width = host.clientWidth;
  const height = host.clientHeight;
  if (width < 40 || height < 40) return;
  const token = {};
  active = { token, finish: () => {} };
  let image;
  try {
    image = await loadImage(imageUrl);
  } catch (_) {
    if (active?.token === token) active = null;
    return;
  }
  if (active?.token !== token) return;

  const { particles, step, rect } = sampleParticles(image, width, height);
  const dpr = Math.min(2, window.devicePixelRatio || 1);
  const veil = document.createElement("div");
  veil.className = "particle-veil";
  veil.setAttribute("aria-hidden", "true");
  const canvas = document.createElement("canvas");
  canvas.width = Math.round(width * dpr);
  canvas.height = Math.round(height * dpr);
  veil.appendChild(canvas);
  host.appendChild(veil);
  const context = canvas.getContext("2d");
  const buffer = new ImageData(width, height);
  const scratch = document.createElement("canvas");
  scratch.width = width;
  scratch.height = height;
  const scratchContext = scratch.getContext("2d");

  let frame = 0;
  let done = false;
  const started = performance.now();
  const particleMs = ASSEMBLE_MS;

  const cleanup = () => {
    window.removeEventListener("keydown", hurry, true);
    window.removeEventListener("pointerdown", hurry, true);
    window.cancelAnimationFrame(frame);
    veil.remove();
    if (active?.token === token) active = null;
  };
  const fadeOut = (ms) => {
    if (done) return;
    done = true;
    window.cancelAnimationFrame(frame);
    veil.style.transitionDuration = `${ms}ms`;
    veil.classList.add("is-leaving");
    window.setTimeout(cleanup, ms + 40);
  };
  // 用户动手了就别让动画挡着:快速淡出。
  function hurry() {
    fadeOut(180);
  }
  active.finish = (immediate) => {
    if (!immediate) {
      fadeOut(180);
      return;
    }
    done = true;
    cleanup();
  };
  window.addEventListener("keydown", hurry, true);
  window.addEventListener("pointerdown", hurry, true);

  const render = (now) => {
    if (done) return;
    const elapsed = now - started;
    const progress = Math.min(1, elapsed / particleMs);
    context.setTransform(1, 0, 0, 1, 0, 0);
    context.clearRect(0, 0, canvas.width, canvas.height);
    drawParticles(buffer, width, height, particles, step, progress);
    scratchContext.putImageData(buffer, 0, 0);
    context.imageSmoothingEnabled = false;
    context.drawImage(scratch, 0, 0, canvas.width, canvas.height);
    if (elapsed > particleMs) {
      // 粒子到位后淡入原图的清晰版本,收成完整图片。
      const sharpen = Math.min(1, (elapsed - particleMs) / SHARPEN_MS);
      context.globalAlpha = sharpen;
      context.imageSmoothingEnabled = true;
      context.drawImage(image, rect.x * dpr, rect.y * dpr, rect.width * dpr, rect.height * dpr);
      context.globalAlpha = 1;
      if (elapsed > particleMs + SHARPEN_MS + HOLD_MS) {
        fadeOut(FADE_MS);
        return;
      }
    }
    frame = window.requestAnimationFrame(render);
  };
  frame = window.requestAnimationFrame(render);
}
