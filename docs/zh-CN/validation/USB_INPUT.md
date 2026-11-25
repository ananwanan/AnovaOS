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

## USB.6 实现与本地验收（2026-11-25）

本批次从干净的 `dev`（`770b881`）创建 `feat/usb-input-recovery`。新增协作式 USB 工作任务、
USB2 Hub 拓扑、根端口／Hub 运行时枚举、按住状态释放、确认停止后的端点／slot 回收和有界恢复。
没有写入实机介质或真实块设备；main-push-only Actions 和 native/GUI 公共 ABI 不变。

新门禁使用 q35/TCG、锁定固件、独立 UART/TCP-QMP、新建 ext4 文件、180 秒会话看门狗以及
五秒交互／插入预算。成功必须经过正常 guest shutdown、e2fsck，并导出精确 PTY 文件字节。
`build/usb-hotplug-test/` 和 `build/usb-recovery-test/` 保留命令、串口／QMP、PPM 画面、导出字节、
结果和旧尝试。`build/usb-lifecycle/` 保留构建／检查日志，包括开发过程中的失败。
上文 USB.1–USB.5 的原始证据保持历史范围，不替代本批次重新运行的检查。

宿主拓扑测试拒绝截断／非法描述符和越界层数／端口，检查最近 TT 继承、速度选择及后代关系。
生命周期证据测试拒绝陈旧代次、仍活跃 slot 的重用、缺少释放以及错误的 DMA／页数记录。
生产输入能力仍受既定解析器边界约束。

最终协议复核收紧了 Hub 接纳，同时检查设备和接口协议。具备多 TT 能力的高速 Hub，其默认
接口仍是单 TT；拒绝未选中的多 TT 模式及不一致的协议组合。宿主夹具覆盖全部三组合法组合
和七组非法组合（`tt-default-protocol.log`）。合成多 TT 继承仅有宿主证据，尚未实现替代接口
选择。依据为 [USB 2.0 第 11.23.1 节](https://www.usb.org/document-library/usb-20-specification)。

保留的开发失败：`hotplug-first.log` 记录唯一键盘删除后 QMP 无法发送松键；
`hotplug-second.log` 记录页数量类型不匹配；`hotplug-third.log` 记录等待已经释放的按钮事件。
现在在删除时检查释放，在重新插入后执行一次新的按下／释放。`hotplug-fourth.log` 记录
QEMU 8.2.2 默认八端口 Hub 的 PortPwrCtrlMask 少一个字节而被拒绝。解析器继续严格检查 USB2
长度，正常 Hub 场景采用四端口及 `port-power=on`。`hotplug-fifth.log` 记录根端口／嵌套 Hub
循环通过。`hotplug-final.log` 记录 QEMU 的 `attached` 属性只读，子树测试改用真实
`device_del`／`device_add`。`recovery-first.log` 保留宿主测试访问私有字段的编译错误；
`recovery-second.log` 记录真实 EP0 STALL 和控制器 halt／重启恢复通过。

QEMU 的全速 Hub 模型没有高速事务转换，TT 记录仅有宿主证据。低速／全速／高速实机、
USB3 Hub／复位、64 字节实机控制器上下文、任意中断端点 STALL／事务故障、无法确认 halt 时的
DMA 隔离注入、32 接口饱和以及断电行为均不在此次验收范围。受控重启成功不证明任意控制器
损坏均可恢复。键盘 LED／输出报告和无限制 HID 布局仍分别开发。不宣称实机或远程 CI 通过。

精确模型依据：[QEMU 8.2.2 Hub](https://github.com/qemu/qemu/blob/v8.2.2/hw/usb/dev-hub.c)
和 [USB 设备生命周期](https://github.com/qemu/qemu/blob/v8.2.2/hw/usb/bus.c)。

### 已执行的 USB.6 门禁

| 命令 | 实际结果 |
| --- | --- |
| `cargo xtask usb-hotplug-test` | 两次独立启动通过：空根控制器、两层可控供电四端口 Hub。每次均执行十轮按住 Shift 拔除／重连、真实按钮释放、鼠标／UART 继续交互、精确物理页回收及超过八个生命周期的槽位复用。Hub 启动还在按住键和按钮时移除／重建整棵子树，随后删除空的嵌套 Hub，正常兄弟设备仍可使用。精确 `before\n`／`after\n` 文件、画面、e2fsck 和正常关机通过。 |
| `cargo xtask usb-recovery-test` | 一次独立启动通过：真实畸形 Hub 拒绝且保留兄弟输入；不支持的 EP0 请求返回 STALL，经 Reset Endpoint／Set TR Dequeue Pointer 后真实重试描述符成功；有中断 TD 在途时实际 halt 控制器，确认复位后仅重启一次。独立 UART、键鼠、画面和精确 `control\n`／`restart\n` 文件通过。 |

首次成功的新增门禁日志为 `build/usb-lifecycle/hotplug-cascade.log` 和 `recovery-final.log`。
协议复核后，两项完整门禁再次通过，日志为 `followup-hotplug.log`、`followup-recovery.log`；
三次独立启动和各会话结果均为 `Ok(())`，`followup-checks.json` 保留最终命令和耗时。
`build/usb-lifecycle/physical-availability.json` 记录本次 WSL 未暴露 USB
总线设备或 `/dev/serial/by-id` 条目。恢复门禁的端点 STALL 为 EP0，不冒充中断端点 STALL
的实测；运行时在尚未观察完成的 EP0 传输中拔除，仍需独立验证。

### USB.6 回归重跑

| 检查 | 实际结果 |
| --- | --- |
| `cargo xtask xhci-test` | 五次启动全部通过，包括 130 个 No Op、slot 复用、控制器缺失／为空及真实超时／STALL 清理。 |
| `cargo xtask xhci-packet-test` | 真实 EP0 从 8 到 64 字节的 Evaluate Context 和描述符／配置传输通过。 |
| `cargo xtask usb-input-test --boot` 和 `cargo xtask usb-input-test` | 每种模式五个独立拓扑均通过，包括键盘、鼠标、Report 滚轮、精确 PTY 字节和画面。 |
| `cargo xtask input-test`、`cargo xtask gui-test` | 独立 PS/2、合成器输入／故障／重启及缺设备回退检查通过。 |
| `cargo xtask desktop-test` | 六次受监督桌面启动及持久化字节全部通过。 |
| `cargo xtask desktop-pty-test --without-toolchain` | 两次启动通过，覆盖真实 PTY 输入、Ctrl+C、resize、有界历史、压力和重启。 |
| `cargo xtask terminal-test` | 独立串口编辑、CRLF／EOF 和全新启动检查通过。 |
| `cargo xtask verify` | 全部 14 个运行时与受控故障场景按要求的退出分类通过。 |
| `cargo xtask test-host` | 完整宿主矩阵通过，包括 USB 描述符／HID／拓扑逻辑、源生命周期、资源耗尽和日志扇区／flush 中断。 |
| 严格 Clippy | `xtask`、`anova-usb`、`anova-input` 的全部目标，以及内核 `desktop,xhci`、`desktop,usb-recovery-test` 均以 `-D warnings` 通过。 |
| `cargo xtask usb-image` | 生产 USB 桌面 ISO 构建通过，位于 `build/images/anova-usb-desktop.iso`。 |
| 格式、文档和文件用途 | workspace／用户格式、90 对文档、189 个 Markdown、587 项用途清单及 `git diff --check` 通过。已审查修改文件的英文用途／安全说明和 LF 换行。 |

`build/usb-lifecycle/regressions.json` 记录命令、耗时、退出码和完整日志。网络／包／编译器／
bootstrap、实机 GPU 和长时 GUI Files-fault 门禁保持不变，本生命周期批次未重跑。
远程 CI 继续按未改变的 main-push-only 策略延后验收。

`build/usb-lifecycle/image-identities.json` 中的最终 ISO 提取检查确认三个打包内核均含 Hub／
回收实现。生产及热插拔镜像不包含恢复注入标记，只有恢复专用镜像包含这些标记。
`frozen-source.json` 在最终门禁前固定 15 个修改的实现／清单文件；`validation-summary.json`
核对其哈希未变，并记录全部 36 个修改／新增文件、三个 ISO、打包内核的哈希、成功检查和
实机可见性。`delivery-checks.json` 保留最终文档／用途／空白检查。

## 无输入屏幕诊断后续（2026-11-25）

用户报告实机插入 USB 键鼠后仍提示无输入，且没有可用 COM1 或 PS/2。这是尚未解决的实机
失败，不是硬件通过结果。合成器现自动显示有界 USB 启动快照，不新增驱动能力或修改 native／
GUI ABI。最多一次读取 32 KiB，保留首条 USB 行及最后十一行，替换不可打印字节，每行最多
126 字节。覆盖层明确标为启动快照，不记录后续运行时错误。

没有 COM1／PS/2 时：

1. 使用新 `build/images/anova-usb-diagnostics.iso`，由 `cargo xtask usb-diagnostics-test`
   生成并验证；其字节与生产 USB 桌面 ISO 一致。
2. 开机前直接连接有线键鼠，初次排查先绕过外置 Hub／扩展坞。
3. 若仍无输入，拍摄全屏，确保底部 `USB BOOT DIAGNOSTICS V1` 面板清晰，记录电脑／主板
   型号及使用的物理端口组。
4. 按日志阶段定位：`USB:ABSENT` 表示没有选中的 xHCI；`USB:REJECT` 为控制器发现／启动
   失败；`USB:ATTACH_REJECT` 为设备枚举拒绝；`USB:HID ... bound=true` 只证明初始类绑定。
   保留完整 `phase=`／`reason=` 行和 PCI BDF。当前仅选择第一个 xHCI，未实现 EHCI／OHCI／
   UHCI；不能只凭端口颜色判断它所属的控制器。

生产诊断门禁三次独立 q35／TCG 启动通过：无控制器、空控制器、真实畸形八端口 Hub 描述符。
独立字形／颜色检查要求实际显示标题及 `ABSENT`、`CONNECTED=0` 或
`HUB-DESCRIPTOR-PORTS-LENGTH`。空控制器场景随后添加真实模拟键鼠，打开图形终端，并确认
面板消失。各会话正常关机并通过 e2fsck，保留五秒交互和 180 秒会话上限。

证据为 `build/usb-diagnostics-test/result.txt`、保留的 UART／QMP／PPM 和精确 ISO SHA；
`build/usb-screen/diagnostics-second.log` 是成功构建／运行日志。首次宿主门禁缺少导入的编译
失败保存在 `diagnostics-first.log`。已目视核对 `build/usb-screen/` 的 PNG，原始 PPM 未改变。
本后续没有写入实机引导介质。用户之后提供的照片及驱动修复记录于下节，本节保留此前仅
屏幕诊断的验收范围。

后续还通过 `cargo xtask usb-hotplug-test`（两次启动）、`cargo xtask desktop-test`（六次启动）、
`cargo xtask gui-test`、`cargo xtask check-apps` 和全部目标的严格 `xtask` Clippy，日志保存在
`build/usb-screen/`。本次修改合成器和诊断门禁，没有修改 USB 内核驱动。先前 USB.6 验收及
源码／镜像哈希保持历史范围，新镜像标识单独记录于 `build/usb-screen/validation-summary.json`。

## 实机 register-range 后续（2026-11-25）

用户的诊断屏幕照片确认 `00:14.0` 的 PCI 类为 `0c0330`，并出现
`USB:REJECT phase=pci reason=register-range`。照片没有标明控制器厂商、失败偏移或已枚举的
HID 设备。代码审查发现固件接管错误地把扩展能力遍历限制在首个 4 KiB，导致拒绝公布更高
能力偏移的合法控制器。单凭照片不能证明是哪项 register-range 检查失败；重建镜像补充了
缺失的寄存器布局与 PCI 标识。

修复按 xECP 链公布的位置，在原有 256 KiB 窗口内经过检查后按需映射 UC／NX 页面，保留
固件 MMIO 排除和 52 位物理地址上限。仍先完成所有权移交，再停止控制器和测量 BAR；所有
已访问启动区间必须包含于测得 BAR，才能分配 DMA。两次遍历均限制为 256 个头部，并拒绝
非法头部／链接。接管前上限明确只是准入限制，不是 BAR 大小证明；此阶段依赖控制器正确
公布自身能力寄存器的位置。

主要依据：[Intel xHCI 1.2b 第 5.3.6、7 节](https://cdrdv2-public.intel.com/625472/625472_xHCI_Rev1_2b.pdf)
和 [Intel 500 系列封装内 PCH 寄存器表](https://cdrdv2-public.intel.com/631120/631120-002.pdf)。
后者记录了 `0x8000` 的能力及 `0x846c` 的旧式所有权寄存器；这是参考布局，不代表用户机器
就是该型号。

新增五个宿主用例覆盖从 `0x8000` 到 `0x846c` 的合成链、越界回调前拒绝、非法／空／未知
头部、遍历工作量上限、跨页区间、对齐／溢出，以及后续测得 BAR 小于已访问区间的情况。
现有 QEMU 控制器把能力放在首个页面内，因此正常 QEMU 通过不能证明实机高偏移 MMIO 能力
或固件所有权移交正常。硬件复测仍待完成。

验证结果及源码／镜像标识单独保存于 `build/usb-registers/`；先前 USB.6 及仅屏幕诊断记录
保持其历史范围。构建不写入实机引导介质。首次新镜像测试保留相同端口和固件设置，开机前
连接键鼠；若仍枚举失败，保留完整面板。native／GUI ABI、输入容量和仅 main 推送 CI 均不变。

首次 `xhci-test-first.log` 已完成控制器初始化、两类 HID 绑定和 guest `[TEST:PASS]`，但
宿主拒绝了修改后的 PCI 日志字段顺序。新诊断标识已保留原有末尾 `class=0c0330` 字段，
没有放宽宿主要求的标记。`checks-first.json` 将该失败与重跑结果分别保留。

`build/usb-registers/checks.json` 中的完整重跑通过全部 20 个 USB 宿主测试，以及 USB／
xtask 和生产／恢复两种内核配置的严格 Clippy。QEMU 共通过 22 次独立启动：`xhci-test`
五次、`xhci-packet-test` 一次、`usb-input-test --boot` 五次、`usb-input-test` 五次、
`usb-hotplug-test` 两次、`usb-recovery-test` 一次、`usb-diagnostics-test` 三次。
包含未改变的 PS/2 独立／共存／无输入场景，以及真实控制传输超时／STALL、slot 回收和
控制器重启。

`fmt-first.log` 保留最终 PCI 日志参数换行的格式修正。完整矩阵之后只改变格式，
`tested-source-before-format.json` 保留该矩阵的源码哈希。`delivery-checks.json` 记录格式化
后的检查及全新生产诊断运行。`validation-summary.json` 核对最终源码哈希、字节一致的生产／
诊断 ISO、提取内核标识以及恢复注入标记缺失。五个新增高偏移／边界用例仍仅宿主覆盖。
不据此推断新的实机或远程 CI 通过；本修复未重跑无关的网络／包／编译器套件。

Scratchpad 后续：控制器缓冲区上限由 32 提高至 512。独立持有的 4 KiB DMA 指针表避免与命令／事件环重叠；数量为零时不分配地址表。分配失败时通过所有权释放未发布缓冲区，已发布 DMA 继续遵循原有停止确认／隔离规则。启动日志记录实际数量、上限和原始 HCSPARAMS2。仅 QEMU 测试配置构建 0／33／512 个缓冲区，检查每个指针并拒绝 513；这验证分配与布局，不证明实机控制器消费正常。仍需实机复测，要求超过 512 个缓冲区的控制器仍不支持。

Scratchpad 验证：xhci-test 五次启动通过，实际在内核检查 0／33／512 个缓冲区布局并拒绝 513；usb-recovery-test 通过。生产诊断启动和严格生产内核 Clippy 通过。日志保留于 build/usb-scratch/；较大数量测试未模拟硬件 DMA 消费 Scratchpad，也不构成实机验收。

## 历史实机 USB 阻塞（2026-11-25）

用户使用 Scratchpad 修复镜像后的最新照片确认 PCI `00:14.0`、厂商／设备 `8086:a12f`、
类 `0c0330`。发现过程已到达 xECP `0x8000`、旧式能力 `0x846c`（BIOS=0／OS=1），测得
BAR 为 `0x10000`。HCSPARAMS2 为 `0x14200054`，Scratchpad 需求为 34，低于新上限 512。
本次启动已越过先前 register-range 和 Scratchpad 准入失败，但尚未证明 DMA 运行正常。

**历史失败（2026-11-26 已解决）：** `USB:REJECT phase=pci reason=port-protocol-speed-table`。Supported Protocol
解析器当时拒绝所有非零 Protocol Speed ID Count。HID 尚未开始枚举，因此实机键鼠可用性
仍未验收。下一步实现有界 Protocol Speed ID 表解析，以及端口复位和设备上下文所需的速度
解释，增加畸形表测试并分别验证 QEMU 和实机。不能仅删除拒绝条件，也不能以已有 QEMU 门禁
推断实机成功。照片未包含速度表条目，其具体值仍未知。
## 显式 USB 协议速度表（2026-11-25）

Intel 8086:A12F 实机在报告 34 个 Scratchpad 后到达 `port-protocol-speed-table`。
驱动现解析每个 Supported Protocol 能力最多 15 个 PSI DWORD，检查 BAR 区间及下一个
能力的重叠，并为每个端口保存 ID 到速度的映射。显式表不继承未公布的默认 ID。
支持 USB2 的 1.5／12／480 Mb/s 和对称 SuperSpeed 5 Gb/s；未支持速率及非对称配对
保持未映射。拒绝非法 ID、重复项、零尾数和错误配对。非零 Protocol Slot Type 仍不在
本配置范围内。已连接端口的速率未支持时记录并跳过，不再拒绝其他可用 USB2 端口。

根设备的 Slot Context 保留实际 PORTSC 速度 ID；USB2 Hub 子设备按解码速度选择已公布
的 ID。EP0 大小、轮询间隔和 TT 决策仍使用内部 USB 速度类别。新增 `USB:PROTOCOL` 和
`USB:PORT_SKIP` 日志显示准入情况。最多八个协议组共享 128 字节映射及 256 字节端口索引，拒绝第九组；原有设备／DMA
上限不变。这不证明 SuperSpeedPlus、非对称传输或实机通过。下一步使用新镜像在用户机器
相同端口启动，保留完整面板。

依据：[Intel xHCI 1.2b 第 7.2.1–7.2.2 节](https://cdrdv2-public.intel.com/625472/625472_xHCI_Rev1_2b.pdf)。
宿主合成用例覆盖显式／重映射 ID、等效速率编码、无隐式回退、未支持高速率、重复／零 ID
及非法非对称配对。标准 QEMU 检查默认 ID 的运行时路径；实机显式速度表的 DMA 消费尚未
验证。本批次证据保留于 `build/usb-speed/`。

首次 QEMU 尝试因异步栈帧复制 4 KiB 逐端口映射而触发内核栈保护。实现已改为八组共享映射和字节端口索引，总计 384 字节；未扩大栈上限。原始失败保留于 build/usb-speed/xhci-first.log 和 checks-first.json。

最终速度映射批次通过 22 个 USB 宿主测试，USB／xtask 及生产／恢复内核的严格 Clippy，另通过 17 次 QEMU 启动：xhci-test 五次、xhci-packet-test 一次、usb-input-test 五次、usb-hotplug-test 两次、usb-recovery-test 一次、usb-diagnostics-test 三次。Boot-only 模式和无关 OS／编译器套件保留但本批次未重跑。格式、双语文档、590 项文件用途及空白检查通过。build/usb-speed/ 下 checks.json 和 delivery.json 保留命令、结果及最终源码／镜像哈希；交付的生产镜像不含测试注入标记。

2026-11-26，用户在此前照片中的 Intel 8086:a12f 机器测试速度表修复镜像（SHA-256 07b34f7cb0b7713e198bbe32e65bb5d329d6c6cefcf7017b844f570ab7a22cc7）后，反馈 USB 键盘／鼠标已正常。本配置的无输入阻塞据此关闭。这是用户报告的实机证据；其他控制器、实机 Hub／插拔／恢复压力、SuperSpeedPlus 及无限制 HID 仍需独立验证。CI 仍仅由 main 推送触发，不宣称远端 CI 通过。
