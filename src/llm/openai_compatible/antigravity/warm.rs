//! agy 预热进程池:上一轮结束后就把下一轮的进程拉起来晾着,下一轮直接喂载荷。
//!
//! 为什么值得做:每轮都是一个全新的 agy 进程,实测「命令跑完」8.5 秒里只有
//! 2.2 秒是模型在生成,剩下 6.3 秒全是进程自己的启动开销(登录态、MCP 握手)。
//! 而 agy 在读 stdin 之前就把这些做完(4.45 秒处吐 init),所以提前拉起来晾着
//! 就能把这 6 秒藏掉。
//!
//! 三条硬约束,少一条就会出错:
//! ①**只在常驻 daemon 里预热**。单次 CLI 一退,晾着的进程就成了孤儿。
//! ②**钥匙要带上整个环境**。agy 把自己的环境原样传给 MCP 桥,桥在启动那一刻
//! 就把 `GQY_SESSION`(工具结果回哪个会话)与 `GQY_TURN_ORIGIN`(本轮发起来源,
//! 工具层的权限判据)读死了。换个会话、换种来源都不能复用这一个。
//! ③**只留一个**。晾着的是一个 agy 加一条 MCP 桥,多开就是拿内存换那 6 秒。
//!
//! 对不上钥匙、过了期、进程已经死了——这三种情况一律退回冷启动,也就是这个
//! 模块出现之前的行为,所以最坏情况只是没省到时间,不会把回合弄坏。

use crate::llm::openai_compatible::cli_relay::process::RelayProcess;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// 晾多久由 `plugins.antigravity.warm_idle_seconds` 决定(默认 300,0 = 不预热)。
// agy 的 `--print-timeout` 默认 24 小时,管不到这里;真正的代价是内存。

/// 复用的判据:命令行、环境、工作目录三者逐字节相同才是同一个位置的下一轮。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WarmKey {
    pub(super) binary: PathBuf,
    pub(super) args: Vec<String>,
    pub(super) env: Vec<(String, Option<String>)>,
    pub(super) workdir: PathBuf,
}

struct Slot {
    key: WarmKey,
    process: RelayProcess,
    generation: u64,
    at: Instant,
    ttl: Duration,
}

static POOL: Mutex<Option<Slot>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn pool() -> std::sync::MutexGuard<'static, Option<Slot>> {
    // 锁中毒只说明某个持锁者 panic 了,池子本身没有不变量要守——清空重来。
    POOL.lock().unwrap_or_else(|poisoned| {
        let mut guard = poisoned.into_inner();
        if let Some(slot) = guard.take() {
            slot.process.kill();
        }
        guard
    })
}

/// 钥匙对得上就把晾着的进程领走。对不上/过期的一律就地杀掉:留着也没人要。
pub(super) fn take(key: &WarmKey) -> Option<RelayProcess> {
    let mut guard = pool();
    let slot = guard.take()?;
    if slot.at.elapsed() >= slot.ttl {
        slot.process.kill();
        return None;
    }
    if &slot.key != key {
        slot.process.kill();
        return None;
    }
    Some(slot.process)
}

/// 把预热好的进程存进池子。已经有一个的话先杀掉旧的——只留一个。
pub(super) fn stash(key: WarmKey, process: RelayProcess, ttl: Duration) {
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    let mut guard = pool();
    if let Some(previous) = guard.take() {
        previous.process.kill();
    }
    *guard = Some(Slot {
        key,
        process,
        generation,
        at: Instant::now(),
        ttl,
    });
    drop(guard);
    // 到点没人领就自己收摊,免得白占一个 agy 加一条 MCP 桥。
    tokio::spawn(async move {
        tokio::time::sleep(ttl).await;
        let mut guard = pool();
        if guard
            .as_ref()
            .is_some_and(|slot| slot.generation == generation)
        {
            if let Some(slot) = guard.take() {
                slot.process.kill();
                tracing::debug!("antigravity warm process expired after {}s", ttl.as_secs());
            }
        }
    });
}

/// 杀掉当前晾着的进程(供应商被关掉、配置变更这类场合)。
pub(crate) fn discard() {
    if let Some(slot) = pool().take() {
        slot.process.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(args: &[&str]) -> WarmKey {
        WarmKey {
            binary: PathBuf::from("agy"),
            args: args.iter().map(|s| s.to_string()).collect(),
            env: vec![("GQY_SESSION".into(), Some("s1".into()))],
            workdir: PathBuf::from("/tmp"),
        }
    }

    /// 来源不同 = 钥匙不同。桥在启动时就把 GQY_TURN_ORIGIN 读死了,复用会让
    /// 工具层拿着别的轮的权限判据。
    #[test]
    fn turn_origin_difference_makes_a_different_key() {
        let mut a = key(&["--model", "x"]);
        let mut b = a.clone();
        a.env.push(("GQY_TURN_ORIGIN".into(), Some("qq:1".into())));
        b.env.push(("GQY_TURN_ORIGIN".into(), Some("qq:2".into())));
        assert_ne!(a, b);
    }

    /// 续传目标不同也必须算两把钥匙:`--conversation` 就在命令行里。
    #[test]
    fn resume_target_difference_makes_a_different_key() {
        assert_ne!(key(&["--conversation", "a"]), key(&["--conversation", "b"]));
    }

    /// 池子空着的时候领不到东西,但不能 panic。
    #[test]
    fn taking_from_an_empty_pool_is_none() {
        assert!(take(&key(&["--model", "x"])).is_none());
    }
}
