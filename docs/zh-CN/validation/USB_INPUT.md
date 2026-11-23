<!-- Record USB evidence separately from planned host, QEMU and physical acceptance procedures. -->
# USB 输入验证

[English](../../en/validation/USB_INPUT.md) | **简体中文**

## 2026-11-23 证据记录

实现边界见 [USB 里程碑](../milestones/USB_INPUT.md)。已检查的远端基线为 `cdaf5d9`。
Windows 进程启动器在 PowerShell 启动前失败（HRESULT 0x8007000F）。未核实本地工作区状态，
未执行 Rust 编译、QEMU 或实机测试，尚无运行串口记录、截图或通过结果。远端 CI 独立，仍仅在推送 main 时触发。

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
第二次启动不加控制器。控制器测试无需手动输入。每次启动限时 90 秒。
检查 `build/logs/xhci.serial.log`、`xhci.qemu.log`、`xhci.result.txt` 及对应的 `xhci-absent` 文件。

第一次应发现 PCI 类别、收到 No Op 完成，经命令环和事件环回绕累计完成 34 条 No Op，
取得有效 slot 并完成 Disable Slot。第二次应标记控制器缺失，并通过原有启动与磁盘测试。
两次均要求 QEMU 退出码 33、原冒烟检查通过且没有 USB 拒绝或 DMA 隔离记录。
`[USB:REJECT]` 给出阶段和原因；环启动前先查 BAR 与固件接管，超时查看 QEMU 标准错误。
枚举和键盘输入需要各自的运行证据。

## USB.2～USB.5 实现后的验收

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
