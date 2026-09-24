//! 按配置路径声明一个设置项：`setting!(form, config, "English", "中文", context.trim_at_ratio)`。
//!
//! 字段的显示与写回由字段类型决定（`SettingValue`），路径同时记进
//! `BoundFields::paths`，防漏测试拿它对照 WebUI 的字段表。没有特殊换算的
//! 设置项都走这里；要换算的（界面语言的中文标签、行数上限）手写
//! `with_path`。

use crate::config_tui::*;

/// 一种配置值怎么放进表单、怎么从表单读回。
pub(in crate::config_tui) trait SettingValue: Sized {
    fn field(label: &'static str, value: &Self) -> Field;
    fn parse(text: &str) -> Result<Self>;
}

impl SettingValue for bool {
    fn field(label: &'static str, value: &Self) -> Field {
        Field::boolean(label, *value)
    }
    fn parse(text: &str) -> Result<Self> {
        parse_bool_field(text)
    }
}

fn parse_number<T: std::str::FromStr>(text: &str) -> Result<T> {
    let text = text.trim();
    text.parse::<T>().map_err(|_| {
        if is_zh() {
            anyhow::anyhow!("无效的数字: {text}")
        } else {
            anyhow::anyhow!("Invalid number: {text}")
        }
    })
}

macro_rules! number_setting {
    ($($ty:ty),*) => {
        $(
            impl SettingValue for $ty {
                fn field(label: &'static str, value: &Self) -> Field {
                    Field::new(label, value.to_string())
                }
                fn parse(text: &str) -> Result<Self> {
                    parse_number(text)
                }
            }
        )*
    };
}

number_setting!(usize, u64, u32, f32, f64);

impl SettingValue for String {
    fn field(label: &'static str, value: &Self) -> Field {
        Field::new(label, value.clone())
    }
    fn parse(text: &str) -> Result<Self> {
        Ok(text.trim().to_string())
    }
}

/// 列表一行一条（条目里可能有逗号、分号）。
impl SettingValue for Vec<String> {
    fn field(label: &'static str, value: &Self) -> Field {
        Field::line_list(label, value.join("\n"))
    }
    fn parse(text: &str) -> Result<Self> {
        Ok(split_line_items(text))
    }
}

/// 留空 = 未设置（`None`，用内置的自动值）。
impl SettingValue for Option<usize> {
    fn field(label: &'static str, value: &Self) -> Field {
        Field::new(
            label,
            value.map(|value| value.to_string()).unwrap_or_default(),
        )
    }
    fn parse(text: &str) -> Result<Self> {
        if text.trim().is_empty() {
            Ok(None)
        } else {
            parse_number(text).map(Some)
        }
    }
}

/// 留空 = 不限（`None`）。
impl SettingValue for Option<Vec<String>> {
    fn field(label: &'static str, value: &Self) -> Field {
        Field::line_list(label, value.as_deref().unwrap_or_default().join("\n"))
    }
    fn parse(text: &str) -> Result<Self> {
        let items = split_line_items(text);
        Ok((!items.is_empty()).then_some(items))
    }
}

/// `setting!(form, config, "English", "中文", a.b.c)`：按路径绑定一个设置项。
/// 末尾可加 `; choices = &["x", "y"]` 给出候选值。
macro_rules! setting {
    ($form:expr, $config:expr, $en:expr, $zh:expr, $head:ident $(. $tail:ident)* $(; choices = $choices:expr)?) => {
        $form.with_path(
            concat!(stringify!($head) $(, ".", stringify!($tail))*),
            {
                let field = $crate::config_tui::settings::spec::SettingValue::field(
                    t($en, $zh),
                    &$config.$head$(.$tail)*,
                );
                $(let field = field.choices($choices);)?
                field
            },
            |config, value| {
                config.$head$(.$tail)* =
                    $crate::config_tui::settings::spec::SettingValue::parse(value)?;
                Ok(())
            },
        )
    };
}

pub(in crate::config_tui) use setting;
