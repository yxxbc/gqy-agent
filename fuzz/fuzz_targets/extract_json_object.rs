//! 模型输出是不可信输入：任意文本都不能让提取器 panic，
//! 提取出的切片必须是以花括号开头结尾的原文子串。
#![no_main]

use gqy::fuzz_api::extract_json_object;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|content: &str| {
    if let Some(object) = extract_json_object(content) {
        assert!(object.starts_with('{') && object.ends_with('}'));
        assert!(content.contains(object));
    }
});
