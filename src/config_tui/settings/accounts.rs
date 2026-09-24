//! 成员账号：成员能否自建人格、成员人格能启用哪些插件。

use super::spec::setting;
use crate::config_tui::*;

pub(super) fn fields(config: &AppConfig) -> BoundFields {
    let form = BoundFields::default();
    let form = setting!(
        form,
        config,
        "Members can create personas",
        "成员可以创建自己的人格",
        accounts.member_personas
    );
    setting!(
        form,
        config,
        "Plugins member personas may enable (empty = all)",
        "成员人格可启用的插件(留空=全部)",
        accounts.member_plugins
    )
}
