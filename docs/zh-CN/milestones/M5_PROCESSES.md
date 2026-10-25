<!-- Translate the M5 PROCESSES contract and preserve exact commands and historical evidence. -->
# M5：原生进程与调度

[English](../../en/milestones/M5_PROCESSES.md) | **简体中文**


本页定义 BSP 调度、CPU/FPU 所有权和原生进程调用，实测见 [M5 验证](../validation/M5.md)。[M4](M4_USERSPACE.md) 的 ELF、入口和用户复制规则继续有效，顺序执行生命周期由 M5 替代。[M6](M6_FILESYSTEM.md) 后续替代仅控制台的描述符模型；[M9](M9_RUNTIME.md) 引入共享线程。下述单线程限制属于 M5 历史范围。

## 进程与执行模型

共有 32 个槽，包括尚未回收的僵尸进程，每进程一个线程，拥有私有低半页表、立即分配的用户页、带双侧保护页的 64 KiB 内核栈、保存上下文和控制台描述符表。会话内 PID 为单调正数，槽复用不复用 PID。启动运行器分别为历史隔离测试和多进程测试建立会话。

`anova-sched` 无分配地管理状态与父子策略，`kernel/task` 负责资源和激活。状态为 Empty、Ready、Running、Sleeping(deadline)、Waiting(child-or-any)、Zombie(status)。M5 无共享地址空间、线程创建、SMP 或内核线程 API。

调度器保留永久根页表和 BSP 栈。恢复前验证 RIP、用户栈、选择子和标志，更新 TSS.RSP0 与 SYSCALL 栈指针，恢复段、基址、512 字节对齐 FPU 状态、私有 CR3 和通用寄存器，再 IRETQ。syscall、同步用户故障或定时器将上下文复制到独立可信缓冲区，随后回到调度器根和栈处理阻塞、分配或销毁。尾部转移不丢弃带资源的 Rust 局部变量或锁；fork 不复制内核栈，活跃栈不能释放。致命内核故障沿独立 IST 诊断。

## 抢占与地址空间

保存 15 个通用寄存器、RIP/RSP/RFLAGS、选择子、DS/ES/FS/GS、FS/GS 基址及 x87/MMX/SSE2、XMM0–15、MXCSR。spawn/exec 清状态，FCW=0x037f、MXCSR=0x1f80；fork 继承捕获状态，子进程 RAX=0。FXSAVE64/FXRSTOR64 区域为 512 字节、16 字节对齐，清 EFER.FFXSR 避免 CPL0 省略 XMM。XSAVE/AVX/FSGSBASE 在实现额外状态前保持关闭。

100 Hz LAPIC 每个用户节拍只发一次 EOI，无分配、锁或日志地捕获上下文并轮转就绪队列。量子为一个已投递节拍，yield 也轮转；普通非阻塞 syscall 继续原进程，直到中断或阻塞。宿主策略测试检查持续就绪进程的公平性。

内核 IF=0，不可抢占，长内核路径可以延迟或合并节拍，因此不是墙钟或实时保证。sleep 使用同一节拍的检查后绝对截止时间，零表示 yield。无就绪任务时执行 STI/HLT，恢复后清 IF 再检查，避免丢失唤醒。

只有 BSP 调度器可修改用户页表。单线程进程独占地址空间，复制全程 IF=0，不跨调度；分配、exec、fork、销毁前恢复永久根。共享空间、并发撤销、可抢占内核或 SMP 必须新增固定与同步协议。内核栈地址为 `0xffffd00001000000 + slot * (64 KiB + 8 KiB)`，从下一页开始放 64 KiB 内容，上下页不映射。CR3 重载先失效再复用页和空表。调度状态无自旋锁；沿用内存管理器先于堆的锁顺序，运行用户或等待时不持锁。

## 原生 v1 增量

原编号 0–2 和错误含义保持。RAX 为编号和结果，RDI/RSI/RDX 为参数，RCX/R11 可破坏。

| 编号 | 调用 | 输入和结果 |
| --- | --- | --- |
| 3 | yield | 调度机会后返回 0 |
| 4 | sleep | 输入节拍，溢出返回 INVALID_ARGUMENT |
| 5 | spawn | 32 字节请求，返回子 PID |
| 6 | exec | 32 字节请求，成功替换镜像不返回 |
| 7 | wait | 子 PID，0 表示任意；16 字节输出，返回回收 PID |
| 8 | fork | 父返回子 PID，子返回 0 |
| 9 | close | 关闭描述符，返回 0 或 BAD_DESCRIPTOR |

ExecRequest 有四个小端 u64：名称指针与长度、单个参数指针与长度。名称 1..128 字节、参数 0..128 字节，均为无内嵌 NUL 的 UTF-8；成功后 argc=2，环境为空。M5 名称选择明确的 hello-rust、hello-c、probe、process-test 模块，不是文件路径；未知返回 NOT_FOUND，加载前检查并复制嵌套指针。

WaitStatus 为 u64 PID、u32 reason、u32 code，共 16 字节。reason 0/1/2 分别表示退出码、CPU 异常向量、非法返回状态；故障地址和错误另见串口。只能等待自己的子进程；无目标返回 NO_CHILD，错误输出指针返回 BAD_ADDRESS 且不消费僵尸状态。子退出唤醒匹配父进程，下一次调度完成复制。新增错误为 NO_CHILD=-5、NO_MEMORY=-6、NOT_FOUND=-7、PROCESS_LIMIT=-8、BAD_DESCRIPTOR=-9；原越域控制台 fd 仍是 INVALID_ARGUMENT。

## 生命周期与验证

spawn 完整加载后才发布。fork 立即复制全部用户页、W/X 权限、CPU/FPU 和描述符；部分失败回滚根、叶、表、栈与槽，父进程继续，未实现 COW。exec 先构建不活跃替代镜像，失败保留旧内存、上下文、身份和描述符；成功重置 BSS、参数与 CPU 状态，保留 PID、父、内核栈和描述符。M5 尚无 CLOEXEC。

描述符只有 stdout=1、stderr=2，无 stdin、文件偏移或打开文件对象；spawn 使用默认表，fork 复制，exec 保留，close/exit 清理自己的槽。exit 和用户故障立即回收不活跃镜像、页表、内核栈和描述符，仅留僵尸状态，由 wait 消费一次。父死亡后子进程交给内核 PID0，孤儿僵尸自动回收，包括收养时仍运行或睡眠的子进程；用户 init 属 M6。

`smoke` 在 M4 测试后运行真实独立进程 ELF，覆盖无 syscall 计算循环的通用寄存器与全部 XMM lane、fork FPU 控制、私有内存和描述符、exec 重置、各种 wait、子 #UD、睡眠/HLT、孤儿、槽耗尽与复用、三种部分分配 OOM。物理页和堆必须精确回到基线，分类器要求实际用例与计数。完整 `verify` 保留 13 个 CPU、内存和负例镜像。CPU 规则依据 Intel SDM 和 AMD64 APM Vol2 24593；未导入第三方调度器。

## 补充规范链接

- [Intel SDM](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)
- [AMD's official documentation](https://docs.amd.com/)
