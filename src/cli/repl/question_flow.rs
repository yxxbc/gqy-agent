//! 远端提问的跨端同步。
//!
//! 两个部件：
//! - [`QuestionWatch`]：面板开着时的后台哨兵，400ms 问一次 daemon
//!   `QuestionState`，发现「已在别处被回答/关闭」就记下并退出线程。
//! - [`finish_remote_question`]：面板退场后的统一收尾——把一问一答记进时间线、
//!   把结果回发给 daemon，并把「已被别处处理」的竞态降级成一行提示。
//!
//! 为什么要有它：提问的唯一持有者是 daemon 的 `QuestionBroker`，答完即删
//! （`AnswerFailure::NotFound`）。面板这边只在本地等键，别的端先答了它并不
//! 知道——用户在自己的输入框里点下去，吃的是一个 `pending question not found`
//! 回合错误（09-26 backlog §5）。哨兵把这段窗口关掉。

use crate::cli::repl::session::send_ipc_command;
use crate::cli::*;

/// 轮询间隔。面板本来就在 100ms 转一圈，几百毫秒的发现延迟对「另一端已经答了」
/// 这种场景绰绰有余；一次 unix socket 往返是微秒级。
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(400);

/// 面板打开期间的后台哨兵。`Drop` 即停（面板关掉/进程退出都不留轮询）。
pub(in crate::cli) struct QuestionWatch {
    resolution: std::sync::Arc<std::sync::Mutex<Option<crate::question_tui::ExternalResolution>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl QuestionWatch {
    pub(in crate::cli) fn start(paths: &GqyPaths, question_id: String) -> Self {
        let resolution = std::sync::Arc::new(std::sync::Mutex::new(None));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let shared = resolution.clone();
        let stop_flag = stop.clone();
        let paths = paths.clone();
        std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            loop {
                if stop_flag.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                // 连不上/daemon 正在重启都当作「还没有结果」继续等：面板那边
                // 的流断了自然会走它自己的错误路径，不必在这里抢先下结论。
                if let Ok(Some(found)) = runtime.block_on(poll_question_state(&paths, &question_id))
                {
                    *shared.lock().unwrap() = Some(found);
                    return;
                }
                std::thread::sleep(POLL_INTERVAL);
            }
        });
        Self { resolution, stop }
    }
}

impl crate::question_tui::QuestionWatchSignal for QuestionWatch {
    fn resolution(&self) -> Option<crate::question_tui::ExternalResolution> {
        self.resolution.lock().unwrap().clone()
    }
}

impl Drop for QuestionWatch {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// 问一次 daemon：`Ok(Some(..))` = 已有结果；`Ok(None)` = 还在等。
async fn poll_question_state(
    paths: &GqyPaths,
    question_id: &str,
) -> Result<Option<crate::question_tui::ExternalResolution>> {
    let mut stream = ipc::connect(&paths.ipc_socket()).await?;
    ipc::send(
        &mut stream,
        &IpcRequest::new(IpcCommand::QuestionState {
            question_id: question_id.to_string(),
        }),
    )
    .await?;
    match ipc::receive::<IpcFrame>(&mut stream).await? {
        Some(IpcFrame::AdminResult { data, .. }) => Ok(parse_question_state(&data)),
        Some(IpcFrame::Error { message, .. }) => bail!("{message}"),
        _ => Ok(None),
    }
}

/// 把 daemon 的答复翻成哨兵信号。`pending` = 还没有结果；答过/关过/取消过都
/// 算「已在别处处理」；`unknown` 也按已处理收场——问题已经不在待答表里，
/// 本地再交也只会吃一个「问题已不在」。
fn parse_question_state(
    data: &serde_json::Value,
) -> Option<crate::question_tui::ExternalResolution> {
    use crate::question_tui::ExternalResolution;
    match data.get("state").and_then(serde_json::Value::as_str) {
        Some("pending") | None => None,
        Some("answered") => {
            let answers = data
                .get("answers")
                .cloned()
                .filter(|value| !value.is_null())
                .and_then(|value| serde_json::from_value(value).ok());
            Some(match answers {
                Some(answers) => ExternalResolution::Answered(answers),
                None => ExternalResolution::ResolvedWithoutAnswer,
            })
        }
        _ => Some(ExternalResolution::ResolvedWithoutAnswer),
    }
}

/// 面板退场后的统一收尾：把这一问一答记进时间线、把结果回发给 daemon。
///
/// 「已在别处处理」不占本端的面板结果，也就不该再发 IPC（发了只会再吃一个
/// 「问题已不在」）；有答案就按「已回答」记档，没有就写一行提示。
pub(in crate::cli) async fn finish_remote_question(
    paths: &GqyPaths,
    renderer: &mut render::StreamRenderer,
    request: &crate::question::QuestionRequest,
    question_id: &str,
    run_id: &str,
    asked: &crate::question::QuestionResponse,
) -> Result<()> {
    use crate::question::QuestionResponse;
    // 时间线只认识「已回答/其余」两档：别处答掉的那份把答案摆出来，没有答案的
    // 就按关闭记（面板自己会留一行说明）。
    let display = match asked {
        QuestionResponse::ResolvedElsewhere(Some(answers)) => {
            QuestionResponse::Answered(answers.clone())
        }
        QuestionResponse::ResolvedElsewhere(None) => QuestionResponse::Closed,
        other => other.clone(),
    };
    // 全屏下面板是**盖在**画面上的，退场之后下一帧就按缓冲重画，问了什么、
    // 答了什么会一起消失。写进缓冲它才算进了历史、回翻找得到。
    renderer.timeline_push_question(request, &display)?;
    if !renderer.timeline_static() {
        renderer.prepare_for_external_output()?;
        renderer.write_question_exchange(request, &display)?;
        if matches!(asked, QuestionResponse::ResolvedElsewhere(None)) {
            renderer.write_system_message(&t(
                "the question was resolved in another client",
                "提问已在其它端处理",
            ))?;
        }
    }
    match asked {
        // 已在别处处理：不发 IPC，继续收流。
        QuestionResponse::ResolvedElsewhere(_) => {}
        QuestionResponse::Answered(answers) => {
            match send_ipc_command(
                paths,
                IpcCommand::AnswerQuestion {
                    question_id: question_id.to_string(),
                    answers: answers.clone(),
                },
            )
            .await
            {
                Ok(()) => {}
                // 竞态兜底：提交的瞬间问题在别处被处理了。不是回合失败。
                Err(error) if crate::question::is_question_resolved_elsewhere(&error) => {
                    renderer.write_system_message(&t(
                        "the question was resolved in another client",
                        "提问已在其它端处理",
                    ))?;
                }
                Err(error) => return Err(error),
            }
            renderer.start_waiting()?;
        }
        // Nobody could be shown the panel — no tty, or it failed to open. That
        // is not the user calling the turn off, so the question is resolved and
        // the turn carries on; the tool that asked finds out that nobody
        // answered and can say so.
        QuestionResponse::Unavailable(_) => {
            let _ = send_ipc_command(
                paths,
                IpcCommand::CloseQuestion {
                    question_id: question_id.to_string(),
                },
            )
            .await;
        }
        // The terminal question UI maps its close gestures to Cancelled; that
        // one really is "stop this turn".
        QuestionResponse::Closed | QuestionResponse::Cancelled => {
            let _ = send_ipc_command(
                paths,
                IpcCommand::Cancel {
                    run_id: run_id.to_string(),
                },
            )
            .await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// pending 之外的一切都算「已在别处处理」——问题已不在待答表里，本地再交
    /// 只会吃一个「问题已不在」。
    #[test]
    fn question_state_parsing_treats_everything_but_pending_as_resolved() {
        let answered = serde_json::json!({
            "state": "answered",
            "answers": [["A"]],
        });
        assert!(matches!(
            parse_question_state(&answered),
            Some(crate::question_tui::ExternalResolution::Answered(answers))
                if answers == vec![vec!["A".to_string()]]
        ));

        let closed = serde_json::json!({ "state": "closed", "answers": null });
        assert!(matches!(
            parse_question_state(&closed),
            Some(crate::question_tui::ExternalResolution::ResolvedWithoutAnswer)
        ));

        let unknown = serde_json::json!({ "state": "unknown" });
        assert!(matches!(
            parse_question_state(&unknown),
            Some(crate::question_tui::ExternalResolution::ResolvedWithoutAnswer)
        ));

        let pending = serde_json::json!({ "state": "pending", "answers": null });
        assert!(parse_question_state(&pending).is_none());
    }

    /// daemon 的两句固定报错都算「已被别处处理」，别的错误照旧是错误。
    #[test]
    fn only_the_daemon_question_gone_errors_are_swallowed() {
        let not_found = anyhow::anyhow!("pending question not found");
        assert!(crate::question::is_question_resolved_elsewhere(&not_found));
        let gone = anyhow::anyhow!("pending question is no longer active");
        assert!(crate::question::is_question_resolved_elsewhere(&gone));
        let other = anyhow::anyhow!("GQY core closed the connection");
        assert!(!crate::question::is_question_resolved_elsewhere(&other));
    }
}
