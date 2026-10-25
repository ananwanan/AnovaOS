<!-- Record owner-approved file-purpose, GUI architecture and timely bilingual documentation requirements. -->
# 开发需求

[English](../../en/guides/REQUIREMENTS.md) | **简体中文**

这里记录项目所有者要求执行的开发规范。架构与验收约定见 [AGENTS](../../../AGENTS.md)
和 [ROADMAP](../ROADMAP.md)。

## DEV-001：解释每个文件的用途

2026-10-21 加入。每个新增或修改的自有文件都必须在顶部附近说明职责和相关边界，涵盖实现、
修复、配置、示例、测试和文档，并适用于后续开发。

- 用途注释使用英文；Rust 使用 `//!`，其他格式使用支持的注释语法。
- 行为或所有权变化时更新说明，维护 [FILE_PURPOSES](FILE_PURPOSES.md)。
- 生成文件、二进制和无法注释的格式使用用途清单或相邻 README，保留生成锁文件和原始法律/第三方文本。
- 交付前运行 `cargo xtask check-files`，CI 使用同一覆盖检查。人工审阅仍需判断说明准确性。
  M4 已为此前文件建立覆盖基线并补齐源码/配置头部说明。

## DEV-002：Rust GUI、三语言 SDK 与五项 M14 验收

2026-10-22 加入。这里记录所有者的架构决定和验收要求，实际完成范围由 [STATUS](STATUS.md)
和各阶段验证记录说明。参见 [M14 GUI](../milestones/M14_GUI.md)、[ROADMAP](../ROADMAP.md)
与 [AGENTS](../../../AGENTS.md)。

- 原生绘制、合成器、runtime 和桌面用户态使用 Rust。窗口/控件策略与合成器位于用户进程，
  内核提供安全设备和 IPC 机制。
- C、C++、Rust SDK 共用语义约定及 Rust 实现的版本化 C ABI，提供 C 头文件、C++ RAII
  和符合习惯的安全 Rust 封装。
- 明确布局检查、所有权、不透明句柄、版本、错误、线程/事件行为、客户端 IPC 隔离与负例。
  Rust panic 和 C++ 异常不能跨 FFI 展开。
- 分别验收：M14.1 framebuffer/软件绘制；M14.2 输入；M14.3 用户态多窗口合成；
  M14.4 runtime 及独立 guest C/C++/Rust 示例；M14.5 可选真实 GPU 绘制和已测 CPU 回退。
- Linux GUI 框架、更多硬件输入、SMP 与通用 Vulkan/OpenGL 独立立项。framebuffer/scanout
  不代表加速。已验收 M14.5 为有界硬件清屏/读回，桌面合成仍用软件绘制。
- 实现后更新状态与阶段证据，文档或未测试 demo 不能证明完成。

## DEV-003：每批交付及时更新开发文档

2026-10-25 按项目所有者要求加入。后续开发必须及时维护文档。

- 实现或配置变化同批更新两份 README、受影响的 `docs/en` 与对应 `docs/zh-CN`、STATUS、
  ROADMAP、架构/ABI、命令说明和验证记录。
- 已完成的“下一步”应替换为首个剩余任务。保留已测限制、阻塞、历史证据，区分实现、本地验收与远端 CI。
- 双语内容保持一致，准确保留标识符、路径、命令和法律文本。移动/新增文档后检查本地链接与用途清单。
  文档一致性属于交付完成条件；不能声称运行了未执行的测试。

## DEV-004：先资源/ext4，再开发桌面

2026-10-26 按项目所有者要求加入；此处记录计划，不代表已实现。

- 按 M16 资源/VFS/ext4、M17 开发桌面、M18 有条件编译器重建和 guest 独立镜像的顺序推进。
  阶段与退出条件见[路线图](../ROADMAP.md)、[M16](../milestones/M16_FOUNDATION.md) 和
  [M17](../milestones/M17_DESKTOP.md)。
- 先恢复受检 CI 依赖获取并测量明确的真实构建，再确定容量；VFS/缓存/堆/task 资源需独立于
  磁盘格式扩展。
- 分阶段验收受控 ext4 只读、可写和 JBD2 恢复；保留 ext2 旧镜像/恢复，通过复制并核对新
  镜像迁移，不原地转换。全新 AnovaFS 格式不进入开发关键路径。
- 使用现有 guest GCC/G++ 和 Rust/Cargo，实现真正的图形 PTY 终端、编辑/文件、日志/资源
  查看，以及全 GUI 编辑/构建/运行/修错/重启流程。
- 完整编译器自举、原生 std、SMP 和实机驱动不作为开发桌面前置条件；保留 COM1/QEMU/
  宿主处理内核、引导或显示故障的恢复入口。
- 新阶段在真实正例、压力、失败和恢复检查通过之前保持计划状态；保留原有门禁，并区分
  本地验收与远端 CI。
