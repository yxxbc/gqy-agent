export const MAX_CONTENT_CHARS = 20_000;

export const MAX_CUSTOM_ANSWER_CHARS = 4_000;

export const MAX_TOOL_OUTPUT_CHARS = 200_000;

export const MAX_ATTACHMENTS = 12;

export const COMMAND_OUTPUT_PREVIEW_ROWS = 8;

export const NEAR_BOTTOM_PX = 120;

// 常驻任务面板的宽度闸,与 styles.css 里 `.main-stage.is-wide` 的判据一致。
export const STAGE_WIDE_PX = 1360;

// smooth 滚动的兜底:动画期间 scroll 事件由 programmaticScroll 守卫吃掉,
// 万一条数不够(或压根没滚动)也不能让守卫永久卡住。
export const PROGRAMMATIC_SCROLL_MS = 600;

// auto 滚动的兜底:视口已经在底时 scrollTo 不会派发 scroll 事件,守卫没有
// 那条「回执」可吃,得靠超时解除,否则用户下一次滚动的第一条事件会被吞掉。
export const PROGRAMMATIC_SCROLL_AUTO_MS = 150;

export const DEFAULT_BOARD_TITLE = "今天想聊些什么？";

export const DEFAULT_BOARD_SUBTITLE = "从一个问题、计划或此刻的想法开始。";

// 输入框提示跟着人格名走,所以是函数不是常量;与后端
// `web::dto::default_composer_placeholder` 保持同一句话。
export const defaultComposerPlaceholder = (name) => `给 ${name} 发消息`;

export const DEFAULT_STARTER_PROMPTS = ["查询今天的天气", "分析一个问题", "发表情包打个招呼吧", "搜索一张图片"];

// 档位一律用供应商原值(max/high/minimal…),不翻译:译名和文档、和模型
// 实际认的参数值对不上,查起来反而费劲。"没设"这一档没有原值,只好写字。
export const THINKING_VARIANT_DEFAULT_LABEL = "default";

export const EVENT_NAMES = [
  "run.started",
  "turn.started",
  "assistant.delta",
  "reasoning.start",
  "reasoning.reset",
  "reasoning.part_start",
  "reasoning.part_end",
  "reasoning.title",
  "reasoning.delta",
  "tool.started",
  "tool.preparing",
  "tool.progress",
  "tool.output",
  "tool.image",
  "tool.artifact",
  "tool.finished",
  "question.requested",
  "question.answered",
  "question.closed",
  "context.compact_start",
  "context.compact_delta",
  "context.compact_end",
  "context.pop_start",
  "context.pop_end",
  "context.error",
  "queue.added",
  "queue.removed",
  "queue.consumed",
  "generation.superseded",
  "chat.round_usage",
  "run.completed",
  "run.cancelled",
  "run.failed",
  "conversation.reset",
  "conversation.pop",
  "conversation.compacted",
  "session.created",
  "session.renamed",
  "session.deleted",
  "session.current_changed",
  "session.updated",
  "session.reordered",
  "job.started",
  "job.finished",
  "job.acknowledged",
  "job.progress",
  "resync_required"
];

export const RUN_EVENTS = new Set(EVENT_NAMES.filter((name) => !name.startsWith("session.") && !name.startsWith("job.") && !["conversation.reset", "conversation.pop", "resync_required", "queue.added", "queue.removed"].includes(name)));

// 和 REPL 的 `wait_spinner.rs::BRAILLE_FRAMES` 同一组帧。
export const BRAILLE_FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
