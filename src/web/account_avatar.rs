//! 登录者自己的头像，以及双方头像的取景与尺寸(09-26)。
//!
//! 头像跟账号走:管理员放在属主档案旁边,成员放在自己家目录,和 profile.md
//! 同一个目录。取景(缩放、平移)和对话里的头像尺寸是「我这边怎么看」的显示
//! 偏好,同样按账号存一份 JSON。她的头像取景按人格作用域分开记:换了人格,
//! 头像换了一张图,上一张图的取景不该套上去。

use crate::web::*;
use std::collections::BTreeMap;

const AVATAR_STEM: &str = "account-avatar";
const DISPLAY_FILE: &str = "account-avatar.json";
const IMAGE_EXTENSIONS: [&str; 4] = ["png", "jpg", "webp", "gif"];

const MIN_ZOOM: f64 = 1.0;
const MAX_ZOOM: f64 = 4.0;
/// 平移按头像框边长的百分比记,±50 足够把图的任意一角挪到框中间。
const MAX_OFFSET: f64 = 50.0;
const MIN_SIZE: u32 = 24;
const MAX_SIZE: u32 = 44;
/// 作用域名是人格文件名推出来的,给个上限防止 JSON 被刷大。
const MAX_SCOPES: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub(in crate::web) struct AvatarFrame {
    pub(in crate::web) zoom: f64,
    pub(in crate::web) x: f64,
    pub(in crate::web) y: f64,
}

impl AvatarFrame {
    fn clamped(self) -> Self {
        let clamp = |value: f64, min: f64, max: f64, fallback: f64| {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        };
        Self {
            zoom: clamp(self.zoom, MIN_ZOOM, MAX_ZOOM, MIN_ZOOM),
            x: clamp(self.x, -MAX_OFFSET, MAX_OFFSET, 0.0),
            y: clamp(self.y, -MAX_OFFSET, MAX_OFFSET, 0.0),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct StoredDisplay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user: Option<AvatarFrame>,
    /// 人格作用域 → 她的头像取景。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    assistant: BTreeMap<String, AvatarFrame>,
}

/// 前端的改动请求。字段缺省 = 不改;显式 null = 恢复默认。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::web) struct AvatarDisplayRequest {
    #[serde(default, deserialize_with = "double_option")]
    size: Option<Option<u32>>,
    #[serde(default, deserialize_with = "double_option")]
    user: Option<Option<AvatarFrame>>,
    #[serde(default, deserialize_with = "double_option")]
    assistant: Option<Option<AvatarFrame>>,
}

fn double_option<'de, D, T>(deserializer: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

pub(in crate::web) fn account_dir(paths: &GqyPaths, identity: &WebIdentity) -> Option<PathBuf> {
    profile_file_for(paths, identity)
        .parent()
        .map(FilePath::to_path_buf)
}

fn avatar_file(dir: &FilePath) -> Option<PathBuf> {
    IMAGE_EXTENSIONS
        .iter()
        .map(|ext| dir.join(format!("{AVATAR_STEM}.{ext}")))
        .find(|path| path.is_file())
}

/// 头像地址带上修改时间:换了图浏览器缓存自然失效。没有头像返回 None。
pub(in crate::web) fn account_avatar_url(
    paths: &GqyPaths,
    identity: &WebIdentity,
) -> Option<String> {
    let path = avatar_file(&account_dir(paths, identity)?)?;
    let version = std::fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    Some(format!("/api/account/avatar?v={version}"))
}

/// 她此刻用的是哪个人格:成员的私有人格,否则全局当前人格。
fn assistant_scope(state: &DaemonState, identity: &WebIdentity) -> String {
    if !identity.admin && !identity.username.is_empty() {
        if let Some(persona) = member_persona::active_persona(&state.paths, &identity.username) {
            return format!("member:{}", persona.slug);
        }
    }
    active_persona_scope(state)
}

fn load_display(dir: &FilePath) -> StoredDisplay {
    std::fs::read_to_string(dir.join(DISPLAY_FILE))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn display_json(stored: &StoredDisplay, scope: &str) -> Value {
    json!({
        "size": stored.size,
        "user": stored.user,
        "assistant": stored.assistant.get(scope),
    })
}

/// bootstrap 里账号的那两项:头像地址与当前人格下的显示偏好。
pub(in crate::web) fn account_avatar_bootstrap(
    state: &DaemonState,
    identity: &WebIdentity,
) -> (Option<String>, Value) {
    let scope = assistant_scope(state, identity);
    let display = account_dir(&state.paths, identity)
        .map(|dir| load_display(&dir))
        .unwrap_or_default();
    (
        account_avatar_url(&state.paths, identity),
        display_json(&display, &scope),
    )
}

fn apply_request(stored: &mut StoredDisplay, request: AvatarDisplayRequest, scope: &str) {
    if let Some(size) = request.size {
        stored.size = size.map(|value| value.clamp(MIN_SIZE, MAX_SIZE));
    }
    if let Some(user) = request.user {
        stored.user = user.map(AvatarFrame::clamped);
    }
    if let Some(assistant) = request.assistant {
        match assistant {
            Some(frame) => {
                if stored.assistant.len() < MAX_SCOPES || stored.assistant.contains_key(scope) {
                    stored.assistant.insert(scope.to_string(), frame.clamped());
                }
            }
            None => {
                stored.assistant.remove(scope);
            }
        }
    }
}

fn write_display(dir: &FilePath, stored: &StoredDisplay) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(DISPLAY_FILE);
    let raw = serde_json::to_string_pretty(stored)?;
    std::fs::write(&path, raw).with_context(|| format!("writing {}", path.display()))
}

fn require_account_dir(
    state: &DaemonState,
    identity: &WebIdentity,
) -> std::result::Result<PathBuf, ApiError> {
    account_dir(&state.paths, identity)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "account directory not found"))
}

pub(in crate::web) async fn account_avatar_get(
    State(state): State<DaemonState>,
    headers: HeaderMap,
) -> std::result::Result<Response, ApiError> {
    let identity = require_identity(&headers, &state)?;
    let dir = require_account_dir(&state, &identity)?;
    let path = avatar_file(&dir)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "account avatar not found"))?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| ApiError::new(StatusCode::NOT_FOUND, "account avatar not found"))?;
    let mime = match image::guess_format(&bytes) {
        Ok(image::ImageFormat::Png) => "image/png",
        Ok(image::ImageFormat::Jpeg) => "image/jpeg",
        Ok(image::ImageFormat::Gif) => "image/gif",
        Ok(image::ImageFormat::WebP) => "image/webp",
        _ => {
            return Err(ApiError::new(
                StatusCode::NOT_FOUND,
                "account avatar format is unsupported",
            ))
        }
    };
    let mut response = bytes.into_response();
    let response_headers = response.headers_mut();
    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static(mime));
    // 地址里带着修改时间,内容变了地址就变,可以放心长缓存。
    response_headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    response_headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

/// 请求体就是图片字节(png/jpeg/webp/gif,4 MiB 以内)。
pub(in crate::web) async fn account_avatar_put(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    body: Bytes,
) -> std::result::Result<Response, ApiError> {
    require_mutation(&headers, &state)?;
    let identity = require_identity(&headers, &state)?;
    let dir = require_account_dir(&state, &identity)?;
    std::fs::create_dir_all(&dir).map_err(ApiError::internal)?;
    member_persona::store_image(&dir, AVATAR_STEM, &body)
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(Json(json!({ "avatar_url": account_avatar_url(&state.paths, &identity) })).into_response())
}

pub(in crate::web) async fn account_avatar_delete(
    State(state): State<DaemonState>,
    headers: HeaderMap,
) -> std::result::Result<Response, ApiError> {
    require_mutation(&headers, &state)?;
    let identity = require_identity(&headers, &state)?;
    let dir = require_account_dir(&state, &identity)?;
    member_persona::remove_image(&dir, AVATAR_STEM);
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub(in crate::web) async fn account_avatar_display_put(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Json(request): Json<AvatarDisplayRequest>,
) -> std::result::Result<Response, ApiError> {
    require_mutation(&headers, &state)?;
    let identity = require_identity(&headers, &state)?;
    let dir = require_account_dir(&state, &identity)?;
    let scope = assistant_scope(&state, &identity);
    let mut stored = load_display(&dir);
    apply_request(&mut stored, request, &scope);
    write_display(&dir, &stored).map_err(ApiError::internal)?;
    Ok(Json(json!({ "avatar_display": display_json(&stored, &scope) })).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(raw: &str) -> AvatarDisplayRequest {
        serde_json::from_str(raw).unwrap()
    }

    #[test]
    fn frames_and_size_are_clamped() {
        let mut stored = StoredDisplay::default();
        apply_request(
            &mut stored,
            request(
                r#"{"size":99,"user":{"zoom":9,"x":-80,"y":12.5},"assistant":{"zoom":0.2,"x":3,"y":70}}"#,
            ),
            "gqy",
        );
        assert_eq!(stored.size, Some(MAX_SIZE));
        assert_eq!(
            stored.user,
            Some(AvatarFrame {
                zoom: MAX_ZOOM,
                x: -MAX_OFFSET,
                y: 12.5
            })
        );
        assert_eq!(
            stored.assistant.get("gqy"),
            Some(&AvatarFrame {
                zoom: MIN_ZOOM,
                x: 3.0,
                y: MAX_OFFSET
            })
        );
    }

    #[test]
    fn missing_fields_keep_and_null_resets() {
        let mut stored = StoredDisplay {
            size: Some(32),
            user: Some(AvatarFrame {
                zoom: 2.0,
                x: 1.0,
                y: 1.0,
            }),
            assistant: BTreeMap::from([(
                "gqy".to_string(),
                AvatarFrame {
                    zoom: 1.5,
                    x: 0.0,
                    y: 0.0,
                },
            )]),
        };
        apply_request(&mut stored, request(r#"{"user":null}"#), "gqy");
        assert_eq!(stored.size, Some(32));
        assert_eq!(stored.user, None);
        assert!(stored.assistant.contains_key("gqy"));
        apply_request(&mut stored, request(r#"{"assistant":null}"#), "gqy");
        assert!(stored.assistant.is_empty());
    }

    #[test]
    fn assistant_frames_are_kept_per_persona_scope() {
        let mut stored = StoredDisplay::default();
        apply_request(
            &mut stored,
            request(r#"{"assistant":{"zoom":2,"x":0,"y":0}}"#),
            "gqy",
        );
        apply_request(
            &mut stored,
            request(r#"{"assistant":{"zoom":3,"x":0,"y":0}}"#),
            "member:cat",
        );
        let json = display_json(&stored, "gqy");
        assert_eq!(json["assistant"]["zoom"], json!(2.0));
        assert!(display_json(&stored, "other")["assistant"].is_null());
    }
}
