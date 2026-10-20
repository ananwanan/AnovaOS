# GitHub 仓库与协作

项目仓库：[ananwanan/AnovaOS](https://github.com/ananwanan/AnovaOS)，默认分支为 `main`。仓库由项目所有者创建，目前为公开仓库；日常提交保持已有可见性和权限。

## 获取源码

```sh
git clone https://github.com/ananwanan/AnovaOS.git
cd AnovaOS
```

已配置 GitHub SSH 密钥时，也可以使用：

```sh
git clone git@github.com:ananwanan/AnovaOS.git
```

## 开始开发

先读取根目录 [AGENTS.md](../AGENTS.md) 与 [当前状态](STATUS.md)。目前只有规划文档；第一批实现按 [FIRST_TASK.md](FIRST_TASK.md) 推进 M0/M1。

文档使用 `AnovaOS` 作为项目名称。将来的自定义 target 名称只是候选本地标识，不能据此宣称已有上游编译器支持。

## 提交约定

- 提交前检查 `git status` 和 diff，保留用户已有修改。
- 代码、ABI、路线图与实际状态同步更新，验证结果写清通过、失败和未执行的范围。
- 保留远端已有历史；不通过删除仓库或强制推送覆盖已有提交。
- 不提交磁盘镜像、工具链缓存、密钥、令牌或包含私人数据的日志。
- 当前没有 Rust workspace 或 CI；对应能力实际落地后再加入构建说明和工作流。

自有代码许可证尚未选定，第三方组件仍需单独记录和保留各自许可。
