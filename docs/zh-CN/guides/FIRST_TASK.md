<!-- Preserve historical M0/M1 implementation order and acceptance while pointing to current follow-up work. -->
# M0/M1 实现顺序与验收

[English](../../en/guides/FIRST_TASK.md) | **简体中文**

本文保留历史 M0/M1 验收约定。当前 CPU/IRQ 行为和命令见
[M2_CPU](../milestones/M2_CPU.md) 与 [DEVELOPMENT](DEVELOPMENT.md)。

起始版本为 `dev` 上的 `847f7c66ddef25616c27a088d0c7044c6f9e8c0d`，当时只有规划文档，
不假设已有 Cargo 命令或内核。以下为实际实现顺序；遵守 [AGENTS](../../../AGENTS.md)，
历史执行结果见 [validation/M0-M1](../validation/M0-M1.md)。

| 顺序 | 实现 | 可执行检查 | 完成条件 |
| --- | --- | --- | --- |
| 1 | 检查分支、状态、规范和真实文件 | `git status --short --branch` | 从 dev 开始、保留已有修改、识别缺失命令 |
| 2 | workspace、实际 kernel/宿主 xtask 和别名 | `cargo metadata --locked --no-deps` | 宿主使用宿主 std，不设全局裸机 target |
| 3 | 锁定实测 Rust、target、rustfmt/Clippy、Cargo.lock | `cargo fmt --all -- --check` | Rust 1.90.0 可用，自有包使用 Rust 2024/Apache-2.0 |
| 4 | no_std 入口、abort panic 和内核专用链接脚本 | `cargo xtask build` | 静态 ELF64 x86_64 ET_EXEC，高半区入口位于 RX 段，无 PT_INTERP/PT_DYNAMIC/RWX |
| 5 | 锁定/验证引导输入与诊断 | `cargo xtask fetch`; `cargo xtask doctor` | 验证 Limine/OVMF 哈希；缺工具/输入变更非零退出 |
| 6 | 保留请求并验证协议响应 | `cargo xtask smoke` | 验证 base 3、UEFI64、四级页表、HHDM、内存图和内核保留 |
| 7 | COM1 与独立 panic 输出 | `cargo xtask smoke --case panic` | 记录身份/阶段，受控 panic、QEMU 35，xtask 返回 1 |
| 8 | 保留引导内存并报告可选数据 | 检查 boot.serial.log | 报告内存图/保留；不分配/回收；允许无 framebuffer |
| 9 | UEFI ISO 与 QEMU/OVMF | `cargo xtask image`; `cargo xtask run` | 普通内核到 BOOT:OK 后闲置，CODE 只读，每次复制 VARS |
| 10 | watchdog 和故障注入 | `cargo xtask verify` | 有序阶段、最终标记、QEMU 33 才通过；panic/hang 拒绝 |
| 11 | 同步状态、命令、证据、CI | 对应 Clippy、宿主测试和工作流 | 使用已实现命令；远端 CI 与本地证据分开 |

先建立别名/可执行程序再首次调用 `cargo xtask`。`fetch` 下载，`doctor` 只检查、不安装宿主包。
设置和命令约定见 [DEVELOPMENT](DEVELOPMENT.md)。

## M0 验收

宿主 xtask 使用宿主 std/链接器；内核使用预编译裸机 core 和捆绑 LLD，不需 nightly/自定义
 target JSON。格式、独立宿主/内核 Clippy 和实际宿主逻辑测试通过。记录工具版本、引导输入
提交和校验和，缺依赖返回 1。

## M1 验收

q35/TCG + OVMF + Limine 在 256 MiB/单 vCPU 下启动实际 Rust ELF。成功必须依次出现
`[BOOT:ENTRY]`、`[BOOT:PROTOCOL]`、`[BOOT:MEMORY]`、`[BOOT:OK]`、`[TEST:PASS]`，
无 `[PANIC]`，QEMU 退出 33。

受控 panic 必须到达注入文本并退出 QEMU 35；受控超时必须先到 `[TEST:HANG]`，再由 watchdog
终止并回收 QEMU。单独运行负例 smoke 返回 1。`verify` 预期这些失败，未到注入点使整体失败。
损坏输入必须阻止镜像生成。

普通启动停留 `cli; hlt`，不访问测试退出设备。固件源哈希不变，产物仅在 target/ 或 build/。

## 所有权与后续任务

M1 全程以 BootInfo 只读借用 Limine 内存图，无分配器，保留引导栈、页表、响应、ACPI、内核
和模块。保留区可重叠，可用/bootloader-reclaimable 范围检查对齐和重叠；M3 必须先建立
所有权才能考虑回收。

base revision 3 提供物理 RSDP；M1 只报告，不解析 ACPI、不调用固件服务、不绘图或执行模块。
历史上的下一步已由 [M2_CPU](../milestones/M2_CPU.md) 和 [M2 验证](../validation/M2.md)
实现。其后的 M3 所有权、页分配、受管映射和堆也已完成。当前源码重建及编译器前置条件见
[STATUS](STATUS.md) 和 [ROADMAP](../ROADMAP.md)，重复构建证据见
[M15.6](../validation/M15.6.md)。外设 IRQ 启用仍需对应驱动和验收。
