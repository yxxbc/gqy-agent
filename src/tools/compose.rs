//! 工具面装配:一条流水线装出任何场所、任何 persona 的工具面。
//!
//! 注册单元写成一张有序表([`UNITS`]),不是手写 if 链:插件 id 与它的注册函数
//! 写在同一行,表与 `config::plugin_catalog` 的一一对应由测试钉着——加插件漏了
//! 哪一边都当场报红,而不是运行时静默少一件工具。

use super::*;

/// 场所声明的两件事之一:谁在说话。Owner=属主类入口(终端、本机 WebUI、
/// 语音);External=不可信入口(QQ 群、远端 WebUI 成员),只拿 `Trust: external`
/// 的工具;Internal=判官/子代理,工具面与 Owner 相同、提示词另算。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceTrust {
    Owner,
    External,
    Internal,
}

/// 场所:信任 + 能力。能力位今天只有「能弹问题」(ask_question 需要面板);
/// 浏览器(artifact/share)那两件仍由 WebUI 层按会话追加,因为要会话 id 与库。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Surface {
    pub trust: SurfaceTrust,
    pub interactive_questions: bool,
}

impl Surface {
    pub const fn owner(interactive_questions: bool) -> Self {
        Self {
            trust: SurfaceTrust::Owner,
            interactive_questions,
        }
    }

    pub const fn external() -> Self {
        Self {
            trust: SurfaceTrust::External,
            interactive_questions: false,
        }
    }
}

/// 一次装配的输入。
struct Compose<'a> {
    config: &'a AppConfig,
    paths: &'a GqyPaths,
    manifest: &'a PersonaManifest,
    surface: Surface,
}

impl Compose<'_> {
    fn plugin(&self, id: &str) -> bool {
        self.manifest.plugin_enabled(id)
    }

    fn external(&self) -> bool {
        self.surface.trust == SurfaceTrust::External
    }
}

/// 一个注册单元。
struct Unit {
    /// 受 persona 插件闸管的单元写插件 id(`config::PLUGIN_IDS` 之一);
    /// core 与子系统单元不受插件闸管,写 `None`。
    plugin: Option<&'static str>,
    /// 插件闸之外的条件:机器级开关、子系统位、场所能力。
    when: fn(&Compose) -> bool,
    register: fn(&mut ToolRegistry, &Compose),
}

fn always(_: &Compose) -> bool {
    true
}

/// 注册顺序沿用旧表(subagent 的快照点、cross_hints 收尾都在原位),定义按名
/// 排序,所以三个面的 tools 数组与改表前逐字节相同(`shape_tests` 钉着)。
///
/// 插件关就不注册:关掉的插件仍然常驻一份完整契约,是三个面都白背的纯浪费
/// (08-17 实测 get_exchange_rate 311 字符)。
const UNITS: &[Unit] = &[
    // ── core:今天的 dev 那套,不看 persona ──
    Unit {
        // 读检索(read/glob/grep)、check_os_info、trash_path 一整套。
        plugin: Some("files"),
        when: always,
        register: |registry, c| {
            default_tools::register(
                registry,
                c.config.skills.allow_command_execution,
                c.config,
                c.paths,
            )
        },
    },
    Unit {
        // files 关着只挂 run_command:coreutils 干得更好的都不注册(dev 验收三轮裁剪)。
        plugin: None,
        when: |c| !c.plugin("files"),
        register: |registry, c| {
            default_tools::register_run_command(registry, c.config.skills.allow_command_execution)
        },
    },
    Unit {
        plugin: None,
        when: always,
        register: |registry, _| jobs::register_management(registry),
    },
    Unit {
        plugin: Some("usage_query"),
        when: always,
        register: |registry, c| {
            usage_query::register(
                registry,
                c.paths
                    .state_dir
                    .join(crate::state::usage::USAGE_HISTORY_FILE),
                c.config.clone(),
            )
        },
    },
    Unit {
        // 编辑器只留 apply_patch(聚合增/改/删,diff 渲染载体)。
        plugin: None,
        when: always,
        register: |registry, _| apply_patch::register(registry),
    },
    Unit {
        plugin: None,
        when: always,
        register: |registry, c| todowrite::register(registry, c.paths.clone()),
    },
    Unit {
        plugin: None,
        when: always,
        register: |registry, c| goal::register(registry, c.config.clone(), c.paths.clone()),
    },
    Unit {
        // 不做成 persona 插件:提 PR、修 PR 是 dev 人格的主业,core_only 也要有。
        // 不可信场所靠 trust 缺省 Owner 筛掉。
        plugin: None,
        when: |c| c.config.tools.github.enabled,
        register: |registry, c| github::register(registry, c.config, c.paths),
    },
    Unit {
        plugin: Some("alarm"),
        when: always,
        register: |registry, c| alarm::register(registry, c.paths.clone()),
    },
    Unit {
        plugin: None,
        when: always,
        register: |registry, _| web::register_fetch(registry),
    },
    // ── 扩展:按 persona 清单启用,各自受机器级开关约束 ──
    Unit {
        plugin: Some("exchange_rate"),
        when: |c| c.config.plugins.exchange_rate.enabled,
        register: |registry, c| {
            exchange_rate::register(registry, c.config.plugins.exchange_rate.clone())
        },
    },
    Unit {
        plugin: Some("map"),
        when: |c| c.config.plugins.map.enabled,
        register: |registry, c| map::register(registry, c.config.plugins.map.clone()),
    },
    Unit {
        plugin: Some("express"),
        when: |c| c.config.plugins.express.enabled,
        register: |registry, c| express::register(registry, c.config.plugins.express.clone()),
    },
    Unit {
        plugin: Some("archlinux"),
        when: |c| c.config.plugins.archlinux.enabled,
        register: |registry, c| archlinux::register(registry, c.paths),
    },
    Unit {
        plugin: Some("api_quota"),
        when: |c| c.config.plugins.api_quota.enabled,
        register: |registry, c| api_quota::register(registry, c.config.plugins.api_quota.clone()),
    },
    Unit {
        plugin: Some("print_image"),
        when: always,
        register: |registry, c| vision::register_print(registry, c.config.clone()),
    },
    Unit {
        plugin: Some("memes"),
        when: |c| c.config.plugins.memes.enabled,
        register: |registry, c| memes::register(registry, c.config.clone(), c.paths.clone()),
    },
    Unit {
        // 图库:和表情包挨着注册,但两件事——表情包是「该有反应时自动挑」,图库是
        // 「人让留的图,按名字/描述找出来发」。没有机器级开关,persona 说了算。
        plugin: Some("album"),
        when: always,
        register: |registry, c| album::register(registry, c.config.clone(), c.paths.clone()),
    },
    Unit {
        plugin: None,
        when: |c| c.manifest.subsystems.voice && c.config.voice.enabled,
        register: |registry, _| voice_chat::register(registry),
    },
    Unit {
        plugin: None,
        when: |c| c.manifest.subsystems.voice && c.config.voice.tts.is_active(),
        register: |registry, _| voice_speak::register(registry),
    },
    Unit {
        // 本地会话专属:平台会话有 send_message_to_user。只在 QQ 的 ws 已连上时
        // 注册(TurnResources 的缓存键带了连接位,连上/掉线会各自重建一份)。
        plugin: Some("platform_outreach"),
        when: |c| {
            c.config.platforms.terminal_outreach
                && c.config.platforms.qq.enabled
                && platform_outreach::qq_connected()
        },
        register: |registry, c| platform_outreach::register(registry, c.config),
    },
    Unit {
        plugin: None,
        when: |c| c.config.plugins.web.enabled,
        register: |registry, c| web::register(registry, c.config.plugins.web.clone()),
    },
    Unit {
        plugin: Some("web_images"),
        when: |c| c.config.plugins.web_images.enabled,
        register: |registry, c| {
            web_images::register(registry, c.config.clone(), c.paths.clone(), true)
        },
    },
    Unit {
        // 看图对 coding 也是刚需(UI 截图排错、设计稿、测试产出的图表);
        // 聊天模型不带眼睛时由 vision 插件路由给专用视觉模型。
        plugin: None,
        when: |c| c.config.plugins.vision.enabled,
        register: |registry, c| vision::register(registry, c.config.clone(), c.paths.clone(), true),
    },
    Unit {
        plugin: Some("image_generation"),
        when: |c| c.config.plugins.image_generation.enabled,
        register: |registry, c| {
            image_generation::register(registry, c.config.clone(), c.paths.clone())
        },
    },
    Unit {
        plugin: Some("knowledge_base"),
        when: |c| c.config.plugins.knowledge_base.enabled,
        register: |registry, c| {
            knowledge_base::register(registry, c.config.clone(), c.paths.clone())
        },
    },
    Unit {
        // 记忆整套按 persona 清单构造:关着就一件工具都不注册(联想注入、日记、
        // 前言在 agent 侧同样按清单裁决)。
        plugin: None,
        when: |c| c.manifest.memory_enabled(c.config),
        register: |registry, c| memory::register(registry, c.config.clone(), c.paths.clone()),
    },
    Unit {
        // 子代理拿的是这一刻的快照,指路句也得按它自己的工具面补。
        plugin: None,
        when: always,
        register: |registry, c| {
            let mut subagent_tools = registry.clone();
            cross_hints::apply(&mut subagent_tools);
            subagent::register(registry, c.config.clone(), c.paths.clone(), subagent_tools);
        },
    },
    Unit {
        // 记账:注册位置就是权限边界——不可信场所连工具名都不存在(trust 缺省 Owner)。
        plugin: Some("ledger"),
        when: always,
        register: |registry, c| ledger::register(registry, c.config.clone(), c.paths.clone()),
    },
    Unit {
        // 人格清单的脚本白名单在扫描层裁决(`scripts::retain_persona_visible`,
        // 注册与热刷新同一条路;人格自己那一层不受白名单管),这里不再往注册表
        // 挂一份按名字的过滤。
        // 不可信场所只收头部写了 `Trust: external` 的脚本;范围记在注册表上,
        // 热刷新走同一条 replace_script_tools 时照样过滤。
        plugin: Some("scripts"),
        when: always,
        register: |registry, c| {
            if c.external() {
                scripts::register_external(registry, c.config, c.paths);
            } else {
                scripts::register(registry, c.config, c.paths);
            }
        },
    },
    Unit {
        // MCP 与脚本同级:人格闸(`mcp` 插件)之上按服务器 id 白名单再筛一道,
        // 关掉的服务器连 tools/list 都不拉。
        plugin: Some("mcp"),
        when: |c| c.config.mcp.enabled,
        register: |registry, c| {
            mcp::register(
                registry,
                c.config.clone(),
                c.manifest.plugins.mcp.as_deref(),
            )
        },
    },
    Unit {
        plugin: None,
        when: |c| c.manifest.subsystems.skills && c.config.skills.enabled,
        register: |registry, c| {
            if let Err(error) = skills::register_skills(registry, c.config, c.paths) {
                tracing::warn!(error = %error, "failed to register skills");
            }
            skills::register_authoring(registry, c.config.clone(), c.paths.clone());
        },
    },
    Unit {
        plugin: None,
        when: |c| c.surface.interactive_questions,
        register: |registry, _| ask_question::register(registry),
    },
    Unit {
        // load_tools 常驻注册(09-01):full 模式下调用它无害(返回契约文本),
        // 而会话中途从需加载模型切到完整模型时,历史里的 load_tools 调用记录
        // 必须仍然可执行,否则模型模仿历史会撞未知工具。
        plugin: None,
        when: always,
        register: |registry, _| load_tools::register(registry),
    },
];

/// 一条流水线装出任何场所、任何 persona 的工具面(09-10 分层架构阶段 4,取代
/// builtin_registry / dev_registry / restricted_platform_registry 三张各写一遍):
///
/// 1. **core**:今天的 dev 那套——命令与后台任务、补丁编辑、todo、goal、web
///    抓取/搜索、看图、MCP、subagent 子代理、load_tools。不看 persona。
/// 2. **扩展**:按 persona 清单启用。子系统(记忆、技能、语音)与插件(其余
///    注册单元,id 见 config::PLUGIN_IDS)各自受 config 的机器级开关约束——
///    persona 只能在「本机装了的」里挑。
/// 3. **场所**:External 只留 `trust == External` 的工具,再做平台专属的描述
///    修饰;`interactive_questions` 决定给不给 ask_question。
pub fn compose_registry(
    config: &AppConfig,
    paths: &GqyPaths,
    manifest: &PersonaManifest,
    surface: Surface,
) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.set_default_timeout_secs(config.tools.default_timeout_secs);
    install_builtin_guards(&mut registry, config);
    let compose = Compose {
        config,
        paths,
        manifest,
        surface,
    };
    for unit in UNITS {
        if unit.plugin.is_none_or(|id| compose.plugin(id)) && (unit.when)(&compose) {
            (unit.register)(&mut registry, &compose);
        }
    }

    // ── 场所 ──
    if compose.external() {
        registry.retain_trust(ToolTrust::External);
        if registry.contains("generate_image") {
            // 静态英文追加,所有平台会话字节一致,不影响本地注册表的描述。
            registry.amend_description(
                "generate_image",
                " In messaging-platform conversations at most one image is generated per user request; the limit is enforced automatically.",
            );
        }
    }
    cross_hints::apply(&mut registry);
    registry
}

/// 属主面、当前人格的全量工具目录(旧 `builtin_registry`)。
pub fn builtin_registry(config: &AppConfig, paths: &GqyPaths) -> ToolRegistry {
    let manifest = PersonaManifest::load(config, paths, &config.active_persona_scope());
    compose_registry(config, paths, &manifest, Surface::owner(false))
}

/// dev persona 的工具目录:core 之上一件不挂(旧 `dev_registry`)。
pub fn dev_registry(config: &AppConfig, paths: &GqyPaths) -> ToolRegistry {
    let manifest = PersonaManifest::load(config, paths, crate::state::DEV_PERSONA);
    compose_registry(config, paths, &manifest, Surface::owner(false))
}

/// 不可信场所的工具面(旧 `restricted_platform_registry`):同一条流水线,
/// 末尾按 `Trust: external` 筛。以前是一张硬编码白名单,现在权限位写在每件
/// 工具自己的清单里(内置在 descriptions/*.json,脚本在头部)。
pub fn restricted_platform_registry(config: &AppConfig, paths: &GqyPaths) -> ToolRegistry {
    let manifest = PersonaManifest::load(config, paths, &config.active_persona_scope());
    compose_registry(config, paths, &manifest, Surface::external())
}

/// 按模式与配置组装工具注册表：REPL、daemon、WebUI、子代理都从这里拿。
///
/// 组装顺序有意义，不是随手排的：
///
/// 1. 先按模式选底座（`normal` 面向日常对话，`dev` 面向写代码），工具总开关
///    关掉时给一个空注册表而不是提前返回——调用方拿到的永远是同一个类型。
/// 2. 技能只在工具开着时注册；技能创作工具（`manage_skill`）只在 normal 模式
///    出现，dev 模式下模型该写代码不该写技能。
/// 3. `ask_question` 单独由调用方决定：daemon 与 WebUI 能弹面板，一次性
///    `gqy ask` 不能，所以它是参数而不是模式的函数。
/// 4. 最后登记脚本工具的显示名——这一步要在所有注册之后，否则新注册的脚本
///    在渲染层会显示成原始工具名。
///
/// 这个函数原本长在 `cli.rs` 里，于是 `web` 和 `tools` 都得反过来
/// `use crate::cli`，把两个底层模块钉死在最上层。它实际只依赖
/// tools/config/paths/agent，与 CLI 毫无关系，所以下沉到这里——拆分要断的
/// 两条边（`web→cli`、`tools→cli`）一次都断掉。
pub(crate) fn build_tool_registry(
    config: &AppConfig,
    paths: &GqyPaths,
    mode: AgentMode,
    interactive_questions: bool,
) -> anyhow::Result<ToolRegistry> {
    // mode 只剩「哪个 persona」这一层含义:Dev = 保留人格 "dev"(清单默认
    // core_only),Normal = 当前人格。真正裁决工具面的是 persona 清单。
    let persona = match mode {
        AgentMode::Dev => crate::state::DEV_PERSONA.to_string(),
        AgentMode::Normal => config.active_persona_scope(),
    };
    let registry = if config.tools.enabled {
        let manifest = PersonaManifest::load(config, paths, &persona);
        compose_registry(
            config,
            paths,
            &manifest,
            Surface::owner(interactive_questions),
        )
    } else {
        ToolRegistry::new()
    };
    // 最后登记脚本工具的显示名——要在所有注册之后,否则新注册的脚本在渲染层
    // 会显示成原始工具名。
    register_script_display_names(&registry);
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::plugin_catalog::PLUGIN_IDS;
    use std::collections::BTreeSet;

    /// 注册单元里的插件 id 与 `config::plugin_catalog` 一一对应:加插件只登记
    /// 了一边(目录里有、这里没注册;或注册了、目录里没名字)都在这里报红。
    #[test]
    fn plugin_units_match_the_plugin_catalog() {
        let mut seen = BTreeSet::new();
        for id in UNITS.iter().filter_map(|unit| unit.plugin) {
            assert!(seen.insert(id), "plugin `{id}` has more than one unit");
        }
        let catalog: BTreeSet<&str> = PLUGIN_IDS.iter().copied().collect();
        assert_eq!(
            seen, catalog,
            "tools::compose units and config::plugin_catalog disagree"
        );
    }
}
