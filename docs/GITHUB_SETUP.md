# GitHub 仓库与协作

项目仓库：[ananwanan/AnovaOS](https://github.com/ananwanan/AnovaOS)。日常开发和 Pull Request 目标使用 `dev`，`main` 保留阶段验收后的稳定快照。仓库由项目所有者创建，目前为公开仓库；日常提交保持已有可见性和权限。

仓库管理约定：GitHub 默认分支使用 `dev`。页面设置与 git 工作区分支需要分别检查；只创建分支不会自动更改默认分支。

## 获取源码

```sh
git clone --branch dev https://github.com/ananwanan/AnovaOS.git
cd AnovaOS
```

已配置 GitHub SSH 密钥时，也可以使用：

```sh
git clone --branch dev git@github.com:ananwanan/AnovaOS.git
```

## 开始开发

先读取根目录 [AGENTS.md](../AGENTS.md) 与 [当前状态](STATUS.md)。M0–M4 已有本地验收，当前边界见 [M4_USERSPACE.md](M4_USERSPACE.md)，下一批推进 M5。后续每个新增/修改文件必须遵守 [DEV-001](REQUIREMENTS.md) 的用途说明要求。

文档使用 `AnovaOS` 作为项目名称。将来的自定义 target 名称只是候选本地标识，不能据此宣称已有上游编译器支持。

## 提交约定

- 日常工作在 `dev` 或由 `dev` 创建的任务分支进行，Pull Request 默认提交到 `dev`。
- 阶段验收后再将经过验证的内容同步到 `main`，不把试验性改动直接提交到稳定分支。
- 提交前检查 `git status` 和 diff，保留用户已有修改。
- 提交消息与 PR 标题/描述使用英文；代码注释、Rustdoc、日志等遵守 [CODING_STYLE.md](CODING_STYLE.md)，默认英文 README 与中文版同步维护。
- workspace 建立后，提交前通过根目录 `rustfmt.toml` 对应的 `cargo fmt --all -- --check`。
- 代码、ABI、路线图与实际状态同步更新，验证结果写清通过、失败和未执行的范围。
- 保留远端已有历史；不通过删除仓库或强制推送覆盖已有提交。
- 不提交磁盘镜像、工具链缓存、密钥、令牌或包含私人数据的日志。
- 已提供 Rust workspace、宿主入口和 CI；命令与验证范围见 [DEVELOPMENT.md](DEVELOPMENT.md)。

本项目原创内容采用 [Apache-2.0](../LICENSE)，第三方组件仍单独记录和保留各自许可；详情见 [LICENSING.md](LICENSING.md)。
