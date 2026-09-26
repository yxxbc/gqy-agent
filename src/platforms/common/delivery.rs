//! 回合结果的投递：`run_platform_turn` 的 `TurnDispatch` 变成发给平台的消息。
//!
//! 取消静默收场；失败只在私聊里说「出错了」（群里所有人都看得见内部错误）；
//! 完成时补上回合里生成的图片，并过两道幂等闸——本会话已经发过的图（内容
//! digest）和本回合工具已经发过的正文（bigram 近似度，AGENTS.md §4.3）。
//! QQ 与连接器平台共用这一份。

use crate::i18n::text as t;
use crate::platforms::*;

pub(crate) async fn deliver_dispatch(
    state: &DaemonState,
    context: &Arc<PlatformTurnContext>,
    dispatch: TurnDispatch,
) -> Result<bool> {
    match dispatch {
        TurnDispatch::Cancelled => {
            context.after_turn_aborted().await;
            tracing::debug!(
                target: "gqy::platform",
                conversation_kind = context.conversation.kind.as_str(),
                "{}",
                t("platform turn cancelled; nothing to deliver", "平台回合已取消,无需投递")
            );
            return Ok(false);
        }
        TurnDispatch::Failed(message) => {
            context.after_turn_aborted().await;
            if context.conversation.kind == ConversationKind::Group {
                tracing::info!(
                    target: "gqy::platform",
                    error = %message,
                    "{}",
                    t("suppressed an internal platform group error", "已抑制平台群聊内部错误")
                );
                return Ok(false);
            }
            context
                .send_bypass_plugins(OutboundMessage::text(
                    OutboundOrigin::Command,
                    format!("{}{message}", t("Something went wrong: ", "出错了：")),
                ))
                .await?;
        }
        TurnDispatch::Completed(mut outcome) => {
            if context.turn_is_superseded() {
                context.after_turn_aborted().await;
                return Ok(false);
            }
            let mut segments = Vec::new();
            let reply_text = final_reply_text(&outcome);
            let delivered_image_digests = context.delivered_image_digests();
            let mut image_digests = delivered_image_digests.clone();
            let mut matched_delivered_image = false;
            let mut unresolved_image_count = 0usize;
            let mut image_count = 0usize;
            for asset_id in &outcome.image_assets {
                match state.state_store.load_image_asset(asset_id) {
                    Ok(Some(asset)) => {
                        let digest = blake3::hash(&asset.bytes);
                        if !image_digests.insert(digest) {
                            let already_delivered = delivered_image_digests.contains(&digest);
                            if already_delivered {
                                matched_delivered_image = true;
                            }
                            tracing::debug!(
                                target: "gqy::platform",
                                asset_id,
                                "{}",
                                if already_delivered {
                                    t(
                                        "suppressed a platform reply image already delivered to this conversation",
                                        "已抑制本会话中先前已投递的平台回复图片",
                                    )
                                } else {
                                    t(
                                        "suppressed a duplicate platform reply image",
                                        "已抑制重复的平台回复图片",
                                    )
                                }
                            );
                            continue;
                        }
                        segments.push(OutboundSegment::ImageBytes {
                            mime: asset.asset.mime,
                            data: Arc::from(asset.bytes),
                            alt: asset.asset.alt,
                        });
                        image_count += 1;
                    }
                    Ok(None) => {
                        unresolved_image_count += 1;
                        tracing::warn!(
                            target: "gqy::platform",
                            asset_id,
                            "{}",
                            t(
                                "a platform reply image asset was not found",
                                "未找到平台回复图片资源",
                            )
                        );
                    }
                    Err(error) => {
                        unresolved_image_count += 1;
                        tracing::warn!(target: "gqy::platform", error = %error, asset_id, "{}", t("loading an image asset for a platform reply failed", "为平台回复加载图片资源失败"));
                    }
                }
            }
            if matched_delivered_image && image_count == 0 && unresolved_image_count == 0 {
                outcome.final_reply_already_sent = true;
            }
            let readable = crate::platforms::format_platform_final_reply_log(
                &outcome,
                context,
                &reply_text,
                image_count,
            );
            // 零宽空格之类的"看起来是空"也算空,别发空气泡。
            if crate::platforms::visibly_blank(&reply_text) {
            } else if context.repeats_delivered_reply_text(&reply_text) {
                // 工具(send_message_to_user)本回合已经把这句话发出去了,最终
                // 回复再发就是用户看到的"重复发送"。图片闸在上面同样处理。
                tracing::info!(
                    target: "gqy::platform",
                    "{}",
                    t(
                        "suppressed a platform final reply already delivered by a tool this turn",
                        "已抑制本回合工具已投递过的平台最终回复文本",
                    )
                );
                if segments.is_empty() {
                    outcome.final_reply_already_sent = true;
                }
            } else {
                segments.insert(0, OutboundSegment::Markdown(reply_text));
            }
            if segments.is_empty() {
                if outcome.final_reply_already_sent {
                    tracing::info!(target: "gqy::platform", "\n{readable}");
                    return Ok(true);
                }
                tracing::info!(
                    target: "gqy::platform",
                    "{}",
                    t("suppressed an empty platform model reply", "已抑制空的平台模型回复")
                );
                return Ok(false);
            }
            context
                .send(OutboundMessage::segments(
                    OutboundOrigin::FinalReply,
                    segments,
                ))
                .await?;
            tracing::info!(target: "gqy::platform", "\n{readable}");
        }
    }
    Ok(true)
}

pub(crate) fn final_reply_text(outcome: &crate::platforms::TurnOutcome) -> String {
    crate::platforms::cut_suppressed_ranges(&outcome.text, &outcome.suppressed_reply_ranges)
}
