//! `gqy-pet`:桌面悬浮窗进程的可执行入口。
//!
//! 与主程序 `gqy` 共用同一个 lib crate,但只有它链接 wry/tao(`--features pet`)。
//! 形态是一个跟命令行走得最近的 GUI 进程:前台跑着,窗口关掉就退出;日志走
//! stderr,`GQY_PET_DEBUG=1` 打开 debug 级别。用户平时用 `gqy pet` 启动它。

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(if std::env::var_os("GQY_PET_DEBUG").is_some() {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();

    if let Err(error) = gqy::pet::run() {
        eprintln!("gqy-pet: {error:#}");
        std::process::exit(1);
    }
}
