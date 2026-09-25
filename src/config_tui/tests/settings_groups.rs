//! 全局参数设置的分组：和 WebUI 字段表对得上、原样写回不改配置。

use crate::config::AppConfig;
use crate::config_tui::SETTINGS_GROUPS;
use crate::web::tests::settings_schema::{fields_of, read_schema};

/// WebUI 里这几个分区在终端配置器里都归「全局参数设置」。
const COVERED_SECTIONS: &[&str] = &[
    "tools",
    "skills",
    "mcp",
    "display",
    "context",
    "cache",
    "notifications",
    "accounts",
];

fn tui_paths() -> Vec<&'static str> {
    let config = AppConfig::default();
    SETTINGS_GROUPS
        .iter()
        .flat_map(|group| (group.fields)(&config).paths)
        .collect()
}

/// 防再漏：WebUI 这几个分区的每个字段，终端配置器里都要有；反过来终端里
/// 声明的路径也都得是 WebUI 认识的（拼错路径会静默写进一个不存在的键）。
#[test]
fn settings_groups_cover_the_webui_sections() {
    let schema = read_schema();
    let mut webui = Vec::new();
    for section in schema["general"].as_array().unwrap() {
        let id = section["id"].as_str().unwrap_or_default();
        if !COVERED_SECTIONS.contains(&id) {
            continue;
        }
        for field in fields_of(section) {
            let path = field["path"]
                .as_str()
                .or_else(|| field["key"].as_str())
                .unwrap();
            webui.push(path.to_string());
        }
    }
    let tui = tui_paths();
    let missing: Vec<&String> = webui
        .iter()
        .filter(|path| !tui.contains(&path.as_str()))
        .collect();
    assert!(missing.is_empty(), "终端配置器缺这些设置: {missing:?}");
    let unknown: Vec<&&str> = tui
        .iter()
        .filter(|path| !webui.iter().any(|known| known == *path))
        .collect();
    assert!(unknown.is_empty(), "WebUI 没有这些路径: {unknown:?}");
}

/// 什么都不改直接退出，配置一个字节都不能变（小数、留空的可选值、列表都
/// 要原样读回）。
#[test]
fn settings_groups_round_trip_unchanged_values() {
    let mut config = AppConfig::default();
    config.accounts.member_plugins = Some(vec!["files".into(), "album".into()]);
    config.context.compact_tail_tokens = Some(12_000);
    let before = serde_json::to_value(&config).unwrap();
    for group in SETTINGS_GROUPS {
        let form = (group.fields)(&config);
        form.apply(&mut config)
            .unwrap_or_else(|error| panic!("{}: {error}", group.id));
        assert_eq!(
            serde_json::to_value(&config).unwrap(),
            before,
            "{} changed the config",
            group.id
        );
    }
}

/// 六个分组铺在主菜单顶层，名字就是 `/config <分组>` 能输的名字（09-24 验收
/// 问题 4：以前藏在「全局参数设置」下面一层，用户不知道有这些）。
#[test]
fn settings_groups_sit_on_the_main_menu() {
    let config = AppConfig::default();
    let (options, actions) = crate::config_tui::main_menu(&config);
    for group in SETTINGS_GROUPS {
        let label = group.title();
        assert!(
            options.iter().any(|option| option.starts_with(label)),
            "主菜单里没有 {label}"
        );
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, crate::config_tui::MainMenuAction::SettingsGroup(group_) if group_.title() == label)),
            "{label} 没有对应的菜单动作"
        );
        // 界面上显示什么就能 `/config` 什么，中英文和 id 都认。
        assert!(crate::config_tui::is_settings_group(label));
        assert!(crate::config_tui::is_settings_group(group.id));
    }
    assert!(!crate::config_tui::is_settings_group("没有这个分组"));
}

#[test]
fn list_settings_keep_items_with_commas_and_semicolons() {
    let mut config = AppConfig::default();
    let group = SETTINGS_GROUPS
        .iter()
        .find(|group| group.id == "tools")
        .unwrap();
    let mut form = (group.fields)(&config);
    let index = form
        .paths
        .iter()
        .position(|path| *path == "tools.command_deny")
        .unwrap();
    form.fields[index].value = ":(){ :|:& };:\nrm -rf /, now\n\n".to_string();
    form.apply(&mut config).unwrap();
    assert_eq!(
        config.tools.command_deny,
        [":(){ :|:& };:", "rm -rf /, now"]
    );
}
