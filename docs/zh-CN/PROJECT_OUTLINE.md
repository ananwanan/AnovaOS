<!-- Summarize current delivery levels, architectural decisions, risks and remaining development priorities. -->
# 项目大纲

[English](../en/PROJECT_OUTLINE.md) | **简体中文**

规划始于 2026-10-20，本大纲于 2026-10-25 对照已验收开发基线复核。下文说明职责和方向，
实际实现及验收以 [STATUS](guides/STATUS.md) 和各阶段验证记录为准。

## 1. 愿景与技术路径

AnovaOS 面向个人开发者，先建立可靠的命令行应用开发平台，再扩展桌面体验。Rust 内核通过
独立实现的 ABI 适配层逐步复用选定的 Linux 用户态软件。

依赖顺序为可靠启动 → 资源管理 → 隔离进程 → 文件/终端 → 已测 Linux 程序 → guest
C/C++/Rust 开发 → SDK 与图形。兼容声明必须列出软件、版本、构建选项和已测行为；
能加载 ELF 或笼统的 POSIX 标签不能证明兼容。

## 2. 四个可交付等级

| 等级 | 用户能力 | 必需证据 | 当前受限交付 |
| --- | --- | --- | --- |
| A：可启动内核 | 启动、看日志、定位异常 | QEMU 串口、镜像/依赖版本、解码退出结果 | M0–M3 本地通过 |
| B：应用运行平台 | 运行其他系统构建的应用 | Ring 3、syscall、文件、进程隔离与恢复 | 至 M9 的原生/Linux 范围本地通过 |
| C：系统内开发平台 | 在 AnovaOS 内编辑、编译、链接、运行 | C/C++/Rust/Cargo 完整流程与重启保留 | M11/M12 受控范围本地通过 |
| D：生态平台 | 安装已验证软件、使用 GUI/SDK | 软件版本矩阵、包策略、GUI 输入输出与恢复 | M10/M13 包及 M14 受限范围本地通过 |

C 级本身就有交付价值，不依赖完整桌面、驱动或编译器自举。已验收范围不代表通用生态兼容。

## 3. 功能工作包

### 3.1 工程与引导

锁定 Rust、Limine、OVMF、QEMU 和镜像输入的来源与校验和。提供真实 `xtask`，隔离宿主与内核
目标，普通运行无需 root。UEFI/ELF 引导保留串口/panic 诊断，检查内存图、framebuffer 和
initramfs；回收引导内存前建立所有权。当前仍保留引导器保留内存。

### 3.2 内核基础

GDT/TSS/IDT/IST、异常、ACPI/APIC 和时钟；物理页、页表、地址区间、堆、映射及 DMA 所有权；
用户隔离、NX/W^X、用户访问检查、故障恢复；进程/线程、用户态抢占、同步、期限、信号和
CPU/FPU 上下文。当前只运行 BSP 单核，内核上下文不可抢占。

### 3.3 系统服务

VFS/描述符/挂载及 initramfs/tmpfs/devfs；PCI/modern virtio-blk 和 `/persist` 受限 ext2；
串口 TTY、管道、重定向和有界 PTY；可选 virtio-net IPv4 TCP/UDP/DNS；锁定包安装、sysroot
和源码准备。根仍为 initramfs，已验证正常关机持久化；日志与断电恢复后置。

### 3.4 应用与兼容

受检查 ELF64、argv/env/auxv、TLS 和有界动态链接；原生与 Linux x86_64 syscall personality
映射到共用内核语义；区分 POSIX 库 API 与 Linux 内核 ABI。先静态 musl/精选 BusyBox，
再有界动态 musl/开发工具。glibc、Wine/Windows、浏览器、Electron、商业 CAD、游戏和直接
加载 Linux 内核驱动不属首版目标。

### 3.5 系统内应用开发

编辑器、持久可写卷、shell、临时文件与进程/构建工具；GCC/G++/汇编器/链接器/头文件/CRT/
musl/libgcc/libstdc++；guest Rust std、LLVM 工具、rustc/Cargo 和 C 链接器。离线依赖、
build.rs、proc-macro、C FFI 和诊断独立验收。宿主准备的 guest 编译器可先用于应用开发，
编译器源码重建另行推进。原生 AnovaOS `std` 是独立端口工作。

### 3.6 Rust 图形与 C/C++/Rust SDK

Rust 实现绘制、用户态合成器和 runtime；内核显示/输入/映射/IPC 机制与窗口策略分离。
Rust 实现的版本化 C ABI 支持 C 头文件、C++ RAII 和安全 Rust 封装，共享事件、错误及生命周期。
已分阶段验收软件 framebuffer、PS/2 输入/光标、隔离多窗口合成、runtime/guest 三语言独立
构建，以及可选硬件 VirGL 有界清屏/读回与 CPU 回退。桌面合成仍用软件绘制。scanout 不代表
GPU 加速；通用 GL/Vulkan、SDL/Qt、Wayland/X11、Linux GUI、USB 输入、SMP 和实机须独立验收。
见 [M14 GUI](milestones/M14_GUI.md) 与 [ROADMAP](ROADMAP.md)。

### 3.7 受控源码重建

M15.1/M15.2 测量资源并在 guest 重建锁定 bzip2；M15.3/M15.4 审计 GCC 与 Rust/LLVM/Cargo
源码并实际触发前置条件失败，编译器构建仍阻塞。M15.5 用继承工具/目标库生成真实内核，
宿主打包原样保留的 ELF。M15.6 在独立干净盘各重复 bzip2/内核构建两次，比较完整最终产物，
验证拒绝与重启恢复。编译器重建和完全系统内镜像生成仍待完成。

## 4. 实施顺序与剩余重点

[ROADMAP](ROADMAP.md) 记录 M0–M15 的验收边界及尚未实现的 M16–M18 计划。
M4 已先于 M13 提供最小 SDK，联网也没有阻塞离线编译。下一步先做
[M16 资源/VFS/ext4](milestones/M16_FOUNDATION.md)，再利用现有 guest 编译器完成
[M17 开发桌面](milestones/M17_DESKTOP.md)。先恢复 CI 基线，分别验收 ext4 只读、可写与
崩溃恢复，并保留旧 ext2 镜像。完整编译器重建和 guest 独立镜像生成属于桌面之后有条件
推进的 M18。全新文件系统格式、原生 std、更多 GUI/runtime 兼容、SMP 和实机各自需要验收方案。

## 5. 架构决定

| 决定 | 原因 | 复审时机 |
| --- | --- | --- |
| Rust 模块化单体内核 | 控制复杂度、明确边界 | 稳定基线出现实际隔离需求 |
| 复用 Limine | 聚焦内核/用户态 | 具体引导需求受阻 |
| QEMU 优先 | 日志、设备、故障可重复 | 基准回归稳定后 |
| 先单核再 SMP | 分阶段验证并发 | 单核调度/锁/中断已测 |
| 实测 Linux 子集 | 按实际程序扩展语义 | 新软件 trace 证实需求 |
| 先运行工具再重建 | 提前交付应用开发 | 依赖/容量足够源码自举 |
| Rust GUI 与版本化 C ABI | 单实现、三 SDK、用户隔离 | 协议/语言实测发现不可接受限制 |

## 6. 风险和应对

锁定软件，以 trace 和回归驱动兼容扩展；GUI 不绕过系统/工具链验收。静态工具仍需真实
syscall/TLS/信号/线程/文件。记录锁定输入、校验和、宿主角色和离线来源。临时盘与持久化测试
隔离，保留 flush/错误语义，不宣称断电安全。限制编译后端/并发并注明资源采样边界。
先用 Linux-musl，原生 std 单独立项。分发前跟踪依赖版本、补丁、通知和许可证义务。

## 7. 首版排除范围

参考平台聚焦 x86_64 用户态、虚拟机与开发流程。32 位、完整 Linux 发行版/POSIX 认证、任意
PC 驱动、通用加速桌面图形、完整 Win32、移动平台和生产安全认证后置。边界调整需具体需求与证据。

## 8. 完成规则

每阶段给出源码/依赖身份、可执行入口、可观察结果、负例和限制。以真实正常/失败路径证据
更新 [STATUS](guides/STATUS.md)，明确未执行检查。同批更新双语内容和下一步。里程碑表示能力，
不承诺未验证的日历工期。
