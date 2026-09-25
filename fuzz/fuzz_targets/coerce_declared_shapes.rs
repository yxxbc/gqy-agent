//! 畸形参数修复（AGENTS.md §2.4）：任意 schema 与参数都不能 panic，
//! schema 声明为 string 的参数一个字节都不能碰。
#![no_main]

use gqy::fuzz_api::coerce_declared_shapes;
use libfuzzer_sys::fuzz_target;
use serde_json::{json, Value};

const TYPES: [&str; 6] = ["string", "array", "object", "integer", "number", "boolean"];

fuzz_target!(|input: (Vec<(u8, String)>, String)| {
    let (fields, fallback_schema) = input;
    let mut properties = serde_json::Map::new();
    let mut args = serde_json::Map::new();
    for (index, (kind, value)) in fields.iter().enumerate().take(16) {
        let name = format!("p{index}");
        properties.insert(name.clone(), json!({ "type": TYPES[*kind as usize % TYPES.len()] }));
        args.insert(name, Value::String(value.clone()));
    }
    let parameters = json!({ "properties": properties });
    let mut args = Value::Object(args);
    let before = args.clone();
    coerce_declared_shapes(&parameters, &mut args);

    for (name, schema) in properties.iter() {
        if schema["type"] == "string" {
            assert_eq!(args[name], before[name], "string 参数被改动");
        }
    }

    // schema 本身也可能是模型/插件给的任意 JSON
    if let Ok(schema) = serde_json::from_str::<Value>(&fallback_schema) {
        let mut args = before;
        coerce_declared_shapes(&schema, &mut args);
    }
});
