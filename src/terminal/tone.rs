//! 终端底色是深是浅。
//!
//! 渲染层的配色（diff 底色、代码高亮、强调色）按深浅各备一套，这里只回答
//! 「现在是哪一套」。判定顺序：配置 `display.theme` 显式写了就听它；`auto`
//! 时交互 REPL 启动前用 OSC 11 问一次终端底色，问不到再看 `COLORFGBG`，
//! 都没有就当深色——绝大多数终端默认深底。
//!
//! 结果进程内只定一次（先到先得）。测试里固定深色，不受开发机终端影响
//! （AGENTS §5.3）。

use std::sync::OnceLock;

use super::palette::Rgb;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tone {
    Dark,
    Light,
}

static TONE: OnceLock<Tone> = OnceLock::new();

/// 当前底色。还没 [`init`] 过就只看环境变量，不发查询。
pub(crate) fn current() -> Tone {
    *TONE.get_or_init(|| resolve(&setting_from_env().unwrap_or_default(), false))
}

/// 定下本进程的底色。`setting` 是配置 `display.theme`，环境变量 `GQY_THEME`
/// 非空时盖过它（临时试另一套用）。`probe = true` 时允许向终端发 OSC 11
/// 查询——只有交互 REPL 该这么做：它此刻独占终端，输入线程还没起，回包不会
/// 被当成按键吃掉。
pub(crate) fn init(setting: &str, probe: bool) -> Tone {
    *TONE.get_or_init(|| {
        let setting = setting_from_env().unwrap_or_else(|| setting.to_string());
        resolve(&setting, probe)
    })
}

fn setting_from_env() -> Option<String> {
    std::env::var("GQY_THEME")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn resolve(setting: &str, probe: bool) -> Tone {
    match setting.trim().to_ascii_lowercase().as_str() {
        "light" => return Tone::Light,
        "dark" => return Tone::Dark,
        _ => {}
    }
    if cfg!(test) {
        return Tone::Dark;
    }
    if probe {
        if let Some(color) = query_background() {
            return tone_of(color);
        }
    }
    std::env::var("COLORFGBG")
        .ok()
        .and_then(|value| from_colorfgbg(&value))
        .unwrap_or(Tone::Dark)
}

/// 相对亮度过半算浅底。
fn tone_of((r, g, b): Rgb) -> Tone {
    let luma = 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b);
    if luma > 127.5 {
        Tone::Light
    } else {
        Tone::Dark
    }
}

/// `COLORFGBG=15;0`：最后一段是底色的 16 色序号。7 与 9..=15 是浅色。
fn from_colorfgbg(value: &str) -> Option<Tone> {
    let background: u8 = value.rsplit(';').next()?.trim().parse().ok()?;
    Some(if background == 7 || (9..=15).contains(&background) {
        Tone::Light
    } else {
        Tone::Dark
    })
}

/// 解析 OSC 11 回包里的 `rgb:RRRR/GGGG/BBBB`（每段 1–4 位十六进制）。
fn parse_osc11(reply: &str) -> Option<Rgb> {
    let start = reply.find("rgb:")? + 4;
    let body = &reply[start..];
    let end = body
        .find(|c: char| !(c.is_ascii_hexdigit() || c == '/'))
        .unwrap_or(body.len());
    let mut parts = body[..end].split('/');
    let mut channel = || -> Option<u8> {
        let hex = parts.next()?;
        if hex.is_empty() || hex.len() > 4 {
            return None;
        }
        let value = u32::from_str_radix(hex, 16).ok()?;
        let max = (1u32 << (4 * hex.len())) - 1;
        Some((value * 255 / max) as u8)
    };
    Some((channel()?, channel()?, channel()?))
}

/// 向控制终端问底色。后面紧跟一个 DA1（所有终端都答），收到 DA1 的回包就
/// 知道 OSC 11 不会再来了，不必干等到超时。
#[cfg(unix)]
fn query_background() -> Option<Rgb> {
    use std::io::{IsTerminal, Read, Write};
    use std::os::fd::AsRawFd;
    use std::time::{Duration, Instant};

    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return None;
    }
    let mut tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    let fd = tty.as_raw_fd();
    let mut saved = std::mem::MaybeUninit::<libc::termios>::uninit();
    // SAFETY: fd 是刚打开的 /dev/tty；termios 由 tcgetattr 填满后才读。
    let saved = unsafe {
        if libc::tcgetattr(fd, saved.as_mut_ptr()) != 0 {
            return None;
        }
        saved.assume_init()
    };
    let mut raw = saved;
    raw.c_lflag &= !(libc::ICANON | libc::ECHO);
    raw.c_cc[libc::VMIN] = 0;
    raw.c_cc[libc::VTIME] = 0;
    // SAFETY: 同上，raw 是 saved 的副本。
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
        return None;
    }
    let mut reply = Vec::new();
    if tty.write_all(b"\x1b]11;?\x1b\\\x1b[c").is_ok() && tty.flush().is_ok() {
        let deadline = Instant::now() + Duration::from_millis(150);
        let mut buffer = [0u8; 256];
        while !da1_seen(&reply) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            let mut poll = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: 单个 pollfd，超时毫秒数有界。
            let ready = unsafe { libc::poll(&mut poll, 1, left.as_millis() as libc::c_int) };
            if ready <= 0 {
                break;
            }
            match tty.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => reply.extend_from_slice(&buffer[..count]),
            }
        }
    }
    // SAFETY: 还原进来时的 termios。
    unsafe {
        libc::tcsetattr(fd, libc::TCSANOW, &saved);
    }
    parse_osc11(&String::from_utf8_lossy(&reply))
}

#[cfg(not(unix))]
fn query_background() -> Option<Rgb> {
    None
}

/// DA1 回包形如 `ESC [ ? 6 2 ; … c`。
fn da1_seen(reply: &[u8]) -> bool {
    reply
        .windows(3)
        .position(|window| window == b"\x1b[?")
        .is_some_and(|start| reply[start..].contains(&b'c'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_setting_wins_and_auto_is_dark_under_test() {
        assert_eq!(resolve("light", true), Tone::Light);
        assert_eq!(resolve(" Dark ", true), Tone::Dark);
        assert_eq!(resolve("auto", true), Tone::Dark);
        assert_eq!(resolve("", false), Tone::Dark);
    }

    #[test]
    fn osc11_reply_parses_all_widths() {
        let reply = "\x1b]11;rgb:ffff/fafa/f0f0\x1b\\\x1b[?62;22c";
        assert_eq!(parse_osc11(reply), Some((255, 250, 240)));
        assert_eq!(parse_osc11("\x1b]11;rgb:1e/1e/2e\x07"), Some((30, 30, 46)));
        assert_eq!(parse_osc11("\x1b[?62c"), None);
        assert_eq!(tone_of((255, 250, 240)), Tone::Light);
        assert_eq!(tone_of((30, 30, 46)), Tone::Dark);
    }

    #[test]
    fn colorfgbg_background_index_decides() {
        assert_eq!(from_colorfgbg("15;0"), Some(Tone::Dark));
        assert_eq!(from_colorfgbg("0;15"), Some(Tone::Light));
        assert_eq!(from_colorfgbg("0;default;7"), Some(Tone::Light));
        assert_eq!(from_colorfgbg("garbage"), None);
    }

    #[test]
    fn da1_reply_ends_the_wait() {
        assert!(!da1_seen(b"\x1b]11;rgb:0/0/0\x1b\\"));
        assert!(da1_seen(b"\x1b]11;rgb:0/0/0\x1b\\\x1b[?62;22c"));
    }
}
