//! 订阅 daemon 的宠物状态。
//!
//! daemon 那头已经把事件折成三种状态(idle / thinking / speaking,见
//! `web/ipc_server.rs` 的 `stream_pet_events`),这里只管收、把变化交给宿主、
//! 断线了隔几秒重连。
//!
//! **不强拉 daemon**:悬浮窗是旁路,daemon 没跑时它就静静等着——用户开 WebUI 或
//! 终端时 daemon 起来,宠物自己接上。
//!
//! 订阅跑在自己的线程上(current_thread 运行时),因为 GUI 事件循环不能 await。

use crate::ipc::{self, Command as IpcCommand, Frame as IpcFrame, PetState, Request as IpcRequest};
use crate::paths::GqyPaths;
use anyhow::{bail, Context, Result};
use std::time::Duration;

/// 断线后的重试间隔。daemon 因 build_id 变化重启是常态,3 秒不至于让状态
/// 空窗太久,也不至于每秒敲一次门。
const RETRY: Duration = Duration::from_secs(3);

/// daemon 推过来的东西。
#[derive(Debug, Clone, Copy)]
pub(in crate::pet) enum PetSignal {
    /// 她在想 / 在说话 / 闲着。
    State(PetState),
    /// 播报音量(0–1),驱动嘴型。只在播报的那几秒里有。
    Mouth(f32),
    /// 情绪(主人在 QQ 上那一份),驱动表情。慢变量,变了才来。
    Mood { valence: f64, arousal: f64 },
}

/// 起一个订阅线程。`on_signal` 在**那条线程**上被调用,宿主自己负责把信号挪到
/// 该去的地方(我们的宿主是 GUI 事件循环,所以它只往事件代理里塞一条)。
pub(in crate::pet) fn spawn(paths: &GqyPaths, on_signal: impl Fn(PetSignal) + Send + 'static) {
    let paths = paths.clone();
    let spawned = std::thread::Builder::new()
        .name("gqy-pet-state".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    tracing::warn!(%error, "pet: 拉不起订阅用的运行时");
                    return;
                }
            };
            runtime.block_on(async move {
                loop {
                    match subscribe(&paths, &on_signal).await {
                        Ok(()) => tracing::debug!("pet: daemon 那边断了,准备重连"),
                        Err(error) => tracing::debug!(%error, "pet: 订阅没接上,准备重试"),
                    }
                    tokio::time::sleep(RETRY).await;
                }
            });
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "pet: 订阅线程起不来");
    }
}

/// 连上、订阅、把信号推到断开为止(断开返回 `Ok`,连不上返回 `Err`)。
async fn subscribe(paths: &GqyPaths, on_signal: &impl Fn(PetSignal)) -> Result<()> {
    let mut stream = ipc::connect(&paths.ipc_socket())
        .await
        .context("daemon 没在跑")?;
    ipc::send(&mut stream, &IpcRequest::new(IpcCommand::SubscribePet)).await?;
    match ipc::receive::<IpcFrame>(&mut stream).await? {
        Some(IpcFrame::Ack) => tracing::info!("pet: 已接上 daemon,开始跟她的状态"),
        Some(IpcFrame::Error { message, .. }) => bail!("{message}"),
        other => bail!("订阅宠物状态时收到意外帧: {other:?}"),
    }
    while let Some(frame) = ipc::receive::<IpcFrame>(&mut stream).await? {
        let IpcFrame::Event { kind, data, .. } = frame else {
            continue;
        };
        if let Some(signal) = parse_signal(&kind, &data) {
            on_signal(signal);
        }
    }
    Ok(())
}

/// 帧 → 信号。认不出的 kind 直接丢:daemon 以后加别的东西不该让老宠物报错。
pub(in crate::pet) fn parse_signal(kind: &str, data: &serde_json::Value) -> Option<PetSignal> {
    match kind {
        "pet.state" => Some(PetSignal::State(
            serde_json::from_value::<PetState>(data.get("state")?.clone()).ok()?,
        )),
        "pet.mouth" => Some(PetSignal::Mouth(data.get("value")?.as_f64()? as f32)),
        "pet.mood" => Some(PetSignal::Mood {
            valence: data.get("valence")?.as_f64()?,
            arousal: data.get("arousal")?.as_f64()?,
        }),
        _ => None,
    }
}
