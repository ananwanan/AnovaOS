<!-- Translate the M9 RUNTIME contract and preserve exact commands and historical evidence. -->
# M9：Linux 运行时

[English](../../en/milestones/M9_RUNTIME.md) | **简体中文**


M9 扩展 M8 的显式 `linux.x86_64.v1` 人格，原生 v1 独立，不通过 ELF OSABI 猜测。实测见 [M9 验证](../validation/M9.md)。[M11](M11_TOOLCHAIN.md)、[M12](M12_RUST.md) 替代下述历史栈和容量限制，后续增加 vfork、文件快照、备用信号栈及编译器。

## 所有权、线程与 futex

进程、线程和僵尸共享 32 个调度槽。每活跃任务有独立 64 KiB 保护栈、GPR/x87/SSE2 上下文、FS 基址、信号掩码和待处理状态。历史调度器栈为 128 KiB，终端缓冲区在栈外可失败分配；内核不使用 SIMD，AVX、SMP 和内核抢占未启用。

实测 musl clone 要求 CLONE_VM/FS/FILES/SIGHAND/THREAD；SETTLS、PARENT_SETTID、CHILD_SETTID、CHILD_CLEARTID 在发布前验证，SYSVSEM 共享空信号量空间，DETACHED 按 Linux 忽略，其他组合拒绝。getpid 返回线程组 ID，gettid 返回真实 TID；无新栈且带 SIGCHLD 的普通 clone 使用立即复制的 fork。

线程引用计数共享根页表、页所有权、描述符表、cwd 和信号处理方式。Shared 控制块分配失败返回 OOM，空描述符占位不分配，继承在发布引用前先分配。RefCell 保证独占修改，调度与用户复制在 IF=0 下执行，无引用跨切换。fork 复制私有页并保留共享匿名后备；线程关闭共享描述符对同伴可见，最后所有者才释放。exec 先完整加载程序与解释器，再移除同组线程、转移 leader 身份、重置信号并在分离表上关闭 CLOEXEC，失败保留调用者。

futex 支持 WAIT/WAKE、WAIT_BITSET/WAKE_BITSET、REQUEUE/CMP_REQUEUE。BSP 上比较和入队为原子操作，私有键用根身份，共享键用永不复用的分配身份和偏移，从而支持共享匿名页跨进程等待。值不匹配 EAGAIN、对齐错误 EINVAL、指针错误 EFAULT、超时 ETIMEDOUT；相对与绝对单调时间向 100 Hz 取整，重排保留截止时间和掩码。无 PI、robust、墙钟截止或优先级继承。

线程退出在释放根之前清除受检 clear-child-tid 并唤醒等待者。exit_group 结束整组，leader 的 pthread_exit 等待其余线程，父进程仅在整组结束后收到最终状态。共享物理页在最后映射所有者释放且低半表销毁后回收。

## 信号

rt_sigaction、rt_sigprocmask、kill/tkill/tgkill、rt_sigsuspend 和 rt_sigreturn 使用受检用户复制。处理方式组内共享，掩码属于任务；KILL/STOP 不可屏蔽或捕获。停止/继续保留任务状态，进程组寻址支持终端前台组；默认 CHLD/URG/WINCH/CONT 不打断 I/O。

真实 rt_sigframe 包含 siginfo、ucontext、恢复入口、原掩码、GPR 和对齐 FXSAVE。返回时导入用户修改，检查选择子、MXCSR、最终指令和栈映射，净化 RFLAGS 后 IRETQ；嵌套处理保留红区。支持 SIGINFO/RESTORER/RESTART/NODEFER/RESETHAND。M9 没有备用栈、排队实时信号、定时器或通用线程取消，备用栈由 M12 增加。

管道、终端和 wait 可返回 EINTR，或依 SA_RESTART 重启原调用；已完成 writev 字节不撤销。futex/poll/ppoll/nanosleep 返回 EINTR，睡眠可输出剩余时间；ppoll/sigsuspend 经信号帧恢复原掩码。Linux 同步故障转成信号和默认整组终止，原生状态契约不变。

## 就绪状态与终端

poll/ppoll 查看真实描述符、EOF/HUP、错误和无效项，有限空等待可阻塞并超时，负 fd 忽略，最多 32 项。NONBLOCK 和 F_GETFL/F_SETFL 属于共享打开描述对象，零进度返回 EAGAIN，部分 writev 返回完成量。

轮询 COM1 与四组 PTY 使用有界行规程；中立事件仅在 Linux 适配器转成信号。`/dev/ptmx` 分配 master，TIOCGPTN 标识 `/dev/pts/0` 至 3，TIOCSPTLCK 控制 slave 锁，打开描述对象决定同伴生命周期，双向各有队列。支持规范输入、回显、擦除、EOF、VMIN=1/VTIME=0 原始模式、TCGETS/TCSETS、窗口大小和前台组。控制字符产生 INT/QUIT/TSTP，master 挂断产生 HUP/CONT；会话 leader 不带 O_NOCTTY 打开时可获取 slave。

这不是完整 ash 作业控制；不承诺 WUNTRACED/WCONTINUED、包模式、任意行规程、定时原始读取、后台 TTOU/TTIN 或完整会话管理。外设 IRQ 仍屏蔽，队列和行长度有界。

## 动态 musl 与内存

验证 ET_EXEC/ET_DYN、段长度和重叠、TLS 和有界绝对 PT_INTERP。主程序使用固定 PIE 偏移，真实 musl 解释器使用独立偏移，入口到解释器并提供主程序 PHDR/PHNUM/ENTRY 及正确 BASE。符号解析和重定位由用户态 musl 完成，不以预重定位镜像代替。

私有文件映射读取不可变 initramfs，不改变 fd 偏移；匿名映射清零，共享匿名后备通过引用计数跨 fork 保留。mprotect 实改 PTE，PROT_NONE 保留后备但拒绝 CPL3，W^X 拒绝同时写执行。历史 mmap 空间 32 MiB、每次最多 8 MiB，ELF 独立限制；MAP_FIXED 仅限该空间，不替换共享后备，保留已测 brk 保护例外。先完成分配再替换，失败回滚，无 ASLR/COW/持久文件回写。

fixture 包含 musl 1.2.4 静态、静态 PIE、动态程序和两个 TLS 共享库：一个启动链接，一个在线程已存在后 dlopen。缺库须真实报文件名，缺解释器事务式 exec 失败。不推断 glibc 或任意动态软件，也不安装伪 `/proc` 数据。

## 构建与验证

宿主沿 M8 需要 musl-tools、make、qemu-user。默认运行时为 `/usr/lib/x86_64-linux-musl/libc.so`，ANOVA_MUSL_LIBC 可选本地普通文件，但必须匹配锁定 Ubuntu musl 1.2.4-2 摘要。manifest 记录实际 ELF/运行时身份，宿主追踪和客户系统日志独立。保留 apps、trace-compat、test-compat、verify、terminal-test、persistence-test；普通 shell 可启动 linux-threads、linux-pie、linux-dynamic。UART 还须验证 EOF、Ctrl+C 和动态启动，横幅不足，负例/OOM 不进普通启动。

迁移的 musl 工具链必须保留共享 libc 链接输入、libc.a 和头文件。M10 构建检查两个 TLS DSO 的真实 libc.so DT_NEEDED，拒绝误从静态库拉入 TLS 访问器的环境。内核依据公开 UAPI 独立实现，没有复制 Linux 或 musl 实现代码。

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

## 精确来源与产物标识

- `6e8ff8ad02f4a3a0b70029577e2eb9df8f6526aea302a60a8d649874d6fa45ea`

## 补充规范链接

- [Linux AMD64 signal context](https://github.com/torvalds/linux/blob/master/arch/x86/include/uapi/asm/sigcontext.h)
- [Linux futex UAPI](https://github.com/torvalds/linux/blob/master/include/uapi/linux/futex.h)
- [musl 1.2.4 source](https://git.musl-libc.org/cgit/musl/tree/?h=v1.2.4)

## M17.2 有界终端扩展

单独准入的 M17.2 ash 增加真实 WUNTRACED/WNOHANG 停止子进程报告，以及按 PTY 进程族控制会话解析 `/dev/tty`。成功复制后消费一次停止事件，继续运行会清除过期停止状态。上文历史 M9 配置和证据不变；WCONTINUED、进程组等待及完整后台/多线程作业控制仍不在验收范围，见 [M17 PTY](M17_PTY.md)。
