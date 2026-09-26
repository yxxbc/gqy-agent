use crate::agent::{
    archive_and_delete_visible_turns, Agent, AgentEvent, AgentMode, AgentTurnControl,
};
use crate::args::WebArgs;
use crate::config::{ActiveProviderModelConfig, AppConfig, PromptAudience, ProviderConfig};
use crate::i18n::text as t;
use crate::ipc::{
    self, Command as IpcCommand, Frame as IpcFrame, ImageAttachment, Request as IpcRequest,
};
use crate::llm::{
    thinking_variant_options_for_model, ChatResult, ChatStreamKind, OpenAiCompatibleClient,
    ThinkingVariantOptions, ThinkingVariantPreferences, Usage,
};
use crate::memory::{
    MemoryAccess, MemoryOrganizer, MemoryOrganizerHandle, MemoryOrigin, MemoryStore,
};
use crate::paths::GqyPaths;
use crate::question::{self, QuestionAnswers};
// daemon 运行时的共享状态已下沉到 runtime：web 只是它的消费者之一，IPC 与
// 平台适配是另外两个。放在 web 里会让平台层反过来依赖 HTTP 服务。
mod account_avatar;
mod accounts_api;
mod actor;
// build.rs 也 include! 这份规则；发布版里只有 build.rs 用得上它，运行时那条路径只在 debug 构建存在。
#[cfg_attr(not(debug_assertions), allow(dead_code))]
mod asset_rules;
mod assets;
mod attachments;
mod bridge_progress;
mod bridge_question;
mod commands_api;
mod config_api;
mod connectors_api;
mod context_panel;
mod dashboards;
#[cfg(debug_assertions)]
mod dev_assets;
mod dto;
mod embedded;
mod event_map;
mod extensions_api;
mod goal_driver;
mod job_access;
mod link_preview;
mod member_persona;
mod ownership;
mod persona;
mod prompt_files;
mod providers_api;
mod qq_history;
mod sandbox_scope;
mod security;
mod selection_menu;
mod server;
mod session_cmds;
mod sessions;
mod shared_files;
#[cfg(test)]
pub(crate) mod tests;
mod themes_api;
mod today;
mod tty;
mod turns;
mod ui_prefs;
mod voice_api;
pub(crate) mod voice_bridge;
pub(crate) mod voice_tts;
// 叫 ipc_server 而不是 ipc：`web::ipc` 会把 `crate::ipc` 遮住，本文件里几十处
// `ipc::send` 会突然解析到子模块上——编译期就报，但报错信息（找不到 send）
// 离真正的原因很远。
mod ipc_server;
// 地图瓦片代理:CSP 是 img-src 'self',瓦片只能由 daemon 代取。
mod map_api;

use account_avatar::*;
use accounts_api::*;
use actor::*;
use assets::*;
use attachments::*;
use bridge_progress::*;
use bridge_question::*;
use commands_api::*;
use config_api::*;
use connectors_api::*;
use context_panel::*;
use dashboards::affection::*;
use dashboards::album::*;
use dashboards::kb::*;
use dashboards::ledger::*;
use dashboards::memes::*;
use dashboards::memory::*;
use dashboards::qq::*;
use dashboards::scripts::*;
use dashboards::sponsor::*;
use dto::*;
use embedded::*;
use event_map::*;
use extensions_api::*;
use goal_driver::*;
use ipc_server::*;
use job_access::*;
use map_api::*;
use ownership::*;
use persona::*;
pub(crate) use persona::{composer_placeholder, persona_display_name};
use prompt_files::*;
use providers_api::*;
use qq_history::*;
use sandbox_scope::*;
use security::*;
use selection_menu::*;
pub(crate) use server::run;
use server::*;
use session_cmds::*;
use sessions::*;
use shared_files::*;
use themes_api::*;
use today::*;
use tty::*;
use turns::*;
use ui_prefs::*;
use voice_api::*;

use crate::runtime::{
    cold_context, enqueue_turn_update, finish_run, random_id, random_token, release_admin,
    reset_platform_persona_state, safe_error_message, startup_context, validate_content,
    ActorCommand, AdminFailure, AnswerFailure, ApiError, ContextSnapshot, DaemonState, EventHub,
    EventRecord, IpcRunGuard, LoginFailure, ManagerState, PlatformPersonaResetError,
    PromptDocument, PromptDocuments, QuestionBroker, RedoWebPrompt, RunInfo, RunOperation,
    SafeQueuedPrompt, SafeUserAttachment, StoreRegistry, ThinkingVariantUpdate, TurnEngineState,
    TurnResourceCache, TurnUpdateMode, TurnUpdateReceipt, TurnUpdateRequest, WebAuth, WebIdentity,
};
use crate::state::{
    ArtifactAsset, ImageAsset, PlatformPluginScopeKey, QueuedPrompt, StateStore, Turn,
    TurnFollowup, TurnStatus, UsageSnapshot, UserAttachment, USER_ATTACHMENT_KIND_FILE,
    USER_ATTACHMENT_KIND_IMAGE, USER_ATTACHMENT_KIND_TEXT,
};
use crate::tools::build_tool_registry;
use crate::tools::{self, CommandOutputStream};
use anyhow::{bail, Context, Result};
use axum::body::Bytes;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Query, State};
use axum::http::header::{
    ACCEPT_ENCODING, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
    ACCESS_CONTROL_MAX_AGE, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_ENCODING, CONTENT_LENGTH,
    CONTENT_SECURITY_POLICY, CONTENT_TYPE, COOKIE, HOST, ORIGIN, REFERRER_POLICY, RETRY_AFTER,
    SET_COOKIE, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use base64::Engine;
use futures_util::stream::{self, Stream};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::Infallible;
use std::future::IntoFuture;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path as FilePath, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::{broadcast, mpsc, oneshot, Semaphore};
use tokio::task::JoinHandle as TokioJoinHandle;

use crate::platforms::{self, PlatformRuntime};

const JSON_BODY_LIMIT: usize = 4 * 1024 * 1024;
const PERSONA_ASSET_LIMIT: usize = 8 * 1024 * 1024;
const MAX_PROMPT_DOCUMENT_CHARS: usize = 200_000;
const MAX_PROMPT_DOCUMENTS: usize = 128;

const INDEX_HTML: &str = include_str!("../../web/index.html");
const FENCE_FRAME_HTML: &str = include_str!("../../web/fence-frame.html");
// 其余 web/ 文件由 build.rs 扫描成表，见 embedded.rs。
// KaTeX 0.18.4(vendored):公式渲染;字体只带 woff2(css 里 woff2 列首,
// 现代浏览器不会去请求 woff/ttf 回退项)。
const KATEX_JS: &str = include_str!("../../web/vendor/katex/katex.min.js");
// PrismJS 1.29.0(vendored,MIT):core + 18 门常用语言,47KB。头部注释里写了
// 拼装顺序,换版本照那个顺序重拼即可。
const PRISM_JS: &str = include_str!("../../web/vendor/prism/prism.min.js");
const KATEX_CSS: &str = include_str!("../../web/vendor/katex/katex.min.css");
// Apache ECharts 6.1.0(vendored,Apache-2.0):artifact 里画图表用的。
// **存的是 gzip 后的字节**(1096KB → 359KB),响应直接带 Content-Encoding: gzip
// 发出去,服务端不解压。更新照做:
//   curl -sL https://cdn.jsdelivr.net/npm/echarts@<版本>/dist/echarts.min.js \
//     | gzip -9 -n > web/vendor/echarts/echarts.min.js.gz
// `-n` 不能少——带上文件名和时间戳的话每次压出来的字节都不一样,构建就不可复现了。
const ECHARTS_JS_GZ: &[u8] = include_bytes!("../../web/vendor/echarts/echarts.min.js.gz");
// Mermaid 12.0.0(vendored,MIT):聊天正文 ```mermaid 围栏在沙箱 iframe 里画图用的
// (web/fencepreview.js)。同样存 gzip(5.6MB → 1.6MB),更新照 echarts 那条命令换包名:
//   curl -sL https://cdn.jsdelivr.net/npm/mermaid@<版本>/dist/mermaid.min.js \
//     | gzip -9 -n > web/vendor/mermaid/mermaid.min.js.gz
const MERMAID_JS_GZ: &[u8] = include_bytes!("../../web/vendor/mermaid/mermaid.min.js.gz");
static KATEX_FONTS: &[(&str, &[u8])] = &[
    (
        "KaTeX_AMS-Regular.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_AMS-Regular.woff2"),
    ),
    (
        "KaTeX_Caligraphic-Bold.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Caligraphic-Bold.woff2"),
    ),
    (
        "KaTeX_Caligraphic-Regular.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Caligraphic-Regular.woff2"),
    ),
    (
        "KaTeX_Fraktur-Bold.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Fraktur-Bold.woff2"),
    ),
    (
        "KaTeX_Fraktur-Regular.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Fraktur-Regular.woff2"),
    ),
    (
        "KaTeX_Main-Bold.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Main-Bold.woff2"),
    ),
    (
        "KaTeX_Main-BoldItalic.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Main-BoldItalic.woff2"),
    ),
    (
        "KaTeX_Main-Italic.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Main-Italic.woff2"),
    ),
    (
        "KaTeX_Main-Regular.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Main-Regular.woff2"),
    ),
    (
        "KaTeX_Math-BoldItalic.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Math-BoldItalic.woff2"),
    ),
    (
        "KaTeX_Math-Italic.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Math-Italic.woff2"),
    ),
    (
        "KaTeX_SansSerif-Bold.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_SansSerif-Bold.woff2"),
    ),
    (
        "KaTeX_SansSerif-Italic.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_SansSerif-Italic.woff2"),
    ),
    (
        "KaTeX_SansSerif-Regular.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_SansSerif-Regular.woff2"),
    ),
    (
        "KaTeX_Script-Regular.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Script-Regular.woff2"),
    ),
    (
        "KaTeX_Size1-Regular.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Size1-Regular.woff2"),
    ),
    (
        "KaTeX_Size2-Regular.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Size2-Regular.woff2"),
    ),
    (
        "KaTeX_Size3-Regular.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Size3-Regular.woff2"),
    ),
    (
        "KaTeX_Size4-Regular.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Size4-Regular.woff2"),
    ),
    (
        "KaTeX_Typewriter-Regular.woff2",
        include_bytes!("../../web/vendor/katex/fonts/KaTeX_Typewriter-Regular.woff2"),
    ),
];

impl From<QueuedPrompt> for SafeQueuedPrompt {
    fn from(prompt: QueuedPrompt) -> Self {
        Self {
            id: prompt.prompt_id,
            content: prompt.display_content,
            submitted_at: prompt.submitted_at,
            attachments: prompt
                .uploaded_attachments
                .into_iter()
                .map(SafeUserAttachment::from)
                .collect(),
        }
    }
}

impl From<UserAttachment> for SafeUserAttachment {
    fn from(attachment: UserAttachment) -> Self {
        Self {
            url: format!("/api/attachments/{}", attachment.attachment_id),
            id: attachment.attachment_id,
            name: attachment.file_name,
            mime: attachment.mime,
            kind: attachment.kind,
            size: attachment.size_bytes,
            width: attachment.width,
            height: attachment.height,
        }
    }
}

// ── spawn_actor ──

impl DaemonState {
    pub(crate) fn for_test_with_actor(
        paths: GqyPaths,
        web_port: u16,
    ) -> Result<(Self, std::thread::JoinHandle<Result<()>>)> {
        let state_store = StateStore::new(&paths)?;
        let config = AppConfig::default();
        let context = cold_context(&config, &paths, &state_store)?;
        let manager = Arc::new(Mutex::new(ManagerState {
            config: config.clone(),
            active_runs: HashMap::new(),
            admin_busy: false,
            admin_session: None,
            context,
            persona_session_ids: HashMap::new(),
            runs_changed: Arc::new(tokio::sync::Notify::new()),
        }));
        let events = EventHub::new();
        let questions = QuestionBroker::new();
        let turn_engine = TurnEngineState::default();
        let stores = StoreRegistry::new(state_store.clone(), paths.clone());
        let (actor_tx, actor_join) = spawn_actor(
            config,
            paths.clone(),
            state_store.clone(),
            stores.clone(),
            manager.clone(),
            events.clone(),
            questions.clone(),
            turn_engine.clone(),
            None,
        )?;
        let (shutdown_tx, _shutdown_rx) = broadcast::channel(1);
        Ok((
            Self {
                auth: WebAuth::new(None),
                boot_id: Arc::from("boot-test"),
                web_port,
                web_public: false,
                web_bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
                paths,
                manager,
                stores,
                state_store,
                events,
                questions,
                actor_tx,
                shutdown_tx,
                turn_engine,
                platforms: PlatformRuntime::new()?,
            },
            actor_join,
        ))
    }
}
