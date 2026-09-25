//! 全局参数设置：按分组（显示 / 工具 / 上下文 / 缓存 / 通知 / 成员账号）各一张
//! 表单，分组与 WebUI 设置页一一对应，`/config <分组>` 可以直达。
//!
//! 每组一个文件，字段用 `setting!` 按配置路径声明（见 `spec.rs`），防漏测试
//! 拿这些路径对照 WebUI 的字段表。

use crate::config_tui::*;

pub(in crate::config_tui) mod spec;

mod accounts;
mod cache;
mod context;
mod display;
mod notifications;
mod tools;

/// true = save and exit, false = discard and exit. A choice is mandatory:
/// `q`/`Esc` are ignored so an accidental key press cannot lose edits.
pub(in crate::config_tui) fn confirm_save_on_exit(stdout: &mut io::Stdout) -> Result<bool> {
    let options = [
        t("Save", "保存").to_string(),
        t("Discard", "不保存").to_string(),
    ];
    let mut selected = 0usize;
    loop {
        draw_menu(
            stdout,
            t(" SAVE EDITED CHANGES? ", " 是否保存已编辑内容 "),
            &options,
            selected,
            t("[j/k]move [Enter]confirm", "[j/k]移动 [Enter]确认"),
        )?;
        match read_key()? {
            KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => selected = (selected + 1).min(1),
            KeyCode::Enter => return Ok(selected == 0),
            _ => {}
        }
    }
}

/// 一组设置：`/config <id>` 直达的就是它。id 与 WebUI 设置页
/// （`web/settings-schema/`）的分区 id 一致。
pub(crate) struct SettingsGroup {
    pub(crate) id: &'static str,
    en: &'static str,
    zh: &'static str,
    pub(in crate::config_tui) fields: fn(&AppConfig) -> BoundFields,
}

impl SettingsGroup {
    pub(crate) fn title(&self) -> &'static str {
        t(self.en, self.zh)
    }
}

pub(crate) const SETTINGS_GROUPS: &[SettingsGroup] = &[
    SettingsGroup {
        id: "display",
        en: "Display",
        zh: "显示",
        fields: display::fields,
    },
    SettingsGroup {
        id: "tools",
        en: "Tools",
        zh: "工具",
        fields: tools::fields,
    },
    SettingsGroup {
        id: "context",
        en: "Context",
        zh: "上下文",
        fields: context::fields,
    },
    SettingsGroup {
        id: "cache",
        en: "Cache",
        zh: "缓存",
        fields: cache::fields,
    },
    SettingsGroup {
        id: "notifications",
        en: "Notifications",
        zh: "通知",
        fields: notifications::fields,
    },
    SettingsGroup {
        id: "accounts",
        en: "Member accounts",
        zh: "成员账号",
        fields: accounts::fields,
    },
];

pub(crate) fn settings_group(id: &str) -> Option<&'static SettingsGroup> {
    let arg = id.trim();
    SETTINGS_GROUPS.iter().find(|group| {
        group.id.eq_ignore_ascii_case(arg) || group.zh == arg || group.en.eq_ignore_ascii_case(arg)
    })
}

pub(in crate::config_tui) fn edit_settings_group(
    stdout: &mut io::Stdout,
    config: &mut AppConfig,
    group: &SettingsGroup,
) -> Result<()> {
    let mut form = (group.fields)(config);
    let title = format!(" {} ", group.title());
    run_form_without_buttons(stdout, &title, &mut form.fields)?;
    form.apply(config)
}

pub(in crate::config_tui) fn language_choice_label(value: &str, zh: bool) -> Option<&'static str> {
    match (value.trim(), zh) {
        ("auto", false) => Some("Auto"),
        ("auto", true) => Some("自动"),
        ("en", false) => Some("English"),
        ("en", true) => Some("英语"),
        ("zh", false) => Some("Simplified Chinese"),
        ("zh", true) => Some("简体中文"),
        _ => None,
    }
}

pub(in crate::config_tui) fn language_choice_value(value: &str) -> Option<&'static str> {
    match value.trim() {
        "auto" | "Auto" | "自动" => Some("auto"),
        "en" | "English" | "英语" => Some("en"),
        "zh" | "Simplified Chinese" | "简体中文" => Some("zh"),
        _ => None,
    }
}

pub(in crate::config_tui) fn parse_mixed_endpoint_display(value: &str) -> String {
    match value.trim() {
        "关" | "Off" | "off" => "off".to_string(),
        "全部模式" | "All modes" | "all" => "all".to_string(),
        _ => "interactive".to_string(),
    }
}

pub(in crate::config_tui) fn normalize_tools_loading_mode(value: &str) -> String {
    // hybrid 档 09-01 删除;它和 lazy 同属懒加载家族,历史值一律归入需加载,
    // 悄悄升成 full 会让旧配置的工具面字节数翻好几倍。
    match value.trim() {
        "full" => "full".to_string(),
        _ => "stub".to_string(),
    }
}

pub(in crate::config_tui) fn parse_bool_field(value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "y" | "1" | "on" | "启用" | "是" => Ok(true),
        "false" | "no" | "n" | "0" | "off" | "禁用" | "否" => Ok(false),
        value => {
            if is_zh() {
                bail!("无效的布尔值: {value}")
            } else {
                bail!("Invalid boolean value: {value}")
            }
        }
    }
}
