//! 播报:在自己的线程里播 daemon 合成好的 wav。
//!
//! 播放期间通过 `on_state(true/false)` 通知宿主(worker 据此让管线丢掉麦克风
//! 帧,并告诉 daemon 正在播报);`stop()` 立即打断。
//!
//! **播报是排队的**(09-16):daemon 把一段回复拆成先后两段合成,第一句先到先
//! 播,剩下的边播边合成。此前播放中送来的段会被直接丢弃,后半句就没了。
//!
//! 段与段之间的 `on_state` 必须保持 `true` 不许抖:哪怕只掉下去一瞬,管线就
//! 会恢复喂麦克风帧,把喇叭里正在播的下一段当成用户开口,于是她自己把自己
//! 打断。`more` 标记说明「后面还有」,播完这段会留一个宽限期等下一段。

use anyhow::{Context, Result};
use rodio::Source;
use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

/// 包络帧长(毫秒)。100ms(10 帧/秒)够嘴型跟得上音量,又不至于把 IPC 灌满——
/// 原始音频一帧都不推(见 `voice_envelope`)。
const ENVELOPE_FRAME_MS: u64 = 100;

pub enum SpeakerCommand {
    /// 播一段完整 wav。`more` = daemon 说后面还有一段,别急着收状态。
    PlayWav { wav: Vec<u8>, more: bool },
    /// 打断当前播放并清空队列。
    Stop,
}

pub struct Speaker {
    tx: mpsc::Sender<SpeakerCommand>,
}

impl Speaker {
    /// `on_state` 报播报起止;`on_envelope` 报音量包络(0–1,每 100ms 一帧),
    /// 给桌面悬浮窗驱动嘴型用。两个回调都在播报线程上被调用。
    pub fn start(
        on_state: Box<dyn Fn(bool) + Send>,
        on_envelope: Arc<dyn Fn(f32) + Send + Sync>,
    ) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<SpeakerCommand>();
        std::thread::Builder::new()
            .name("gqy-voice-speaker".into())
            .spawn(move || run(rx, on_state, on_envelope))
            .context("启动播报线程失败")?;
        Ok(Self { tx })
    }

    pub fn play_wav(&self, wav: Vec<u8>, more: bool) {
        let _ = self.tx.send(SpeakerCommand::PlayWav { wav, more });
    }

    pub fn stop(&self) {
        let _ = self.tx.send(SpeakerCommand::Stop);
    }
}

/// 输出流按需打开,连续 30s 没播放才关(每次重新打开设备要几十到上百毫秒,
/// 会让播报比通知晚一拍;关掉是为了不说话时不挂着音频设备)。
const OUTPUT_IDLE: Duration = Duration::from_secs(30);

/// `more` 段播完后等下一段的宽限期。等不到就照常收状态——合成失败或 daemon
/// 掉线时不能把 speaking 永久钉在 true,那会让麦克风一直被丢帧。
const NEXT_SEGMENT_GRACE: Duration = Duration::from_secs(3);

fn run(
    rx: mpsc::Receiver<SpeakerCommand>,
    on_state: Box<dyn Fn(bool) + Send>,
    on_envelope: Arc<dyn Fn(f32) + Send + Sync>,
) {
    let mut output: Option<(rodio::OutputStream, rodio::OutputStreamHandle)> = None;
    let mut queue: VecDeque<(Vec<u8>, bool)> = VecDeque::new();
    let mut speaking = false;
    // 包络线程的代际:新的一段 / 被打断都 +1,老线程看到自己的号过期就闭嘴退出。
    let generation = Arc::new(AtomicU64::new(0));
    loop {
        let (wav, more) = match queue.pop_front() {
            Some(segment) => segment,
            None => {
                // 队列空了:上一段的状态到这里才收。
                if speaking {
                    on_state(false);
                    speaking = false;
                }
                let command = if output.is_some() {
                    match rx.recv_timeout(OUTPUT_IDLE) {
                        Ok(command) => command,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            output = None;
                            continue;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                } else {
                    match rx.recv() {
                        Ok(command) => command,
                        Err(_) => return,
                    }
                };
                match command {
                    SpeakerCommand::PlayWav { wav, more } => (wav, more),
                    SpeakerCommand::Stop => continue,
                }
            }
        };
        if output.is_none() {
            match rodio::OutputStream::try_default() {
                Ok(pair) => output = Some(pair),
                Err(error) => {
                    tracing::warn!("打开音频输出失败,播报跳过: {error}");
                    continue;
                }
            }
        }
        let Some((_, handle)) = output.as_ref() else {
            continue;
        };
        let Ok(sink) = rodio::Sink::try_new(handle) else {
            continue;
        };
        // 这一段自己的包络:换代 → 上一段的推送线程作废,免得两段一起动嘴。
        // 包络要在 wav 被 move 进解码器之前算。
        let mine = generation.fetch_add(1, Ordering::Relaxed) + 1;
        let envelope = voice_envelope(&wav);
        let Ok(source) = rodio::Decoder::new(Cursor::new(wav)) else {
            tracing::warn!("播报音频解码失败");
            continue;
        };
        if !speaking {
            on_state(true);
            speaking = true;
        }
        sink.append(source);
        if !envelope.is_empty() {
            let emit = Arc::clone(&on_envelope);
            let generation = Arc::clone(&generation);
            let spawned = std::thread::Builder::new()
                .name("gqy-voice-envelope".into())
                .spawn(move || {
                    for value in envelope {
                        std::thread::sleep(Duration::from_millis(ENVELOPE_FRAME_MS));
                        if generation.load(Ordering::Relaxed) != mine {
                            return;
                        }
                        emit(value);
                    }
                    // 收尾:这一段播完把嘴闭上。
                    if generation.load(Ordering::Relaxed) == mine {
                        emit(0.0);
                    }
                });
            if let Err(error) = spawned {
                tracing::warn!(%error, "包络推送线程起不来,嘴型不会动");
            }
        }
        if wait_for_sink(&rx, &sink, &mut queue) {
            // 被打断:连同还没播的段一起作废,脸也收回去。
            generation.fetch_add(1, Ordering::Relaxed);
            queue.clear();
            if speaking {
                on_state(false);
                speaking = false;
            }
            continue;
        }
        if more && queue.is_empty() {
            // 后面还有,但还没送到:等一会儿,别把状态收了。
            match rx.recv_timeout(NEXT_SEGMENT_GRACE) {
                Ok(SpeakerCommand::PlayWav { wav, more }) => queue.push_back((wav, more)),
                Ok(SpeakerCommand::Stop) => {
                    queue.clear();
                    if speaking {
                        on_state(false);
                        speaking = false;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    tracing::warn!("等后续播报段超时,按播完收尾");
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

/// 等 sink 播完;期间把送来的段收进 `queue`,收到 Stop 立即停并返回 true。
fn wait_for_sink(
    rx: &mpsc::Receiver<SpeakerCommand>,
    sink: &rodio::Sink,
    queue: &mut VecDeque<(Vec<u8>, bool)>,
) -> bool {
    while !sink.empty() {
        let mut stop = false;
        while let Ok(command) = rx.try_recv() {
            match command {
                // 以前这里只认 Stop,PlayWav 取出来就丢——一段回复拆成两段送
                // 时后半句会凭空消失。
                SpeakerCommand::PlayWav { wav, more } => queue.push_back((wav, more)),
                SpeakerCommand::Stop => stop = true,
            }
        }
        if stop {
            sink.stop();
            return true;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    sink.sleep_until_end();
    false
}

/// 把一段播报音频压成音量包络:每 100ms 一个 RMS,按峰值归一到 0–1。
///
/// 桌面悬浮窗拿它驱动嘴型(见 `docs/design/2026-09-28-desktop-pet.md` §6)。
/// **不推原始音频**:数据量小、不重复解码,也不会让宠物变成第二个播放器。
/// 不同 TTS 引擎的响度差很多,所以按峰值归一——不归一的话有的引擎嘴上几乎没动静。
fn voice_envelope(wav: &[u8]) -> Vec<f32> {
    let Ok(decoder) = rodio::Decoder::new(Cursor::new(wav.to_vec())) else {
        return Vec::new();
    };
    let rate = decoder.sample_rate() as usize;
    let channels = usize::from(decoder.channels()).max(1);
    let per_frame = (rate * ENVELOPE_FRAME_MS as usize / 1000).max(1);
    let frame_samples = per_frame * channels;

    let mut frames = Vec::new();
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for sample in decoder {
        sum += f64::from(sample) * f64::from(sample);
        count += 1;
        if count >= frame_samples {
            frames.push((sum / count as f64).sqrt() as f32);
            sum = 0.0;
            count = 0;
        }
    }
    if count > 0 {
        frames.push((sum / count as f64).sqrt() as f32);
    }

    let peak = frames.iter().copied().fold(0.0f32, f32::max).max(0.01);
    for value in &mut frames {
        *value = (*value / peak).clamp(0.0, 1.0);
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::voice_envelope;

    /// 造一段 wav:前半静音、后半正弦。包络该是「先 0、后有值」,而且峰值归一。
    #[test]
    fn the_envelope_follows_loudness() {
        let rate = 44100u32;
        let half = rate / 2;
        let mut samples = Vec::with_capacity((half * 2) as usize);
        for index in 0..half * 2 {
            let value = if index < half {
                0.0
            } else {
                (index as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.8
            };
            samples.push((value * f32::from(i16::MAX)) as i16);
        }

        let frames = voice_envelope(&pcm_wav(&samples, rate));
        assert!(frames.len() >= 8, "包络帧太少: {}", frames.len());
        assert!(frames[0] < 0.05, "开头是静音,不该有值: {}", frames[0]);
        assert!(
            frames.iter().copied().fold(0.0f32, f32::max) > 0.9,
            "响亮的那半段应当归一"
        );
    }

    /// 坏数据不给包络,也不许 panic——播报路径上任何 panic 都是事故。
    #[test]
    fn garbage_input_yields_no_envelope() {
        assert!(voice_envelope(b"").is_empty());
        assert!(voice_envelope(b"not a wav").is_empty());
    }

    /// 44 字节头 + i16 PCM,单声道。
    fn pcm_wav(samples: &[i16], rate: u32) -> Vec<u8> {
        let data_len = (samples.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for sample in samples {
            out.extend_from_slice(&sample.to_le_bytes());
        }
        out
    }
}
