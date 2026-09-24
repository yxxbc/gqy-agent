//! 空会话的画面：Claude Code 式的开屏欢迎框（吉祥物 + 欢迎语 + 目录 + 模式 +
//! 提示，框下列最近会话）。
//!
//! 只在**会话没有任何回合**时存在。第一条消息一发它就撤，会话模式随之钉死
//! （中途换模式 = 系统提示词换血 = 全量缓存作废）；`/new` 开出空会话它又回来。
//!
//! 09-24 起取代原来的大号渐变艺术字 + 星空 + 扫光（方案见
//! `docs/plan/2026-09-23-tui-launch-and-lobby.md` §三）。画面是静止的，空闲时
//! 不再每帧重画。吉祥物由 `display.mascot` 选（见 [`mascot`]），
//! `config/banner.txt` 是 `custom` 那一档的来源；`display.banner = false` 整个关掉。
//!
//! 画面本身不认识后端：全屏后端用 [`BannerScene::lobby`] 把它铺进正文区、输入框
//! 嵌在框下面；inline 后端用 [`BannerScene::render_ansi`] 塞在输入框上方。

use crate::agent::AgentMode;
use crate::config::AppConfig;
use crate::i18n::text as t;
use crate::paths::GqyPaths;
use crate::terminal::palette::Theme;
use crate::terminal::starfield::BannerArt;

pub(in crate::cli) mod mascot;
pub(in crate::cli) mod preview;
pub(in crate::cli) mod welcome;

use mascot::Mascot;
use welcome::{welcome_rows, WelcomeInfo};

/// 用户自带艺术字的文件名（放在配置目录下），`display.mascot = custom` 时用。
pub(in crate::cli) const BANNER_FILE: &str = "banner.txt";

/// 欢迎框最宽多少列。
const MAX_WIDTH: usize = 84;
/// inline 模式下欢迎框最多占几行（含框线与最近那行）：inline 挤在输入框上面，
/// 不能把输入框顶出屏幕。
const INLINE_MAX_ROWS: usize = 17;
/// 最近会话列几条。
const RECENT_LIMIT: usize = 3;
/// 最近会话标题最多显示几列。
const RECENT_TITLE_COLS: usize = 16;

pub(in crate::cli) struct BannerScene {
    theme: Theme,
    mascot: Mascot,
    info: WelcomeInfo,
    mode: AgentMode,
}

impl BannerScene {
    /// 按配置决定画不画、画哪种吉祥物。`None` = 关掉了。
    pub(in crate::cli) fn load(
        config: &AppConfig,
        paths: &GqyPaths,
        mode: AgentMode,
    ) -> Option<Self> {
        if !config.display.banner {
            return None;
        }
        let theme = Theme::detect();
        let custom = std::fs::read_to_string(paths.config_dir.join(BANNER_FILE))
            .ok()
            .and_then(|text| BannerArt::from_text(&text));
        let mascot = Mascot::from_setting(&config.display.mascot, custom, theme);
        // 测试里不读人格文件和会话库：`GqyPaths::new()` 指向开发机真实的 ~/.gqy
        // （AGENTS §5.3）。
        let info = if cfg!(test) {
            WelcomeInfo::default()
        } else {
            WelcomeInfo {
                persona: crate::web::persona_display_name(config, paths),
                cwd: current_dir_label(),
                recent: recent_sessions(config, paths, mode),
            }
        };
        Some(Self {
            theme,
            mascot,
            info,
            mode,
        })
    }

    pub(in crate::cli) fn set_mode(&mut self, mode: AgentMode) {
        self.mode = mode;
    }

    /// 原来用来跳过出场淡入；欢迎框是静止的，留着接口给调用方。
    pub(in crate::cli) fn settle(&mut self) {}

    /// 走一帧。欢迎框是静止的：不需要重画（空闲时零重绘）。
    pub(in crate::cli) fn tick(&mut self) -> bool {
        false
    }

    pub(in crate::cli) fn mode(&self) -> AgentMode {
        self.mode
    }

    /// inline 模式下欢迎框占几行（按当前终端宽度）。
    pub(in crate::cli) fn block_rows(&self) -> usize {
        let cols = crossterm::terminal::size()
            .map(|(cols, _)| usize::from(cols))
            .unwrap_or(80);
        self.card(inline_width(cols), INLINE_MAX_ROWS).len()
    }

    fn card(&self, width: usize, max_rows: usize) -> Vec<String> {
        welcome_rows(
            &self.info,
            &self.mascot,
            self.theme,
            self.mode,
            width,
            max_rows,
        )
    }

    /// inline 用：欢迎框缩进两格画在输入框上方，补齐或截到正好 `rows` 行。
    pub(in crate::cli) fn render_ansi(&self, cols: usize, rows: usize) -> Vec<String> {
        if cols == 0 || rows == 0 {
            return Vec::new();
        }
        let mut lines: Vec<String> = self
            .card(inline_width(cols), rows.min(INLINE_MAX_ROWS))
            .into_iter()
            .map(|line| format!("  {line}"))
            .collect();
        lines.truncate(rows);
        lines.resize(rows, String::new());
        lines
    }

    /// 全屏大厅：欢迎框和输入框（`activity_rows` 行，含前面一行空）一起垂直
    /// 居中，输入框和欢迎框同宽、左边对齐。回车发第一句话后整块撤掉，输入框
    /// 回到屏底、恢复全宽。
    pub(in crate::cli) fn lobby(&self, cols: usize, rows: usize, activity_rows: usize) -> Lobby {
        let width = lobby_width(cols);
        let left = cols.saturating_sub(width) / 2;
        let card = self.card(width, rows.saturating_sub(activity_rows + 1));
        let block_rows = card.len() + activity_rows;
        let top = rows.saturating_sub(block_rows) / 2;
        let tail_start = top + card.len();
        let mut out = vec![String::new(); rows];
        for (offset, line) in card.iter().enumerate() {
            if let Some(slot) = out.get_mut(top + offset) {
                *slot = format!("{}{line}", " ".repeat(left));
            }
        }
        Lobby {
            rows: out,
            tail_start: tail_start.min(u16::MAX as usize) as u16,
            left: left.min(u16::MAX as usize) as u16,
            width: width.min(u16::MAX as usize) as u16,
            below: (top + block_rows).min(u16::MAX as usize) as u16,
        }
    }
}

/// 全屏大厅里欢迎框（也是输入框）的宽度：终端的四分之三上下，最多 84 列。
fn lobby_width(cols: usize) -> usize {
    (cols * 3 / 4)
        .min(MAX_WIDTH)
        .min(cols.saturating_sub(2))
        .max(32.min(cols))
}

/// inline 下欢迎框的宽度：两边各留两格。
fn inline_width(cols: usize) -> usize {
    cols.saturating_sub(4).min(MAX_WIDTH)
}

/// 当前目录，家目录写成 `~`。
fn current_dir_label() -> String {
    let Ok(dir) = std::env::current_dir() else {
        return String::new();
    };
    let text = dir.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && text.starts_with(&home) => {
            format!("~{}", &text[home.len()..])
        }
        _ => text,
    }
}

/// 这条车道最近几条说过话的会话：标题（没起名就用最后一句话）与相对时间。
fn recent_sessions(config: &AppConfig, paths: &GqyPaths, mode: AgentMode) -> Vec<(String, String)> {
    let persona = if mode == AgentMode::Dev {
        crate::state::DEV_PERSONA.to_string()
    } else {
        config.active_persona_scope()
    };
    let Ok(store) = crate::state::StateStore::new(paths) else {
        return Vec::new();
    };
    let Ok(mut sessions) = store.list_sessions(&persona) else {
        return Vec::new();
    };
    sessions.retain(|session| {
        session.turn_count > 0
            && !session.record.archived
            && session.record.kind == crate::state::USER_SESSION_KIND
            && session.record.session_id != crate::state::DEFAULT_SESSION_ID
    });
    sessions.sort_by(|a, b| b.record.updated_at.cmp(&a.record.updated_at));
    let now = chrono::Utc::now();
    sessions
        .into_iter()
        .take(RECENT_LIMIT)
        .map(|session| {
            let name = session.record.name.trim().to_string();
            let title = if name.is_empty() {
                session
                    .last_user_content
                    .as_deref()
                    .and_then(|text| text.lines().next())
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            } else {
                name
            };
            (
                crate::cli::truncate_visible_width(&title, RECENT_TITLE_COLS),
                welcome::relative_age(&session.record.updated_at, now),
            )
        })
        .filter(|(title, _)| !title.is_empty())
        .collect()
}

/// 全屏大厅的一帧：整屏的行，加上输入框该落在哪。
pub(in crate::cli) struct Lobby {
    pub rows: Vec<String>,
    /// 活动区（空行 + 输入框 + footer）从第几行开始。
    pub tail_start: u16,
    /// 输入框的左边距与宽度。
    pub left: u16,
    pub width: u16,
    /// 整块结束的下一行:斜杠命令候选之类的浮层从这里往下摆。
    pub below: u16,
}

/// 空会话提示行（inline 后端在输入框上方、没有 banner 时也要有一句）。
#[allow(dead_code)]
pub(in crate::cli) fn plain_mode_hint(mode: AgentMode) -> String {
    format!(
        "{} · {}",
        welcome::mode_name(mode),
        t("Tab switches normal/dev", "Tab 切换 普通/开发")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::palette::Depth;

    fn scene(mascot: Mascot) -> BannerScene {
        BannerScene {
            theme: Theme {
                depth: Depth::True,
                ascii: false,
            },
            mascot,
            info: WelcomeInfo::default(),
            mode: AgentMode::Normal,
        }
    }

    #[test]
    fn render_fills_exactly_the_requested_rows() {
        let scene = scene(Mascot::Portrait);
        let ansi = scene.render_ansi(80, 16);
        assert_eq!(ansi.len(), 16);
        for row in &ansi {
            let width = crate::cli::repl::width::visible_width(
                &crate::cli::strip_terminal_control_sequences(row),
            );
            assert!(width <= 80, "row wider than terminal: {width}");
        }
        assert!(scene.render_ansi(0, 0).is_empty());
        assert_eq!(scene.render_ansi(20, 6).len(), 6);
    }

    /// 欢迎框是静止的：空闲时不要求重画。
    #[test]
    fn welcome_screen_does_not_animate() {
        let mut scene = scene(Mascot::Portrait);
        assert!(!scene.tick());
    }

    #[test]
    fn lobby_reserves_the_activity_band_below_the_card() {
        let scene = scene(Mascot::Portrait);
        let lobby = scene.lobby(100, 40, 5);
        assert_eq!(lobby.rows.len(), 40);
        assert!(lobby.width >= 40 && lobby.width <= 84);
        assert!(usize::from(lobby.left) + usize::from(lobby.width) <= 100);
        // 活动区那几行留白，输入框自己往上画。
        for offset in 0..5 {
            let band = &lobby.rows[usize::from(lobby.tail_start) + offset];
            assert!(band.trim().is_empty(), "band not blank: {band:?}");
        }
        // 欢迎框在活动区之上，最后一行是下框线。
        let above = &lobby.rows[usize::from(lobby.tail_start) - 1];
        assert!(crate::cli::strip_terminal_control_sequences(above).contains('╰'));
        let tiny = scene.lobby(30, 10, 4);
        assert_eq!(tiny.rows.len(), 10);
    }

    #[test]
    fn mascot_setting_and_banner_switch_are_honoured() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(BANNER_FILE), "subtitle: MINE\nAB\nCD\n").unwrap();
        let mut config = AppConfig::default();
        config.display.banner = true;
        config.display.mascot = "custom".into();
        let mut paths = crate::paths::GqyPaths::new().unwrap();
        paths.config_dir = dir.path().to_path_buf();
        let scene = BannerScene::load(&config, &paths, AgentMode::Normal).unwrap();
        assert!(matches!(&scene.mascot, Mascot::Custom(art) if art.rows() == 2));
        config.display.mascot = "off".into();
        let scene = BannerScene::load(&config, &paths, AgentMode::Normal).unwrap();
        assert!(matches!(scene.mascot, Mascot::Off));
        config.display.banner = false;
        assert!(BannerScene::load(&config, &paths, AgentMode::Normal).is_none());
    }
}
