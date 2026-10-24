<!-- Provide the synchronized Simplified Chinese project introduction and build guide. -->
# AnovaOS

[English](README.md) | **简体中文**

用 Rust 从零开发的 x86_64 操作系统，目标是在自己的系统中编辑、编译、调试和运行应用，并逐步兼容部分现有 Linux 软件。

**当前状态：M0–M13（受限离线与联网范围）已通过本地 QEMU q35/TCG 验证。原生应用、VFS、管道、init/shell、现代 virtio-blk 和 `/persist` 受限 ext2 已支持源码编辑、sync/fsync、正常关机及重启保留。受限 Linux profile 已运行 musl 1.2.4 静态、静态 PIE 和动态程序，验证 pthread/TLS/futex、真实信号处理与中断/重启、poll、非阻塞 I/O、四组 PTY，以及启动共享库、dlopen 和动态 TLS；保留固定 BusyBox 1.36.1 echo/ls/cat/ash 子集。当前单核，共 32 个进程/线程槽；M10 新增可选 virtio-net IPv4 TCP/UDP/DNS、可中断 socket、严格证书验证的用户态 TLS，以及锁定软件包的安装、升级、回滚与卸载。M11 新增可选离线 GCC/G++/make 开发镜像，已验证系统内编译运行 C/C++、异常、线程及重启保留。M12 新增系统内 Rust 1.90.0/std 与离线 Cargo，分别验证 vendor 依赖、build.rs、C FFI 和系统内编译的过程宏。可选在线 Cargo 已验证严格 HTTPS registry/Git 下载、缓存保留及断网重编译；公网 crates.io/GitHub 通过配置的宿主代理验证。M13 新增 SDK 1.0.0、原生 C/C++/Rust 模板、显式 ABI 应用包、安装目标策略校验和真实用户故障源码符号化。通用 Linux/glibc 仍待实现，外设 IRQ 保持屏蔽。**

项目仓库：[ananwanan/AnovaOS](https://github.com/ananwanan/AnovaOS)。

M14.1 已增加可选软件显示、受检 framebuffer 映射和真实 QEMU 逐像素/截图验证。
M14.2 已实现轮询式 PS/2 物理输入与裁剪游标。M14.3 已通过本地 QEMU 验收，交付独立
Rust 用户态窗口合成器、焦点输入与崩溃/重启恢复。M14.4 已交付 Rust GUI runtime、
C/C++/Rust SDK 与图形应用，系统内独立构建、真实键鼠输入和重启保留通过验收；
M14.5 已验证有界硬件 VirGL 离屏清屏/读回与 CPU fallback；普通桌面仍采用软件合成。
M15.1/M15.2 已验证容量测量、系统内 bzip2 源码重建、重启与编译失败恢复。
M15.3/M15.4 已通过 GCC 及独立 Rust/LLVM/Cargo 可行性实测评估，当前开发依赖和容量限制阻塞编译器重建；编译器/内核自举仍未验证。

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
| [docs/M11_TOOLCHAIN.md](docs/M11_TOOLCHAIN.md) | 系统内离线 GCC/G++/make、sysroot 与开发步骤 |
| [docs/validation/M11.md](docs/validation/M11.md) | 工具链源码构建、C/C++/make、负例与重启证据 |
| [docs/M12_NETWORK.md](docs/M12_NETWORK.md) | 可选联网 Cargo、严格 TLS、公网来源及断网缓存重编译 |
| [docs/M12_RUST.md](docs/M12_RUST.md) | 系统内 Rust/std、离线 Cargo、构建脚本、C FFI、过程宏及资源范围 |
| [docs/validation/M12.md](docs/validation/M12.md) | 实际系统内编译、持久文件校验、重启和回归证据 |
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
| [docs/M10_NETWORK_PACKAGES.md](docs/M10_NETWORK_PACKAGES.md) | 受控 IPv4、DNS、TLS 与持久软件包范围 |
| [docs/validation/M10.md](docs/validation/M10.md) | 网络、软件包、TLS、重启与回归证据 |
| [docs/M13_SDK.md](docs/M13_SDK.md) | 版本化 SDK、应用清单、安装升级流程与故障符号 |
| [docs/validation/M13.md](docs/validation/M13.md) | SDK、应用包、诊断和保留回归的实际证据 |
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
- 使用 `cargo-online build` 获取固定 registry/Git 依赖，重启后用 `--offline --frozen` 从缓存重编译。
- 完整调试器、更广泛的 Cargo 构建与编译器自举后续分别验证。

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

以上命令已在宿主 `xtask` 包中实现。镜像、缓存、日志和临时固件变量副本均位于 `build/`；普通启动使用临时数据盘、不连接网络。GitHub Actions 已配置相同检查，其运行结果与 [本地验收](docs/validation/M12.md) 分开记录。

## 下一步

M13 已交付版本化 SDK 与应用格式，见 [M13 SDK](docs/M13_SDK.md)。M14.1 已交付受检软件绘制，M14.2 已实现有界 PS/2 输入/游标诊断；M14.3 用户态合成器和 M14.4 GUI runtime/SDK 已通过本地验收；M14.5 已验收[有界硬件清屏/读回](docs/M14_GPU.md)及 CPU fallback；普通桌面仍采用软件合成。当前 Rust/Cargo 范围见 [M12 Rust](docs/M12_RUST.md)，另见 [当前状态](docs/STATUS.md) 和 [路线图](ROADMAP.md)。

## M15 受控源码重建

```sh
cargo xtask bootstrap-prepare
cargo xtask bootstrap-test
```

在独立的 16 MiB Linux personality 工作盘上准备校验锁定的官方 bzip2 1.0.8 源码，
由 guest GCC 编译、链接工具，压缩字节须与官方样本一致。三次 QEMU 启动分别验证
资源越限与回收、源码重建、正常重启及真实编译失败后的旧产物保留。容量报告明确区分
调度边界采样与关机后磁盘用量。见 [M15 设计](docs/M15_BOOTSTRAP.md) 与
[本地验收](docs/validation/M15.1-M15.2.md)。

`cargo xtask bootstrap-gcc-test` 审计七个锁定源码归档，实际验证 C++11 种子编译、
缺失头文件/工具、configure 启动失败、越限恢复与重启。评估报告明确当前 GCC
重建障碍，没有产出新编译器阶段。见 [M15 GCC](docs/M15_GCC.md) 与
[本地实测](docs/validation/M15.3.md)。GCC 路线下一步需补齐构建工具、数学库开发
依赖和实际所需容量。

`cargo xtask bootstrap-rust-test` 独立审计官方 Rust 1.90.0 完整发行源码、
所含 LLVM/Cargo 组件和规定的 stage0。两次 guest 启动验证 Rust std/C++17 种子、
实际构建工具缺失、独立离线依赖拒绝、真实 Rust 编译失败后旧 ELF 保留及重启哈希一致。
当前 profile 仍阻塞源码重建，没有重建 LLVM/rustc/Cargo 组件。见
[M15 Rust](docs/M15_RUST.md) 与 [本地实测](docs/validation/M15.4.md)。
M15.5 可沿用已验收的 guest rustc 独立推进；M15.5/M15.6 尚未实现。

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

## M10 受控网络与软件包

```sh
cargo xtask network-test
cargo xtask package-test
cargo xtask run --network --disk build/disks/work.ext2
```

系统内使用 `pkg install demo 1`、`pkg status demo`、`pkg install demo 2` 和
`pkg remove demo`。`/persist/data/demo` 中的用户数据在升级和卸载时保留。
`dns NAME` 查询 IPv4 A 记录。HTTPS 获取当前仅验收宿主受控仓库和生成的测试 CA。
具体范围见 [M10 设计](docs/M10_NETWORK_PACKAGES.md) 与
[验收记录](docs/validation/M10.md)。M12 在此基础上加入下述离线 Rust/Cargo 开发范围。

## M11 离线 C/C++ 开发

```sh
cargo xtask toolchain-build
cargo xtask toolchain-image
cargo xtask root-image --linux build/disks/devel.ext2
cargo xtask run --toolchain --disk build/disks/devel.ext2
cargo xtask toolchain-test
```

开发镜像使用 1 GiB 内存。在系统 shell 的 `/persist` 中用 `edit` 编辑保存源码，
再运行 `gcc`、`g++ -std=c++17 -pthread` 和 `make`。已验证 C 的 stdio、文件、
malloc、参数和多文件构建，以及 C++ string/vector、RAII、异常、线程、make
增量构建和正常关机重启保留。范围见 [M11 设计](docs/M11_TOOLCHAIN.md) 与
[验收记录](docs/validation/M11.md)。


## M12 Rust/Cargo 开发

```sh
cargo xtask rust-build
cargo xtask rust-image
cargo xtask root-image --linux build/disks/rust-devel.ext2
cargo xtask run --rust --disk build/disks/rust-devel.ext2
cargo xtask rust-cross-test
cargo xtask rust-runtime-test
cargo xtask rust-test
```

可选镜像使用 3 GiB 内存，组合校验锁定的官方 Rust 1.90.0 musl 二进制包和
M11 源码构建的 C 工具链。系统内 rustc 编译 std 应用与过程宏共享库；离线
Cargo 构建锁定的 vendor crate，执行 build.rs，并链接系统内编译的 C 代码。
还验证 Cargo 过程宏及其普通依赖、构建依赖和生成源码；支持库修改触发依赖重建，
非法宏输入返回 101 后旧程序仍可运行。验收检查构建新鲜度与正常关机重启后的执行。
详见 [M12 设计](docs/M12_RUST.md) 与 [验收记录](docs/validation/M12.md)。
联网项目在 `run --rust` 时增加 `--network`，在 `/persist` 下编辑项目，使用
`cargo-online build`。缓存默认保存于 `/persist/cargo-home`；普通 `cargo` 保持离线
默认。正常关机后无网卡重启，可用 `cargo-online build --offline --frozen` 继续构建。

`cargo xtask rust-online-test` 验证受控 HTTPS sparse registry/Git、CA/名称/日期检查、
损坏及截断下载、超时与断网全量重编译。`cargo xtask rust-public-test` 独立验证固定的
crates.io 与 GitHub 来源；本地公网验收使用可达的宿主代理和公开 PEM 根证书。
配置与资源限制见 [M12 联网范围](docs/M12_NETWORK.md)。AnovaOS 原生 std、任意
Cargo 项目与编译器自举仍独立推进。

## M13 SDK 与应用包

```sh
cargo xtask sdk
cargo xtask app-new c build/hello-c
cargo xtask app-build build/hello-c
cargo xtask app-pack build/hello-c/app.toml build/hello-c/hello-c.apkg
cargo xtask sdk-test
cargo xtask app-test
cargo xtask package-v2-test
cargo xtask diagnostics-test
```

用 `cpp` 或 `rust` 创建另外两种原生模板。`app-run PACKAGE SHA256` 会把已校验的自建包放入镜像，创建匹配 ABI 的临时磁盘并打印可直接使用的 guest 安装和运行命令；`--disk` 可复用显式持久镜像。完整创建、构建、安装和升级步骤见 [M13 SDK](docs/M13_SDK.md)。原生 SDK 与 M11/M12 的 Linux-musl 系统内编译器分别验收。权限声明用于安装准入；GUI runtime、GDB stub、进程沙箱和原生 Rust std 不属于当前成果。

## M14.1–M14.2 软件显示与输入

```sh
cargo xtask graphics-image
cargo xtask graphics-test
cargo xtask input-test
cargo xtask run --graphics
```

可选 profile 绘制有界 CPU 测试图案并保留 COM1 shell；同时轮询 QEMU PS/2 键盘与
三按钮相对鼠标，向一个诊断消费者投递物理事件并绘制裁剪游标。文本和窗口焦点策略
另行实现。输入与游标存储不分配堆内存；交互运行需要 GTK 显示环境。

`graphics-test` 验证启动截图与有/无 VGA 的串口恢复；`input-test` 验证有序 QMP 输入、
五个游标位置的全部像素、有界 reset/重连，以及缺少显示设备或控制器时的恢复。
M14.2 本地 QEMU 验收已通过。USB、物理热插拔、外设 IRQ、用户态窗口及 GUI SDK
不属于当前 profile。
见 [M14 设计](docs/M14_GUI.md)、[M14.1 证据](docs/validation/M14.1.md)
与 [M14.2 证据](docs/validation/M14.2.md)。

## M14.3 隔离的原生窗口

```sh
cargo xtask gui-image
cargo xtask gui-test
cargo xtask run --gui
```

交互模式中，在 COM1 shell 输入 `gui-test` 启动受监督的合成器和两个独立原生窗口。
点击切换焦点，拖动标题栏移动，拖动右下角缩放。在红色窗口按 X 可触发客户端异常，
F12 触发合成器异常及重启，重启后按 F11 结束并返回串口 shell。
交互显示需要 GTK；自动化验收以无界面方式运行。

本地三个启动配置已通过验收，包括八张完整逐像素截图、焦点输入、窗口重绘、
客户端/合成器各自真实异常、新会话重连、堆耗尽回滚与串口/设备缺失恢复。
每客户端拥有一个独立的 32×24 至 320×240 RGB surface，最多四个连接；
画面通过有界 CPU 复制提交。下文 M14.4 已交付文字/控件、图形终端及公共
C/C++/Rust GUI SDK；GPU 加速仍按独立阶段推进。
见 [M14.3 设计](docs/M14_COMPOSITOR.md) 与 [验收记录](docs/validation/M14.3.md)。

## M14.4 GUI runtime 与 C/C++/Rust SDK

```sh
cargo xtask gui-sdk
cargo xtask gui-sdk-image
cargo xtask gui-sdk-test
cargo xtask run --gui-sdk
```

无动态分配的 Rust runtime 实现独立版本化 C ABI、原创位图文字、裁剪绘图、布局、
按钮、文本框及有界进程内剪贴板。C 头文件、C++ move-only RAII 和 Rust 安全封装
使用同一套语义。开发镜像包含已验收的 GCC/G++ 和 Rust 工具，guest 中通过
`gui-build c`、`gui-build cpp`、`gui-build rust` 分别编译独立的原生应用源码；
原生数据盘在正常关机、重启后保留源码和可执行产物。

Rust note 应用演示编辑和复制。受限图形终端支持输入 `echo TEXT` 与 `pwd`，
通过真实原生命令进程和管道取得、显示输出；不提供 PTY/作业控制或 ANSI 模拟。
构建后运行 `gui-run /persist/gui-c`，使用物理按键 `a`、点击按钮，再按 `q`
关闭应用，最后 F11 结束合成器并返回串口输入。完整步骤与边界见
[开发说明](docs/DEVELOPMENT.md)、[runtime 契约](docs/M14_RUNTIME.md) 和
[M14.4 验收](docs/validation/M14.4.md)。有界 GPU 路径见下文。

## M14.5 有界 GPU 绘制与 CPU fallback

```sh
cargo xtask gpu-test
cargo xtask gpu-fallback-test
```

完整 gate 使用第二块 virtio-gpu/VirGL 设备完成真实离屏 render-target 清屏和 fence 读回。
本地该设备实际报告 `D3D12 (NVIDIA GeForce RTX 4060 Ti)`。测试逐像素对照 CPU 颜色，
验证非整齐尺寸的输出保护、非法参数拒绝、真实 timeout/reset、CPU fallback 与 GPU 重建。
无界面的 fallback gate 覆盖仅 2D 和缺少 GPU 的情况，并已纳入 CI。

认证的 Rust 合成器通过保留的 VGA framebuffer 显示结果。普通桌面合成与画面复制仍在 CPU
完成；这不证明 shader draw、guest GL/Vulkan API 或性能提升。范围、运行环境及证据见
[M14 GPU](docs/M14_GPU.md)、[开发说明](docs/DEVELOPMENT.md)和
[M14.5 验收](docs/validation/M14.5.md)。
