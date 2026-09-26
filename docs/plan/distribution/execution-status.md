# Distribution implementation status

> **这是一份历史记录（09-26 复核后加注）**：本目录记的是 0.6.0 那轮 Linux 打包分发的施工与取证，**任务已完成、只作存档**。文中引用的路径与分支现在多已不存在：`packaging/`（含 `packaging/common/third-party.lock.json`）已随 Arch/DEB/RPM 打包体系一起删除（AGENTS.md §7.7，只有 Nix 和 `install.sh` 两条安装路线），分支 `worktree-distribution-2026-09-14` 从未合并，仓库也已从上游独立为 `yxxbc/gqy-agent`。要动打包或发布，看 `docs/wiki/18-Nix安装开发与发布.md` 与 AGENTS.md §7，不要照这里的步骤做。

总体进度：100%。用户修改后的 Linux 0.6.0 容器安装、真实模型验收、正式 Release 和环境清理已完成。

- 正式发布：https://github.com/SHORiN-KiWATA/miyu-agent/releases/tag/v0.6.0
- 应用 release commit：`bc7087f4c4b03adcef7f8cc8fce64a1c1abb0aaa`。
- 工作分支：`worktree-distribution-2026-09-14`，未合并 main。
- 五个 Linux 安装目标、八个包、36 项必需检查全部通过。AUR 包装包另做实际重包安装和模型输出验证。
- 正式附件 21 个，包含四张 OOBE 截图；全部上传后下载回读 SHA256。
- 本次容器、七个镜像、临时供应商配置及构建/下载缓存已清理，保留最终资产和证据。
- 原完整 T00–T24 的 Mac、硬件、托管服务、升级恢复等未验证项没有被记为 PASS。当前任务按用户后续修改的 linux-smoke 标准完成。

详见 [最终发布验收记录](../../releases/0.6.0/validation.md)、[流程修复证据](gate-repairs.md) 和 [更新后的发布手册](../../../gqy-release-workflow.md)。
