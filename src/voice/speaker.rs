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
use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::mpsc;
use std::time::Duration;

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
    pub fn start(on_state: Box<dyn Fn(bool) + Send>) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<SpeakerCommand>();
        std::thread::Builder::new()
            .name("gqy-voice-speaker".into())
            .spawn(move || run(rx, on_state))
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

fn run(rx: mpsc::Receiver<SpeakerCommand>, on_state: Box<dyn Fn(bool) + Send>) {
    let mut output: Option<(rodio::OutputStream, rodio::OutputStreamHandle)> = None;
    let mut queue: VecDeque<(Vec<u8>, bool)> = VecDeque::new();
    let mut speaking = false;
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
        let Ok(source) = rodio::Decoder::new(Cursor::new(wav)) else {
            tracing::warn!("播报音频解码失败");
            continue;
        };
        if !speaking {
            on_state(true);
            speaking = true;
        }
        sink.append(source);
        if wait_for_sink(&rx, &sink, &mut queue) {
            // 被打断:连同还没播的段一起作废。
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
