//! 平台中立的回合机器：会话解析与限流（scheduling）、回合上下文与投递幂等闸
//! （turn_context）、回合驱动（turn_run）、回复整形（reply）、活动与在途记录、
//! 平台指令、平台工具面。QQ（onebot/）、iMessage 与以后的平台都复用这里，
//! 不各写一份。
//!
//! 目录名不用 `core`：会遮蔽标准库的 `core` crate（AGENTS.md §6.3 的「模块名遮蔽」）。
//! 各文件原来写的 `use crate::platforms::*` / `use super::…` 照旧可用：
//! 本模块把父模块的名字原样引进来，父模块再把这里的名字原样导出去。

use super::*;

pub(super) mod access_control;
pub(super) mod activity;
pub(super) mod assets;
pub(crate) mod commands;
pub(crate) mod file_reader;
pub(super) mod inflight;
pub(super) mod live_turns;
pub(super) mod logging;
pub(super) mod reply;
pub(super) mod scheduling;
pub(super) mod tool;
pub(super) mod tool_context;
pub(super) mod turn_context;
pub(super) mod turn_order;
pub(super) mod turn_run;
pub(crate) use activity::*;
pub(crate) use logging::*;
pub(crate) use reply::*;
pub(crate) use scheduling::*;
pub(crate) use turn_context::*;
pub(crate) use turn_order::*;
pub(crate) use turn_run::*;
