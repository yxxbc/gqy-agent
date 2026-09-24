//! 开屏欢迎框里的吉祥物（`display.mascot`）。
//!
//! 四种：顾清影立绘（默认）、黑猫字符画、`config/banner.txt` 自定义、不画。
//! 方案与素材来历见 `docs/plan/2026-09-23-tui-launch-and-lobby.md` §三。
//!
//! 立绘有两种画法：kitty 下用 Unicode 占位符贴原图（小而清楚，16 列 × 8 行
//! 以内）；其余真彩 / 256 色终端用半格字符画（`▀` 上半格前景、下半格底色，
//! 32 列或 24 列）。16 色以下画不出立绘，自动换成黑猫。
//!
//! 立绘源图是 `assets/mascot/portrait.png`（`testkit/tui/mascot.py
//! --export-portrait` 从用户提供的原图裁出头肩、缩到 256 像素宽），半格版在
//! 运行时按终端能给的格数现缩。

use std::sync::OnceLock;

use image::{imageops::FilterType, DynamicImage, GenericImageView};
use ratatui::style::{Color, Style};

use crate::terminal::palette::{to_256, Depth, Rgb, Theme};
use crate::terminal::starfield::{gradient_banner, BannerArt, Seg};
use crate::terminal::tone::{self, Tone};

const PORTRAIT_PNG: &[u8] = include_bytes!("../../../../assets/mascot/portrait.png");
const CAT_ART: &str = include_str!("../../../../assets/mascot/cat.txt");

/// 半格立绘的两档宽度（列）。高度按图的比例算：32 列约 17 行，24 列约 13 行。
const PORTRAIT_WIDTHS: [usize; 2] = [32, 24];
/// kitty 贴图最多占的格子。
const KITTY_MAX: (u16, u16) = (16, 8);
/// 透明度低于这个就当背景，不画。
const ALPHA_CUT: u8 = 128;

/// 黑猫的青瓷绿渐变：上淡下浓，深浅底各一套。
const CAT_DARK: (Rgb, Rgb) = ((0xc9, 0xd3, 0xd0), (0x7f, 0xb5, 0xa3));
const CAT_LIGHT: (Rgb, Rgb) = ((0x6a, 0x75, 0x72), (0x2f, 0x6f, 0x5c));

pub(in crate::cli) enum Mascot {
    Portrait,
    Cat,
    Custom(BannerArt),
    Off,
}

/// 画好的吉祥物：每行一条带 SGR 的字符串，以及它占多少列。
pub(in crate::cli) struct MascotRows {
    pub rows: Vec<String>,
    pub cols: usize,
}

impl Mascot {
    /// 按配置挑一种。`custom` 读不到 `banner.txt` 时退回立绘；立绘在画不出的
    /// 色深下换黑猫。
    pub(in crate::cli) fn from_setting(
        setting: &str,
        custom: Option<BannerArt>,
        theme: Theme,
    ) -> Self {
        let chosen = match setting.trim() {
            "off" => Mascot::Off,
            "cat" => Mascot::Cat,
            "custom" => custom.map(Mascot::Custom).unwrap_or(Mascot::Portrait),
            _ => Mascot::Portrait,
        };
        match chosen {
            Mascot::Portrait if !portrait_ok(theme) => Mascot::Cat,
            other => other,
        }
    }

    /// 在最多 `max_cols` × `max_rows` 格里画出来；放不下返回 `None`（欢迎框
    /// 就只画文字）。
    pub(in crate::cli) fn render(
        &self,
        theme: Theme,
        max_cols: usize,
        max_rows: usize,
    ) -> Option<MascotRows> {
        match self {
            Mascot::Off => None,
            Mascot::Portrait => portrait(theme, max_cols, max_rows),
            Mascot::Cat => cat(theme, max_cols, max_rows),
            Mascot::Custom(art) => custom(art, theme, max_cols, max_rows),
        }
    }
}

fn portrait_ok(theme: Theme) -> bool {
    matches!(theme.depth, Depth::True | Depth::X256)
}

fn portrait_image() -> Option<&'static DynamicImage> {
    static IMAGE: OnceLock<Option<DynamicImage>> = OnceLock::new();
    IMAGE
        .get_or_init(|| image::load_from_memory(PORTRAIT_PNG).ok())
        .as_ref()
}

fn portrait(theme: Theme, max_cols: usize, max_rows: usize) -> Option<MascotRows> {
    let image = portrait_image()?;
    if let Some(rows) = kitty_portrait(image, max_cols, max_rows) {
        return Some(rows);
    }
    for width in PORTRAIT_WIDTHS {
        let height = half_block_height(image, width);
        if width <= max_cols && height / 2 <= max_rows {
            return Some(half_block(image, theme, width, height));
        }
    }
    None
}

/// 缩到 `width` 列时的像素高度（取偶数：半格一格装两个像素）。
fn half_block_height(image: &DynamicImage, width: usize) -> usize {
    let (w, h) = image.dimensions();
    let height = (h as f32 * width as f32 / w as f32).round() as usize;
    height + height % 2
}

fn half_block(image: &DynamicImage, theme: Theme, width: usize, height: usize) -> MascotRows {
    let small = image
        .resize_exact(width as u32, height as u32, FilterType::Triangle)
        .to_rgba8();
    let mut rows = Vec::with_capacity(height / 2);
    for y in (0..height as u32).step_by(2) {
        let mut segs = Vec::with_capacity(width);
        for x in 0..width as u32 {
            let top = small.get_pixel(x, y).0;
            let bottom = small.get_pixel(x, y + 1).0;
            let (show_top, show_bottom) = (top[3] >= ALPHA_CUT, bottom[3] >= ALPHA_CUT);
            let top_rgb = (top[0], top[1], top[2]);
            let bottom_rgb = (bottom[0], bottom[1], bottom[2]);
            segs.push(match (show_top, show_bottom) {
                (true, true) => Seg::new("▀", with_bg(theme.fg(top_rgb), theme, bottom_rgb)),
                (true, false) => Seg::new("▀", theme.fg(top_rgb)),
                (false, true) => Seg::new("▄", theme.fg(bottom_rgb)),
                (false, false) => Seg::raw(" "),
            });
        }
        rows.push(segs_to_ansi(segs));
    }
    MascotRows { rows, cols: width }
}

fn with_bg(style: Style, theme: Theme, color: Rgb) -> Style {
    match theme.depth {
        Depth::True => style.bg(Color::Rgb(color.0, color.1, color.2)),
        Depth::X256 => style.bg(Color::Indexed(to_256(color))),
        Depth::Ansi16 | Depth::Mono => style,
    }
}

/// kitty 下贴原图。像素只传一次（进程内缓存占位符网格），之后每帧画的都是
/// 普通字符格，能跟着大厅一起重画。测试与非 kitty 终端不走这条。
fn kitty_portrait(image: &DynamicImage, max_cols: usize, max_rows: usize) -> Option<MascotRows> {
    if cfg!(test) || !crate::terminal::kitty::is_native_kitty_terminal() {
        return None;
    }
    let (want_cols, want_rows) = KITTY_MAX;
    let cols = want_cols.min(u16::try_from(max_cols).unwrap_or(u16::MAX));
    let rows = want_rows.min(u16::try_from(max_rows).unwrap_or(u16::MAX));
    if cols < 8 || rows < 4 {
        return None;
    }
    static GRID: OnceLock<Option<(Vec<String>, u16, u16)>> = OnceLock::new();
    let (grid, grid_cols, grid_rows) = GRID
        .get_or_init(|| {
            let (transfer, grid) =
                crate::terminal::kitty::kitty_image_parts(image, cols, rows).ok()?;
            use std::io::Write;
            let mut stdout = std::io::stdout();
            stdout.write_all(transfer.as_bytes()).ok()?;
            stdout.flush().ok()?;
            let grid_cols = grid
                .first()
                .map(|line| line.chars().filter(|c| *c == '\u{10eeee}').count())
                .unwrap_or(0) as u16;
            let grid_rows = grid.len() as u16;
            Some((grid, grid_cols, grid_rows))
        })
        .as_ref()?;
    // 第一次按当时能给的格数传了图；之后画面变窄放不下，就退回字符画。
    if usize::from(*grid_cols) > max_cols || usize::from(*grid_rows) > max_rows {
        return None;
    }
    Some(MascotRows {
        rows: grid.clone(),
        cols: usize::from(*grid_cols),
    })
}

fn cat(theme: Theme, max_cols: usize, max_rows: usize) -> Option<MascotRows> {
    let lines: Vec<&str> = CAT_ART.lines().collect();
    let cols = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    if cols > max_cols || lines.len() > max_rows {
        return None;
    }
    let (from, to) = match tone::current() {
        Tone::Dark => CAT_DARK,
        Tone::Light => CAT_LIGHT,
    };
    let last = lines.len().saturating_sub(1).max(1) as f32;
    let rows = lines
        .iter()
        .enumerate()
        .map(|(y, line)| {
            let style = theme.lerp(from, to, y as f32 / last);
            let padded = format!("{line:<cols$}");
            segs_to_ansi(vec![Seg::new(padded, style)])
        })
        .collect();
    Some(MascotRows { rows, cols })
}

fn custom(art: &BannerArt, theme: Theme, max_cols: usize, max_rows: usize) -> Option<MascotRows> {
    if art.cols() > max_cols || art.rows() > max_rows {
        return None;
    }
    let rows = gradient_banner(art, theme, None)
        .into_iter()
        .map(segs_to_ansi)
        .collect();
    Some(MascotRows {
        rows,
        cols: art.cols(),
    })
}

pub(in crate::cli) fn segs_to_ansi(segs: Vec<Seg>) -> String {
    use crate::cli::repl::tail::screen::ansi::{spans_to_ansi, AnsiSpan};
    let spans: Vec<AnsiSpan> = segs
        .into_iter()
        .map(|seg| AnsiSpan::styled(seg.text, seg.style))
        .collect();
    spans_to_ansi(&spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(depth: Depth) -> Theme {
        Theme {
            depth,
            ascii: false,
        }
    }

    fn width_of(row: &str) -> usize {
        crate::cli::repl::width::visible_width(&crate::cli::strip_terminal_control_sequences(row))
    }

    #[test]
    fn portrait_picks_the_largest_size_that_fits() {
        let big = Mascot::Portrait.render(theme(Depth::True), 40, 30).unwrap();
        assert_eq!(big.cols, 32);
        assert!(big.rows.iter().all(|row| width_of(row) == 32));
        let small = Mascot::Portrait.render(theme(Depth::True), 30, 14).unwrap();
        assert_eq!(small.cols, 24);
        assert!(Mascot::Portrait
            .render(theme(Depth::True), 20, 30)
            .is_none());
    }

    #[test]
    fn portrait_falls_back_to_the_cat_on_16_colors() {
        let custom = None;
        assert!(matches!(
            Mascot::from_setting("portrait", custom, theme(Depth::Ansi16)),
            Mascot::Cat
        ));
        assert!(matches!(
            Mascot::from_setting("custom", None, theme(Depth::True)),
            Mascot::Portrait
        ));
        assert!(matches!(
            Mascot::from_setting("off", None, theme(Depth::True)),
            Mascot::Off
        ));
    }

    #[test]
    fn cat_is_48_by_23_and_needs_the_room() {
        let cat = Mascot::Cat.render(theme(Depth::True), 60, 30).unwrap();
        assert_eq!((cat.cols, cat.rows.len()), (48, 23));
        assert!(cat.rows.iter().all(|row| width_of(row) == 48));
        assert!(Mascot::Cat.render(theme(Depth::True), 47, 30).is_none());
        assert!(Mascot::Cat.render(theme(Depth::True), 60, 22).is_none());
    }
}
