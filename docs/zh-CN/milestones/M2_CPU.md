<!-- Translate the M2 CPU contract and preserve exact commands and historical evidence. -->
# M2：BSP 异常与定时器

[English](../../en/milestones/M2_CPU.md) | **简体中文**


本页记录使用锁定 OVMF/Limine 输入、单 CPU 的 q35/TCG 早期启动实现。M2 不分配或回收物理页；[M3](M3_MEMORY.md) 后续接管内存所有权，[M5](M5_PROCESSES.md) 引入进程调度。实际结果见 [M2 验证](../validation/M2.md)，当前进度见 [STATUS](../guides/STATUS.md)。下述限制均属于历史 M2 范围。

## CPU 表与入口

GDT 包含空描述符、内核代码和数据描述符，以及占两槽的 64 位 TSS。TSS 大小为 104 字节，IDT 门大小为 16 字节；全部 256 个门都是进入时清除 IF 的 CPL0 中断门。I/O 位图起点位于 TSS 限长之外。M2 尚无用户入口；进入 Ring 3 前必须将 TSS.RSP0 改为相应进程的内核栈。

汇编入口统一处理有无硬件错误码的向量，保存 15 个通用寄存器，清除 DF，并按 System V AMD64 约定对齐 Rust 调用栈。176 字节的 C 布局帧包含向量、错误码及 RIP/CS/RFLAGS/RSP/SS；`iretq` 恢复原栈和标志。入口不使用不稳定 Rust 中断 ABI 或 FPU/SIMD。

| 向量 | 使用的栈 | 行为 |
| --- | --- | --- |
| 断点 3 | 当前 BSP 栈 | 有界诊断后返回，检查通用寄存器和 IF |
| 双重故障 8 | IST1 | 在独立栈输出致命诊断 |
| #UD/#GP/#PF，6/13/14 | IST2 | 致命诊断并解码适用错误位 |
| NMI/机器检查，2/18 | IST3 | 已安装处理器，未验证实际硬件投递 |
| LAPIC 定时器 64 | 当前 BSP 栈 | 原子增加节拍，仅发送一次 EOI |
| LAPIC 伪中断 255 | 当前 BSP 栈 | 直接返回，不发送 EOI |
| 其他向量 | 当前 BSP 栈 | 致命诊断，无设备驱动或恢复能力 |

每个 IST 有 16 KiB 保留栈和下方 4 KiB 未映射保护页，Rust 引用不能跨越保护页。故障记录包含 RIP、RSP、CS/SS、RFLAGS、全部通用寄存器、错误码、CR2、当前 IF 和实际 IST。IST2 内嵌套 #PF 可以升级为 IST1 上的 #DF；不声称 IST3 硬件故障或耗尽 IST1 后可恢复。

## 早期映射与保留区

Limine base revision 3 的 HHDM 覆盖可用内存、引导器可回收内存、内核、模块和 framebuffer，不覆盖 ACPI RAM/APIC MMIO。因此不能仅增加 HHDM 偏移来访问固件或设备。

`EarlyMapper` 只由 IF=0 的 BSP 使用，要求四级分页、关闭 PCID，并探测 NX/PAT。它复制 CR3 及修改路径的祖先；原页表的 ACPI/MMIO PML4 槽必须为空。永久保留池有 32 个对齐页，共 128 KiB。读取现有页表要求其内存仍保留且可经 HHDM 访问。编辑路径可以拆分 1 GiB/2 MiB 大页，保留物理地址并转换 PAT 位；每次编辑使对应虚拟地址失效。

| 地址范围 | 用途和属性 |
| --- | --- |
| `0xffffc00000000000` 加最多 1 MiB 及页内偏移 | 可重用 ACPI 读取窗口；只读、NX、普通固件 RAM |
| `0xffffc00000200000` 加 4 KiB | BSP LAPIC；可写、NX、UC |
| `0xffffc00000400000` 加最多四页 | I/O APIC；可写、NX、UC |
| 启动时报告的 IST 保护页 | 不建立映射 |

固件范围必须完整位于保留固件、ACPI 或引导器内存条目内，且结尾计算不能溢出。临时切片借用映射器，防止其存活期间重映射。设备页不能与 RAM、固件或 framebuffer 重叠，PAT 3 必须为 UC；设备地址来自检查后的 MADT，LAPIC 地址还须匹配 APIC-base MSR。移除保护页映射不会释放其物理后备页；原映射、启动数据和别名继续保留。M3 后续复制全部可达页表，但仍保留早期池和启动后备内存。

## ACPI、中断与时间

`anova-platform` 是供内核和宿主测试复用的 `no_std` 库，按字节解析，不创建指向 packed 结构的引用。RSDP 检查适用的两个校验和，优先使用 XSDT，必要时回退 RSDT。MADT/FADT 所用表均检查签名、长度和校验和。根表最多 128 项，单表最多 1 MiB，最多四个 I/O APIC；错误长度、零长记录、重复控制器或不支持的 x2APIC 记录明确失败，未知子表不解释。

BSP 检查 APIC 支持、ID、基址和 LVT 数量，屏蔽两个 8259 PIC 及所有 I/O APIC 重定向。屏蔽项仍配置有效内核向量和 BSP 目标，并读回屏蔽状态。M2 不启用来源覆盖、极性和触发模式、NMI 路由或设备 IRQ；这些必须随实际设备驱动独立验收。

FADT 必须提供四字节 System I/O PM timer，扩展地址优先，按标志处理 24/32 位回绕。LAPIC 使用 divide-by-16，以 3,579,545 Hz PM 计数器测量至少 20 ms。有限轮询预算用于发现计时源卡死，不能用循环次数代替时间。

周期为 10 ms，即 100 Hz。IRQ 只增加原子节拍并发送 EOI；BOOT:OK 前必须收到八次 IRQ、纳秒时间单调前进，且 PM 计数器测得间隔处于 20 ms–1 s 的宽松容差内。IF=0 检查后执行 `sti; hlt`，避免丢失唤醒。`monotonic_ns()` 等于已投递节拍乘 10 ms，溢出时饱和；它不是墙钟，也不补偿关闭中断期间丢失的节拍。

普通串口输出保存并清除 IF，输出后恢复；IRQ 不输出、不分配、不加锁、不睡眠。断点使用有界紧急输出并保留原标志返回，致命异常和 panic 不获取日志锁，诊断后关闭中断并停机。NMI 输出允许交错，避免死锁。M2 没有线程或 AP，后续必须另行定义上下文、锁和每 CPU 所有权。

## 故障门禁

原 M2 门禁启动七个独立镜像，M3 将当前 `cargo xtask verify` 扩展为 13 个。独立负例返回 1，QEMU 退出码 37 不算冒烟成功；必须匹配实际注入点、向量、错误码、栈和退出分类。#PF 的 CR2 必须等于保护页地址；#DF 必须在 IST1 上处理，保存的坏 RSP 必须匹配注入值。

双重故障镜像临时取消 #PF 的 IST，令 RSP 指向保护页并执行真实 push，在 #PF 投递期间再次栈故障后升级为 #DF；普通镜像仍用 IST2 处理 #PF。注入特性互斥，并且仅在测试镜像启用 QEMU 退出设备。

## 精确来源与产物标识

- `3a0526b700e356f0eac1b71a77697b3fd1c707a3`

<a id="primary-specifications"></a>

## 规范与来源链接

- [Intel SDM, Volume 3A, revision 090](https://cdrdv2-public.intel.com/874249/253668-090-sdm-vol-3a.pdf):
- [ACPI 6.6, software programming model](https://uefi.org/specs/ACPI/6.6/05_ACPI_Software_Programming_Model.html):
- [ACPI 6.6, hardware specification](https://uefi.org/specs/ACPI/6.6/04_ACPI_Hardware_Specification.html):
- [Pinned Limine protocol](https://github.com/Limine-Bootloader/limine-protocol/blob/3a0526b700e356f0eac1b71a77697b3fd1c707a3/PROTOCOL.md):
