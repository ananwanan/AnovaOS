<!-- Provide the synchronized Simplified Chinese project introduction and build guide. -->
# AnovaOS

[English](README.md) | **简体中文**

用 Rust 从零开发的 x86_64 操作系统，目标是在自己的系统中编辑、编译、调试和运行应用，并逐步兼容部分现有 Linux 软件。

**当前状态：M0–M9 已通过本地 QEMU q35/TCG 验证。原生应用、VFS、管道、init/shell、现代 virtio-blk 和 `/persist` 受限 ext2 已支持源码编辑、sync/fsync、正常关机及重启保留。受限 Linux profile 已运行 musl 1.2.4 静态、静态 PIE 和动态程序，验证 pthread/TLS/futex、真实信号处理与中断/重启、poll、非阻塞 I/O、四组 PTY，以及启动共享库、dlopen 和动态 TLS；保留固定 BusyBox 1.36.1 echo/ls/cat/ash 子集。当前单核，共 32 个进程/线程槽；通用 Linux/glibc、网络及系统内编译器仍未实现，外设 IRQ 保持屏蔽。**

项目仓库：[ananwanan/AnovaOS](https://github.com/ananwanan/AnovaOS)。

日常开发与 Pull Request 的目标分支为 **`dev`**；`main` 保留阶段验收后的稳定快照。分支约定见 [GitHub 协作说明](docs/GITHUB_SETUP.md)。

## 项目目标

1. Rust 自研内核，采用 x86_64、UEFI、Limine，先在 QEMU/OVMF 上建立稳定基线。
2. 实现内存管理、中断、进程隔离、抢占调度、文件系统、终端与网络。
3. 提供应用 SDK，让宿主机交叉编译的 Rust/C/C++ 程序以用户进程运行。
4. 按程序和版本逐个验证 Linux x86_64 ABI：先静态 musl 命令行程序，再动态链接和较复杂的软件。
5. 在 AnovaOS 中运行 GCC/G++、rustc 和 Cargo，完成应用开发闭环。
6. 在开发闭环稳定后，推进窗口系统、图形应用、多核和更多实机驱动。

这里的 Rust 主要指内核及自有系统组件。复用的引导器、libc、编译器等可以使用 C/C++ 等语言；不以重写成熟工具作为前置条件。

## 技术基线

| 项目 | 初始选择 | 边界 |
| --- | --- | --- |
| 架构 | x86_64，4 级页表，先单核 | 首版不承担 32 位/x32 ABI |
| 启动 | UEFI + Limine，内核 ELF | Secure Boot 签名链后置 |
| 内核 | 模块化单体内核，Rust `no_std` + `alloc`（M3 已启用） | `panic=abort`，少量必要汇编 |
| 编译目标 | 内核使用 `x86_64-unknown-none` | 用户程序使用独立目标和链接配置 |
| 参考平台 | QEMU q35、OVMF、串口、现代 virtio PCI | TCG 必须可用，KVM 可选 |
| 用户态 | ELF64、System V AMD64 函数调用约定 | native ABI 与 Linux ABI 独立建档 |
| 软件复用 | Linux ABI 兼容层 + 固定版本 musl 用户态 | 不承诺通用 Linux 发行版兼容 |
| 持久存储 | initramfs/tmpfs → virtio-blk + 受限 ext2 | ESP/FAT 与根文件系统分开 |
| 第一开发环境 | Linux/WSL2 交叉开发，后续系统内编译 | 图形桌面不是编译能力前提 |

选型属于项目设计决定；实施时须固定实际验证过的版本、提交和校验和。官方依据见 [参考资料](docs/REFERENCES.md)。

## 文档入口

详细规划文档当前以简体中文编写，开发语言与格式规范使用英文。

| 文件 | 内容 |
| --- | --- |
| [PROJECT_OUTLINE.md](PROJECT_OUTLINE.md) | 总体大纲、交付等级、范围与风险 |
| [AGENTS.md](AGENTS.md) | Codex 的约束、执行顺序、代码规范、验收要求 |
| [ROADMAP.md](ROADMAP.md) | M0–M15 的前置依赖、实现任务和退出条件 |
| [ARCHITECTURE.md](ARCHITECTURE.md) | 内核、应用、ABI、驱动和目录边界 |
| [COMPATIBILITY.md](COMPATIBILITY.md) | 现有软件兼容策略、ABI 需求和验证矩阵 |
| [docs/TOOLCHAINS.md](docs/TOOLCHAINS.md) | GCC/G++/Rust 从交叉编译到系统内开发 |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | 宿主配置、已实现命令、产物和失败处理 |
| [docs/CODING_STYLE.md](docs/CODING_STYLE.md) | 英文开发语言约定与 Rust 格式规范 |
| [docs/FIRST_TASK.md](docs/FIRST_TASK.md) | M0/M1 的实现顺序与每步完成标准 |
| [docs/validation/M0-M1.md](docs/validation/M0-M1.md) | 实际本地构建和启动验证记录 |
| [docs/M2_CPU.md](docs/M2_CPU.md) | CPU 表、异常栈、ACPI/APIC、时钟和上下文规则 |
| [docs/validation/M2.md](docs/validation/M2.md) | 实际 M2 定时器与异常验证记录 |
| [docs/M3_MEMORY.md](docs/M3_MEMORY.md) | 物理页/页表所有权、栈、堆、权限与 OOM |
| [docs/validation/M3.md](docs/validation/M3.md) | 实际 M3 内存与保护验证记录 |
| [docs/M4_USERSPACE.md](docs/M4_USERSPACE.md) | Ring 3、ELF、原生 v1 ABI、用户复制和 Rust/C SDK |
| [docs/validation/M4.md](docs/validation/M4.md) | 实际 M4 隔离与应用验收记录 |
| [docs/M5_PROCESSES.md](docs/M5_PROCESSES.md) | 调度、进程所有权、原生生命周期 ABI 与边界 |
| [docs/validation/M5.md](docs/validation/M5.md) | 多进程、CPU/FPU、OOM 与资源回收验证 |
| [docs/M6_FILESYSTEM.md](docs/M6_FILESYSTEM.md) | VFS、文件/管道所有权、串口终端、init 与原生 shell |
| [docs/validation/M6.md](docs/validation/M6.md) | 文件、shell、管道、终端与 tmpfs 重启丢失验证 |
| [docs/M7_STORAGE.md](docs/M7_STORAGE.md) | PCI/virtio 所有权、ext2 范围、同步关机、磁盘镜像与编辑器 |
| [docs/validation/M7.md](docs/validation/M7.md) | 重启后的源码/二进制校验、设备错误与回归证据 |
| [docs/M8_LINUX.md](docs/M8_LINUX.md) | 静态 Linux ABI、固定 musl/BusyBox 构建与受测 CLI 范围 |
| [docs/M9_RUNTIME.md](docs/M9_RUNTIME.md) | 线程、信号、终端、动态 musl 与所有权边界 |
| [docs/validation/M9.md](docs/validation/M9.md) | 运行时哈希、宿主对照、QEMU 验收与回归证据 |
| [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md) | 强制开发需求，包括文件用途说明 |
| [docs/FILE_PURPOSES.md](docs/FILE_PURPOSES.md) | 仓库中每个文件的用途 |
| [docs/STATUS.md](docs/STATUS.md) | 当前已实现能力和下一步 |
| [docs/GITHUB_SETUP.md](docs/GITHUB_SETUP.md) | 仓库克隆、文档维护与提交约定 |
| [docs/LICENSING.md](docs/LICENSING.md) | Apache-2.0 选择、使用与第三方许可边界 |

## 开发约定

自有代码的标识符、注释、Rustdoc、安全说明、日志、错误消息、CLI 帮助、测试、提交消息、PR 标题/描述和新开发文档统一使用英文。明确的翻译与本地化资源可以使用目标语言；第三方文本及现有中文规划文档按 [代码规范](docs/CODING_STYLE.md) 保留相应语言。

后续每个新增或修改的自有文件，都必须在文件开头写清用途，并维护 [文件用途清单](docs/FILE_PURPOSES.md)。提交前执行 `cargo xtask check-files`，具体要求见 [DEV-001](docs/REQUIREMENTS.md)。

Rust 格式由根目录 [rustfmt.toml](rustfmt.toml) 定义：4 空格缩进、100 列建议宽度、LF 换行、Rust 2024 parser/style edition，仅使用稳定版 rustfmt 选项。在仓库根目录执行：

```sh
cargo fmt --all
cargo fmt --all -- --check
cargo fmt --manifest-path user/Cargo.toml --all -- --check
```

共享项目信息变更时，同步维护默认英文 [README.md](README.md) 与本中文版。

## 怎样才算“可以在系统上开发 app”

在一次 AnovaOS 启动期间，从系统自己的终端和持久磁盘完成以下工作，执行期间不把编译请求转发给宿主机：

- 编辑并保存 `main.c`、`main.cpp`、`main.rs`。
- 使用系统中运行的 `gcc`、`g++`、`rustc` 编译、链接并运行产物。
- 使用 `cargo build --offline` 构建带已准备依赖的小型项目，运行应用并核对输出。
- 应用能够读写文件、处理命令行参数、报告错误；重启后源码与产物保留。
- 后续再验证联网获取依赖、完整调试器、复杂 Cargo 构建与编译器自举。

可以先通过 Linux 兼容层运行这些工具链；这仍是应用在自研内核上的系统内开发。AnovaOS 专用 target 和原生 `std` 移植另设阶段，避免混淆进度。

## 构建与启动

宿主使用 x86_64 Ubuntu 24.04 或 WSL2，安装 rustup、QEMU 8.2.2、xorriso、curl、dpkg-deb、GCC 和 e2fsprogs。仓库固定 Rust 1.90.0、rustfmt、Clippy 与裸机目标；依赖安装和固件路径见 [开发说明](docs/DEVELOPMENT.md)。

```sh
cargo xtask fetch       # 下载并校验固定的 Limine/OVMF 输入
cargo xtask doctor      # 汇总缺失依赖与版本不符
cargo xtask build       # 构建并检查裸机内核 ELF
cargo xtask apps        # 构建独立 Rust/C 用户 ELF 和 ABI/校验清单
cargo xtask check-files # 检查每个文件的用途说明与清单覆盖
cargo xtask image       # 生成包含原生应用的 build/images/anova-normal.iso
cargo xtask run         # 启动原生工作负载、PID 1 init 和交互式 shell
cargo xtask smoke       # CPU/内存/进程与 VFS/管道/shell/OOM 验证，退出码 33
cargo xtask verify      # 13 场景：启动通过，故障、panic、超时和 OOM 被拒绝
cargo xtask terminal-test # 真实串口编辑/EOF 与两次启动的 tmpfs 丢失验证
cargo xtask persistence-test # 源码/二进制重启校验、文件系统与 I/O 错误验证
cargo xtask root-image build/disks/work.ext2 # 创建新的持久数据盘
cargo xtask run --disk build/disks/work.ext2 # 复用此盘，在系统内 shutdown 正常关机
```

以上命令已在宿主 `xtask` 包中实现。镜像、缓存、日志和临时固件变量副本均位于 `build/`；普通启动使用临时数据盘、不连接网络。GitHub Actions 已配置相同检查，其运行结果与 [本地验收](docs/validation/M9.md) 分开记录。

## 下一步

M8/M9 已加入独立 Linux syscall personality、受测静态 CLI，以及受限的线程、信号、终端和动态 musl 运行时。当前磁盘和编辑器范围见 [M7 存储设计](docs/M7_STORAGE.md)，另见 [当前状态](docs/STATUS.md) 和 [路线图](ROADMAP.md)。

## 许可证

本项目的原创代码、文档和示例采用 **[Apache License 2.0](LICENSE)**，SPDX 标识为 `Apache-2.0`。Copyright 2026 ananwanan and the AnovaOS contributors.

该许可证允许商用、修改和分发，也允许在遵守许可条件的前提下发布闭源衍生作品，并包含贡献者专利授权。分发时须履行许可证、归属声明与修改标记等要求；详见 [许可说明](docs/LICENSING.md) 与 [NOTICE](NOTICE)。

第三方引导器、libc、编译器、依赖库与导入代码仍适用各自许可证，不因放入本仓库或系统镜像而改为 Apache-2.0。

详见 [M8 静态 Linux CLI](docs/M8_LINUX.md)，包含边界、构建依赖以及 `cargo xtask trace-compat` / `cargo xtask test-compat`。
普通串口会话可执行 `/linux/sh`，进入后使用 `/linux/echo`、`/linux/cat` 和 `/linux/ls`。

## M9 musl 运行时

```sh
linux-threads
linux-pie
linux-dynamic
```

在原生 shell 输入以上命令可启动独立 Linux 用户 ELF，验证 pthread/futex、共享内存、信号、就绪等待和 PTY；linux-dynamic 还会实际装载共享库和动态 TLS。支持范围、资源限制、缺库诊断和作业控制边界见 [M9 运行时](docs/M9_RUNTIME.md) 与 [M9 验证](docs/validation/M9.md)。native v1 和持久盘的 ABI 策略保留各自契约。
