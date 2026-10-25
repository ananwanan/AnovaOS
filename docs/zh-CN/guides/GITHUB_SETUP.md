<!-- Define development branches, source synchronization and documentation-aware contribution requirements. -->
# GitHub 仓库与协作

[English](../../en/guides/GITHUB_SETUP.md) | **简体中文**

项目仓库为 [ananwanan/AnovaOS](https://github.com/ananwanan/AnovaOS)。日常开发与 Pull Request
以 `dev` 为目标，`main` 接收验收后的稳定快照。保留现有可见性、权限与历史。
仓库默认分支和本地工作区分支是独立设置，创建分支不会自动更改这两项。

## 获取并同步源码

```sh
git clone --branch dev https://github.com/ananwanan/AnovaOS.git
cd AnovaOS
git status --short --branch
git fetch origin --prune
git switch dev
git pull --ff-only origin dev
```

已经配置 SSH 时，可用 `git clone --branch dev git@github.com:ananwanan/AnovaOS.git`。
切换前检查并保留本地修改；无法快进时不要强制覆盖。

## 开始开发

先读 [AGENTS](../../../AGENTS.md)、[当前状态](STATUS.md)、[路线图](../ROADMAP.md)
及相关阶段的设计和证据。M0–M14 已有受限范围验收；M15.1/M15.2、M15.5 guest 内核重建
与 M15.6 独立重复构建已有本地验收。M15.3/M15.4 评估了编译器重建的前置条件与阻塞，
没有重建 GCC、LLVM、rustc 或 Cargo。当前后续任务见 STATUS。
候选 target 名称不代表已有上游编译器支持。文档统一使用 `AnovaOS` 名称。

## 提交要求

- 在 `dev` 或基于它的任务分支开发，PR 默认提交到 `dev`。
- 验证后再将验收内容同步到 `main`。保留用户修改与远端历史，不删除仓库或强制推送覆盖历史。
- 交付前审阅状态与 diff。提交消息、PR 标题/描述、源码注释、Rustdoc 和日志使用英文，遵守
  [代码风格](CODING_STYLE.md)。
- 按 [DEV-001](REQUIREMENTS.md) 为新增/修改文件写用途说明并维护
  [文件用途清单](FILE_PURPOSES.md)。运行 `cargo xtask check-files` 与根配置对应的
  `cargo fmt --all -- --check`。
- 行为变化时同步中英文 README、相关双语指南、状态、路线图、ABI 和验收证据。区分通过、失败
  和未执行的验证；文档一致性属于完成条件。
- 不提交镜像、工具链缓存、凭据、令牌或私人日志。
- 使用 [DEVELOPMENT](DEVELOPMENT.md) 中已实现的命令，本地结果与远端 CI 结果分别记录。

原创内容采用 [Apache-2.0](../../../LICENSE)，第三方通知和条款独立保留，见
[许可说明](LICENSING.md)。
