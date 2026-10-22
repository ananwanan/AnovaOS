<!-- Record verified native/Linux profiles and remaining existing-software compatibility boundaries. -->
# 现有软件兼容策略

The locally verified M0–M12 baseline covers native Rust/C applications, persistent
storage, the pinned BusyBox CLI and static/PIE/dynamic musl runtime subset,
controlled M10 networking/packages, M11 C/C++ development and M12 bounded
offline/online Rust/Cargo development. This establishes the versioned profiles below, not
general Linux/glibc compatibility. See [M9 runtime](docs/M9_RUNTIME.md) and
[M12 Rust](docs/M12_RUST.md) for current bounds.

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

自有 native 样例已按 [M4 验证记录](docs/validation/M4.md) 验收；受限 Linux 静态 CLI 和 musl 运行时分别见 [M8 验证](docs/validation/M8.md) 与 [M9 验证](docs/validation/M9.md)。实际 ELF 哈希见对应记录和镜像中的应用清单。

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
| 指定 glibc CLI 软件 | M14 子任务 | 指定版本的受测行为 | 计划 |
| SDL/图形软件 | M14 子任务 | 软件渲染、窗口与输入 | 计划 |
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

M8 的确切构建、syscall 子集与限制见 [M8_LINUX](docs/M8_LINUX.md) 和 [验证记录](docs/validation/M8.md)。这是 M8 历史记录。M9 已验证声明的自定义信号处理器、原生串口 Ctrl+C、PTY 前台信号及动态 musl；上述 BusyBox shell 测试仍不代表完整交互作业控制或任意脚本。

## M9 measured runtime additions

| Program/input | Result | Boundaries |
| --- | --- | --- |
| First-party musl 1.2.4 static ET_EXEC | Verified | pthread/cond/mutex/join, TLS, futex, signals, poll, PTYs, lifecycle negatives |
| Same source as static PIE | Verified | Musl rcrt1 self-relocation; same runtime cases |
| Same source as PT_INTERP dynamic ELF | Verified | Actual musl interpreter, startup TLS .so, dlopen after thread creation and dynamic TLS |
| Missing interpreter / missing dlopen library | Verified rejection | ENOENT leaves exec caller intact; loader names missing library |
| UART dynamic launch and Ctrl+C | Verified within declared terminal profile | No complete ash jobs, realtime signals or glibc claim |

Exact build flags, runtime checksum, guest evidence, host trace differences and
remaining limits are in [M9 validation](docs/validation/M9.md).

## M10 measured additions

The [controlled M10 profile](docs/M10_NETWORK_PACKAGES.md) adds AF_INET client
TCP/UDP, bind/connect/read/write/sendto/recvfrom, names/shutdown, SO_ERROR and I/O
timeouts, poll/nonblocking behavior and signal interruption. Actual guest DNS A,
TLS 1.2 and package clients are tested. File readv, dup and restricted chmod(0755)
complete their measured runtime needs. Existing musl static/PIE/dynamic hashes
remain the M9 baseline. This does not establish curl, wget, musl getaddrinfo,
TCP servers, general socket vectored/message APIs, filesystem AF_UNIX, IPv6 or arbitrary Linux
network application support. Unsupported interfaces return real errors.

## Verified M11 offline compiler profile

| Software | Verified behavior | Remaining limits |
| --- | --- | --- |
| GCC/G++ 11.2.0 | Actual guest C/C++ compile/assemble/link; C argv/stdio/malloc/files/multiple sources; C++17 string/vector/RAII/exceptions/std::thread | Small static applications; no general project, LTO/OpenMP/sanitizer or self-bootstrap claim |
| GNU binutils 2.37 | Guest assembler, linker and collect2 pipeline; static ELF output and unresolved-symbol failure | Other utilities/targets and dynamic application toolchains unverified |
| GNU make 4.4.1 | Real shell/child compilation, no-op and selective incremental build | Serial fork configuration; arbitrary build scripts unverified |
| musl 1.2.4 and libgcc/libstdc++ 11.2.0 | Matching source-built CRT/headers/archives in `/tc`; generated C/C++ runtime tests | No glibc compatibility claim |

Linux disk personality is explicitly inherited from its `.anova-abi` manifest.
M11 adds real vfork parent suspension, private file snapshots, mode/umask and
resource-query support; optional unsupported compiler probes return ENOSYS.
Exact bounds and exclusions: [M11_TOOLCHAIN](docs/M11_TOOLCHAIN.md).
Actual logs and file hashes: [M11 acceptance](docs/validation/M11.md).


## Verified M12 offline Rust profile

Official checksum-locked Rust 1.90.0 `x86_64-unknown-linux-musl` rustc/Cargo/std
and rust-lld run on the AnovaOS kernel. A host cross test is preliminary;
acceptance binaries compile from UART-edited sources inside the guest. Separate
tests cover std/threads/child output, compiler startup, a standalone procedural macro,
locked offline itoa, build.rs generated input/environment and guest GCC/ar C FFI.
The fixed Cargo macro graph includes an ordinary host/build dependency, generated
macro input, transitive rebuilds from 42 to 84, invalid-input status 101 and an
unchanged runnable prior application, followed by a successful source repair.
Cargo freshness/source edits, a real parser failure, disk hashes, e2fsck and
reboot execution are checked. Runtime tests include alternate signal stacks,
detached exit, invalid-return faults, positioned/resized files and OFD locks.

This establishes the bounded profile in [M12_RUST](docs/M12_RUST.md), not general
Cargo/ecosystem compatibility. Arbitrary macro graphs, native AnovaOS std
and Rust/LLVM source construction/bootstrap remain unverified.
M12 also adds bounded anonymous AF_UNIX SOCK_SEQPACKET pairs; SOCK_STREAM
pairs remain rejected with ESOCKTNOSUPPORT. Linux/VFS names and paths use
255/512-byte bounds without changing native v1 directory wire records.
Actual logs and hashes: [M12 acceptance](docs/validation/M12.md).


## Verified M12 online Cargo profile

The same official Cargo 1.90.0 downloads a checksum-pinned crate from an HTTPS
sparse registry and a full-revision Git dependency over HTTPS smart HTTP.
`cargo-online` selects this opt-in transport and `/persist/cargo-home`; ordinary
Cargo remains offline by default. Independent controlled-peer tests reject
untrusted/expired/wrong-name certificates, corrupt/truncated archives and an
application timeout. Guest compilation, no-op builds and fresh-target rebuilds
after a no-NIC reboot preserve inspected source/cache/lockfile/ELF hashes.

The separate public gate downloads crates.io itoa 1.0.15 and GitHub dtolnay/itoa
revision `e2766b868e4ac1ae2bf5bea1ac43d4c0da23b899` without source replacement.
Local public acceptance uses a configured host HTTP proxy and public PEM roots;
direct public DNS/transport is not established. It does not imply arbitrary
Cargo projects, private registry credentials, SSH, Cargo publish or IPv6.

Runtime coverage adds select/readiness/deadline/EINTR, actual file timestamps,
readonly shared-file coherence and inode lifetime across fork/close/unlink/name
replacement, an actual readonly write fault, and real heap OOM/reuse. Writable
shared-file mappings and persistent mmap writeback remain rejected. Exact bounds
and configuration: [M12_NETWORK](docs/M12_NETWORK.md). Actual command results:
[M12 acceptance](docs/validation/M12.md).
