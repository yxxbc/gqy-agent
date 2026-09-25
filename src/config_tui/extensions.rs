//! 插件菜单末尾的「扩展」：技能、脚本工具、MCP 服务器、pm 包。
//!
//! 与 WebUI 设置 → 插件 →「扩展」同一套后端（`skills::admin`、脚本面板的
//! 启用 / 禁用）。终端这边只做列表与开关：
//! - 技能、脚本：空格立即生效（改的是文件，不经过「保存配置」）；
//! - MCP：空格改的是这里的配置，和其他插件开关一样随「保存」写回；
//! - pm 包：只读，升级 / 卸载用 `gqy pm upgrade` / `gqy pm remove`。

use crate::config_tui::*;

enum Row {
    Header(&'static str),
    Skill {
        name: String,
        description: String,
        enabled: bool,
        fixed: bool,
    },
    Script {
        id: String,
        title: String,
        description: String,
        enabled: bool,
    },
    Mcp {
        index: usize,
        title: String,
        description: String,
    },
    Package {
        name: String,
        version: String,
        description: String,
    },
}

impl Row {
    fn selectable(&self) -> bool {
        !matches!(self, Row::Header(_))
    }
}

fn load_rows(config: &AppConfig, paths: &GqyPaths) -> Vec<Row> {
    let mut rows = vec![Row::Header(t("Skills", "技能"))];
    for skill in crate::skills::admin_catalog(config, paths).unwrap_or_default() {
        rows.push(Row::Skill {
            fixed: skill.toggle == "fixed",
            name: skill.name,
            description: skill.description,
            enabled: skill.enabled,
        });
    }
    rows.push(Row::Header(t("Script tools", "脚本工具")));
    if let Ok(overview) = crate::tools::scripts_dashboard_overview(config, paths) {
        for (list, enabled) in [("scripts", true), ("disabled", false)] {
            for script in overview[list].as_array().into_iter().flatten() {
                let id = script["id"].as_str().unwrap_or_default().to_string();
                rows.push(Row::Script {
                    title: script["display_name"].as_str().unwrap_or(&id).to_string(),
                    description: script["description"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    id,
                    enabled,
                });
            }
        }
    }
    rows.push(Row::Header(t("MCP servers", "MCP 服务器")));
    for (index, server) in config.mcp.servers.iter().enumerate() {
        let title = if server.display_name.trim().is_empty() {
            server.id.clone()
        } else {
            server.display_name.clone()
        };
        let description = std::iter::once(server.command.as_str())
            .chain(server.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        rows.push(Row::Mcp {
            index,
            title,
            description,
        });
    }
    rows.push(Row::Header(t("pm packages", "pm 包")));
    if let Ok(lock) = crate::pm::load_lock(paths) {
        for (name, package) in lock.packages {
            rows.push(Row::Package {
                name,
                version: package.version,
                description: package.description,
            });
        }
    }
    rows
}

pub(in crate::config_tui) fn edit_extensions(
    stdout: &mut io::Stdout,
    paths: &GqyPaths,
    config: &mut AppConfig,
) -> Result<()> {
    let mut rows = load_rows(config, paths);
    let mut selected = rows.iter().position(Row::selectable).unwrap_or(0);
    let mut message = String::new();
    loop {
        draw(stdout, config, &rows, selected, &message)?;
        match read_key()? {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(index) = (0..selected).rev().find(|&i| rows[i].selectable()) {
                    selected = index;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(index) = (selected + 1..rows.len()).find(|&i| rows[i].selectable()) {
                    selected = index;
                }
            }
            KeyCode::Char(' ') => {
                message = toggle(config, paths, &rows[selected]);
                rows = load_rows(config, paths);
                selected = selected.min(rows.len().saturating_sub(1));
            }
            _ => {}
        }
    }
}

/// 返回要显示在底部的一句结果。
fn toggle(config: &mut AppConfig, paths: &GqyPaths, row: &Row) -> String {
    let result = match row {
        Row::Header(_) => return String::new(),
        Row::Skill { fixed: true, .. } => {
            return t(
                "Platform built-in skills are always on.",
                "平台内置技能一直开着，不能关。",
            )
            .to_string()
        }
        Row::Skill { name, enabled, .. } => {
            crate::skills::set_skill_enabled(config, paths, name, !enabled)
        }
        Row::Script {
            id, enabled: true, ..
        } => crate::tools::scripts_dashboard_disable(config, paths, id).map(|_| ()),
        Row::Script {
            id, enabled: false, ..
        } => crate::tools::scripts_dashboard_enable(config, paths, id).map(|_| ()),
        Row::Mcp { index, .. } => {
            if let Some(server) = config.mcp.servers.get_mut(*index) {
                server.enabled = !server.enabled;
            }
            return t(
                "Changed; takes effect after saving.",
                "已修改，保存后生效。",
            )
            .to_string();
        }
        Row::Package { .. } => {
            return t(
                "Upgrade or remove with `gqy pm upgrade` / `gqy pm remove`.",
                "升级或卸载用命令行：gqy pm upgrade / gqy pm remove。",
            )
            .to_string()
        }
    };
    match result {
        Ok(()) => t(
            "Done; applies from the next turn.",
            "已生效，下一轮对话起用上。",
        )
        .to_string(),
        Err(error) => format!("{}: {error:#}", t("Failed", "失败")),
    }
}

fn draw(
    stdout: &mut io::Stdout,
    config: &AppConfig,
    rows: &[Row],
    selected: usize,
    message: &str,
) -> Result<()> {
    let (cols, term_rows) = terminal::size()?;
    let width = cols.saturating_sub(4).max(60);
    let height = term_rows.saturating_sub(2).max(10);
    let (x, y) = (2, 1);
    let inner = width.saturating_sub(4) as usize;
    queue!(stdout, Clear(ClearType::All))?;
    draw_box(stdout, x, y, width, height, t(" EXTENSIONS ", " 扩展 "))?;
    queue!(
        stdout,
        MoveTo(x + 2, y + 1),
        Print(pad(
            t(
                "[Space]enable/disable [j/k]move [q]back · skills and scripts apply at once, MCP after saving",
                "[Space]启用/禁用 [j/k]移动 [q]返回 · 技能和脚本立即生效，MCP 保存后生效",
            ),
            inner,
        ))
    )?;
    let visible = height.saturating_sub(6) as usize;
    let start = selected.saturating_sub(visible.saturating_sub(1));
    for (line, (index, row)) in rows
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .enumerate()
    {
        queue!(stdout, MoveTo(x + 2, y + line as u16 + 3))?;
        let on_off = |enabled: bool| {
            if enabled {
                t("[ON]", "[开]")
            } else {
                t("[OFF]", "[关]")
            }
        };
        let text = match row {
            Row::Header(title) => {
                queue!(
                    stdout,
                    SetAttribute(Attribute::Bold),
                    Print(pad(title, inner)),
                    SetAttribute(Attribute::Reset)
                )?;
                continue;
            }
            Row::Skill {
                name,
                description,
                enabled,
                fixed,
            } => plugin_row(
                if *fixed {
                    t("[FIX]", "[常开]")
                } else {
                    on_off(*enabled)
                },
                name,
                description,
                inner,
            ),
            Row::Script {
                title,
                description,
                enabled,
                ..
            } => plugin_row(on_off(*enabled), title, description, inner),
            Row::Mcp {
                index,
                title,
                description,
            } => {
                let enabled = config
                    .mcp
                    .servers
                    .get(*index)
                    .is_some_and(|server| server.enabled);
                plugin_row(on_off(enabled), title, description, inner)
            }
            Row::Package {
                name,
                version,
                description,
            } => plugin_row(&format!("v{version}"), name, description, inner),
        };
        if index == selected {
            queue!(
                stdout,
                SetAttribute(Attribute::Reverse),
                Print(pad(&text, inner)),
                SetAttribute(Attribute::Reset)
            )?;
        } else {
            queue!(stdout, Print(pad(&text, inner)))?;
        }
    }
    if !message.is_empty() {
        queue!(
            stdout,
            MoveTo(x + 2, y + height.saturating_sub(2)),
            Print(pad(&truncate(message, inner), inner))
        )?;
    }
    stdout.flush()?;
    Ok(())
}
