//! Seatbelt（macOS）profile 生成：`SandboxPolicy` → SBPL 文本。纯函数，任何平台都能测。
//!
//! 语法与语义都是实测定下来的（macOS 26/27，探针记录见 docs/design/2026-09-27-macos-sandbox.md）：
//!
//! - **规则顺序即优先级**：同一类操作匹配到多条规则时，**后写的那条赢**。所以
//!   profile 只能是「先 deny 一片，再把要放行的子树 allow 回来」；反过来写（allow
//!   在前、deny 在后）等于把 allow 全部吃掉——`deny file-write*` 后面接
//!   `allow file-write* (subpath X)` 才是「只准写 X」的意思。
//! - **规则里的路径不做解析**：操作侧的路径由内核解析（`/tmp` 是软链，落到
//!   `/private/tmp`），规则里写 `/tmp/...` 则一条都匹配不上。所有根先 canonicalize。
//! - **`(deny default)` + 逐项 allow 走不通**：读侧一旦默认全禁，进程连 dyld 都读不到，
//!   exec 当场被拒。基线只能是 `(allow default)`，再分「写白名单」「读黑名单」两个方向收口。
//! - `(import "system.sb")` 在内联 profile 里不可用（`-p` 下 execvp 直接被拒），不指望它。
//!
//! 产出结构与上面那条优先级契约一一对应：
//!
//! ```text
//! (version 1)
//! (allow default)
//! (deny file-write*)                        ← 写：默认全禁
//! (allow file-write* (subpath …) …)         ← 再逐条放回策略里的可写根 + 输出设备
//! (deny file-read* (subpath "/Users"))      ← 读：先禁掉所有人的家
//! (allow file-read* (subpath …) …)          ← 再放回策略里的只读/可写根
//! (deny file-read* file-write* (subpath …)) ← 凭证兜底：放行根里也不给
//! ```
//!
//! 读侧为什么是 `/Users` 整棵：Linux 那份策略的读集本来就只列了系统目录与策略里的
//! 根，管理员的家不在其中。macOS 的家都挂 `/Users` 下，整棵禁掉再放回策略根，就
//! 得到同一件事——成员读不到管理员的家与 `~/.gqy` 的配置库。

use super::SandboxPolicy;
use std::path::{Path, PathBuf};

/// macOS 的「家」都挂这里（Linux 的 `/home` 对应物）。
const HOME_ROOT: &str = "/Users";

/// 写白名单的设备兜底：stdout/stderr 是管道时不受限，是终端或文件时就要这几条。
/// `/dev/dtracehelper` 留给 dyld 与调试器。
const WRITE_DEVICES: &[&str] = &[
    "/dev/null",
    "/dev/zero",
    "/dev/stdout",
    "/dev/stderr",
    "/dev/tty",
    "/dev/dtracehelper",
];

/// 家目录下的凭证目录（相对家）。即使被策略放行（管理员 `/sandbox ~` 这种），也
/// 一律不给读写。哪条会连累策略里的放行根就自动让位，见 [`credential_paths`]。
const CREDENTIALS: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".netrc",
    ".kube",
    ".docker/config.json",
    ".config/gh",
    "Library/Keychains",
    ".gqy",
];

/// 策略 → SBPL。`home` 只用于展开凭证清单；没有家目录（容器里跑测试）就跳过那一节。
pub(super) fn profile_text(policy: &SandboxPolicy, home: Option<&Path>) -> String {
    let writable = collapse(policy.read_write.iter().map(|path| resolve(path)));
    let readable = collapse(
        policy
            .read_only
            .iter()
            .chain(policy.read_write.iter())
            .map(|path| resolve(path)),
    );

    let mut text = String::from("(version 1)\n(allow default)\n");
    text.push_str("(deny file-write*)\n");
    // 空清单不能写 `(allow file-write*)`——那等于把上面那条 deny 全放开。
    if !writable.is_empty() {
        text.push_str("(allow file-write*");
        for path in &writable {
            text.push_str(&format!(" (subpath {})", quote(path)));
        }
        for device in WRITE_DEVICES {
            text.push_str(&format!(" (literal {})", quote(Path::new(device))));
        }
        text.push_str(")\n");
    }
    text.push_str(&format!(
        "(deny file-read* (subpath {}))\n",
        quote(Path::new(HOME_ROOT))
    ));
    if !readable.is_empty() {
        text.push_str("(allow file-read*");
        for path in &readable {
            text.push_str(&format!(" (subpath {})", quote(path)));
        }
        text.push_str(")\n");
    }
    if let Some(home) = home {
        for path in credential_paths(home, &readable) {
            text.push_str(&format!(
                "(deny file-read* file-write* (subpath {}))\n",
                quote(&path)
            ));
        }
    }
    text
}

/// 凭证目录里哪些该写进 deny：策略放行的根落在它里面时让位（成员工作区就在
/// `~/.gqy` 下，写死这条会把工作区一起锁掉——「后写的赢」意味着 deny 会吃掉 allow）。
pub(super) fn credential_paths(home: &Path, allowed: &[PathBuf]) -> Vec<PathBuf> {
    CREDENTIALS
        .iter()
        .map(|suffix| resolve(&home.join(suffix)))
        .filter(|credential| {
            !allowed
                .iter()
                .any(|root| root.starts_with(credential.as_path()))
        })
        .collect()
}

/// 已存在的路径 canonicalize（跟软链走）；不存在的原样留着——规则按路径匹配，
/// 不需要文件真的在（Landlock 那边的授权根是 open 出来的，所以那边会失败关闭）。
fn resolve(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// 去掉被别的条目覆盖的路径，顺便去重：父目录的 `subpath` 已经管住了子目录。
fn collapse(paths: impl Iterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut all: Vec<PathBuf> = paths.collect();
    all.sort();
    all.dedup();
    let mut kept: Vec<PathBuf> = Vec::with_capacity(all.len());
    for path in all {
        if kept.iter().any(|parent| path.starts_with(parent)) {
            continue;
        }
        kept.push(path);
    }
    kept
}

/// SBPL 字符串字面量：路径里的引号与反斜杠转义（家目录里真有人放这些字符）。
fn quote(path: &Path) -> String {
    let mut out = String::from("\"");
    for character in path.to_string_lossy().chars() {
        if character == '"' || character == '\\' {
            out.push('\\');
        }
        out.push(character);
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(read_only: &[&str], read_write: &[&str]) -> SandboxPolicy {
        SandboxPolicy {
            read_only: read_only.iter().map(PathBuf::from).collect(),
            read_write: read_write.iter().map(PathBuf::from).collect(),
            ..SandboxPolicy::default()
        }
    }

    /// 顺序契约：deny 必须在 allow 之前，否则 allow 会被吃掉（实测：反过来写全拦）。
    #[test]
    fn denies_come_before_the_allows() {
        let text = profile_text(&policy(&["/usr", "/etc"], &["/tmp"]), None);
        let write_deny = text.find("(deny file-write*)").expect("写 deny");
        let write_allow = text.find("(allow file-write*").expect("写 allow");
        let read_deny = text
            .find("(deny file-read* (subpath \"/Users\"))")
            .expect("读 deny");
        let read_allow = text.find("(allow file-read*").expect("读 allow");
        assert!(write_deny < write_allow, "{text}");
        assert!(read_deny < read_allow, "{text}");
        assert!(write_allow < read_deny, "{text}");
    }

    /// 可写集只进写白名单；只读集进读白名单，两个方向的边不串。
    #[test]
    fn writable_roots_never_land_in_the_read_only_half() {
        let text = profile_text(&policy(&["/usr"], &["/tmp"]), None);
        let write_allow =
            &text[text.find("(allow file-write*").unwrap()..text.find("(deny file-read*").unwrap()];
        assert!(
            write_allow.contains(&quote(&resolve(Path::new("/tmp")))),
            "{text}"
        );
        assert!(!write_allow.contains(&quote(Path::new("/usr"))), "{text}");
        let read_allow = &text[text.find("(allow file-read*").unwrap()..];
        assert!(
            read_allow.contains(&quote(&resolve(Path::new("/usr")))),
            "{text}"
        );
        assert!(
            read_allow.contains(&quote(&resolve(Path::new("/tmp")))),
            "{text}"
        );
    }

    /// 空策略：不许产出「全放开」的两条 allow。
    #[test]
    fn an_empty_policy_stays_locked() {
        let text = profile_text(&policy(&[], &[]), None);
        assert!(text.contains("(deny file-write*)"), "{text}");
        assert!(!text.contains("(allow file-write*)"), "{text}");
        assert!(!text.contains("(allow file-read*)"), "{text}");
    }

    /// 凭证兜底在最后：策略放行了整个家，`~/.ssh` 仍然不给。
    #[test]
    fn credentials_survive_a_grant_of_the_whole_home() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let text = profile_text(&policy(&[], &[home.to_str().unwrap()]), Some(home));
        assert!(
            text.contains(&format!(
                "(deny file-read* file-write* (subpath {}))",
                quote(&home.join(".ssh"))
            )),
            "{text}"
        );
    }

    /// 凭证目录托着策略里的放行根时让位：成员工作区在 `~/.gqy` 下，不能被锁掉。
    #[test]
    fn a_credential_that_holds_a_grant_steps_aside() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let workspace = home.join(".gqy/home/member/workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let allowed = vec![workspace.canonicalize().unwrap()];
        let credentials = credential_paths(home, &allowed);
        assert!(
            !credentials.contains(&home.join(".gqy")),
            "工作区在 ~/.gqy 下,这条 deny 要让位: {credentials:?}"
        );
        assert!(
            credentials.contains(&home.join(".ssh")),
            "别的凭证不受影响: {credentials:?}"
        );
    }

    /// 父目录已放行时不再重复写子目录（profile 短一点，读起来也清楚）。
    #[test]
    fn nested_roots_collapse_into_the_parent() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let child = root.join("child");
        std::fs::create_dir_all(&child).unwrap();
        let text = profile_text(
            &policy(&[], &[root.to_str().unwrap(), child.to_str().unwrap()]),
            None,
        );
        // profile 里是解析过的路径（`/var/folders/…` 在 macOS 上落到 `/private/var/…`）。
        let root = root.canonicalize().unwrap();
        let child = child.canonicalize().unwrap();
        // 可写根在两节里各出现一次（写白名单一次、读白名单一次），子目录一次都不该有。
        let write_half =
            &text[text.find("(allow file-write*").unwrap()..text.find("(deny file-read*").unwrap()];
        let read_half = &text[text.find("(allow file-read*").unwrap()..];
        assert_eq!(write_half.matches(&quote(&root)).count(), 1, "{text}");
        assert_eq!(read_half.matches(&quote(&root)).count(), 1, "{text}");
        assert!(!text.contains(&quote(&child)), "{text}");
    }

    /// 软链根写进 profile 的是真身：规则里的路径不做解析，写软链等于没写。
    #[test]
    fn roots_are_resolved_before_they_reach_the_profile() {
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real");
        let link = temp.path().join("link");
        std::fs::create_dir_all(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let text = profile_text(&policy(&[], &[link.to_str().unwrap()]), None);
        let resolved = real.canonicalize().unwrap();
        assert!(text.contains(&quote(&resolved)), "{text}");
        assert!(!text.contains(&quote(&link)), "{text}");
    }
}
