<!-- Define mandatory repository development, safety, validation and delivery conventions. -->
# AGENTS.md — AnovaOS

## 1. 项目目标与当前事实

这是 Rust 编写的 x86_64 自研操作系统。最终用户应能在系统自己的终端中编辑源码、使用 GCC/G++/Rust/Cargo 编译、链接并运行应用，同时逐步运行经过验证的现有 Linux 软件。

当前 M0–M13（受限离线与联网范围）已通过本地 QEMU TCG 验证。原生进程、VFS、init/shell、virtio-blk、受限 ext2 `/persist`、文本编辑、sync/fsync 和正常关机/重启已实现。M9 还验证 musl 1.2.4 静态、静态 PIE、动态 ELF、pthread/TLS/futex、信号栈帧/返回、中断/重启、poll、非阻塞 I/O 和四组 PTY。**单核，共 32 个进程/线程槽；内核不可抢占；tmpfs 易失；ext2 无日志和断电恢复、单文件最多 1 MiB；无 SMP 或通用 Linux/glibc；引导保留区未回收，堆固定 16 MiB。** 外设 IRQ 仍屏蔽，CI 远端结果单独记录。先读 [M9_RUNTIME](docs/M9_RUNTIME.md)、[STATUS](docs/STATUS.md)、[M8_LINUX](docs/M8_LINUX.md)、[M7_STORAGE](docs/M7_STORAGE.md) 和 [开发需求](docs/REQUIREMENTS.md)，再检查真实代码与 git 状态。

M10 adds opt-in, polled virtio-net IPv4 TCP/UDP sockets, bounded user-space DNS,
strict user-space TLS and checksum-locked native package install/upgrade/rollback/
remove with user data retained. Read [M10 profile](docs/M10_NETWORK_PACKAGES.md)
and [validation](docs/validation/M10.md). Public Internet package repositories,
TCP servers, general Linux networking and package dependency solvers remain outside
this accepted controlled profile. Preserve both commands
`cargo xtask network-test` and `cargo xtask package-test` alongside every M9 gate.

M11 adds the optional source-built GCC/G++ 11.2.0, binutils/make and musl/CRT/
C++ sysroot development image. Read [M11_TOOLCHAIN](docs/M11_TOOLCHAIN.md) and
[validation/M11](docs/validation/M11.md). Preserve `cargo xtask toolchain-test`
with all previous gates. Current bounds are a 16 MiB heap, 256 KiB scheduler
stack, 1 MiB Linux user stack and tmpfs files, 2,048 VFS nodes. M12 expands only Linux startup/ELF/mmap bounds; native ELF
and 64-entry native startup limits remain separate.
Development builds use opt-level 1 with debug information.

M12 adds the optional checksum-locked official Rust 1.90.0 musl binary tools,
matching std/rust-lld and offline vendor profile at `/rs`. Read
[M12_RUST](docs/M12_RUST.md) and [validation/M12](docs/validation/M12.md).
Preserve `rust-cross-test`, `rust-runtime-test` and `rust-test` alongside all
previous gates. Actual guest compilation covers std, worker-thread child wait,
build.rs, C FFI, a standalone procedural macro and a fixed Cargo macro graph
with shared host/build dependencies. Preserve dependency-change rebuilds,
macro input rejection and retained prior outputs. It does not establish Rust source
construction, native AnovaOS std or compiler bootstrap.

The opt-in `cargo-online` profile adds strict HTTPS sparse-registry/Git downloads,
a pinned public CA store and persisted-cache rebuilds. Read
[M12_NETWORK](docs/M12_NETWORK.md). Preserve `rust-online-test` alongside the
original Rust and OS gates; `rust-public-test` independently covers pinned
crates.io/GitHub sources and may need the documented test proxy/public CA inputs.
Local public acceptance used that proxy; it does not establish direct public
DNS/transport, arbitrary Cargo graphs, SSH or authenticated private registries.
Readonly file-backed MAP_SHARED is coherent within the bounded cache/inode
lifetime; writable shared-file mapping/writeback remains unsupported.

Linux accepts 256 argv/env entries, a checked 32 KiB startup block, 256 MiB ELF
and per-mmap bounds, the `[0x50000000, 0xc0000000)` mapping window and bounded
512-byte paths/255-byte components. Native v1 directory records remain fixed;
Linux getdents uses extended internal entries. Heap,
writable-file, thread-slot and single-BSP bounds remain. M13 delivers SDK 1.0.0,
ANOVAPK2 and the bounded developer workflow. Read [M13_SDK](docs/M13_SDK.md)
and [validation/M13](docs/validation/M13.md). Preserve `sdk-test`, `app-test`,
`package-v2-test` and `diagnostics-test` alongside all previous gates. Native
syscall v1 remains frozen; permissions are admission metadata, and static
ET_EXEC Linux package admission is separate from the wider M9 runtime profile.
The mounted persistent ABI marker is immutable. M14.1 adds optional checked CPU
graphics and actual QMP scanout/serial-fallback verification. Preserve
`cargo xtask graphics-test` and the `graphics` kernel feature check alongside
previous gates. M14.2 implements optional scheduler-polled PS/2 physical input,
a bounded diagnostic queue and clipped cursor; its local three-boot QEMU gate passed.
Preserve `cargo xtask input-test` and the `graphics,input-test` kernel feature
check as well. See [M14 GUI](docs/M14_GUI.md),
[validation/M14.1](docs/validation/M14.1.md) and
[validation/M14.2](docs/validation/M14.2.md). M14.3 adds an isolated user-space compositor, authenticated bounded copied IPC,
private surfaces, focused input and actual client/server-fault restart verification.
Preserve `cargo xtask gui-test` and both `graphics,gui` and
`graphics,gui,gui-test` kernel checks. Read [M14 compositor](docs/M14_COMPOSITOR.md)
and [validation/M14.3](docs/validation/M14.3.md). M14.4 adds the bounded Rust
GUI runtime, versioned C ABI, C++/Rust ownership wrappers, note/interactive
graphical terminal and independently guest-built native clients. Preserve
`cargo xtask gui-sdk-test`, `anova-gui-runtime` host tests and generated GUI
header checks. Read [M14 runtime](docs/M14_RUNTIME.md) and
[validation/M14.4](docs/validation/M14.4.md). M14.5 GPU rendering remains open;
GDB stubs, native std and compiler bootstrap remain independently scoped.

本文件是后续 Codex 开发规范。用户明确指令优先；已有修改应保留。日常、可逆的实现选择可自主完成，不因普通阶段切换反复请求确认。

日常开发基线与 Pull Request 默认目标为 `dev`。除用户明确指定外，不直接在 `main` 开发；`main` 用于接收阶段验收后的稳定快照。开始工作前检查并切换到实际的 `dev` 或由其创建的任务分支，不仅依赖 GitHub 页面当前选中的分支。

本项目原创代码、文档与示例采用 `Apache-2.0`；保留根目录 `LICENSE`、`NOTICE` 和已有归属声明。第三方代码保持原许可证，具体边界见 [docs/LICENSING.md](docs/LICENSING.md)。

## Development language and formatting (mandatory)

- Every new or modified first-party file must explain its purpose near the top. Rust source uses module Rustdoc (`//!`); assembly, C, linker scripts, manifests, configuration and workflows use the format's comments. Describe responsibilities and relevant boundaries, not just the filename. Keep these explanations in English and update them when responsibilities change.
- Formats that cannot carry comments, generated files, binaries, and unmodified third-party files are documented in [docs/FILE_PURPOSES.md](docs/FILE_PURPOSES.md) or their adjacent README. Do not hand-edit generated lockfiles or upstream license text just to add a header. A task is not complete until purpose coverage for every new/modified file has been reviewed.
- Use English for first-party identifiers, source filenames, comments, Rustdoc, `SAFETY:`/`TODO:`/`FIXME:` explanations, logs, errors, panic/assertion messages, CLI help, test names, commit messages, and pull request titles/descriptions.
- Write new development documentation in English. Explicit translations, localization resources, multilingual test fixtures, quoted external text, and third-party notices may retain their original language. Existing Chinese planning documents may be maintained until translated.
- Keep [README.md](README.md) as the default English README and [README.zh-CN.md](README.zh-CN.md) as its Chinese translation; update both together when shared content changes.
- Follow [docs/CODING_STYLE.md](docs/CODING_STYLE.md) and the root [rustfmt.toml](rustfmt.toml). Use four spaces, LF, a preferred width of 100 columns, and Rust 2024 parser/style editions. First-party Cargo packages must declare `edition = "2024"` when created.
- M0 must pin a tested toolchain with `rustfmt` and `clippy`. Once the workspace exists, run `cargo fmt --all -- --check` before delivery and use `cargo fmt --all` to apply formatting. Before then, validate only existing files/configuration and report the scope honestly.
- Keep formatting options stable-compatible. Avoid nested configs, unexplained formatter skips, and reformatting third-party/generated code. Rustfmt does not enforce English or verify `SAFETY:` explanations; review those separately.

## 2. 开始任务前

1. 读取本文件、[README](README.md)、[ROADMAP](ROADMAP.md)、[ARCHITECTURE](ARCHITECTURE.md)、[STATUS](docs/STATUS.md)。
2. 做兼容层时再读 [COMPATIBILITY](COMPATIBILITY.md)；做编译器或 SDK 时读 [TOOLCHAINS](docs/TOOLCHAINS.md)。
3. 检查 `git status`、当前分支、相关代码和适用的子目录 AGENTS；优先 `rg` 搜索。
4. 确认当前阶段、前置依赖、最小验收闭环和可用工具。未知协议/ABI 细节查官方文档。
5. 无具体任务时，从第一个未完成的必要阶段推进。初次实现按 [FIRST_TASK](docs/FIRST_TASK.md) 执行。

不要一次性生成大量不能构建的子系统。每批工作围绕一个可启动或可执行的结果完成实现、验证和文档同步。

## 3. 架构基线

- x86_64，小端，4 级页表，先 BSP 单核；CPU 特性必须探测。
- UEFI + Limine；首个参考机器是 QEMU q35 + OVMF，TCG 基线可运行。
- Rust 模块化单体内核，`#![no_std]`；M3 已启用 `alloc`，初始化堆前禁止分配。
- 内核采用 `x86_64-unknown-none`、内核链接脚本、无 red zone、初期 panic abort。
- 内核不得依赖宿主机 libc/`std`。FFI 和汇编仅放在清楚界定的边界内。
- 宿主 `xtask` 使用宿主目标；native 用户程序、Linux 兼容程序使用各自用户态目标。禁止把内核的 code model、链接脚本、red-zone/FPU 假设误用于普通用户程序。
- 内核入口按已锁定的 Limine 协议修订实现；不要直接把 `extern "efiapi"` 的 UEFI 应用入口当成内核入口。
- 初期不自行重写完整 bootloader，不引入 Linux 内核作为实际运行内核，不为演示 GUI 重排核心依赖。

## 4. 工作目录约定

现有文档位于根目录与 `docs/`。规划的代码布局见 [ARCHITECTURE](ARCHITECTURE.md)，按实现需要逐步创建；不要提交一片占位目录或空模块来充当功能。

内核中的 `arch/` 处理 CPU 机制，`mm/` 处理内存，`task/` 处理执行实体，`fs/` 处理统一文件语义，`abi/` 处理外部 ABI 转换。Linux 结构体/错误码/编号不能散落到通用 VFS、调度器和驱动中。

用户 SDK、宿主镜像工具和兼容测试均不应反向成为内核的平台依赖。

## 5. 工具链与依赖

- M0 验证后写入精确 `rust-toolchain.toml`，包含 `rustfmt`、`clippy` 组件；不能永久使用浮动 `nightly`/`stable` 作为可复现基线。
- 能用 stable 则用 stable。确需 nightly 功能时固定日期，并写出所需 feature 与原因。
- 提交 workspace `Cargo.lock`；固定引导器、协议 crate、固件、测试软件的版本/提交与校验和。
- 创建自有 Cargo package 时填写 `license = "Apache-2.0"`；新建自有源码使用 `SPDX-License-Identifier: Apache-2.0` 标记。第三方文件不能被批量改写成项目许可证。
- 尽量复用质量可靠的 `no_std` crate；引入前检查功能、许可、unsafe、分配与运行时假设。
- 下载须校验；禁止 `curl | sh`、静默执行未知脚本、把访问令牌写进镜像或日志。
- 不假定 `x86_64-unknown-none` 提供 `std`；自定义 target JSON 也不自动获得用户态标准库支持。
- 镜像只写入仓库构建目录里的普通文件，不访问真实块设备。默认 QEMU 使用临时工作盘或 overlay，持久化测试用独立镜像。

## 6. Rust、unsafe 与并发规范

- 使用 Rust 命名风格和清晰的小模块，避免无必要的抽象和依赖。
- 每个 `unsafe` 块写 `SAFETY:`，说明地址范围、对齐、生命周期、别名和同步前提中实际相关的部分。
- 指针、用户地址、物理地址和虚拟地址使用不同包装类型；转换只能通过明确接口。
- ABI 结构使用明确宽度与 `#[repr(C)]` 等必要布局，检查大小、对齐、填充和版本。用户指针不能作为可信引用直接构造。
- 对地址/长度/偏移做 checked 运算，验证跨页、页表权限、用户 canonical 地址与边界。
- 用户错误返回可诊断错误；不要为坏 ELF、坏指针或未实现 syscall 触发内核 panic。
- `copy_from_user`/`copy_to_user` 同时处理合法性和访问时故障；设计后续并发解除映射的情况，不只做一次范围检查。
- MMIO、DMA、端口 I/O、volatile 和内存屏障按设备协议处理。volatile 不替代并发同步。
- 不使用无说明的 `static mut` 或无约束全局可变状态。
- 写明锁顺序、能否睡眠、是否可在中断上下文获取；持 spinlock 时不能等待 I/O 或进行可能睡眠的分配。
- 不在普通 IRQ 路径里用可能递归的日志锁、等待锁或堆分配。panic/异常日志应有独立的尽力输出路径。
- 单核也遵守 IRQ 与线程的并发规则；启用抢占、SMP 或 FPU/SIMD 前补齐上下文和锁验证。
- 不通过全局允许 clippy 警告或扩大 unsafe 范围来掩盖问题。

## 7. 引导、内存与中断底线

- 内核、栈、页表、模块、ACPI、framebuffer、固件保留区域和仍被引用的引导数据均不能交给页分配器。
- 回收 bootloader-reclaimable 内存前完成数据拷贝和生命周期核查；设备地址不能当普通 RAM。
- 独立的异常栈/guard page、防双重故障路径、用户/内核权限、NX 与 W^X 应逐步建立并测试。
- 不在异常返回前恢复中断到错误状态；APIC IRQ 要按协议 EOI。
- 内核初期不生成浮点/SIMD；用户 ELF 可能使用 SSE2。M5 已保存 x87/MMX/XMM0–15/MXCSR，并禁用 XSAVE/AVX/FSGSBASE 与 FFXSR 省略 XMM 的行为；M9 线程复用已验收的完整状态保存；扩展 CPU 状态或启用 SMP 前必须补齐对应上下文。
- 时钟、调度和时间单位集中定义，超时不能用未经校准的忙循环充当稳定实现。

## 8. 应用 ABI 与 Linux 兼容

- 首先区分函数调用 ABI、syscall ABI、ELF 格式、libc ABI 和源码兼容。实现 ELF 解析不等于兼容 Linux。
- 原生与 Linux 进程由显式镜像清单或 exec 配置选择 ABI personality；不要仅凭 ELF OSABI 或“没有 PT_INTERP”猜测，因为很多 Linux ELF 也使用 System V 标记。
- 固定程序版本、构建参数与 SHA-256，在宿主 Linux 记录 syscall，再在 AnovaOS 验证实际行为。
- Linux syscall 编号、寄存器规则、负 errno、结构体布局、flags、TLS、信号帧、auxv、文件/线程语义均以 x86_64 UAPI 为准。
- 未实现返回 `ENOSYS`，不支持的参数返回对应错误；不得伪造成功来骗过 libc 探测。真实且受限的实现可用，但要写明支持范围。
- 静态 musl 程序仍可能需要 TLS、随机数、信号、futex 等；所需子集由固定构建实测确定，不能凭一张简短 syscall 表宣称兼容。
- 先静态 ET_EXEC、小应用，再受限静态 PIE/动态链接；`PT_INTERP`、TLS、重定位、共享库和线程分别验收。
- 优先保证 shell、文件工具、构建工具和编译器；glibc、Wine、浏览器、GPU 栈后置。
- 不复制 Linux 内核实现或假装其驱动可直接装入本内核。可依据公开规范独立实现，并追踪所引入代码的许可。

## 9. 开发工具链验收

分清三个阶段：宿主编译出的程序能运行；编译器本体能在 AnovaOS 中运行；编译器能在 AnovaOS 中重建自己。

先交叉构建 guest-hosted GCC/G++ 和 rustc/Cargo，补齐 guest libc、链接器、CRT、头文件、标准库、TLS、线程、信号、临时文件与工具运行时。`x86_64-elf-gcc` 的存在不能证明可编译完整用户态 C 程序；改 Rust target 字符串也不能证明有 `std`。

系统内应用开发验收必须记录编辑/保存 → 编译 → 链接 → 运行 → 重启保留的全流程。Cargo 先离线依赖，再在线，再 build.rs/proc-macro/C FFI；分别报告，不用 `hello world` 概括所有 Cargo 功能。

## 10. 已实现的统一命令

以下命令由 M0/M1 的宿主 `xtask` 提供；完整说明见 [DEVELOPMENT](docs/DEVELOPMENT.md)：

```sh
cargo xtask doctor
cargo xtask fetch
cargo xtask build
cargo xtask image
cargo xtask run
cargo xtask smoke
cargo xtask verify
cargo xtask test-host
cargo xtask apps
cargo xtask check-apps
cargo xtask check-files
cargo xtask terminal-test
cargo xtask root-image build/disks/NAME.ext2
cargo xtask run --disk build/disks/NAME.ext2
cargo xtask persistence-test
cargo xtask trace-compat
cargo xtask test-compat
```

`fetch` 下载并校验锁定输入；`doctor` 对缺少依赖明确报错。`verify` 的 13 种场景要求正常启动通过内存所有权/压力检查、断点返回和至少 8 次定时 IRQ；受控 panic/超时、#UD/#GP/#PF/#DF、普通栈 guard、只读写入、普通/NX 直映别名执行、解除映射后访问和堆 OOM 必须到达真实注入点、符合诊断与退出码并被拒绝。正常测试镜像还必须通过 Rust/C Ring 3 应用、11 个用户隔离用例、两类真实复制故障恢复、部分装载 OOM 回滚和 8 次重复执行的回收检查。M5 还要求两个计算进程保留完整 GPR/FPU 状态、真实抢占切换、阻塞/HLT、fork/exec/wait/孤儿与 32 槽复用、三个生命周期 OOM 回滚，以及物理页和堆恢复。M6 还验证文件/路径/共享偏移、fork/spawn/exec/cwd、fd 限额回滚、真实 tmpfs 堆耗尽恢复、管道阻塞/EOF/断管，以及 shell 5,000 字节管道和错误退出码。`terminal-test` 用两次普通启动验证串口退格/CRLF/Ctrl+D、shell 重启与 tmpfs 丢失。M7 还要求真实 PCI/virtio 队列与边界检查、ext2 间接块/稀疏/共享偏移/解除链接与复用、成功 reset 后 DMA/物理页/堆回收。`persistence-test` 用普通串口启动验证编辑器保存、源码字节和 ELF SHA-256、重启执行、e2fsck、只读盘、未知特性与真实 write/flush 错误。完整压力检查仅在测试镜像运行。`test-host` 覆盖 frame/buddy/page-table 的边界、保留、复用与 OOM 回滚，以及 ACPI/PM/异常判定。CPU 上下文见 [M2_CPU](docs/M2_CPU.md)，当前内存和锁规则见 [M3_MEMORY](docs/M3_MEMORY.md)。M9 还要求静态/PIE/动态 musl 的 pthread、futex requeue/超时、共享映射、信号返回和真实 SSE 恢复、缺解释器/共享库、线程内 exec/组退出、PTY/原始输入/前台 Ctrl+C，以及真实堆耗尽下的共享元数据 OOM 回滚。`terminal-test` 还通过真实 UART 运行动态程序和 Ctrl+C 中断；完整边界见 M9_RUNTIME。M8 `test-compat` 要求 Linux 探针、musl 和 BusyBox CLI、5,000 字节 ash 管道、CLOEXEC、TLS、exec/env、信号默认/忽略、OOM 回滚和资源回收；`trace-compat` 保存 Linux 宿主 QEMU-user trace。

## 11. 验证要求

- 文档修改检查本地链接、阶段/状态一致性和命令是否被明确标为计划；无代码时不伪造 Rust 构建结果。
- 有 workspace 后运行 `cargo fmt --all -- --check`，并按目标运行适用的构建、clippy 和相关测试，不盲目对所有目标执行 `cargo test --workspace`。
- 纯逻辑适合宿主测试；IRQ、分页、syscall、调度、真实驱动需要 QEMU/系统内测试。
- 冒烟测试必须有超时、明确成功标记、失败路径和可解析的 QEMU 退出结果。仅搜到一次启动 banner 不够。
- 至少验证相关负例：坏 ELF/越界指针、非法 syscall、OOM、磁盘边界、进程异常或阻塞唤醒，按改动范围选择。
- 持久化使用关机/重启的独立用例，不能被默认临时盘掩盖。
- 兼容能力按软件版本和实际功能列入 [COMPATIBILITY](COMPATIBILITY.md)，失败也要保留有用诊断。
- CI 在有真实可复现命令后再引入；必须失败即失败，不能 `continue-on-error` 掩盖核心回归。
- 缺少运行环境或网络受限时，完成不受阻的工作，准确报告未运行项目与原因。

## 12. 交付与文档同步

每批完成后更新相关架构/ABI/路线图、[STATUS](docs/STATUS.md) 和已验证命令。简明说明：实现内容、验证结果、仍有的限制、下一步。

阶段完成须有证据；TODO、占位返回值、日志文本和预生成图片不能算实现。不擅自回滚用户改动、不 force-push；新增依赖和架构变化需有实际理由。

常规实现、修复和检查可以持续推进。涉及改写真实磁盘、删除用户数据、泄露密钥或改变用户已指定的重大目标时，停下并说明具体后果。其余情况以用户本次授权和现有事实为依据，减少无必要中断。

## 13. Planned M14 GUI architecture and acceptance (mandatory)

The project owner has selected a Rust implementation for the native GUI,
with first-class **C, C++ and Rust application SDKs**, and five separate GUI
submilestones. See [M14_GUI](docs/M14_GUI.md), [ROADMAP](ROADMAP.md)
and owner requirement [DEV-002](docs/REQUIREMENTS.md). This section is a
future-development contract: no M14 GUI code or verified GUI behavior is
claimed by the M0–M12 bounded baseline.

- Write the window compositor, desktop shell and GUI runtime in Rust. Keep
  the compositor in a **user process**; the kernel supplies safe display/input
  drivers, memory protection, IPC and scheduling, not widget/window policy.
- Use one **versioned C ABI** implemented in Rust as the portable library
  interface: C headers, a C++ RAII/move-safe wrapper and safe Rust bindings.
  Preserve equivalent observable behavior for all three languages.
- At every FFI boundary, use checked `extern "C"` / `#[repr(C)]` layouts,
  explicit widths, opaque handles, version/error checks, clear allocation
  ownership and no unwinding. Specify threading, callbacks, event ordering,
  lifetime, invalid arguments and resource exhaustion before freezing v1.
- The compositor must validate untrusted client IPC, surface dimensions,
  stride/format, shared memory and lifetime, object capabilities, focus and
  event queues. Client crash and compositor failure must not corrupt the
  kernel or other processes.
- Develop in this order with separate acceptance evidence:
  **M14.1** framebuffer/software drawing;
  **M14.2** keyboard/pointer events;
  **M14.3** user-space compositor/multiple windows;
  **M14.4** GUI runtime and independent C/C++/Rust demos;
  **M14.5** optional proven GPU acceleration with CPU fallback.
  Do not mark a substage accepted based on a log, screenshot or stub alone.
- Do not assert GPU acceleration from Limine framebuffer or virtio-gpu
  scanout. Do not claim Vulkan/OpenGL, glibc, Wayland/X11, SDL/Qt,
  USB input, SMP or real hardware support without their own tested profiles.
  Preserve accepted serial/QEMU negative gates as graphics are introduced.
- Plan ABI/package metadata during M13; implement and accept the actual
  GUI SDK only after its dependent GUI runtime and relevant guest toolchains
  are verified. Never equate host cross-compilation with guest compilation.
- Any new GUI file follows DEV-001 purpose headers, English comments,
  inventory, ownership/safety review, ABI compatibility tests and exact
  validation evidence requirements. Add `cargo xtask` GUI commands only
  together with their functioning implementation.
