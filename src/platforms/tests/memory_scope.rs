//! 平台回合的记忆权限：最高权限只在管理员私聊里给；主人绑定只认配置里的号。

use super::shared::test_turn_context;
use crate::platforms::ConversationKind;

#[test]
fn admins_read_all_memory_only_in_private_chats() {
    let (_temp, mut context, _adapter) = test_turn_context(false);
    context.is_admin = true;
    assert!(context.privileged_memory());

    // 群里的回复所有人都看得见：管理员在群里也不能召回只该主人看的记忆。
    context.conversation.kind = ConversationKind::Group;
    assert!(!context.privileged_memory());

    context.conversation.kind = ConversationKind::Private;
    context.is_admin = false;
    assert!(!context.privileged_memory());
}

#[test]
fn owner_binding_needs_the_configured_number_in_a_private_chat() {
    let (_temp, mut context, _adapter) = test_turn_context(false);
    // test_turn_context 的发起者是 20000。
    context.is_admin = true;
    assert!(!context.owner_bound(), "admin alone is not the owner");

    context.config.platforms.qq.admin_users = vec![20000];
    context.config.platforms.qq.owner_users = vec![20000];
    assert!(context.owner_bound());

    context.conversation.kind = ConversationKind::Group;
    assert!(!context.owner_bound(), "never in group chats");

    context.conversation.kind = ConversationKind::Private;
    context.is_admin = false;
    assert!(
        !context.owner_bound(),
        "owner binding requires admin status"
    );
}
