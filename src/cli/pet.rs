//! `gqy pet`:把桌面悬浮窗进程拉起来。
//!
//! 悬浮窗是独立进程(`gqy-pet`,链接 wry/tao;主程序不链接),这里只负责找到
//! 同目录下的它、脱离终端启动,然后立刻返回——窗口什么时候关是它自己的事。
//! 方案见 `docs/design/2026-09-28-desktop-pet.md`。

use crate::cli::*;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;

pub(in crate::cli) fn run_pet(paths: &GqyPaths, dry_run: bool) -> Result<()> {
    let binary = pet_binary()?;
    if dry_run {
        println!("{}", binary.display());
        return Ok(());
    }

    // 它是个 GUI 进程,stdout/stderr 没人看:落到 state 目录的日志里,
    // 启动失败(缺 feature、没有桌面会话)时用户有个地方查。
    let log_path = paths.state_dir.join("pet.log");
    std::fs::create_dir_all(&paths.state_dir)?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| t("could not open the pet log file", "打不开悬浮窗的日志文件"))?;

    let mut command = std::process::Command::new(&binary);
    command
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    #[cfg(unix)]
    unsafe {
        // 脱离控制终端:终端里 Ctrl+C 不该把悬浮窗一起带走。
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command
        .spawn()
        .with_context(|| t("could not start gqy-pet", "启动 gqy-pet 失败"))?;

    eprintln!(
        "\x1b[2m{} (pid {})\x1b[0m",
        if is_zh() {
            "悬浮窗已开"
        } else {
            "the desktop window is up"
        },
        child.id()
    );
    Ok(())
}

/// 悬浮窗二进制在主程序旁边(同一套安装目录 / 同一个 cargo target 目录)。
/// `EXE_SUFFIX` 让 Windows 上找的是 `gqy-pet.exe`。
fn pet_binary() -> Result<PathBuf> {
    let exe = crate::paths::gqy_executable()?;
    let directory = exe.parent().context(t(
        "the GQY executable has no directory",
        "GQY 可执行文件没有所在目录",
    ))?;
    let candidate = directory.join(format!("gqy-pet{}", std::env::consts::EXE_SUFFIX));
    if candidate.is_file() {
        return Ok(candidate);
    }
    bail!(t(
        "gqy-pet is not next to the gqy binary. The desktop window ships as a separate binary; build it with `cargo install --path . --features pet` (or, while developing, `cargo build --features pet --bin gqy-pet`).",
        "gqy 旁边没有 gqy-pet。悬浮窗是单独一个二进制,要带 pet 功能构建:`cargo install --path . --features pet`(开发时用 `cargo build --features pet --bin gqy-pet`)。"
    ))
}
