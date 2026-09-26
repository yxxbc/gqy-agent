//! cline 侧会话文件的发现与核对。
//!
//! CLI 不在事件流里报会话 id(只有 agentId/taskId),续传只能靠它落盘的会话
//! 文件:`~/.cline/data/sessions/<id>/<id>.json`,里面有 `session_id`、`prompt`、
//! `cwd`(09-26 实机核对)。布局是 CLI 的内部实现,版本一变就可能对不上;所以
//! 这里的一切都是「尽力而为」——认不出来只是不续传(下一轮全量重放),绝不报错。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// CLI 的数据目录:优先 `CLINE_DATA_DIR`,否则 `~/.cline/data`(与 CLI
/// `--data-dir` 的默认值一致;中转自己不传 `--data-dir`)。
pub(super) fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("CLINE_DATA_DIR") {
        return PathBuf::from(dir);
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".cline").join("data"))
        .unwrap_or_else(|| std::env::temp_dir().join("gqy-cline-data"))
}

fn sessions_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("sessions")
}

/// 一个能读出来的会话文件。
struct SessionFile {
    id: String,
    dir: PathBuf,
    json_path: PathBuf,
    json: serde_json::Value,
}

/// 会话目录下的全部会话文件;读不出来的条目直接跳过。
fn scan(data_dir: &Path) -> Vec<SessionFile> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(sessions_dir(data_dir)) else {
        return out;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(name) = dir.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        // 布局 A:`<id>/<id>.json`(09-26 实测)。布局 B 兜底:目录里唯一的
        // *.json(`.messages.json` 是消息流水,不是元数据)。
        let direct = dir.join(format!("{name}.json"));
        let mut candidates = Vec::new();
        if direct.is_file() {
            candidates.push(direct);
        } else if let Ok(files) = std::fs::read_dir(&dir) {
            for file in files.flatten() {
                let path = file.path();
                let is_meta = path.extension().is_some_and(|ext| ext == "json")
                    && !path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.ends_with(".messages.json"));
                if is_meta {
                    candidates.push(path);
                }
            }
        }
        for json_path in candidates {
            let Ok(text) = std::fs::read_to_string(&json_path) else {
                continue;
            };
            let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let id = json
                .get("session_id")
                .and_then(serde_json::Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| name.to_string());
            out.push(SessionFile {
                id,
                dir: dir.clone(),
                json_path,
                json,
            });
        }
    }
    out
}

/// `--id` 的目标还在不在。不在就别传 `--id` 了:CLI 会先抛错,白跑一轮。
pub(super) fn session_target_exists(data_dir: &Path, id: &str) -> bool {
    scan(data_dir).into_iter().any(|file| file.id == id)
}

/// 清空 顾清影 会话时的联动:尽力删除 cline 侧的会话目录
/// (`<sessions>/<id>/`)。存储布局是 CLI 的内部实现,删不到只记日志不报错——
/// 映射已丢弃,该会话无论如何不会再被续传。
pub(super) fn remove_session(data_dir: &Path, id: &str) {
    let dir = sessions_dir(data_dir).join(id);
    if !dir.exists() {
        return;
    }
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => tracing::info!(
            session = id,
            "removed the cline-side session for a cleared GQY session"
        ),
        Err(error) => tracing::warn!(
            %error,
            path = %dir.display(),
            "failed to remove a cline-side session (best effort)"
        ),
    }
}

/// 一轮**全量重放**结束后,按 `prompt` 找到本次会话的 id。
/// 判据:会话文件的 `prompt` 与我们发出的位置参数逐字相等,且 `cwd` 对得上
/// (同一轮里两条载荷完全相同已是极小概率,再叠 cwd 就够唯一了)。
pub(super) fn discover(data_dir: &Path, prompt: &str, workdir: &Path) -> Option<String> {
    let mut best: Option<(SystemTime, String)> = None;
    for file in scan(data_dir) {
        if !prompt_matches(&file, prompt) || !cwd_matches(&file, workdir) {
            continue;
        }
        let mtime = modified(&file.dir).unwrap_or(SystemTime::UNIX_EPOCH);
        if best
            .as_ref()
            .is_none_or(|(best_mtime, _)| mtime > *best_mtime)
        {
            best = Some((mtime, file.id));
        }
    }
    best.map(|(_, id)| id)
}

/// 续传核对:传了 `--id` 的那一轮,目标会话是不是真的被本轮写过。
/// 判据二选一:①目录/元数据/消息流三个 mtime 有落在本轮窗口内的(CLI 在
/// 回合中/结束时都会回写);②`prompt` 字段等于本轮载荷(把新提问写回同一
/// 字段的实现)。两条都不满足 ⇒ `--id` 没被兑现,调用方忘掉这条链、拉闸。
pub(super) fn touched_since(data_dir: &Path, id: &str, since: SystemTime, prompt: &str) -> bool {
    for file in scan(data_dir) {
        if file.id != id {
            continue;
        }
        let freshened = [
            Some(file.dir.clone()),
            Some(file.json_path.clone()),
            Some(file.dir.join(format!("{id}.messages.json"))),
        ]
        .into_iter()
        .flatten()
        .filter_map(|path| modified(&path))
        .any(|mtime| mtime >= since);
        if freshened || prompt_matches(&file, prompt) {
            return true;
        }
    }
    false
}

fn prompt_matches(file: &SessionFile, prompt: &str) -> bool {
    file.json.get("prompt").and_then(serde_json::Value::as_str) == Some(prompt)
}

fn cwd_matches(file: &SessionFile, workdir: &Path) -> bool {
    let Some(cwd) = file.json.get("cwd").and_then(serde_json::Value::as_str) else {
        return false;
    };
    if Path::new(cwd) == workdir {
        return true;
    }
    match (std::fs::canonicalize(cwd), std::fs::canonicalize(workdir)) {
        (Ok(recorded), Ok(expected)) => recorded == expected,
        _ => false,
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write_session(data_dir: &Path, id: &str, prompt: &str, cwd: &Path) {
        let dir = data_dir.join("sessions").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let json = json!({
            "version": 1,
            "session_id": id,
            "prompt": prompt,
            "cwd": cwd.display().to_string(),
            "workspace_root": cwd.display().to_string(),
            "status": "completed",
        });
        std::fs::write(dir.join(format!("{id}.json")), json.to_string()).unwrap();
        std::fs::write(
            dir.join(format!("{id}.messages.json")),
            json!([{ "role": "user", "content": prompt }]).to_string(),
        )
        .unwrap();
    }

    #[test]
    fn discovery_matches_prompt_and_workdir() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path();
        let workdir = temp.path().join("work");
        std::fs::create_dir_all(&workdir).unwrap();
        write_session(data, "1790405581688_8qylh", "hello relay", &workdir);

        assert_eq!(
            discover(data, "hello relay", &workdir).as_deref(),
            Some("1790405581688_8qylh")
        );
        assert!(discover(data, "another prompt", &workdir).is_none());
        assert!(discover(data, "hello relay", &temp.path().join("elsewhere")).is_none());
        assert!(session_target_exists(data, "1790405581688_8qylh"));
        assert!(!session_target_exists(data, "nope"));
    }

    #[test]
    fn resumed_target_counts_as_touched_when_prompt_matches() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path();
        let workdir = temp.path().join("work");
        std::fs::create_dir_all(&workdir).unwrap();
        write_session(data, "session_a", "second turn prompt", &workdir);

        // 未来时刻 ⇒ mtime 分支必然为假,只有 prompt 分支能救。
        let since = SystemTime::now() + std::time::Duration::from_secs(3600);
        assert!(touched_since(
            data,
            "session_a",
            since,
            "second turn prompt"
        ));
        assert!(!touched_since(data, "session_a", since, "unrelated prompt"));
        assert!(!touched_since(data, "missing", since, "second turn prompt"));
    }

    #[test]
    fn a_corrupt_session_file_is_skipped_not_fatal() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path();
        let dir = data.join("sessions").join("broken");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("broken.json"), "{not json").unwrap();
        assert!(discover(data, "anything", temp.path()).is_none());
    }
}
