# macOS 沙盒后端（Seatbelt）方案稿

> 状态：已施工（2026-09-27）。对应 `docs/plan/backlog.md` §12 / `todolist.md` 的「macOS 沙盒后端（Seatbelt）」。代码落在 `src/tools/sandbox/`：`seatbelt.rs`（策略 → SBPL，纯函数）+ `macos.rs`（libsandbox 胶水）。
> 需求：macOS 上 `/sandbox` 与会话沙盒此前是**失败关闭**（`unsupported.rs` 直接 `ENOTSUP`），绑了沙盒的会话命令一条都跑不了。补上后端，让两端语义尽量一致。

## 1. 探测记录（macOS 27 / 26A428，2026-09-27 实测）

先跑探针再定方案，结论全部推翻了两个「想当然」：

| # | 探针 | 结果 |
|---|---|---|
| A | `sandbox-exec -p '(allow default)(deny file-read* (subpath ~/.ssh))'` | 读被拒（`Operation not permitted`）→ deny 在 allow-default 基线上有效 |
| B | `(allow default)(deny file-write*)(allow file-write* (subpath /private/tmp/x))` | 里面能写、外面拦 → 白名单写法**可行**（前提是路径已解析） |
| F/G | 规则路径写 `/tmp/...` vs `/private/tmp/...` | 写 `/tmp` 的规则**一条都拦不住**：操作侧被内核解析成 `/private/tmp`，规则侧不做解析 |
| I/J | `deny 父目录` 与 `allow 子目录` 的先后 | **后写的赢**：deny 在前 → 子目录可读；allow 在前 → 全拦 |
| H | `(deny file-read*)` + allow 子树 | 进程直接 abort（dyld 读不到库）→ `(deny default)` 那条路走不通 |
| E | `(import "system.sb")` + `-p` 内联 | `execvp()` 被拒 → 不能指望系统 profile 当基线 |
| K/L | `sandbox_compile_string`（父进程）→ `sandbox_apply`（子进程） | 编译成功、apply rc=0，放行目录可写、别的拒绝 → **这条路可行** |
| M | `dlopen("/usr/lib/libsandbox.dylib")` | 磁盘上**没有**这个文件（macOS 27 已并入 dyld 共享缓存），但句柄拿得到，`sandbox_compile_string` / `sandbox_apply` / `sandbox_free_profile` / `sandbox_check` 都在 |

## 2. 机制：进程内 libsandbox（M1，用户 09-27 拍板）

- **父进程**编译：`Rules::prepare` 里 `sandbox_compile_string` 把 SBPL 编译成
  `sandbox_profile_t`。能分配、能报错、能记日志——profile 被拒时 `tracing::warn`
  把原因写下来（`sandbox_free_error` 取错误串）。
- **子进程**应用：`Rules::apply` 只调一次 `sandbox_apply`，不分配、不碰锁，
  正好合 `backend.rs`「pre_exec 里只做 syscall 安全的事」那条约定。
- 备选方案是「子进程里 execv `/usr/bin/sandbox-exec -p <profile>`」（M2）：实测
  `pre_exec` 里换 exec 能保住 stdio，但要在父进程抓下 program/args/env 重建，
  多一次 exec，且 Apple 一旦删掉 `sandbox-exec` 与删掉 libsandbox 是同一件事——
  两套等价，取更干净的那套。私有 API 的代价如实记在这里：符号不在 SDK、不在
  磁盘上，靠共享缓存；取不到就 `probe()` 返回 `None` → **失败关闭**（命令被拒，
  不裸奔），与「内核没有 Landlock」同一姿势。

## 3. profile 形状（顺序就是契约）

```text
(version 1)
(allow default)
(deny file-write*)                          ← 写：默认全禁
(allow file-write* (subpath …) … (literal "/dev/null") …)
(deny file-read* (subpath "/Users"))        ← 读：先禁掉所有人的家
(allow file-read* (subpath …) …)
(deny file-read* file-write* (subpath …))   ← 凭证兜底，最后写、最有权
```

四条规矩，每一条都由探针钉着：

1. **deny 在前、allow 在后**（I/J）。反序等于 allow 被吃掉，profile 变成「全禁」。
2. **根先 canonicalize**（F/G）。规则里的路径不做解析，写软链或者 `/tmp` 这种
   转写路径等于没写。生成时对每个根做 `canonicalize`，不存在则原样保留
   （规则按路径匹配，不需要文件真的在）。
3. **空清单不产出 allow**。`(allow file-write*)` 没有子句就是「放行一切」，
   会把上面那条 deny 整个抵消——空策略必须保持全禁。
4. **凭证兜底最后写**：`~/.ssh`、`~/.gnupg`、`~/.aws`、`~/.netrc`、`~/.kube`、
   `~/.docker/config.json`、`~/.config/gh`、`~/Library/Keychains`、`~/.gqy`。
   即使策略放行了整个家（管理员 `/sandbox ~`），这几处也不给。有一条**自动让位**：
   凭证目录托着策略里的放行根时（成员工作区在 `~/.gqy/home/<user>/workspace`）
   就不写这条 deny，否在后写的 deny 会把工作区一起锁掉。

读侧为什么是 `/Users` 整棵：Linux 那一份策略的读集本来就只列系统目录 + 策略里的
根，管理员的家不在里面。macOS 的家都挂 `/Users` 下，整棵禁掉再放回策略根，结果
与 Linux 一致——成员读不到管理员的家、也读不到 `~/.gqy` 的配置与库。

## 4. 与 Linux 后端的差异

| 维度 | Landlock | Seatbelt |
|---|---|---|
| 写 | 允许列表（ABI 掩码），授权根打不开即失败关闭 | 允许列表（deny 在前 + allow 在后） |
| 读 | 允许列表 | 先禁 `/Users` 再放回策略根；`/usr` `/etc` `/private/var` 等系统目录保持可读（Linux 的读集里本来也有它们） |
| 授予根不存在 | `open()` 失败 → 整个 spawn 失败（失败关闭） | 规则照写，不报错（路径匹配不需要文件在） |
| 网络 | ABI 4 起能管，**刻意没管** | `network*` 没进限制 |
| 缺失后端 | `ENOSYS`/`ENOTSUP` 失败关闭 | 取不到符号 → `ENOTSUP` 失败关闭 |

临时目录：macOS 的 `TMPDIR` 是每个用户私有的 `/var/folders/…/T/`（不是 `/tmp`），
所以 `web::sandbox_scope::base_read_write` 把 `TMPDIR` 也放进可写集（Linux 上通常
就是 `/tmp`，提前覆盖了）。摘要文案仍写 `/tmp`，不为此多掰一次提示词前缀。

## 5. 连带改动

- `src/tools/sandbox/seatbelt.rs`（新）：策略 → SBPL，纯函数 + 7 条单测（顺序契约、
  路径解析、空策略、凭证让位、去重等），任何平台都能跑（Linux CI 也守着生成逻辑）。
- `src/tools/sandbox/macos.rs`（新）：dlsym + 编译 + apply + 探测。
- `backend.rs` / `mod.rs`：后端选择与 `probe()`；新增 `backend_label()`
  （`landlock` / `seatbelt`）供提示词、日志、拒绝提示共用。
- `host_info.rs`：`<host-environment sandbox="…">` 的后端名不再写死（属计划内冷启动）。
- `tests.rs` / `distribution_sandbox.rs`：原来挂在 `cfg(target_os = "linux")` 的强制
  用例改成两端通用，macOS runner 上真验 Seatbelt；`unavailable_backend` 两端都走
  `FORCE_UNSUPPORTED` 钩子。
- `web/server.rs`、`web/session_cmds.rs`：日志与拒绝提示按后端名说话，不再写死 Landlock。

## 6. 已知代价与风险

- **私有 API**：`libsandbox` 不在 SDK，符号靠 dyld 共享缓存。Apple 移除它 = 同时
  移除 `sandbox-exec`，届时 `probe()` 返回 `None`、客户端绑定 `/sandbox` 被拒、
  已绑会话的命令失败关闭——不会静默放宽。
- **一进程只能套一次**：实测第二次 `sandbox_init` 直接失败，所以测试不能在测试
  进程里 apply（会把同进程里别的用例一起关进去）；强制用例都在子进程里跑。
- **不做读白名单**：`(deny default)` + 逐项 allow 会把进程打死（dyld 读不到库），
  所以读侧是「黑名单 + `/Users` 整棵」，`/Volumes`、`/private/var` 之类仍默认可读。
- **Keychain / 网络 / XPC**：走 mach 服务，不在文件系统规则内，本轮不管（与 Linux
  侧「只管文件系统」同一口径）。
