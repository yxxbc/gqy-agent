//! 渲染用的三个第三方 JS:获取与缓存。
//!
//! **为什么不进仓库**:Cubism Core 是 Live2D 的专有运行时,SDK 许可管的是「嵌进
//! 你的应用里分发」,把原始 SDK 文件塞进源码仓库不属于那一档;pixi 与
//! pixi-live2d-display 是 MIT,但既然另两个得下载,就一起走同一条路,逻辑只有一份。
//!
//! 落在 `state_dir/pet/vendor/`,下过一次就不再下。页面经自定义协议
//! (`gqy-pet://app/vendor/…`)读它们——WebView 不让页面直接开本地文件。
//!
//! 下载失败**不拦启动**:回退到静态立绘,原因写进日志(见 `window.rs`)。

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 一个待下载的运行时文件。版本钉死:升级要连同「模型还能不能加载」一起验。
pub(in crate::pet) struct VendorFile {
    /// 落盘的文件名,也是页面里引用的名字。
    pub(in crate::pet) name: &'static str,
    pub(in crate::pet) url: &'static str,
    /// 日志里说清楚这是什么、什么许可。
    pub(in crate::pet) note: &'static str,
}

pub(in crate::pet) const FILES: &[VendorFile] = &[
    VendorFile {
        name: "live2dcubismcore.min.js",
        url: "https://cubism.live2d.com/sdk-web/cubismcore/live2dcubismcore.min.js",
        note: "Live2D Cubism Core (proprietary; Live2D SDK terms apply)",
    },
    VendorFile {
        name: "pixi.min.js",
        url: "https://cdn.jsdelivr.net/npm/pixi.js@7.4.2/dist/pixi.min.js",
        note: "pixi.js 7.4.2 (MIT)",
    },
    VendorFile {
        name: "cubism4.min.js",
        url: "https://cdn.jsdelivr.net/npm/pixi-live2d-display@0.4.0/dist/cubism4.min.js",
        note: "pixi-live2d-display 0.4.0 cubism4 build (MIT)",
    },
];

/// 少到这个数就认为文件是坏的/半截的:三个文件都远大于它。
const MIN_BYTES: u64 = 8 * 1024;

/// 确保三个文件都在,返回目录。缺哪个下哪个(已经有的不重复下载)。
pub(in crate::pet) fn ensure(state_dir: &Path) -> Result<PathBuf> {
    let directory = state_dir.join("pet").join("vendor");
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("建运行时目录失败: {}", directory.display()))?;

    let missing: Vec<&VendorFile> = FILES
        .iter()
        .filter(|file| !is_usable(&directory.join(file.name)))
        .collect();
    if missing.is_empty() {
        return Ok(directory);
    }

    // 20 秒够慢网络下完单个文件;三个文件合起来一般不到两秒。
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .context("建 HTTP 客户端失败")?;
    for file in missing {
        let target = directory.join(file.name);
        tracing::info!(file = file.name, "pet: 正在下载渲染运行时({})", file.note);
        let bytes = client
            .get(file.url)
            .send()
            .with_context(|| format!("下载 {} 失败(网络?)", file.name))?
            .error_for_status()
            .with_context(|| format!("{} 的地址返回了错误状态", file.name))?
            .bytes()
            .with_context(|| format!("读 {} 的响应体失败", file.name))?;
        std::fs::write(&target, &bytes).with_context(|| format!("写 {} 失败", target.display()))?;
        tracing::info!(file = file.name, bytes = bytes.len(), "pet: 运行时已就位");
    }
    Ok(directory)
}

/// 文件在、且大小说得过去。没有 hash 可比:官方 CDN 不发稳定的摘要,而这份缓存在
/// 本机——真坏了会在页面加载时报错,日志里看得到。
fn is_usable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() >= MIN_BYTES)
        .unwrap_or(false)
}
