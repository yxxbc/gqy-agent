/// 普通模式正文背后的落梅:十来片花瓣很慢地飘过,是整个界面唯一常驻的动效。
///
/// 画在 host(正文区)最底下一层 canvas 上,排在对话之前,文字永远压在花瓣
/// 上面;输入坞有底色,花瓣飘到那里就被挡住。页面在后台、系统要求减少动效、
/// 切去开发模式时停下并清空。颜色读 CSS 变量 --petal,主题换了下一片就跟上。
const PETAL_COUNT = 12;

const petalState = {
  canvas: null,
  context: null,
  petals: [],
  frame: 0,
  last: 0,
  running: false,
  observer: null,
  host: null,
  enabled: false
};

function reducedMotion() {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
}

function petalColor() {
  return getComputedStyle(petalState.host || document.body).getPropertyValue("--petal").trim() || "rgba(255, 214, 224, 0.5)";
}

function spawn(width, height, anywhere) {
  const size = 6 + Math.random() * 6;
  return {
    x: Math.random() * width,
    // 开场时撒满整屏,之后从顶上一片片进来。
    y: anywhere ? Math.random() * height : -size * 2,
    size,
    fall: 14 + Math.random() * 18,
    drift: -8 + Math.random() * 16,
    sway: 10 + Math.random() * 18,
    phase: Math.random() * Math.PI * 2,
    spin: (Math.random() - 0.5) * 1.2,
    angle: Math.random() * Math.PI,
    alpha: 0.35 + Math.random() * 0.45,
    color: petalColor()
  };
}

function resize() {
  const { canvas, host } = petalState;
  if (!canvas || !host) return;
  const dpr = Math.min(2, window.devicePixelRatio || 1);
  canvas.width = Math.round(host.clientWidth * dpr);
  canvas.height = Math.round(host.clientHeight * dpr);
  petalState.context?.setTransform(dpr, 0, 0, dpr, 0, 0);
}

/// 一片梅瓣:一头圆一头微尖的椭圆,转着落。
function drawPetal(context, petal) {
  context.save();
  context.translate(petal.x, petal.y);
  context.rotate(petal.angle);
  context.globalAlpha = petal.alpha;
  context.fillStyle = petal.color;
  // 一点点柔光,暗底上才读得出是花瓣而不是灰点。
  context.shadowColor = petal.color;
  context.shadowBlur = petal.size * 0.8;
  context.beginPath();
  context.moveTo(0, -petal.size);
  context.bezierCurveTo(petal.size * 0.9, -petal.size * 0.6, petal.size * 0.7, petal.size * 0.8, 0, petal.size);
  context.bezierCurveTo(-petal.size * 0.7, petal.size * 0.8, -petal.size * 0.9, -petal.size * 0.6, 0, -petal.size);
  context.fill();
  context.restore();
}

function tick(now) {
  if (!petalState.running) return;
  const { canvas, context, host } = petalState;
  const width = host.clientWidth;
  const height = host.clientHeight;
  const dt = Math.min(0.05, (now - (petalState.last || now)) / 1000);
  petalState.last = now;
  context.clearRect(0, 0, canvas.width, canvas.height);
  petalState.petals = petalState.petals.map((petal) => {
    petal.phase += dt * 0.9;
    petal.y += petal.fall * dt;
    petal.x += (petal.drift + Math.sin(petal.phase) * petal.sway) * dt;
    petal.angle += petal.spin * dt;
    if (petal.y > height + petal.size * 2 || petal.x < -40 || petal.x > width + 40) return spawn(width, height, false);
    drawPetal(context, petal);
    return petal;
  });
  petalState.frame = window.requestAnimationFrame(tick);
}

function start() {
  if (petalState.running || !petalState.canvas || document.hidden || reducedMotion()) return;
  petalState.running = true;
  petalState.last = 0;
  petalState.frame = window.requestAnimationFrame(tick);
}

function stop() {
  petalState.running = false;
  window.cancelAnimationFrame(petalState.frame);
  petalState.context?.clearRect(0, 0, petalState.canvas.width, petalState.canvas.height);
}

/// 开或关落梅。host 只在第一次开的时候用上。
export function setPetals(host, enabled) {
  petalState.enabled = Boolean(enabled);
  if (!enabled) {
    if (petalState.canvas) stop();
    return;
  }
  if (!petalState.canvas && host) {
    const canvas = document.createElement("canvas");
    canvas.className = "petal-layer";
    canvas.setAttribute("aria-hidden", "true");
    host.prepend(canvas);
    petalState.canvas = canvas;
    petalState.context = canvas.getContext("2d");
    petalState.host = host;
    resize();
    petalState.observer = new ResizeObserver(resize);
    petalState.observer.observe(host);
    document.addEventListener("visibilitychange", () => {
      if (document.hidden) stop();
      else if (petalState.enabled) start();
    });
    const width = host.clientWidth;
    const height = host.clientHeight;
    petalState.petals = Array.from({ length: PETAL_COUNT }, () => spawn(width, height, true));
  }
  start();
}
