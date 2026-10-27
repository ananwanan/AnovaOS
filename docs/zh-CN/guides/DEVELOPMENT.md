<!-- Translate reproducible host setup, implemented commands, validation and documentation maintenance. -->
# 开发与验收

[English](../../en/guides/DEVELOPMENT.md) | **简体中文**

从 `dev` 或任务分支开发，先读 [STATUS](STATUS.md)、[AGENTS](../../../AGENTS.md)。
M0/M1 建立 Cargo workspace、宿主 xtask、裸机内核和 UEFI ISO；M2 增加 BSP CPU 表、受保护
异常栈、ACPI/LAPIC 校时；M3 增加物理页/页表所有权、guard 栈和可回收 `alloc` 堆。
M4 引入隔离 Rust/C 用户程序，M5 增加 CPU/FPU 保存、用户抢占和进程生命周期，M6 增加 VFS、
init/shell、阻塞管道与轮询串口；M7 增加 PCI/virtio-blk、ext2 数据盘、编辑器与重启持久性。
对应边界见 [M3](../milestones/M3_MEMORY.md)、[M4](../milestones/M4_USERSPACE.md)、
[M5](../milestones/M5_PROCESSES.md)、[M6](../milestones/M6_FILESYSTEM.md)、[M7](../milestones/M7_STORAGE.md)。
外设 IRQ 仍屏蔽，block completion 使用轮询。后续受测配置截至 M15.6，按下文分别使用。

## 宿主配置

受测宿主为 x86_64 Ubuntu 24.04.3；WSL2 可用 TCG，M13 另验证 Ubuntu 24.04 WSL2/QEMU 8.2.2，
见 [M13 证据](../validation/M13.md)。从官方渠道安装 rustup，再安装工具：

```sh
sudo apt-get update
sudo apt-get install --no-install-recommends qemu-system-x86 xorriso curl dpkg-dev gcc g++ binutils make musl-tools qemu-user e2fsprogs openssl python3 binutils
rustup show active-toolchain
cargo xtask fetch
cargo xtask doctor
```

`rust-toolchain.toml` 固定 stable Rust 1.90.0、rustfmt、Clippy 和预编译 `x86_64-unknown-none`。
检查 QEMU 8.2.2，Ubuntu 包后缀可不同；dpkg-deb 来自 dpkg，dpkg-dev 会带入。
fetch 本地提取固定 OVMF，无需系统安装固件。xtask 不运行 sudo 或包安装脚本。

Cargo.lock 固定 registry 版本/校验和；`boot/dependencies.toml` 固定 Limine 9.5.0 二进制提交、
协议引用、OVMF 包与引导文件哈希。HTTPS 下载先校验再提取/使用；缓存损坏明确失败，核查后
仅删除报错的缓存文件再重试。默认固件为 `build/firmware/usr/share/OVMF/`；
`ANOVA_OVMF_CODE`/`ANOVA_OVMF_VARS` 可选现有普通文件，但仍须匹配固定哈希，不能绕过校验。

## 已实现基础命令

均从仓库根目录运行。普通失败返回 1，编译器/测试错误向上传播。

| 命令 | 输出/行为 | 拒绝或失败条件 |
| --- | --- | --- |
| `cargo xtask fetch` | 校验 `build/cache/`、`build/firmware/` | 网络、提取、校验错误 |
| `cargo xtask doctor` | 检查工具、版本、target、引导输入 | 汇总全部问题后失败 |
| `cargo xtask build` | `target/x86_64-unknown-none/debug/anova-kernel` 与 ELF 检查 | 编译/链接/ELF 失败 |
| `cargo xtask apps` | `build/apps/` 原生 Rust/C ELF、initramfs、ABI/SHA-256 清单 | 编译/链接/ELF 失败 |
| `cargo xtask check-apps` | 使用真实用户 target/链接参数做 Clippy | 警告视为错误 |
| `cargo xtask check-files` | DEV-001 文件用途头和清单覆盖 | 缺说明/清单项 |
| `cargo xtask image` | `build/images/anova-normal.iso` | 缺少/损坏输入即停止 |
| `cargo xtask run` | UEFI 工作负载、原生 PID 1、交互串口 shell | 进程错误传播；Ctrl+C 停 QEMU |
| `cargo xtask smoke` | 内存/CPU、11 项隔离、CPU/FPU/进程与 VFS/管道/shell/tmpfs-OOM | 缺隔离/回收证据、内核故障、退出码/期限错误 |
| `cargo xtask smoke --case panic` | 受控 panic，QEMU 35 | 故意返回 1 |
| `cargo xtask smoke --case timeout --timeout-secs 8` | 看门狗终止受控挂起 | 故意返回 1 |
| `cargo xtask smoke --case page-fault` | 实际 guard 写入到 IST2，QEMU 37 | 故意返回 1 |
| `cargo xtask smoke --case double-fault` | 栈失败到 IST1 的 #DF，QEMU 37 | 故意返回 1 |
| `cargo xtask smoke --case read-only` | 实际 supervisor 写只读页，QEMU 37 | 故意返回 1 |
| `cargo xtask smoke --case no-execute` | NX RAM 实际取指，QEMU 37 | 故意返回 1 |
| `cargo xtask smoke --case direct-no-execute` | HHDM 别名实际 NX 取指，QEMU 37 | 故意返回 1 |
| `cargo xtask smoke --case stack-guard` | 写普通栈 guard，QEMU 37 | 故意返回 1 |
| `cargo xtask smoke --case unmapped` | unmap/TLB flush 后访问，QEMU 37 | 故意返回 1 |
| `cargo xtask smoke --case oom` | infallible 分配 panic，QEMU 35 | 故意返回 1 |
| `cargo xtask verify` | 正常启动通过，十二种实际失败注入均拒绝 | 向量/error/IST/地址/结果错误或未到注入点 |
| `cargo xtask terminal-test` | 两次启动：UART 编辑/CRLF/EOF、动态 musl、Ctrl+C/EINTR、shell 重启、tmpfs 丢失 | 缺输出、guest 失败、每会话 30 秒超时 |
| `cargo xtask root-image build/disks/NAME.ext2` | 新 16 MiB native-v1 ext2 盘 | 拒绝已有文件和不安全路径 |
| `cargo xtask run --disk build/disks/NAME.ext2` | 复用显式镜像，guest shutdown 关机 | 不格式化缺少/已有镜像 |
| `cargo xtask persistence-test` | 编辑器/源码/二进制跨重启、e2fsck、设备错误 | 缺字节/哈希/mount/错误/真实退出，或每会话 60 秒超时 |
| `cargo xtask test-host` | frame/buddy/页表、VFS/pipe/FD、调度、archive/parser、记录判定 | 测试失败传播 |

smoke 支持 boot/panic/timeout/invalid-opcode/general-protection/page-fault/double-fault/
stack-guard/read-only/no-execute/direct-no-execute/unmapped/oom；`--timeout-secs` 为 1..300，
默认 30 秒。verify 每项 30 秒（含故意挂起），较大 M9 镜像仍须真正到达注入点才能分类。
期限限制 QEMU 进程，编译/下载另有自己的失败行为。

## target 和镜像边界

alias 运行宿主 xtask，不配置全局 build.target，default-members 排除内核，避免误作宿主构建。
内核 build.rs 拒绝错误 target 并选择独立链接脚本；裸机为 soft-float/no SIMD、kernel code
model、无 red zone，不链接宿主 libc/系统 allocator/std，M3 提供 buddy allocator。
M4 独立用户 workspace 使用 freestanding x86_64-unknown-linux-gnu core/SSE2 和自己的
startup/linker，不使用 libc/Linux syscall，不能套用内核参数。

链接器保留 delimiter 间可写 Limine requests，在 `0xffffffff80000000` 分离 RX/R/RW。
build 检查 ELF class/endian/machine、ET_EXEC、program-header 边界、segment 范围/权限及入口，
拒绝 PT_INTERP/PT_DYNAMIC、RWX。ISO 仅 UEFI El Torito，不安装 BIOS、不访问真实块设备；
固定产物路径拒绝 symlink/设备文件。QEMU 基础为 q35/TCG、qemu64、256 MiB、单核、无网络与
现代 virtio 数据盘；CODE 只读，VARS 在独立 `build/tmp/` 副本中，正常结束删除。
默认数据盘同样临时，显式持久盘及测试位于 `build/disks/`。看门狗 kill/reap guest；
ext2 无日志与恢复限制见 M7。

## 验证

```sh
cargo fmt --all -- --check
cargo fmt --manifest-path user/Cargo.toml --all -- --check
cargo xtask check-files
python3 scripts/check_docs.py
cargo clippy --locked -p xtask -p anova-platform -p anova-memory -p anova-abi -p anova-elf -p anova-sched -p anova-vfs -p anova-storage --all-targets -- -D warnings
cargo xtask check-apps
cargo xtask apps
cargo clippy --locked -p anova-kernel --target x86_64-unknown-none --features qemu-test -- -D warnings
cargo clippy --locked -p anova-kernel --target x86_64-unknown-none --features test-double-fault -- -D warnings
cargo clippy --locked -p anova-kernel --target x86_64-unknown-none --features test-no-execute -- -D warnings
cargo xtask test-host
cargo xtask build
cargo xtask fetch
cargo xtask doctor
cargo xtask verify
cargo xtask terminal-test
cargo xtask persistence-test
```

不要对硬件内核直接 `cargo test --workspace`，也不要同时启用多种 fault injection。
内核没有宿主测试 harness。通过要求有序 boot/memory/CPU/timer 标记、8 次以上一致纳秒计时
IRQ、两种 IF 状态 breakpoint 返回、`TEST:PASS`、QEMU 33。
isa-debug-exit 为 `(value << 1) | 1`：0x10→33 成功、0x11→35 panic、0x12→37 exception，
退出 0 不算通过。仅测试 feature 访问该设备，普通启动停在交互 shell，阻塞输入交给可中断
STI/HLT。panic 独立有界输出 phase/源码位置/消息，异常输出寄存器、CR2、错误和实际 IST；
gate 逐项验证预期注入，不能把异常视为成功。IRQ/映射/时间限制见 [M2](../milestones/M2_CPU.md)。
固件/引导记录在 `*.serial.log`，进程消息在 `*.qemu.log`，case/退出/超时/分类在 `*.result.txt`；
单个启动 banner 永远不足以通过。

## CI 与证据

[boot.yml](../../../.github/workflows/boot.yml) 在 Ubuntu 24.04 运行基础检查、13 项 QEMU、
双启动串口及持久/设备错误回归，失败仍上传日志；Actions 固定提交。
配置 CI 不等于远端执行通过。最新历史证据截至 M15.6，见[验收索引](../index.md#验收记录)；
[M7](../validation/M7.md)、[M6](../validation/M6.md)、[M5](../validation/M5.md)、
[M4](../validation/M4.md)、[M3](../validation/M3.md)、[M2](../validation/M2.md)、
[M0/M1](../validation/M0-M1.md) 保留早期验收。
引导变更记录基线、宿主/工具版本、输入/产物哈希和实际命令结果；debug 路径、ISO 时间戳仍会
影响产物。同步双语 README 与状态。

## M8 静态 Linux 工具

先读 [M8](../milestones/M8_LINUX.md)。apps 还需 musl-gcc/make，首次获取固定 BusyBox 修订需
Git/网络；日志/配置/源码包留在 `build/`。`cargo xtask trace-compat` 使用 Linux 上的
`qemu-x86_64 -strace` 记录真实调用，`cargo xtask test-compat` 执行包含 Linux 证据的完整启动
回归，UART 测试也运行 musl/ash。其他 applet 未验收；M9 单独增加有界信号/终端能力。

## M9 runtime

[M9](../milestones/M9_RUNTIME.md)定义 musl 1.2.4 哈希、`ANOVA_MUSL_LIBC`、资源和 flags。
apps 构建 static/PIE/dynamic 线程/信号/TTY 探针、故意缺解释器 ELF、两个 TLS DSO 和实际
解释器，initramfs 打包至 `/bin`/`/lib`。trace-compat 分别记录三种 M9 配置及 M8；同架构 exec
子进程可能离开 QEMU-user，记录说明该边界。`M9_HOST` 标识 Linux 比较中允许 W+X 的策略。
QEMU-user 8.2.2 在故意 malformed sigreturn 样本中 abort，仅该宿主模拟用例标记不可用；
原生 Linux/系统 QEMU 独立执行，guest gate 始终要求正确 signal 结果。
verify/test-compat 要求三种 runtime 成功、精确 frame/heap/task/FD 清理、真实耗尽回滚和
缺库/解释器错误；terminal-test 通过 UART 运行动态 musl 并给可中断 read 发送真实 Ctrl+C。
持久化仍保留原受控保存/关机/重启及设备错误要求，见 [M9 证据](../validation/M9.md)。
多镜像命令仅构建一次用户态，保存 SHA-256 快照，每次复用前重验 app/runtime/archive；
改变/缺失立即失败，独立调用仍正常重建，避免反复 BusyBox 构建又接受陈旧产物。

## M10 受控网络和软件包

额外需要 OpenSSL、Python 3 标准库、readelf；apps 获取精确 Mbed TLS 2.28.10 提交，
Cargo.lock 固定 smoltcp。须安装 static/shared musl 输入，缺共享 libc 会在错误 TLS DSO
打包前失败，见 [M9](../milestones/M9_RUNTIME.md)。

```sh
cargo xtask network-test
cargo xtask package-test
cargo xtask run --network --disk build/disks/work.ext2
```

network-test 使用宿主 loopback 18080–18082；package-test 的 Python HTTPS peer 使用
18083/18085，18084/18086 留作未绑定负例。冲突明确失败，不修改宿主网络。
依据 watchdog、syscall、真实 X.509 flags 和 clean e2fsck 判定；日志为 `build/logs` 下
`network.*`/`packages-*.serial.log`。CA 私钥只留在忽略的 `build/compat`，
联网前读 [M10](../milestones/M10_NETWORK_PACKAGES.md)。

## M11 离线系统内 C/C++

`cargo xtask toolchain-build` 从锁定源码构建 GCC/G++、binutils、make、musl/CRT/C++ sysroot，
额外需 Python 3.12+、GCC/G++、make、m4、bison、flex、texinfo。
`ANOVA_BUILD_JOBS=1..8` 控制并行，默认 6；`toolchain-image` 独立验证/打包，`toolchain-test`
执行真实 UART 编辑/构建/运行/重启。

```sh
cargo xtask root-image --linux build/disks/devel.ext2
cargo xtask run --toolchain --disk build/disks/devel.ext2
```

正常关机保留源码/ELF。配置为 1 GiB、单 CPU、只读 `/tc` 与 Linux personality 数据盘；
普通 run 仍为小镜像。编辑命令/能力/哈希/排除范围见 [M11](../milestones/M11_TOOLCHAIN.md)。

## M12 离线系统内 Rust/Cargo

`rust-build` 验证 M11 并准备锁定官方 musl Rust 1.90.0 二进制、配套 std/rust-lld、itoa vendor。
`rust-image` 独立验证打包 `build/images/anova-rust.iso`；`run --rust` 用 3 GiB。
`--rust`/`--toolchain` 互斥，保留工作使用显式 Linux 盘并 guest shutdown：

```sh
cargo xtask root-image --linux build/disks/rust-devel.ext2
cargo xtask run --rust --disk build/disks/rust-devel.ext2
cargo xtask rust-cross-test
cargo xtask rust-runtime-test
cargo xtask rust-test
```

三 gate 分离宿主交叉构建、真实编译器启动、UART 编辑/guest 编译/运行/重启。
完整 gate 检查 std、过程宏 `.so`、frozen vendor Cargo、build.rs、C FFI、freshness/修改、真实
parser 失败、持久源码/ELF 哈希、e2fsck；保留全部 M9/M10/M11 回归。
来源和边界见 [M12 Rust](../milestones/M12_RUST.md)、[M12 证据](../validation/M12.md)。

## M12 可选在线 Cargo

普通 cargo 默认离线，cargo-online 使用同一官方二进制和持久 Cargo home：

```sh
cargo xtask rust-image
cargo xtask root-image --linux build/disks/rust-online.ext2
cargo xtask run --rust --network --disk build/disks/rust-online.ext2
```

在 `/persist` 项目执行 `cargo-online build`、`cargo-online build --locked`；无 NIC 的独立
重启可用 `cargo-online build --offline --frozen --target-dir rebuilt` 从缓存重编译，须正常关机。
`rust-online-test` 提供确定性 HTTPS sparse registry/Git smart-HTTP，检查严格证书/传输失败、
guest 编译、FS 完整性和全新离线重编译；需 Python/OpenSSL/Git 及既有工具。
`rust-public-test` 独立下载固定 crates.io/GitHub，不替换来源；可选 `ANOVA_CARGO_TEST_PROXY`/
`ANOVA_CARGO_TEST_CA` 只描述可达 HTTP 代理/公开 PEM，凭证私钥不能打包。
CI 保留确定性 gate，公网 gate 依赖外部可用性；配置见 [M12 网络](../milestones/M12_NETWORK.md)，
实际结果见 [M12 证据](../validation/M12.md)。

## M13 应用与诊断

[M13](../milestones/M13_SDK.md)提供 create/build/pack/boot/install/upgrade、native/Linux 盘策略、
稳定错误和符号化。已实现 `sdk`、`app-new`、`app-build`、`app-pack`、`app-image`、`app-run`、
`symbolize`、`sdk-test`、`app-test`、`package-v2-test`、`diagnostics-test`。
需 GCC/G++、binutils 的 readelf/strip/addr2line；源码模板保留 debug，紧凑包 payload 哈希独立。
内核资源日志为全局计数，不是进程计量；四项新 gate 与 M0–M12 回归共同保留，CI 单独记录。

## M14.1–M14.2 软件显示与输入

```sh
cargo xtask graphics-image
cargo xtask graphics-test
cargo xtask input-test
cargo xtask run --graphics
```

graphics feature 在 `build/images/anova-graphics.iso` 绘制 CPU 图案，普通串口/编译器镜像保持
独立。交互需 GTK，COM1 仍连接标准输入输出；该 graphics 配置仅用于普通恢复镜像。
headless graphics-test 用 standard VGA/QMP screendump 对比全部像素，无 VGA 仍须串口恢复。
此 M14.1 镜像不提供后续 M14.4 的文字/图形终端，见 [M14 设计](../milestones/M14_GUI.md)、
[M14.1 证据](../validation/M14.1.md)。

同一配置初始化 QEMU i8042 scan-code set 1 键盘/ID-0 三键鼠标。scheduler 每轮最多 64 字节/
64 队列事件，IRQ 仍屏蔽。物理输入只到诊断消费者，不自动成为 shell 文本。
游标固定 7×9 保存/恢复、四边裁剪，无堆分配；首次 pointer 事件才出现，以保持启动参考图。
input-test 构建 `build/images/anova-input.iso`（graphics,input-test），每项 watchdog 60 秒，
覆盖 standard VGA、`-vga none`、`-machine i8042=off`。QMP 注入有序键/重复/扩展/修饰键、
指针及三按钮；比较中心/四角全部 480,000 像素与恢复背景，检查溢出、畸形输入、消费者
断开/重连、独立 PM-timer 半序列超时、真实端口禁用/重初始化。每次启动要求 COM1 恢复和
ACPI 正常关机。`build/input/` 保存 serial/QMP/QEMU/result/PPM/SHA-256，三启动本地通过，
见 [M14.2](../validation/M14.2.md)。CI 保留该目录，但远端执行独立；不证明物理热插拔、USB、
IRQ、实机或此配置中的 GUI 路由。保留 graphics/input/kernel feature 和旧 gate。

## M14.3 原生窗口合成

```sh
cargo xtask gui-image
cargo xtask gui-test
cargo xtask run --gui
```

gui-image 为 `build/images/anova-gui.iso`（graphics,gui）。COM1 输入 `gui-test` 启动独立
合成器/两个 native client；点击焦点、标题拖移、右下角缩放，红窗口 X 故障，F12 故障/重启
合成器，重启后 F11 结束返回 shell。交互 GTK 与 headless gate 分开。
gui-test 生成 `build/images/anova-gui-test.iso`
（graphics,gui,gui-test）；三次启动覆盖 standard VGA/i8042、无 VGA、无 i8042。
仅测试镜像真实耗尽堆并验证连接回滚/复用；QMP 驱动窗口和两类隔离故障，八张截图全像素
对照独立宿主参考，每次要求串口恢复/关机。结果/哈希/serial/QMP/画面位于 `build/gui/`，
CI 归档与远端通过分开。私有复制像素、每客户端一个有界窗口/信箱，不导出设备映射；
内部 syscall transport 与 native v1、M14.4 C ABI 分离。见 [合成器](../milestones/M14_COMPOSITOR.md)、
[M14.3](../validation/M14.3.md)，M14.4/M14.5 各有独立验收。

## M14.4 GUI runtime 与三语言 SDK

独立 `anova.gui.v1` C ABI 由 Rust runtime 实现文字、布局、按钮、文本框、poll 和私有剪贴板；
生成 C、move-only C++ RAII、安全 Rust 共用语义，沿用 M14.3 隔离。
不增加 Linux GUI/native std，见 [runtime](../milestones/M14_RUNTIME.md)、[SDK](../../../sdk/gui/README.zh-CN.md)。

```sh
cargo xtask gui-sdk
cargo xtask gui-sdk-image
cargo xtask gui-sdk-test
cargo xtask gui-sdk-replay # Recheck retained outputs after a successful build gate
cargo xtask root-image build/disks/gui-dev.ext2
cargo xtask run --gui-sdk --disk build/disks/gui-dev.ext2
```

源码发行位于 `build/sdk/anova-gui-sdk-1.0.0`，独立构建前先复制，避免 Cargo 输出污染不可变
release。镜像需 M11/M12，3 GiB/单 BSP/VGA/轮询 PS/2；交互 GTK、headless QMP 分开。
使用 `.anova-abi` 为 `anova.native.v1` 的 native 盘，编译器仍显式为 Linux personality。
在 guest：

```sh
cd /persist
cp /gui/templates/main.c main.c
gui-build c
gui-run /persist/gui-c
```

物理 `a`、按钮、`q` 完成 demo，F11 停合成器返回串口。C++/Rust 复制 main.cpp/main.rs，
运行 `gui-build cpp`/`gui-build rust`，执行 `/persist/gui-cpp`/`/persist/gui-rust`。
`gui-build note`/`gui-build terminal` 接受 note.rs/terminal.rs。图形终端经 GUI 前 helper 和
真实管道执行 `echo TEXT`/`pwd`，Enter 提交、空框 q 结束回收；没有 PTY/ANSI。
GCC/G++ 生成独立 freestanding object，guest rustc 构建 runtime rlib 并链接 native
startup/intrinsics/panic；输出 ABI 遵守 native 盘，无 runtime libc/C++ 异常/动态 std/宿主
预制客户端，编译失败保留旧 ELF。
replay 验证原成功 gate 的 ISO/源码/ELF 哈希，无重编译执行物理输入/scanout 并另记结果，
不能替代初次编译。完整 gate 用真实串口编辑器保存源码、guest 编译、物理输入/画面、真实
page fault、两项 wait 状态和跨关机/重启哈希。**镜像 gate 顺序运行**，并发写会使 build/apps
受检快照失效。宿主/QEMU/未测实机边界见 [M14.4](../validation/M14.4.md)。

## M14.5 有界硬件清屏/读回

```sh
cargo xtask gpu-test
cargo xtask gpu-fallback-test
cargo clippy --locked -p anova-kernel --target x86_64-unknown-none --features gpu -- -D warnings
cargo clippy --locked -p anova-kernel --target x86_64-unknown-none --features gpu-test -- -D warnings
```

两命令构建 `build/images/anova-gpu.iso`（graphics,gui,gpu-test）。完整 gate 需 GTK/OpenGL 和
实际认可硬件 renderer，保留 VGA 恢复，附第二块 virtio-gpu-gl-pci；真实 COM1 启动合成器，
比较离屏像素、注入丢门铃、CPU fallback 和 GPU 复用。fallback gate 无需 GUI/GL，覆盖
2D-only/无 GPU，相同用户接口。在具备环境的 WSLg 宿主，QEMU 子进程选 Gallium d3d12、
Wayland GTK；显式 GALLIUM_DRIVER/GDK_BACKEND 保留，`ANOVA_GPU_DISPLAY` 可选如 Linux 有
DRM render node 的 egl-headless。读取实际设备 capset renderer，拒绝软件/未知；外部 GL
探针或 `/dev/dxg` 存在不算硬件验收。
`build/gpu/` 保存启动结果、命令、ELF/ISO 哈希、宿主图形包版本、serial/QMP 和完整 800×600
PPM 比较。完整/无界面结果分开；gate 顺序运行。普通 CI 只运行 fallback/feature 检查，
硬件需支持宿主显式运行。所有权/延迟/GPU 绘制与 CPU 显示边界见
[M14 GPU](../milestones/M14_GPU.md)、[M14.5](../validation/M14.5.md)。

## M15 受控源码重建

准备 M11 后，`cargo xtask bootstrap-prepare` 创建纯 bzip2 1.0.8 源码盘与测量 ISO，
`bootstrap-test` 自建新盘验证容量、guest 编译、重启/拒绝恢复。保留输出的盘并正常关机。
`build/bootstrap/prepared.json`、`build/logs/bootstrap.result.json` 保留输入/种子/源码/输出哈希
和采样，逐启动日志前缀为 bootstrap-。前置/测量限制见 [M15 配置](../milestones/M15_BOOTSTRAP.md)，
执行范围见 [M15.1/M15.2](../validation/M15.1-M15.2.md)。

### GCC 重建可行性

toolchain-build 后运行 `cargo xtask bootstrap-gcc-test`，验证七份源码缓存，独立工作盘，
C++11 种子、实际缺依赖、选定 configure 入口拒绝、容量恢复和重启。
`python3 ports/bootstrap/test_gcc_probe.py` 测拒绝，`python3 ports/bootstrap/gcc_probe.py --verify`
只读核查准备库存。成功评估可报告阻塞，不产生 GCC；结果在
`build/logs/bootstrap-gcc.result.json`，日志前缀 bootstrap-gcc-，嵌入源码/种子/镜像身份。
见 [M15 GCC](../milestones/M15_GCC.md)、[M15.3](../validation/M15.3.md)。

### Rust/LLVM/Cargo 重建可行性

toolchain-build、rust-build 后运行 `cargo xtask bootstrap-rust-test`。独立清点锁定官方源码
各组件，两次 q35/TCG 启动验证 Rust std/C++17 种子、缺工具 exec、隔离离线依赖拒绝和重启
哈希；不重建编译器/Cargo。`python3 ports/bootstrap/test_rust_probe.py` 检查 archive/cache 拒绝，
`python3 ports/bootstrap/rust_probe.py --verify` 只读核查发布。
结果在 `build/logs/bootstrap-rust.result.json`，嵌入 `build/bootstrap-rust/prepared.json`、
源码审计、精确种子/镜像和 bootstrap-rust- 日志；继承 M12 与上游规定 stage0 的区别见
[M15 Rust](../milestones/M15_RUST.md)、[M15.4](../validation/M15.4.md)。

### 系统内内核重建

锁定 M12 后运行：

```sh
python3 ports/bootstrap/test_kernel_prepare.py
cargo xtask bootstrap-kernel-test
```

准备精确内核源码、离线 registry 和继承裸机库，执行实际 guest Cargo/rustc/rust-lld，
保留 ELF；宿主打包而不重编内核，提取 ISO 比较字节，验证启动、编译失败保留和恢复重启。
结果 `build/logs/bootstrap-kernel.result.json`，每会话日志前缀 bootstrap-kernel-，清单位于
`build/bootstrap-kernel/inputs.json`、`prepared.json`。顺序运行，见
[M15 内核](../milestones/M15_KERNEL.md)：持久文件 4 MiB、tmpfs/cache 1 MiB，完整 guest
镜像生成仍独立。

### 重复干净构建

`cargo xtask bootstrap-repeat-test` 顺序运行 bzip2/内核各两次独立新盘 gate，验证初始空/
纯源码目录，逐轮保存至独立 `build/bootstrap-repeat/` 目录，对比输入/种子/配方身份及完整
输出字节；每轮均包含功能、编译拒绝、重启、FS 检查。
聚合 `build/logs/bootstrap-repeat.result.json`，不匹配保留 comparison.json 并失败。
独占运行镜像 gate；产物/元数据差异/自举边界见 [M15 重复](../milestones/M15_REPEAT.md)、
[M15.6](../validation/M15.6.md)。

## 文档维护

接口、命令、限制、状态或下一步变化时，同批更新双语 README 和对应 docs/en、docs/zh-CN。
历史证据保留原日期/范围；本地、远端 CI、实机结果分开记录。
执行 `python3 scripts/check_docs.py`、`cargo xtask check-files`、`git diff --check`。
脚本验证页面对应、语言导航、本地文件/标题链接，语义一致性仍需人工审查。

## M16 文件系统基础

`cargo xtask foundation-test` 构建普通原生镜像，在 `build/foundation/` 下新建受控 ext4
测试盘。四次真实 TCG 启动验证可写/只读设备上的只读访问、10,001 项目录、20 MiB 普通
文件、200 MiB 稀疏文件、盘上原生执行、正常重启、未知特性/需重放镜像拒绝及串口恢复。
每次正向启动限时 120 秒，负向启动沿用 60 秒门禁。保留准确 ISO/initramfs、原始/拒绝盘、
哈希/result JSON 和 `build/logs/foundation-*.serial.log`；正常停止盘必须通过 `e2fsck -fn`。
`test-host` 包含独立 ext4 镜像与 VFS 缓存压力测试，CI 还执行
`python3 ports/gcc/test_fetch.py`，见 [M16 证据](../validation/M16.md)。

编译器输入在 musl.cc 传输失败后回退到锁定的官方 more.musl.cc 归档，两者使用原有摘要。
可选 `ANOVA_SOURCE_CACHE=/absolute/directory` 按锁定本地文件名供应归档（包括
`bootstrap.tgz`）；坏缓存或符号链接直接失败，不绕过检查。CI 仅缓存受检源码归档，
不缓存生成的编译器产物。旧 ext2 镜像仍为默认可写路线；本只读门禁未验收 ext4 写入、
JBD2 恢复或迁移/构建期间的前台响应。
