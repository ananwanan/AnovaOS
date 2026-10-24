<!-- Translate the M4 USERSPACE contract and preserve exact commands and historical evidence. -->
# M4：隔离的原生应用

[English](../../en/milestones/M4_USERSPACE.md) | **简体中文**


本页定义历史 Ring 3、ELF 装载器、原生 v1 ABI 和 Rust/C SDK。[M4 验证](../validation/M4.md) 保存实测结果，[M3](M3_MEMORY.md) 定义后备内存所有权。[M5](M5_PROCESSES.md) 取代顺序执行生命周期，[M6](M6_FILESYSTEM.md) 取代仅控制台描述符和模块目录，增加 VFS、继承、管道、输入及 init/shell。历史证据保留。

## 执行与资源

M4 在单 BSP 上一次执行一个显式选择的原生应用。每次调用拥有私有 PML4、低半页表、清零用户页和独立 64 KiB 内核栈，栈两侧保护页不映射；只借用内核特权的高半页表，不复制内核低半根项。用户祖先设置 U/S，借用的内核映射在进程销毁前保持稳定。

`IRETQ` 使用 CS=0x33、SS=0x2b、IF=1、IOPL=0，初始通用寄存器清零。TSS.RSP0 指向本次内核栈，IST 保持独立；用户定时器中断返回同一上下文。此阶段没有调度或并发第二应用。

exit 或用户同步异常记录结果，恢复原 CR3 和暂停的内核栈，再释放私有页表、数据和内核栈。返回转移不跨越持有锁或拥有资源的 Rust 局部变量。NMI、双重故障和机器检查仍是致命内核异常。每次调用和部分加载失败都核对空闲物理页恢复。

## ELF 与启动布局

只接受小端 x86_64 ELF64 ET_EXEC。历史限制为文件 8 MiB、程序头 32 个、PT_LOAD 16 个、页取整装载总量 4 MiB，地址范围 `[0x10000, 0x40000000)`。检查头部、版本、大小、文件和虚拟范围、二次幂及同余对齐、filesz≤memsz、升序及页取整无重叠。要求可读，拒绝 W+X，入口必须位于文件支持的可执行字节中，清零后备页提供 BSS 和页尾填充。

拒绝 PT_DYNAMIC、PT_INTERP、PT_TLS、未知运行时程序头和可执行 GNU_STACK；NULL/NOTE/PHDR 及非执行 GNU_STACK 仅作为元数据。M4 不提供重定位、动态链接、TLS、PIE、libc 或 Linux 人格。

用户栈为 64 KiB、可写且 NX，结束于 `0x7fffffff0000`，两侧设保护页。RSP 对齐 16 字节，布局为 argc、argv 指针、NULL 和空 envp；历史 argc 为 1..16，每串最多 128 字节 UTF-8，不含内嵌 NUL，所有数据必须放入顶端 4 KiB 页，无 Linux auxv。后续容量见 [M12](M12_RUST.md) 与 [M13](M13_SDK.md)。

## 原生 v1 syscall

清单与 Limine 模块字符串明确选择 `anova.native.v1:<name>`，不根据 ELF OSABI 猜测。`xtask apps` 验证真实 ELF，生成带身份和 SHA-256 的 `build/apps/manifest.toml`，ISO 携带原文件，Limine 将其保留为模块。

真正执行 `SYSCALL`：RAX 为编号及有符号结果，RDI/RSI/RDX 为参数；RCX、R11 和条件标志可改变，其他通用寄存器保持。0 为 write，接受 fd 1/2 和最多 4096 字节；1 为 exit，记录 u32 状态；2 为 info，要求精确 32 字节可写输出；其他编号返回 -2。

write 先完整检查并复制到内核缓冲区，再轮询 COM1，拒绝时不打印部分用户数据；LF 转 CRLF，零长写不访问指针。info 按小端序写入：偏移 0 为 u32 ABI=1，4 为 u32 大小=32，8 为 u64 页大小=4096，16 为 u64 LAPIC 节拍，24 为 u64 PID，对齐 8 且不暴露填充。错误 -1/-2/-3/-4 分别表示地址错误、未支持调用、参数错误和写入过大，与 Linux errno 分离。头文件和断言在 `crates/abi`、`user/c/anova.h`。

## 入口、返回与复制

SYSCALL 不自动换栈。入口先将用户 RSP 存入永久内核暂存槽，立即换到可信栈，再 push 或 call。显式配置 STAR/LSTAR/EFER，FMASK 清除 IF/TF/DF/IOPL/NT/AC，构造与异常入口相同的帧，使用 `IRETQ` 返回。返回前验证可执行用户 RIP 和可写栈，固定选择子并净化标志；非法状态在内核栈终止进程。暂存状态仅限 IF=0 的 BSP，FS/GS 基址为零，禁用 FSGSBASE，不使用 SWAPGS，不能直接推广为 SMP 入口。

`UserAddr` 检查低半规范地址和算术溢出；复制逐页检查有效 U/S、读写权限，不将用户指针转换为 Rust 引用。唯一可恢复 REP MOVSB 故障点要求 CS、RIP、方向、CR2 范围和已启用保护状态全部匹配，并拒绝保留位、用户和取指错误；其他内核故障仍致命。真实访问故障可能已部分修改目标，调用者须处理。

检查和复制全程 IF=0，不跨阻塞或上下文切换。M5 继续以单线程空间独占和调度器根页表编辑保证后备页存活；共享空间、并发解除映射和 SMP 必须另加固定与同步协议。IRQ 和故障修复路径不获取内存管理器或堆锁。

## CPU 状态与 SDK

进入前要求 SYSCALL、FXSR、SSE、SSE2。CR0 启用 MP/NE、清 EM/TS，CR4 启用 OSFXSR/OSXMMEXCPT，关闭 XSAVE/AVX/FSGSBASE。新镜像清零 x87/MMX/XMM 状态，FCW=0x037f、MXCSR=0x1f80；M5 后切换完整保存恢复，内核仍无 SIMD。

独立 `user/` 工作区使用锁定 GNU Rust 目标的 core 进行普通 x86_64/SSE2 代码生成，程序为 `no_std`，使用自有 CRT、链接脚本和内存函数，不链接宿主 libc 或 Linux syscall。panic 经原生 exit 终止，不展开栈。C 使用 freestanding、静态、非 PIE、关闭栈保护的 GCC 配置，用户构建不得混入内核参数。

Rust 示例检查参数、BSS、FPU/x87/SSE2 并退出 0；C 示例检查参数、BSS/SSE2 并退出 7；独立 probe 负责负例及定时器。使用 `cargo xtask apps`、`check-apps`、`smoke`、`verify`。调度、文件、输入、持久盘、网络、通用 libc 和系统内编译器由后续阶段分别实现验收。
## 规范与来源链接

- [Intel SDM Volume 3A](https://cdrdv2-public.intel.com/874249/253668-090-sdm-vol-3a.pdf):
- [Intel instruction reference](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2b-manual.pdf):
- [System V gABI program loading](https://gabi.xinuos.com/elf/08-pheader.html):
