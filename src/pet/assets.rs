//! 悬浮窗的资源与自定义协议。
//!
//! 页面、脚本、立绘都编译进二进制;模型与下载来的运行时在磁盘上,由
//! `gqy-pet://app/…` 协议供给页面——WebView 不让页面直接开本地文件,自定义协议
//! 是 wry 给的正路。三类资源**同一个 origin**(host 固定 `app`,用路径前缀分段),
//! 省掉读模型文件时的跨源折腾:
//!
//! - `gqy-pet://app/page/index.html`   页面与脚本
//! - `gqy-pet://app/vendor/<文件>`     下载来的运行时(见 [`super::vendor`])
//! - `gqy-pet://app/model/<相对路径>`  模型目录里的文件(清单已改写,见 [`super::model`])

use crate::pet::{model::Model, vendor};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use wry::http::{header, Request, Response, StatusCode};

pub(in crate::pet) const PAGE: &str = include_str!("web/index.html");
pub(in crate::pet) const SCRIPT: &str = include_str!("web/app.js");
pub(in crate::pet) const PORTRAIT: &[u8] = include_bytes!("../../assets/mascot/portrait.png");

/// 立绘原始尺寸(px)。没有模型时窗口按它算大小。
pub(in crate::pet) const PORTRAIT_WIDTH: f64 = 256.0;
pub(in crate::pet) const PORTRAIT_HEIGHT: f64 = 261.0;

/// 窗口从这里加载页面。
pub(in crate::pet) const PAGE_URL: &str = "gqy-pet://app/page/index.html";

/// 喂给协议的东西:内嵌资源是常量,磁盘上的模型与运行时在这里。
pub(in crate::pet) struct Assets {
    pub(in crate::pet) model: Option<Model>,
    pub(in crate::pet) vendor_dir: PathBuf,
}

impl Assets {
    /// 处理一次 `gqy-pet://` 请求。认不出的路径给 404——页面是我们自己写的,
    /// 真 404 了说明代码里 URL 拼错了,越早看见越好。
    pub(in crate::pet) fn serve(&self, request: &Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
        let path = request.uri().path();
        let decoded = urlencoding::decode(path.trim_start_matches('/'))
            .map(|value| value.into_owned())
            .unwrap_or_else(|_| path.trim_start_matches('/').to_string());
        match decoded.as_str() {
            "page/" | "page/index.html" | "" => {
                return body(PAGE.as_bytes().to_vec(), "text/html; charset=utf-8")
            }
            "page/app.js" => {
                return body(SCRIPT.as_bytes().to_vec(), "text/javascript; charset=utf-8")
            }
            "page/portrait.png" => return body(PORTRAIT.to_vec(), "image/png"),
            _ => {}
        }
        if let Some(name) = decoded.strip_prefix("vendor/") {
            // 只给清单里的名字:这个前缀背后是磁盘路径,不能由页面拼。
            if vendor::FILES.iter().any(|file| file.name == name) {
                return match std::fs::read(self.vendor_dir.join(name)) {
                    Ok(bytes) => body(bytes, "text/javascript; charset=utf-8"),
                    Err(error) => {
                        tracing::warn!(%error, file = name, "pet: 运行时文件读不出来");
                        not_found()
                    }
                };
            }
        }
        if let Some(relative) = decoded.strip_prefix("model/") {
            return self.serve_model(relative);
        }
        not_found()
    }

    /// 模型目录里的文件。`model3.json` 给的是改写过的清单,别的照原样读。
    fn serve_model(&self, relative: &str) -> Response<Cow<'static, [u8]>> {
        let Some(model) = self.model.as_ref() else {
            return not_found();
        };
        if relative == model.manifest_name {
            return body(model.manifest.clone().into_bytes(), "application/json");
        }
        let Some(path) = safe_join(&model.dir, relative) else {
            tracing::warn!(relative, "pet: 模型资源请求越出了模型目录");
            return not_found();
        };
        match std::fs::read(&path) {
            Ok(bytes) => body(bytes, mime_for(&path)),
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "pet: 模型文件读不出来");
                not_found()
            }
        }
    }
}

/// 拼路径并确认没跑出根目录(模型清单里的名字来自磁盘,`..` 不该被放行)。
fn safe_join(root: &Path, relative: &str) -> Option<PathBuf> {
    if relative.contains("..") {
        return None;
    }
    let path = root.join(relative);
    let root = root.canonicalize().ok()?;
    let path = path.canonicalize().ok()?;
    path.starts_with(root).then_some(path)
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("png") => "image/png",
        Some("json") | Some("moc3") => "application/json",
        Some("motion3") => "application/json",
        _ => "application/octet-stream",
    }
}

fn body(bytes: Vec<u8>, mime: &str) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        // 本地单页应用:禁止缓存能省掉一堆「改了没生效」的疑案。
        .header(header::CACHE_CONTROL, "no-store")
        .body(Cow::Owned(bytes))
        .expect("静态响应构造失败")
}

fn not_found() -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Cow::Borrowed(b"not found".as_slice()))
        .expect("404 响应构造失败")
}
