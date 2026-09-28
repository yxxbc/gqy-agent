//! 悬浮窗里不需要窗口就能验的那部分:页面内联、运行时状态读写。
//!
//! 窗口本身(透明、置顶、拖动、IPC)要真跑起来看,不进这里——CI 上没有桌面
//! 会话。

use crate::pet::{page, state};

/// 立绘必须真的内联进页面:漏了占位符就是一块空白窗,而这条只有跑起来才看得见。
#[test]
fn the_page_carries_the_portrait_inline() {
    let html = page::html(&crate::config::PetConfig::default());
    assert!(
        !html.contains("__PORTRAIT_DATA_URI__"),
        "页面里还留着立绘占位符,内联没发生"
    );
    assert!(html.contains("data:image/png;base64,"), "没有内联的立绘");
    // 页面是这个进程唯一能渲染的东西,不能漏了窗口指令的出口。
    assert!(html.contains("window.ipc"), "页面没有 IPC 出口");
    assert!(html.contains("\"drag\""), "页面发不出拖动指令");
}

/// 配置状态要真的注进页面:右键菜单上的勾靠它,漏了就是永远不打勾。
#[test]
fn the_page_gets_the_pet_config_injected() {
    let pet = crate::config::PetConfig {
        scale: 1.4,
        always_on_top: false,
    };
    let html = page::html(&pet);
    assert!(!html.contains("__PET_STATE__"), "状态占位符没被替换");
    assert!(html.contains("\"scale\":1.4"), "缩放没进页面");
    assert!(html.contains("\"on_top\":false"), "置顶状态没进页面");
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
