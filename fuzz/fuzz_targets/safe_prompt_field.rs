//! 不可信文本进提示词前的转义（AGENTS.md §4.1）：输出不得含换行或尖括号，
//! 否则昵称/正文就能伪造记录行或 XML 标签。
#![no_main]

use gqy::fuzz_api::safe_prompt_field;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|value: &str| {
    let safe = safe_prompt_field(value);
    assert!(
        !safe.contains(['\n', '\r', '<', '>']),
        "未转义的控制字符: {safe:?}"
    );
});
