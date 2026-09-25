//! 插件的开关与逐项设置。
//!
//! 插件用 [`TuiPlugin`] 枚举标识,不用菜单下标:开关、名字、表单都按枚举
//! `match`,漏一个分支编译不过。表单字段与写回写在一起([`BoundFields`]),
//! 加字段不会让其后的值错位。

use crate::config_tui::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TuiPlugin {
    Web,
    Vision,
    ImageGeneration,
    WebImages,
    PrintImage,
    Memes,
    KnowledgeBase,
    Archlinux,
    Memory,
    ApiQuota,
}

/// 菜单顺序。
const TUI_PLUGINS: [TuiPlugin; 10] = [
    TuiPlugin::Web,
    TuiPlugin::Vision,
    TuiPlugin::ImageGeneration,
    TuiPlugin::WebImages,
    TuiPlugin::PrintImage,
    TuiPlugin::Memes,
    TuiPlugin::KnowledgeBase,
    TuiPlugin::Archlinux,
    TuiPlugin::Memory,
    TuiPlugin::ApiQuota,
];

impl TuiPlugin {
    fn name(self) -> &'static str {
        match self {
            Self::Web => t("Web search", "网络搜索"),
            Self::Vision => t("Vision", "识图"),
            Self::ImageGeneration => t("Image generation", "生图"),
            Self::WebImages => t("Image search", "搜图"),
            Self::PrintImage => t("Print image", "打印图片"),
            Self::Memes => t("Memes", "表情包"),
            Self::KnowledgeBase => t("Knowledge base", "知识库"),
            Self::Archlinux => "Arch Linux",
            Self::Memory => t("Memory", "记忆"),
            Self::ApiQuota => t("LLM API quota", "大模型额度查询"),
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Web => t(
                "Search APIs with script fallback",
                "搜索 API 与脚本 fallback",
            ),
            Self::Vision => t(
                "Image understanding and terminal preview",
                "图片理解和终端预览",
            ),
            Self::ImageGeneration => t("Generate images from text", "文本生成图片"),
            Self::WebImages => t(
                "Search, download, and review web images",
                "网络图片搜索、下载与审核",
            ),
            Self::PrintImage => t("Terminal image print size", "终端图片打印尺寸"),
            Self::Memes => t("Persona meme library and send size", "人格表情库与发送尺寸"),
            Self::KnowledgeBase => t(
                "Local file search and semantic index",
                "本地文件检索与语义索引",
            ),
            Self::Archlinux => t(
                "AUR lookup, PKGBUILD review, ArchWiki",
                "AUR 查询、PKGBUILD 审查与 ArchWiki",
            ),
            Self::Memory => t("Long-term memory and association", "长期记忆与联想"),
            Self::ApiQuota => t(
                "Query DeepSeek and OpenRouter API quota",
                "查询 DeepSeek 与 OpenRouter API 额度",
            ),
        }
    }

    fn enabled_flag(self, config: &mut AppConfig) -> &mut bool {
        let plugins = &mut config.plugins;
        match self {
            Self::Web => &mut plugins.web.enabled,
            Self::Vision => &mut plugins.vision.enabled,
            Self::ImageGeneration => &mut plugins.image_generation.enabled,
            Self::WebImages => &mut plugins.web_images.enabled,
            Self::PrintImage => &mut plugins.print_image.enabled,
            Self::Memes => &mut plugins.memes.enabled,
            Self::KnowledgeBase => &mut plugins.knowledge_base.enabled,
            Self::Archlinux => &mut plugins.archlinux.enabled,
            Self::Memory => &mut plugins.memory.enabled,
            Self::ApiQuota => &mut plugins.api_quota.enabled,
        }
    }

    fn enabled(self, config: &AppConfig) -> bool {
        let plugins = &config.plugins;
        match self {
            Self::Web => plugins.web.enabled,
            Self::Vision => plugins.vision.enabled,
            Self::ImageGeneration => plugins.image_generation.enabled,
            Self::WebImages => plugins.web_images.enabled,
            Self::PrintImage => plugins.print_image.enabled,
            Self::Memes => plugins.memes.enabled,
            Self::KnowledgeBase => plugins.knowledge_base.enabled,
            Self::Archlinux => plugins.archlinux.enabled,
            Self::Memory => plugins.memory.enabled,
            Self::ApiQuota => plugins.api_quota.enabled,
        }
    }

    fn toggle(self, config: &mut AppConfig) {
        let flag = self.enabled_flag(config);
        *flag = !*flag;
    }
}

pub(in crate::config_tui) fn edit_plugins(
    stdout: &mut io::Stdout,
    paths: &GqyPaths,
    config: &mut AppConfig,
) -> Result<()> {
    let mut selected = 0usize;
    loop {
        draw_plugin_menu(stdout, config, selected)?;
        // 最后一行是「扩展」入口，不是内置插件，没有开关
        let plugin = TUI_PLUGINS.get(selected).copied();
        match read_key()? {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
            KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => selected = (selected + 1).min(TUI_PLUGINS.len()),
            KeyCode::Char(' ') => {
                if let Some(plugin) = plugin {
                    plugin.toggle(config)
                }
            }
            KeyCode::Enter | KeyCode::Char('i') => match plugin {
                Some(plugin) => edit_plugin_detail(stdout, config, plugin)?,
                None => edit_extensions(stdout, paths, config)?,
            },
            _ => {}
        }
    }
}

fn draw_plugin_menu(stdout: &mut io::Stdout, config: &AppConfig, selected: usize) -> Result<()> {
    let (cols, rows) = terminal::size()?;
    let width = cols.saturating_sub(4).max(60);
    let height = rows.saturating_sub(2).max(10);
    let x = 2;
    let y = 1;
    queue!(stdout, Clear(ClearType::All))?;
    draw_box(stdout, x, y, width, height, t(" PLUGINS ", " 插件 "))?;
    queue!(
        stdout,
        MoveTo(x + 2, y + 1),
        Print(t(
            "[Space]enable/disable [Enter]configure [j/k]move [q]back",
            "[Space]启用/禁用 [Enter]配置 [j/k]移动 [q]返回",
        ))
    )?;
    queue!(
        stdout,
        MoveTo(x + 2, y + 3),
        SetAttribute(Attribute::Bold),
        Print(pad(
            &plugin_row(
                t("Status", "状态"),
                t("Plugin", "插件"),
                t("Description", "说明"),
                width.saturating_sub(4) as usize,
            ),
            width.saturating_sub(4) as usize,
        )),
        SetAttribute(Attribute::Reset)
    )?;
    let visible_rows = height.saturating_sub(6) as usize;
    let start = selected.saturating_sub(visible_rows.saturating_sub(1));
    let extensions = (
        "  ›",
        t("Extensions", "扩展"),
        t(
            "Skills, script tools, MCP servers and pm packages",
            "技能、脚本工具、MCP 服务器和 pm 包",
        ),
    );
    let lines = TUI_PLUGINS
        .iter()
        .map(|plugin| {
            let state = if plugin.enabled(config) {
                t("[ON]", "[开]")
            } else {
                t("[OFF]", "[关]")
            };
            (state, plugin.name(), plugin.description())
        })
        .chain(std::iter::once(extensions));
    for (row, (index, (state, name, description))) in
        lines.enumerate().skip(start).take(visible_rows).enumerate()
    {
        let line = plugin_row(state, name, description, width.saturating_sub(4) as usize);
        queue!(stdout, MoveTo(x + 2, y + row as u16 + 4))?;
        if index == selected {
            queue!(
                stdout,
                SetAttribute(Attribute::Reverse),
                Print(pad(&line, width.saturating_sub(4) as usize)),
                SetAttribute(Attribute::Reset)
            )?;
        } else {
            queue!(stdout, Print(pad(&line, width.saturating_sub(4) as usize)))?;
        }
    }
    stdout.flush()?;
    Ok(())
}

pub(in crate::config_tui) fn plugin_row(
    state: &str,
    name: &str,
    description: &str,
    width: usize,
) -> String {
    let fixed = pad(state, 8) + &pad(name, 24);
    let remaining = width.saturating_sub(display_width(&fixed)).max(10);
    fixed + &truncate(description, remaining)
}

fn edit_plugin_detail(
    stdout: &mut io::Stdout,
    config: &mut AppConfig,
    plugin: TuiPlugin,
) -> Result<()> {
    // api_quota 有专门的账号管理界面,不走通用表单。
    if plugin == TuiPlugin::ApiQuota {
        return edit_api_quota(stdout, config);
    }
    let title = format!(" {}: {} ", t("PLUGIN", "插件"), plugin.name());
    let mut form = plugin_fields(config, plugin);
    if !run_form(stdout, &title, &mut form.fields)? {
        return Ok(());
    }
    form.apply(config)
}

fn enabled_field(plugin: TuiPlugin, config: &AppConfig) -> Field {
    Field::boolean(t("Enabled", "启用"), plugin.enabled(config))
}

fn plugin_fields(config: &AppConfig, plugin: TuiPlugin) -> BoundFields {
    let form = BoundFields::default();
    match plugin {
        TuiPlugin::Web => form
            .with(enabled_field(plugin, config), |config, value| {
                config.plugins.web.enabled = parse_bool_field(value)?;
                Ok(())
            })
            .with(
                Field::new(
                    t("Results per request", "每次返回数量"),
                    config.plugins.web.max_results.to_string(),
                ),
                |config, value| {
                    config.plugins.web.max_results = value.trim().parse::<usize>()?.clamp(1, 10);
                    Ok(())
                },
            )
            .with(
                Field::textarea(
                    "Tavily API Keys",
                    config.plugins.web.tavily_api_keys.join("\n"),
                )
                .sensitive(),
                |config, value| {
                    config.plugins.web.tavily_api_keys = parse_key_list(value);
                    Ok(())
                },
            )
            .with(
                Field::textarea(
                    "Firecrawl API Keys",
                    config.plugins.web.firecrawl_api_keys.join("\n"),
                )
                .sensitive(),
                |config, value| {
                    config.plugins.web.firecrawl_api_keys = parse_key_list(value);
                    Ok(())
                },
            )
            .with(
                Field::textarea(
                    "AnySearch API Keys",
                    config.plugins.web.anysearch_api_keys.join("\n"),
                )
                .sensitive(),
                |config, value| {
                    config.plugins.web.anysearch_api_keys = parse_key_list(value);
                    Ok(())
                },
            )
            .with(
                Field::textarea(
                    t(
                        "Exa API Keys (optional; keyless free quota)",
                        "Exa API Keys（可留空用免费额度）",
                    ),
                    config.plugins.web.exa_api_keys.join("\n"),
                )
                .sensitive(),
                |config, value| {
                    config.plugins.web.exa_api_keys = parse_key_list(value);
                    Ok(())
                },
            )
            .with(
                Field::new("SearXNG URL", config.plugins.web.searxng_base_url.clone()),
                |config, value| {
                    config.plugins.web.searxng_base_url =
                        value.trim().trim_end_matches('/').to_string();
                    Ok(())
                },
            ),
        TuiPlugin::Vision => form
            .with(enabled_field(plugin, config), |config, value| {
                config.plugins.vision.enabled = parse_bool_field(value)?;
                Ok(())
            })
            .with(
                Field::boolean(
                    t(
                        "Prefer current chat model for images",
                        "优先使用当前对话模型识图",
                    ),
                    config.plugins.vision.prefer_current_multimodal_model,
                ),
                |config, value| {
                    config.plugins.vision.prefer_current_multimodal_model =
                        parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Vision provider/model", "识图 Provider/模型"),
                    vision_provider_value(config),
                )
                .choices_owned(vision_provider_model_choice_values(config)),
                |config, value| {
                    let (provider_id, model) = parse_provider_model_choice(value);
                    config.plugins.vision.vision_provider_id = provider_id;
                    config.plugins.vision.vision_model = model;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Response header timeout (seconds)", "响应头超时秒数"),
                    config
                        .plugins
                        .vision
                        .response_header_timeout_seconds
                        .to_string(),
                ),
                |config, value| {
                    config.plugins.vision.response_header_timeout_seconds =
                        value.trim().parse::<u64>()?.max(1);
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Stream idle timeout (seconds)", "流空闲超时秒数"),
                    config
                        .plugins
                        .vision
                        .stream_idle_timeout_seconds
                        .to_string(),
                ),
                |config, value| {
                    config.plugins.vision.stream_idle_timeout_seconds =
                        value.trim().parse::<u64>()?.max(1);
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Per-image timeout (seconds)", "单图总超时秒数"),
                    config.plugins.vision.image_timeout_seconds.to_string(),
                ),
                |config, value| {
                    config.plugins.vision.image_timeout_seconds =
                        value.trim().parse::<u64>()?.max(1);
                    Ok(())
                },
            ),
        TuiPlugin::ImageGeneration => form
            .with(enabled_field(plugin, config), |config, value| {
                config.plugins.image_generation.enabled = parse_bool_field(value)?;
                Ok(())
            })
            .with(
                Field::new(
                    t("Image API type", "生图 API 类型"),
                    config.plugins.image_generation.provider_type.clone(),
                )
                .choices(&["openai", "rightcode"]),
                |config, value| {
                    config.plugins.image_generation.provider_type = value.trim().to_string();
                    Ok(())
                },
            )
            .with(
                Field::new("Base URL", config.plugins.image_generation.base_url.clone()),
                |config, value| {
                    config.plugins.image_generation.base_url =
                        value.trim().trim_end_matches('/').to_string();
                    Ok(())
                },
            )
            .with(
                Field::textarea(
                    "API Keys",
                    config.plugins.image_generation.api_keys.join("\n"),
                )
                .sensitive(),
                |config, value| {
                    config.plugins.image_generation.api_keys = parse_key_list(value);
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Model", "模型"),
                    config.plugins.image_generation.model.clone(),
                ),
                |config, value| {
                    config.plugins.image_generation.model = value.trim().to_string();
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Default aspect ratio", "默认宽高比"),
                    config.plugins.image_generation.default_aspect_ratio.clone(),
                )
                .choices(&[
                    "自动", "1:1", "2:3", "3:2", "3:4", "4:3", "4:5", "5:4", "9:16", "16:9", "21:9",
                ]),
                |config, value| {
                    config.plugins.image_generation.default_aspect_ratio = value.trim().to_string();
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Default resolution", "默认分辨率"),
                    config.plugins.image_generation.default_resolution.clone(),
                )
                .choices(&["1K", "2K", "4K"]),
                |config, value| {
                    config.plugins.image_generation.default_resolution = value.trim().to_string();
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Output directory", "输出目录"),
                    config.plugins.image_generation.output_dir.clone(),
                ),
                |config, value| {
                    config.plugins.image_generation.output_dir = value.trim().to_string();
                    Ok(())
                },
            )
            .with(
                Field::boolean(
                    t("Print when complete", "完成后打印"),
                    config.plugins.image_generation.auto_print,
                ),
                |config, value| {
                    config.plugins.image_generation.auto_print = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Timeout (seconds)", "超时秒数"),
                    config.plugins.image_generation.timeout_seconds.to_string(),
                ),
                |config, value| {
                    config.plugins.image_generation.timeout_seconds = value.trim().parse()?;
                    Ok(())
                },
            ),
        TuiPlugin::WebImages => form
            .with(enabled_field(plugin, config), |config, value| {
                config.plugins.web_images.enabled = parse_bool_field(value)?;
                Ok(())
            })
            .with(
                Field::new(
                    t("Search source mode", "搜索来源模式"),
                    config.plugins.web_images.source_mode.clone(),
                )
                .choices(&["auto", "global", "mainland"]),
                |config, value| {
                    config.plugins.web_images.source_mode = match value.trim() {
                        "auto" | "global" | "mainland" => value.trim().to_string(),
                        other => {
                            if is_zh() {
                                anyhow::bail!("未知搜图来源模式: {other}")
                            } else {
                                anyhow::bail!("Unknown image search source mode: {other}")
                            }
                        }
                    };
                    Ok(())
                },
            )
            .with(
                Field::boolean(
                    t("Vision model review", "视觉模型审核"),
                    config.plugins.web_images.vision_screening_enabled,
                ),
                |config, value| {
                    config.plugins.web_images.vision_screening_enabled = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Maximum results", "数量上限"),
                    config.plugins.web_images.max_results.to_string(),
                ),
                |config, value| {
                    config.plugins.web_images.max_results =
                        value.trim().parse::<usize>()?.clamp(1, 10);
                    Ok(())
                },
            )
            .with(
                Field::boolean(
                    t("Safe search", "安全搜索"),
                    config.plugins.web_images.safe_search,
                ),
                |config, value| {
                    config.plugins.web_images.safe_search = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::boolean(
                    t("Automatic preview", "自动预览"),
                    config.plugins.web_images.auto_preview,
                ),
                |config, value| {
                    config.plugins.web_images.auto_preview = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Default preview count", "默认预览数量"),
                    config.plugins.web_images.preview_count.to_string(),
                ),
                |config, value| {
                    config.plugins.web_images.preview_count = value.trim().parse::<usize>()?.min(5);
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Maximum download (MB)", "最大下载 MB"),
                    config.plugins.web_images.max_download_mb.to_string(),
                ),
                |config, value| {
                    config.plugins.web_images.max_download_mb =
                        value.trim().parse::<f64>()?.clamp(0.1, 50.0);
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Timeout (seconds)", "超时秒数"),
                    config.plugins.web_images.timeout_seconds.to_string(),
                ),
                |config, value| {
                    config.plugins.web_images.timeout_seconds =
                        value.trim().parse::<u64>()?.clamp(5, 120);
                    Ok(())
                },
            ),
        TuiPlugin::PrintImage => form
            .with(enabled_field(plugin, config), |config, value| {
                config.plugins.print_image.enabled = parse_bool_field(value)?;
                Ok(())
            })
            .with(
                Field::new(
                    t("Print width percent", "打印宽度百分比"),
                    config.plugins.print_image.width_percent.to_string(),
                ),
                |config, value| {
                    config.plugins.print_image.width_percent = value.trim().parse::<u8>()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Print height percent", "打印高度百分比"),
                    config.plugins.print_image.height_percent.to_string(),
                ),
                |config, value| {
                    config.plugins.print_image.height_percent = value.trim().parse::<u8>()?;
                    Ok(())
                },
            ),
        TuiPlugin::Memes => form
            .with(enabled_field(plugin, config), |config, value| {
                config.plugins.memes.enabled = parse_bool_field(value)?;
                Ok(())
            })
            .with(
                Field::new(
                    t("Send width percent", "发送宽度百分比"),
                    config.plugins.memes.width_percent.to_string(),
                ),
                |config, value| {
                    config.plugins.memes.width_percent = value.trim().parse::<u8>()?.clamp(1, 100);
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Send height percent", "发送高度百分比"),
                    config.plugins.memes.height_percent.to_string(),
                ),
                |config, value| {
                    config.plugins.memes.height_percent = value.trim().parse::<u8>()?.clamp(1, 100);
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Maximum image size (MB)", "最大图片 MB"),
                    config.plugins.memes.max_image_mb.to_string(),
                ),
                |config, value| {
                    config.plugins.memes.max_image_mb = value.trim().parse::<u64>()?.clamp(1, 100);
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Maximum search results", "搜索最大结果数"),
                    config.plugins.memes.search_max_results.to_string(),
                ),
                |config, value| {
                    config.plugins.memes.search_max_results =
                        value.trim().parse::<usize>()?.clamp(1, 3);
                    Ok(())
                },
            )
            .with(
                Field::boolean(
                    t("Allow animated GIFs", "允许 GIF 动画"),
                    config.plugins.memes.allow_gif_animation,
                ),
                |config, value| {
                    config.plugins.memes.allow_gif_animation = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::boolean(
                    t("Suggest memes automatically", "自动提示发送表情"),
                    config.plugins.memes.auto_send_enabled,
                ),
                |config, value| {
                    config.plugins.memes.auto_send_enabled = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::boolean(
                    t(
                        "Suggest memes automatically on platforms",
                        "通讯平台自动提示发送表情",
                    ),
                    config.plugins.memes.auto_send_platform_enabled,
                ),
                |config, value| {
                    config.plugins.memes.auto_send_platform_enabled = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t(
                        "Automatic meme suggestion probability",
                        "自动提示发送表情概率",
                    ),
                    config.plugins.memes.auto_send_probability.to_string(),
                ),
                |config, value| {
                    config.plugins.memes.auto_send_probability =
                        value.trim().parse::<f32>()?.clamp(0.0, 1.0);
                    Ok(())
                },
            ),
        TuiPlugin::KnowledgeBase => {
            let kb = &config.plugins.knowledge_base;
            form.with(enabled_field(plugin, config), |config, value| {
                config.plugins.knowledge_base.enabled = parse_bool_field(value)?;
                Ok(())
            })
            .with(
                Field::new(
                    t("Knowledge base directory", "知识库目录"),
                    kb.data_dir.clone(),
                ),
                |config, value| {
                    config.plugins.knowledge_base.data_dir = value.trim().to_string();
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Maximum search results", "搜索最大结果数"),
                    kb.max_search_results.to_string(),
                ),
                |config, value| {
                    config.plugins.knowledge_base.max_search_results = value.trim().parse()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Snippet context characters", "片段上下文字数"),
                    kb.snippet_context_chars.to_string(),
                ),
                |config, value| {
                    config.plugins.knowledge_base.snippet_context_chars = value.trim().parse()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Proximity window characters", "同窗匹配范围"),
                    kb.proximity_window_chars.to_string(),
                ),
                |config, value| {
                    config.plugins.knowledge_base.proximity_window_chars = value.trim().parse()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Maximum lines to read", "读取最大行数"),
                    kb.max_read_lines.to_string(),
                ),
                |config, value| {
                    config.plugins.knowledge_base.max_read_lines = value.trim().parse()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Maximum file size (KB)", "最大文件 KB"),
                    kb.max_file_size_kb.to_string(),
                ),
                |config, value| {
                    config.plugins.knowledge_base.max_file_size_kb = value.trim().parse()?;
                    Ok(())
                },
            )
            .with(
                Field::boolean(
                    t("Allow AI uploads", "允许 AI 上传"),
                    kb.upload_tool_enabled,
                ),
                |config, value| {
                    config.plugins.knowledge_base.upload_tool_enabled = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::boolean(
                    t("Enable embedding", "启用 Embedding"),
                    kb.embedding_enabled,
                ),
                |config, value| {
                    config.plugins.knowledge_base.embedding_enabled = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Embedding provider/model", "Embedding Provider/模型"),
                    kb_embedding_provider_value(config),
                )
                .choices_owned(provider_model_choice_values(config, false))
                .empty_choice_label(t("Embedding not configured", "未配置 Embedding")),
                |config, value| {
                    let (provider_id, model) = parse_provider_model_choice(value);
                    config.plugins.knowledge_base.embedding_provider_id = provider_id;
                    config.plugins.knowledge_base.embedding_model = model;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Semantic chunk size", "语义块大小"),
                    kb.semantic_chunk_chars.to_string(),
                ),
                |config, value| {
                    config.plugins.knowledge_base.semantic_chunk_chars = value.trim().parse()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Semantic chunk overlap", "语义块重叠"),
                    kb.semantic_chunk_overlap.to_string(),
                ),
                |config, value| {
                    config.plugins.knowledge_base.semantic_chunk_overlap = value.trim().parse()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Semantic candidates", "语义候选数"),
                    kb.semantic_top_k.to_string(),
                ),
                |config, value| {
                    config.plugins.knowledge_base.semantic_top_k = value.trim().parse()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Minimum semantic score", "语义最低分"),
                    kb.semantic_min_score.to_string(),
                ),
                |config, value| {
                    config.plugins.knowledge_base.semantic_min_score = value.trim().parse()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Strong keyword match threshold", "关键词强命中阈值"),
                    kb.keyword_strong_score_threshold.to_string(),
                ),
                |config, value| {
                    config.plugins.knowledge_base.keyword_strong_score_threshold =
                        value.trim().parse()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Embedding timeout (seconds)", "Embedding 超时秒数"),
                    kb.embedding_timeout_seconds.to_string(),
                ),
                |config, value| {
                    config.plugins.knowledge_base.embedding_timeout_seconds =
                        value.trim().parse()?;
                    Ok(())
                },
            )
        }
        TuiPlugin::Archlinux => form.with(enabled_field(plugin, config), |config, value| {
            config.plugins.archlinux.enabled = parse_bool_field(value)?;
            Ok(())
        }),
        TuiPlugin::Memory => {
            let memory = config.memory_config();
            form.with(
                Field::boolean(t("Enabled", "启用"), memory.enabled),
                |config, value| {
                    // 表单按合并视图(memory_config)展示;写回时旧的顶层
                    // `memory` 段清空,全部落到 plugins.memory。
                    config.memory = crate::config::MemoryConfig::default();
                    config.plugins.memory.enabled = parse_bool_field(value)?;
                    config.plugins.memory.auto_skill_enabled = false;
                    Ok(())
                },
            )
            .with(
                Field::boolean(
                    t("Evicted context cache", "上下文弹出缓存"),
                    memory.evicted_context_enabled,
                ),
                |config, value| {
                    config.plugins.memory.evicted_context_enabled = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::boolean(
                    t("Enable association", "联想启用"),
                    memory.association_enabled,
                ),
                |config, value| {
                    config.plugins.memory.association_enabled = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::boolean(t("Automatic diary", "自动日记"), memory.auto_diary_enabled),
                |config, value| {
                    config.plugins.memory.auto_diary_enabled = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::boolean(
                    t("Automatic fact memory", "自动知识记忆"),
                    memory.auto_fact_enabled,
                ),
                |config, value| {
                    config.plugins.memory.auto_fact_enabled = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Diary batch size", "日记整理轮数"),
                    memory.diary_batch_size.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.diary_batch_size =
                        value.trim().parse::<usize>()?.clamp(2, 100);
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Short diary retention days", "短期日记保留天数"),
                    memory.short_diary_retention_days.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.short_diary_retention_days =
                        value.trim().parse::<u64>()?.clamp(1, 3650);
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Diary promotion recalls", "日记长期化召回次数"),
                    memory.diary_promotion_recalls.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.diary_promotion_recalls =
                        value.trim().parse::<u64>()?.clamp(1, 100);
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Organizer timeout seconds", "记忆整理超时秒数"),
                    memory.organizer_timeout_seconds.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.organizer_timeout_seconds =
                        value.trim().parse::<u64>()?.clamp(5, 600);
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t(
                        "Chat review idle seconds (0 = off)",
                        "聊后复盘等待秒数（0 关闭）",
                    ),
                    memory.review_idle_seconds.to_string(),
                ),
                |config, value| {
                    let seconds = value.trim().parse::<u64>()?;
                    config.plugins.memory.review_idle_seconds = if seconds == 0 {
                        0
                    } else {
                        seconds.clamp(300, 86400)
                    };
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Associated facts", "联想知识条数"),
                    memory.association_facts.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.association_facts = value.trim().parse::<usize>()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Associated events", "联想事件条数"),
                    memory.association_episodes.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.association_episodes = value.trim().parse::<usize>()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Association character limit", "联想字符上限"),
                    memory.association_max_chars.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.association_max_chars = value.trim().parse::<usize>()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Per-entry association limit (chars)", "单条联想正文上限"),
                    memory.association_entry_chars.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.association_entry_chars =
                        value.trim().parse::<usize>()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Memory snippet length (chars)", "记忆片段字数"),
                    memory.snippet_chars.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.snippet_chars = value.trim().parse::<usize>()?;
                    Ok(())
                },
            )
            .with(
                Field::boolean(
                    t("Enable forgetting", "遗忘启用"),
                    memory.forgetting_enabled,
                ),
                |config, value| {
                    config.plugins.memory.forgetting_enabled = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Forgetting half-life (days)", "遗忘半衰期天"),
                    memory.forgetting_half_life_days.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.forgetting_half_life_days =
                        value.trim().parse::<f64>()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Minimum forgetting strength", "遗忘最低强度"),
                    memory.forgetting_min_strength.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.forgetting_min_strength = value.trim().parse::<f64>()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Recall boost strength", "回忆增强强度"),
                    memory.forgetting_review_boost.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.forgetting_review_boost = value.trim().parse::<f64>()?;
                    Ok(())
                },
            )
            .with(
                Field::boolean(
                    t("Association dedup", "联想跨回合去重"),
                    memory.association_dedup,
                ),
                |config, value| {
                    config.plugins.memory.association_dedup = parse_bool_field(value)?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t("Forget after (days)", "遗忘期限(天)"),
                    memory.forget_after_days.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.forget_after_days = value.trim().parse::<u64>()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t(
                        "Minimum task length for learning (chars)",
                        "学习任务最短字数",
                    ),
                    memory.learning_min_task_chars.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.learning_min_task_chars =
                        value.trim().parse::<usize>()?;
                    Ok(())
                },
            )
            .with(
                Field::new(
                    t(
                        "Minimum method length for learning (chars)",
                        "学习方法最短字数",
                    ),
                    memory.learning_min_method_chars.to_string(),
                ),
                |config, value| {
                    config.plugins.memory.learning_min_method_chars =
                        value.trim().parse::<usize>()?;
                    Ok(())
                },
            )
        }
        TuiPlugin::ApiQuota => form.with(enabled_field(plugin, config), |config, value| {
            config.plugins.api_quota.enabled = parse_bool_field(value)?;
            Ok(())
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 写回按字段自己的 setter 走:改表单里第 N 个值,只动那一个设置项。
    #[test]
    fn plugin_forms_write_each_field_back_to_its_own_setting() {
        let mut config = AppConfig::default();
        let mut form = plugin_fields(&config, TuiPlugin::Memes);
        let probability = form
            .fields
            .iter_mut()
            .find(|field| {
                field.label
                    == t(
                        "Automatic meme suggestion probability",
                        "自动提示发送表情概率",
                    )
            })
            .unwrap();
        probability.value = "0.25".to_string();
        let before = config.plugins.memes.search_max_results;
        form.apply(&mut config).unwrap();
        assert_eq!(config.plugins.memes.auto_send_probability, 0.25);
        assert_eq!(config.plugins.memes.search_max_results, before);
    }

    #[test]
    fn every_plugin_toggles_its_own_flag() {
        for plugin in TUI_PLUGINS {
            let mut config = AppConfig::default();
            let before = plugin.enabled(&config);
            plugin.toggle(&mut config);
            assert_eq!(plugin.enabled(&config), !before, "{plugin:?}");
            for other in TUI_PLUGINS.into_iter().filter(|other| *other != plugin) {
                assert_eq!(
                    other.enabled(&config),
                    other.enabled(&AppConfig::default()),
                    "toggling {plugin:?} changed {other:?}"
                );
            }
        }
    }
}
