//! macOS 后端：把 Seatbelt profile 装到子进程上（libsandbox，私有 API）。
//!
//! profile 文本由 `super::seatbelt` 生成（纯函数、任何平台都能测）；这里只做两件事：
//! `prepare` 在**父进程**把文本编译成 `sandbox_profile_t`（能分配、能报错、能记账），
//! `apply` 在**子进程**里只调一次 `sandbox_apply`（不分配，符合 backend.rs 的约定）。
//! 规则随 exec 继承，命令与它再起的子进程都受限，daemon 本身不受影响。
//!
//! 符号从 dyld 共享缓存里取：macOS 27 上 `/usr/lib/libsandbox.dylib` 已经不在磁盘上，
//! 但 `dlopen` 这个路径仍拿得到句柄（`sandbox-exec` 走的也是这套）。取不到、或
//! profile 编译不过，都**失败关闭**：`apply` 报错，spawn 随之失败，不裸奔。

use super::seatbelt::profile_text;
use super::SandboxPolicy;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::sync::OnceLock;

const LIBRARY: &str = "/usr/lib/libsandbox.dylib";
/// Seatbelt profile 语言的版本（profile 头一行 `(version 1)`）；`probe()` 报的就是它。
const PROFILE_ABI: i64 = 1;

type CompileString =
    unsafe extern "C" fn(*const c_char, *mut c_void, *mut *mut c_char) -> *mut c_void;
type ApplyProfile = unsafe extern "C" fn(*const c_void) -> c_int;
type FreeProfile = unsafe extern "C" fn(*mut c_void);
type FreeError = unsafe extern "C" fn(*mut c_char);

struct Api {
    compile: CompileString,
    apply: ApplyProfile,
    free_profile: FreeProfile,
    free_error: FreeError,
}

/// 一次 dlsym，全程复用：`dlopen`/`dlsym` 都要分配，别在 `apply` 里做。
fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| unsafe {
        let library = CString::new(LIBRARY).ok()?;
        let handle = libc::dlopen(library.as_ptr(), libc::RTLD_NOW);
        if handle.is_null() {
            return None;
        }
        let symbol = |name: &str| -> *mut c_void {
            match CString::new(name) {
                Ok(name) => libc::dlsym(handle, name.as_ptr()),
                Err(_) => std::ptr::null_mut(),
            }
        };
        let compile = symbol("sandbox_compile_string");
        let apply = symbol("sandbox_apply");
        let free_profile = symbol("sandbox_free_profile");
        let free_error = symbol("sandbox_free_error");
        if compile.is_null() || apply.is_null() || free_profile.is_null() {
            return None;
        }
        Some(Api {
            compile: std::mem::transmute(compile),
            apply: std::mem::transmute(apply),
            free_profile: std::mem::transmute(free_profile),
            free_error: std::mem::transmute(free_error),
        })
    })
    .as_ref()
}

/// libsandbox 在手就算有后端。`Some(1)` = Seatbelt profile 语言第 1 版。
pub(super) fn probe() -> Option<i64> {
    api().is_some().then_some(PROFILE_ABI)
}

/// fork 前编译好的规则集。指针存成 `usize`：`pre_exec` 的闭包要求 Send + Sync。
pub(super) struct Rules {
    /// `sandbox_profile_t*`；0 = 没编译出来（缺库或文本被拒），apply 时失败关闭。
    profile: usize,
}

impl Rules {
    pub(super) fn prepare(policy: &SandboxPolicy) -> Self {
        let Some(api) = api() else {
            tracing::warn!("seatbelt: libsandbox unavailable; sandboxed commands will be refused");
            return Self { profile: 0 };
        };
        let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf());
        let text = profile_text(policy, home.as_deref());
        let Ok(text) = CString::new(text) else {
            tracing::warn!("seatbelt: profile carries a NUL byte");
            return Self { profile: 0 };
        };
        let mut error: *mut c_char = std::ptr::null_mut();
        let profile = unsafe { (api.compile)(text.as_ptr(), std::ptr::null_mut(), &mut error) };
        if !error.is_null() {
            let message = unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned();
            unsafe { (api.free_error)(error) };
            tracing::warn!(error = %message, "seatbelt: profile rejected");
        }
        if profile.is_null() {
            return Self { profile: 0 };
        }
        Self {
            profile: profile as usize,
        }
    }

    /// 子进程里跑：把编译好的 profile 套到自己身上。出错返回 errno 风格的
    /// io::Error（不分配），spawn 随之失败。
    pub(super) fn apply(&self) -> std::io::Result<()> {
        let Some(api) = api() else {
            return Err(std::io::Error::from_raw_os_error(libc::ENOTSUP));
        };
        if self.profile == 0 {
            return Err(std::io::Error::from_raw_os_error(libc::EINVAL));
        }
        let rc = unsafe { (api.apply)(self.profile as *const c_void) };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
}

impl Drop for Rules {
    fn drop(&mut self) {
        // 父进程侧回收（子进程 fork 时已经拿到自己的副本，各自独立）。
        if self.profile != 0 {
            if let Some(api) = api() {
                unsafe { (api.free_profile)(self.profile as *mut c_void) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 探测与规则集对应：`probe()` 说有后端，`prepare` 就不该产出空的规则集。
    ///
    /// 真正「装上去有没有用」由 `tests.rs` 在**子进程**里验——Seatbelt 一旦套上就
    /// 摘不下来（实测第二次 sandbox_init 直接失败），在测试进程里 apply 会把同一个
    /// 进程里别的用例一起关进去。
    #[test]
    fn probe_agrees_with_prepare() {
        let policy = SandboxPolicy {
            read_write: vec![std::env::temp_dir()],
            ..SandboxPolicy::default()
        };
        assert_eq!(probe().is_some(), Rules::prepare(&policy).profile != 0);
    }
}
