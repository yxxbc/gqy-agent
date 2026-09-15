//! `web/settings-schema.js` 与 Rust 配置的同步检查。
//!
//! 设置页的字段表是手抄 Rust 默认值的("改 Rust 默认值时请同步"),以前没有
//! 任何东西比对:字段路径拼错会让设置页往配置里写一个不存在的键,默认值抄错
//! 会让「缺省时显示的值」与实际生效的值不一致。这里把 JS 表读出来,逐字段对
//! `AppConfig::default()`。
//!
//! CI 没有 node,测试也不能依赖开发机环境,所以不执行 JS,只读它用到的那一小块
//! 字面量语法:对象/数组/字符串/数字、`const` 绑定、两个返回对象字面量的箭头
//! 小函数(`enabledField`、`score01`)。schema 里出现别的语法时这里会直接报错,
//! 而不是静默跳过。

use crate::config::AppConfig;
use serde_json::{Map, Value};
use std::collections::HashMap;

const SCHEMA: &str = include_str!("../../../web/settings-schema.js");

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Punct(char),
    Arrow,
    Str(String),
    /// 原文(去掉 `_` 分隔符),交给 serde_json 解析,整数保持整数。
    Num(String),
    Ident(String),
}

fn tokenize(src: &str) -> Vec<Tok> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if c == '"' || c == '\'' {
            let quote = c;
            let mut text = String::new();
            i += 1;
            while chars[i] != quote {
                if chars[i] == '\\' {
                    i += 1;
                    match chars[i] {
                        'n' => text.push('\n'),
                        't' => text.push('\t'),
                        'u' => {
                            let hex: String = chars[i + 1..i + 5].iter().collect();
                            text.push(
                                char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).unwrap(),
                            );
                            i += 4;
                        }
                        other => text.push(other),
                    }
                } else {
                    text.push(chars[i]);
                }
                i += 1;
            }
            i += 1;
            out.push(Tok::Str(text));
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            let text: String = chars[start..i].iter().filter(|c| **c != '_').collect();
            assert!(
                serde_json::from_str::<Value>(&text).is_ok_and(|value| value.is_number()),
                "settings-schema.js: unsupported number literal {text}"
            );
            out.push(Tok::Num(text));
        } else if c.is_alphabetic() || c == '_' || c == '$' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
            {
                i += 1;
            }
            out.push(Tok::Ident(chars[start..i].iter().collect()));
        } else if c == '=' && chars.get(i + 1) == Some(&'>') {
            out.push(Tok::Arrow);
            i += 2;
        } else {
            out.push(Tok::Punct(c));
            i += 1;
        }
    }
    out
}

enum Binding {
    Value(Value),
    Func { params: Vec<String>, body: usize },
}

struct Reader {
    toks: Vec<Tok>,
    globals: HashMap<String, Binding>,
}

impl Reader {
    fn expect(&self, pos: &mut usize, punct: char) {
        assert_eq!(
            self.toks.get(*pos),
            Some(&Tok::Punct(punct)),
            "settings-schema.js: expected `{punct}` at token {pos}"
        );
        *pos += 1;
    }

    fn eat(&self, pos: &mut usize, punct: char) -> bool {
        if self.toks.get(*pos) == Some(&Tok::Punct(punct)) {
            *pos += 1;
            true
        } else {
            false
        }
    }

    /// `(` 开头的是不是箭头函数的参数表。
    fn is_arrow(&self, pos: usize) -> bool {
        if self.toks.get(pos) != Some(&Tok::Punct('(')) {
            return false;
        }
        let mut depth = 0;
        for (index, tok) in self.toks.iter().enumerate().skip(pos) {
            match tok {
                Tok::Punct('(') => depth += 1,
                Tok::Punct(')') => {
                    depth -= 1;
                    if depth == 0 {
                        return self.toks.get(index + 1) == Some(&Tok::Arrow);
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// 表达式 = 基本项,可用 `+` 串接字符串(长提示文字跨行拼接)。
    fn expr(&self, pos: &mut usize, scope: &HashMap<String, Value>) -> Value {
        let mut value = self.primary(pos, scope);
        while self.eat(pos, '+') {
            match (value, self.primary(pos, scope)) {
                (Value::String(left), Value::String(right)) => value = Value::String(left + &right),
                (left, right) => panic!("settings-schema.js: unsupported `{left} + {right}`"),
            }
        }
        value
    }

    fn primary(&self, pos: &mut usize, scope: &HashMap<String, Value>) -> Value {
        let tok = self.toks[*pos].clone();
        *pos += 1;
        match tok {
            Tok::Str(text) => Value::String(text),
            Tok::Num(text) => serde_json::from_str(&text).unwrap(),
            Tok::Punct('-') => match self.primary(pos, scope) {
                Value::Number(number) => match number.as_i64() {
                    Some(value) => serde_json::json!(-value),
                    None => serde_json::json!(-number.as_f64().unwrap()),
                },
                other => panic!("settings-schema.js: cannot negate {other}"),
            },
            Tok::Punct('(') => {
                let value = self.expr(pos, scope);
                self.expect(pos, ')');
                value
            }
            Tok::Punct('[') => {
                let mut items = Vec::new();
                while !self.eat(pos, ']') {
                    items.push(self.expr(pos, scope));
                    if !self.eat(pos, ',') {
                        self.expect(pos, ']');
                        break;
                    }
                }
                Value::Array(items)
            }
            Tok::Punct('{') => {
                let mut object = Map::new();
                while !self.eat(pos, '}') {
                    let key = match &self.toks[*pos] {
                        Tok::Ident(key) | Tok::Str(key) => key.clone(),
                        other => panic!("settings-schema.js: unsupported object key {other:?}"),
                    };
                    *pos += 1;
                    let value = if self.eat(pos, ':') {
                        self.expr(pos, scope)
                    } else {
                        self.lookup(&key, scope)
                    };
                    object.insert(key, value);
                    if !self.eat(pos, ',') {
                        self.expect(pos, '}');
                        break;
                    }
                }
                Value::Object(object)
            }
            Tok::Ident(name) => match name.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                "null" | "undefined" => Value::Null,
                _ if self.toks.get(*pos) == Some(&Tok::Punct('(')) => {
                    *pos += 1;
                    let mut args = Vec::new();
                    while !self.eat(pos, ')') {
                        args.push(self.expr(pos, scope));
                        if !self.eat(pos, ',') {
                            self.expect(pos, ')');
                            break;
                        }
                    }
                    let Some(Binding::Func { params, body }) = self.globals.get(&name) else {
                        panic!("settings-schema.js: `{name}` is not a known helper function");
                    };
                    let local = params
                        .iter()
                        .cloned()
                        .zip(args.into_iter().chain(std::iter::repeat(Value::Null)))
                        .collect();
                    self.expr(&mut body.clone(), &local)
                }
                _ => self.lookup(&name, scope),
            },
            other => panic!("settings-schema.js: unsupported syntax {other:?} at token {pos}"),
        }
    }

    fn lookup(&self, name: &str, scope: &HashMap<String, Value>) -> Value {
        if let Some(value) = scope.get(name) {
            return value.clone();
        }
        match self.globals.get(name) {
            Some(Binding::Value(value)) => value.clone(),
            _ => panic!("settings-schema.js: unknown identifier `{name}`"),
        }
    }

    /// 跳过一个表达式(箭头函数体),停在顶层的 `;` 上。
    fn skip_to_semicolon(&self, pos: &mut usize) {
        let mut depth = 0i32;
        while *pos < self.toks.len() {
            match self.toks[*pos] {
                Tok::Punct('(' | '[' | '{') => depth += 1,
                Tok::Punct(')' | ']' | '}') => depth -= 1,
                Tok::Punct(';') if depth == 0 => return,
                _ => {}
            }
            *pos += 1;
        }
    }
}

/// 读出 `window.GqySettingsSchema = {...}` 的值。
fn read_schema() -> Value {
    let mut reader = Reader {
        toks: tokenize(SCHEMA),
        globals: HashMap::new(),
    };
    let mut pos = 0;
    let empty = HashMap::new();
    while pos < reader.toks.len() {
        match (&reader.toks[pos], reader.toks.get(pos + 1)) {
            (Tok::Ident(keyword), Some(Tok::Ident(name))) if keyword == "const" => {
                let name = name.clone();
                pos += 2;
                reader.expect(&mut pos, '=');
                let binding = if reader.is_arrow(pos) {
                    pos += 1;
                    let mut params = Vec::new();
                    while let Tok::Ident(param) = &reader.toks[pos] {
                        params.push(param.clone());
                        pos += 1;
                        reader.eat(&mut pos, ',');
                    }
                    reader.expect(&mut pos, ')');
                    assert_eq!(reader.toks[pos], Tok::Arrow);
                    pos += 1;
                    let body = pos;
                    reader.skip_to_semicolon(&mut pos);
                    Binding::Func { params, body }
                } else {
                    Binding::Value(reader.expr(&mut pos, &empty))
                };
                reader.globals.insert(name, binding);
            }
            (Tok::Ident(object), Some(Tok::Punct('.')))
                if object == "window"
                    && reader.toks.get(pos + 2)
                        == Some(&Tok::Ident("GqySettingsSchema".into())) =>
            {
                pos += 3;
                reader.expect(&mut pos, '=');
                return reader.expr(&mut pos, &empty);
            }
            _ => pos += 1,
        }
    }
    panic!("settings-schema.js: window.GqySettingsSchema assignment not found");
}

fn lookup_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(root, |value, segment| value.get(segment))
}

fn set_path(root: &mut Value, path: &str, value: Value) {
    let mut cursor = root;
    let segments: Vec<&str> = path.split('.').collect();
    for segment in &segments[..segments.len() - 1] {
        if !cursor.get(*segment).is_some_and(Value::is_object) {
            cursor[*segment] = Value::Object(Map::new());
        }
        cursor = &mut cursor[*segment];
    }
    cursor[segments[segments.len() - 1]] = value;
}

fn same(js: &Value, rust: &Value) -> bool {
    match (js, rust) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| same(a, b)))
        }
        _ => js == rust,
    }
}

/// 把 `value` 写进 `path` 后走一遍 Rust 的反序列化与序列化。
///
/// 序列化会省掉等于默认值的分区(`platforms`、`embedding`……),所以不能直接
/// 在 `to_value(AppConfig::default())` 里按路径找键;走一遍 serde 才知道 Rust
/// 认不认这个键、这个值。
fn round_trip(base: &Value, path: &str, value: Value) -> Result<Value, String> {
    let mut json = base.clone();
    set_path(&mut json, path, value);
    serde_json::from_value::<AppConfig>(json)
        .map(|config| serde_json::to_value(config).unwrap())
        .map_err(|error| error.to_string())
}

/// 与默认值同类型、但一定不等于它的探针值。
fn sentinel(default: &Value) -> Value {
    match default {
        Value::Bool(value) => Value::Bool(!value),
        Value::Number(number) => match number.as_u64() {
            Some(value) => serde_json::json!(value + 1),
            None => serde_json::json!(number.as_f64().unwrap() + 0.5),
        },
        Value::Array(items) => {
            let mut items = items.clone();
            items.push(match items.first() {
                Some(Value::Number(_)) => serde_json::json!(1),
                _ => Value::String("schema-probe".into()),
            });
            Value::Array(items)
        }
        _ => Value::String("schema-probe".into()),
    }
}

/// 表示方式不同、值其实一致的字段。
const REPRESENTATION_DIFFERS: &[(&str, &str)] = &[(
    "plugins.image_generation.output_dir",
    "Rust expands the default under $HOME; the schema shows it as ~/",
)];

/// 一个分区/插件定义里的全部字段:`fields` 加上分组的 `groups[].fields`。
fn fields_of(definition: &Value) -> Vec<&Value> {
    let direct = definition["fields"].as_array().into_iter().flatten();
    let grouped = definition["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|group| group["fields"].as_array().into_iter().flatten());
    direct.chain(grouped).collect()
}

/// 字段表 + 它们相对配置根的前缀。`qqPlugins` 不查:它的 settings 是各平台
/// 插件自己解析的自由 JSON,`AppConfig::default()` 里没有对应默认值。
fn schema_fields(schema: &Value) -> Vec<(String, &Value)> {
    let mut out = Vec::new();
    for section in schema["general"].as_array().unwrap() {
        out.extend(
            fields_of(section)
                .into_iter()
                .map(|field| (String::new(), field)),
        );
    }
    for (id, plugin) in schema["toolPlugins"].as_object().unwrap() {
        let prefix = format!("plugins.{id}.");
        out.extend(
            fields_of(plugin)
                .into_iter()
                .map(|field| (prefix.clone(), field)),
        );
    }
    for list in schema["qq"].as_object().unwrap().values() {
        for field in list.as_array().into_iter().flatten() {
            out.push(("platforms.qq.".to_string(), field));
        }
    }
    out
}

#[test]
fn settings_schema_matches_rust_config_defaults() {
    let schema = read_schema();
    let base = serde_json::to_value(AppConfig::default()).unwrap();
    let baseline = round_trip(&base, "config_version", base["config_version"].clone()).unwrap();
    let mut problems = Vec::new();
    for (prefix, field) in schema_fields(&schema) {
        let relative = field["path"]
            .as_str()
            .or_else(|| field["key"].as_str())
            .unwrap_or_else(|| panic!("schema field without path/key: {field}"));
        let path = format!("{prefix}{relative}");
        let kind = field["kind"].as_str().unwrap_or_default();
        let default = field.get("default").cloned().unwrap_or(Value::Null);

        // 键存不存在:写探针值,Rust 不认识的键会被 serde 静默丢掉;类型不对
        // 报错也说明键是认识的。
        let probe = sentinel(&default);
        if let Ok(value) = round_trip(&base, &path, probe.clone()) {
            if !lookup_path(&value, &path).is_some_and(|found| same(&probe, found)) {
                problems.push(format!("{path} ({kind}): no such config key"));
                continue;
            }
        }

        // model-ref 的默认值是 {provider_id, model} 一对,落在两个兄弟键上。
        if kind == "model-ref" || field.get("default").is_none() {
            continue;
        }
        if REPRESENTATION_DIFFERS
            .iter()
            .any(|(known, _)| *known == path)
        {
            continue;
        }
        // 空串在设置页里就是「未设置」,对应 Rust 的 `Option::None`(序列化时省略)。
        if default == Value::String(String::new()) && lookup_path(&baseline, &path).is_none() {
            continue;
        }
        match round_trip(&base, &path, default.clone()) {
            Ok(value) if same(&value, &baseline) => {}
            Ok(_) => problems.push(format!(
                "{path} ({kind}): schema default {default} != rust default {}",
                lookup_path(&baseline, &path).map_or("(omitted)".to_string(), Value::to_string),
            )),
            Err(error) => problems.push(format!(
                "{path} ({kind}): schema default {default} is rejected by rust: {error}"
            )),
        }
    }
    assert!(
        problems.is_empty(),
        "web/settings-schema.js drifted from src/config ({} fields):\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}
