<!-- Provide the synchronized Simplified Chinese overview, current profiles and development entry points. -->
# AnovaOS

[English](README.md) | **简体中文**

用 Rust 从零开发的 x86_64 操作系统，目标是在系统内编辑、编译、调试、运行应用，
并逐步支持经过验证的现有 Linux 软件。

日常开发以 **`dev`** 为基线，Pull Request 也以其为目标；`main` 保留阶段验收后的稳定快照。
项目仓库：[ananwanan/AnovaOS](https://github.com/ananwanan/AnovaOS)。

后续所有 M16 开发统一在 **`feat/m16-foundation`** 分支进行，Pull Request 目标为 `dev`。


GitHub Actions 暂时**仅在推送 `main` 时运行**，关闭 PR、手动和其他分支入口。
最近一次功能分支 CI 失败及本地验证的镜像修复，与本地容量验收分开记录。

## 当前状态

本地已验收范围覆盖 **M0–M13、M14.1–M14.5、M15.1–M15.6**。
其中 M15.3/M15.4 交付的是带明确阻塞结论的编译器可行性评估，尚未重建编译器。
这些结果均有明确边界，不能据此宣称完整桌面系统或完整自举。

| 阶段 | 本地验证的能力 |
| --- | --- |
| M0–M7 | UEFI/Limine 启动、内存保护、Ring 3、用户态抢占调度、原生应用、VFS/管道/init/shell、现代 virtio-blk、受检 ext2 `/persist`、源码编辑器、sync/fsync、正常关机和重启保留 |
| M8–M9 | 固定 BusyBox 1.36.1 CLI 子集；musl 1.2.4 静态、静态 PIE、动态 ELF；pthread/TLS/futex、信号处理与中断/重启、poll/非阻塞 I/O、四组 PTY、启动共享库与 dlopen/TLS |
| M10 | 可选轮询式 virtio-net IPv4 TCP/UDP、有界 DNS、严格用户态 TLS、校验锁定包的安装/升级/回滚/卸载及用户数据保留 |
| M11 | 源码构建的系统内 GCC/G++ 11.2.0、binutils/make、配套 musl/C++ sysroot；实际编辑/编译/运行、异常、线程与重启保留 |
| M12 | 系统内运行官方 Rust 1.90.0/std/Cargo 二进制工具；离线 vendor/build.rs/C FFI/过程宏；严格联网 registry/Git 和缓存离线重编译；固定公网来源通过配置的宿主代理验证 |
| M13 | SDK 1.0.0、原生 C/C++/Rust 模板、ANOVAPK2 显式 ABI 包、安装策略与实际用户故障的源码符号化 |
| M14.1–M14.4 | 受检 CPU framebuffer、轮询 PS/2 输入/游标、隔离 Rust 用户态合成器、版本化 `anova.gui.v1` C/C++/Rust SDK、系统内独立构建客户端、便笺和有界图形终端 |
| M14.5 | 硬件 VirGL 离屏清屏/读回、受保护的逐像素对照、超时/reset/重新初始化和 CPU fallback；普通桌面仍采用软件合成 |
| M15.1–M15.2 | 容量测量、锁定 bzip2 源码的系统内重建、固定样本精确比较、失败/重启恢复 |
| M15.3–M15.4 | GCC 和 Rust/LLVM/Cargo 独立源码审计与实际 guest 前置依赖探测；编译器重建仍被阻塞 |
| M15.5 | 使用继承工具和目标库在系统内编译真实内核；保留 ELF 后由宿主辅助打包，验证启动身份与恢复 |
| M15.6 | bzip2 和内核各进行两轮独立干净构建，最终产物逐字节一致，归档证据与全部 12 次启动/拒绝/恢复会话通过 |
| M16（有界本地配置） | 受检输入、后端/缓存解耦、受控 ext4 只读/写入及 JBD2 恢复；新盘 bzip2/串口验收，以及堆/VFS/FD/任务/磁盘耗尽恢复后的真实内核重建；远端 CI 单独记录 |
| M17.1 | 受监督桌面、C/C++/Rust 同时输入/窗口、真实客户端/合成器故障、有界重启、资源复用及五次持久化/回退启动 |
| M17.2 | 真实 Linux-musl PTY、有界 ANSI/回滚、Ctrl+C/缩放、实际 GCC 编译、耗尽清理及重启精确字节 |
| M17.3 | 文件浏览、有界源码编辑、保存保护及 PTY 共享工作盘；真实 GUI 大小/inode/磁盘满/写入/flush 故障、恢复副本、重试及正常重启已本地验收 |

证据按阶段收录在[验收索引](docs/zh-CN/index.md#验收记录)，各记录明确实际运行的检查。
已配置 CI、远端 CI 实际结果和本地验收是分别记录的事实。

系统当前为单 BSP、32 个进程/线程槽、内核不可抢占、固定 16 MiB 堆；外设 IRQ 保持屏蔽，
设备 I/O 采用轮询。ext2 可写文件上限 4 MiB，受控 ext4 文件上限 256 MiB，tmpfs 和只读共享文件缓存上限仍为 1 MiB。
ext2 没有日志或断电恢复。通用 Linux/glibc、SMP、USB/实机支持、原生 Rust `std`、
编译器源码重建及完全由 guest 生成启动镜像仍未验证。详见
[当前状态](docs/zh-CN/guides/STATUS.md)和[兼容范围](docs/zh-CN/COMPATIBILITY.md)。

`foundation-capacity-test` 已在新建 64 MiB ext4 工作盘上通过真实资源耗尽/恢复及内核重建。
正常构建峰值为 9,494,480 堆字节、1,564 个 VFS 槽、14 个任务、每进程 16 个 FD，
磁盘分配 6,072 个 4 KiB 块。[M16 容量](docs/zh-CN/validation/M16_CAPACITY.md)保留完整结果与 CI 状态。

## 下一步开发

项目所有者确定的顺序为 **M16 资源/VFS/ext4 → M17 开发桌面 → M18 有条件的编译器及镜像自举**。
M16 已实现后端/缓存、受控 ext4 只读/写入、JBD2 恢复和显式启用的迁移/构建验收。
M17.1 受监督会话、物理输入、真实客户端/合成器故障、资源复用及文件保留已通过本地五启动验收；M17.2 的真实 PTY shell/编译/重启配置也已本地验收；M17.3 文件/编辑器及真实 GUI 存储故障已本地验收；M17.4–M17.5 和 M18 待完成。

1. [M16 基础能力](docs/zh-CN/milestones/M16_FOUNDATION.md)：远端 CI 按所有者要求暂缓；声明的内核容量配置已本地通过，
   更广负载仍需独立定容。JBD2 与新盘迁移已有独立的 [M16.5](docs/zh-CN/validation/M16.5.md)/
   [M16.6](docs/zh-CN/validation/M16.6.md) 本地门禁。后端解耦、按需目录、可回收文件节点和受控
   ext4 只读访问已实现，可写范围见 [M16.4](docs/zh-CN/validation/M16.4.md)，既有证据见 [M16](docs/zh-CN/validation/M16.md)。
2. [M17 开发桌面](docs/zh-CN/milestones/M17_DESKTOP.md)：提供真正的 PTY 终端、可靠编辑、
   文件浏览与诊断，利用现有工具在桌面完成 C/C++/Rust 编辑、构建、运行、修错和重启保留。
3. [M18 后续自举](docs/zh-CN/ROADMAP.md#m18有条件的自举)：满足实测前置条件后，分别重新评估
   GCC、LLVM/rustc/Cargo 和 guest 独立镜像生成；完整编译器自举不阻塞 M17。

保留既有运行时、存储、GUI 和 M15 重建门禁。原生 `std`、SMP、设备 IRQ/异步 I/O、实机和
全新文件系统格式独立推进。[STATUS](docs/zh-CN/guides/STATUS.md) 将已观察到的工具链依赖下载
失败与本地验收分开记录。

所有者已暂缓 M16 远端收尾，授权在 `feat/m17-desktop` 开始 M17。`cargo xtask desktop-build` 构建启动器及宿主构建的既有 C/C++/Rust 客户端，并包含显式 Linux 边界探针；`cargo xtask run --desktop [--disk build/disks/work.ext4]` 选择显式桌面会话。`cargo xtask desktop-test` 验证已验收的有界配置。见 [M17.1 本地验收](docs/zh-CN/validation/M17.1.md)。M17.2 已加入真实 PTY 终端：聚焦启动器按 `t`，`F5` 重开 shell，`F4` 关闭。`cargo xtask run --desktop --toolchain --disk build/disks/pty-work.ext4` 将已验证的 M11 工具加入显式 Linux personality 工作盘。详见 [PTY 契约](docs/zh-CN/milestones/M17_PTY.md)及 [M17.2 证据](docs/zh-CN/validation/M17.2.md)。M17.3 已加入[有界文件/编辑闭环](docs/zh-CN/milestones/M17_FILES.md)；`cargo xtask desktop-files-fault-test` 验证真实 GUI 边界/存储故障、精确恢复副本和重试，见[本地验收](docs/zh-CN/validation/M17.3.md)。

## 构建与启动

宿主使用 x86_64 Ubuntu 24.04 或 WSL2，安装 rustup、QEMU 8.2.2、xorriso、curl、
dpkg-deb、GCC/G++、make、musl-tools、QEMU-user、Python 3 和 e2fsprogs。
仓库固定 Rust 1.90.0、rustfmt、Clippy 与 `x86_64-unknown-none`。
[开发指南](docs/zh-CN/guides/DEVELOPMENT.md)说明完整安装、可选编译器依赖、产物路径和失败行为。

```sh
git clone --branch dev https://github.com/ananwanan/AnovaOS.git
cd AnovaOS
cargo xtask fetch
cargo xtask doctor
cargo xtask build
cargo xtask apps
cargo xtask image
cargo xtask run
```

QEMU 默认使用临时数据盘。需要保留工作时，创建显式数据盘并在 guest 中执行 `shutdown`：

```sh
cargo xtask root-image build/disks/work.ext2
cargo xtask run --disk build/disks/work.ext2
```

镜像、缓存和日志均位于 `build/`；命令仅操作普通镜像文件。内核采用 `no_std` + `alloc`、
UEFI/Limine、四级页表，并分别配置内核/用户态编译参数。
原生 ABI 与 Linux-musl personality 明确区分。

## 开发配置与检查

| 配置 | 命令与范围 |
| --- | --- |
| 系统基础 | `cargo xtask test-host`、`verify`、`terminal-test`、`persistence-test`、`test-compat`、`trace-compat` |
| 网络/软件包 | `cargo xtask network-test`、`package-test`；交互使用 `cargo xtask run --network` |
| 系统内 C/C++ | `cargo xtask toolchain-build`、`toolchain-image`、`toolchain-test`；`run --toolchain` 配合 Linux personality 数据盘 |
| 系统内 Rust/Cargo | `cargo xtask rust-build`、`rust-image`、`rust-cross-test`、`rust-runtime-test`、`rust-test`、`rust-online-test`、`rust-public-test`；`run --rust` |
| 原生 SDK | `cargo xtask sdk`、`app-new <c\|cpp\|rust> PROJECT`、`app-build PROJECT`、`app-pack MANIFEST OUTPUT`、`app-image PACKAGE SHA256`、`app-run PACKAGE SHA256`、`symbolize ELF ADDRESS...`；`sdk-test`、`app-test`、`package-v2-test`、`diagnostics-test` |
| 显示/输入/合成器 | `cargo xtask graphics-test`、`input-test`、`gui-test`；交互使用 `run --graphics` 或 `run --gui` |
| GUI runtime | `cargo xtask gui-sdk`、`gui-sdk-image`、`gui-sdk-test`、`gui-sdk-replay`；`run --gui-sdk` 配合原生数据盘 |
| GPU | `cargo xtask gpu-test` 需要经确认的硬件 GL renderer；`gpu-fallback-test` 不需要图形显示 |
| ext4 可写（显式启用） | `cargo xtask foundation-write-test`；`ext4-image [--linux] build/disks/work.ext4`，再用 `run --ext4-write [--toolchain] --disk build/disks/work.ext4`；[配置与证据](docs/zh-CN/validation/M16.4.md) |
| JBD2 / 新工作盘 | `cargo xtask foundation-journal-test`; `cargo xtask foundation-migrate-test`; [M16.5](docs/zh-CN/validation/M16.5.md), [M16.6](docs/zh-CN/validation/M16.6.md) |
| 文件系统基础 | `cargo xtask foundation-test`：受控 ext4 只读、10,001 项目录、20 MiB 文件、200 MiB 稀疏文件、盘上执行、重启和拒绝测试 |
| 源码重建 | `cargo xtask bootstrap-prepare`、`bootstrap-test`、`bootstrap-gcc-test`、`bootstrap-rust-test`、`bootstrap-kernel-test`、`bootstrap-repeat-test` |

镜像构建 gate 共享受检产物快照，**必须顺序运行**。交互图形需要可用显示环境；无界面 QMP
验收属于独立配置。应用编译确实运行于 AnovaOS 内核之上，但已安装种子工具本身不能证明
已从源码重建编译器。

## 文档

完整对应的开发文档见[英文索引](docs/en/index.md)与[简体中文索引](docs/zh-CN/index.md)，
每页均可切换语言。新增共享图片和图表放在 `docs/assets/` 下。

| 入口 | 内容 |
| --- | --- |
| [AGENTS.md](AGENTS.md) | 强制开发、安全和交付规范 |
| [项目大纲](docs/zh-CN/PROJECT_OUTLINE.md)、[路线图](docs/zh-CN/ROADMAP.md) | 目标、依赖、验收与剩余工作 |
| [架构](docs/zh-CN/ARCHITECTURE.md)、[兼容范围](docs/zh-CN/COMPATIBILITY.md) | 子系统所有权、ABI、限制及受测软件 |
| [开发指南](docs/zh-CN/index.md#开发指南) | 配置、风格、工具链、许可、需求、状态与文件用途 |
| [阶段设计](docs/zh-CN/index.md#阶段设计)、[验收记录](docs/zh-CN/index.md#验收记录) | 截至 M15.6 的验收契约/证据、有界本地 M16/M17.1 及后续 M17 计划 |
| [第三方输入与声明](docs/zh-CN/index.md#第三方输入与声明) | 来源及原样保留的上游许可证正文 |
| [原生 SDK](sdk/README.zh-CN.md)、[GUI SDK](sdk/gui/README.zh-CN.md)、[重建配方](ports/bootstrap/README.zh-CN.md) | 各组件旁的双语开发入口 |

后续每批开发都必须**及时同步文档**：两种语言、双语 README、状态、路线图、命令和验收边界
应在同一批改动中更新。自有标识符、源码注释、日志及提交消息仍使用英文。
每个自有文件需有用途说明并登记清单。

```sh
python3 scripts/check_docs.py
cargo xtask check-files
cargo fmt --all -- --check
cargo fmt --manifest-path user/Cargo.toml --all -- --check
git diff --check
```

原创代码、文档与示例采用 [Apache-2.0](LICENSE)；[NOTICE](NOTICE) 和第三方许可证保留
各自的归属与条款，详见[许可指南](docs/zh-CN/guides/LICENSING.md)。
