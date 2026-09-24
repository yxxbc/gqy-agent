//! REPL 的斜杠命令表。
//!
//! `REPL_COMMAND_TABLE` 是单一事实来源：补全、帮助、解析全从它派生，加命令只
//! 用改这一处。

use crate::cli::*;

/// 帮助全文。**要能拿到字符串**：全屏下它得走缓冲进正文（直接 `println!`
/// 的话字节不在缓冲里，下一帧重画就被抹掉，回翻也找不到）。
pub(in crate::cli) fn repl_help_text() -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "{}", t("commands:", "命令:"));
    let width = REPL_COMMAND_TABLE
        .iter()
        .map(|spec| {
            spec.name.len()
                + if spec.arg_hint.is_empty() {
                    0
                } else {
                    spec.arg_hint.len() + 1
                }
        })
        .max()
        .unwrap_or(0);
    for spec in REPL_COMMAND_TABLE {
        let invocation = if spec.arg_hint.is_empty() {
            spec.name.to_string()
        } else {
            format!("{} {}", spec.name, spec.arg_hint)
        };
        let _ = writeln!(
            out,
            "  {invocation:<width$}  {}",
            t(spec.help_en, spec.help_zh)
        );
    }
    let _ = writeln!(out, "{}", t("keys:", "快捷键:"));
    let _ = writeln!(
        out,
        "  Tab         {}",
        t(
            "switch normal/dev while the session is empty, or complete slash commands",
            "空会话时切换 普通/开发，或补全斜杠命令"
        )
    );
    let _ = writeln!(out, "  Enter       {}", t("send message", "发送消息"));
    let _ = writeln!(out, "  Shift+Enter {}", t("insert newline", "插入换行"));
    let _ = writeln!(
        out,
        "  Ctrl+J      {}",
        t(
            "insert newline, same as Shift+Enter",
            "插入换行，与 Shift+Enter 相同"
        )
    );
    let _ = writeln!(
        out,
        "  Ctrl+V      {}",
        t(
            "paste image or text from clipboard",
            "从剪贴板粘贴图片或文本"
        )
    );
    let _ = writeln!(out, "  Ctrl+L      {}", t("clear screen", "清屏"));
    let _ = writeln!(
        out,
        "  Up/Down     {}",
        t(
            "browse input history, or move through slash-command candidates",
            "切换输入历史，或在斜杠命令候选里移动"
        )
    );
    let _ = writeln!(
        out,
        "  Esc Esc     {}",
        t("interrupt running reply", "中断当前回复")
    );
    let _ = writeln!(
        out,
        "  Ctrl+C      {}",
        t(
            "clear the draft, else interrupt the reply, else stop background tasks, else exit",
            "先清空输入；输入为空则中断回复；再无回复则停止后台任务；都没有则退出"
        )
    );
    let _ = writeln!(out, "  Ctrl+D      {}", t("exit", "退出"));
    out
}

/// `/config [分组]`：不带参数打开完整设置菜单，带分组直达那一组。分组不认识
/// 就不进设置界面，返回给用户看的提示。
pub(in crate::cli) fn open_config_ui(paths: &GqyPaths, args: &str) -> Result<Option<String>> {
    let args = args.trim();
    if args.is_empty() {
        crate::config_tui::run(paths)?;
        return Ok(None);
    }
    let choices = crate::config_tui::settings_group_choices();
    if !choices.iter().any(|(id, _)| id.eq_ignore_ascii_case(args)) {
        let ids: Vec<&str> = choices.iter().map(|(id, _)| *id).collect();
        return Ok(Some(format!(
            "{}: {args} ({})",
            t("unknown settings group", "没有这个设置分组"),
            ids.join(" | ")
        )));
    }
    crate::config_tui::run_settings_group(paths, args)?;
    Ok(None)
}

pub(in crate::cli) fn print_repl_help() {
    print!("{}", repl_help_text());
}

/// 斜杠命令候选面板的内容：每条一行「命令 + 它是干什么的」，选中的那条
/// 行首 `▸` 并上主色。
///
/// inline 那边只能在 footer 位置塞一行挤在一起的命令名——全屏有地方，就把
/// 说明也给上，省得记不住哪个是哪个。最多四条，选中项往下走窗口跟着滚。
pub(in crate::cli) fn command_hint_lines(view: &PickerView, cols: usize) -> Vec<String> {
    use crate::render::style::{ACCENT, RESET};
    let width = cols.saturating_sub(10).max(20);
    let name_col = view
        .items
        .iter()
        .map(|item| item.text.len())
        .max()
        .unwrap_or(0)
        .min(24);
    let (start, end) = picker_window(view.items.len(), view.selected);
    (start..end)
        .map(|index| {
            let name = view.items[index].text.as_str();
            let help = view.items[index].help;
            let pad = " ".repeat(name_col.saturating_sub(name.len()));
            let line = if index == view.selected {
                format!("\x1b[1m{ACCENT}▸ {name}{RESET}{pad}  \x1b[2m{help}\x1b[0m")
            } else {
                format!("  {name}{pad}  \x1b[2m{help}\x1b[0m")
            };
            truncate_visible_width(&line, width)
        })
        .collect()
}

/// inline 的候选行：命令名挤成一行，选中的那条上主色、其余暗显。选中项在
/// 行宽之外时，从前面丢掉几条，保证它看得见。
pub(in crate::cli) fn repl_command_suggestions_line(
    suggestions: &[&str],
    selected: Option<usize>,
    max_width: usize,
) -> String {
    use crate::render::style::{ACCENT, RESET};
    let mut start = 0;
    if let Some(selected) = selected {
        while start < selected {
            let through: usize = suggestions[start..=selected]
                .iter()
                .map(|name| name.len() + 2)
                .sum();
            if through <= max_width + 2 {
                break;
            }
            start += 1;
        }
    }
    let line = suggestions[start..]
        .iter()
        .enumerate()
        .map(|(offset, name)| {
            if Some(start + offset) == selected {
                format!("\x1b[1m{ACCENT}{name}{RESET}")
            } else if selected.is_some() {
                format!("\x1b[2m{name}\x1b[0m")
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("  ");
    truncate_visible_width(&line, max_width)
}
