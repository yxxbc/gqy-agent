//! 回合还在跑时来了新消息：找到那个回合，决定并进去的方式。
//!
//! 平台中立：QQ 与连接器平台共用。

use crate::platforms::*;
use crate::runtime::TurnUpdateMode;

/// 回合还在跑时,新消息该排队还是该取代当前生成。
///
/// 群聊走的是另一条路(`reserve_tool_followup` 只在工具执行期返回 Some,
/// 其余落到下面的覆盖分支),所以这里恒为排队。
///
/// 私聊的判据与群聊同源:**工具正在跑**说明她在真干活,排队别打断;否则她
/// 只是在写回复,新消息该取代它。
///
/// 08-29 取证:QQ 里一句话拆成几条发是常态。用户先发"这是什么鱼"、三秒后
/// 补图,回合已经带着"没有图"开跑并写出"你没发图我怎么知道",这句被中间
/// 消息通道投递了出去,随后消费队列才答对——用户看到的是先装瞎再答题。
/// 同样两条消息在群里会被覆盖窗口合并。
pub(crate) fn active_turn_update_mode(is_group: bool, tool_executing: bool) -> TurnUpdateMode {
    if is_group || tool_executing {
        TurnUpdateMode::Followup
    } else {
        TurnUpdateMode::Supersede
    }
}

/// 这条会话里、同一发送者正在跑的平台回合（最近开始的那个）。新消息据此决定
/// 是并进这个回合（排队 / 取代）还是另起一个。
pub(crate) fn platform_update_target(
    state: &DaemonState,
    session_id: &str,
    conversation: &PlatformConversation,
    sender_id: &str,
) -> Option<(String, String, Arc<PlatformFollowupRun>)> {
    let manager = state.manager.lock().unwrap();
    manager
        .active_runs
        .iter()
        .filter(|(_, run)| &*run.session_id == session_id)
        .filter_map(|(run_id, run)| {
            let followup = run.platform_followup.as_ref()?;
            if followup.conversation != *conversation || followup.sender_id != sender_id {
                return None;
            }
            Some((
                followup.started(),
                run_id.clone(),
                run.turn_id.clone()?,
                followup.clone(),
            ))
        })
        .max_by_key(|(started, _, _, _)| *started)
        .map(|(_, run_id, turn_id, followup)| (run_id, turn_id, followup))
}

pub(crate) fn reserve_tool_followup(
    state: &DaemonState,
    session_id: &str,
    conversation: &PlatformConversation,
    sender_id: &str,
) -> Option<(
    String,
    String,
    Arc<PlatformFollowupRun>,
    crate::agent::QueueIngressReservation,
)> {
    let (run_id, turn_id, followup) =
        platform_update_target(state, session_id, conversation, sender_id)?;
    let reservation = followup.try_reserve()?;
    Some((run_id, turn_id, followup, reservation))
}
