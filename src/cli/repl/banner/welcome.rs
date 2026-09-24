//! 开屏欢迎框：Claude Code 式的小框，左边吉祥物，右边欢迎语、目录、模式、提示，
//! 框下一行列最近的会话。取代原来的大号渐变艺术字 + 星空 + 扫光。
//!
//! 09-23 用户选定，方案见 `docs/plan/2026-09-23-tui-launch-and-lobby.md` §三。
//! 这里只产行（带 SGR 的字符串），全屏大厅与 inline 都吃它。

use crate::agent::AgentMode;
use crate::cli::repl::width::visible_width;
use crate::i18n::text as t;
use crate::terminal::palette::{Theme, BLUE, CORAL, DIM, FAINT, GOLD};
use crate::terminal::starfield::Seg;
use ratatui::style::Modifier;

use super::mascot::{segs_to_ansi, Mascot, MascotRows};

/// 右边文字至少要这么宽，吉祥物才和文字并排；不够就叠在文字上面。取模式行
/// （`◉ 普通模式   ○ 开发模式   Tab 切换`，34 列）的宽度，并排时它不被截。
const TEXT_MIN: usize = 34;
/// 吉祥物与文字之间的空列。
const GAP: usize = 3;

/// 欢迎框要显示的内容。
#[derive(Clone, Debug, Default)]
pub(in crate::cli) struct WelcomeInfo {
    /// 当前人格的显示名。
    pub persona: String,
    /// 当前目录（家目录写成 `~`）。
    pub cwd: String,
    /// 最近几条非空会话：标题、相对时间。
    pub recent: Vec<(String, String)>,
}

impl WelcomeInfo {
    /// 这条车道有没有说过话的会话：决定说「欢迎回来」还是「初次见面」，
    /// 以及提示 `gqy -c` 还是 `/help`。
    fn returning(&self) -> bool {
        !self.recent.is_empty()
    }
}

/// 画欢迎框：`width` 列宽、最多 `max_rows` 行（含上下框线），另附框下的
/// 「最近」一行（有的话）。返回的每行宽度都正好是 `width`（最近那行除外）。
pub(in crate::cli) fn welcome_rows(
    info: &WelcomeInfo,
    mascot: &Mascot,
    theme: Theme,
    mode: AgentMode,
    width: usize,
    max_rows: usize,
) -> Vec<String> {
    let width = width.max(TEXT_MIN + 4);
    let inner = width - 4; // `│ ` + … + ` │`
    let text = text_lines(info, theme, mode, inner);
    let text_rows = text.len();
    let budget = max_rows.saturating_sub(2);
    // 先试并排：吉祥物高度不超过预算，宽度给文字留够。
    let side = mascot
        .render(theme, inner.saturating_sub(TEXT_MIN + GAP), budget)
        .filter(|art| art.cols + GAP + TEXT_MIN <= inner);
    let body: Vec<String> = match side {
        Some(art) => side_by_side(&art, &text, inner, theme),
        None => {
            // 并排放不下就叠在上面（黑猫这种宽图常走这条），再放不下就只画字。
            let stacked = mascot
                .render(theme, inner, budget.saturating_sub(text_rows + 1))
                .map(|art| stacked(&art, &text, inner));
            stacked.unwrap_or_else(|| text.iter().map(|line| pad(line, inner)).collect())
        }
    };
    let border = theme.fg(FAINT);
    let mut rows = Vec::with_capacity(body.len() + 3);
    rows.push(segs_to_ansi(vec![Seg::new(
        format!("╭{}╮", "─".repeat(width - 2)),
        border,
    )]));
    for line in body.into_iter().take(budget) {
        rows.push(format!(
            "{}{line}{}",
            segs_to_ansi(vec![Seg::new("│ ", border)]),
            segs_to_ansi(vec![Seg::new(" │", border)])
        ));
    }
    rows.push(segs_to_ansi(vec![Seg::new(
        format!("╰{}╯", "─".repeat(width - 2)),
        border,
    )]));
    if let Some(recent) = recent_line(info, theme, width) {
        rows.push(recent);
    }
    rows
}

/// 右边那几行字：欢迎语、目录、模式、提示。每行已着色，宽度不超过 `inner`。
fn text_lines(info: &WelcomeInfo, theme: Theme, mode: AgentMode, inner: usize) -> Vec<String> {
    let name = if info.persona.trim().is_empty() {
        t("Selene", "顾清影").to_string()
    } else {
        info.persona.trim().to_string()
    };
    let greeting = if info.returning() {
        format!(
            "{}{name}{}",
            t("Welcome back — ", "欢迎回来，"),
            t(" is here", "在这儿")
        )
    } else {
        format!("{}{name}", t("Nice to meet you, I'm ", "初次见面，我是"))
    };
    let accent = match mode {
        AgentMode::Normal => BLUE,
        AgentMode::Dev => CORAL,
    };
    let tip = if info.returning() {
        t(
            "gqy -c resumes last chat · /help",
            "gqy -c 回到上次 · /help 看命令",
        )
    } else {
        t(
            "/help for commands · /config",
            "/help 看命令 · /config 进入设置",
        )
    };
    vec![
        clip(
            vec![Seg::new(
                greeting,
                theme.fg(accent).add_modifier(Modifier::BOLD),
            )],
            inner,
        ),
        clip(vec![Seg::new(info.cwd.clone(), theme.fg(DIM))], inner),
        clip(mode_row(theme, mode), inner),
        clip(vec![Seg::new(tip, theme.fg(FAINT))], inner),
    ]
}

/// `◉ 普通模式   ○ 开发模式   Tab 切换`。当前模式用它自己的颜色。
pub(in crate::cli) fn mode_row(theme: Theme, current: AgentMode) -> Vec<Seg> {
    let mut segs = Vec::new();
    for (index, mode) in [AgentMode::Normal, AgentMode::Dev].into_iter().enumerate() {
        if index > 0 {
            segs.push(Seg::raw("   "));
        }
        let here = mode == current;
        let color = match mode {
            AgentMode::Normal => BLUE,
            AgentMode::Dev => CORAL,
        };
        let style = if here {
            theme.fg(color).add_modifier(Modifier::BOLD)
        } else {
            theme.fg(FAINT)
        };
        let dot = if here {
            theme.dot_here()
        } else {
            theme.dot_todo()
        };
        segs.push(Seg::new(dot, style));
        segs.push(Seg::raw(" "));
        segs.push(Seg::new(mode_name(mode), style));
    }
    segs.push(Seg::raw("   "));
    segs.push(Seg::new("Tab", theme.fg(GOLD)));
    segs.push(Seg::raw(" "));
    segs.push(Seg::new(t("switch", "切换"), theme.fg(FAINT)));
    segs
}

pub(in crate::cli) fn mode_name(mode: AgentMode) -> &'static str {
    match mode {
        AgentMode::Normal => t("normal", "普通模式"),
        AgentMode::Dev => t("dev", "开发模式"),
    }
}

/// 吉祥物在左、文字在右，文字在吉祥物高度里垂直居中。
fn side_by_side(art: &MascotRows, text: &[String], inner: usize, _theme: Theme) -> Vec<String> {
    let height = art.rows.len().max(text.len());
    let text_top = height.saturating_sub(text.len()) / 2;
    let text_width = inner - art.cols - GAP;
    (0..height)
        .map(|y| {
            let left = art
                .rows
                .get(y)
                .cloned()
                .unwrap_or_else(|| " ".repeat(art.cols));
            let right = y
                .checked_sub(text_top)
                .and_then(|index| text.get(index))
                .map(|line| {
                    pad(
                        &crate::cli::truncate_visible_width(line, text_width),
                        text_width,
                    )
                })
                .unwrap_or_else(|| " ".repeat(text_width));
            format!("{left}{}{right}", " ".repeat(GAP))
        })
        .collect()
}

/// 吉祥物居中在上，空一行，文字在下。
fn stacked(art: &MascotRows, text: &[String], inner: usize) -> Vec<String> {
    let left = inner.saturating_sub(art.cols) / 2;
    let mut rows: Vec<String> = art
        .rows
        .iter()
        .map(|row| {
            format!(
                "{}{row}{}",
                " ".repeat(left),
                " ".repeat(inner - left - art.cols)
            )
        })
        .collect();
    rows.push(" ".repeat(inner));
    rows.extend(text.iter().map(|line| pad(line, inner)));
    rows
}

/// 框下那行：`最近：修 CI 报错（2 小时前）· 周末去哪（昨天）`，放不下的截掉。
fn recent_line(info: &WelcomeInfo, theme: Theme, width: usize) -> Option<String> {
    if info.recent.is_empty() {
        return None;
    }
    let items: Vec<String> = info
        .recent
        .iter()
        .map(|(title, age)| format!("{title}（{age}）"))
        .collect();
    let line = format!("  {}{}", t("Recent: ", "最近："), items.join(" · "));
    Some(clip(vec![Seg::new(line, theme.fg(FAINT))], width))
}

fn clip(segs: Vec<Seg>, width: usize) -> String {
    let line = segs_to_ansi(segs);
    crate::cli::truncate_visible_width(&line, width)
}

/// 按显示宽度补空格到 `width` 列。
fn pad(line: &str, width: usize) -> String {
    let used = visible_width(line);
    format!("{line}{}", " ".repeat(width.saturating_sub(used)))
}

/// 会话的相对时间：刚刚 / N 分钟前 / N 小时前 / 昨天 / N 天前。
pub(in crate::cli) fn relative_age(updated_at: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let Ok(then) = chrono::DateTime::parse_from_rfc3339(updated_at) else {
        return String::new();
    };
    let minutes = (now - then.with_timezone(&chrono::Utc))
        .num_minutes()
        .max(0);
    match minutes {
        0..=1 => t("just now", "刚刚").to_string(),
        2..=59 => format!("{minutes}{}", t(" min ago", " 分钟前")),
        60..=1439 => format!("{}{}", minutes / 60, t(" h ago", " 小时前")),
        1440..=2879 => t("yesterday", "昨天").to_string(),
        _ => format!("{}{}", minutes / 1440, t(" days ago", " 天前")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::palette::Depth;

    fn theme() -> Theme {
        Theme {
            depth: Depth::True,
            ascii: false,
        }
    }

    fn plain(row: &str) -> String {
        crate::cli::strip_terminal_control_sequences(row)
    }

    fn info(recent: bool) -> WelcomeInfo {
        WelcomeInfo {
            persona: "顾清影".into(),
            cwd: "~/Projects/gqy-agent".into(),
            recent: if recent {
                vec![("修 CI 报错".into(), "2 小时前".into())]
            } else {
                Vec::new()
            },
        }
    }

    /// 每一行（除最近那行）宽度都等于框宽，框线对得齐。
    #[test]
    fn card_rows_are_exactly_the_card_width() {
        for (mascot, width) in [
            (Mascot::Portrait, 76),
            (Mascot::Portrait, 60),
            (Mascot::Cat, 70),
            (Mascot::Off, 40),
        ] {
            let rows = welcome_rows(&info(false), &mascot, theme(), AgentMode::Normal, width, 40);
            assert!(plain(&rows[0]).starts_with('╭'));
            let last = rows.len() - 1;
            assert!(plain(&rows[last]).starts_with('╰'));
            for row in &rows {
                assert_eq!(visible_width(&plain(row)), width, "{:?}", plain(row));
            }
        }
    }

    #[test]
    fn greeting_and_recent_follow_the_history() {
        let fresh = welcome_rows(
            &info(false),
            &Mascot::Off,
            theme(),
            AgentMode::Normal,
            50,
            20,
        );
        let text: String = fresh.iter().map(|row| plain(row)).collect();
        assert!(text.contains("初次见面") || text.contains("Nice to meet you"));
        assert!(!text.contains("最近") && !text.contains("Recent"));
        let back = welcome_rows(
            &info(true),
            &Mascot::Off,
            theme(),
            AgentMode::Normal,
            50,
            20,
        );
        let text: String = back.iter().map(|row| plain(row)).collect();
        assert!(text.contains("欢迎回来") || text.contains("Welcome back"));
        assert!(text.contains("修 CI 报错"));
    }

    /// 矮终端：吉祥物放不下就只画字，行数不超过给的预算。
    #[test]
    fn short_terminal_drops_the_mascot() {
        let rows = welcome_rows(
            &info(false),
            &Mascot::Portrait,
            theme(),
            AgentMode::Normal,
            70,
            8,
        );
        assert!(rows.len() <= 8);
        assert!(rows.iter().all(|row| !row.contains('▀')));
    }

    #[test]
    fn ages_read_naturally() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-24T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert!(relative_age("2026-09-24T11:30:00Z", now).starts_with("30"));
        assert!(relative_age("2026-09-24T09:00:00Z", now).starts_with('3'));
        assert!(!relative_age("2026-09-23T10:00:00Z", now).is_empty());
        assert!(relative_age("garbage", now).is_empty());
    }
}
