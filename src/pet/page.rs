//! 悬浮窗的页面:一个 HTML + 一张立绘,都编译进二进制。
//!
//! 立绘以 data URI 内嵌进页面。v1 只有一张图,不值得为它搭一套自定义协议
//! (`with_custom_protocol`);换 Cubism 时模型是多文件、几十 MB 的资产,
//! 那时再改,见 `docs/design/2026-09-28-desktop-pet.md` §5.3。

use crate::config::PetConfig;
use base64::Engine as _;

const PAGE: &str = include_str!("web/index.html");
const PORTRAIT: &[u8] = include_bytes!("../../assets/mascot/portrait.png");

/// 立绘原始尺寸(px)。窗口大小按配置的缩放系数乘它算出来。
pub(in crate::pet) const PORTRAIT_WIDTH: f64 = 256.0;
pub(in crate::pet) const PORTRAIT_HEIGHT: f64 = 261.0;

/// 页面源码:立绘换成 data URI,配置状态注入成 `__PET_STATE__`——
/// 右键菜单上的勾(置顶、缩放)要看得到当前值,不能靠猜。
pub(in crate::pet) fn html(pet: &PetConfig) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(PORTRAIT);
    let state = serde_json::json!({
        "on_top": pet.always_on_top,
        // 一位小数:配置里可能留着 0.7000000000000001 这种累加残留,
        // 而 f32 直接序列化也会带出精度伪影(1.4 → 1.399999976158142)。
        "scale": (f64::from(pet.scale) * 10.0).round() / 10.0,
    });
    PAGE.replace(
        "__PORTRAIT_DATA_URI__",
        &format!("data:image/png;base64,{encoded}"),
    )
    .replace("__PET_STATE__", &state.to_string())
}
