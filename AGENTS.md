# AGENTS.md — AnovaOS

## 1. 项目目标与当前事实

这是 Rust 编写的 x86_64 自研操作系统。最终用户应能在系统自己的终端中编辑源码、使用 GCC/G++/Rust/Cargo 编译、链接并运行应用，同时逐步运行经过验证的现有 Linux 软件。

当前仓库只有设计文档，没有 Rust workspace、内核、镜像或工作流。**不能声称任何内核功能已经实现。** 实施时先读 [docs/STATUS.md](docs/STATUS.md)，再检查真实文件和 git 状态，以代码与验证记录为准。

本文件是后续 Codex 开发规范。用户明确指令优先；已有修改应保留。日常、可逆的实现选择可自主完成，不因普通阶段切换反复请求确认。

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
- Rust 模块化单体内核，`#![no_std]`，分配器完成后才使用 `alloc`。
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

- M0 验证后写入精确 `rust-toolchain.toml`；不能永久使用浮动 `nightly`/`stable` 作为可复现基线。
- 能用 stable 则用 stable。确需 nightly 功能时固定日期，并写出所需 feature 与原因。
- 提交 workspace `Cargo.lock`；固定引导器、协议 crate、固件、测试软件的版本/提交与校验和。
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
- 内核初期不生成浮点/SIMD；用户 ELF 可能使用 SSE2。M4 运行此类用户代码前就要正确初始化 CPU 状态，M5 切换任务前实现相应状态保存恢复，不能等到多线程后才考虑。
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

## 10. 计划中的统一命令

以下命令目前不存在，由 M0/M1 实现后才能作为实际操作说明：

```sh
cargo xtask doctor
cargo xtask build
cargo xtask image
cargo xtask run
cargo xtask smoke
cargo xtask test-host
```

M0 应提供最小可构建入口；M1 结束前 build/image/run/smoke 必须真实工作。`doctor` 对缺少依赖明确报错，`test-host` 只测实际存在的纯逻辑，不创建镜像式空测试。将来 `cargo xtask test-compat` 等入口随能力落地再加入文档。

## 11. 验证要求

- 文档修改检查本地链接、阶段/状态一致性和命令是否被明确标为计划；无代码时不伪造 Rust 构建结果。
- 有代码后运行适用的格式化、构建、clippy 和相关测试，不盲目对所有目标执行 `cargo test --workspace`。
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
