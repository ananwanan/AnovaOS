<!-- Record USB evidence separately from planned host, QEMU and physical acceptance procedures. -->
# USB 输入验证

[English](../../en/validation/USB_INPUT.md) | **简体中文**

## 2026-11-23 证据记录

实现边界见 [USB 里程碑](../milestones/USB_INPUT.md)。已检查的远端基线为 `cdaf5d9`。
Windows 进程启动器在 PowerShell 启动前失败（HRESULT 0x8007000F）。未核实本地工作区状态，
未执行 Rust 编译、QEMU 或实机测试，尚无运行串口记录、截图或通过结果。远端 CI 独立，仍仅在推送 main 时触发。

## 2026-11-24 本地后续验证

`origin/dev` 与本地 `dev` 均为 `cdaf5d9`，干净的 USB 分支 `719f4e5` 已包含该基线。
获取远端并合并 `origin/dev` 无需改动。当前已可通过 Ubuntu-24.04 WSL 启动命令。

首次真实 QEMU 启动在 USB 门禁失败，错误为 `phase=start reason=controller-fault`；
增加诊断后的重跑记录 `status=0x1000 command=23`（HCE）。控制器停机时发布 ERSTBA 也可能
读取事件环段表。修复为初始化持有的 DMA 后、发布 ERSTBA 前启用 PCI 总线主控。
USB 失败时普通启动／存储测试仍到达 `[TEST:PASS]`，USB 门禁正确拒绝了该结果。

命令环及根端口复位通过三次启动门禁：连接键鼠、空 xHCI 控制器、无 xHCI 控制器。
每次均退出 33、未超时，保留原运行时／存储证据并返回 `result=Ok(())`。
带键鼠启动记录端口 5、6，均为 `protocol=2 speed=3 enabled=true`，随后报告
`connected=2 reset=complete`；空控制器报告 `connected=0 reset=complete`；
无控制器报告 `[USB:ABSENT] class=0c0330`。有控制器的两次启动均完成跨环回绕的 34 条
No Op 以及 Enable/Disable Slot 循环。

宿主门禁拒绝重复／越界端口、错误协议／速度、禁用端口、缺失或数量不符的复位记录、隔离、
错误退出与超时。本批执行完整 xtask 宿主测试、严格 xtask／内核 Clippy、非测试 xHCI 内核构建、
格式、文档、文件用途与空白检查。实机、USB 3 复位、畸形控制器注入、端点超时及全部 HID 报告
仍未验证。本次仅验收命令环部分及 USB.2 复位前置步骤，不代表完整 USB.1 或 USB.2 验收。

## 控制器测试

在项目支持的 Linux/WSL 开发环境使用锁定的工具和固件运行：

```sh
cargo xtask doctor
cargo xtask fetch
cargo fmt --all -- --check
cargo build --locked -p anova-kernel --target x86_64-unknown-none --features xhci
cargo clippy --locked -p anova-kernel --target x86_64-unknown-none --features xhci,qemu-test -- -D warnings
cargo test --locked -p xtask xhci_gate
cargo xtask xhci-test
python3 scripts/check_docs.py
cargo xtask check-files
git diff --check
```

测试复用 q35/TCG、OVMF、临时磁盘、串口捕获和 debug-exit。第一次启动加
`-device qemu-xhci,id=xhci -device usb-kbd,bus=xhci.0 -device usb-mouse,bus=xhci.0`，
第二次仅添加控制器，第三次不加控制器。控制器测试无需手动输入。每次启动限时 90 秒。
检查 `build/logs/xhci.serial.log`、`xhci.qemu.log`、`xhci.result.txt` 及对应的 `xhci-empty`、`xhci-absent` 文件。

第一次应发现 PCI 类别、收到 No Op 完成，经命令环和事件环回绕累计完成 130 条 No Op，
取得有效 slot 并完成 Disable Slot。带键鼠启动还要求两个不同的 USB 2 根端口实际启用。第二次报告零个已连接端口，第三次标记控制器缺失，并通过原有启动与磁盘测试。
三次均要求 QEMU 退出码 33、原冒烟检查通过且没有 USB 拒绝或 DMA 隔离记录。
`[USB:REJECT]` 给出阶段和原因；环启动前先查 BAR 与固件接管，超时查看 QEMU 标准错误。
枚举和键盘输入需要各自的运行证据。

当前 `xhci-test` 还要求实际提交的超时及真实 STALL 两次启动通过；`xhci-packet-test` 单独要求 EP0 输出上下文包长回读通过。

## USB.2～USB.5 验收流程

- 用桌面镜像和上述三个设备参数启动，另加 `-machine i8042=off` 再测一次以排除 PS/2。
  记录 VID/PID、接口、端点、协议和真实传输完成事件。另启仅 PS/2 的环境运行 `cargo xtask input-test`。
- 在有焦点的图形终端输入 `echo Aa1`，核对 PTY 输出和退出状态。验证 Shift、Ctrl、Alt、编辑键、
  方向键、功能键、多键、释放和重复。用双键盘验证另一把键盘按住 Shift 时第一把释放的行为。
- 验证相对移动、边缘裁剪、左/右/中键按放、焦点以及实际窗口动作。连续移动期间测量交互延迟，
  保存独立 QMP 屏幕读回。用描述符确实声明 Wheel 的设备测试有符号滚轮及窗口响应。
- 宿主解析测试覆盖描述符链、键盘差分/rollover、符号轴、独立报告 ID、非字节对齐字段、未知
  Usage、截断、溢出和过深嵌套。每个样例记录预期事件序列。
- 运行 `cargo xtask desktop-test`、`cargo xtask desktop-pty-test --without-toolchain`。
  两种输入源都缺失时，核对普通桌面与独立 COM1，保留 desktop-test 的缺设备退出语义。按拓扑保存日志。

## 物理机器检查——待实机验收

1. 在现有系统记录 PCI BDF 和类别（例如 `lspci -nn -d ::0c03`）、控制器型号、BAR0、
   固件版本、USB 拓扑以及设备 VID/PID，确认控制器为 0c/03/30。
2. 记录 UEFI、Secure Boot、xHCI handoff 和 Legacy USB 设置（如固件提供）。
   采用项目已有的 UEFI 启动方式、独立测试介质和串口控制台，将有线键鼠直接插入根端口后启动。
3. 按控制器与枚举里程碑核对串口，再测桌面输入。固件界面能够打字不足以验证 AnovaOS 驱动。
   另测启动前拔除 USB 设备及 PS/2、USB 共存（如有相应接口）。
4. USB.5 完成后使用复合键盘及带滚轮鼠标复测，保留串口日志、控制器标识、配置、实际 PTY 字节和窗口行为。
   外置 Hub 和运行时拔插在 USB.6 单独验收。本批没有通过验收的物理机器。


## USB.2–USB.5 后续（2026-11-25）

获取的 `origin/dev` 为 `cdaf5d9`，`feat/usb-hid-input` 已包含该版本，fast-forward-only 合并无需改动。保留了开始时全部 20 个修改文件；未 reset、force-push、写真实块设备或修改 Actions 触发器。重新检查了当前源码、控制器行为及测试；上文旧 HCE／复位记录仅为历史背景。

最终 Boot 与 Report 桌面轮次各通过禁用 i8042 的 USB-only、共存、全速 USB 键鼠、PS/2-only 和无输入五种拓扑，精确字节／状态、几何、光标像素、时延及失败断言均通过；最终命令／结果见下表。QEMU 提供真实描述符及传输完成；预生成报告日志或截图不能替代传输。

证据目录：

- `build/usb-input/frozen-checks.json`：命令退出码、耗时及完整日志路径。
- `build/logs/xhci*.serial.log`、`.qemu.log`、`.result.txt`：控制器、枚举、空／无控制器、已提交 EP0 的超时及真实不支持描述符 STALL。超时／STALL 必须确认 halt/reset、精确恢复空闲帧数、保留正常 OS／运行时／存储证据并以 QEMU 33 退出。
- `build/usb-boot-test/` 和 `build/usb-input-test/`：各拓扑独立命令、UART、QMP、结果、PPM scanout 和精确导出 PTY 文件；`attempts/<timestamp>/` 在后续重跑前保留 UART／QMP／结果／截图，原工作盘文件也保留。归档安装前部分早期 USB UART／QMP 已被覆盖，原失败命令日志及 PNG 截图仍保留；最终验收轮次保留完整 UART／QMP。
- `build/usb-input/physical-availability.json`：WSL USB／串口可见性及明确待验的实机状态。

图形测试以物理按键输入 `echo Aa1`，五秒内检查字形，导出精确 `Aa1\n`，要求 echo 状态 `0\n`、Ctrl+C 子进程状态 `130\n`，并在干净 ACPI 关机／e2fsck 后用 debugfs 独立读取。字体按大写绘制，因此 scanout 核对 `AA1`，文件检查仍保持大小写精确。按下／释放确认限制宿主注入速度；超时或缺少确认直接失败，不重试注入。USB-only 矩阵检查编辑／导航／功能键、Shift/Ctrl/Alt、多键、调度器重复及最终释放。鼠标证据覆盖连续移动、边界处全部光标像素、三键按放、焦点及标题拖动，并核对精确最终几何与 scanout 像素。Report 滚轮使终端历史偏移 `10 → 0`，同时保留对应 scanout。

宿主夹具覆盖描述符截断／零长度／计数／端点界限、复合／alternate 接口、Boot rollover／保留字节／截断、跨源聚合、独立 Report ID、未知字段、常量填充、有符号非对齐提取、数组／位图、Push/Pop、Collection 及尺寸／深度／usage／报告上限。真实 QEMU 当前验证普通 Boot 键盘及相对滚轮鼠标布局；Report ID／NKRO／复合／附加按钮夹具属于宿主证据，不能宣称对应模拟或实机设备传输已验证。USB 3 复位、64 字节上下文硬件、无法确认 halt 的注入、任意 HID 布局及 USB.6 均待验。

保留失败：`build/usb-input/enumeration-first.log` 包含最初 Rust 借用检查失败，`enumeration-second.log` 保存成功枚举。旧 Boot 尝试保留禁用 i8042 后仍轮询的缺陷、大写字体 scanout 不匹配，以及批量注入导致的丢键／状态失败。现已跳过不存在的 PS/2 端口、保留各源保持状态，并在重复键释放后恢复较早仍按住的键；测试在下一字符前等待真实按下／释放。中间失败的看门狗／重复键轮次和命令启动错误不算验收。未降低运行时断言来通过失败。

**实机待验。** 当前 WSL 中无可见 USB 总线或 serial-by-id 路径，也未识别到已授权的实机及独立恢复通道。未制作或改写实机引导介质。步骤 5 不能标为全部完成；main-push-only 远端 CI 独立，本功能分支未运行远端 CI。

补充复现与修复：`build/usb-input/queue-overflow-original.log`／`queue-overflow-fixed.log`、`unsigned-original.log`／`unsigned-fixed.log`、`usability-original.log`／`usability-fixed.log` 保留修复前断言与修复后宿主结果；`ps2-original/` 保留完整无控制器回归失败，`ps2-fixed.log` 保存恢复早期回退后、原 PS/2 断言不变的通过结果。`attempt-retention.log` 验证失败 UART 按字节归档，并在新运行前清除过期成功状态。

## 最终实际检查（2026-11-25）

| 检查 | 结果（均通过） |
| --- | --- |
| `cargo xtask xhci-test` | 5 启动；130 No Op、两次事件回绕、slot、枚举、空／无控制器、超时／STALL 及精确帧恢复 |
| `cargo xtask xhci-packet-test` | 1 启动；真实 EP0 8→64、VID 0525/PID a4a2、配置 2、两个未绑定接口 |
| `cargo xtask usb-input-test --boot` | 5 独立拓扑；全速／高速 Boot 键鼠 |
| `cargo xtask usb-input-test` | 5 独立拓扑；Report 键鼠及实际滚轮历史消费 |
| `input-test`, `gui-test`, `desktop-test` | 3 输入启动、隔离 GUI 回归、6 桌面启动 |
| `desktop-pty-test --without-toolchain` | 2 启动；真实 PTY、中断、resize、压力、字节及重启 |
| `terminal-test`, `persistence-test`, `verify` | UART／重启、存储／错误恢复、全部 14 运行时／故障场景 |
| `test-host`, `check-apps`, `test_kernel_prepare.py` | 完整宿主矩阵、原生应用 Clippy、源码准备拒绝检查 |
| `gpu-fallback-test` | 无硬件回退回归；未重跑实机 GPU 门禁 |
| 严格宿主／内核 Clippy | 15 宿主包及 12 内核配置，随后复查修改配置；-D warnings |
| `usb-image`、格式、生成 GUI 头文件 | 构建生产 USB 桌面 ISO；workspace／用户格式及未改变的生成 C ABI 头文件通过 |

`build/usb-input/final-checks.json` 保留完整回归初轮及随后已修复的 Clippy 测试模块位置失败；`quality-checks.json` 记录修正后的严格检查；`followup-checks.json` 记录 EP0 后续及再次成功获取 dev／祖先检查；`capacity-checks.json` 记录最终 64 项事件环／130 命令 USB 重跑。保留原始 `clippy-host-original.log`。最终源码哈希和文档检查见 `validation-summary.json`、`delivery-checks.json`。

EP0 夹具为不连接网络后端的 QEMU 8.2.2 `usb-net`，仅验证描述符／配置及控制传输，保持未绑定。[模型定义](https://github.com/qemu/qemu/blob/v8.2.2/hw/usb/dev-network.c)。不据此验收网络能力。全部 32 接口饱和流量、低速实机、USB 3 复位及 64 字节实机上下文仍独立待验。保留网络／包／编译器／bootstrap 和长时 GUI Files-fault 门禁，本输入批次未重跑这些门禁。main-push-only 工作流文件未改动。
