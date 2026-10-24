<!-- Specify delivered software GUI/input/runtime SDK stages and the independently bounded GPU gate. -->
# M14：GUI 架构与五阶段交付

[English](../../en/milestones/M14_GUI.md) | **简体中文**

状态：**M14.1–M14.4 本地验证通过；M14.5 有界硬件 clear/readback 与 CPU fallback 本地验证通过**（2026-10-24）。实际记录分别为 [M14.1](../validation/M14.1.md)、[M14.2](../validation/M14.2.md)、[M14.3](../validation/M14.3.md)、[M14.4](../validation/M14.4.md)、[M14.5](../validation/M14.5.md)。本设计服从 [AGENTS](../../../AGENTS.md)、[架构](../ARCHITECTURE.md)、[路线图](../ROADMAP.md)和[状态](../guides/STATUS.md)；引导 framebuffer response 或宿主 mockup 本身不证明桌面可用。

## 1. 决策与范围

原生 GUI 用 **Rust** 实现，通过同一版本化语义/API 契约提供 **C、C++、Rust SDK**。

- 内核负责显示/输入驱动、安全映射、IPC、隔离，不实现 widget、桌面策略或内核窗口管理器。
- 具准入权限的**用户态** compositor/window 服务器管理 presentation、堆叠、焦点、输入、damage 和合成。
- 用户态 Rust 运行时管理绘图、字体、控件、布局、应用事件、高层窗口 API。
- 稳定语言中立二进制边界为 **Rust 实现的 C ABI**：C 声明、C++ move-safe RAII、Rust safe crate。内部优化必须保持可观察语义和协议兼容。
- 先 CPU/software，具备驱动、同步、用户接口与实测后才引入可选 hardware path，并保留 software fallback。
- native SDK 为一等接口。Linux GUI ABI/Wayland/X11/Qt/GTK 是独立 port；ELF、glibc、SDL、framebuffer 或本 SDK 均不暗示这些兼容性。

架构目标不承诺特定 graphics crate 或完整 GPU/Vulkan。

## 2. 信任与组件边界

```text
 Rust app         C++ app          C app
    |                |               |
 Rust SDK         C++ RAII       C headers/wrappers
    +------ Rust GUI runtime / stable C ABI ------+
                              |
             versioned GUI IPC + private copied surfaces
                              |
               user-space Anova compositor/server
                       |              |
               software renderer   optional GPU backend
                       |              |
               kernel IPC / display / input drivers
                              |
                     framebuffer / device
```

C ABI 是用户库边界，不是内核 syscall ABI。应用不能取得无限制物理 framebuffer 或其他进程映射。GUI 对象由版本化 IPC/compositor capability handles 支撑；内核保留校验、页所有权、复制/映射和隔离。

协议与实现分别文档化：framing/version、surface 所有权、每客户端配额、geometry/damage、format/stride、submit/present、事件、destroy/reconnect/failure。使用前检查每 length/count/offset/pointer/generation 及 `width × height × bytes-per-pixel`，不能依赖“可信客户端”“单线程”或永不退出的服务器。

缓冲区 lifetime 须说明 allocation、可读/写/映射者、present 中 sealing/ownership transfer、completion/fence、close/crash 回收、restart 行为。已交付配置使用复制私有 surface；CPU path 独立于 GPU。

## 3. SDK/ABI 契约

下列架构契约由 [M14 运行时](M14_RUNTIME.md)和 [SDK](../../../sdk/gui/README.zh-CN.md)给出实际 v1 选择：

1. 独立 `ANOVA_GUI_ABI_VERSION` 与 capability negotiation，明确不支持版本，区别 native syscall/Linux personality。
2. `extern "C"`、固定 `#[repr(C)]`、显式整数宽度/对齐/padding/calling convention，不传 Rust String/Vec/trait object/unwinding/internal borrow/unstable 布局。
3. UTF-8 指针+字节 length；输入借用仅限调用，输出缓冲区携带容量/已写长度，或由显式释放函数管理所有权，不暗含 allocator 耦合。
4. opaque app/window/surface 生命周期、所有权、double destroy/stale 明确；不能以原始 address 跨进程授予权限。
5. 明确返回错误；Rust panic/C++ exception 不越过 C ABI，用户失败不 panic 内核。
6. 定义事件 loop、callback、UI-thread affinity、reentry、ordering、cancel、overflow/backpressure/thread-safety；v1 实际无 callbacks。
7. C++ RAII/move 和 exception/no-exception 策略明确，Rust types/lifetimes/Result，各语言行为一致。
8. `sdk/gui/generate.py` 确定性头文件生成、library packaging、symbol/export 版本、兼容测试、分离 SDK/toolchain 目标。

已实现 `anova.gui.v1` 导出 `anova_gui_query_version`、`anova_gui_window_create`、`anova_gui_window_show`、`anova_gui_poll_event`、`anova_gui_window_destroy`。allocation-free v1 每进程一个窗口并轮询，无 app 对象、callback 或 public 共享映射。

验收须独立构建/运行 C/C++/Rust 客户端，并覆盖版本、malformed parameters、stale handles、资源失败和退出；一个语言产物不能证明另两种。

## 4. 实际布局与独立扩展

```text
crates/graphics/              # Checked software graphics and VirGL encoding
crates/input/                 # Physical PS/2 decoding, reset and bounded queues
crates/gui/                   # Wire/transport and compositor policy
crates/gui-runtime/           # Rust raster/widgets, C ABI and Rust wrappers
user/compositor/              # Isolated window server
sdk/gui/                      # Generated C header and move-only C++ wrapper
tests/guest/gui/              # Independent language/ABI/fault fixtures
docs/en/validation/M14*.md    # Actual stage evidence
docs/zh-CN/validation/M14*.md # Chinese evidence translations
```

完整桌面 shell、更丰富字体/renderer 属于后续扩展，不创建空目录或 dummy 实现。新增组件须有可运行实现和门禁。

未来可评估 `embedded-graphics`、`tiny-skia`、`fontdue`，当前并非依赖。选型前审查准确版本、许可、target/allocator/no_std、成本、安全、guest 可重现性；font/rasterizer/layout 留在用户态。

## 5. 五个独立验收阶段

### M14.1 — 显示与软件 framebuffer

依赖：Limine response、owned paging、安全 device map、可重复 QEMU video 配置。

- [x] 检查模式、格式、pitch/stride、尺寸/span，拒绝不支持格式，不假设 RGBA。
- [x] 有界 pixel/rectangle/blit、裁剪、back 缓冲区，backend 支持时才提供同步 presentation。
- [x] 确定性启动图案、保留 COM1/failure 的 text fallback。
- [x] 显存不进普通 RAM allocator，记录 cache/packing/ownership。

门禁：真实 QEMU 裁剪图案、known pixels/checksum、越界/format 负例、timeout/panic 拒绝。明确 mode/device 上限；framebuffer 不代表窗口 server/GPU。

本地 standard VGA 800×600×32：480,000 pixels 和固定 checksum 全符，显存回读、heap 回收、no-display fallback、serial recovery/clean 关机均通过；24/32 RGB/BGR 和裁剪另由宿主测试覆盖，无 vblank/page-flip。见 [M14.1](../validation/M14.1.md)。

### M14.2 — 事件与光标

依赖 M14.1 与选定 controller/poll/IRQ 契约。

- [x] QEMU i8042 native set-1 keyboard、ID-0 三字节相对鼠标，规范 key/button/nine-bit motion。
- [x] 有界队列交给一个诊断消费者，裁剪光标；物理键与文字分离，focus 留 M14.3。
- [x] 暂用 scheduler 有界轮询，IRQ1/12 屏蔽；routing/EOI/lifetime 独立验证。
- [x] overflow/malformed/detach 重置 held/history，无 display/controller 保留 serial。

门禁：注入真实 key/button/motion，有序事件/光标，bounds/overflow/disconnect；QEMU PS/2 不等于 USB/实机。三次启动 input-test 已过，范围见 [M14.2](../validation/M14.2.md)。

### M14.3 — 用户合成器与窗口

依赖 M14.1/.2、已验 IPC/usercopy。首版用复制私有 surface，无共享用户 map。见 [compositor](M14_COMPOSITOR.md)。

- [x] 独立进程、认证连接、surface quota/ownership/cleanup。
- [x] damage 软件合成、stack/move/resize/focus/visibility/input；内核无 widget policy。
- [x] 缓冲区 ownership/completion、遮挡重画、restart。
- [x] client/server fault 后保留 COM1 恢复。

门禁：至少两独立进程重叠可移动窗口、正确 focus/redraw/input；杀客户端不伤其他；bad surface/stale/OOM/server restart 有实测。

### M14.4 — 运行时与三语言 SDK

依赖 M14.3/M13 packaging 和各语言真实 guest toolchains（M11 C/C++、M12 Rust）。

- [x] Rust primitives/glyph/text/layout/button/textfield/event/window。
- [x] Rust C ABI、生成头文件、errors/handles/ownership/compatibility。
- [x] C header/library、C++ RAII、safe Rust、templates 和 ABI link/install 元数据。
- [x] 基本图形终端、clipboard policy、native GUI app，明列后置。
- [x] 三语言独立 guest compile/link/run；宿主 cross 构建仅初步证据。

门禁：三客户端独立 open/event/render/close，lifecycle、UTF-8、布局/无 callback ABI、bad version/user fault/reclaim，实际 edit/build/run。

### M14.5 — 可选硬件绘制

依赖 software desktop、PCI/DMA 所有权、coherent backing、选定 device/userspace interface。接受私有 copied 输出，无 public 共享 map。

- [x] 评估 virtio-gpu，区别 2D scanout/真实 rendering，保留 M14.1 fallback。
- [x] 明确 queue/buffer/cache/fence/reset/isolation。
- [x] 实现至少一个有实证 GPU drawing/composition path。
- [x] deterministic 输出对 CPU 参考、reset/failure/recovery。
- [x] 记录版本/限制/platform/测量，不无证宣称 GL/Vulkan。

门禁：证明 GPU-backed、检查 presentation/recovery/CPU fallback；仅 framebuffer/scanout 不满足，无真实 path 则不完成。

受验 [GPU 配置](M14_GPU.md) 用 secondary device 的 RTX 4060 Ti D3D12 backend 执行真实 VirGL clear/fenced readback，由已认证 compositor 选择 fallback、CPU 复制至 VGA。验证 7×5 非平凡颜色、四 64×64 colors、lost-doorbell timeout/reset/reinit、2D-only/absent/full scanout/serial 恢复。普通 composition/display 复制仍 CPU；shader、guest GL/Vulkan、broad hardware、zero-copy/speedup 均不涵盖。reset-refusal quarantine 实现/审查但未实际诱发，见 [M14.5](../validation/M14.5.md)。

## 6. 依赖、关联工作与排除

[ROADMAP](../ROADMAP.md)/[STATUS](../guides/STATUS.md) 独立跟踪每门禁，后阶段不追认前阶段。真实执行才记录 QEMU config、binary hashes/version、标记、负例、exit/timeout 和 non-QEMU hardware 范围。

SMP、进一步 ACPI/power、NVMe/AHCI、USB/xHCI、物理 GPU、glibc、Wayland/X11 独立开发，不默纳入 M14。导出 C ABI 不等于 Linux GUI，接启动 framebuffer 不等于 GPU。保留已验 serial/process/VFS/toolchain/negative 门禁；真实 `graphics-test`、`input-test`、`gui-test`、`gui-sdk-test`、`gpu-test`、`gpu-fallback-test` 按各配置执行，新增命令须带工作检查与证据。

## 7. M14.1 实际显示契约

`crates/graphics` 为无依赖 no_std、无 map/alloc。`Layout` 检查 width/height/pixel count/pitch/length 和不重叠 8-bit RGB masks，再 little-endian 打包 24/32-bit。每 axis 4,096、共 2,097,152 pixels、RGB back 缓冲区 8 MiB、含 padding 16 MiB。`Surface` 借 caller 存储，signed origins 裁剪；blit 写前检查全部 source/stride；present 仅 visible 字节，保留 padding。

可选 `graphics` 拥有一次启动诊断。`boot::display` 仅主 RGB framebuffer，验 visible span 属 reservation，page-rounded map 不重叠 allocator/boot/module/firmware。Limine 最终部分 page 的 tail 永不访问。`mm::check_display` 验每页物理 identity、supervisor RW/NX、PAT index 5，PAT5 必须 WC。保留原映射，无新 alias/PAT/MTRR 改动。仅 USABLE 进 allocator，显存不分配/释放/交 app/入 heap。

遵循锁定 [Limine cache/HHDM 契约](https://github.com/Limine-Bootloader/limine-protocol/blob/3a0526b700e356f0eac1b71a77697b3fd1c707a3/PROTOCOL.md)。heap fallible reserve back 缓冲区、画确定图案、volatile 写、MFENCE、逐可见字节回读，才报 `[GRAPHICS:PRESENT]`。user 启动初期前释放并核 heap 基线 `[GRAPHICS:RECLAIM]`，出错 `[GRAPHICS:FALLBACK]` 保留 COM1。初显示仅 BSP 启动；M14.2 仅永久 root 的 BSP scheduler 访问，无 IRQ/app 权限。

`graphics-image`/`graphics-test`/`run --graphics` 已实现。宿主门禁经真实 QMP PPM 逐 pixel/checksum 比较，serial/ACPI 恢复；`-vga none` 独立启动同样恢复。宿主 JSON 仅 QMP，不入 kernel/guest graphics library。

CPU 复制无 vblank/tear-free/page-flip；硬件只验 800×600×32，其他 packing 仅宿主。M14.1 诊断自身无 GPU/text terminal/compositor/syscall；M14.3 transport、M14.4 library 独立交付。M14.2 暂诊断输入/光标，已有 M14.3 绑定时交还 presentation，不加 window/focus/font/widget 策略。

## 8. M14.2 实际 PS/2 契约

### 设备与调度所有权

`graphics` 选择 q35 i8042、translation disabled 的 native set-1 keyboard、ID-0 三键三字节 mouse。`kernel/src/drivers/input.rs` 拥有 0x60/0x64、controller/interface 测试、reply/enable。每命令 argument 独立 ACK；AUX/status 分开源。初始化 ACPI PM timer 100 ms deadline+attempt 上限、有界 flush、最多三 send；missing/unsupported 保留 COM1。

暂 scheduler 轮询：BSP IF=0/permanent root，每 pass 最多 64 bytes/64 事件，IRQ1/12 屏蔽，IRQ 不 alloc/log/access state。每设备独立 last-byte PM timestamp；部分 prefix/packet 超 100 ms 在消费 late 字节前失效，即使另一设备活跃。PM wrap/source-independent expiry 有 image 检查。

独立参考 [QEMU 8.2.2 i8042](https://github.com/qemu/qemu/blob/v8.2.2/hw/input/pckbd.c)、[PS/2 model](https://github.com/qemu/qemu/blob/v8.2.2/hw/input/ps2.c)、[Microsoft packet 格式](https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/keyboard-and-mouse-hid-client-drivers)，未复制 upstream decoder。

### 物理事件、边界与恢复

`crates/input` 无依赖 no_std、无 port I/O/allocator，提供 press/release/repeat、extended key/modifiers/pointer/buttons。PrintScreen fake shifts 不改按住的 Shift；Alt+PrintScreen/SysRq 同物理 key；Pause 仅 press，无 break/持续按住的。key number 是诊断 ID，非字符/冻结 ABI；此阶段不提供 layout/Unicode/IME/shell injection。

mouse 结合包头的符号位与无符号数据字节，保留完整九位 `-256..=255` 位移范围，dispatcher 把向上 wire Y 转向下 screen Y。三 button 状态/变化明确。overflow 丢双轴 motion 但保留 buttons，bad header/transport invalidates。仅在寻找包头时使用 bit 3，data 也可含；任意丢字节不能全检测，依赖 controller error/partial expiry。

64-entry queue 满时丢 history 和触发事件、清 decoder/held、发布一个重置再收后续。malformed/error 同样；consumer detach 丢 history/state 并忽略字节，reconnect 先重置。指针 position 保留/裁剪，重置松开全部按钮；consumer 须清按住的 keys/buttons。

本阶段唯一目标为内核诊断，记录事件/画光标，无 user connection/focus/app routing；M14.3 单独实现。测试 consumer detach/reconnect 不证明物理 hotplug。

### 光标与本地门禁

7×9 cursor 保存 63 device pixels（含 unused channel）的原字节，先恢复旧背景，再存新裁剪足迹，画黑白箭头。所有坐标/offset 经检查的，WC writes fenced。input/queue/cursor 不 alloc；M14.1 full-screen temp 仍释放。首次指针前图案不变，保留 `graphics-test`；重置隐藏/恢复背景。

`input-test` 独立 `graphics,input-test` image，三个 60 s watchdog 启动：standard VGA、`-vga none`、`-machine i8042=off`。QMP 注入 ordered key/repeat/extended/modifiers/motion/三 buttons，显示启动在 center/四 corner 逐比较 480,000 pixels，验 clipping/old footprint。无显示仍测 input/serial；每启动须 COM1 recovery/clean ACPI 关机，意外 healthy 重置、fatal/early exit/wrong pixels/timeout 均失败。

测试 image 在 READY 前独立测 overflow/malformed/detach/reconnect/stale 源码 expiry/真实 port disable-reinit，不代表 general removal/replug。hash/范围见 [M14.2](../validation/M14.2.md)，remote CI 另记；CI 运行并留 `build/input/`。

接受仅 QEMU PS/2/software cursor，USB/hotplug/IRQ routing/real hardware/text terminal/compositor/public GUI ABI/SDK 不属于 M14.2。M14.3/.4/.5 已各在 [compositor](M14_COMPOSITOR.md)/[运行时](M14_RUNTIME.md)/[GPU](M14_GPU.md) 及对应验收独立通过。

## 9. M14.4 运行时实现

Rust runtime/生成 C ABI/三语言 SDK 在 `crates/gui-runtime`、`sdk/gui`。Rust note 和有界 interactive terminal 与独立 demos 用同运行时。guest GCC/G++/Rust 生成 native 输出，不证明 native libc/std。准确 API/ownership/范围见 [运行时](M14_RUNTIME.md)、[M14.4 记录](../validation/M14.4.md)。M14.5 独立有界 clear/readback 不加速普通桌面。
