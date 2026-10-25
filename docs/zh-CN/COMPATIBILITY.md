<!-- Record verified native/Linux software profiles, package admission and source-reconstruction compatibility limits. -->
# 现有软件兼容策略

[English](../en/COMPATIBILITY.md) | **简体中文**

本地验收范围覆盖原生应用、持久化存储、精选 BusyBox、musl 静态/PIE/动态运行时、
受控网络/包、guest C/C++/Rust/Cargo、原生 SDK 和 M14 GUI。M15 另验证受控源码重建。
这些是版本化受限范围，不代表通用 Linux/glibc 兼容。当前边界见
[M9 runtime](milestones/M9_RUNTIME.md)、[M12 Rust](milestones/M12_RUST.md) 和
[STATUS](guides/STATUS.md)。

## 1. 兼容层次

| 层次 | 含义 | 本项目策略 |
| --- | --- | --- |
| 源码兼容 | 为 AnovaOS 重新编译已有软件 | 先支持可移植 C/C++/Rust 程序及受限 POSIX API |
| Linux 二进制兼容 | 无需改源码重编译，运行特定 Linux ELF | 实现 x86_64 Linux 用户态 ABI 的受测子集 |
| Windows 兼容 | 运行 PE/Win32 程序 | 长期探索 Wine/专门移植；不属于初期路线 |
| 虚拟机运行 | 在虚拟化的另一个完整 OS 中运行软件 | 独立未来方向，不能算本系统 ABI 兼容 |
| 内核驱动兼容 | 装载其他内核的驱动 | 不支持，驱动必须针对本内核实现或适配 |

ELF 是文件格式，System V AMD64 描述一部分调用与运行环境规则，POSIX 描述 API/行为，Linux syscall ABI 则是内核接口。它们不能相互替代。

## 2. 主线选择

1. native `no_std` app 验证用户隔离和 SDK。
2. 固定版本的 Linux x86_64 静态 ET_EXEC 小程序。
3. 静态 musl 的 hello/文件工具与 BusyBox 精选 applet。
4. musl 动态链接、TLS、信号、线程、构建工具。
5. GCC/G++、rustc/Cargo；以系统内应用开发闭环驱动完善。
6. 有具体需求后再选定 glibc 软件组合、图形库和桌面协议。

静态链接主要减少动态解释器/共享库部署问题，不会移除内核依赖。BusyBox 的 applet 差异很大；`echo` 能运行不能推出 shell、网络和 mount 命令可运行。

## 3. ABI 实现检查项

### 3.1 装载和进程启动

- ELF64 头、PT_LOAD、BSS、地址/长度/对齐、映射权限和受支持的 ELF 类型。
- 16 字节等必要对齐、argc/argv/envp、auxv 和正确的入口环境。
- ELF personality 使用显式元数据。Linux 常使用 System V ELF OSABI，不能据此自动区分 native 与 Linux。
- 初期选择可控 ET_EXEC；静态 PIE/动态 PIE 作为后续独立能力。
- PT_TLS、FS base、FPU/SSE 状态；进程切换时保持各线程状态。

### 3.2 系统调用

Linux x86_64 常规 syscall 约定：RAX 为调用号，参数通过 RDI/RSI/RDX/R10/R8/R9 传递，结果放入 RAX；RCX/R11 受 `SYSCALL` 影响。返回值遵循 Linux 的负 errno 规则，不能混用 libc 的 `-1 + errno` 包装语义。

以下是调查起点，**不是静态程序的完整需求表**：

| 能力 | 可能涉及的接口/语义 | 验证重点 |
| --- | --- | --- |
| 控制台与文件 | read/write/writev、openat/close、fstat/newfstatat、lseek | 部分读写、errno、fd 生命周期 |
| 内存 | brk、mmap/munmap/mprotect、页权限 | 对齐、MAP flags、失败路径 |
| 启动与退出 | execve、exit/exit_group、wait4、getpid | 参数/环境、退出码、清理 |
| 路径与目录 | getdents64、readlink、cwd、rename/unlink | 结构布局、相对目录、打开文件偏移 |
| 时间与随机 | clock_gettime、nanosleep、getrandom | 单调时间、阻塞/错误和真实熵来源 |
| TLS 与线程 | arch_prctl、clone、set_tid_address、futex | FS base、flags、join、超时与清理 |
| 信号 | rt_sigaction、rt_sigprocmask、rt_sigreturn | x86_64 信号帧、屏蔽和重启 |
| shell 与终端 | pipe2、dup2/dup3、ioctl、termios、进程组 | EOF、Ctrl+C、重定向、作业控制 |
| I/O 等待 | poll/ppoll、后续 epoll 等 | 唤醒竞态、超时、关闭与信号 |
| 软件探测 | uname、受需的 `/proc`、limits 等 | 真实能力、精确错误、不能假报支持 |

实际需求依编译器/libc/软件版本和构建选项变化。只能根据 trace、源代码和行为测试添加支持；探测失败可被 libc 合理回退时，返回规定错误即可。

对外宣称能力前必须实现其语义；例如 futex 不能一律返回 0，fsync 不能丢弃未写数据后报告成功。随机接口不能拿固定模式冒充安全随机数据。

### 3.3 动态链接与 glibc

内核装载 PT_INTERP 对应解释器，并提供 mmap、mprotect、文件与启动信息。共享库的符号解析和通常重定位交给用户态动态链接器。

musl 和 glibc 是不同的用户态组合。不能混用不匹配的解释器和 sysroot；glibc 还涉及其实际版本使用的符号版本、线程、信号、TLS、vDSO 回退、NSS/locale 等行为。初期不运行任意发行版的二进制目录树。

`ioctl` 与 `/proc` 等同样属于实际软件依赖。仅覆盖 syscall 编号不能推出完整 Linux ABI 兼容。

## 4. 首批软件矩阵

自有 native 样例已按 [M4 验证记录](validation/M4.md) 验收；受限 Linux 静态 CLI 和 musl 运行时分别见 [M8 验证](validation/M8.md) 与 [M9 验证](validation/M9.md)。实际 ELF 哈希见对应记录和镜像中的应用清单。

| 软件/构建 | 计划阶段 | 首个受测功能 | 当前状态 |
| --- | --- | --- | --- |
| 自有 native Rust `no_std` | M4 | write/exit/info、参数、BSS、x87/SSE2、坏指针隔离 | q35/TCG 本地通过 |
| 自有 native C freestanding | M4 | write/exit/info、参数、BSS、SSE2、退出码 7 | q35/TCG 本地通过 |
| 自有 Linux syscall ELF | M8 | errno、auxv、TLS、fork/exec/env/wait、CLOEXEC、SIGPIPE、匿名内存 | q35/TCG 本地通过 |
| 静态 musl 1.2.4 hello/file | M8 | TLS、errno、malloc、文件读写 | q35/TCG 本地通过 |
| BusyBox 1.36.1 受限静态构建 | M8 | echo/ls/cat、指定 sh 管道重定向及退出码 | q35/TCG 本地通过；其他 applet 未验证 |
| 动态 musl 1.2.4 小程序 | M9 | PT_INTERP、共享库、dlopen、动态 TLS | q35/TCG 本地通过；受限 profile |
| musl 静态/静态 PIE/动态 pthread 样例 | M9 | 线程同步、join、futex、信号返回、poll、PTY | q35/TCG 本地通过；完整边界见 M9_RUNTIME |
| GNU make 4.4.1 与 binutils 2.37 | M11 | 子进程、文件时间、无改动/选择性增量构建 | q35/TCG 本地通过；离线 profile |
| GCC/G++ 11.2.0 受控构建包 | M11 | guest 内编译/链接 C 与 C++、异常和线程 | q35/TCG 本地通过；完整边界见 M11_TOOLCHAIN |
| Rust 1.90.0 `std` app | M12 | Vec/String、参数、文件、线程与线程内子进程 | q35/TCG 本地通过；musl target |
| rustc/Cargo 1.90.0 受控二进制包 | M12 | 系统内 std、离线 vendor、build.rs/C FFI、过程宏 | q35/TCG 本地通过；范围见 M12_RUST |
| SQLite CLI 等可移植工具 | M13 后按需 | 数据写入、锁、持久化 | 计划 |
| 指定 glibc CLI 软件 | 独立后续任务 | 指定版本的受测行为 | 计划 |
| SDL/图形软件 | 独立后续任务 | 软件渲染、窗口与输入 | 计划 |
| 浏览器/Electron/Wine/商业 app | 未排期 | 另行分析依赖与成本 | 未纳入首版 |

## 5. 软件验证记录

每个软件建立记录，至少包括：

- 上游 URL、版本/提交、许可证、源码和二进制 SHA-256。
- 编译器、target、libc/sysroot、静态/动态、PIE 和功能开关。
- 包含的动态解释器/共享库、运行数据、环境变量。
- 宿主 trace 和 guest 结果，覆盖的命令、输入、预期输出与退出状态。
- 当前支持、不支持、未验证的功能，以及对应缺失接口。
- 最近验证的内核 commit、机器配置、内存与 vCPU 数。

状态仅使用：计划、构建中、可启动、部分通过、已验证、回归失败。无法执行的测试保留“未验证”，不能把准备好的二进制视为通过。

## 6. 对照与回归方法

先在受控 Linux 环境对相同二进制记录行为；随后在 AnovaOS 上运行相同输入，比较输出、退出码、文件结果与关键系统调用语义。宿主 `strace` 只是发现线索，不能证明 guest 正确。

重点负例包括：坏地址、部分 I/O、EINTR、无效 fd、EOF、并发关闭、超时、资源不足、文件截断、缺解释器/共享库。按软件需求逐步扩展，不一次追求完整 Linux 测试集。

M8 的确切构建、syscall 子集与限制见 [M8_LINUX](milestones/M8_LINUX.md) 和 [验证记录](validation/M8.md)。这是 M8 历史记录。M9 已验证声明的自定义信号处理器、原生串口 Ctrl+C、PTY 前台信号及动态 musl；上述 BusyBox shell 测试仍不代表完整交互作业控制或任意脚本。

## M9 实测运行时增量

| 程序/输入 | 结果 | 边界 |
| --- | --- | --- |
| 自有 musl 1.2.4 静态 ET_EXEC | 已验证 | pthread/cond/mutex/join、TLS、futex、信号、poll、PTY、生命周期负例 |
| 同源码静态 PIE | 已验证 | musl rcrt1 自重定位，同一运行时用例 |
| 同源码 PT_INTERP 动态 ELF | 已验证 | 实际 musl 解释器、启动 TLS .so、线程后 dlopen、动态 TLS |
| 缺解释器/缺 dlopen 库 | 拒绝已验证 | ENOENT 保留 exec 调用者，loader 报出缺库名 |
| UART 动态启动与 Ctrl+C | 已验证受限终端行为 | 不包含完整 ash jobs、实时信号或 glibc |

确切构建参数、运行时校验和、guest 证据、宿主 trace 差异和限制见 [M9 验证](validation/M9.md)。

## M10 实测联网增量

原有 Linux personality 提供有界 IPv4 TCP/UDP 客户端 socket/connect/send/recv、非阻塞/poll
和信号中断；实际 guest DNS A、TLS 1.2 与包客户端通过。文件 readv、dup、受限 chmod(0755)
补齐其运行需要，原有 musl 静态/PIE/动态哈希保留为 M9 基线。不代表 curl/wget、musl
getaddrinfo、TCP server、通用消息/向量 socket、文件系统 AF_UNIX、IPv6 或任意 Linux 网络
程序可用；不支持的接口返回真实错误。

## M11 离线编译器范围

| 软件 | 已验证行为 | 限制 |
| --- | --- | --- |
| GCC/G++ 11.2.0 | guest 编译/汇编/链接，C argv/stdio/malloc/文件/多源；C++17 string/vector/RAII/异常/std::thread | 小型静态应用，不含通用项目、LTO/OpenMP/sanitizer、自举 |
| GNU binutils 2.37 | guest assembler/linker/collect2、静态 ELF、未定义符号失败 | 其他工具/目标和动态应用工具链未验 |
| GNU make 4.4.1 | shell/子进程、无操作及选择性增量 | 串行 fork 配置，任意脚本未验 |
| musl 1.2.4 与 libgcc/libstdc++ 11.2.0 | `/tc` 下匹配的源码构建 CRT/头文件/归档与 C/C++ 测试 | 不代表 glibc |

Linux 盘从 `.anova-abi` 显式继承 personality。M11 增加 vfork 父进程暂停、私有文件快照、
mode/umask、资源查询；不支持的可选探测返回 ENOSYS。边界见 [M11_TOOLCHAIN](milestones/M11_TOOLCHAIN.md)，
日志/哈希见 [M11 验收](validation/M11.md)。

## M12 离线 Rust 范围

锁定官方 Rust 1.90.0 `x86_64-unknown-linux-musl` rustc/Cargo/std/rust-lld 在 AnovaOS 内核
运行；宿主交叉测试只是前置检查，验收二进制来自 guest UART 编辑和编译。独立验证 std/
线程/子进程输出、编译器启动、独立 proc-macro、锁定离线 itoa、build.rs 生成输入/环境及
 guest GCC/ar C FFI。固定 Cargo 宏图包括共享 host/build 依赖、生成宏输入、传递重建从
42 到 84、非法输入退出 101 且旧应用仍可运行、源码修复成功；还检查 freshness/源码变化、
真实 parser 失败、盘哈希、e2fsck 和重启运行。运行时覆盖备用信号栈、detached 退出、非法
返回故障、定位/resize 文件和 OFD 锁。

仅证明 [M12_RUST](milestones/M12_RUST.md) 的受限范围，任意 Cargo/宏图、原生 AnovaOS std、
Rust/LLVM 源码构建/自举未验。有界 AF_UNIX SOCK_SEQPACKET pair 已有，SOCK_STREAM pair
返回 ESOCKTNOSUPPORT；Linux/VFS 名称/路径为 255/512 字节，原生 v1 目录 wire 记录不变。
见 [M12 验收](validation/M12.md)。

## M12 在线 Cargo 范围

同一官方 Cargo 1.90.0 从 HTTPS sparse registry 下载校验锁定 crate，通过 HTTPS smart HTTP
获取完整 revision 的 Git 依赖。`cargo-online` 选择联网与 `/persist/cargo-home`，普通 Cargo
默认离线。受控对端独立拒绝不可信/过期/域名错误证书、损坏/截断归档和应用超时。
guest 编译、无操作构建与无 NIC 重启后全新 target 重建保留源/缓存/锁文件/ELF 哈希。

独立公开门禁下载 crates.io itoa 1.0.15 和 GitHub dtolnay/itoa 的
`e2766b868e4ac1ae2bf5bea1ac43d4c0da23b899`，不替换源。本地验收使用配置好的宿主 HTTP 代理
与公共 PEM 根，不证明直接公网 DNS/传输、任意项目、私有 registry 凭据、SSH、publish 或 IPv6。

运行时增加 select 就绪/期限/EINTR、实际时间戳、只读共享文件一致性与 fork/close/unlink/
名称替换的 inode 生命周期、真实只读写故障、堆 OOM/复用。可写共享文件映射和持久 mmap
writeback 仍拒绝。见 [M12_NETWORK](milestones/M12_NETWORK.md) 与 [M12 验收](validation/M12.md)。

## M15.5 持久文件容量

ext2 普通文件在原有直接/单级间接块格式内支持 4 MiB，已有镜像无需重新格式化。tmpfs 和
只读共享缓存仍为 1 MiB。活动共享映射使超限增长返回 `EBUSY`；普通文件 I/O/私有映射可访问
更大的持久文件。扩容不代表更多编译器/自举或任意 Cargo 兼容。见 [M15 内核](milestones/M15_KERNEL.md)。

## M13 应用包准入

SDK 1.0.0 冻结原生 v1 编号/错误/布局；ANOVAPK2 明确 target/ABI/入口/依赖并检查可执行一致性。
原生盘接收原生包，Linux 盘接收受限静态 ET_EXEC Linux 包。安装不能改变 VFS personality，
挂载 ABI 标记不能重写/替换。动态/PIE 包准入后置，独立 M9 ELF 运行时不变。不代表通用
Linux 包、glibc、依赖求解、签名认证或进程沙箱。见 [M13](milestones/M13_SDK.md) 与 [验收](validation/M13.md)。

## M15.5 guest 内核重建

`bootstrap-kernel-test` 用 AnovaOS Linux-musl 进程中的继承 Rust/Cargo/rust-lld 重建真实
裸机内核，宿主打包原样保留的 ELF，独立提取和实际启动身份验证目标内核。三次会话覆盖
构建、启动、语法拒绝保留旧内核和恢复。见 [M15 kernel](milestones/M15_KERNEL.md) 与
[证据](validation/M15.5.md)。验收配置为带 bootstrap metrics、无 GUI 的 stripped opt-z，
目标库仍是继承种子。编译器重建与 guest 独立镜像生成仍独立；[M15.6](validation/M15.6.md)
已通过独立重复干净构建、最终产物精确比较，保留中间文件差异。
