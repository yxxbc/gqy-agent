import { apiRequest } from "../../core/api.js";
import { showToast } from "../../core/toast.js";
import { updateControlState } from "./input.js";
import { elements } from "../../state/elements.js";
import { state } from "../../state/store.js";

// 语音输入(流式听写):浏览器麦克风 → 16kHz PCM16 → WebSocket
// /api/voice/stream → daemon → gqy-voice(VAD/分句/识别)→ 识别一句回一句,
// 逐句填进输入框。按一下开始,再按一下或 Esc 结束;静默 10 秒 daemon 自动收。
// 按钮只在 daemon 说语音功能已启用时显示;LAN 上的 http 页面拿不到麦克风
// (浏览器安全策略),这时提示改用本机 REPL 的 /stt。
/// 麦克风按钮显隐随 daemon 的语音开关;登录前这条 401,登录后要再拿一次。
export function refreshVoiceButton() {
  const button = elements.micButton;
  if (!button) return;
  apiRequest("/api/voice/status")
    .then((response) => response.json())
    .then((status) => {
      // 语音按钮只在 gqy voice 可用时才存在(用户);具体显 mic 还是 send 由
      // updateComposerControls 按有没有输入切换(空+语音可用=麦,有输入=发送)。
      state.voiceEnabled = Boolean(status?.enabled);
      updateControlState();
    })
    .catch(() => { state.voiceEnabled = false; updateControlState(); });
}

export function wireMicButton() {
  const button = elements.micButton;
  const indicator = elements.voiceIndicator;
  if (!button) return;
  let session = null;
  const MAX_MS = 5 * 60_000;

  refreshVoiceButton();

  function setIndicator(shown) {
    if (indicator) indicator.hidden = !shown;
    if (!shown) setLevel(0);
  }

  function setLevel(level) {
    if (elements.voiceLevel) elements.voiceLevel.style.width = `${Math.round(Math.max(0, Math.min(1, level)) * 100)}%`;
  }

  // 线性重采样到 16kHz,跨块保留相位和上一块末尾采样,块边界不跳变。
  function makeResampler(sourceRate) {
    const ratio = sourceRate / 16000;
    let previous = 0;
    let phase = 0;
    return (input) => {
      if (input.length === 0) return new Float32Array(0);
      const output = [];
      let position = phase;
      while (position <= input.length - 1) {
        const index = Math.floor(position);
        const from = index < 0 ? previous : input[index];
        const to = input[Math.min(index + 1, input.length - 1)];
        output.push(from + (to - from) * (position - index));
        position += ratio;
      }
      phase = position - input.length;
      previous = input[input.length - 1];
      return Float32Array.from(output);
    };
  }

  function insertText(text) {
    const input = elements.composerInput;
    const start = input.selectionStart ?? input.value.length;
    const end = input.selectionEnd ?? input.value.length;
    const before = input.value.slice(0, start);
    const after = input.value.slice(end);
    const glue = before && !/\s$/.test(before) ? " " : "";
    input.value = `${before}${glue}${text}${after}`;
    const cursor = before.length + glue.length + text.length;
    input.setSelectionRange(cursor, cursor);
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.focus();
  }

  function startCapture(current) {
    const { context } = current;
    const source = context.createMediaStreamSource(current.stream);
    const processor = context.createScriptProcessor(4096, 1, 1);
    const resample = makeResampler(context.sampleRate);
    processor.onaudioprocess = (event) => {
      current.lastFrameAt = Date.now();
      if (current.socket.readyState !== WebSocket.OPEN) return;
      const samples = resample(event.inputBuffer.getChannelData(0));
      const pcm = new Int16Array(samples.length);
      let energy = 0;
      for (let i = 0; i < samples.length; i += 1) {
        const clamped = Math.max(-1, Math.min(1, samples[i]));
        pcm[i] = clamped < 0 ? clamped * 0x8000 : clamped * 0x7fff;
        energy += clamped * clamped;
      }
      current.socket.send(pcm.buffer);
      setLevel(Math.sqrt(energy / Math.max(1, samples.length)) * 6);
    };
    source.connect(processor);
    processor.connect(context.destination);
    context.resume().catch(() => {});
    Object.assign(current, { source, processor, lastFrameAt: Date.now() });
    // 看门狗:音频帧断流 3 秒(标签页被挂起、AudioContext 没跑起来)就收,
    // 否则 daemon 那头收不到静音帧,10 秒静默自动结束永远不会触发。
    current.watchdog = setInterval(() => {
      if (Date.now() - current.lastFrameAt > 3000) stop(current, "麦克风没有音频,听写已停止");
    }, 1000);
  }

  function stop(current, notice) {
    if (session !== current) return;
    session = null;
    clearTimeout(current.timer);
    clearInterval(current.watchdog);
    try { current.processor?.disconnect(); current.source?.disconnect(); } catch { /* 已断 */ }
    current.stream.getTracks().forEach((track) => track.stop());
    current.context?.close().catch(() => {});
    current.socket.onclose = null;
    current.socket.onmessage = null;
    if (current.socket.readyState === WebSocket.OPEN) {
      try { current.socket.send("stop"); } catch { /* 对端已关 */ }
    }
    try { current.socket.close(); } catch { /* 已关 */ }
    button.classList.remove("is-recording");
    button.setAttribute("aria-pressed", "false");
    button.title = "语音输入";
    setIndicator(false);
    if (notice) showToast(notice, "info");
  }

  async function start() {
    if (session) return;
    if (!navigator.mediaDevices?.getUserMedia) {
      showToast("这个页面拿不到麦克风(需要 https 或 localhost);可在终端 REPL 里用 /stt 听写", "error");
      return;
    }
    const context = new (window.AudioContext || window.webkitAudioContext)();
    let stream;
    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true } });
    } catch (error) {
      context.close().catch(() => {});
      showToast(`麦克风不可用:${error?.message || error}`, "error");
      return;
    }
    const url = `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/api/voice/stream`;
    const socket = new WebSocket(url);
    socket.binaryType = "arraybuffer";
    const current = { socket, stream, context, source: null, processor: null, timer: null, watchdog: null, ready: false, lastFrameAt: 0 };
    session = current;
    button.classList.add("is-recording");
    button.setAttribute("aria-pressed", "true");
    button.title = "点击结束听写(Esc 也可以)";
    socket.onmessage = (event) => {
      let message;
      try { message = JSON.parse(event.data); } catch { return; }
      if (message.type === "ready") {
        current.ready = true;
        startCapture(current);
        setIndicator(true);
      } else if (message.type === "dictation") {
        if (message.text) insertText(String(message.text));
      } else if (message.type === "ended") {
        stop(current, "听写结束");
      } else if (message.type === "error") {
        showToast(`语音听写不可用:${message.message || "未知错误"}`, "error");
        stop(current, null);
      }
    };
    socket.onclose = () => {
      if (session !== current) return;
      if (!current.ready) showToast("语音听写连接失败", "error");
      stop(current, current.ready ? "听写结束" : null);
    };
    current.timer = setTimeout(() => stop(current, "听写超时结束"), MAX_MS);
  }

  button.addEventListener("click", () => {
    if (session) stop(session, "听写已停止");
    else start();
  });
  elements.composerInput.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && session) {
      event.preventDefault();
      stop(session, "听写已停止");
    }
  });
}
