//! `github` 工具(09-15):commit、提 PR / issue、评论,署名由代码负责。
//!
//! - 身份([`identity`]):缺省宿主身份。`as_bot=true`(用户明确要求时)才用
//!   `gqy github login` 登录的 bot 账号,凭据隔离在 `<GQY_HOME>/github/`。
//! - 署名([`attribution`]):用户固定是 author,顾清影 以 Co-Authored-By 挂尾
//!   (09-15 用户拍板),名字里带本回合的模型与上下文窗口。

mod actions;
mod attribution;
mod identity;
#[cfg(test)]
mod tests;

pub(crate) use identity::{BotAccount, BotHome};

use super::{ToolRegistry, ToolSpec};
use crate::config::{AppConfig, GithubToolConfig};
use crate::paths::GqyPaths;
use crate::tools::workspace::TurnModel;
use std::sync::Arc;

pub(super) struct GithubContext {
    home: BotHome,
    config: GithubToolConfig,
    /// 回合外(工具桥、子代理)读不到本回合 task-local 时的兜底:注册时的主模型。
    fallback_model: Option<TurnModel>,
}

impl GithubContext {
    fn new(config: &AppConfig, paths: &GqyPaths) -> Self {
        let fallback_model = config
            .active_provider_model_choices()
            .into_iter()
            .next()
            .map(|choice| TurnModel {
                model: choice.model,
                context_window: config.active_context_window().ok().flatten(),
            });
        Self {
            home: BotHome::new(paths),
            config: config.tools.github.clone(),
            fallback_model,
        }
    }

    fn co_author(&self) -> attribution::CoAuthor {
        let model =
            crate::tools::workspace::current_turn_model().or_else(|| self.fallback_model.clone());
        attribution::CoAuthor::resolve(&self.config, self.home.account().as_ref(), model.as_ref())
    }
}

pub(super) fn register(registry: &mut ToolRegistry, config: &AppConfig, paths: &GqyPaths) {
    let context = Arc::new(GithubContext::new(config, paths));
    registry.register(
        ToolSpec::new(
            "github",
            "Commit, open PRs and issues, and comment on GitHub.",
            serde_json::json!({ "type": "object" }),
            move |args| {
                let context = context.clone();
                async move { actions::run(&context, args).await }
            },
        )
        .writes(),
    );
}
