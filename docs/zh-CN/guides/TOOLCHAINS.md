<!-- Explain build/host/target roles, accepted guest toolchains and separately scoped compiler and kernel reconstruction. -->
# 从交叉编译到系统内开发

[English](../../en/guides/TOOLCHAINS.md) | **简体中文**

## 1. build、host、target 是不同角色

| 阶段 | build | host | target | 证明内容 |
| --- | --- | --- | --- | --- |
| 内核/应用交叉构建 | Linux 开发机 | Linux 开发机 | AnovaOS 内核/用户 ABI | 宿主能为 OS 生成代码 |
| 准备 guest-hosted 编译器 | Linux 开发机 | AnovaOS 受测用户 ABI | AnovaOS 受测用户 ABI | 工具准备完成，仍需 guest 实测 |
| guest 应用构建 | AnovaOS | AnovaOS | AnovaOS | 系统内应用开发 |
| 编译器自举 | AnovaOS | AnovaOS | AnovaOS | 声明范围的组件在系统内重建 |

guest 用户 ABI 可为 Linux x86_64 musl。在 guest 能重建完整 GCC/LLVM 之前，宿主构建的
编译器即可运行。区分“工具运行”和“自举”。

## 2. 两条用户态路线

### 路线 A：通过 Linux 兼容层复用工具

保持自研 Rust 内核，C/C++ 使用受控 Linux x86_64 musl sysroot，Rust 可采用
`x86_64-unknown-linux-musl`。程序调用的 Linux ABI 语义必须真实实现。先静态/受控依赖工具
包配合编辑器、shell、libc、链接器与开发输入。自研 OS 不自动满足官方 Linux target 的运行
假设。该路线减少初期专用平台移植，但兼容内核语义仍需大量工作。

### 路线 B：独立 AnovaOS 原生 target

定义 native syscall、CRT、栈、动态加载/libc 约定；增加编译器配置、启动/链接规则、C/C++
运行库。可评估 newlib，但它不意味着完整 POSIX。Rust 从自有 `core/alloc` 开始，再移植
std 的平台/libc/线程/文件/时间/进程层。`x86_64-unknown-anovaos` 只是候选本地名称，
不是已有上游 Rust/GCC target。target JSON 只提供部分代码生成配置，不提供运行时/标准库。
上游化之前先锁定本地补丁与差异。路线 A 应用开发和路线 B 原生 std 分别报告。

## 3. 内核与首批应用

`x86_64-unknown-none` 提供裸机 ELF core、不提供 std，初始化分配器后才使用 alloc。
内核链接/no-red-zone 参数不能污染用户程序或宿主 xtask。优先 stable，确需 nightly 时锁定
实测日期；实验性 `-Z build-std` 不会移植 std。

M4 的 `_start`、native runtime、用户链接脚本和 write/exit 封装支持 Ring 3 no_std 应用。
程序通过 syscall 退出，不返回无效地址或直接调用内核。用户 code model、red zone、SSE/FPU
及链接布局独立定义。首批 std 应用用锁定 Linux-musl 工具交叉构建，再测 guest 分配/参数/
文件/错误。通过检查 ELF 判断静态/PIE，不从 target 字符串推断。

## 4. GCC/G++ 链路

### 前置条件

内核需要已测 exec/wait、地址空间、文件/目录/临时文件、管道、时间、信号等；用户态需要
shell、CRT、头文件、libc、assembler/linker、编译支持库和一致 sysroot。单独一个
`x86_64-elf-gcc` 不提供完整用户 OS 的头文件/libc/启动/运行时，不能证明应用开发可用。

准备实际 gcc/cc1/assembler/linker/collect2 和搜索路径；区分 GMP/MPFR/MPC 构建与运行依赖。
版本 banner 不能证明子工具可完成构建。记录已验 configure/sysroot/补丁；修改 `--host`
不能证明移植。静态工具减少共享库部署，但仍需 fork/exec/mmap/信号/线程/libc。TCC 可以
支持早期实验，不能代替 GCC/G++ 验收。

C++ 还需 cc1plus、libstdc++、libgcc/libgcc_s 或声明的替代、ABI、构造/析构和线程运行时。
禁用异常/展开的配置需明确说明，启用后另验。

### 已验收 M11 离线流程

[M11](../milestones/M11_TOOLCHAIN.md) 已支持以下 guest 命令：

```sh
gcc main.c -o hello-c
./hello-c
g++ main.cpp -o hello-cpp
./hello-cpp
make
```

C 覆盖 stdio/argv/文件/malloc/多源及非零编译失败；C++ 覆盖 string/vector/RAII、异常展开和
真实线程，更广标准库按程序验证。测试包含子工具/头文件/临时文件、make 无操作与选择性增量、
链接失败、guest 编辑源码、guest 生成 ELF 和重启保留。

## 5. rustc、Cargo 与 LLVM

LLVM/rustc 对内存、TLS、线程、同步、文件/映射/信号/运行时的需求高于 hello。先限制后端和
并行数，按明确采样范围测量资源，再评估完整自举。工具包需匹配的 rustc/std/target 文件/
LLVM 依赖、C linker 和 sysroot；只有 rustc 不能完成普通 ELF 最终链接。

### 分层 Cargo 验收

| 层次 | 能力 | 额外前置条件 |
| --- | --- | --- |
| R1 | guest rustc 构建单文件 std 应用 | 编译器、std、链接器、OS 语义 |
| R2 | 无外部依赖 Cargo 应用 | 子进程、文件、环境、元数据、锁 |
| R3 | 锁定离线依赖 | registry/vendor/cache、Cargo.lock |
| R4 | build.rs、C FFI | host/target 区分、脚本与 C 工具 |
| R5 | proc-macro | 编译器 host 动态库、匹配 ABI/搜索路径 |
| R6 | 在线 crates.io/Git | 网络、DNS、TLS、CA、时间、传输 |

M12 验收 R1–R3、build.rs/C FFI、独立宏和固定 Cargo 宏/构建脚本图。宏图检查共享 host/build
依赖、生成输入/环境、依赖变化重建、拒绝宏输入并保留旧产物。R6 独立验收严格 HTTPS sparse
registry/Git、受控负例和无 NIC 重启后缓存新 target 重建。公开门禁使用锁定 crates.io/GitHub
输入、不替换 vendor，本地验收经宿主代理/公共 PEM 根。见 [M12 Rust](../milestones/M12_RUST.md)、
[M12 网络](../milestones/M12_NETWORK.md) 与 [证据](../validation/M12.md)。Rust 1.90.0/LLVM/
Cargo/std 是官方锁定 musl 二进制，不是本地 Rust 源码构建。

build.rs 和 proc-macro 在编译器 host 执行，可能与应用 target 不同；guest 构建也要区分，
不能将构建脚本按内核目标编译。

### 已实现流程

启动 `cargo xtask run --rust`，在 AnovaOS 内运行：

```sh
rustc main.rs -o hello-rust
./hello-rust
cargo build --offline
./target/debug/m12-app
```

验收 String/Vec/参数/文件、线程、工作线程子进程输出/spawn/wait、脚本、共享宏、C FFI、锁定
vendor、无操作/源码变化、真实语法失败及干净重启。在线 `/persist` 工程启动
`run --rust --network` 后用 `cargo-online build`，缓存默认 `/persist/cargo-home`。
无 NIC 重启后用 `cargo-online build --offline --frozen`。普通 cargo 默认离线。
通用项目、原生 std 和编译器自举独立。宿主门禁为 `cargo xtask rust-build`、`rust-cross-test`、
`rust-runtime-test`、`rust-test`、`rust-online-test` 和独立 `rust-public-test`；完整命令/边界见 M12。

## 6. 文件系统与包内容

目标布局考虑 `/bin`、`/usr/bin`、`/usr/lib`、`/usr/include`、`/tmp`、`/dev` 和实际需要的
`/proc`，当前工具归档使用 `/tc`、`/rs`。遵循真实搜索路径，解释器位置由 PT_INTERP 决定，
不能任意重命名。记录编译器版本、host/target ABI、sysroot/libc/CRT/库、哈希、补丁与通知。
应用 ABI 单独记录，防止将用户 ELF 当作内核镜像。

## 7. 应用开发与源码自举

在 AnovaOS 内用准备好的编译器构建并运行应用，已经达到系统内开发目标。重建 GCC/Rust/LLVM/
内核是额外 M15 工作，需要更多生态与容量，不能成为应用开发的隐含前置条件。

[M15](../ROADMAP.md#m15可选自举与系统自编译) 分离容量/工作盘、小型 C/C++ 工具、GCC、
Rust/LLVM/Cargo、内核启动与重复比较/恢复。[M15.1/M15.2](../milestones/M15_BOOTSTRAP.md)
及 [证据](../validation/M15.1-M15.2.md) 验收有界 bzip2 重建。
[M15.3 GCC](../milestones/M15_GCC.md) 审计锁定 GCC/GMP/MPFR/MPC/工具源码，验证继承 C++11、
缺头文件/工具、configure 启动和容量恢复。[证据](../validation/M15.3.md) 为评估成功，非重建编译器。

[M15.4 Rust](../milestones/M15_RUST.md) 独立审计发行源码/LLVM/Cargo/vendor、发行要求的 stage0
和依赖。Rust std/C++17 种子、工具 exec 失败和隔离离线 Cargo 依赖拒绝有
[本地证据](../validation/M15.4.md)。没有重建 LLVM/rustc/Cargo；继承 Rust 1.90.0 不等于
规定的 1.89.0 stage0。GCC 与 Rust/LLVM 重建仍受工具/依赖/容量阻塞；完整归档不是最小源码
闭包，裁剪路线需重新评估。

[M15.5 内核](../milestones/M15_KERNEL.md) 使用继承裸机库、确切离线源码及实际 guest Cargo/
rustc/rust-lld，宿主原样打包保留 ELF，不重建，比较 ISO 提取字节并验证启动/恢复。
见 [证据](../validation/M15.5.md)。guest 独立镜像生成另计；内核可先使用已验收 guest rustc，
不要求先重建该编译器。

[M15.6 重复构建](../milestones/M15_REPEAT.md) 已本地通过：bzip2/内核各两次独立干净构建、
声明输入/最终字节一致、12 次启动/拒绝/恢复。内核中间差异保留，见
[证据](../validation/M15.6.md)。只证明声明条件，不是普遍可复现或无种子自举。区分 guest
内核编译、宿主镜像打包、guest 独立镜像生成，保留输入/种子/配方身份并披露差异和宿主参与。

## 8. M13 原生 SDK 与独立 GUI SDK

源码分发 [SDK 1.0.0](../milestones/M13_SDK.md) 导出独立宿主 C/C++/Rust native v1 工程、锁定
本地 Rust 依赖、稳定错误与明确包元数据。freestanding 模板无 libc、C++ STL/异常或 Rust std。
普通用户 target 参数/SDK 链接器与内核分离。runtime 参数助手遵循原有 64 项、每串 1,023
字节边界，包含边界及超限测试。M11/M12 继续使用独立验收的 Linux-musl 工具与显式 Linux
磁盘策略。原生 SDK 不等于原生 libc/std 或编译器自举。M14.4 独立版本
[GUI SDK](../../../sdk/gui/README.zh-CN.md) 已本地验收，通过独立 `anova.gui.v1` 库 ABI
使用相同 native 应用 personality。

## 9. 计划中的资源、桌面与自举顺序

[路线图](../ROADMAP.md) 将 [M16 资源/VFS/ext4](../milestones/M16_FOUNDATION.md) 安排在
[M17 开发桌面](../milestones/M17_DESKTOP.md) 之前，随后有条件推进 M18 编译器/镜像工作。
这些阶段均未实现。先恢复经校验的 CI 输入获取和后续工具链执行，再在扩容工作空间上
验证选定 POSIX 工具和真实构建负载，保留现有编译器种子和门禁。

M17 使用已验收的 GCC/G++ 与 Rust/Cargo，不要求先重建它们。原生 GUI 与 Linux-musl
编译器/shell 进程仍需显式准入和已测试的 PTY 桥接。原生 std、重建编译器和 guest 独立
启动镜像各自独立。M18 必须重新评估每个源码闭包、数学库依赖、构建工具、发行要求的
stage0 和离线 vendor 条件；旧 M15 评估不是新的构建证据。
