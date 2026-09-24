//! 插件用的小账本：一份 JSON 文件 + 进程内缓存。
//!
//! 读改写在同一把锁里完成，改了才整份原子写回（先写临时文件再改名）。缓存按文件
//! 路径分（测试各用各的临时目录），第一次访问时从磁盘读。热路径上的只读查询
//! （比如每条群消息查黑名单）只碰内存。
//!
//! 适合小而低频改动的状态；追加型、会无限增长的数据别放这里（AGENTS §3.2）。

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::any::Any;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

type Cache = HashMap<PathBuf, Box<dyn Any + Send>>;

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn load<T: Default + DeserializeOwned>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("writing {}", temp.display()))?;
    std::fs::rename(&temp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

/// 在锁内读改写 `path` 处的账本；`change` 返回 `(结果, 是否改动)`，改动了才落盘。
pub(crate) fn update<T, R>(path: &Path, change: impl FnOnce(&mut T) -> (R, bool)) -> Result<R>
where
    T: Default + Serialize + DeserializeOwned + Send + 'static,
{
    let mut cache = cache().lock().unwrap();
    let entry = cache
        .entry(path.to_path_buf())
        .or_insert_with(|| Box::new(load::<T>(path)));
    let ledger = entry
        .downcast_mut::<T>()
        .context("one ledger file is used with two different types")?;
    let (value, dirty) = change(ledger);
    if dirty {
        save(path, ledger)?;
    }
    Ok(value)
}

/// 只读查询：不落盘。
pub(crate) fn read<T, R>(path: &Path, query: impl FnOnce(&T) -> R) -> Result<R>
where
    T: Default + Serialize + DeserializeOwned + Send + 'static,
{
    update(path, |ledger: &mut T| (query(ledger), false))
}
