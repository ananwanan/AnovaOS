<!-- Translate the M3 MEMORY contract and preserve exact commands and historical evidence. -->
# M3：内存所有权与分配

[English](../../en/milestones/M3_MEMORY.md) | **简体中文**


本页记录单 BSP、四级分页的原始 M3 实现，包括物理页分配释放、完整自有页表、带保护页的执行栈和可回收内核堆。[M11](M11_TOOLCHAIN.md) 后续替代堆地址与容量、调度器和用户栈上限；保留区、所有权、保护页及回滚规则继续有效。结果见 [M3 验证](../validation/M3.md)，异常和 IRQ 契约见 [M2](M2_CPU.md)。

## 物理页所有权

只有 Limine `USABLE` 范围进入 `FrameAllocator`。启动检查拒绝可用 RAM 与其他条目重叠。内核、模块、早期 32 页表池、IST 及保护页后备内存、ACPI、framebuffer、固件、坏内存、原始启动页表与栈、协议响应存储均不参与分配。引导器可回收内存仍有活跃响应引用，因此不回收。

最多接受 128 个排序且页对齐的范围，检查地址和长度溢出及 CPU 物理地址宽度。每 4 KiB 页使用一个状态字节；元数据放在原本可用且已映射的内存中，在首次分配前排除其完整页取整范围。耗尽或无法表示的内存图明确失败，不能截断。

状态包括 Free、Reserved、PageTable、Stack、Heap 和 Test。Reserved 包含元数据及存在时的物理页零；页表只允许页表后端编辑；栈和固定堆的后备页永久固定；Test 只用于独立验证。可用范围之外永远不能分配。分配返回 4 KiB `PhysAddr`，释放前检查对齐、范围、保留状态、已分配状态和期望所有者，拒绝重复释放、外来页、保留页或错误所有者。调用者必须先解除映射；底层接口是内存管理器私有机制，不是公开 capability 或代际句柄 API。

`Zone::Dma32` 只限制整页位于 4 GiB 以下，不代表实现 DMA 驱动、连续分配、IOMMU 或设备一致性协议。MMIO 不属于可分配 RAM。

## 页表、权限与缓存

ACPI/APIC 发现后，M3 消耗 `EarlyMapper` 并复制所有活跃非叶页表，包括未修改的路径。新根不再引用 Limine 或早期池页表；导入的叶后备内存仍保留。切换 CR3 后遍历核对所有可达表页的所有者与数量。

`anova-memory` 提供导入、查询、4 KiB 映射、保护、解除映射和空表回收。内核 `TableStore` 验证自有表页，使用对齐 volatile 访问并提供失效操作。失败时释放部分创建的私有链，全部分配成功后才发布路径；解除映射后先刷新 TLB 和分页结构缓存，再释放表和后备页。

导入的 1 GiB/2 MiB 叶保留并可查询 PAT；动态修改大页返回 `HugePage`，M3 没有通用拆分器或用户地址空间。有效祖先权限也参与检查。激活时保持四级分页，关闭 PGE 和 PCID，启用 CR0.WP，保留已探测 NXE；单核 CR3 重载完成本地失效，不代表 SMP TLB shootdown。新映射仅供内核，拒绝 W+X；全部 HHDM 根槽及其大页均为 NX，避免直接别名执行堆或栈。

| 虚拟范围 | 原始 M3 属性 |
| --- | --- |
| `0xffffc00000000000` 以下 HHDM | 内核特权、NX，保留引导缓存属性 |
| `0xffffc00000000000` | M2 固件只读 NX 窗口 |
| `0xffffc00000200000` / `0xffffc00000400000` | LAPIC/I/O APIC；可写、NX、UC，PAT 3 |
| `0xffffd00000010000` / `0xffffd00000021000` | 普通栈下方和上方保护页 |
| `0xffffd00000011000..0xffffd00000021000` | 自有 64 KiB 栈，可写、NX、WB |
| `0xffffd00000200000..0xffffd00000400000` | 自有 2 MiB 伙伴分配堆，可写、NX、WB |
| `0xffffd00000800000` 起 | 每次验证后回收的临时映射 |
| `0xffffd08000000000` | 部分映射 OOM 回滚测试使用的空槽 |
| 内核 ELF 和三个 IST | 原保留区、ELF RX/R/RW 和既有保护页 |

PAT 0 必须为 WB，PAT 3 必须为 UC。查询暴露实际 PAT，设备映射沿用 M2 重叠检查，不创建改变缓存属性的别名。内核代码虚拟映射为 RX，HHDM 别名为 NX，但特权数据别名仍可写；不声称物理页整体不可变、用户隔离或完整地址空间销毁。

## 栈、堆与锁

BSP 仅一次经稳定 naked 汇编边界转移到 64 KiB 自有栈，保持 System V 对齐、清除 RBP，不返回旧栈。边界只传递复制后的定时器参数，更新 TSS.RSP0，保持三个 IST 独立，并检查真实 RSP 和两侧未映射保护页。

固定 2 MiB 堆采用侵入式伙伴分配，最小块 16 字节。布局向合适的二次幂取整，分裂和合并不分配元数据；`GlobalAlloc` 提供稳定 Rust `Box/Vec/String`。堆不扩缩，也不向物理分配器返还后备页，允许预期的内部碎片。OOM 返回 null 并输出有界、无分配诊断；`Vec::try_reserve_exact` 返回错误且保留原对象。不可失败分配进入 Rust 1.90 默认 `no_std` 分配错误处理和无锁 panic，不使用 nightly 属性，也不返回重叠的活跃块。

内存管理器与堆锁先保存并清除 IF，释放后恢复，不能睡眠；锁顺序为管理器再堆，固定堆不反调管理器。IRQ/NMI/异常/panic 不获取这些锁或分配内存；持锁发生保护故障时也必须完成独立诊断。M5 后用户可抢占，内核及锁区仍不可抢占，SMP 未实现。

## 验证边界

普通启动检查栈、表所有者、CPU 控制位、映射和缓存权限；真实内存耗尽、重复映射及容器压力只在 `qemu-test` 中运行。宿主覆盖保留区、DMA32、碎片合并、活跃内容、导入所有权、每层 OOM、大页、W^X、祖先权限和回收。QEMU 实际触碰内存，耗尽并恢复全部空闲页，验证回滚，触发保护页、只读写、普通及 HHDM 的 NX 取指、解除映射后访问故障。单独不可失败 OOM 镜像必须进入分配 panic，负例不能计为冒烟成功。
## 精确来源与产物标识

- `3a0526b700e356f0eac1b71a77697b3fd1c707a3`

## 规范与来源链接

- [Intel SDM Volume 3A](https://cdrdv2-public.intel.com/874249/253668-090-sdm-vol-3a.pdf),
- [Pinned Limine protocol](https://github.com/Limine-Bootloader/limine-protocol/blob/3a0526b700e356f0eac1b71a77697b3fd1c707a3/PROTOCOL.md),
- [Rust GlobalAlloc](https://doc.rust-lang.org/core/alloc/trait.GlobalAlloc.html)

## 补充规范链接

- [handle_alloc_error](https://doc.rust-lang.org/alloc/alloc/fn.handle_alloc_error.html)
