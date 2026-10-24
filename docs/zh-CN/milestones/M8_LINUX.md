<!-- Translate the M8 LINUX contract and preserve exact commands and historical evidence. -->
# M8：静态 Linux 命令行

[English](../../en/milestones/M8_LINUX.md) | **简体中文**


本页记录历史 M8 范围。[M9](M9_RUNTIME.md) 替代单线程、自定义信号、终端和动态链接限制，[M11](M11_TOOLCHAIN.md)、[M12](M12_RUST.md) 扩大启动、ELF 和内存容量。当前 Linux 人格为 `linux.x86_64.v1`，与原生 ABI 分离。M8 只验证固定 Linux AMD64 子集，不代表 Linux 内核、发行版 ABI、POSIX 认证或系统内编译，证据见 [M8 验证](../validation/M8.md)。

## 身份与启动

归档类型 5 明确选择历史 `linux.x86_64.static.v1`，类型 4 仍为原生。VFS 跟随符号链接后读取策略，不凭 ELF OSABI、后缀或缺少 PT_INTERP 推测人格。原生持久盘仍由 `.anova-abi` 指定原生 v1，复制 Linux ELF 不会转换策略。

两种装载器都要求受检 ET_EXEC、私有清零页、文件支持的可执行入口和 W^X。Linux 额外接受静态 PT_TLS 和 GNU_RELRO 元数据，程序头必须位于可装载段；musl 用 AT_PHDR 初始化静态 TLS，但没有动态 TLS，也未对 RELRO 增加写保护。M8 拒绝 PT_INTERP、PT_DYNAMIC、ET_DYN、静态 PIE、可执行栈和未知程序头。

Linux RSP 对齐 16 字节，依次为 argc、argv、NULL、envp、NULL、以 AT_NULL 结束的 auxv。辅助值包含 PHDR/PHENT/PHNUM、PAGESZ、ENTRY、BASE=0、FLAGS=0、root 身份、SECURE=0 和 16 字节 AT_RANDOM。历史 argv/envp 各最多 16 串，每串 128 字节，共占顶端 4 KiB，超限拒绝。exec 先复制嵌套指针；坏路径、向量、ELF 或 OOM 保留调用者。

Linux 编号使用 RAX，参数为 RDI/RSI/RDX/R10/R8/R9，独立派发，返回负 x86_64 errno，未知调用 ENOSYS。Linux 转译集中在适配器和 `anova-abi::linux`，VFS、内存和调度策略保持中立。

## 调用与容量

文件调用提供真实偏移、EOF、部分传输和阻塞管道，单次读写最多 4 KiB，writev 最多 16 向量。open/openat 支持读写、创建、排他、截断、追加、目录、largefile 和 CLOEXEC，M8 openat 仅 AT_FDCWD。stat 系列显式序列化 144 字节 AMD64 结构，报告真实虚拟 inode、类型、大小、固定后端权限和零时间戳。getdents64 每次返回一条对齐变长记录，坏或过小输出不消费游标。

路径操作沿 VFS 受限 UTF-8 语义，access 保守拒绝写权限检查。dup2、pipe、pipe2 和 fcntl 维护共享打开对象及描述符局部 CLOEXEC，exec 成功才关闭；pipe2 标志为零，fcntl 支持 DUPFD/GETFD/SETFD/DUPFD_CLOEXEC。fsync/sync 使用真实磁盘路径并传播失败。

历史 brk 为 `0x40000000` 起 8 MiB，失败返回旧值，缩小释放页；mmap 为 `0x50000000` 起 8 MiB 私有匿名 RW，空洞可复用、部分失败回滚，无共享或文件映射。musl 的 brk 内 PROT_NONE/MAP_FIXED 保护请求实际撤销访问。arch_prctl 仅 SET_FS/GET_FS，检查低半规范基址并复制输出，FS 跨 syscall、抢占和 fork 保留，exec 重置。

fork/execve/exit/exit_group/wait4 提供真实进程和 Linux 四字节状态，支持目标、任意子、WNOHANG 和空状态指针，无 rusage 或进程组等待。身份只有单 root，每进程一个线程；set_tid_address 注册退出清理地址，但不声称线程或 futex 唤醒。clock_gettime/nanosleep 使用 100 Hz 已投递单调节拍，相对睡眠向上取整，无墙钟或信号重启。

getrandom 最多 4 KiB，使用探测后的 RDRAND 和有限重试；不支持标志或硬件失败明确返回错误，零长度不访问指针或熵源，不以计数器伪造随机。QEMU 要求 `qemu64,+rdrand`；缺熵源拒绝加载 Linux。uname 如实报告 AnovaOS。原生父进程等待 Linux 信号退出得到原布局 reason=3。

M8 无通用权限身份：新文件经固定 022 掩码须为 0644，目录须为 0755，其他模式 EOPNOTSUPP；既有文件忽略无用的创建模式。ioctl 返回 ENOTTY，坏描述符 EBADF，没有 termios/PTY。后续权限和时间戳由 M11/M12 独立提供。

## 信号与复制边界

支持掩码、默认或忽略处理、阻塞 SIGPIPE 的待处理状态、默认终止和信号退出码。kill 仅正活 PID、信号 0 或受限 KILL/PIPE/TERM；KILL 不可屏蔽忽略，忽略信号丢弃，未屏蔽致命信号在下一用户指令前终止，包括睡眠或 I/O 阻塞者。

自定义处理函数注册返回 ENOSYS，没有用户信号帧、rt_sigreturn、SA_RESTART、sigsuspend、停止继续或进程组。BusyBox ash 的 SIGCHLD/INT 注册明确失败，固定同步等待和非交互脚本仍可用；不能由此声称 trap、Ctrl+C、作业控制或任意脚本。

字符串按用户页剩余空间分块复制，到 NUL 即停止，不触碰后方保护页；argv/envp 共用有界解码器并原地填写。原生和 Linux 启动布局共用受检放置算法。writev 在 I/O 前完整导入描述符并验证长度和地址，后续载荷访问故障只返回已完成字节，见 [优化验证](../validation/M8_OPTIMIZATION.md)。

## 构建、使用与许可

锁定 musl 1.2.4 和未修改 BusyBox 1.36.1，后者提交为 `1a64f6a20aaf6ea4dbba68bbfa8cc1ab7e5c57c4`。初次获取精确版本，后续拒绝源码变化，清单记录 ELF、配置和归档摘要。宿主需 musl-tools、make、qemu-user；命令与客户系统示例保留下方。

trace-compat 使用 QEMU-user 对这些 ELF 在 Linux 上追踪，不冒称完整 ptrace；同架构 exec 可能离开模拟器，因此另追踪 echo/cat/ls。test-compat 要求完整历史回归、真实 Linux 标记、5000 字节 ash 管道、资源回收和 QEMU 成功退出。terminal-test 在普通镜像从 UART 启动 musl/ash；存储门禁保留 M7 全部契约。压力只在测试镜像运行。

`/bin` 仍是原生工具，ash 不能把它们当 Linux 执行，应使用 `/linux` 路径。BusyBox 为 GPL-2.0-only，musl 保留 MIT 归属，均不链接内核。分发 ISO 时还需保留生成的 `build/images/busybox-1.36.1-source.tar.gz`，详见 [许可](../guides/LICENSING.md) 和 [第三方记录](../third-party/musl.md)。

## 原始命令与输出

以下命令、源码与日志保留原文；执行位置及通过条件见上文。

```sh
cargo xtask apps
cargo xtask trace-compat
cargo xtask test-compat
cargo xtask verify
cargo xtask terminal-test
cargo xtask persistence-test
```

```sh
linux-musl test
/linux/echo hello
/linux/ls /etc
/linux/cat /etc/motd
/linux/sh
```

```sh
/linux/echo hello | /linux/cat > /tmp/linux-note
/linux/cat /tmp/linux-note
exit
```

## 规范与来源链接

- [Linux x86_64 syscall table](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/arch/x86/entry/syscalls/syscall_64.tbl)
- [Linux x86_64 stat UAPI](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/arch/x86/include/uapi/asm/stat.h)
- [Linux FS/GS documentation](https://www.kernel.org/doc/html/latest/arch/x86/x86_64/fsgs.html)
- [musl startup/TLS source](https://git.musl-libc.org/cgit/musl/tree/src/env/__init_tls.c?h=v1.2.4)
- [BusyBox 1.36.1 upstream source](https://git.busybox.net/busybox/tree/?h=1_36_1)
