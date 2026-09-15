//! 内置插件目录:插件 id 的唯一真相源。
//!
//! 以前 id、引导显示名、引导里给不给开关分散在三张手写清单里(`PLUGIN_IDS`、
//! `plugin_label` 的 match、`TOGGLE_PLUGINS`),加插件漏改一张不报错——
//! `plugin_label` 兜底返回空串,album/map/express 就这样一直没有名字。现在一行
//! 写全,`PLUGIN_IDS` 与 `TOGGLE_PLUGINS` 编译期从这张表派生。
//!
//! 工具侧的注册单元在 `tools::compose`,与这张表的一一对应由那边的测试钉着
//! (config 层不能反过来依赖 tools)。

/// 一个内置插件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginInfo {
    /// persona.toml 里写的名字。
    pub id: &'static str,
    /// 引导与 WebUI 里的显示名。
    pub name: &'static str,
    /// 一句话说明。
    pub hint: &'static str,
    /// 引导「自选功能」里给不给开关;不给的一律常开、不摆出来。
    pub toggle: bool,
}

const fn plugin(id: &'static str, name: &'static str, hint: &'static str) -> PluginInfo {
    PluginInfo {
        id,
        name,
        hint,
        toggle: false,
    }
}

const fn toggle(id: &'static str, name: &'static str, hint: &'static str) -> PluginInfo {
    PluginInfo {
        id,
        name,
        hint,
        toggle: true,
    }
}

/// 顺序即 `PLUGIN_IDS` 的顺序,也是引导写回 `plugins.enabled` 白名单的顺序——
/// 存量 persona.toml 按它落盘,别随手重排。
pub const PLUGINS: &[PluginInfo] = &[
    plugin("files", "文件", "读写工作区文件"),
    plugin("album", "图库", "存图、按名字找图发图"),
    plugin("usage_query", "用量查询", "对话里问用了多少 token"),
    toggle("alarm", "闹钟", "定时提醒"),
    toggle("exchange_rate", "汇率", "货币换算"),
    plugin("map", "地图", "地名与坐标互查"),
    plugin("express", "快递查询", "按单号查物流"),
    toggle("archlinux", "Arch Linux", "AUR 查询与审查安装、Arch 新闻"),
    toggle("api_quota", "API 额度", "查供应商余额"),
    plugin("print_image", "视觉分析", "看图片和截图"),
    toggle("memes", "表情包", "用表情包回复"),
    plugin("platform_outreach", "外发", "从对话里给通讯平台发消息"),
    plugin("web_images", "搜图", "网络找图"),
    toggle("image_generation", "生图", "AI 画图"),
    plugin("knowledge_base", "知识库", "自己的资料库,对话里能查"),
    toggle("ledger", "记账", "记账本"),
    plugin("scripts", "脚本工具", "逐个勾选"),
    // MCP 与脚本同级:插件闸之上还能按服务器 id 逐个勾(`plugins.mcp`)。
    plugin("mcp", "MCP", "外接 MCP 服务器的工具"),
];

const PLUGIN_COUNT: usize = PLUGINS.len();

const TOGGLE_COUNT: usize = {
    let mut count = 0;
    let mut index = 0;
    while index < PLUGIN_COUNT {
        if PLUGINS[index].toggle {
            count += 1;
        }
        index += 1;
    }
    count
};

const PLUGIN_ID_ARRAY: [&str; PLUGIN_COUNT] = {
    let mut ids = [""; PLUGIN_COUNT];
    let mut index = 0;
    while index < PLUGIN_COUNT {
        ids[index] = PLUGINS[index].id;
        index += 1;
    }
    ids
};

const TOGGLE_ID_ARRAY: [&str; TOGGLE_COUNT] = {
    let mut ids = [""; TOGGLE_COUNT];
    let mut index = 0;
    let mut next = 0;
    while index < PLUGIN_COUNT {
        if PLUGINS[index].toggle {
            ids[next] = PLUGINS[index].id;
            next += 1;
        }
        index += 1;
    }
    ids
};

/// 全部插件 id,顺序同 [`PLUGINS`]。
pub const PLUGIN_IDS: &[&str] = &PLUGIN_ID_ARRAY;

/// 引导里给开关的插件 id,顺序同 [`PLUGINS`]。
pub const TOGGLE_PLUGINS: &[&str] = &TOGGLE_ID_ARRAY;

/// 按 id 查插件。
pub fn plugin_info(id: &str) -> Option<&'static PluginInfo> {
    PLUGINS.iter().find(|plugin| plugin.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn plugin_ids_are_unique_and_every_plugin_is_labelled() {
        let unique: BTreeSet<&str> = PLUGIN_IDS.iter().copied().collect();
        assert_eq!(unique.len(), PLUGIN_IDS.len(), "duplicate plugin id");
        for plugin in PLUGINS {
            assert!(!plugin.name.trim().is_empty(), "{} has no name", plugin.id);
            assert!(!plugin.hint.trim().is_empty(), "{} has no hint", plugin.id);
        }
    }

    #[test]
    fn derived_lists_follow_the_table() {
        assert_eq!(
            PLUGIN_IDS,
            PLUGINS.iter().map(|plugin| plugin.id).collect::<Vec<_>>()
        );
        assert_eq!(
            TOGGLE_PLUGINS,
            [
                "alarm",
                "exchange_rate",
                "archlinux",
                "api_quota",
                "memes",
                "image_generation",
                "ledger"
            ]
        );
        assert_eq!(
            plugin_info("memes").map(|plugin| plugin.name),
            Some("表情包")
        );
        assert!(plugin_info("nope").is_none());
    }
}
