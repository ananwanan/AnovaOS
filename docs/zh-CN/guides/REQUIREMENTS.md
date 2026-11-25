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

## DEV-004 排期调整（2026-10-27）

所有者明确暂缓 M16 远端 CI 收尾并授权开始 M17。保持 M16 待验状态及只由 main 推送触发 Actions 的规则。M17 可以基于已有本地验收基础推进，所有 M17 验收条件继续适用。

## DEV-005：M18 独立重建证据

2026-11-21 加入。从 `dev` 建立 `feat/m18-bootstrap` 开发分支，Pull Request 目标为
`dev`。保留 M16 远端收尾待办和仅 main 推送运行 Actions 的规则。

- 分别记录继承工具运行、应用编译、guest 内编译器重建和多阶段 bootstrap。按源码指定的
  stage0 分别界定 LLVM、rustc 与 Cargo；保留独立的 M15 探针。
- 每次构建前声明真实工作负载预算和精确输入。在 `build/` 下使用新建显式工作盘，保留
  ext2 和 COM1 恢复路线、原生/GUI ABI 以及显式 personality。
- 在 guest 内完成最终镜像构建，保留宿主准备的输入、精确提取的内核字节及带身份的实际启动，
  带 GUI 的构建另行处理。
- 使用干净盘重复可行组件，保留哈希、日志、失败恢复结果及差异。对应真实正例、负例和恢复
  检查全部通过后再验收子阶段。参阅 [M18 契约](../milestones/M18_BOOTSTRAP.md)。

## USB 键鼠

DEV-006 要求按 USB.1～USB.5 顺序在真实传输链路验收键鼠，复用 PS/2 的统一输入接口，保留无设备桌面与串口恢复。
USB.6 新增有界 USB2 Hub、热插拔与恢复，并使用独立 QEMU 门禁；实机验收仍分别进行。

[USB 开发里程碑](../milestones/USB_INPUT.md) · [测试指南及证据](../validation/USB_INPUT.md)
