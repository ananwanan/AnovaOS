<!-- Record verified capabilities, historical acceptance, current blockers and timely bilingual documentation maintenance. -->
# 实现状态

[English](../../en/guides/STATUS.md) | **简体中文**

## M17.3 文件/编辑器与存储故障本地验收（2026-10-28）

`desktop-files-test` 已通过原两次普通编辑器/PTY 启动；新的
`desktop-files-fault-test` 在独立普通 ext4/JBD2 镜像上通过完整 16 用例/43 启动：
精确 16 KiB 边界和拒绝打开/编辑，真实零/一个 inode/块压力，以及临时文件、
旧副本、发布和清理阶段的真实 virtio 写入/flush 错误。检查真实错误/脏/已保存
像素、精确待保存缓冲区字节、目标/恢复副本及正常关机/重启内容；独立审计确认
32 条干净关机记录及全部预期恢复副本。详见[保存策略](../milestones/M17_FILES.md)
和[已执行验证与保留失败](../validation/M17.3.md)。

真实清理错误暴露 UI 缺陷：不必要的目录刷新覆盖了保存成功/清理错误提示。
客户端现保留该结果和干净状态，并要求检查存储，因为备份可能已经被清理删除。
保存诊断观察实际操作和缓冲区字节，不替换文件系统或设备返回值。

外部故障解除后，既有日志挂载仍故障封闭；通过独立镜像副本、普通重新挂载、
明确重新输入待保存字节及图形重试恢复。未保存缓冲区崩溃恢复、保存期间断电
未验证。M17.4–M17.5、全 GUI 三语言编译流程、编译负载响应、远端 CI 和实机仍
独立处理；资源配额及仅 main 推送的 Actions 策略不变。
## M17.2 真实 PTY 终端已本地验收（2026-10-27）

`desktop-pty-test` 已通过声明的两次 QEMU 启动：真实 Linux-musl ash、物理命令输入/退格、已知 stdout/stderr 内容、子进程状态 23、Ctrl+C 状态 130、真实停止子进程等待、600 行输出期间缩放/SIGWINCH、256 行回滚淘汰、EOF/F5 重启、四 PTY/任务/全局打开描述耗尽，以及精确回收快照。原生 GUI 与 Linux shell/编译器保持显式独立 personality；master 为私有，创建者终止时跨 setsid 回收带标签后代。

图形 shell 使用已有源码构建的 GCC 11.2.0 编译指定 C 源码，以状态 7 运行，正常重启后再次运行。独立 e2fsck 及保存文件/程序/ABI 标记的精确比较均通过。UI 分块异步上传，期间继续处理输入和 PTY I/O。固定预算、实际测量和保留日志/画面见 [PTY 契约](../milestones/M17_PTY.md)及 [M17.2 证据](../validation/M17.2.md)。GUI/native v1 和全局限额不变；可靠图形编辑、更完整作业控制、全 GUI C/C++/Rust、远端 CI 和实机仍独立验收。

## M17.1 受监督桌面已本地验收（2026-10-27）

所有者暂缓 M16 远端收尾，授权在 `feat/m17-desktop` 开发 M17。`desktop-test` 已通过五次 QEMU 启动：宿主构建的三个既有 C/C++/Rust GUI 客户端完成物理输入/焦点/移动、关闭重开、配额、真实客户端/合成器 UD2 故障、新会话重连和三次共同替换限额。直接子进程/Linux 拒绝边界、真实任务/FD 耗尽恢复、独立 COM1 重启、ext4/重启/ext2 精确文件内容及缺设备恢复均通过，修复了绘制中断连未重启的问题。

预声明的 5,000 ms 交互预算通过：选定 C/C++/Rust 操作至画面校验分别为 2,360/1,763/2,988 ms，COM1 恢复最大 72 ms。边界采样最大 625,952 堆字节、额外 4,533 页、52 个 VFS 槽，以及主动 32 任务/32 FD 压力，没有扩大配额。完整保留证据和限制见 [M17.1](../validation/M17.1.md)。远端 CI 与实机未验证；M17.2 现已按上文声明配置本地验收；M17.3 有界文件/编辑器及真实 GUI 存储故障已本地验收；M17.4–M17.5 待完成。

## M16 基础能力进展（2026-10-27）

新 ext4 内核容量门禁已在 `e2db30f` 本地通过：真实堆/VFS/FD/任务/磁盘耗尽与复用、
在恢复后的 64 MiB 盘上干净重建真实内核、精确 ELF 打包、语法错误保留和两次新内核启动。
构建峰值为 9,494,480 堆字节、额外 142,825 页、1,564 个 VFS 槽、14 个任务、
每进程 16 个 FD，以及 6,072 块 / 241 个 inode。
[M16 容量](../validation/M16_CAPACITY.md)保留完整测量、输入身份和原始日志。
`1b63f42` 的 #57 通过核心、源码工具链构建及 guest C/C++，随后因镜像复制了
runner 的 UID/GID 1001 而在 ext4 准入失败。新镜像所有权已本地修复（54 项 xtask
测试与严格 Clippy 通过）。完整远端复验待完成：所有者现仅允许 `main` 推送触发
Actions，关闭 PR/手动和功能分支入口。


M16 开发已统一到 `feat/m16-foundation`，包含基础能力与 M16.4 写支持的全部提交。
后续所有 M16 工作继续在此分支进行，Pull Request 目标为 `dev`。

`feat/m16-foundation` 基于 `dev` `616ff47`，实现受检下载回退/缓存、文件系统后端边界、
按需目录、可回收持久文件节点与受控 ext4 只读 v1；准确范围与检查见
[M16 验证](../validation/M16.md)。默认 ext2 镜像、16 MiB 堆、32 task/fd 和 2,048 缓存槽
保持原有限额。显式启用的无日志 ext4 write-v1 已实现，见 [M16.4](../validation/M16.4.md)。
显式启用的 [JBD2 恢复](../validation/M16.5.md)及[新盘迁移/构建](../validation/M16.6.md)
已按声明的 bzip2 配置本地验收，包含前台串口延迟预算。声明的 ext4 内核容量也已本地验收；完整远端 CI 待完成。ext2 仍为默认。

计划于 2026-10-26（Asia/Shanghai）对照已同步 `dev` 的 `05a5fb1` 复核，其中包含 M15.6
交付 `16c0ddf`。以下验收记录对应各自实际执行版本；M15.6 使用实现基线 `55e3751` 及保留
源码快照。此次文档审查没有重新运行运行时/QEMU 门禁，也不证明远端 CI 通过。
历史 M15 任务分支为 `feat/m15-bootstrap`。M15.1/M15.2 有界 bzip2 重建本地通过；
M15.3 GCC 与 M15.4 Rust/LLVM/Cargo 可行性评估及 guest 探针本地通过，编译器重建仍被依赖/
容量阻塞；M15.5 guest 内核重建、精确保留 ELF 的宿主打包和启动/恢复本地通过；M15.6 重复
构建也有独立证据。远端 CI、发布与本地验收分开记录。

设计和证据：[M15 bootstrap](../milestones/M15_BOOTSTRAP.md)、[M15.1-M15.2](../validation/M15.1-M15.2.md)、
[M15 GCC](../milestones/M15_GCC.md)、[M15.3](../validation/M15.3.md)、[M15 Rust](../milestones/M15_RUST.md)、
[M15.4](../validation/M15.4.md)、[M15 kernel](../milestones/M15_KERNEL.md)、[M15.5](../validation/M15.5.md)、
[M15 repeat](../milestones/M15_REPEAT.md)、[M15.6](../validation/M15.6.md)。
图形设计和验收见 [M14 GUI](../milestones/M14_GUI.md)、[M14.1](../validation/M14.1.md)、
[M14.2](../validation/M14.2.md)、[M14.3](../validation/M14.3.md)、
[M14.4](../validation/M14.4.md)、[M14.5](../validation/M14.5.md)。

## M16–M18 后续计划（2026-10-26）

项目所有者确定顺序为 **资源/VFS/ext4 → 可用开发桌面 → 有条件的编译器/镜像自举**。
当时的计划变更没有实现；上文 M16 进展已替代其未实现状态。M17.1 已通过有界本地验收，见上文；M17.3 有界文件/编辑器及真实 GUI 存储故障已本地验收；M17.4–M17.5/M18 待完成，M16 远端 CI 待验。

| 计划 | 范围 | 下一前置条件 |
| --- | --- | --- |
| [M16 基础能力](../milestones/M16_FOUNDATION.md) | CI/预算、后端解耦与资源、ext4 只读、可写、JBD2 恢复和新工作盘采用 | 完成远端 CI；JBD2、迁移、bzip2/串口和 ext4 内核容量已本地验收 |
| [M17 开发桌面](../milestones/M17_DESKTOP.md) | 受监督会话、PTY 终端、文件/编辑器、诊断和全 GUI C/C++/Rust 流程 | M16 工作盘/资源验收；使用继承 guest 编译器 |
| [M18 自举](../ROADMAP.md#m18有条件的自举) | 独立编译器组件阶段、guest 独立镜像生成和重复恢复 | M16/M17 后各组件工具、源码闭包、stage0/依赖和实测容量 |

ext4 是中期工作盘格式计划；保留 ext2 旧镜像和恢复门禁。全新 AnovaFS 格式不进入此关键
开发路径。无论选择哪种磁盘格式，VFS 节点/缓存策略、堆与任务上限均需单独修改。
历史 M14 图形终端仅支持 echo/pwd，M17.2 另行提供有界 PTY/ANSI 开发终端。完整编译器自举、SMP
和实机驱动不作为开发桌面的前置条件。

2026-10-26 核对远端基线：[提交 `05a5fb1` 的运行 40116492704](https://github.com/ananwanan/AnovaOS/actions/runs/40116492704)
结果为 `boot=success`、`guest-toolchain=failure`。源码构建步骤下载 musl.cc 种子工具链时
HTTPS 连接超时（`curl` 退出 28），后续 guest 工具链、自举和 GUI SDK 步骤被跳过。
这是宿主依赖获取失败，不是 guest 内核故障证据；它不撤销历史本地验收，也不证明远端通过。
M16.1 应恢复受检输入获取并实际执行后续门禁，不降低检查要求，不把 skipped 视为成功。

本次计划交付用 `python3 scripts/check_docs.py` 检查双语覆盖、语言导航和本地链接，
用 `cargo xtask check-files` 检查用途清单，并检查 workspace/user 格式及 `git diff --check`。
这些本地检查已通过；文档变更未重跑内核/QEMU 门禁。后续远端 CI 结果须按实际提交核对。

## 文档维护（2026-10-25）

开发文档按 `docs/en` 与 `docs/zh-CN` 镜像组织，根目录保留双语 README 和语言导航。
[DEV-003](REQUIREMENTS.md) 与 [AGENTS](../../../AGENTS.md) 要求实现同批更新相关 README、
架构/ABI、路线图、状态、命令和证据。已完成内容应移出下一步列表，历史测量和未执行能力
保留其边界。文档及其引用方迁移不会改变下述历史验收结果。

本批包含 67 组对应文档；加上根目录/组件 README 与 AGENTS，共 143 个 Markdown 文件。
`python3 scripts/check_docs.py` 已通过双语覆盖、语言导航、本地文件/标题链接检查；
独立负例能拒绝缺译文、失效文件/标题和缺语言链接。用途清单覆盖 422 个现存文件。
`cargo test --locked -p xtask --all-targets` 的 48 项宿主工具测试通过；严格 xtask Clippy、
workspace/用户态格式、修改的 Python 语法和工作流 YAML 解析通过。
这些是宿主与文档检查，没有重新运行内核或 QEMU 回归矩阵。

已实际执行 `cargo xtask sdk`、`cargo xtask gui-sdk`，两个导出均包含双语 README，
并核对源码字节和 SHA-256 清单。musl/网络上游许可正文原样保留，两种语言均保留完整长度的
证据摘要。CI 已加入本地文档检查，但本次工作区修改不声明远端 CI 执行或发布结果。

## 已实现并通过本地验证

- M0–M3：Rust 1.90.0、锁定 Limine/OVMF、q35/TCG UEFI、COM1、BSP 表/IST、故障诊断、
  校准 LAPIC、受管 frame/页表、NX/WP、固定 16 MiB 可回收 buddy 堆；引导/固件保留仍有效。
- M4/M5：隔离 Rust/C ELF、受检 syscall/usercopy、完整启用的 GPR/x87/SSE2、用户态抢占、
  阻塞 wait/sleep/HLT、eager fork、事务 exec/spawn、故障/孤儿、有界槽与精确回收。
- M6：initramfs/tmpfs/devfs、路径/cwd、共享打开描述和偏移、文件/目录、打开后 unlink 生命周期、
  1 KiB 管道、重定向、原生 init/shell、COM1 行编辑；tmpfs 易失。
- M7：实际 PCI/modern virtio-blk 队列与 DMA、扇区/错误检查、flush/reset、`/persist` 单块组
  4 KiB ext2、稀疏/间接文件、append/truncate/unlink、显式磁盘 ABI、sync/fsync、干净 ACPI S5。
  源码编辑和重启逐字节/ELF 哈希、只读/未知特性拒绝、真实 write/flush 错误均有持久化门禁。
- M8：Linux syscall/errno/flags/layout 适配、argv/envp/auxv、musl 1.2.4 静态、BusyBox 1.36.1
  echo/ls/cat/ash 指定管道、CLOEXEC、FS TLS、随机/时间、fork/exec/wait 与负例。
- M9：Linux pthread/clone 共享地址空间/文件/cwd/信号处置，线程 TLS，futex wait/wake/bitset/
  requeue/单调期限、clear-child-tid、组退出/故障、线程 exec 身份转移。
- 实际 rt_sigframe/rt_sigreturn 和 x87/SSE 恢复、信号屏蔽、发送者/故障元数据、EINTR/
  SA_RESTART、ppoll 屏蔽恢复、nanosleep 剩余时间；坏 sigreturn 只结束用户组。
- poll/ppoll、O_NONBLOCK、共享文件状态、四组 PTY、slave 锁、raw/canonical、termios/窗口
  ioctl、前台 Ctrl+C；普通 UART 也启动动态 musl 并中断阻塞 read。
- ET_DYN/静态 PIE/PT_INTERP musl、正确 auxv、initramfs 映射、mprotect/PROT_NONE/W^X、
  fork 共享匿名页、启动共享库、线程后 dlopen/动态 TLS；缺解释器/库明确报错。
- 根/backing/fd/信号处置使用可失败 Shared 控制块；真实堆耗尽验证 fork/spawn 回滚、exec
  保留和精确页/堆恢复。futex backing 身份不复用已释放 allocation 键，用户借用/锁不跨切换。
- M10：可选 modern virtio-net 独立 RX/TX DMA、smoltcp 0.12.0、有界 IPv4 TCP/UDP 客户端、
  fd 继承、非阻塞/poll/超时/信号、UDP 指针/MTU 负例、复用/reset 拆卸。
- 用户 DNS A 实际受控响应，compression/ID/source 边界与超时；已检 CMOS UTC/单调期限。
- Mbed TLS 2.28.10 用户态 HTTPS，强制 CA/域名/日期验证；负例检查真实 X.509 原因，
  测试私钥不入镜像。
- 原生包安装/升级/移除、归档/逐文件 SHA-256、ABI/SDK/license/path 校验、非活动槽失败
  回滚、干净重启、已装 ELF 执行、保留用户数据；在线获取限受控宿主对端/测试 CA。
- DEV-001 继续要求英文用途头与 [文件清单](FILE_PURPOSES.md)，阶段结果见
  [M14.5](../validation/M14.5.md)。历史 M12 验收记录 83 宿主测试、宿主/内核/用户 Clippy、
  格式和 13 QEMU 场景通过；普通 UART/持久化会话直接消费有序 stdout，日志快照不破坏 cursor。

| 能力 | 状态 |
| --- | --- |
| M0–M8 工程、隔离应用、VFS、存储、精选静态 Linux CLI | q35/TCG 本地通过 |
| M9 pthread/futex/信号/就绪/PTY/动态 musl | 在 M9_RUNTIME 内本地通过 |
| 调度池 | 单 BSP，共 32 进程/线程/zombie 槽 |
| 栈/堆 | 64 KiB guard trap 栈、256 KiB guard 调度栈、固定 16 MiB 堆 |
| VFS | 2,048 节点、每 task/table 32 fd、128 打开描述、四组 PTY |
| ext2 | 正常关机持久化，无日志、原子保存或断电保证 |
| 每线程备用信号栈/SA_ONSTACK | M12 有界范围本地通过 |
| 完整终端 jobs、实时队列、robust/PI futex | 未实现/未验 |
| COW、可写共享文件 mmap/writeback、ASLR、SMP、实机 | 未实现/未验 |
| M14.3 隔离软件合成/窗口 | M14_COMPOSITOR 本地通过 |
| M14.4 Rust GUI/runtime 与 guest 三语言客户端 | M14_RUNTIME 本地通过 |
| M14.5 VirGL 有界硬件 clear/readback/CPU 回退 | M14_GPU 本地通过，普通桌面仍软件绘制 |
| M10 IPv4 客户端/原生包/TLS | M10_NETWORK_PACKAGES 本地通过 |
| M11 离线 GCC/G++ 11.2.0、binutils/make/musl/C++ sysroot | M11_TOOLCHAIN 本地通过 |
| M12 guest Rust 1.90.0/std、离线 Cargo/vendor、build.rs/C FFI/固定宏图 | M12_RUST 本地通过 |
| M12 HTTPS sparse-registry/Git/缓存重建 | M12_NETWORK 本地通过，公开源经配置代理 |
| 通用 Linux 网络/glibc、公共原生包源 | 未实现/未验 |
| 外设 IRQ、异步块 I/O、热插拔、引导内存回收 | 未实现 |

BSP 轮询块 I/O，期限两秒；长内核 I/O 延迟任务与交付 IRQ 时间。根仍只读 initramfs，普通 run
使用临时盘。持久数据要显式镜像与 guest 正常关机。编辑器 truncate/write 保存不具原子性，
M9 不改变这些边界。

## M11 已实现并本地验证

以下保留历史 M11 边界，M12 已替代其中 Linux 大小/启动限制，native 边界独立。

- 可选 ISO 在 `/tc` 提供源码构建 GCC/G++ 11.2.0、binutils 2.37、make 4.4.1、musl 1.2.4/
  CRT/libgcc/libstdc++ 11.2.0。
- UART 编辑、guest C stdio/malloc/argv/文件/多源链接；C++17 string/vector/RAII/异常/std::thread；
  make 无操作/选择增量、真实语法/链接失败、临时文件清理、源码/ELF 重启 SHA-256 一致。
- vfork 暂停父进程、共享根并延后父 handler；exec 失败/退出唤醒，tmpfs/ext2 私有快照保留
  偏移，普通文件 mode/umask、资源查询、持久修改时间。
- 历史 Linux ELF 64 MiB、装载 128 MiB、brk 128 MiB、mmap 窗口 512 MiB；Linux 栈/tmpfs
  1 MiB、调度栈 256 KiB、堆 16 MiB、VFS 2,048。历史启动向量 64 项、字符串 1,023 字节、
  每向量 8 KiB payload 和受检 16 KiB startup。
- 源归档锁、完整已装文件清单、逐文件/归档哈希与原许可证；磁盘/initramfs personality 显式，
  开发盘声明 Linux。开发 Rust opt-level 1 含 debug；根页表所属查询不放宽用户边界/guard/W^X，
  保留全部所有权/OOM 门禁。

## M12 已实现并本地验证

- 可选 `/rs`：锁定官方 Rust 1.90.0 musl rustc、内含 LLVM、rust-lld、Cargo/std 与 vendor itoa
  1.0.15。这些二进制有别于 M11 源码构建 C 工具，不证明 Rust/LLVM 源码或自举。
- guest UART 编辑、std 分配/参数/文件、真实线程与工作线程子进程输出/wait、guest 构建宏 .so。
- 无依赖 Cargo、锁定 vendor/checksum 的 frozen 构建、build.rs 输入/环境、真实 guest GCC/ar
  C FFI、无操作/源码变化、真实语法失败、源/ELF 持久哈希与干净重启。
- 固定过程宏图共享 host/build 依赖、生成源码/环境，依赖修改从 42 展开为 84；宏错误 101
  保留旧产物，修复后成功；23 个保存源码/ELF 文件重启一致。
- Linux 开发盘 16 MiB 单块组、512 inode，验收 Cargo 使用 146 inode；native 仍 128 inode，
  旧盘不扩容。
- 每线程 sigaltstack/SA_ONSTACK 递送/返回与拒绝；musl posix_spawn 共享根 vfork、线程组子进程
  等待、detached 自解除栈后退出、非法返回强制 fault。
- AF_UNIX sequenced-packet pair 支持 Rust pre-exec 错误通道，双向消息/背压/截断/复制/EOF。
- 定位 I/O、有界 ftruncate、普通文件 rename 保留被替换打开 inode、OFD flock、pipe FIONREAD/
  FIONBIO；目录 rename 限同父且目标不存在；空目录移除保留打开身份到最后 close，活动 cwd/
  removed-child 引用返回 EBUSY；相对 dirfd open/stat/unlink 与 O_NOFOLLOW 独立检查。
- Linux ELF/装载/每 mmap 上限 256 MiB，窗口 `[0x50000000, 0xc0000000)`，每根已提交 gap hint
  避免重复扫描不可变大映射。Linux 启动 argv/env 256 项、检查 32 KiB block；native 64 项。
  Linux/VFS 路径/名称 512/255 字节；native v1 wire 容量独立，不可表示名称不消费记录。
- Rust QEMU 3 GiB，仍单 BSP/32 槽、16 MiB 内核堆、1 MiB tmpfs/Linux 栈、4 MiB 持久文件、
  256 KiB 调度栈、保留引导内存。可写共享文件 mmap/writeback 不支持，见只读映射说明。

## M12 在线 Cargo 已实现并本地验证

- `cargo-online` 使用未修改官方 Cargo 1.90.0、严格 TLS、锁定公共 CA、`/persist/cargo-home`，
  保留显式工程/环境配置；普通 Cargo 默认离线。
- 受控 HTTPS sparse registry/Git smart HTTP、完整 revision/crate checksum、实际 CA/域名/
  过期失败、损坏/截断传输拒绝、超时、修复重试、guest 编译。
- 公开 crates.io itoa 1.0.15/GitHub dtolnay/itoa 全 revision，不替换源码；本地经宿主代理/
  CA，不证明直接公网 DNS/传输。
- 两门禁保存源/缓存/锁文件/ELF 哈希，e2fsck 及无对端/NIC 重启后新 target 依赖重建。
  宿主对端/代理只运输源码。
- select 实际就绪/单调期限/集合/EINTR；真实 utimensat/futimesat 与 ext2 时间戳。
- 只读文件 MAP_SHARED 一致性、inode 跨 fork/close/unlink/替换生命周期、真实只读写 fault、
  堆 OOM/复用；可写映射和截断活动映射文件拒绝。嵌套 ext2 缓存改迭代导入，在节点限额内，
  不再受原八层递归限制。
- 历史 M12 记录 83 宿主测试、228 文件用途、严格三目标 Clippy、格式、13 QEMU 场景及此前终端/
  存储/网络/包/C/C++/Rust 门禁通过；完整结果见 [M12](../validation/M12.md) 与
  [联网设计](../milestones/M12_NETWORK.md)。确定性联网门禁加入 CI，远端执行另计。

## M13 已实现并本地验证

- SDK 1.0.0 冻结 native v1 编号/布局/错误，导出正常用户参数的独立 C/C++/Rust 模板。
  Rust 锁定本地依赖/类型化错误，C++ move-safe fd 所有权，参数助手遵循 64 项/1,023 字节。
- ANOVAPK2 记录 ABI/架构/入口/有界依赖最低版本/权限和能力准入，staging 前验哈希/ELF；
  拒绝磁盘 native/Linux 不匹配，包括 Linux 盘上的旧 native v1 包。ABI marker 不可写/
  截断/chmod/unlink/替换/rename。
- 已装 C/C++/Rust/静态 musl Linux 实际执行；31 个重新哈希 parser 负例和无写控制、重建 C
  升级、staging 写失败、降级拒绝、四启动/e2fsck、用户数据保留。
- 公开 create/build/pack/image/run 支持外部哈希锁定自有包；真实 guest 应用经公共镜像入口
  返回稳定缺文件错误且退出 0。
- 硬件故障产生受检 usercopy 帧追踪、寄存器/资源计数；宿主用确切 debug ELF 符号化五个
  实际函数/源码位置，坏 RBP 安全停止，shell 继续；无 GDB stub/通用 DWARF unwinder。
- 独立 GUI C ABI/C++/Rust companion 后由 M14.4 验收，不属于 M13 原始证据。权限为准入
  元数据，不是沙箱。Linux 包只接收静态 ET_EXEC，较广 M9 runtime 独立。
  见 [M13 SDK](../milestones/M13_SDK.md) 与 [验收](../validation/M13.md)。

## 下一步

下一步为 M17.4 观测与受控恢复，之后推进 M17.5 全 GUI 编译流程。M17.1–M17.3 已通过各自有界本地验收，M16 远端 CI 仍按所有者要求暂缓。

独立后置的编译器路线需补齐实测 POSIX 构建工具、数学库开发依赖、存储/inode/VFS 容量阻塞，再尝试声明范围内
首个编译器源码阶段。guest 独立镜像生成、原生 libc/std、带 GUI 内核重建和更广图形/运行时/
硬件各自独立未验。M15.6 已本地完成，不应继续作为尚未实现的下一步。

M14 五阶段各有本地验收，M14.5 仅硬件离屏 clear/readback 与 CPU 显示复制；shader/窗口 GPU
合成/更多设备独立，桌面仍软件渲染。M15 为独立可选工作流：

- M15.1/M15.2：官方 bzip2 1.0.8、锁定 source-only 盘、资源采样、guest 编译/压缩/解压、越限
  恢复，编译拒绝/重启后源和产物哈希一致。
- M15.3：锁定源码审计与两启动探针；C++11 种子、各数学头文件缺失、九工具 exec 错误、
  原 configure 启动拒绝、容量恢复、持久哈希。首个 GCC 阶段前需 POSIX 工具、数学开发输入、
  明确源码闭包与大文件/盘/inode/VFS 容量。
- M15.4：完整 Rust/LLVM/Cargo/vendor 审计、规定 1.89.0 stage0、真实 Rust std/C++17 种子、
  六缺工具、Python 入口拒绝、隔离离线 curl 依赖失败、真实编译拒绝/重启保留 ELF/工作区。
  没有执行规定 stage0 或重建 LLVM/rustc/Cargo。Rust 回归修复 ext2 缓存时间戳为持久整秒，
  保留 tmpfs 纳秒及原 Cargo 依赖重建断言；聚焦时间戳和完整离线 Rust 重启门禁通过，初始
  失败/修复记录在 M15.4 验证。
- M15.5：三启动 guest Cargo/rustc/rust-lld 构建，源码/离线 crate/继承裸机库锁定；保留
  349,464 字节内核，宿主不重建，逐字节比 ISO 提取 ELF，启动匹配身份；语法拒绝保留旧内核，
  恢复重启后 181 工作区文件不变，各盘 e2fsck 通过。持久文件上限 4 MiB，tmpfs/只读共享
  缓存 1 MiB。每写私有间接表快照让 2 MiB/512 字节 append 用例扇区读从 16,211,368 降为
  175,376，保留顺序及拒绝/恢复。
- M15.6：bzip2/内核各两次独立干净构建、每轮证据、相同输入、压缩产物/完整内核 ELF 逐字节
  一致，12 会话通过；完整 bzip2 工作区清单相同，内核十个日志/缓存/生成/中间文件不同，
  包括 smoltcp 常量顺序，原始差异保留，只验声明最终产物。M15.3/M15.4 历史审计保留原
  1 MiB 容量范围，不声称重新评估了裁剪路线。

保留 `cargo xtask bootstrap-test`、`bootstrap-gcc-test`、`bootstrap-rust-test`、
`bootstrap-kernel-test`、`bootstrap-repeat-test`、四组源码拒绝测试、`bootstrap-metrics`
内核检查、此前所有运行时/存储/网络/C/C++/Rust/Cargo/native SDK/图形门禁、`gpu-test` 和
无头 `gpu-fallback-test`。原生 libc/std、通用 Unicode/IME、超出 M17.2 ANSI 子集的终端行为、共享剪贴板
策略、动态 GUI 服务包、实机和广泛 Linux GUI 兼容均独立立项。

### M15 自举状态

| 阶段 | 目标 | 当前状态 |
| --- | --- | --- |
| M15.1 | 容量/构建环境/持久工作盘 | 原 bzip2 范围本地通过 |
| M15.2 | guest 构建 C/C++ 工具并重启执行 | bzip2/golden sample/重启/失败恢复通过 |
| M15.3 | GCC 可行性及可行时的编译器阶段 | 评估通过，当前阻塞，未重建编译器 |
| M15.4 | 独立 LLVM/rustc/Cargo 可行性及可行源码构建 | 评估通过，当前阻塞，未重建组件 |
| M15.5 | guest 内核真实启动，单列镜像生成边界 | ELF/精确宿主打包/启动/恢复通过，guest 镜像生成未验 |
| M15.6 | 独立干净重复构建、比较、恢复 | bzip2/内核各两轮、声明精确产物、12 会话通过，保留内核中间差异 |

M15.1→M15.2 后按实际前置能力推进 M15.3/4/5；M15.5 可用已验 guest rustc，不以编译器
源码自举为前提。完整 GCC/Rust/LLVM 自举以可行性为条件，未执行内容保持未验。每阶段保留
输入/命令/日志/哈希，M15.6 汇总声明范围。

当前 Rust 开发盘 16 MiB/512 inode，持久文件 4 MiB、tmpfs/共享缓存 1 MiB、VFS 2,048、
内核堆 16 MiB、32 槽；增大 QEMU RAM 不自动解除限制。bzip2 不需原容量扩展，更大目标需
独立测量/解除真实阻塞。边界采样非精确峰值 RSS，磁盘为关机后用量。独立耗尽探针验证 EFBIG/
EMFILE/EAGAIN 与复用。guest 内核编译、宿主打包、guest 镜像生成分开证据，两次相同不代表
普遍可复现或无种子自举。

## M14.1 软件显示——本地验证（2026-10-23）

[DEV-002](REQUIREMENTS.md) 要求 Rust runtime、用户态 compositor/桌面、稳定版本化 C ABI
和三语言 SDK；架构、安全、候选依赖及五门禁见 [M14_GUI](../milestones/M14_GUI.md) 与
[ROADMAP](../ROADMAP.md)。M14.1 至 M14.4 本地通过，M14.5 三启动有界 GPU/回退通过。

可选 graphics 检查完整 framebuffer 保留跨度和 supervisor RW/NX WC 映射后写入；无依赖
no_std 库提供裁剪像素/矩形/blit、24/32 位 RGB 检查与调用者 back buffer。standard VGA
480,000 像素/固定 checksum 与参考相同，设备读回/堆恢复通过；无 VGA 启动保留 COM1。
两启动执行恢复命令/干净关机。门禁和回归见 [M14.1](../validation/M14.1.md)，已有
`graphics-image`、`graphics-test`、`run --graphics`。

仅该诊断不提供用户输入路由/窗口。GUI SDK/GPU 分别于 M14.4/5 验收，不由 framebuffer
诊断证明。CPU 提交无 vblank/原子 page flip；scanout 只验 QEMU 800×600×32，其他格式仅宿主
测试。M12/M13 分别提供 Rust/Cargo 与 SDK/包前置，M14.4 有独立开发验收，SMP/实机/Linux GUI
另外立项。

## M14.2 轮询 PS/2 输入/光标——本地验证（2026-10-23）

可选 graphics 拥有 QEMU i8042 键盘和 ID-0 相对鼠标，显式 set-1、source/ACK 检查、PM 时钟
有界等待/重试。BSP IF=0 每轮最多读 64 字节、消费 64 事件，IRQ1/12 屏蔽，分设备半包超时
先清理再接迟到字节；输入/光标不分配堆。

no_std 解码区分物理按下/释放/重复/扩展键和文本，处理 Pause/PrintScreen、修饰键、九位鼠标
轴及三按钮。坏输入/队列溢出/消费者断开重连丢弃旧历史并清理 held。只有固定诊断消费者，
应用焦点/路由归 M14.3。

7×9 光标保存/恢复最多 63 设备像素，保留未用通道、全访问裁剪；指针事件前不改启动图。
`input-test` 标准 VGA/无 VGA/无 i8042 三启动，QMP 顺序键/按钮/移动，五位置共 480,000
像素比较，要求 COM1 恢复/关机。自检覆盖溢出/坏输入/断连、独立 PM 半包超时及真实端口停用/
重初始化。见 [M14.2](../validation/M14.2.md)；该历史宿主覆盖 149 测试含 22 纯输入测试、
光标 raster 和门禁负例。不包含 USB/物理热插拔/外设 IRQ/实机/图形终端/GUI SDK；M14.3/4 独立。

## M14.3 原生软件合成器——本地验证（2026-10-23）

独立 Rust native 进程管理软件 surface、叠放/damage/移动/resize/可见性/焦点/物理路由。
内核只向不可变 compositor ELF 授 display/input 权，提供认证复制消息/四连接；每客户端一个
私有 32×24 至 320×240 RGB 窗口，无显存或他进程映射。800×600 合成器符合 4 MiB native ELF
限制；native v1 冻结，实验 transport 与 M14.4 公共库 ABI 分离。

`gui-test` 三配置通过，八图共 3,840,000 像素对独立参考；两客户端焦点/QMP、移动/resize/
遮挡重绘；client/compositor 各真实 UD2，核对 PID/vector/wait，存活者输入、新 session
重连、COM1/关机。独立验证真实堆 OOM 回滚、quota、坏 surface/handle、外来所有权、reply
背压/eviction、复制失败保留。

`gui-image`/`run --gui` 为交互配置，COM1 `gui-test` 启监督 demo。见
[M14_COMPOSITOR](../milestones/M14_COMPOSITOR.md) 与 [M14.3](../validation/M14.3.md)。
纯 GUI 库新增 11 测试，历史总数 163；CPU stores 无 vblank/原子多块上传。USB/IRQ/实机/
可写共享 surface/GPU 不在该范围；runtime/文本/控件/终端/三 SDK 和 GPU 各独立验收。

## M14.4 GUI runtime/SDK——本地验证（2026-10-23）

无分配 Rust runtime 实现 `anova.gui.v1` (`0x00010000`)、固定 C 布局、生成声明、进程 generation
句柄、C++ move-only RAII、安全 Rust。裁剪 raster、原创位图字形、布局、按钮/文本框、顺序
poll、私有 UTF-8 剪贴板。溢出保留 resize 并发 RESET；server 丢失失效 handle。不加内核 ABI/std。

`gui-sdk-test` 两次实际 q35/TCG、3 GiB、VGA/PS2，UART 保存七确切源码；guest GCC/G++/rustc
独立客户端绘制/物理输入/退出。Rust note 测 copy，图形终端键入 echo hi 经 GUI 前 helper、
真实管道/原生命令/child wait；故意 GUI 用户页 fault 隔离，后续 client 正常。两启动回收全部
compositor/client 并关机。

28 图通过独立像素/字形抽样，三语言三个匹配阶段全图哈希相同；抽样不说成逐像素全图。
源/native ELF 重启哈希一致，失败编译保留旧 ELF、两 e2fsck 通过。最终 retained-output replay
含终端 presentation ACK/文件哈希不变通过；16 runtime/三个新 gate 宿主测试通过，记录重跑
后 26 选定旧检查通过。原失败、命令、哈希与范围见 [M14.4](../validation/M14.4.md)。

源码 SDK 独立导出/校验/header 检查和 Cargo manifest；CI 有门禁，未声称远端/实机通过。
限制：每 client 一个窗口、ASCII 字形/Unicode 替代框、私有剪贴板、US 文本、无 GUI callback/
线程、有界 echo/pwd 终端、静态链接、无动态 GUI 服务包。GPU 独立。

## M14.5 有界硬件绘制——本地验证（2026-10-24）

可选 modern virtio-gpu controlq 提交真实 VirGL 离屏 render-target clear 与 fenced 读回。
该设备 capset2 报 `D3D12 (NVIDIA GeForce RTX 4060 Ti)`。compositor 专用检查操作最多返回
64×64 RGB，不暴露原始命令/用户 DMA/共享设备映射；独立用户合成器选 CPU 回退，经 VGA 提交。

`gpu-test` 硬件 VirGL/仅2D/无GPU 三启动通过。硬件验证四种64×64色、7×5中间色与输出尾 guard、
校准 lost-doorbell 超时、真实 reset/DMA 回收、CPU 回退、硬件重初始化；五张完整800×600图
对 CPU 参考一致。所有配置要准入负例、compositor 正常退出、COM1恢复/关机；CI 独立无头
fallback 门禁，远端未验。

只验 GPU clear/readback；scanout 复制/普通窗口合成仍 CPU，没有 shader、guest GL/Vulkan、
zero-copy、性能提升、物理 GPU passthrough 或未确认 reset 后恢复证据。见
[M14 GPU](../milestones/M14_GPU.md) 与 [M14.5](../validation/M14.5.md)。

## 本次交付回归检查

本地通过 `foundation-journal-test`、`foundation-migrate-test`，以及原有
`foundation-write-test`、`foundation-test`、`persistence-test` 和 `verify` 全部 13 个启动/
故障场景。`test-host` 通过 224 个测试，最终 xtask 单独复跑 48 个测试全部通过。严格宿主
Clippy、裸机 `ext4-write,bootstrap-metrics` 内核 Clippy、workspace/user 格式、Python/
workflow 解析、双语文档、454 个文件用途覆盖及 `git diff --check` 均通过。以上为本地结果，
远端 CI 已配置但未据此验收；当时 workflow 由 `dev` 推送/PR 或手动 dispatch 触发；当前策略已改为仅 main 推送。
