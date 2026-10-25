<!-- Track accepted milestones and planned resource, ext4, desktop and conditional self-hosting acceptance. -->
# 开发路线与阶段验收

[English](../en/ROADMAP.md) | **简体中文**

计划复核：2026-10-26，对照已同步 `dev` 的 `05a5fb1`；本次文档更新未重跑历史门禁。**M0–M13（受限离线与联网范围）及 M14.1 软件显示已通过本地 q35/TCG 验收；M14.2 输入与游标已通过本地 q35/TCG 验收；M14.3 用户态合成器已通过本地 q35/TCG 验收；M14.4 GUI runtime/三语言 SDK 已通过本地验收；M14.5 已通过有界硬件清屏/读回及 CPU fallback 的本地验收，M15.1/M15.2 已通过受控 bzip2 源码重建的本地验收，M15.3 已完成 GCC 可行性评估，编译器重建受当前依赖和容量限制阻塞；M15.4 已完成 Rust/LLVM/Cargo 可行性评估，源码重建受当前前置条件和容量限制阻塞；M15.5 已通过 guest 内核构建、宿主辅助打包与真实重启的本地验收；M15.6 已通过 bzip2/内核两次独立干净构建、声明产物比较及恢复的本地验收，外设 IRQ 路由启用待对应驱动。** 证据见 [M13 验证记录](validation/M13.md)、[M14.1 验证记录](validation/M14.1.md)、[M14.2 验证记录](validation/M14.2.md)、[M14.3 验证记录](validation/M14.3.md)、[M14.4 验证记录](validation/M14.4.md)、[M14.5 验证记录](validation/M14.5.md) 与 [STATUS](guides/STATUS.md)。[FIRST_TASK](guides/FIRST_TASK.md) 记录 M0/M1 原始实现顺序；M2 边界见 [CPU 基线](milestones/M2_CPU.md)。

## 阶段总览

| 阶段 | 可见结果 | 主要依赖 | 当前状态 |
| --- | --- | --- | --- |
| M0 | 可复现工程与宿主入口 | 无 | 本地验收通过 |
| M1 | UEFI 启动并可靠输出 | M0 | 本地验收通过 |
| M2 | 异常、中断与时钟可诊断 | M1 | BSP 基线验收；外设 IRQ 待驱动 |
| M3 | 受控页管理和堆 | M2 | 本地验收通过；引导保留区未回收 |
| M4 | 第一个隔离的用户 app | M3 | 本地验收通过；顺序运行 Rust/C ELF |
| M5 | 多进程与阻塞/抢占调度 | M4、M2 | 单核进程基线通过；M9 后共享进程/线程槽 |
| M6 | VFS、init、基础 shell | M5 | 本地验收通过；tmpfs 重启丢失 |
| M7 | 源码和应用可持久保存 | M6 | 本地验收通过；单块组 ext2、正常关机保证 |
| M8 | 首批静态 Linux CLI 程序 | M6、M7 | 本地验收通过；受限静态 CLI profile |
| M9 | 线程、信号、动态 musl | M8 | 本地验收通过；受限运行时与 PTY profile |
| M10 | 网络和受控包安装 | M7、M9 | 本地受控 IPv4/TLS/包验收通过 |
| M11 | 系统内 GCC/G++ 应用开发 | M7–M9；可先离线 | 本地离线 C/C++/make 验收通过 |
| M12 | 系统内 Rust/Cargo 应用开发 | M11、M9；联网依赖 M10 | 离线与受限 HTTPS registry/Git 本地验收通过；原生端口独立推进 |
| M13 | 可发布的 SDK 与应用包 | M4 基础 SDK、M11/M12 | 本地 SDK/包/诊断验收通过 |
| M14 | Rust 图形桌面与 C/C++/Rust GUI SDK（M14.1–M14.5） | M9、M13；具体依赖见 M14 文档 | M14.1–M14.4 本地通过；M14.5 有界硬件清屏/读回与 CPU fallback 本地通过 |
| M15 | 可选的工具链/内核自举（M15.1–M15.6） | M11–M13，资源满足 | M15.1/M15.2 受控 bzip2 本地通过；M15.3/M15.4 评估完成、编译器未重建；M15.5 内核重建与重启本地通过；M15.6 已通过 bzip2/内核两次独立干净构建、声明产物比较及恢复的本地验收 |
| M16 | 资源、独立文件系统后端的 VFS 与受控 ext4 | 既有存储/运行时；恢复 CI 并确定实测预算 | 计划；ext4 尚未实现或验收 |
| M17 | 可用开发桌面及全 GUI 应用开发流程 | M16 已验收工作盘/资源；M9/M14 和继承工具 | 计划；不要求完整编译器自举 |
| M18 | 有条件的编译器重建与 guest 独立镜像 | M16/M17 后各组件的实测前置条件 | 计划；各组件独立验收 |

可以交叉推进互不依赖的工作，但不得跳过实际前置条件。网络包获取、图形桌面和完整自举不阻塞第一次离线应用开发。

## M0：工程基线

目标：后续代码有统一的构建、依赖与诊断入口。

- [x] 创建最小 workspace：`kernel` 与宿主 `xtask`，仅加入实际需要的 crate。
- [x] 固定实际验证过的 Rust 工具链，优先使用内置裸机 target。
- [x] 设置 kernel 独立链接配置、panic abort、早期阶段日志和宿主目标隔离。
- [x] 定义 Limine/协议/OVMF/QEMU 的锁定清单、来源、校验和与下载缓存。
- [x] 实现 `cargo xtask doctor`，列出缺失依赖并返回非零；不自动改宿主系统配置。
- [x] 定义 image/run/smoke 输入输出约定；和 M1 一起逐个落地。
- [x] 以真实命令配置格式、Clippy、构建、宿主测试和 QEMU CI；远端执行结果与本地验收分开记录。

退出条件：干净环境按文档可构建最小内核 ELF；宿主工具无需裸机 `std`；缺依赖时给出可操作错误；版本信息已记录。

## M1：UEFI、Limine 与日志

目标：一个可以可靠定位失败的引导闭环。

- [x] 正确放置 Limine 请求与 linker sections，校验支持的 base revision。
- [x] COM1 串口输出，panic 输出阶段和错误，提供无锁/尽力 fallback。
- [x] 校验 memory map、HHDM、RSDP、framebuffer 与模块响应；非必需信息缺失可降级。
- [x] 以只读借用保留启动数据与 map，报告保留区间；M1 不分配或回收任何内存，M3 再建立可回收的所有权模型。
- [x] 生成 UEFI 启动镜像，运行 q35 + OVMF，TCG 基线通过。
- [x] 实现 build/image/run/smoke；测试固件变量使用临时副本。
- [x] 冒烟验证含超时、阶段标记与测试专用退出设备/结果解码。

退出条件：从命令行生成镜像并启动，得到版本/架构/启动完成标记；注入 panic 与超时能被 runner 判为失败。截图只作辅助，串口与退出结果是主要证据。

## M2：CPU、异常、中断、时钟

- [x] GDT/TSS/IDT、独立异常栈、double fault 的 IST 与 guard page。
- [x] page fault/GP fault/非法指令报告寄存器、地址和错误位。
- [x] 解析并校验 RSDP/XSDT（或 RSDT）/MADT/FADT；配置 LAPIC 定时器与保持屏蔽的 IOAPIC 路由。
- [ ] 外设 IRQ、MADT source override/NMI 路由随首个对应驱动启用并验证；当前不宣称外设中断可用。
- [x] 使用 ACPI PM 时钟校准 LAPIC，提供粗粒度单调 IRQ 时间、周期中断和正确 EOI。
- [x] 区分 trap/IRQ/普通 BSP 上下文，记录 IF 和无锁/不分配规则；线程上下文尚未存在。

退出条件：定时中断连续计数；人为异常有可理解诊断；异常/中断不会因为日志锁递归失去全部输出。尚无用户态时，故障注入使用独立测试镜像。

## M3：内存与分配器

- [x] 基于保留区间的物理页分配与回收，拒绝重复释放/非法页。
- [x] 接管并管理内核页表、地址空间和映射权限，启用可用的 NX/WP。
- [x] 区分普通 RAM、MMIO、DMA；映射缓存属性可追踪。
- [x] 建立堆后启用 `alloc`；OOM 明确失败且保留诊断。
- [x] 对映射、分配与地址计算做溢出和边界检查。

实现边界见 [M3_MEMORY](milestones/M3_MEMORY.md)：仅分配 USABLE RAM，完整复制表层级，64 KiB 双 guard 普通栈、2 MiB 固定 buddy 堆；DMA32 仅提供地址约束，尚无 DMA 驱动。

退出条件：分配/释放压力与边界用例通过；保留区域不被分配；guard page/只读页/NX 的相关故障有证据；OOM 无无声内存破坏。

## M4：用户态、ELF 与最小 SDK

目标：宿主编译的自有 app 作为真正用户进程运行。

- [x] Ring 3、独立用户地址空间、内核栈、可验证 syscall/return 路径。
- [x] 受限 ELF64 ET_EXEC 装载、段校验、BSS、用户栈与命令行。
- [x] 最小 native ABI：控制台 write、exit、基础查询，带版本/布局定义。
- [x] 用户指针复制及访问故障恢复；用户非法访问终止用户进程。
- [x] 最小 `no_std` Rust/C 用户 runtime、链接脚本和 hello-app 示例。
- [x] 初始化用户程序需要的 FPU/SSE 状态；内核仍避免使用 SIMD。
- [x] 显式记录镜像的 ABI personality；不靠 ELF OSABI 猜测。

退出条件：一个独立 ELF 在 Ring 3 输出参数并正常退出；坏 ELF、坏地址、非法 syscall 和用户 page fault 不破坏内核。app 的 syscall 是真实边界，不是直接链接内核函数。

实现边界与证据见 [M4_USERSPACE](milestones/M4_USERSPACE.md) 和 [M4 验证](validation/M4.md)：显式 native v1 模块、SYSCALL/IRETQ、11 个用户用例与 8 次重复执行，M5 已加入并发调度，后续 M8/M9 已实现受限 Linux ABI。后续开发必须遵守 [DEV-001 文件用途说明](guides/REQUIREMENTS.md)。

## M5：进程与调度

- [x] task/process 模型、PID、保存上下文和每进程唯一线程的独立内核栈。
- [x] GPR、段选择子/FS/GS base、x87/MMX、XMM0–15、MXCSR 的保存/恢复；AVX 禁用。
- [x] 单核用户态抢占、阻塞 wait、sleep/wakeup、公平轮转与 STI/HLT 闲置。
- [x] spawn/exec/exit/wait、父子关系、故障退出状态、孤儿收养和资源回收。
- [x] eager-copy fork 和独立用户页；COW 留待后续单独验证。
- [x] 地址空间与控制台 fd 表所有权、fork 继承、exec 保留、close/退出清理。VFS 打开文件和共享偏移属于 M6。

退出条件已验证：两个计算进程交错执行并保持 CPU/FPU 状态；wait/sleep 阻塞且无就绪进程时 HLT；一个子进程崩溃后其他进程继续；16 次重复创建/等待、32 槽耗尽及复用、部分 OOM 回滚后，物理页与堆占用恢复原值。

实现范围见 [M5_PROCESSES](milestones/M5_PROCESSES.md)，证据见 [M5 验证](validation/M5.md)。该历史基线最多 32 个单线程进程（含 zombie）；M9 后进程/线程共用 32 槽；内核不可抢占、无 SMP，执行入口来自显式原生模块目录。

## M6：VFS、init 与基础 shell

- [x] VFS 与 mount、路径解析、cwd、文件/目录/符号链接的演进设计。
- [x] initramfs、tmpfs、devfs、stdin/stdout/stderr。
- [x] 文件描述符、打开文件描述、seek/dup 共享偏移语义。
- [x] 原生 init、shell、ls/cat/echo 等少量自有命令。
- [x] 原生进程执行、管道与重定向的基础接口。
- [x] 串口输入行编辑和终端抽象，后续可接 termios/PTY。

已验证退出条件：通过 shell 列目录、创建/读取 tmpfs 文件、执行用户 app、查看退出码；管道/重定向正常关闭，不在 EOF 处死锁。此阶段 tmpfs 数据重启丢失，应明确说明。

实现范围见 [M6 文件系统设计](milestones/M6_FILESYSTEM.md)，证据见 [M6 验证](validation/M6.md)。挂载仅在引导阶段；32 fd/进程、128 打开对象、16 个 1 KiB 管道、64 KiB/tmpfs 文件。原生 shell 支持四级管道、重定向与串口规范行输入，尚无 termios/PTY、信号、作业控制或 Linux ABI。

## M7：持久磁盘与可编辑环境

- [x] PCI 枚举、现代 virtio-blk、DMA 缓冲区和队列生命周期。
- [x] 块边界、设备容量、错误与 flush 路径。
- [x] 明确特性集合的 ext2 读写；未知/不支持特性拒绝挂载。
- [x] 数据/元数据更新顺序、sync/fsync、受控关机和根盘镜像工具。
- [x] 最小文本编辑器，先支持打开/编辑/保存；之后扩展终端体验。
- [x] 默认运行 overlay/临时盘与持久化测试盘分开。

已验证退出条件：系统内用编辑器保存源码、复制原生二进制、正常 ACPI 关机，重启后源码逐字节与 ELF SHA-256 一致，磁盘原生应用再次执行，e2fsck 无错误；越界 I/O、坏特性、只读盘与真实 write/flush 错误明确失败。实现范围与非日志/断电限制见 [M7_STORAGE](milestones/M7_STORAGE.md)，证据见 [M7 验证](validation/M7.md)。数据盘挂载于 `/persist`，根仍为 initramfs，默认运行盘为临时盘；32 个单线程进程、2 MiB 固定堆和外设 IRQ 屏蔽仍保持。

## M8：POSIX 基础与静态 Linux CLI

目标：运行首批固定构建的 Linux x86_64 程序。

- [x] Linux syscall 分派、errno/布局/flags、用户栈与 auxv。
- [x] 以固定程序 trace 为依据实现内存、文件、目录、时间、随机数和执行接口。
- [x] Linux `arch_prctl`/TLS 等所需 CPU 用户态设施。
- [x] 必要信号基础、基本终端语义、管道、重定向、wait/fork/exec 行为。
- [x] tiny Linux syscall ELF、静态 musl hello/文件程序、BusyBox 精选 applet。
- [x] 对程序版本/构建选项/SHA-256 建立兼容测试清单。

退出条件：固定的静态 hello、文件操作工具、BusyBox `echo/ls/cat` 通过，`sh` 的受测管道与重定向用例通过；未知 syscall 返回真实错误。未验证的 applet 保持“未验证”。

实现范围和限制见 [M8 Linux ABI](milestones/M8_LINUX.md)，证据见 [M8 验证](validation/M8.md)。M8 的记录是历史快照；M9 已增加受测信号、termios/PTY 和动态 musl，通用 BusyBox applet 与完整交互作业控制仍未验证。

## M9：线程、信号、终端与动态 musl

- [x] clone/线程、TLS、futex、退出清理、阻塞唤醒与超时语义。
- [x] sigaction、sigprocmask、信号递送/返回、SA_RESTART 相关范围。
- [x] poll/ppoll 或等效所需接口、非阻塞 I/O、PTY、termios 与作业控制基础。
- [x] ELF PT_INTERP、受测动态解释器、共享映射、mprotect、动态 TLS。
- [x] 静态 PIE 的所需装载/重定位策略，按构建实测处理。
- [x] 按实测需求增加 `/dev/ptmx`、四组 `/dev/pts`；受测程序不需要 `/proc`，不提供假数据掩盖缺失。

退出条件：动态 musl 小程序与 pthread/futex 测试通过；信号中断阻塞调用和终端 Ctrl+C 等已声明行为通过；共享库缺失时有清楚诊断。不会因此宣称 glibc 可用。

实现范围见 [M9_RUNTIME](milestones/M9_RUNTIME.md)，证据见 [validation/M9](validation/M9.md)。
进程/线程共用 32 槽，匿名映射跨 fork 共享，实际 requeue/期限和可失败引用计数元数据已测。
静态 PIE、PT_INTERP、启动库、线程后 dlopen、动态 TLS、信号中断/重启、PTY Ctrl+C 和完整回收通过。
历史 M9 不包含备用信号栈和持久文件 mmap，M12 后已分别验收备用栈和有界只读文件映射；
robust/PI futex、实时队列、完整终端作业控制、可写共享文件映射与 glibc 仍不在已验范围。

## M10：网络与包环境

- [x] virtio-net，优先复用适合 `no_std` 的协议栈并验证内核集成。
- [x] IP、UDP/TCP、DNS、socket 阻塞/非阻塞行为和时间源。
- [x] 先本地仓库和离线包，再联网安装；锁定依赖、校验包内容。
- [x] TLS 放在合适的用户态库，准备 CA 与正确时间；禁止跳过证书验证。
- [x] 软件包清单、安装路径、许可证、升级/失败回滚和用户数据保留。

退出条件：受控网络对端的收发/超时/DNS 测试通过；离线包可安装、升级与卸载；在线能力按实测范围另报。联网能力不是离线 GCC 验收的门槛。

实现范围见 [M10_NETWORK_PACKAGES](milestones/M10_NETWORK_PACKAGES.md)，证据见
[validation/M10](validation/M10.md)。可选轮询 IPv4 TCP/UDP 客户端、有界用户 DNS A。
包使用 native SDK v1、完整归档/逐文件锁、交替持久槽并保留数据；TLS 验证受控 CA/域名/日期正负例。
不声明公共原生包源、通用 Linux 网络、任意包依赖求解或 ext2 断电事务。

## M11：系统内 C/C++ 应用开发

实施细节见 [TOOLCHAINS](guides/TOOLCHAINS.md)。

- [x] 在宿主构建可在 guest 运行的 gcc/g++、cc1/cc1plus、assembler/linker、构建工具。
- [x] 安装 CRT、头文件、libc、libgcc、libstdc++、必要依赖和明确 sysroot。
- [x] 补齐编译器实际使用的文件、映射、临时文件、子进程、信号和线程语义。
- [x] 先静态/受控构建包运行，再评估动态工具链；TCC 可作中间验证但不替代 GCC。
- [x] 使用编辑器在 guest 创建 C/C++ 工程并编译运行。

退出条件：C 的 stdio/文件 I/O 和多文件编译通过；C++ `std::string/vector`、RAII 通过，异常/线程按声明配置分别测试；源码与产物重启保留。只在宿主编译、在 guest 执行不算完成。

已实现离线范围见 [M11_TOOLCHAIN](milestones/M11_TOOLCHAIN.md)：源码构建 GCC/G++/
binutils/make、musl/CRT/libgcc/libstdc++。C stdio/文件/argv/malloc/多源、C++ 容器/RAII/异常/
线程、make 增量、真实失败诊断及重启哈希通过，见 [validation/M11](validation/M11.md)。
通用项目、动态应用工具链、LTO/OpenMP/sanitizer 和编译器自举不在该范围。

## M12：系统内 Rust 与 Cargo

- [x] 先验证 Linux-musl 兼容 target 的交叉编译 `std` 应用，不改变内核 target。
- [x] 在宿主准备校验锁定的官方 musl rustc/LLVM/Cargo/std/rust-lld 二进制包；M11 C sysroot 单独源码构建。Rust/LLVM 源码重建不在本次成果内。
- [x] 在 guest 中编译、链接、运行含 Vec/String、文件 I/O、参数的小型 Rust 应用。
- [x] `cargo build --offline --frozen`：准备固定 vendor、Cargo.lock、校验和与配置。
- [x] 分别验证线程、线程内子进程等待、build.rs、proc-macro `.so` 装载和 C FFI。
- [x] 扩展可选联网 registry/Git、严格 TLS/CA 与依赖下载；受控失败用例及经宿主代理的 crates.io/GitHub 实测通过，断网重启从保留缓存全量重编译。
- [ ] 若要 AnovaOS 原生 target：独立实现 libc/`std` 的平台端口，不能仅改 target JSON。

退出条件：guest 内编辑 Rust 源码，运行 rustc 和 Cargo 完成构建，执行产物且重启保留；运行期间无宿主远程编译。离线基本闭环与完整 Cargo 能力分项标记。原生 target 移植是另一个成果。实际范围见 [M12 设计](milestones/M12_RUST.md)、[联网范围](milestones/M12_NETWORK.md) 与 [验收](validation/M12.md)。

## M13：应用 SDK、包与开发体验

- [x] 将 M4 起的 runtime/ABI 固化为带版本的 SDK，并提供稳定错误模型。
- [x] 定义应用清单：ABI、目标架构、依赖、入口、权限和最低系统能力。
- [x] 文档化交叉开发、系统内开发、日志、崩溃符号化与资源调试。
- [x] ELF 符号与栈回溯先行，GDB stub/用户调试器按能力推进。
- [x] 发布 C/C++/Rust app 模板、离线依赖示例、安装升级与用户数据规则。
- [x] 为未来 GUI SDK 预留独立的版本化 C ABI / 三语言封装交付约定；GUI 本体及实测推迟到 M14.4。
- [x] 对 ABI 变更增加版本策略和兼容回归。

退出条件：新开发者按文档创建应用、构建、安装、运行、查看错误并升级；native 和 Linux 兼容包不会因目标/ABI 混淆而被误装。

实际交付为 SDK 1.0.0、ANOVAPK2、三语言原生模板、静态 ET_EXEC Linux 包和受限用户帧指针回溯；GDB stub/交互调试器仍后置。见 [M13 SDK](milestones/M13_SDK.md) 与 [验收](validation/M13.md)。权限属于准入元数据，GUI 本体已在 M14.4 独立验收，证据与 M13 分开。

## M14：Rust 图形桌面与三语言 GUI SDK

**M14.1–M14.4 已通过本地验证；M14.5 有界硬件清屏/读回与 CPU fallback 已通过本地验证。** 完整子阶段任务、体系边界、错误/所有权模型、
验收负例与候选目录见 [M14 GUI 专项设计](milestones/M14_GUI.md)。
采用 Rust 实现用户态 compositor、GUI runtime 和稳定 C ABI；
提供 C 头文件、C++ RAII SDK、Rust 安全 SDK，三者共享一个 API/协议语义。
内核仅提供显示/输入驱动、映射、IPC 与隔离，不在内核中实现窗口管理器。
先软件渲染，硬件 GPU 加速为最后的可选阶段，不能把 framebuffer 当成 GPU 加速。

### M14.1：显示与软件 framebuffer

- [x] 校验 Limine framebuffer 的格式、尺寸、pitch、映射所有权和边界，支持受限格式。
- [x] 软件像素、矩形、位图、裁剪、离屏缓冲和画面提交；保留串口故障诊断。
- [x] QEMU 可见确定性测试图形；像素校验、越界/不支持格式、超时和故障负例通过。

**验收：** 正确显示且边界安全，可稳定复现；不声称已有桌面或 GPU 加速。

本地 QEMU 8.2.2 standard VGA/800×600×32：48 万像素与固定 checksum 全部一致，
显存读回、临时堆回收、无显示设备 fallback、串口恢复和正常关机通过。
24/32 位 RGB/BGR 与裁剪边界另由宿主测试覆盖；无 vblank/page-flip 保证。
命令与证据见 [M14.1 验证](validation/M14.1.md)。

### M14.2：输入事件与鼠标

- [x] QEMU i8042 原生 set-1 键盘与 ID-0 三字节鼠标，标准化物理按键、重复、按钮与九位相对移动。
- [x] 64 项事件队列、7×9 裁剪游标及单一诊断消费者；溢出、坏输入和消费者断开/重连清除状态。
- [x] 调度器每轮最多读取 64 字节，外设 IRQ 继续屏蔽；初始化使用 PM 时钟及次数上限，半包超时分设备检查。

**验收：** QEMU 输入注入能得到顺序一致的按键/移动/按钮事件和光标反馈；
不能因此宣称实机 USB 可用。

已实现 `cargo xtask input-test`：标准 VGA、无显示设备和关闭 i8042 三次启动；
QMP 注入按键/重复/扩展键/按钮，中心与四角画面逐像素比较，测试镜像检查队列溢出、
坏输入、消费者断开/重连与真实控制器端口停用/重新初始化，均要求串口恢复和正常关机。
本地三次启动验收通过，具体结果与回归范围由 [M14.2 验证](validation/M14.2.md) 单独记录。
物理按键不转换为文本；焦点仅指固定内核诊断目标，用户态窗口和输入路由属于 M14.3。
不声称 USB、物理热插拔、外设 IRQ、真实硬件或 GUI SDK 支持。

### M14.3：用户态窗口合成器

- [x] 构建独立 Rust compositor 进程和有版本/所有权约束的 IPC、surface/缓冲接口。
- [x] 软件窗口合成、重绘区域、叠放顺序、移动、缩放、焦点与输入分发。
- [x] 两个隔离用户应用并存；错误 buffer、资源耗尽、客户端崩溃和 compositor 失败可诊断/回收。

**验收：** 至少两个独立进程的窗口可正确遮挡、移动和分发事件，故障不破坏
内核或其他进程。

本地 `cargo xtask gui-test` 已验证三个启动配置、两个隔离用户窗口、真实 QMP 焦点/移动/缩放、
八张逐像素截图、客户端与合成器各自真实 #UD、重连及串口恢复。内核仅提供受检复制传输，
每客户端一个 32×24 至 320×240 RGB surface，最多四个连接；实际堆耗尽回滚另有内核检查。
窗口内容使用有界分块复制，无共享映射或原子多块提交。见 [M14.3 设计](milestones/M14_COMPOSITOR.md)
与 [验证](validation/M14.3.md)。

### M14.4：Rust GUI runtime 与 C/C++/Rust SDK

- [x] Rust 用户态控件、布局、字体/文本、绘图、事件循环、图形终端和首个原生 GUI app。
- [x] 冻结版本化 C ABI：`extern "C"`、`#[repr(C)]`、固定宽度数据、
      不透明句柄、错误码、生命周期、线程/回调语义；不跨 ABI unwind。
- [x] 提供 C 头文件/链接库、C++ RAII 封装和 Rust 安全 API，以及各语言构建模板。
- [x] 受测 guest 中分别编辑、编译、链接、运行 C、C++、Rust GUI 示例；
      SDK 版本不匹配、句柄失效、非法参数与故障资源回收均验证。

**验收：** 三种语言都能独立打开窗口、处理事件、绘制并退出；
宿主交叉编译成功不等于系统内开发验收。依赖 M11/M12/M13 的实际能力。

已通过 `cargo xtask gui-sdk-test`：独立 C/C++/Rust guest 编辑、构建和真实键鼠事件；
Rust note 与交互式 echo/pwd 图形终端；ABI/UTF-8/句柄/限额负例，真实用户页故障，
失败构建保留与两次启动文件哈希一致。28 张实际截图校验独立像素样本，
不将抽样说成全屏逐像素比较。边界见 [M14 runtime](milestones/M14_RUNTIME.md) 与
[验收](validation/M14.4.md)；字体采用原创 ASCII 位图及 Unicode 替代框，
剪贴板为进程私有，终端无 PTY/ANSI，未实现动态 GUI 服务包准入。

### M14.5：可选 GPU 硬件加速

- [x] 选定现代 virtio-gpu/VirGL，有界 GPU render-target clear/读回与 2D scanout 分别验收。
- [x] 实现现代 PCI/controlq 驱动、合成器专用 clear/readback 接口、私有 DMA backing、fence 与真实 timeout/reset；不暴露共享用户映射或原始命令。
- [x] 实际设备报告 RTX 4060 Ti/D3D12；四种 64×64 和一组 7×5 颜色读回、超时 CPU fallback、重置后 GPU 重建与无设备/仅 2D 设备路径通过。
- [x] 记录 QEMU/Mesa/VirGL 版本、实际 renderer、提交与读回延迟和逐像素结果；不声称桌面加速、shader 或 guest OpenGL/Vulkan。

**验收：** 至少一条经验证的真实 GPU 渲染路径与正常软件回退；
如仅实现显示扫描输出，M14.5 仍未完成。

本地有界 profile 已通过：GPU 仅做离屏清屏/读回，画面仍由 CPU 复制到保留的 VGA framebuffer，
普通桌面仍使用软件合成；未建立性能加速、shader draw、物理 GPU 驱动或 reset 拒绝后的恢复。
接口、DMA 生命周期与适用边界见 [M14 GPU](milestones/M14_GPU.md)，实测证据见
[M14.5 验证](validation/M14.5.md)。

其他计划：SMP（per-CPU、TLB shootdown、中断亲和性）、实机 ACPI/电源/
NVMe/AHCI/USB、glibc 指定版本、Wayland/X11/SDL/Qt 等 Linux GUI 兼容，
均为**独立后续工作流**，不计入以上五个 GUI 子阶段的完成率。
每一项要单独记录适用硬件、依赖和验证证据。

## M15：可选自举与系统自编译

**M15.1/M15.2 已通过受控 bzip2 1.0.8 源码重建的本地验收；M15.3 已完成 GCC 可行性评估，编译器重建受当前依赖和容量限制阻塞；M15.4 已完成 Rust/LLVM/Cargo 可行性评估，源码重建受当前前置条件和容量限制阻塞；M15.5 已通过 guest 内核构建、宿主辅助打包与真实重启的本地验收；M15.6 已通过 bzip2/内核重复干净构建、声明产物比较及恢复的本地验收。**
设计与实测见 [M15 bootstrap](milestones/M15_BOOTSTRAP.md) 和
[验证记录](validation/M15.1-M15.2.md)。M15 是独立的可选自举工作流，
不影响 M11–M13 已验收的系统内应用开发能力。优先沿用受测的 Linux-musl 兼容工具链；
原生 libc/std、SMP 和完整 Linux 兼容仍按独立工作流推进。

执行顺序：先完成 **M15.1 → M15.2**，再按实际前置能力分别推进 M15.3、M15.4 和 M15.5，
最后由 M15.6 汇总已声明范围的重复构建与恢复证据。**M15.5 可使用已验证的 guest rustc，
不要求先完成 GCC 或 Rust/LLVM 源码自举。** 每一步同步保留锁定输入、命令、日志和产物哈希。

GCC 多阶段与完整 Rust/LLVM bootstrap 按可行性结果决定是否实施，不能因为拆分子阶段就
默认其完整自举必定可完成。评估结论与真实构建结果分别记录；未实施的能力保持未验证。

### M15.1：构建环境与容量

- [x] 为选定构建目标测量内存、磁盘、inode、文件大小、文件描述符、进程/线程和耗时需求。
- [x] 根据实际瓶颈扩展存储、VFS、内存与运行时边界，保留旧镜像兼容性、越限诊断和失败回滚。
- [x] 准备独立的持久构建工作盘、锁定的离线输入与构建配方，明确 build/host/target、ABI 和 sysroot。

选定 bzip2 构建已在现有容量内实测通过，没有扩大资源边界；可选 PostScript 文档超过单文件上限，保留 HTML 文档并排除该非构建输入。
当前开发盘为 16 MiB、512 inode，持久文件上限为 4 MiB，tmpfs 单文件上限仍为 1 MiB，VFS 为 2,048 节点；
另有 16 MiB 内核堆和 32 个进程/线程槽等限制。数值与适用边界见 [STATUS](guides/STATUS.md)
及 [M12 Rust](milestones/M12_RUST.md)。增加 QEMU RAM 不会自动解除这些边界，扩容目标须由实测决定。

**验收：** 选定目标所需的构建环境和工作盘可重复准备，实际资源需求有记录，
越限返回可诊断错误；相关存储/运行时测试及已有门禁继续通过。

### M15.2：小型 C/C++ 工具源码重建

- [x] 选择一个体量小、依赖受控的实际 C/C++ 工具，锁定版本、源码校验和、许可证和必要补丁。
- [x] 使用 guest 内的编译器完成源码准备、编译、链接和工具功能测试，记录实际子工具和依赖。
- [x] 持久保存源码、配方和产物，验证真实构建失败的非零退出及旧可用产物保留。

**验收：** 完成 guest 内源码 → 构建 → 运行 → 保存 → 正常关机 → 重启运行闭环，
重启前后源码和产物哈希一致；宿主预先编译的二进制不能替代该阶段的 guest 构建产物。

本地 `cargo xtask bootstrap-test` 通过三次启动：资源越限/回收、八个编译单元的 guest GCC 构建、
重启运行及真实编译失败后旧工具保留。压缩字节与官方样本一致，解压由 guest 与宿主独立验证；
各次关机后 e2fsck 干净。容量数据为调度边界采样与关机后磁盘用量，不声明精确 RSS 或瞬时磁盘峰值。

### M15.3：GCC 自举路线

- [x] 独立评估固定 GCC 版本、GMP/MPFR/MPC、构建工具与 sysroot 的依赖和资源，记录可行性结论。
- [ ] 条件满足时先在 guest 内重建一个明确声明的 GCC 编译器阶段，并用新产物编译、运行测试应用。
- [ ] 再决定是否推进 stage2/stage3；如实施多阶段 bootstrap，记录阶段关系、产物比较和差异原因。

**验收：** 首先交付可复核的可行性报告、实际探针日志和继续实施所需条件。
若实施编译器重建，须证明新编译器的实际编译功能；若实施多阶段自举，须额外提供阶段比较证据。
仅完成评估时，单阶段重建与完整 GCC 自举仍标为未验证。

当前评估见 [M15 GCC](milestones/M15_GCC.md) 与 [实测记录](validation/M15.3.md)：
C++11 种子编译运行、缺失数学库头文件、九个构建工具 exec 失败、原版 configure
启动失败、越限恢复和重启哈希验证已通过。完整归档容量审计与最小构建需求分开记录；
先补齐 POSIX 构建工具、数学库开发依赖及大文件/磁盘/inode/VFS 容量，再推进首个 GCC 阶段。

### M15.4：Rust/LLVM 自举路线

- [x] 分别评估固定 LLVM、rustc、Cargo 版本的源码依赖、stage0 工具、构建工具和资源需求。
- [ ] 按可行性逐级推进受控源码构建，明确完成到 LLVM、rustc 或 Cargo 的哪一层，以及 host/target 范围。
- [ ] 对实际重建的工具运行功能测试，记录继承的二进制种子、未重建组件、阻塞项与后续条件。

**验收：** 首先交付各组件独立的可行性结论和实际探针证据；已构建的工具须实际运行并通过声明的测试。
现有 M12 官方二进制工具能运行和编译应用，不构成 Rust/LLVM 源码构建或编译器自举证据；
未执行的层次保持未验证，完整 bootstrap 按评估结果继续规划。

当前评估见 [M15 Rust](milestones/M15_RUST.md) 和 [实测记录](validation/M15.4.md)：
锁定完整 Rust 1.90.0 发行源码并分别审计 LLVM 20.1.8、rustc、Cargo；
两次实际 QEMU 启动验证 Rust std/C++17 种子编译运行、六个缺失工具 exec 错误、
Python bootstrap 入口无法启动、所选离线 curl 依赖缺失、真实 Rust 编译失败后旧 ELF 保留及重启哈希一致。
完整发行源码容量不是最小构建需求；1.89.0 stage0 仅静态审计、未运行，
没有重建 LLVM/rustc/Cargo。M15.5 已独立沿用已验收的 guest rustc 完成受控内核重建与启动。

### M15.5：内核自编译与重启

- [x] 准备 guest 可用的内核 target、core/alloc、链接器和锁定离线依赖，隔离内核与普通用户程序的构建配置。
- [x] 在 guest 内编译、链接自身内核，保留确切源码版本、配置、日志、ELF 哈希和可核对的启动身份。
- [x] 验证 guest 内核 ELF 的实际启动，分别记录宿主辅助镜像打包与尚未实现的 guest 独立镜像生成。
- [x] 打包时显式使用保留的 guest ELF，避免镜像入口重新在宿主构建内核而替换待验收产物。

**验收：** 下一次 QEMU 启动确实运行该 guest 构建的内核，并通过相应启动及回归检查。
guest 内核编译、宿主辅助镜像打包和 guest 独立镜像生成分别报告；仅前两项通过不能宣称完整镜像自生成。

本地 `cargo xtask bootstrap-kernel-test` 已通过三次启动：guest Cargo/rustc/rust-lld
重建真实内核，保留 ELF 并逐字节核对 ISO 内容，启动新内核验证构建身份，再验证
真实 Rust 编译拒绝后旧内核保留及恢复重启。见 [M15 kernel](milestones/M15_KERNEL.md)
和 [本地验收](validation/M15.5.md)。内核为 `bootstrap-metrics`、无 GUI 的
opt-z 配置；core/alloc/compiler_builtins 是锁定的继承种子，镜像仍由宿主打包。
持久 ext2 上限扩至 4 MiB，tmpfs/共享缓存保持 1 MiB；每次写入的私有间接块表快照
降低大文件小块写入的重复读取，磁盘格式和有序写入规则保持不变。
guest 独立生成完整镜像仍未实现，不作为已验证能力。

### M15.6：重复构建、产物比较与恢复

实现命令 `cargo xtask bootstrap-repeat-test`，见 [重复构建](milestones/M15_REPEAT.md) 与
[验证记录](validation/M15.6.md)。双轮构建、声明产物逐字节比较及 12 次启动/恢复已通过本地验收；内核中间文件差异保留，不宣称整个构建目录一致。

- [x] 对声明完成的范围执行至少两次独立干净构建，分别保留输入清单、种子工具、环境、完整日志和产物哈希。
- [x] 比较产物与功能结果，记录时间戳、路径、构建身份等已知非确定性，不隐藏差异或省略宿主参与。
- [x] 验证构建失败不破坏旧可用工具和可启动内核，确认正常关机、重启及恢复流程。

**验收：** 已实施范围有可重复执行的构建配方、产物比较、重启和失败恢复证据；
未实施的 GCC/Rust 完整自举及镜像生成层次有明确边界。两次结果一致只证明所记录条件下的比较结果，
不能据此宣称普遍可复现或从零自举。

退出条件：按声明的范围给出完整构建日志、产物和重启证据，并明确已完成、仅评估和未验证的能力。
**M15 不属于“能在系统内开发 app”的必要条件。**

## M16：资源、VFS 与受控 ext4

项目所有者于 2026-10-26 确定此计划；以下各子阶段均为**计划，尚未实现**。
详细契约拆分至 [M16 基础能力](milestones/M16_FOUNDATION.md)。现有 ext2 限制属于我们的
实现子集，不是格式的最大容量。采用 ext4 作为中期工作盘格式，保留 ext2 旧镜像和恢复用途。
全新 AnovaFS 格式属于独立研究项目，不进入此开发顺序。

| 阶段 | 计划交付 | 验收边界 |
| --- | --- | --- |
| M16.1 | 恢复 CI，测量明确构建/工作空间配置 | 受检依赖输入、实际后续门禁及内存/存储/inode/task/fd/延迟预算 |
| M16.2 | 解耦文件系统后端并扩展 VFS/资源管理 | 按需命名空间/缓存策略、有界分配回收及旧 ext2 回归 |
| M16.3 | 受控 ext4 只读后端 | 多块组/extent 及所支持元数据检查；拒绝坏格式、未知特性和无法重放的脏盘 |
| M16.4 | 受控 ext4 写入和文件语义 | 创建/读写/调整大小/重命名/删除/打开生命周期及耗尽/错误恢复；不切默认 |
| M16.5 | JBD2 重放及崩溃一致性 | 明确事务/flush 契约、逐写入点崩溃注入及独立文件系统检查 |
| M16.6 | 新工作盘迁移和实际开发负载 | 复制/哈希/重启验证、保留 ext2 回退、构建期间前台响应实测 |

先完成 M16.1 再确定容量；M16.6 将新工作盘配置提升为默认前，必须完成 M16.2–M16.5。
未知不兼容特性明确拒绝，不静默降级；不原地转换用户磁盘。额外 QEMU 内存不会自动解除
VFS、文件、缓存或固定堆限制。日志验收不代表应用未同步的数据一定能在崩溃后保留。
本地验收与远端 CI 分别记录。

## M17：开发桌面

全部子阶段均为**计划，尚未实现**，详见 [M17 桌面](milestones/M17_DESKTOP.md)。
在 M16 工作盘/资源契约验收后，使用已经可运行的 guest GCC/G++、Rust/Cargo 种子工具推进。
编译器源码重建不是前置条件。

| 阶段 | 计划交付 | 验收边界 |
| --- | --- | --- |
| M17.1 | 受监督的用户态桌面会话 | 启动/关闭、窗口焦点/配额、可重启的合成器和会话生命周期 |
| M17.2 | 真正的图形 PTY 终端 | 明确 Linux-musl shell、构建工具、输入输出、Ctrl+C、resize、滚动与 ANSI 子集 |
| M17.3 | 文件浏览和可靠源码编辑 | 真实持久文件、保存/错误处理、明确 fsync/rename 持久性和源码保留 |
| M17.4 | 日志、资源检查、任务/服务控制 | 真实状态/容量、受检任务终止与服务重启，不伪造资源统计 |
| M17.5 | 完全通过图形界面的开发验收 | C/C++/Rust 编辑/保存/构建/运行/修错/重启、构建期间响应和 GUI 故障恢复 |

现有 echo/pwd 图形示例不是通用 shell。原生 GUI 连接工具进程时，保留明确的 native/Linux
执行准入。应用/服务故障应能从桌面诊断；内核 panic、引导或显示/输入故障仍需 COM1/QEMU/
宿主恢复。单靠截图不能证明开发流程已完成。

## M18：有条件的自举

此部分为**后续计划**，不代表 M15 受阻的编译器目标已经完成。M16/M17 之后，沿用原有证据
契约，按组件分别推进：

- [ ] M18.1：重新评估明确的 GCC 源码闭包、POSIX 工具和 GMP/MPFR/MPC 输入；可行时重建
  一个阶段并编译实际应用，再决定是否有必要进行 stage2/stage3。
- [ ] M18.2：独立界定 LLVM、rustc、Cargo，包括可用构建工具、所选发行要求的 stage0 和
  离线依赖闭包；逐项标明继承组件。
- [ ] M18.3：在 AnovaOS 内从保留的 guest 构建内核生成可启动镜像，比较打包字节和启动身份，
  说明所有剩余宿主参与。
- [ ] M18.4：在干净工作空间重复已验收组件构建，验证失败/重启恢复，保留旧工具、旧内核及
  所有观察到的产物差异。

M18.3 可使用现有 guest 编译器，不必等待 M18.1/M18.2。仍不可行的编译器路线保留评估与
阻塞项，不将构建标记为通过。参见 [M15 GCC](milestones/M15_GCC.md)、
[M15 Rust](milestones/M15_RUST.md)、[M15 内核](milestones/M15_KERNEL.md) 和
[M15 重复构建](milestones/M15_REPEAT.md)。

## 当前后续顺序

从 M16.1 开始，先验收资源/存储，再做 M17 开发桌面。M18 深度自举按各组件前置条件推进。
原生 libc/std、带 GUI 内核重建、SMP、设备 IRQ/异步 I/O、USB/NVMe/AHCI、更广 GPU API
及实机仍独立界定范围，不整体作为 M17 前置条件。新命令随实际实现和证据加入。
按 [DEV-003/DEV-004](guides/REQUIREMENTS.md) 同批维护双语文档、README、状态和验收边界。
