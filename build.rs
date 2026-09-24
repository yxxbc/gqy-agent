use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use base64::{engine::general_purpose, Engine as _};

const PROMPT_MASK: &[u8] = b"GqyPromptMask";

fn main() {
    println!("cargo:rerun-if-changed=src/prompts/gqy.md");
    println!("cargo:rerun-if-changed=src/prompts/gqy.hint.md");
    println!("cargo:rerun-if-changed=src/prompts/gqy-dialogs.md");
    println!("cargo:rerun-if-changed=assets/o200k_base.tiktoken");
    println!("cargo:rerun-if-changed=assets/jieba/dict.txt");
    // Rerun on any source or frontend change so GQY_BUILD_ID uniquely
    // identifies a build; the CLI uses it to detect (and restart) a daemon
    // left running from an older build.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=web");
    println!("cargo:rerun-if-env-changed=GQY_BUILD_ID");
    let build_id = env::var("GQY_BUILD_ID").unwrap_or_else(|_| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
            .to_string()
    });
    assert!(
        !build_id.is_empty()
            && build_id.len() <= 128
            && build_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')),
        "GQY_BUILD_ID must contain 1 to 128 ASCII letters, digits, dots, underscores or hyphens"
    );
    println!("cargo:rustc-env=GQY_BUILD_ID={build_id}");

    let obfuscate = |path: &str| {
        let content = fs::read(path).unwrap_or_else(|_| panic!("read {path}"));
        let encoded = content
            .into_iter()
            .enumerate()
            .map(|(index, byte)| byte ^ PROMPT_MASK[index % PROMPT_MASK.len()])
            .collect::<Vec<_>>();
        base64_encode(&encoded)
    };
    let prompt = obfuscate("src/prompts/gqy.md");
    let hint = obfuscate("src/prompts/gqy.hint.md");
    let dialogs = obfuscate("src/prompts/gqy-dialogs.md");
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR is set by cargo");
    let dest = Path::new(&out_dir).join("default_gqy_prompt.rs");
    fs::write(
        dest,
        format!(
            "const PROMPT_MASK: &[u8] = b\"GqyPromptMask\";\n\
             const OBFUSCATED_DEFAULT_SYSTEM_PROMPT: &str = \"{prompt}\";\n\
             const OBFUSCATED_DEFAULT_GQY_HINT: &str = \"{hint}\";\n\
             const OBFUSCATED_DEFAULT_GQY_DIALOGS: &str = \"{dialogs}\";\n"
        ),
    )
    .expect("write generated prompt asset");

    build_tool_description_index(&out_dir);
    build_web_asset_index(&out_dir);
    build_o200k_vocab();
    build_jieba_index();
}

/// Every `src/tools/descriptions/*.json` except `groups.json` becomes one
/// `include_str!` entry. The list used to be hand-maintained, and a JSON file
/// missing from it silently fell back to the Rust placeholder description.
fn build_tool_description_index(out_dir: &str) {
    const DIR: &str = "src/tools/descriptions";
    println!("cargo:rerun-if-changed={DIR}");
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");
    let mut files = fs::read_dir(DIR)
        .expect("read tool descriptions directory")
        .map(|entry| entry.expect("tool description entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter(|path| path.file_name().is_some_and(|name| name != "groups.json"))
        .collect::<Vec<_>>();
    files.sort();
    let mut source = String::from("const TOOL_DESCRIPTION_FILES: &[(&str, &str)] = &[\n");
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy();
        let absolute = Path::new(&manifest_dir).join(path);
        source.push_str(&format!(
            "    ({name:?}, include_str!({:?})),\n",
            absolute.display().to_string()
        ));
    }
    source.push_str("];\n");
    fs::write(Path::new(out_dir).join("tool_description_files.rs"), source)
        .expect("write generated tool description index");
}

/// Every servable file under `web/` becomes one `(url, bytes, content-type)`
/// entry, served at `/<path relative to web/>`. Adding a frontend file used to
/// take three hand edits in `src/web` (a constant, a handler, a route) plus a
/// `?v=` rewrite; forgetting one only showed up as a 404 in the browser.
///
/// Skipped: `vendor/` (gzip bodies and CORS preflight, served by hand),
/// `index.html` and `fence-frame.html` (their own handlers: versioned rewrite
/// and a sandbox CSP), and anything that is not a frontend asset.
fn build_web_asset_index(out_dir: &str) {
    const ROOT: &str = "web";
    const SPECIAL: &[&str] = &["index.html", "fence-frame.html"];
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");
    let mut files = Vec::new();
    collect_web_assets(Path::new(ROOT), &mut files);
    files.sort();
    let mut source = String::from("static WEB_ASSETS: &[(&str, &[u8], &str)] = &[\n");
    for path in &files {
        let relative = path
            .strip_prefix(ROOT)
            .expect("web asset under web/")
            .to_string_lossy()
            .replace('\\', "/");
        if SPECIAL.contains(&relative.as_str()) {
            continue;
        }
        let Some(content_type) = web_content_type(path) else {
            continue;
        };
        let absolute = Path::new(&manifest_dir).join(path);
        source.push_str(&format!(
            "    ({:?}, include_bytes!({:?}), {content_type:?}),\n",
            format!("/{relative}"),
            absolute.display().to_string()
        ));
    }
    source.push_str("];\n");
    fs::write(Path::new(out_dir).join("web_assets.rs"), source)
        .expect("write generated web asset index");
}

fn collect_web_assets(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|_| panic!("read {}", dir.display())) {
        let path = entry.expect("web asset entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "vendor") {
                collect_web_assets(&path, files);
            }
        } else {
            files.push(path);
        }
    }
}

fn web_content_type(path: &Path) -> Option<&'static str> {
    Some(match path.extension()?.to_str()? {
        "js" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        _ => return None,
    })
}

fn build_jieba_index() {
    let source = fs::read_to_string("assets/jieba/dict.txt").expect("read Jieba dictionary");
    let mut entries = BTreeMap::<String, u64>::new();
    for (line_number, line) in source.lines().enumerate() {
        let mut fields = line.split_whitespace();
        let word = fields.next().expect("Jieba dictionary word");
        let frequency = fields
            .next()
            .expect("Jieba dictionary frequency")
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("invalid Jieba frequency on line {}", line_number + 1));
        entries.insert(word.to_string(), frequency);
    }
    let total = entries.values().copied().sum::<u64>();
    let max_word_chars = entries
        .keys()
        .map(|word| word.chars().count())
        .max()
        .expect("Jieba dictionary is not empty");
    let destination = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("jieba.fst");
    let mut file = fs::File::create(destination).expect("create compact Jieba index");
    use std::io::Write as _;
    file.write_all(&total.to_le_bytes())
        .expect("write Jieba frequency total");
    file.write_all(
        &u32::try_from(max_word_chars)
            .expect("maximum Jieba word length fits in u32")
            .to_le_bytes(),
    )
    .expect("write maximum Jieba word length");
    let mut builder = fst::MapBuilder::new(file).expect("create Jieba FST builder");
    for (word, frequency) in entries {
        builder
            .insert(word, frequency)
            .expect("insert sorted Jieba entry");
    }
    builder.finish().expect("finish compact Jieba index");
}

fn build_o200k_vocab() {
    let source =
        fs::read_to_string("assets/o200k_base.tiktoken").expect("read o200k_base vocabulary");
    let mut output = Vec::with_capacity(source.len() / 2);
    let mut tokens = HashSet::with_capacity(199_998);
    let mut count = 0usize;
    for (expected_rank, line) in source.lines().enumerate() {
        let mut parts = line.split(' ');
        let token = general_purpose::STANDARD
            .decode(parts.next().expect("vocabulary token"))
            .expect("decode vocabulary token");
        assert!(tokens.insert(token.clone()), "duplicate o200k token");
        let rank = parts
            .next()
            .expect("vocabulary rank")
            .parse::<usize>()
            .expect("parse vocabulary rank");
        assert_eq!(rank, expected_rank, "o200k ranks must be sequential");
        let len = u16::try_from(token.len()).expect("token length fits in u16");
        output.extend_from_slice(&len.to_le_bytes());
        output.extend_from_slice(&token);
        count += 1;
    }
    assert_eq!(count, 199_998, "unexpected o200k vocabulary size");
    assert_eq!(tokens.len(), count, "o200k tokens must be unique");

    let destination =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("o200k_base.bin");
    fs::write(destination, output).expect("write compact o200k vocabulary");
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(TABLE[(((first & 0b0000_0011) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[(((second & 0b0000_1111) << 2) | (third >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(third & 0b0011_1111) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}
