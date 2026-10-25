<!-- Define bounded VirGL rendering, compositor admission, coherent DMA ownership and CPU fallback. -->
# M14.5：有界离屏 GPU 绘制

[English](../../en/milestones/M14_GPU.md) | **简体中文**

本配置实现可选 modern virtio-gpu control queue 和仅合成器可用的离屏 clear/readback，独立于[软件桌面](M14_COMPOSITOR.md)与 [GUI SDK](M14_RUNTIME.md)。硬件/回归结果见 [M14.5 验收](../validation/M14.5.md)。

## 渲染与显示

guest 用 Rust 独立实现 modern PCI transport 及所需 VirtIO/VirGL wire 子集，协商 `VERSION_1`/`VIRGL`，验证 capset 2 与 BGRA render-target，创建 context/2D render-target 资源，附加四个自有 backing 页，通过 `SUBMIT_3D` 提交 VirGL surface、framebuffer binding 和 `CLEAR`。带 fence 的 `TRANSFER_FROM_HOST_3D` 将宿主渲染结果复制到 guest DMA；结果不由 `TRANSFER_TO_HOST_2D` 或预制参考图提供。

读回前每个 backing pixel 填入不同 poison，驱动与合成器将 RGB 结果同独立 CPU 颜色参考比较，驱动还验证不透明 alpha。UNORM 通道用整数编码，内核不执行浮点；宿主测试把每种 8 位通道同独立 IEEE-754 计算比较。

第二块 GPU 离屏绘制；既有 standard-VGA/Limine scanout 保留为恢复显示，合成器用复制式 `PRESENT` 显示读回像素。成果是 GPU render-target 清屏/读回再由 CPU 复制显示，不加速普通窗口合成，不代表 shader 绘制、OpenGL/Vulkan API、guest Mesa、共享 scanout、page flip 或提速。

## 准入与所有权

实验性 `SYS_GUI` 增加 `GPU_CLEAR = 12`，仅已认证并绑定的合成器可用，不改变冻结 native syscall v1 或 GUI C ABI。无 padding 的 24 字节 `GpuClear` 请求：

| 字段 | 契约 |
| --- | --- |
| `pixels: u64` | 独占可写、包含 `width * height` 个 packed RGB u32 像素的输出 |
| `width, height: u32` | 各为 `1..=64`，行紧密排列 |
| `rgb: u32` | `0x00RRGGBB`，拒绝高位非零 |
| `reserved: u32` | 必须为零 |

设备访问前检查准确大小、权限、几何、格式及完整输出可写范围。请求先复制，在内核 DMA 渲染，私有 staging 保存初始化像素，再经检查的 usercopy。客户端指针/context/resource ID/命令流不进入宿主。复制失败不保留用户地址；调度期间不悬置分配或地址空间引用。

单 BSP IF=0 串行执行。设备有独立有界 supervisor UC/NX PCI 窗口、一个 WB-coherent DMA32 queue 页和四 backing 页。八槽 split queue 同时仅一个双 descriptor 请求。每命令唯一 fence；完成检查 used index、descriptor ID、长度、预期响应、fence flag/identity、context、padding。内存屏障约束发布/读取，读取 capability-count 前后都采样配置世代号，确认配置未变化，IRQ 保持屏蔽。

每调用一个临时设备 session；确认重置后才释放 queue/backing，错误走同一清理。重置未获确认时保留全部暴露 DMA 页、禁用 bus mastering 并永久禁用后续 GPU 调用。这是隔离策略；成功重置测试不证明拒绝重置的设备可恢复。

## 恢复与门禁

`gpu` 启用生产操作；`gpu-test` 另启用仅服务器标量 `GPU_TEST_TIMEOUT = 13` 和非法 capset 探测。超时注入发布真实请求但丢弃 notification，要求校准 watchdog 到期，再重置真实设备。生产内核拒绝注入；PM 时间和迭代上限共同防止无限等待。

显式 `compositor gpu` / `compositor gpu-fail` 比较读回并由用户态选择 CPU fallback；普通桌面继续软件渲染。

```sh
cargo xtask gpu-test           # Requires an actual hardware GL backend
cargo xtask gpu-fallback-test  # Headless 2D-only and absent-device CPU paths
```

完整门禁检查这块 QEMU 设备的 capset renderer 字符串，忽略大小写拒绝 software renderer，并要求可识别硬件身份。独立宿主 GL probe 不证明设备 backend。`build/gpu/` 保存命令、输入哈希、包版本、serial/QMP 日志、渲染/读回时间、独立全屏比较和退出结果。headless 门禁适用于普通 CI，不能代替硬件验收；缺硬件证据时完整门禁失败。

WSLg 环境存在 `/dev/dxg` 且未指定驱动时，仅为 QEMU 子进程选择 Gallium `d3d12`；`GDK_BACKEND`、`GALLIUM_DRIVER`、`ANOVA_GPU_DISPLAY` 可选其他环境。协议可工作也不意味着 software backend 满足硬件验收。

## 协议一手参考

- [VirtIO 1.2](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)：GPU control queue、3D 命令、fence 和 modern PCI。
- [QEMU backend 区分](https://www.qemu.org/docs/master/system/devices/virtio/virtio-gpu.html)与 [QEMU 8.2.2 VirGL transport](https://gitlab.com/qemu-project/qemu/-/blob/v8.2.2/hw/display/virtio-gpu-virgl.c)。
- VirGL [协议](https://gitlab.freedesktop.org/virgl/virglrenderer/-/blob/1.0.1/src/virgl_protocol.h)、[capability 布局](https://gitlab.freedesktop.org/virgl/virglrenderer/-/blob/1.0.1/src/virgl_hw.h)及[宿主 clear/readback 实现](https://gitlab.freedesktop.org/virgl/virglrenderer/-/blob/1.0.1/src/vrend_renderer.c)。

这些资料用于独立编码协议；guest 没有 vendored 上游 renderer、Linux GPU 驱动或 Mesa 运行时。
