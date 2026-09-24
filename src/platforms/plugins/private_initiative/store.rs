//! 主动私聊的账本：待执行的计划 + 每人每天已发次数，存在 `state/qq_private_initiative.json`。
//!
//! 内存里缓存一份（按文件路径分，测试各用各的临时目录），每次改动整份原子写回。
//! 取消计划发生在每条私聊的 ingress 钩子里，必须便宜：没有计划时不碰磁盘。

use crate::paths::GqyPaths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const FILE_NAME: &str = "qq_private_initiative.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Plan {
    pub(super) account: String,
    pub(super) user: String,
    /// 计划执行时刻（unix 秒）。
    pub(super) at: i64,
    pub(super) topic: String,
    pub(super) planned_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Sent {
    date: String,
    count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Ledger {
    #[serde(default)]
    plans: BTreeMap<String, Plan>,
    #[serde(default)]
    sent: BTreeMap<String, Sent>,
}

pub(super) fn key(account: &str, user: &str) -> String {
    format!("{account}:{user}")
}

fn file(paths: &GqyPaths) -> PathBuf {
    paths.state_dir.join(FILE_NAME)
}

fn cache() -> &'static Mutex<HashMap<PathBuf, Ledger>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Ledger>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn load(path: &Path) -> Ledger {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save(path: &Path, ledger: &Ledger) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(ledger)?)
        .with_context(|| format!("writing {}", temp.display()))?;
    std::fs::rename(&temp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

/// 在锁内读改写；`change` 返回 true 才落盘。
fn update<T>(paths: &GqyPaths, change: impl FnOnce(&mut Ledger) -> (T, bool)) -> Result<T> {
    let path = file(paths);
    let mut cache = cache().lock().unwrap();
    let ledger = cache.entry(path.clone()).or_insert_with(|| load(&path));
    let (value, dirty) = change(ledger);
    if dirty {
        save(&path, ledger)?;
    }
    Ok(value)
}

/// 同一个人只留一份计划：新的覆盖旧的。
pub(super) fn save_plan(paths: &GqyPaths, plan: Plan) -> Result<()> {
    update(paths, |ledger| {
        ledger.plans.insert(key(&plan.account, &plan.user), plan);
        ((), true)
    })
}

/// 对方先说话了：取消计划。返回是否真的取消了一份。
pub(super) fn cancel(paths: &GqyPaths, account: &str, user: &str) -> Result<bool> {
    update(paths, |ledger| {
        let removed = ledger.plans.remove(&key(account, user)).is_some();
        (removed, removed)
    })
}

/// 取出到点的计划（`at <= now`）。超过 `window` 秒的算错过，一并移除、不返回。
/// **先记账再发送**：取出即从账本删除，发送失败也不会重试。
pub(super) fn take_due(paths: &GqyPaths, now: i64, window: i64) -> Result<Vec<Plan>> {
    update(paths, |ledger| {
        let due_keys = ledger
            .plans
            .iter()
            .filter(|(_, plan)| plan.at <= now)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        let mut due = Vec::new();
        for key in &due_keys {
            if let Some(plan) = ledger.plans.remove(key) {
                if now - plan.at <= window {
                    due.push(plan);
                }
            }
        }
        (due, !due_keys.is_empty())
    })
}

pub(super) fn sent_today(paths: &GqyPaths, account: &str, user: &str, date: &str) -> Result<u32> {
    update(paths, |ledger| {
        let count = ledger
            .sent
            .get(&key(account, user))
            .filter(|sent| sent.date == date)
            .map_or(0, |sent| sent.count);
        (count, false)
    })
}

pub(super) fn record_sent(paths: &GqyPaths, account: &str, user: &str, date: &str) -> Result<()> {
    update(paths, |ledger| {
        let entry = ledger.sent.entry(key(account, user)).or_default();
        if entry.date != date {
            *entry = Sent {
                date: date.to_string(),
                count: 0,
            };
        }
        entry.count += 1;
        ((), true)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::tests::shared::test_paths;

    fn plan(user: &str, at: i64) -> Plan {
        Plan {
            account: "1".to_string(),
            user: user.to_string(),
            at,
            topic: "问问考试".to_string(),
            planned_at: 0,
        }
    }

    #[test]
    fn due_plans_are_taken_once_and_missed_ones_are_dropped() {
        let temp = tempfile::tempdir().unwrap();
        let paths = test_paths(temp.path());
        save_plan(&paths, plan("a", 1_000)).unwrap();
        save_plan(&paths, plan("b", 100)).unwrap(); // 早就过了窗口
        save_plan(&paths, plan("c", 5_000)).unwrap(); // 还没到

        let due = take_due(&paths, 1_050, 300).unwrap();
        assert_eq!(
            due.iter()
                .map(|plan| plan.user.as_str())
                .collect::<Vec<_>>(),
            ["a"]
        );
        assert!(
            take_due(&paths, 1_050, 300).unwrap().is_empty(),
            "taken means gone"
        );
        assert!(cancel(&paths, "1", "c").unwrap());
        assert!(
            !cancel(&paths, "1", "b").unwrap(),
            "missed plan was dropped"
        );
    }

    #[test]
    fn daily_counts_reset_on_a_new_date() {
        let temp = tempfile::tempdir().unwrap();
        let paths = test_paths(temp.path());
        record_sent(&paths, "1", "a", "2026-09-25").unwrap();
        record_sent(&paths, "1", "a", "2026-09-25").unwrap();
        assert_eq!(sent_today(&paths, "1", "a", "2026-09-25").unwrap(), 2);
        assert_eq!(sent_today(&paths, "1", "a", "2026-09-26").unwrap(), 0);
    }
}
