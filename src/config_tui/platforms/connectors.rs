//! 连接器平台（iMessage 等）的开关与口令。联系人是列表套列表，在终端里改容易
//! 手滑，这里只报个数，指去 WebUI 或配置文件改。
//!
//! 只有真改了哪一项才建 `platforms.connectors.<平台>` 这一节，免得进来看一眼就
//! 把整节默认值写进配置文件。

use crate::config::ConnectorPlatformConfig;
use crate::config_tui::*;

const IMESSAGE: &str = "imessage";

pub(in crate::config_tui) fn connector_label(config: &AppConfig, platform: &str) -> String {
    let enabled = config
        .platforms
        .connectors
        .get(platform)
        .is_some_and(|connector| connector.enabled);
    if enabled {
        t("enabled", "已启用").to_string()
    } else {
        t("disabled", "未启用").to_string()
    }
}

pub(in crate::config_tui) fn edit_imessage(
    stdout: &mut io::Stdout,
    config: &mut AppConfig,
) -> Result<()> {
    let mut selected = 0usize;
    loop {
        let current = config
            .platforms
            .connectors
            .get(IMESSAGE)
            .cloned()
            .unwrap_or_default();
        let yes_no = |value: bool| {
            if value {
                t("on", "开")
            } else {
                t("off", "关")
            }
        };
        let options = vec![
            format!("{}: {}", t("Enabled", "启用"), yes_no(current.enabled)),
            format!(
                "{}: {}",
                t("Connector token", "连接器口令"),
                if current.token.trim().is_empty() {
                    t("not set", "未设置")
                } else {
                    t("set", "已设置")
                }
            ),
            t("Generate a new token", "生成新口令").to_string(),
            format!("{}: {}", t("Contacts", "联系人"), current.contacts.len()),
            format!(
                "{}: {}",
                t("Host tools for yourself", "本人可用宿主工具"),
                yes_no(current.owner_host_tools)
            ),
        ];
        draw_menu(
            stdout,
            " iMessage ",
            &options,
            selected,
            t(
                "[Enter]change [j/k]move [q]back",
                "[Enter]修改 [j/k]移动 [q]返回",
            ),
        )?;
        match read_key()? {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
            KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                selected = (selected + 1).min(options.len() - 1)
            }
            KeyCode::Enter => match selected {
                0 => section(config).enabled = !current.enabled,
                1 => {
                    if let Some(value) = edit_inline_value(
                        stdout,
                        t(" CONNECTOR TOKEN ", " 连接器口令 "),
                        &current.token,
                        true,
                    )? {
                        section(config).token = value.trim().to_string();
                    }
                }
                2 => {
                    let token = crate::runtime::random_token(24);
                    section(config).token = token.clone();
                    message(
                        stdout,
                        &format!(
                            "{}\n\n{token}\n\n{}",
                            t("New token (shown once):", "新口令（只显示这一次）："),
                            t(
                                "Save the settings, then put the same token into ~/.gqy/config/imessage.json.",
                                "保存设置后，把同一个口令填进 ~/.gqy/config/imessage.json 的 token。",
                            )
                        ),
                    )?;
                }
                3 => message(
                    stdout,
                    t(
                        "Edit contacts in the WebUI (Console → Platforms → iMessage) or under platforms.connectors.imessage.contacts in config.jsonc.",
                        "联系人在 WebUI（控制台 → 通讯平台 → iMessage）或 config.jsonc 的 platforms.connectors.imessage.contacts 里改。",
                    ),
                )?,
                4 => section(config).owner_host_tools = !current.owner_host_tools,
                _ => {}
            },
            _ => {}
        }
    }
}

fn section(config: &mut AppConfig) -> &mut ConnectorPlatformConfig {
    config
        .platforms
        .connectors
        .entry(IMESSAGE.to_string())
        .or_default()
}
