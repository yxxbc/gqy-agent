# 18 · Nix：安装、开发与发布

顾清影以 **Nix** 为主要的安装和分发方式。这一页讲清三件事：用户怎么装、开发者平时怎么干活、维护者怎么发版。`install.sh` 和 AUR 包仍然保留，但只是备用路线。

支持的平台：Linux x86_64 / ARM64（含 NixOS）、macOS Apple 芯片。**Intel Mac 不走 Nix**：nixpkgs 从 26.11 起不再支持 x86_64-darwin，Release 里仍然发布 Intel 包，这部分用户用 `install.sh` 安装。

## 0. 一张图看懂

```
开发者改代码 ──► 提交到 gqy 分支 ──► 改 Cargo.toml 版本、打 tag vX.Y.Z 推上去
                                              │
                        GitHub Actions（publish-release.yml）
                        ├─ 云端编译 4 个平台，打包成 gqy-<平台>.tar.gz
                        ├─ 发布到 GitHub Releases
                        └─ 把新包的 sha256 写进 gqy 分支的 nix/release.json（自动提交）
                                              │
用户：nix profile install / upgrade ◄─────────┘
      Nix 读 gqy 分支上的 release.json，下载对应的包并校验 hash，几秒装完，不在本地编译
```

## 1. 相关文件

| 文件 | 作用 |
|---|---|
| `flake.nix` | 入口。`packages.gqy`（默认，预编译版）、`packages.gqy-src`（源码编译）、`apps.default`、`devShells.default` |
| `nix/prebuilt.nix` | 下载 Releases 里的包，按 `release.json` 校验；Linux 上用 autoPatchelf 修正动态库路径 |
| `nix/package.nix` | 从源码编译，ONNX Runtime 用 nixpkgs 的 |
| `nix/release.json` | 当前发布的版本号和 4 个平台包的 sha256。**只用脚本生成，不要手改** |
| `nix/update-release.py` | 根据 `SHA256SUMS` 生成 `release.json` |
| `nix/migrate.sh` | 从 `install.sh` 装的版本换成 Nix 版 |
| `.github/workflows/publish-release.yml` | 云端编译、发布 Release、回写 `release.json` |
| `install.sh` | 备用：没有 Nix 的用户用它装到 `~/.local` |

两个 Nix 包装出来的文件布局一致：

```
bin/gqy                  包装脚本：把 ripgrep、chafa 加到 PATH 末尾，再启动 bin/.gqy-wrapped
share/gqy/{fonts,models,scripts,default-kb}
lib/gqy/libonnxruntime.*
```

程序在「二进制所在目录/../share/gqy」和「../lib/gqy」下找资源，所以不需要任何额外配置。

## 2. 用户：安装、升级、卸载

```bash
nix profile install github:yxxbc/gqy-agent/gqy   # 安装（需要开启 flakes）
nix profile upgrade gqy-agent                     # 升级到最新发布版
nix profile rollback                              # 升级出问题，退回上一代
nix profile remove gqy-agent                      # 卸载
nix run github:yxxbc/gqy-agent/gqy                # 不安装，只运行一次
```

- 在 profile 里的名字是 **`gqy-agent`**（取自仓库名），升级和卸载都用这个名字。
- 数据都在 `~/.gqy`，装在哪、怎么装都共用这份数据。卸载程序不会删数据。
- 升级后第一次运行 CLI 时，会发现 daemon 还是旧版本（比较 `GQY_BUILD_ID`），自动重启到新版本。
- NixOS / home-manager：把 `github:yxxbc/gqy-agent/gqy` 加为 flake input，使用 `inputs.gqy-agent.packages.${system}.gqy`。

### 以前用 install.sh 装过

两份同时存在时，终端里敲 `gqy` 用的是 PATH 里排在前面的那个，很容易装了新版却还在用旧版。用迁移脚本：

```bash
curl -fsSL https://raw.githubusercontent.com/yxxbc/gqy-agent/gqy/nix/migrate.sh | sh -s -- --dry-run   # 先看会做什么
curl -fsSL https://raw.githubusercontent.com/yxxbc/gqy-agent/gqy/nix/migrate.sh | sh                    # 执行
```

它会依次：

1. 先用 Nix 装好（失败就什么都不动）；
2. 停掉旧版本的 daemon；
3. 把 install.sh 装的**程序文件**挪到 `~/.gqy/bin-backup/install-sh-<时间>/` 备份，不删除；
4. 检查终端里的 `gqy` 现在指向哪一个，提醒还有哪些东西排在 Nix 版前面。

`~/.gqy` 里的数据，以及 `~/.local/share/gqy` 下除了程序资源以外的文件（老版本可能在那里留过数据），一概不动。

反过来，已经用 Nix 装过，再跑 `install.sh` 会被拒绝（除非设置 `GQY_FORCE=1`），免得又装出两份。

### 用 cargo 装过开发版

`cargo install` 装在 `~/.cargo/bin/gqy`，通常排在 `~/.nix-profile/bin` 前面，所以它会遮住 Nix 版。普通用户不需要了就运行 `cargo uninstall gqy`。开发者见下一节。

## 3. 开发者：日常开发

### 本机用哪个 gqy

**预编译包和 Nix 包都不带语音前端 `gqy-voice`**（它依赖 sherpa-onnx 静态库）。需要语音的开发机，日常用的还是自己编译的版本：

```bash
cargo install --path . --locked --features voice   # 装到 ~/.cargo/bin/{gqy,gqy-voice}
```

开发机上**不要再 `nix profile install` 一份**：`~/.cargo/bin` 排在前面，Nix 版根本用不到，只会造成混乱。要验收发布出去的 Nix 版，用沙箱跑一次就够了（`GQY_HOME` 沙箱，别连到生产 daemon）：

```bash
GQY_HOME=$(mktemp -d) nix run github:yxxbc/gqy-agent/gqy -- --version
```

### 工具链

rustup 和 Nix 两种都行，任选一种：

```bash
# 用 rustup：照常 cargo build
cargo build --release

# 用 Nix：进入带 Rust 工具链、clippy、rustfmt、ONNX Runtime 的环境
nix develop
cargo build --release
```

### 改了这些地方，Nix 这边也要跟着改

| 改动 | 还要改哪里 |
|---|---|
| 新增要随程序一起发布的资源（字体、模型、脚本……） | `publish-release.yml` 的「打包」一步 **和** `nix/package.nix` 的 `postInstall`（布局保持一致），Arch 包对应改 `packaging/common/assets.json` |
| 新增运行时要从 PATH 调用的外部命令 | `nix/prebuilt.nix` 和 `nix/package.nix` 里的 `wrapProgram ... --suffix PATH` |
| 新增 Linux 系统库依赖（链接期） | `nix/package.nix` 的 `buildInputs`；预编译版 `nix/prebuilt.nix` 的 `buildInputs`（给 autoPatchelf 用） |
| 新增发布平台 | `publish-release.yml` 的 matrix、`nix/update-release.py` 的 `TARGETS`、`flake.nix` 的 `systems`（要确认 nixpkgs 还支持这个平台） |
| 改依赖（Cargo.lock） | 什么都不用改，`nix/package.nix` 直接读 `Cargo.lock` |

改完打包相关的东西，本地验证：

```bash
nix flake check --no-build --all-systems   # 3 个平台都能解析
nix build .#gqy-src          # 源码版能编译，结果在 ./result
./result/bin/gqy --version
```

### 注意：不要把 gqy 自己的绝对路径写进持久化配置

Nix 版的程序在 `/nix/store/<hash>-gqy-<版本>/` 下，**每次升级路径都会变**，旧路径在回收旧版本（`nix-collect-garbage`）后就失效了。`paths::gqy_executable()` 只能用来启动子进程，或者用在每次运行都会重新生成的配置里（例如 antigravity 的 MCP 配置，每次请求都会刷新）。要写进 shell rc、launchd/systemd 服务、用户配置文件，就写 `gqy` 这个名字，靠 PATH 查找。

## 4. 维护者：发布新版本

1. 确认要发布的内容都已经合进 `gqy` 分支，并通过验收；发布说明写在 `docs/releases/<版本>/release-notes.md`。
2. 修改 `Cargo.toml` 的 `version`（README 顶部的版本徽章也一起改），然后提交：
   ```bash
   git commit -am "release: vX.Y.Z"
   git push gqy gqy
   ```
3. 打 tag 并推送，会触发云端构建：
   ```bash
   git tag vX.Y.Z
   git push gqy vX.Y.Z
   ```
4. 在 GitHub Actions 里看「发布 Release（云端构建）」跑完。它会：
   - 编译 4 个平台并发布到 Releases；
   - 自动往 `gqy` 分支提交一个 `chore(nix): 预编译包更新到 vX.Y.Z`，更新 `nix/release.json`。
5. 把 CI 的提交拉回本地，免得下次推送冲突：
   ```bash
   git pull gqy gqy
   ```
6. 验收：
   ```bash
   GQY_HOME=$(mktemp -d) nix run github:yxxbc/gqy-agent/gqy --refresh -- --version   # 应该输出新版本号
   ```
   用户那边运行 `nix profile upgrade gqy-agent` 就能升到新版。

### 出了问题怎么办

| 现象 | 处理 |
|---|---|
| CI 最后一步推送被拒（分支保护） | 要么在分支保护规则里放行 `github-actions`，要么手动更新：`gh release download vX.Y.Z -p SHA256SUMS` → `python3 nix/update-release.py vX.Y.Z SHA256SUMS` → 提交 `nix/release.json` |
| 用户安装时报 `hash mismatch` | Release 里的包被重新上传过，`release.json` 还是旧 hash。重新跑一次发布工作流，或者按上一行手动更新 |
| 某个平台编译失败 | 那个平台的包不在 Release 里，`update-release.py` 会报「SHA256SUMS 里没有 …」并退出，`release.json` 保持旧版本，不会写出坏数据。修好后重新跑工作流（可以手动运行并填同一个 tag） |
| 重新发布旧 tag | 工作流会跳过回写，`release.json` 不会被降级到旧版本 |

### 为什么 `github:yxxbc/gqy-agent/vX.Y.Z` 装不了预编译版

一个 tag 里的 `release.json` 没法提前写好它自己编译产物的 hash（包是打 tag 之后才编译出来的）。所以预编译版一律从 `gqy` 分支装。要固定某个版本，就用源码版：`nix profile install github:yxxbc/gqy-agent/vX.Y.Z#gqy-src`。
