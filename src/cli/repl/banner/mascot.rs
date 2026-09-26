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
//!
//! 黑猫是字符画，三档降级（48×23 / 24×12 / 16×8，同一张原图整块降采样）：欢迎框
//! 并排时最多只能给 43 列，只留原图那一档等于永远不画猫。

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

/// 黑猫的降级阶梯：原图按这个倍数整块降采样，从大到小试（1 = 原图 48×23）。
/// 并排时欢迎框最多只给 43 列（`MAX_WIDTH` 84 扣掉框线 4、文字下限 34、间隔 3），
/// 原图那一档在任何终端宽度下都塞不进，没有阶梯就永远看不见猫（09-27 验收问题 8）。
const CAT_RUNGS: [usize; 3] = [1, 2, 3];
/// 字符画的密度阶梯，越靠后越黑；降采样按块取平均密度。
const CAT_RAMP: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

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

/// 黑猫：挑阶梯里放得下的最大一档，一档都放不下才返回 `None`。
fn cat(theme: Theme, max_cols: usize, max_rows: usize) -> Option<MascotRows> {
    cat_ladder()
        .iter()
        .find(|(cols, lines)| *cols <= max_cols && lines.len() <= max_rows)
        .map(|(cols, lines)| paint_cat(lines, *cols, theme))
}

/// 阶梯的三档字符画（宽度 + 行）。降采样是原图的纯函数，进程内算一次。
fn cat_ladder() -> &'static [(usize, Vec<String>)] {
    static LADDER: OnceLock<Vec<(usize, Vec<String>)>> = OnceLock::new();
    LADDER.get_or_init(|| {
        let art: Vec<&str> = CAT_ART.lines().collect();
        CAT_RUNGS
            .iter()
            .map(|factor| {
                let lines = shrink_cat(&art, *factor);
                let cols = lines
                    .iter()
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or(0);
                (cols, lines)
            })
            .collect()
    })
}

/// 整块平均密度降采样：`factor` 倍缩小，每块画成它的平均密度对应的那个字符。
/// 直接隔行隔列取样会把一字符宽的耳朵和尾巴轮廓抽掉，猫就散架了。
fn shrink_cat(lines: &[&str], factor: usize) -> Vec<String> {
    if factor <= 1 {
        return lines.iter().map(|line| line.to_string()).collect();
    }
    let width = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let grid: Vec<Vec<char>> = lines
        .iter()
        .map(|line| {
            let mut chars: Vec<char> = line.chars().collect();
            chars.resize(width, ' ');
            chars
        })
        .collect();
    let rows = grid.len().div_ceil(factor);
    let cols = width.div_ceil(factor);
    (0..rows)
        .map(|block_y| {
            (0..cols)
                .map(|block_x| {
                    let mut total = 0usize;
                    let mut taken = 0usize;
                    for y in block_y * factor..(block_y + 1) * factor {
                        let Some(row) = grid.get(y) else { continue };
                        for x in block_x * factor..(block_x + 1) * factor {
                            total += density(row.get(x).copied().unwrap_or(' '));
                            taken += 1;
                        }
                    }
                    // 整数四舍五入回密度阶梯；整块空白（或整块越界）才是空白。
                    if taken == 0 {
                        ' '
                    } else {
                        let index = (2 * total + taken) / (2 * taken);
                        CAT_RAMP[index.min(CAT_RAMP.len() - 1)]
                    }
                })
                .collect()
        })
        .collect()
}

/// 字符在密度阶梯上的位置；认不出的字符当空白（不污染降采样）。
fn density(cell: char) -> usize {
    CAT_RAMP.iter().position(|ch| *ch == cell).unwrap_or(0)
}

/// 按深浅底各一套的青瓷绿渐变逐行上色（上淡下浓）。
fn paint_cat(lines: &[String], cols: usize, theme: Theme) -> MascotRows {
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
    MascotRows { rows, cols }
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

    /// 最大的一档必须是原图：大终端不该因此丢掉细节。
    #[test]
    fn cat_keeps_the_full_48_by_23_art_when_it_fits() {
        let cat = Mascot::Cat.render(theme(Depth::True), 60, 30).unwrap();
        assert_eq!((cat.cols, cat.rows.len()), (48, 23));
        assert!(cat.rows.iter().all(|row| width_of(row) == 48));
    }

    /// 并排那一档最多只有 43 列（`banner/mod.rs` 的 MAX_WIDTH 84 扣掉框线 4、
    /// 文字下限 34、间隔 3），原图 48 列在任何终端宽度下都塞不进去——09-27
    /// 验收问题 8 就是「设了 cat 却什么都看不到」。必须有更小的档。
    #[test]
    fn cat_falls_down_the_ladder_when_the_room_is_tight() {
        let medium = Mascot::Cat.render(theme(Depth::True), 43, 22).unwrap();
        assert_eq!((medium.cols, medium.rows.len()), (24, 12));
        assert!(medium.rows.iter().all(|row| width_of(row) == 24));
        let small = Mascot::Cat.render(theme(Depth::True), 20, 11).unwrap();
        assert_eq!((small.cols, small.rows.len()), (16, 8));
        // 连最小的一档都放不下，才真的什么都不画。
        assert!(Mascot::Cat.render(theme(Depth::True), 15, 30).is_none());
        assert!(Mascot::Cat.render(theme(Depth::True), 60, 7).is_none());
    }

    /// 小档得还是一只坐着的黑猫：剪影不能降采样降到断成几块。
    /// 密度阶梯里越靠后的字符越黑，用它数「有墨的格子」占比。
    #[test]
    fn small_rungs_keep_a_solid_silhouette() {
        for max_cols in [43, 20] {
            let cat = Mascot::Cat
                .render(theme(Depth::True), max_cols, 30)
                .unwrap();
            let inked: usize = cat
                .rows
                .iter()
                .map(|row| crate::cli::strip_terminal_control_sequences(row))
                .map(|row| row.chars().filter(|c| *c != ' ').count())
                .sum();
            let area = cat.cols * cat.rows.len();
            assert!(
                inked * 100 >= area * 35,
                "{max_cols} 列这一档只剩 {:.0}% 的墨",
                inked as f64 * 100.0 / area as f64
            );
        }
    }

    /// 按欢迎框并排时真正传给黑猫的格子数（大厅：宽度取终端 3/4 封顶 84，
    /// 行数扣掉活动区 5 行和框线 2 行），常见终端尺寸下必须画得出来。
    #[test]
    fn cat_fits_the_side_by_side_room_of_real_terminal_sizes() {
        for (term_cols, term_rows) in [(100usize, 30usize), (120, 35), (80, 24)] {
            let width = (term_cols * 3 / 4).min(84).max(32);
            let inner = width - 4;
            let budget = term_rows.saturating_sub(6).saturating_sub(2);
            let art = Mascot::Cat
                .render(theme(Depth::True), inner.saturating_sub(37), budget)
                .unwrap_or_else(|| panic!("{term_cols}x{term_rows} 画不出黑猫"));
            assert!(art.cols + 3 + 34 <= inner, "还是太宽：{}", art.cols);
            assert!(art.rows.len() <= budget, "还是太高：{}", art.rows.len());
        }
    }
}
