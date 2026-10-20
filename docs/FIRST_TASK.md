# 交给 Codex 的第一批任务

## 任务目标

请遵守根目录 [AGENTS](../AGENTS.md)，在现有规划基础上实现 M0 和 M1，做到：一条构建/镜像入口可启动 Rust x86_64 内核，串口输出可诊断，冒烟测试能可靠判断成功、panic 与超时。

当前仓库仅有规划文档。先核对文件和可用工具，保留既有用户改动。可以分成数个连贯的小批次完成，但不要只生成一套未验证的文件后宣称系统可用。

本批在 `dev` 或基于 `dev` 的任务分支实施，Pull Request 目标为 `dev`。新建自有代码采用 Apache-2.0，并按根目录 AGENTS 保留相应标识与归属声明。

## 1. 工程骨架

- 建立最小 `kernel` + `xtask` workspace 和 Cargo alias。
- 验证并固定精确 Rust 工具链，包含 `rustfmt`、`clippy` 组件；使用 `x86_64-unknown-none` 构建内核，宿主工具保持 host target。
- 自有 Cargo package 声明 `edition = "2024"`，遵守根目录 `rustfmt.toml` 和 [CODING_STYLE](CODING_STYLE.md)；注释、Rustdoc、日志与错误消息使用英文。
- 内核不依赖 `std`；设置 panic abort、独立 linker script、最小入口。
- 根据实际协议选择匹配的 Limine 与 Rust 绑定，固定版本/提交与下载校验和。
- 只创建本批需要的模块；不提前生成未实现的网络、GUI、兼容层和编译器目录。

## 2. 引导闭环

- 生成可由 OVMF 启动的 UEFI 镜像；固件路径可检测/配置。
- 实现 COM1 输出和 panic 尽力日志，报告版本、阶段和关键信息。
- 校验 Limine base revision 与必需响应，正确保留所有仍使用的启动内存。
- 打印内存图摘要、framebuffer 信息（如果存在），安全停止或进入明确的 idle 循环。
- 非必需 framebuffer 缺失不能阻止串口诊断。
- 不使用 UEFI target 代替真正的裸机内核，不访问已经失效的 Boot Services。

## 3. 操作入口

实现以下约定并同步真实使用说明：

```sh
cargo xtask doctor
cargo xtask build
cargo xtask image
cargo xtask run
cargo xtask smoke
```

doctor 报告缺依赖；image 校验外部输入；run 默认使用仓库内生成的普通镜像和临时变量盘；smoke 使用 TCG 可运行，KVM 可选。

`test-host` 可以在确有可独立测试的逻辑时加入，不需要为打印 banner 写镜像式测试。

## 4. 必须实际验证

- workspace 建立后运行 `cargo fmt --all -- --check`，再执行适用的目标构建与测试。
- 在本次可用环境内构建内核 ELF并检查其架构/布局。
- QEMU + OVMF 启动，串口出现最终成功标记。
- 测试 runner 同时检查标记和可解析的测试退出结果。
- 人为 panic 和超时各有一次受控验证，runner 返回失败。
- 缺依赖或校验和不符时明确失败，不自动改系统配置。
- 检查生成流程不会写真实块设备或修改宿主固件。

如果环境缺少 QEMU/固件或外部下载受限，继续完成能做的实现与静态/构建检查，准确列出未执行项目。此时 M1 必须保持未验收，不能把计划中的日志当成实际输出。

## 5. 文档与交付

更新 [STATUS](STATUS.md)、[ROADMAP](../ROADMAP.md) 和 [DEVELOPMENT](DEVELOPMENT.md)；写入实际工具版本、运行命令和验证记录。简明交代已实现、已验证、限制和下一步。

M0/M1 完成后，下一批是 M2 的 CPU 异常与定时中断，再推进内存管理；不要因为有 framebuffer 就转而把桌面当主线。
