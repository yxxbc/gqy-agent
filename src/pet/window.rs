//! 悬浮窗本体:透明、无边框、置顶。按住人物可以拖走,双击开 WebUI,关掉窗口
//! 时把位置记下来。
//!
//! 页面只通过 IPC 跟我们说话(`window.ipc.postMessage(...)`,消息形如
//! `{"cmd":"drag"}`)。页面线程不直接碰窗口——指令经事件循环代理回传,
//! 窗口与退出只在事件循环线程里动。

use crate::config::AppConfig;
use crate::ipc::PetState;
use crate::paths::GqyPaths;
use crate::pet::{ipc, page, state};
use anyhow::{Context, Result};
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder};
#[cfg(target_os = "macos")]
use tao::platform::macos::{EventLoopExtMacOS, WindowBuilderExtMacOS};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

/// 立绘上方留的余量(逻辑像素):呼吸与浮动动效要有地方挪,不然会被窗口边裁掉。
const HEADROOM: f64 = 40.0;

/// 页面发来的指令。
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum Command {
    /// 按住人物拖动窗口(用系统的原生拖窗,比自己跟鼠标顺)。
    Drag,
    /// 双击 / 右键菜单:打开 WebUI。
    OpenWebui,
    /// 关闭悬浮窗。
    Close,
    /// 右键菜单:置顶开关。窗口与配置一起改,免得下次开又变回来。
    SetOnTop { on: bool },
    /// 右键菜单:缩放。夹在 0.5–2.0,写回配置并把窗口尺寸改掉。
    SetScale { scale: f32 },
    /// 页面加载完了。带上立绘的自然宽度:它 > 0 就说明内联的图真的解码出来了,
    /// 而不是一块空白窗(截屏看不见的时候,这一条就是证据)。
    Ready {
        #[serde(default)]
        width: u32,
    },
}

/// 送进事件循环的意图。IPC 回调与事件循环不在同一处,消息走代理回传。
enum UserEvent {
    Command(Command),
    /// daemon 报来的宠物状态(订阅线程转发进来,见 `pet::ipc`)。
    PetState(PetState),
}

pub(in crate::pet) fn run(paths: &GqyPaths) -> Result<()> {
    // 事件循环的闭包要活到进程结束,所以 paths 得自己持有一份。
    let paths = paths.clone();
    let config = AppConfig::load_or_default(&paths)?;
    let pet = config.display.pet.clone();
    // 配置可能被手改坏:缩放夹在 0.5–2.0,免得窗口大到点不到边、小到看不见。
    let scale = f64::from(pet.scale).clamp(0.5, 2.0);

    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    // 不占 Dock、不抢前台:她是桌面上的一个东西,不是又一个 App。
    event_loop.set_activation_policy(tao::platform::macos::ActivationPolicy::Accessory);
    let proxy = event_loop.create_proxy();

    let saved = state::load(&paths.state_dir);
    let size = pet_window_size(scale);
    // 落点:记过就照记的来;头一次开贴主屏右下角(而不是系统给的正中间——她会挡住
    // 你在看的东西)。位置在**建窗时**就定下来:macOS 上窗口显示之后再挪是不可靠的
    // (实测 set_outer_position 之后读回来还是老的)。
    let start = saved
        .position()
        .map(|(x, y)| PhysicalPosition::new(x, y))
        .or_else(|| bottom_right_position(&event_loop, size));
    let window_builder = WindowBuilder::new()
        .with_title("顾清影")
        .with_inner_size(size)
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top(pet.always_on_top)
        .with_visible_on_all_workspaces(true)
        // 先别显示:透明窗在 WebView 就位前露一下会闪一块白底。
        .with_visible(false);
    #[cfg(target_os = "macos")]
    let window_builder = window_builder.with_has_shadow(false);
    let window_builder = match start {
        Some(position) => window_builder.with_position(position),
        None => window_builder,
    };
    let window = window_builder
        .build(&event_loop)
        .context("建悬浮窗失败(桌面会话是否可用?)")?;

    let ipc_proxy = proxy.clone();
    let webview = WebViewBuilder::new()
        .with_html(page::html(&pet))
        .with_transparent(true)
        .with_background_color((0, 0, 0, 0))
        .with_devtools(cfg!(debug_assertions))
        .with_ipc_handler(move |request: wry::http::Request<String>| {
            match serde_json::from_str::<Command>(request.body()) {
                Ok(command) => {
                    let _ = ipc_proxy.send_event(UserEvent::Command(command));
                }
                // 页面是我们自己编译进去的,解析不了就是版本对不上,得说出来。
                Err(error) => {
                    tracing::warn!(%error, body = %request.body(), "pet: 认不出的页面指令");
                }
            }
        })
        .build(&window)
        .context("起 WebView 失败")?;
    window.set_visible(true);
    // daemon 的状态订阅:她在想 / 在说话 / 闲着。断开它自己重连,这里只接线。
    {
        let state_proxy = proxy.clone();
        ipc::spawn(&paths, move |state| {
            let _ = state_proxy.send_event(UserEvent::PetState(state));
        });
    }
    // 把落点写进日志:窗口跑到屏幕外时,这一行是唯一能对上的线索。
    if let Ok(position) = window.outer_position() {
        let size = window.inner_size();
        tracing::info!(
            x = position.x,
            y = position.y,
            width = size.width,
            height = size.height,
            scale,
            "pet: 悬浮窗已就位"
        );
    }

    let mut position = saved;
    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::Moved(moved) => {
                    position.x = Some(moved.x);
                    position.y = Some(moved.y);
                }
                WindowEvent::CloseRequested => {
                    save_position(&paths.state_dir, &position);
                    *control_flow = ControlFlow::Exit;
                }
                _ => {}
            },
            Event::UserEvent(UserEvent::Command(Command::Drag)) => {
                if let Err(error) = window.drag_window() {
                    tracing::warn!(%error, "pet: 原生拖窗没起来");
                }
            }
            Event::UserEvent(UserEvent::Command(Command::OpenWebui)) => open_webui(&paths),
            Event::UserEvent(UserEvent::Command(Command::SetOnTop { on })) => {
                window.set_always_on_top(on);
                save_pet_config(&paths, |pet| pet.always_on_top = on);
            }
            Event::UserEvent(UserEvent::Command(Command::SetScale { scale })) => {
                let scale = f64::from(scale).clamp(0.5, 2.0);
                window.set_inner_size(pet_window_size(scale));
                save_pet_config(&paths, |pet| pet.scale = scale as f32);
            }
            Event::UserEvent(UserEvent::Command(Command::Ready { width })) => {
                tracing::info!(natural_width = width, "pet: 页面已就绪");
            }
            Event::UserEvent(UserEvent::PetState(state)) => {
                // 表现交给页面(状态点颜色,以后还有表情与口型);这里只转一道。
                let name = pet_state_name(state);
                tracing::info!(state = name, "pet: 状态变化");
                let script = format!("window.gqyPet?.setState(\"{name}\")");
                if let Err(error) = webview.evaluate_script(&script) {
                    tracing::debug!(%error, "pet: 状态没送进页面");
                }
            }
            Event::UserEvent(UserEvent::Command(Command::Close)) => {
                save_position(&paths.state_dir, &position);
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    })
}

/// 退出前把位置落盘:下次开在该在的地方。写不进去只写日志——为这个卡住退出不值。
fn save_position(state_dir: &std::path::Path, position: &state::PetState) {
    if let Err(error) = state::save(state_dir, position) {
        tracing::warn!(%error, "pet: 窗口位置没写进 pet.json");
    }
}

/// 窗口逻辑尺寸:立绘 × 缩放,上方留出动效的余量。
fn pet_window_size(scale: f64) -> LogicalSize<f64> {
    LogicalSize::new(
        page::PORTRAIT_WIDTH * scale,
        page::PORTRAIT_HEIGHT * scale + HEADROOM * scale,
    )
}

/// 状态名:与 `PetState` 的 serde 口径一致(帧里就是这么写的),页面按它选样式。
fn pet_state_name(state: PetState) -> &'static str {
    match state {
        PetState::Idle => "idle",
        PetState::Thinking => "thinking",
        PetState::Speaking => "speaking",
    }
}

/// 头一次开窗的落点:主屏右下角(离屏幕边 16 逻辑像素)。
///
/// 拿的是**建窗之前**的事件循环:窗口还没影的时候它也能枚举显示器,而窗口自己的
/// `current_monitor()` 那时给不出东西。尺寸按该屏的缩放换算成物理像素,与
/// `state/pet.json` 里的口径一致。
fn bottom_right_position(
    event_loop: &EventLoop<UserEvent>,
    size: LogicalSize<f64>,
) -> Option<PhysicalPosition<i32>> {
    let monitor = event_loop
        .primary_monitor()
        .or_else(|| event_loop.available_monitors().next())?;
    let scale = monitor.scale_factor();
    let width = (size.width * scale).round() as i32;
    let height = (size.height * scale).round() as i32;
    let screen = monitor.size();
    let origin = monitor.position();
    let margin = (16.0 * scale).round() as i32;
    let position = PhysicalPosition::new(
        origin.x + screen.width as i32 - width - margin,
        origin.y + screen.height as i32 - height - margin,
    );
    tracing::debug!(
        x = position.x,
        y = position.y,
        ?size,
        "pet: 头一次开,贴右下角"
    );
    Some(position)
}

/// 改一个 `display.pet` 字段并落盘。读-改-写整份配置:daemon 也可能同时在改
/// 别的字段,这种竞态窗口极小,而且配置改了随时能再改,不值得为它上锁。
fn save_pet_config(paths: &GqyPaths, edit: impl FnOnce(&mut crate::config::PetConfig)) {
    let mut config = match AppConfig::load_or_default(paths) {
        Ok(config) => config,
        Err(error) => {
            tracing::warn!(%error, "pet: 读配置失败,这次的改动没落盘");
            return;
        }
    };
    edit(&mut config.display.pet);
    if let Err(error) = config.save(paths) {
        tracing::warn!(%error, "pet: 配置没写进去");
    }
}

/// 双击:问 daemon 要 WebUI 端口,交给系统打开。查端口是异步的,而事件循环里
/// 不能 await——开一个线程去办;办不成就只写日志(窗口还好好的,不影响别的)。
fn open_webui(paths: &GqyPaths) {
    let paths = paths.clone();
    let spawned = std::thread::Builder::new()
        .name("gqy-pet-open-webui".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    tracing::warn!(%error, "pet: 拉不起查端口用的运行时");
                    return;
                }
            };
            let Some(info) = runtime.block_on(crate::ipc::daemon_info(&paths)) else {
                tracing::warn!("pet: daemon 没在跑,没有 WebUI 可开");
                return;
            };
            open_url(&format!("http://127.0.0.1:{}/", info.web_port));
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "pet: 开浏览器的线程起不来");
    }
}

/// 交给系统打开一个 URL。TUI 侧 `cli/repl/tail/screen/select.rs` 里有一份同样
/// 口径的实现,将来该提到低层合并(现在动那边会牵 TUI 的测试)。
pub(in crate::pet) fn open_url(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let (program, args): (&str, Vec<String>) = ("open", vec![url.to_string()]);
    #[cfg(target_os = "windows")]
    let (program, args): (&str, Vec<String>) = (
        "cmd",
        vec!["/C".into(), "start".into(), String::new(), url.to_string()],
    );
    #[cfg(all(unix, not(target_os = "macos")))]
    let (program, args): (&str, Vec<String>) = ("xdg-open", vec![url.to_string()]);

    match std::process::Command::new(program)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            // 打开命令转手给桌面后很快退出,收掉它,免得留僵尸进程。
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(error) => {
            tracing::warn!(%error, url, "pet: 打不开这个链接");
            false
        }
    }
}
