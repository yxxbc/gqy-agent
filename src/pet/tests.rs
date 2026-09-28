//! 悬浮窗里不需要窗口就能验的那部分:内嵌资源、注入脚本、模型清单补全、
//! 运行时状态读写。
//!
//! 窗口本身(透明、置顶、拖动、协议、IPC)要真跑起来看,不进这里——CI 上没有
//! 桌面会话。

use crate::pet::{assets, model, state, window};

/// 页面、脚本、立绘要真的编译进来:页面没引到脚本、脚本没有 IPC 出口,
/// 跑起来都是一块空白窗,而这条只有跑起来才看得见。
#[test]
fn the_page_script_and_portrait_are_embedded() {
    assert!(assets::PAGE.contains("./app.js"), "页面没引到脚本");
    assert!(assets::SCRIPT.contains("window.ipc"), "脚本没有 IPC 出口");
    assert!(assets::SCRIPT.contains("\"drag\""), "脚本发不出拖动指令");
    assert!(
        assets::SCRIPT.contains("\"open_webui\""),
        "脚本发不出打开 WebUI 的指令"
    );
    assert!(!assets::PORTRAIT.is_empty(), "立绘没内联");
    assert!(
        assets::PAGE_URL.starts_with("gqy-pet://"),
        "页面地址不是我们自己的协议"
    );
}

/// 注入脚本要带上模型地址与偏好:漏了页面就不知道去哪拿模型,菜单也永远不打勾。
#[test]
fn the_bootstrap_script_carries_the_model_url_and_config() {
    let script = window::pet_bootstrap(Some("gqy-pet://app/model/x.model3.json"), 1.4, false);
    assert!(script.contains("x.model3.json"), "模型地址没进页面");
    assert!(script.contains("\"scale\":1.4"), "缩放没进页面");
    assert!(script.contains("\"on_top\":false"), "置顶状态没进页面");

    let bare = window::pet_bootstrap(None, 1.0, true);
    assert!(
        bare.contains("\"model_url\":null"),
        "没有模型时应当明确是 null"
    );
}

/// 清单补全:VTS 导出的模型把动作与表情摊在目录里,而 model3.json 的
/// Motions / Expressions 是空的——补不上,模型就是个不会动的立绘。
#[test]
fn motions_and_expressions_are_registered_from_disk() {
    let dir = tempfile::tempdir().expect("temp dir");
    write(
        dir.path(),
        "x.model3.json",
        r#"{"Version":3,"FileReferences":{"Moc":"x.moc3","Textures":["t.png"]}}"#,
    );
    write(dir.path(), "x.moc3", "");
    write(dir.path(), "motions/idle.motion3.json", "{}");
    write(dir.path(), "EXP3/脸红.exp3.json", "{}");

    let loaded = model::load(dir.path()).expect("load");
    assert_eq!(loaded.motions, 1, "动作没被注册");
    assert_eq!(loaded.expressions, 1, "表情没被注册");
    assert!(loaded.manifest.contains("\"Idle\""), "待机组名应当是 Idle");
    assert!(loaded.manifest.contains("脸红"), "表情名应当取文件名");

    // 原文件一个字节都不该动。
    let raw = std::fs::read_to_string(dir.path().join("x.model3.json")).expect("read");
    assert!(!raw.contains("Motions"), "不该改写用户的 model3.json");
}

/// 目录里没有 model3.json、或它指向的 moc3 不在,都要明确失败——猜一个加载更糟。
#[test]
fn a_directory_without_a_manifest_is_rejected() {
    let dir = tempfile::tempdir().expect("temp dir");
    assert!(model::load(dir.path()).is_err(), "空目录应当报错");

    write(dir.path(), "x.model3.json", r#"{"FileReferences":{}}"#);
    assert!(model::load(dir.path()).is_err(), "没有 Moc 字段应当报错");
}

/// 位置存了就要读得回来——重启后「还在那儿」全靠这一对函数。
#[test]
fn the_saved_position_survives_a_round_trip() {
    let dir = tempfile::tempdir().expect("temp dir");
    let saved = state::PetState {
        x: Some(-1200),
        y: Some(64),
    };
    state::save(dir.path(), &saved).expect("save");
    assert_eq!(state::load(dir.path()).position(), Some((-1200, 64)));
}

/// 状态文件坏掉 / 只有一半坐标时,不能拦启动,也不能给出半个位置。
#[test]
fn a_broken_state_file_falls_back_to_defaults() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("pet.json"), b"{ not json").expect("write");
    assert_eq!(state::load(dir.path()).position(), None);

    // 只有 x:当没记过,而不是把窗口丢到 x 轴上某个怪位置。
    let half = state::PetState {
        x: Some(10),
        y: None,
    };
    state::save(dir.path(), &half).expect("save");
    assert_eq!(state::load(dir.path()).position(), None);
}

/// 帧 → 信号:三条链路各认一种帧,认不出的安静丢掉。
#[test]
fn pet_frames_map_to_signals() {
    use crate::ipc::PetState;
    use crate::pet::ipc::{parse_signal, PetSignal};
    use serde_json::json;

    assert!(matches!(
        parse_signal("pet.state", &json!({ "state": "thinking" })),
        Some(PetSignal::State(PetState::Thinking))
    ));
    assert!(matches!(
        parse_signal("pet.mouth", &json!({ "value": 0.42 })),
        Some(PetSignal::Mouth(value)) if (value - 0.42).abs() < 0.001
    ));
    assert!(matches!(
        parse_signal("pet.mood", &json!({ "valence": 0.6, "arousal": 0.7 })),
        Some(PetSignal::Mood { valence, arousal }) if (valence - 0.6).abs() < 0.001 && (arousal - 0.7).abs() < 0.001
    ));
    // daemon 以后加别的帧 / 帧里缺字段,都不该让宠物报错。
    assert!(parse_signal("something.else", &json!({})).is_none());
    assert!(parse_signal("pet.mouth", &json!({})).is_none());
    assert!(parse_signal("pet.state", &json!({ "state": "never-heard-of-it" })).is_none());
}

fn write(root: &std::path::Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, contents).expect("write");
}
