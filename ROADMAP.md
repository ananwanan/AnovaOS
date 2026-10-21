# 开发路线与阶段验收

日期：2026-10-21。**M0–M3 已通过本地 q35/TCG 验收；M4–M15 尚未实现，外设 IRQ 路由启用待对应驱动。** 证据见 [M3 验证记录](docs/validation/M3.md) 与 [STATUS](docs/STATUS.md)。[FIRST_TASK](docs/FIRST_TASK.md) 记录 M0/M1 原始实现顺序；当前 M2 边界见 [CPU 基线](docs/M2_CPU.md)。

## 阶段总览

| 阶段 | 可见结果 | 主要依赖 | 当前状态 |
| --- | --- | --- | --- |
| M0 | 可复现工程与宿主入口 | 无 | 本地验收通过 |
| M1 | UEFI 启动并可靠输出 | M0 | 本地验收通过 |
| M2 | 异常、中断与时钟可诊断 | M1 | BSP 基线验收；外设 IRQ 待驱动 |
| M3 | 受控页管理和堆 | M2 | 本地验收通过；引导保留区未回收 |
| M4 | 第一个隔离的用户 app | M3 | 未开始 |
| M5 | 多进程与阻塞/抢占调度 | M4、M2 | 未开始 |
| M6 | VFS、init、基础 shell | M5 | 未开始 |
| M7 | 源码和应用可持久保存 | M6 | 未开始 |
| M8 | 首批静态 Linux CLI 程序 | M6、M7 | 未开始 |
| M9 | 线程、信号、动态 musl | M8 | 未开始 |
| M10 | 网络和受控包安装 | M7、M9 | 未开始 |
| M11 | 系统内 GCC/G++ 应用开发 | M7–M9；可先离线 | 未开始 |
| M12 | 系统内 Rust/Cargo 应用开发 | M11、M9；联网可后补 | 未开始 |
| M13 | 可发布的 SDK 与应用包 | M4 基础 SDK、M11/M12 | 未开始 |
| M14 | 图形 app、多核、实机扩展 | M9、M13；按子任务拆分 | 未开始 |
| M15 | 可选的工具链/内核自举 | M11–M13，资源满足 | 未开始 |

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

实现边界见 [M3_MEMORY](docs/M3_MEMORY.md)：仅分配 USABLE RAM，完整复制表层级，64 KiB 双 guard 普通栈、2 MiB 固定 buddy 堆；DMA32 仅提供地址约束，尚无 DMA 驱动。

退出条件：分配/释放压力与边界用例通过；保留区域不被分配；guard page/只读页/NX 的相关故障有证据；OOM 无无声内存破坏。

## M4：用户态、ELF 与最小 SDK

目标：宿主编译的自有 app 作为真正用户进程运行。

- [ ] Ring 3、独立用户地址空间、内核栈、可验证 syscall/return 路径。
- [ ] 受限 ELF64 ET_EXEC 装载、段校验、BSS、用户栈与命令行。
- [ ] 最小 native ABI：控制台 write、exit、基础查询，带版本/布局定义。
- [ ] 用户指针复制及访问故障恢复；用户非法访问终止用户进程。
- [ ] 最小 `no_std` Rust/C 用户 runtime、链接脚本和 hello-app 示例。
- [ ] 初始化用户程序需要的 FPU/SSE 状态；内核仍避免使用 SIMD。
- [ ] 显式记录镜像的 ABI personality；不靠 ELF OSABI 猜测。

退出条件：一个独立 ELF 在 Ring 3 输出参数并正常退出；坏 ELF、坏地址、非法 syscall 和用户 page fault 不破坏内核。app 的 syscall 是真实边界，不是直接链接内核函数。

## M5：进程与调度

- [ ] task/process 模型、PID、上下文切换、每线程内核栈。
- [ ] FPU/SIMD 等已启用扩展状态的正确保存/恢复。
- [ ] 抢占、阻塞等待、sleep/wakeup、公平性与闲置路径。
- [ ] spawn/exec/exit/wait、父子关系、退出状态与资源回收。
- [ ] 初版 fork 可 eager-copy；COW 后续单独实现和验证。
- [ ] fd 表与地址空间所有权、关闭/退出时清理。

退出条件：多个进程交错执行；阻塞等待不忙转；一个进程崩溃其他进程继续；反复创建和退出不持续泄露页、栈或 fd。

## M6：VFS、init 与基础 shell

- [ ] VFS 与 mount、路径解析、cwd、文件/目录/符号链接的演进设计。
- [ ] initramfs、tmpfs、devfs、stdin/stdout/stderr。
- [ ] 文件描述符、打开文件描述、seek/dup 共享偏移语义。
- [ ] 原生 init、shell、ls/cat/echo 等少量自有命令。
- [ ] 原生进程执行、管道与重定向的基础接口。
- [ ] 串口输入行编辑和终端抽象，后续可接 termios/PTY。

退出条件：通过 shell 列目录、创建/读取 tmpfs 文件、执行用户 app、查看退出码；管道/重定向正常关闭，不在 EOF 处死锁。此阶段 tmpfs 数据重启丢失，应明确说明。

## M7：持久磁盘与可编辑环境

- [ ] PCI 枚举、现代 virtio-blk、DMA 缓冲区和队列生命周期。
- [ ] 块边界、设备容量、错误与 flush 路径。
- [ ] 明确特性集合的 ext2 读写；未知/不支持特性拒绝挂载。
- [ ] 数据/元数据更新顺序、sync/fsync、受控关机和根盘镜像工具。
- [ ] 最小文本编辑器，先支持打开/编辑/保存；之后扩展终端体验。
- [ ] 默认运行 overlay/临时盘与持久化测试盘分开。

退出条件：系统内保存源码与二进制、正常关机、重启读取校验一致；越界 I/O 与坏文件系统明确失败；记录非日志 FS 的断电局限。

## M8：POSIX 基础与静态 Linux CLI

目标：运行首批固定构建的 Linux x86_64 程序。

- [ ] Linux syscall 分派、errno/布局/flags、用户栈与 auxv。
- [ ] 以固定程序 trace 为依据实现内存、文件、目录、时间、随机数和执行接口。
- [ ] Linux `arch_prctl`/TLS 等所需 CPU 用户态设施。
- [ ] 必要信号基础、基本终端语义、管道、重定向、wait/fork/exec 行为。
- [ ] tiny Linux syscall ELF、静态 musl hello/文件程序、BusyBox 精选 applet。
- [ ] 对程序版本/构建选项/SHA-256 建立兼容测试清单。

退出条件：固定的静态 hello、文件操作工具、BusyBox `echo/ls/cat` 通过，`sh` 的受测管道与重定向用例通过；未知 syscall 返回真实错误。未验证的 applet 保持“未验证”。

## M9：线程、信号、终端与动态 musl

- [ ] clone/线程、TLS、futex、退出清理、阻塞唤醒与超时语义。
- [ ] sigaction、sigprocmask、信号递送/返回、SA_RESTART 相关范围。
- [ ] poll/ppoll 或等效所需接口、非阻塞 I/O、PTY、termios 与作业控制基础。
- [ ] ELF PT_INTERP、受测动态解释器、共享映射、mprotect、动态 TLS。
- [ ] 静态 PIE 的所需装载/重定位策略，按构建实测处理。
- [ ] 按实际软件需求加入最小 `/proc`/`/dev` 等接口，不提供假数据掩盖缺失。

退出条件：动态 musl 小程序与 pthread/futex 测试通过；信号中断阻塞调用和终端 Ctrl+C 等已声明行为通过；共享库缺失时有清楚诊断。不会因此宣称 glibc 可用。

## M10：网络与包环境

- [ ] virtio-net，优先复用适合 `no_std` 的协议栈并验证内核集成。
- [ ] IP、UDP/TCP、DNS、socket 阻塞/非阻塞行为和时间源。
- [ ] 先本地仓库和离线包，再联网安装；锁定依赖、校验包内容。
- [ ] TLS 放在合适的用户态库，准备 CA 与正确时间；禁止跳过证书验证。
- [ ] 软件包清单、安装路径、许可证、升级/失败回滚和用户数据保留。

退出条件：受控网络对端的收发/超时/DNS 测试通过；离线包可安装、升级与卸载；在线能力按实测范围另报。联网能力不是离线 GCC 验收的门槛。

## M11：系统内 C/C++ 应用开发

实施细节见 [TOOLCHAINS](docs/TOOLCHAINS.md)。

- [ ] 在宿主构建可在 guest 运行的 gcc/g++、cc1/cc1plus、assembler/linker、构建工具。
- [ ] 安装 CRT、头文件、libc、libgcc、libstdc++、必要依赖和明确 sysroot。
- [ ] 补齐编译器实际使用的文件、映射、临时文件、子进程、信号和线程语义。
- [ ] 先静态/受控构建包运行，再评估动态工具链；TCC 可作中间验证但不替代 GCC。
- [ ] 使用编辑器在 guest 创建 C/C++ 工程并编译运行。

退出条件：C 的 stdio/文件 I/O 和多文件编译通过；C++ `std::string/vector`、RAII 通过，异常/线程按声明配置分别测试；源码与产物重启保留。只在宿主编译、在 guest 执行不算完成。

## M12：系统内 Rust 与 Cargo

- [ ] 先验证 Linux-musl 兼容 target 的交叉编译 `std` 应用，不改变内核 target。
- [ ] 在宿主构建 guest-hosted rustc、所需 LLVM、Cargo、标准库和 linker/sysroot。
- [ ] 在 guest 中编译、链接、运行含 Vec/String、文件 I/O、参数的小型 Rust 应用。
- [ ] `cargo build --offline`：准备固定 registry/vendor/lockfile、依赖与配置。
- [ ] 分别验证线程、子进程、build.rs、proc-macro `.so` 装载和 C FFI。
- [ ] M10 完成后扩展联网 registry/Git、TLS/CA 和依赖下载。
- [ ] 若要 AnovaOS 原生 target：独立实现 libc/`std` 的平台端口，不能仅改 target JSON。

退出条件：guest 内编辑 Rust 源码，运行 rustc 和 Cargo 完成构建，执行产物且重启保留；运行期间无宿主远程编译。离线基本闭环与完整 Cargo 能力分项标记。原生 target 移植是另一个成果。

## M13：应用 SDK、包与开发体验

- [ ] 将 M4 起的 runtime/ABI 固化为带版本的 SDK，并提供稳定错误模型。
- [ ] 定义应用清单：ABI、目标架构、依赖、入口、权限和最低系统能力。
- [ ] 文档化交叉开发、系统内开发、日志、崩溃符号化与资源调试。
- [ ] ELF 符号与栈回溯先行，GDB stub/用户调试器按能力推进。
- [ ] 发布 C/C++/Rust app 模板、离线依赖示例、安装升级与用户数据规则。
- [ ] 对 ABI 变更增加版本策略和兼容回归。

退出条件：新开发者按文档创建应用、构建、安装、运行、查看错误并升级；native 和 Linux 兼容包不会因目标/ABI 混淆而被误装。

## M14：桌面、多核与硬件扩展

这是后续工作集合，分别建子里程碑，避免把它视作一个小任务。

- [ ] framebuffer → 输入事件 → 合成器 → 窗口/字体/剪贴板/图形终端 → GUI SDK。
- [ ] 首个自有 GUI app；再验证 SDL 等可移植库与软件渲染。
- [ ] glibc 的指定版本/软件组合、Wayland/X11 等兼容按明确需求单独立项。
- [ ] SMP：per-CPU、跨核唤醒、锁与内存序、TLB shootdown、中断亲和性。
- [ ] 指定实机：ACPI、电源管理、NVMe/AHCI、USB/xHCI、输入、网卡；逐设备验证。
- [ ] GPU 加速需要驱动、用户态图形栈与 ioctl 等完整链路；不以 framebuffer 替代 Vulkan 支持。

退出条件按子任务给证据。GUI shell、SMP、某台实机和某种 Linux GUI 软件分别记录支持状态。

## M15：可选自举与系统自编译

- [ ] 在 guest 内重建一个受控版本的 C/C++ 工具或编译器阶段。
- [ ] 评估 GCC 多阶段 bootstrap、Rust/LLVM bootstrap 的内存、磁盘、时间和依赖。
- [ ] 再评估使用系统内工具编译自身内核、生成镜像并在下次启动运行。
- [ ] 记录 bootstrap 依赖、产物比较和已知非确定性；不以一次成功宣称可复现自举。

退出条件：按声明的范围给出完整构建日志、产物和重启证据。**M15 不属于“能在系统内开发 app”的必要条件。**
