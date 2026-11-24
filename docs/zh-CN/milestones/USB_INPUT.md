<!-- Define USB host, enumeration, HID and desktop responsibilities and their independent gates. -->
# USB 输入开发

[English](../../en/milestones/USB_INPUT.md) | **简体中文**

首个目标是启动前直连 q35 xHCI 根端口的键盘和相对鼠标。开发分支为从 `dev` 创建的
`feat/usb-hid-input`。本任务扩展 M14 输入与 M17 桌面，可独立于 M18 编译器自举验收。
见[测试指南与证据](../validation/USB_INPUT.md)。

## 所有权与事件流

| 层 | 所有者与接口 | 向上提供的数据 |
| --- | --- | --- |
| 主机控制器 | `kernel/src/drivers/xhci/` 持有 PCI 功能、UC 寄存器映射、环、DMA 页、slot 和上下文 | 含实际长度、端口与端点标识的已校验完成事件 |
| USB Core | 管理设备、配置、接口、端点及枚举状态，通过主机控制器提交 setup 和 interrupt IN | 接口描述符、类控制请求结果和有界报告数据 |
| HID 类驱动（规划） | 每个活动接口拥有独立协议、上一份报告和按 ID 划分的布局 | 物理键变化、相对移动、按钮变化和有符号滚轮增量 |
| 输入与桌面 | 扩展 `anova-input::Dispatcher`；内核在调度点轮询，合成器按焦点将 GUI 硬件事件发给窗口或 PTY | 来源无关的键鼠事件，沿用消费端的布局与文本处理 |

计划链路始于设备完成 interrupt IN。调度器每轮限量取出事件 TRB，校验 slot、端点、在途 TRB 标识、
完成码和剩余长度，只把实际完成的字节交给绑定接口。HID 生成统一事件后重新提交接收。
桌面刷新不等待命令完成。PS/2 解码器继续作为输入来源。每设备独立记录键和按钮状态，合并后再发布变化，
以免一把键盘松开 Shift 影响另一把键盘。沿用光标裁剪、队列溢出复位、布局和重复策略。
滚轮验收前应在实验性传输接口及消费端定义兼容事件，保留冻结的公共 ABI。

## 分阶段实施与退出条件

| 阶段 | 实现与验收 | 当前状态 |
| --- | --- | --- |
| USB.1 | 识别 PCI 0c/03/30，校验 MMIO，接管并复位，管理 DCBAA、命令环、事件环、ERST、slot 与端点。验证完成事件、回绕、slot 释放、超时及无控制器启动。 | 控制器／命令环及实际 EP0 失败清理已实现；有界 QEMU 证据与实机／IRQ 验收分开。 |
| USB.2 | 按协议和速度复位各直连端口；Enable Slot、准备 EP0 上下文、Address Device、读取设备描述符前缀并更新 EP0 最大包长，再读取完整设备和配置树。配置中断端点并执行 Set Configuration，按接口绑定。 | 已实现上下文、Address Device、描述符、EP0 包长评估、端点配置及接口记录；按有界 QEMU 范围验收。 |
| USB.3 | 匹配 HID 03/01/01，设置 Boot Protocol，接收 interrupt IN。差分八字节键盘报告、修饰键和多键状态。rollover 或截断时保留原状态。在图形 PTY 验证字母、数字、编辑键、方向键、功能键、Shift/Ctrl/Alt、保持和释放。 | 已实现并在 Boot QEMU 桌面会话验证；非法报告保留按键状态。 |
| USB.4 | 匹配 HID 03/01/02，设置 Boot Protocol。从启动协议前缀读取三键和有符号相对 X/Y。验证光标边界、左右键窗口操作、中键按放、连续移动与 PS/2 共存。 | 已实现并在 Boot QEMU 会话验证裁剪、三键、焦点、真实标题拖动及 PS/2 共存。 |
| USB.5 | 读取 HID 描述符和所声明的 Report Descriptor。解析 Short Main/Global/Local Item、嵌套 Collection、Push/Pop、Usage 列表/范围、ID/Size/Count、逻辑范围、Input 数组/变量、填充及按类型和 ID 区分的位偏移。存在可用布局时选择 Report Protocol。 | 已实现；有界解析／状态宿主矩阵及真实 QEMU 描述符／Report 传输、终端滚轮消费。ID／位图／复合接口／附加按钮布局为宿主证据，不能替代实机验收。 |
| USB.6 | 外置 Hub、动态插拔及异常恢复分别验收，设计见下文。 | 仅规划。 |

USB Core 应拒绝零长度、截断头、总长度矛盾、使用端点零作为数据端点及资源耗尽。诊断保留
VID、PID、类别、配置、接口及备用设置、端点地址/传输类型/最大包长/间隔。可选字符串使用有界
UTF-16LE 读取。跳过不支持的接口时保留同设备其他接口。设备地址由 xHCI Address Device 设置。

USB.5 支持 Generic Desktop、Keyboard/Keypad、Button 页，包括相对 X/Y/Wheel 和附加按键。
计算字段偏移时计入常量及未知 Usage。在提取前校验长度、数量乘位宽等算术，并限制描述符字节数、
栈深、全局状态、报告 ID 和字段数。验证整包实际长度后再提交状态。测试跨字节符号扩展、多 ID、
键盘数组及位图、非法 ID 零、截断、缺失 End Collection、过深嵌套、超大字段。
仅允许确实支持 Boot 的接口回退。绝对轴与厂商专用功能另行扩展。

## USB.1 实现范围

`pci::first_xhci` 复用 PCI 桥遍历。早期引导预留独立虚拟窗口并依据内存映射排除普通 RAM、
固件和帧缓冲。先映射一个引导页读取能力与接管链，确认停机后再探测 BAR 大小。
当前范围为首个控制器、BAR0 前 256 KiB、4 KiB 页、最多八个 slot 和 32 个 scratchpad。
接管链超出引导页时返回错误。这些边界有待实机验证。

内存初始化后由 `DmaPage` 提供清零的 DMA32 页。DCBAA、scratchpad 指针表、ERST 和两条对齐
的环位于同一页内互不重叠的区域。生产者最后发布 cycle bit，用命令 TRB 的物理地址匹配完成事件。
消费端读完整个事件后推进 ERDP。轮询采用 ACPI PM 定时器和尝试次数上限。启动出错时关闭总线主控，
无法确认停机及复位时保留 DMA 页。控制器按需启用。本阶段尚无 HID 报告到达桌面。

## USB.6 设计与验收

1. **外置 Hub。** 拓扑记录包括控制器、根端口、父 slot/端口、层数、路由字符串、速度，以及适用时的
   TT Hub slot、端口和 think time。Hub 类驱动读取描述符，逐端口供电并等待稳定，监听中断位图，
   逐项读取和清除状态变化并复位下游端口。子设备复用枚举逻辑。限制层数、端口和路由半字节。
   测试 USB 2 单/多 TT、嵌套 Hub、不支持的拓扑、供电缺失及兄弟端口隔离。
2. **热插拔。** 根端口和 Hub 通知进入有界调度工作队列。设备句柄包含 slot 和代次；状态依次为连接、
   枚举、已配置、断开中和已回收。注销输入绑定并释放按住的键和按钮，停止端点并确认在途完成后回收
   DMA。忽略 slot 重用前的过期完成事件。测试 EP0/interrupt IN 在途拔除、反复重连、双键盘与鼠标
   互不干扰，对比每轮 DMA 页、slot 和队列占用。
3. **异常恢复。** 每端点记录错误码、计数、超时与重试上限。按适用协议清除设备端点 halt，
   执行 Reset Endpoint、Set TR Dequeue Pointer 后恢复；失败则升级为端口复位和重新枚举，再到有界
   控制器重启。确认硬件停止访问前保留 DMA。验收 STALL、短包、迟到完成、停机超时和控制器故障，
   同时记录独立 COM1 与桌面响应。

## 根端口复位实现（2026-11-24）

有界遍历 Supported Protocol 能力，拒绝无效／重叠端口范围、未知主版本和自定义速度 ID 表。
仅对已连接端口供电。PORTSC 写入仅保留 RW 的电源、指示灯和唤醒字段，不回写 PED、复位触发位或
RW1C 变化位。USB 2 使用端口复位；USB 3 接受已启用的 U0 链路，否则执行 warm reset。
供电、复位及恢复等待均由 PM 定时器限制，断开或错误速度按失败处理。QEMU 门禁仅验证 USB 2
端口；USB 3 复位尚未验证。该历史复位批次早于下述端点实现。

首次运行在未启用 PCI 总线主控时发布 ERSTBA，触发 HCE。修复为初始化持有的 DMA 页、启用总线
主控，再发布控制器指针，保留原有停机／复位／隔离失败路径。实际结果见验证记录。


## USB.2–USB.5 实现边界（2026-11-25）

可选 USB.2–USB.5 已实现根设备枚举、Boot/Report 键盘和相对鼠标，并接入现有桌面。Boot 与 Report 各通过五种独立 QEMU 拓扑，包含全速／高速；桌面、PTY、串口、存储及运行时回归通过。验证记录区分模拟设备、仅宿主及实机覆盖范围。native/GUI 公共 ABI 保持冻结。USB.6 Hub、运行时热插拔和完整恢复另行验收。实机待验。

仅接纳一个控制器、八个直连根设备、每设备四个 alternate-zero 接口、每 HID 接口一个 interrupt IN 端点、64 字节包、2,048 字节配置／报告描述符、八个 Report ID、48 个 Input 字段、每 Main 项 16 个显式 usage、四层 Global Push 和八层 Collection。usage 范围可描述更大的键盘位图；各 ID 的 Input/Output/Feature 位偏移独立。未知数据字段和常量填充仍推进偏移；绝对轴、长项及 Local Delimiter 不在此范围。只选择第一个配置，不发送字符串或 interrupt OUT／LED 请求。

低／全速、高速、SuperSpeed 的 EP0 初值分别为 8／64／512 字节；描述符前缀决定是否 Evaluate Context 更新包长。检查 Address Device 完成及输出状态，不发送 USB SET_ADDRESS。先选 Boot，再可选解析 Report；解析／长度拒绝时，符合条件的接口保留 Boot。传输失败则停止可选控制器，确认 halt/reset 后回收，不能确认时隔离 DMA；通用端点恢复属于 USB.6。空闲中断端点的 NAK 没有虚假的输入超时。布局内存分配可失败返回。

实机、USB 3 复位及 64 字节控制器上下文支持分别待验。仅支持标准速度 ID；外置 Hub、TT 拓扑、运行时拔插、任意恢复及无限制 HID 布局未验收。使用实机前须查阅验证记录。

官方依据：[Intel xHCI 1.2b](https://cdrdv2-public.intel.com/625472/625472_xHCI_Rev1_2b.pdf)、[USB-IF HID 1.11](https://www.usb.org/sites/default/files/hid1_11.pdf)、[USB 2.0](https://www.usb.org/document-library/usb-20-specification)。

Boot 接口切换 Report 时还须保留键盘输入或同时具备相对 X/Y 轴。相对轴／滚轮增量限定为 i16；未知字段支持受检的有符号／无符号 32 位逻辑值。QEMU 验收覆盖全速／高速，低速实机设备仍分别待验。

单个 DMA 帧现容纳 16 项命令环和 64 项事件环；编译期不变量为全部 32 个中断请求再额外保留超过 16 项空间。调度器每轮仍最多消费 16 项。门禁要求 130 个 No Op，保留至少两次事件环回绕、命令环回绕及 slot 回收验证。32 接口同时流量具有容量证明，不宣称已做 QEMU 饱和验收。
