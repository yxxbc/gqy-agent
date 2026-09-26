//! 上下文分项:输入框下方的上下文圆环点开后看到的分项占用(2026-09-14,
//! 计划 `docs/plan-is-true/2026-09-14/context-panel.md`)。
//!
//! 只从真实请求的同一份字节里拆:消息走 `chat_messages("", "")`,工具走
//! `ToolRegistry::presented_definitions`(`definitions` / `stub_definitions`
//! 也从它出)。各分项之和必须等于 `context_tokens_estimate()`,测试钉着——另写
//! 一套渲染、或漏归一类消息,那里先报红。
//!
//! 供应商报的实测总数拆不开,只整数给出;实测与估算之差由前端单列成「分词器
//! 差异」,这里不按比例摊进各分项——那是编数字。

use crate::agent::*;
use crate::llm::{ChatContent, ChatContentPart, ChatMessage, ToolDefinition};
use crate::tools::PresentedToolKind;
use serde::Serialize;
use std::collections::HashMap;

/// `tool_report::summary_checkpoint_message` 的外壳,压缩摘要行靠它认。
const CHECKPOINT_PREFIX: &str = "<conversation-checkpoint>";
const SKILL_TOOL: &str = "load_skill";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    System,
    Skills,
    Summary,
    Fossil,
    Messages,
}

/// 各分项的 o200k 估算。字段名就是前端的分项键。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ContextCategories {
    /// system 消息与预设对话。
    pub system: u64,
    /// 完整 schema 发出的工具(full 模式全部、stub 模式的常驻工具)。
    pub tools_full: u64,
    /// stub 模式下懒工具的「真名 + 摘要 + 宽松参数壳」。
    pub tools_stub: u64,
    /// MCP 服务器登记的工具,不论以哪种形态发出。
    pub mcp: u64,
    /// `load_skill` 的结果。
    pub skills: u64,
    /// 压缩摘要行。
    pub summary: u64,
    /// 化石化回放的瞬态尾巴(runtime、联想记忆、提醒等)。
    pub fossil: u64,
    /// 其余历史:用户消息、她的回复、工具调用与结果。
    pub messages: u64,
}

impl ContextCategories {
    pub fn total(&self) -> u64 {
        self.system
            + self.tools_full
            + self.tools_stub
            + self.mcp
            + self.skills
            + self.summary
            + self.fossil
            + self.messages
    }

    fn add(&mut self, category: Category, tokens: u64) {
        let slot = match category {
            Category::System => &mut self.system,
            Category::Skills => &mut self.skills,
            Category::Summary => &mut self.summary,
            Category::Fossil => &mut self.fossil,
            Category::Messages => &mut self.messages,
        };
        *slot += tokens;
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextBreakdown {
    pub categories: ContextCategories,
    /// 各分项之和,等于 `context_tokens_estimate()`。
    pub estimate_tokens: u64,
    /// 上一回合最后一次请求的供应商实测占用;没有锚点时为空。
    pub measured_tokens: Option<u64>,
    /// stub 模式下还没展开的完整契约。不在上下文里。
    pub deferred_tools_tokens: u64,
}

impl Agent {
    pub fn context_breakdown(&self) -> Result<ContextBreakdown> {
        let (messages, _) = self.chat_messages("", "")?;
        let preset_end = 1 + self.preset_dialogs.len() * 2;

        let mut categories = ContextCategories::default();
        let mut tool_names: HashMap<&str, &str> = HashMap::new();
        for (index, message) in messages.iter().enumerate() {
            for call in message.tool_calls.iter().flatten() {
                tool_names.insert(call.id.as_str(), call.function.name.as_str());
            }
            let tokens = overflow::message_tokens(message) as u64;
            let tool_name = message
                .tool_call_id
                .as_deref()
                .and_then(|id| tool_names.get(id).copied());
            let category = if index < preset_end {
                Category::System
            } else if message.transient_context {
                Category::Fossil
            } else if message.role == "tool" && tool_name == Some(SKILL_TOOL) {
                Category::Skills
            } else if message.role == "user" && text_of(message).starts_with(CHECKPOINT_PREFIX) {
                Category::Summary
            } else {
                Category::Messages
            };
            categories.add(category, tokens);
        }

        let mut deferred_tools_tokens = 0;
        if self.tools_enabled {
            let tools = self.tools.lock().unwrap();
            let stub_mode = crate::tools::is_stub_loading_mode(
                &crate::tools::effective_tools_loading_mode(&self.config),
            );
            let mut full = Vec::<ToolDefinition>::new();
            let mut stub = Vec::new();
            let mut mcp = Vec::new();
            for presented in tools.presented_definitions(stub_mode) {
                match presented.kind {
                    PresentedToolKind::Full => full.push(presented.definition),
                    PresentedToolKind::Stub => stub.push(presented.definition),
                    PresentedToolKind::Mcp => mcp.push(presented.definition),
                }
            }
            categories.tools_full = estimate_tool_definition_tokens(&full) as u64;
            categories.tools_stub = estimate_tool_definition_tokens(&stub) as u64;
            categories.mcp = estimate_tool_definition_tokens(&mcp) as u64;
            if stub_mode {
                deferred_tools_tokens =
                    estimate_tool_definition_tokens(&tools.deferred_contract_definitions()) as u64;
            }
        }

        let estimate_tokens = categories.total();
        Ok(ContextBreakdown {
            categories,
            estimate_tokens,
            measured_tokens: self.context_anchor_tokens()?,
            deferred_tools_tokens,
        })
    }
}

fn text_of(message: &ChatMessage) -> &str {
    match &message.content {
        Some(ChatContent::Text(text)) => text,
        Some(ChatContent::Parts(parts)) => parts
            .iter()
            .find_map(|part| match part {
                ChatContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .unwrap_or_default(),
        None => "",
    }
}
