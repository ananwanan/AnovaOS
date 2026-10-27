<!-- Translate every repository file responsibility and document purpose-header exceptions. -->
# 仓库文件用途

[English](../../en/guides/FILE_PURPOSES.md) | **简体中文**

下表列出所有现存的已跟踪文件和未被忽略的新文件。`cargo xtask check-files` 检查清单覆盖及
源码用途头，待完成移动中已删除的旧路径不再参与检查。说明是否准确仍须人工审查。
按 DEV-001/DEV-003 同步维护本清单与英文版。生成 lockfile、JSON 无普通注释语法；
LICENSE/NOTICE 与上游许可原样保留。Markdown 在开头使用英文用途注释。
共享图片/图表在实际加入时登记。

| 文件 | 用途 |
| --- | --- |
| `.cargo/config.toml` | 通过 xtask 路由宿主命令，并将内核代码生成参数限制在内核目标。 |
| `.gitattributes` | 将第一方文本规范为 LF，同时保留二进制镜像和归档。 |
| `.github/workflows/boot.yml` | 对双语文档、宿主/用户/内核、图形/输入/合成和 QEMU 回归实施门禁，保留 GUI 证据并运行独立 guest 工具链任务。 |
| `.gitignore` | 忽略生成的宿主/用户构建、镜像、工具链及本机机密。 |
| `AGENTS.md` | 定义强制仓库安全、验证、交付及同批双语文档维护规范。 |
| `Cargo.lock` | 生成的内核/宿主锁定依赖图与 registry 校验和；不得手工插入注释。 |
| `Cargo.toml` | 定义内核、宿主及共享策略/VFS 包；用户程序使用独立 workspace。 |
| `LICENSE` | 原始 Apache-2.0 法律条款；原样保留，不添加源码用途头。 |
| `NOTICE` | 项目归属和声明；保留现有法律内容。 |
| `README.md` | 介绍 AnovaOS、已验证能力、构建入口及项目目标。 |
| `README.zh-CN.md` | 提供同步的简体中文项目介绍和构建指南。 |
| `boot/dependencies.toml` | 锁定已验证的编译器/QEMU 版本和 bootloader/firmware 来源、哈希；变更须刷新完整性检查并完成全部 M1 验证。 |
| `boot/limine.conf` | 定义 UEFI 内核入口；xtask 追加显式标记的原生 ELF 模块。 |
| `crates/abi/Cargo.toml` | 定义由内核与应用 SDK 共享、无需分配的原生 ABI。 |
| `crates/abi/src/lib.rs` | 定义 native v1 常量、经检查的用户地址及无 padding 的进程/查询布局；这是 AnovaOS 接口，不实现 Linux syscall ABI。 |
| `crates/abi/src/linux.rs` | 转换原生错误及受支持 Linux open flags，避免污染通用子系统。 |
| `crates/abi/src/linux/strings.rs` | 通过 checked copy 回调解码页边界内 C 字符串和指针向量，无需分配。 |
| `crates/elf/Cargo.toml` | 提供不依赖硬件的有界 ELF 验证及原生启动栈构造。 |
| `crates/elf/src/lib.rs` | 映射前验证有界 native ET_EXEC、Linux ET_EXEC/ET_DYN 段、解释器路径和 TLS。 |
| `crates/elf/src/stack.rs` | 在经检查的启动存储中编码有界 native/Linux 参数、环境和可执行文件/解释器 auxv。 |
| `crates/graphics/Cargo.toml` | 为内核和宿主检查构建无依赖 no_std 布局及软件绘图库。 |
| `crates/graphics/src/lib.rs` | 验证有界 RGB 模式，提供固定光标光栅，并在调用者缓冲区内裁剪绘制像素、矩形和位图。 |
| `crates/graphics/src/tests.rs` | 独立于硬件检查 RGB 打包、边界、哨兵、溢出、裁剪、字面量光标形状和确定性图案。 |
| `crates/graphics/src/virgl.rs` | 用整数 UNORM 转换编码有界 VirGL 清屏命令，并提供独立宿主参考测试。 |
| `crates/gui-runtime/Cargo.toml` | 打包无依赖用户态 GUI C ABI 及安全 Rust 库。 |
| `crates/gui-runtime/src/lib.rs` | 实现冻结 GUI v1 FFI、有界控件、UTF-8 存储及 Rust 句柄所有权。 |
| `crates/gui-runtime/src/render.rs` | 裁剪 RGB 图元，光栅化原创 ASCII 字形，并为其他 Unicode 字符绘制替代框。 |
| `crates/gui-runtime/src/runtime.rs` | 管理复制式原生 RPC、私有光栅生命周期，以及有序且丢失时重置的输入投递。 |
| `crates/gui-runtime/src/tests.rs` | 检查 ABI 拒绝、生命周期、渲染、UTF-8、缩放和队列压力恢复。 |
| `crates/gui/Cargo.toml` | 定义 no_std 内部 GUI wire 与软件合成库。 |
| `crates/gui/src/lib.rs` | 暴露有界用户态窗口策略和实验性传输契约。 |
| `crates/gui/src/policy.rs` | 验证私有 surface，实现损伤区、堆叠、移动、缩放和焦点。 |
| `crates/gui/src/tests.rs` | 以纯逻辑测试对抗性 wire 包、所有权、配额、损伤区和输入路由。 |
| `crates/gui/src/transport.rs` | 定义复制式内核消息、硬件事件及 checked scanout 布局。 |
| `crates/gui/src/wire.rs` | 序列化定宽、版本化 GUI 请求/回复，不包含指针或 padding。 |
| `crates/input/Cargo.toml` | 为内核及宿主检查构建无依赖 no_std PS/2 解码和有界诊断事件投递。 |
| `crates/input/src/lib.rs` | 无需分配或 GUI 策略，规范物理 set-1 按键、三字节鼠标包、有界队列/reset/reconnect 语义和裁剪指针状态。 |
| `crates/input/src/tests.rs` | 检查按键/修饰键/特殊序列状态、完整九位位移、畸形/溢出恢复、消费者生命周期和指针边界。 |
| `crates/memory/Cargo.toml` | 封装供内核/宿主测试复用的 no_std 页、堆和页表策略。 |
| `crates/memory/src/frames.rs` | 仅将显式提供的可用 RAM 交分配器；其他内存全部保留，包括空洞和每个 bootloader-reclaimable 字节。 |
| `crates/memory/src/heap.rs` | 实现侵入式 buddy 堆；空闲块自行存储链，分配/合并不分配元数据或调用其他分配器。 |
| `crates/memory/src/lib.rs` | 提供内核与宿主测试共享的分配和页表策略。 |
| `crates/memory/src/paging.rs` | 实现四级内核/私有用户页表、权限检查和表回收；用户根仅借用 supervisor 高半区条目，拥有其低半区表。 |
| `crates/memory/src/shared.rs` | 提供可失败的单线程引用计数元数据所有权，测试准确最终销毁和分配失败。 |
| `crates/platform/Cargo.toml` | 封装独立于硬件的 ACPI 解析和带类型物理/虚拟地址。 |
| `crates/platform/src/acpi.rs` | 按 ACPI 6.6 第 5.2.5、5.2.8、5.2.9、5.2.12 节解码字节，不对 packed firmware 结构建立引用。 |
| `crates/platform/src/clock.rs` | 验证公历 UTC 日期与 Unix epoch 转换，覆盖闰年和边界。 |
| `crates/platform/src/lib.rs` | 提供硬件无关地址类型和有界 ACPI 解码。 |
| `crates/platform/src/power.rs` | 验证受限参考 QEMU PM1a 与常量 ACPI S5 交接，并测试损坏输入。 |
| `crates/sched/Cargo.toml` | 与宿主测试共享有界进程状态、父进程/wait 语义及轮转策略。 |
| `crates/sched/src/lib.rs` | 提供无需硬件或分配的有界 BSP 调度策略及父子生命周期；上下文/资源由内核拥有，本策略不切栈或加锁。 |
| `crates/storage/Cargo.toml` | 声明 no_std checked sector/ext2 crate 及其原生错误依赖。 |
| `crates/storage/src/ext2.rs` | 实现声明范围内的 ext2、checked allocation、模式/时间戳保留、有序写入和正常关机。 |
| `crates/storage/src/ext4.rs` | 读取受控 ext4 extent，并显式接入审计后的无日志可写及有界分配归属。 |
| `crates/storage/src/filesystem.rs` | 定义持久后端能力、稳定元数据及有界目录游标操作。 |
| `crates/storage/src/lib.rs` | 定义自有同步扇区设备、只读/flush/stop 契约及 checked ranges。 |
| `crates/storage/tests/ext2.rs` | 验证盘上 ext2 字节、间接块、稀疏数据、耗尽、重启及 I/O 故障注入。 |
| `crates/storage/tests/ext4.rs` | 比较 e2fsprogs 镜像并注入 extent、几何、分配及只读设备故障。 |
| `crates/vfs/Cargo.toml` | 与宿主测试共享有界内存 VFS、归档、打开文件和管道语义。 |
| `crates/vfs/src/archive.rs` | 以 checked byte ranges 解码项目 ANOVAFS1 归档，不做强制类型转换；条目为父目录优先绝对 UTF-8 路径，负载借用引导 RAM。 |
| `crates/vfs/src/fd.rs` | 管理可失败共享描述符表、close-on-exec 状态及无需分配的空占位。 |
| `crates/vfs/src/file_mapping.rs` | 独立于描述符生命周期保留有界共享文件字节，并为成功写入维护版本。 |
| `crates/vfs/src/foundation_tests.rs` | 压力验证按需查询和 10,001 项遍历、缓存回收、身份、共享偏移与映射。 |
| `crates/vfs/src/lib.rs` | 管理有界内存/ext2 namespace、显式 ABI 继承、与后端一致的时间戳精度、私有映射快照、共享打开文件、管道及 PTY。 |
| `crates/vfs/src/tests.rs` | 测试 VFS 路径/挂载边界、描述符所有权、私有快照、持久时间戳一致性、管道关闭和容量回滚。 |
| `docs/en/ARCHITECTURE.md` | 说明已实现子系统的所有权、隔离、ABI、驱动边界与后续架构（英文）。 |
| `docs/en/COMPATIBILITY.md` | 区分已验证原生支持、Linux 软件兼容子集及需要独立验证的 ABI 功能（英文）。 |
| `docs/en/PROJECT_OUTLINE.md` | 说明项目目标、交付等级、范围、工作包、设计决策与风险（英文）。 |
| `docs/en/ROADMAP.md` | 维护阶段依赖、已验收能力、未完成任务、编译器阻塞与镜像自举条件（英文）。 |
| `docs/en/guides/CODING_STYLE.md` | 定义英文源码规范、用途说明和稳定 Rust 格式要求（英文）。 |
| `docs/en/guides/DEVELOPMENT.md` | 说明可复现宿主安装、目标隔离、全部已实现命令、产物与验证流程（英文）。 |
| `docs/en/guides/FILE_PURPOSES.md` | 逐文件记录用途，解释生成/法律文本的头部例外，保持双语清单同步（英文）。 |
| `docs/en/guides/FIRST_TASK.md` | 保存历史 M0/M1 的实施顺序和初始验收标准，并链接当前剩余任务（英文）。 |
| `docs/en/guides/GITHUB_SETUP.md` | 说明 dev/main 分工、源码克隆、当前开发入口和贡献约定（英文）。 |
| `docs/en/guides/LICENSING.md` | 说明项目 Apache-2.0、贡献条款、上游声明与源码/二进制分发边界（英文）。 |
| `docs/en/guides/REFERENCES.md` | 列出官方规范依据、原始查阅时间及实施时必须固定版本的要求（英文）。 |
| `docs/en/guides/REQUIREMENTS.md` | 记录所有者要求的文件用途、Rust GUI/三语言 SDK 和同批文档维护规则（英文）。 |
| `docs/en/guides/STATUS.md` | 维护当前实现、各阶段证据、已知限制、编译器阻塞与下一步（英文）。 |
| `docs/en/guides/TOOLCHAINS.md` | 区分交叉编译、系统内开发、源码重建、继承种子与完整自举（英文）。 |
| `docs/en/index.md` | 导航完整双语文档、当前范围、开发指南、阶段设计、历史验收与第三方声明（英文）。 |
| `docs/en/milestones/M10_NETWORK_PACKAGES.md` | 定义受控 IPv4 socket、DNS、严格 TLS 与持久软件包契约（英文）。 |
| `docs/en/milestones/M11_TOOLCHAIN.md` | 说明离线系统内 GCC/G++/make、sysroot、容量与编辑/构建/运行流程（英文）。 |
| `docs/en/milestones/M12_NETWORK.md` | 定义可选在线 Cargo、严格 TLS、公网来源和缓存离线重编译边界（英文）。 |
| `docs/en/milestones/M12_RUST.md` | 说明官方 guest Rust/std、离线 Cargo、构建脚本、FFI、过程宏与资源限制（英文）。 |
| `docs/en/milestones/M13_SDK.md` | 定义 SDK/ABI 版本、应用清单、安装升级、诊断及独立 GUI 配套边界（英文）。 |
| `docs/en/milestones/M14_COMPOSITOR.md` | 定义实验 GUI 权限、私有复制 surface、damage、输入与故障重启语义（英文）。 |
| `docs/en/milestones/M14_GPU.md` | 定义有界硬件离屏清屏/读回、同步、DMA 所有权、恢复与 CPU 回退（英文）。 |
| `docs/en/milestones/M14_GUI.md` | 说明五个图形子阶段的实现、体系边界、受测范围与持续验收契约（英文）。 |
| `docs/en/milestones/M14_RUNTIME.md` | 定义已实现 GUI ABI、绘图、事件所有权和三语言系统内构建边界（英文）。 |
| `docs/en/milestones/M15_BOOTSTRAP.md` | 定义有界 bzip2 源码重建、准备来源、资源测量及宿主参与边界（英文）。 |
| `docs/en/milestones/M15_GCC.md` | 定义锁定 GCC 源码可行性评估、实际探测、阻塞和未来编译器重建前提（英文）。 |
| `docs/en/milestones/M15_KERNEL.md` | 定义系统内内核构建、保留 ELF 打包、源码/种子身份与独立镜像生成边界（英文）。 |
| `docs/en/milestones/M15_REPEAT.md` | 定义独立干净重建、归档、精确最终产物比较、差异和恢复边界（英文）。 |
| `docs/en/milestones/M15_RUST.md` | 分别定义 Rust/LLVM/Cargo 源码可行性、规定 stage0、依赖阻塞与重建前提（英文）。 |
| `docs/en/milestones/M16_FOUNDATION.md` | 规划桌面之前的实测资源、文件系统后端解耦、受控 ext4 与经验证的新盘迁移。 |
| `docs/en/milestones/M17_DESKTOP.md` | 规划受监督开发桌面、PTY 终端、可靠编辑、诊断及图形开发验收。 |
| `docs/en/milestones/M2_CPU.md` | 定义 CPU 表、受保护异常栈、ACPI/APIC 时钟及上下文规则（英文）。 |
| `docs/en/milestones/M3_MEMORY.md` | 定义物理页/页表所有权、栈、堆、权限、保留区与 OOM 回滚（英文）。 |
| `docs/en/milestones/M4_USERSPACE.md` | 定义 Ring 3、ELF、原生 v1 ABI、用户复制、Rust/C SDK 与生命周期（英文）。 |
| `docs/en/milestones/M5_PROCESSES.md` | 定义单核调度、CPU/FPU、阻塞、进程所有权和生命周期 ABI 边界（英文）。 |
| `docs/en/milestones/M6_FILESYSTEM.md` | 定义 VFS、文件描述符/管道所有权、串口、init 与原生 shell（英文）。 |
| `docs/en/milestones/M7_STORAGE.md` | 定义 PCI/virtio、受限 ext2、耐久性、镜像与串口源码编辑器（英文）。 |
| `docs/en/milestones/M8_LINUX.md` | 说明静态 Linux ABI、固定 musl/BusyBox 构建与受测 CLI 子集（英文）。 |
| `docs/en/milestones/M9_RUNTIME.md` | 定义有界线程、信号、PTY、动态 musl、共享映射和资源所有权（英文）。 |
| `docs/en/third-party/m10.md` | 记录网络栈/TLS 锁定输入、许可与测试证书私钥边界（英文）。 |
| `docs/en/third-party/m11.md` | 记录编译器/运行库源码、种子、许可与对应源码分发要求（英文）。 |
| `docs/en/third-party/m12.md` | 记录官方 Rust 二进制来源、LLVM/unwinder 声明和公共 CA 许可（英文）。 |
| `docs/en/third-party/m15.md` | 记录 bzip2、编译器评估及内核的源码/种子身份、保留声明与重建边界（英文）。 |
| `docs/en/third-party/musl.md` | 原样保留受测 musl 1.2.4-2 软件包的版权和许可正文（英文）。 |
| `docs/en/third-party/network-licenses.md` | 原样保留固定网络栈及启用传递依赖的上游许可正文（英文）。 |
| `docs/en/validation/M0-M1.md` | 记录 M0-M1 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M10.md` | 记录 M10 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M11.md` | 记录 M11 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M12.md` | 记录 M12 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M13.md` | 记录 M13 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M14.1.md` | 记录 M14.1 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M14.2.md` | 记录 M14.2 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M14.3.md` | 记录 M14.3 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M14.4.md` | 记录 M14.4 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M14.5.md` | 记录 M14.5 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M15.1-M15.2.md` | 记录 M15.1-M15.2 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M15.3.md` | 记录 M15.3 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M15.4.md` | 记录 M15.4 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M15.5.md` | 记录 M15.5 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M15.6.md` | 记录 M15.6 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M16.md` | 记录 M16 部分依赖获取、guest 构建实测、后端/缓存和受控 ext4 只读验收。 |
| `docs/en/validation/M2.md` | 记录 M2 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M3.md` | 记录 M3 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M4.md` | 记录 M4 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M5.md` | 记录 M5 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M6.md` | 记录 M6 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M7.md` | 记录 M7 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M8.md` | 记录 M8 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M8_OPTIMIZATION.md` | 记录 M8_OPTIMIZATION 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/en/validation/M9.md` | 记录 M9 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（英文）。 |
| `docs/zh-CN/ARCHITECTURE.md` | 说明已实现子系统的所有权、隔离、ABI、驱动边界与后续架构（简体中文，与英文对应）。 |
| `docs/zh-CN/COMPATIBILITY.md` | 区分已验证原生支持、Linux 软件兼容子集及需要独立验证的 ABI 功能（简体中文，与英文对应）。 |
| `docs/zh-CN/PROJECT_OUTLINE.md` | 说明项目目标、交付等级、范围、工作包、设计决策与风险（简体中文，与英文对应）。 |
| `docs/zh-CN/ROADMAP.md` | 维护阶段依赖、已验收能力、未完成任务、编译器阻塞与镜像自举条件（简体中文，与英文对应）。 |
| `docs/zh-CN/guides/CODING_STYLE.md` | 定义英文源码规范、用途说明和稳定 Rust 格式要求（简体中文，与英文对应）。 |
| `docs/zh-CN/guides/DEVELOPMENT.md` | 说明可复现宿主安装、目标隔离、全部已实现命令、产物与验证流程（简体中文，与英文对应）。 |
| `docs/zh-CN/guides/FILE_PURPOSES.md` | 逐文件记录用途，解释生成/法律文本的头部例外，保持双语清单同步（简体中文，与英文对应）。 |
| `docs/zh-CN/guides/FIRST_TASK.md` | 保存历史 M0/M1 的实施顺序和初始验收标准，并链接当前剩余任务（简体中文，与英文对应）。 |
| `docs/zh-CN/guides/GITHUB_SETUP.md` | 说明 dev/main 分工、源码克隆、当前开发入口和贡献约定（简体中文，与英文对应）。 |
| `docs/zh-CN/guides/LICENSING.md` | 说明项目 Apache-2.0、贡献条款、上游声明与源码/二进制分发边界（简体中文，与英文对应）。 |
| `docs/zh-CN/guides/REFERENCES.md` | 列出官方规范依据、原始查阅时间及实施时必须固定版本的要求（简体中文，与英文对应）。 |
| `docs/zh-CN/guides/REQUIREMENTS.md` | 记录所有者要求的文件用途、Rust GUI/三语言 SDK 和同批文档维护规则（简体中文，与英文对应）。 |
| `docs/zh-CN/guides/STATUS.md` | 维护当前实现、各阶段证据、已知限制、编译器阻塞与下一步（简体中文，与英文对应）。 |
| `docs/zh-CN/guides/TOOLCHAINS.md` | 区分交叉编译、系统内开发、源码重建、继承种子与完整自举（简体中文，与英文对应）。 |
| `docs/zh-CN/index.md` | 导航完整双语文档、当前范围、开发指南、阶段设计、历史验收与第三方声明（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M10_NETWORK_PACKAGES.md` | 定义受控 IPv4 socket、DNS、严格 TLS 与持久软件包契约（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M11_TOOLCHAIN.md` | 说明离线系统内 GCC/G++/make、sysroot、容量与编辑/构建/运行流程（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M12_NETWORK.md` | 定义可选在线 Cargo、严格 TLS、公网来源和缓存离线重编译边界（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M12_RUST.md` | 说明官方 guest Rust/std、离线 Cargo、构建脚本、FFI、过程宏与资源限制（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M13_SDK.md` | 定义 SDK/ABI 版本、应用清单、安装升级、诊断及独立 GUI 配套边界（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M14_COMPOSITOR.md` | 定义实验 GUI 权限、私有复制 surface、damage、输入与故障重启语义（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M14_GPU.md` | 定义有界硬件离屏清屏/读回、同步、DMA 所有权、恢复与 CPU 回退（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M14_GUI.md` | 说明五个图形子阶段的实现、体系边界、受测范围与持续验收契约（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M14_RUNTIME.md` | 定义已实现 GUI ABI、绘图、事件所有权和三语言系统内构建边界（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M15_BOOTSTRAP.md` | 定义有界 bzip2 源码重建、准备来源、资源测量及宿主参与边界（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M15_GCC.md` | 定义锁定 GCC 源码可行性评估、实际探测、阻塞和未来编译器重建前提（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M15_KERNEL.md` | 定义系统内内核构建、保留 ELF 打包、源码/种子身份与独立镜像生成边界（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M15_REPEAT.md` | 定义独立干净重建、归档、精确最终产物比较、差异和恢复边界（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M15_RUST.md` | 分别定义 Rust/LLVM/Cargo 源码可行性、规定 stage0、依赖阻塞与重建前提（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M16_FOUNDATION.md` | 翻译 M16 资源、VFS、ext4 和迁移计划，保持验收边界一致。 |
| `docs/zh-CN/milestones/M17_DESKTOP.md` | 翻译 M17 开发桌面、图形开发流程与独立恢复计划。 |
| `docs/zh-CN/milestones/M2_CPU.md` | 定义 CPU 表、受保护异常栈、ACPI/APIC 时钟及上下文规则（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M3_MEMORY.md` | 定义物理页/页表所有权、栈、堆、权限、保留区与 OOM 回滚（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M4_USERSPACE.md` | 定义 Ring 3、ELF、原生 v1 ABI、用户复制、Rust/C SDK 与生命周期（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M5_PROCESSES.md` | 定义单核调度、CPU/FPU、阻塞、进程所有权和生命周期 ABI 边界（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M6_FILESYSTEM.md` | 定义 VFS、文件描述符/管道所有权、串口、init 与原生 shell（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M7_STORAGE.md` | 定义 PCI/virtio、受限 ext2、耐久性、镜像与串口源码编辑器（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M8_LINUX.md` | 说明静态 Linux ABI、固定 musl/BusyBox 构建与受测 CLI 子集（简体中文，与英文对应）。 |
| `docs/zh-CN/milestones/M9_RUNTIME.md` | 定义有界线程、信号、PTY、动态 musl、共享映射和资源所有权（简体中文，与英文对应）。 |
| `docs/zh-CN/third-party/m10.md` | 记录网络栈/TLS 锁定输入、许可与测试证书私钥边界（简体中文，与英文对应）。 |
| `docs/zh-CN/third-party/m11.md` | 记录编译器/运行库源码、种子、许可与对应源码分发要求（简体中文，与英文对应）。 |
| `docs/zh-CN/third-party/m12.md` | 记录官方 Rust 二进制来源、LLVM/unwinder 声明和公共 CA 许可（简体中文，与英文对应）。 |
| `docs/zh-CN/third-party/m15.md` | 记录 bzip2、编译器评估及内核的源码/种子身份、保留声明与重建边界（简体中文，与英文对应）。 |
| `docs/zh-CN/third-party/musl.md` | 原样保留受测 musl 1.2.4-2 软件包的版权和许可正文（简体中文，与英文对应）。 |
| `docs/zh-CN/third-party/network-licenses.md` | 原样保留固定网络栈及启用传递依赖的上游许可正文（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M0-M1.md` | 记录 M0-M1 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M10.md` | 记录 M10 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M11.md` | 记录 M11 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M12.md` | 记录 M12 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M13.md` | 记录 M13 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M14.1.md` | 记录 M14.1 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M14.2.md` | 记录 M14.2 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M14.3.md` | 记录 M14.3 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M14.4.md` | 记录 M14.4 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M14.5.md` | 记录 M14.5 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M15.1-M15.2.md` | 记录 M15.1-M15.2 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M15.3.md` | 记录 M15.3 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M15.4.md` | 记录 M15.4 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M15.5.md` | 记录 M15.5 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M15.6.md` | 记录 M15.6 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M16.md` | 提供同步的 M16 部分验收中文证据及剩余阶段边界。 |
| `docs/zh-CN/validation/M2.md` | 记录 M2 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M3.md` | 记录 M3 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M4.md` | 记录 M4 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M5.md` | 记录 M5 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M6.md` | 记录 M6 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M7.md` | 记录 M7 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M8.md` | 记录 M8 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M8_OPTIMIZATION.md` | 记录 M8_OPTIMIZATION 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `docs/zh-CN/validation/M9.md` | 记录 M9 的实际本地执行、命令和产物身份、失败/恢复证据、当时限制与未测范围（简体中文，与英文对应）。 |
| `kernel/Cargo.toml` | 构建 bare-metal 内核及隔离的 QEMU 故障注入配置。 |
| `kernel/build.rs` | 拒绝宿主 target 内核构建，选择仅用于内核的 linker script。 |
| `kernel/src/abi/mod.rs` | 适配原生系统查询；进程生命周期归 task，描述符调用归 fs。 |
| `kernel/src/arch/mod.rs` | 选择已实现的 x86_64 硬件架构边界。 |
| `kernel/src/arch/x86_64/acpi.rs` | 仅读取 q35 BSP 中断基线所需 firmware tables。 |
| `kernel/src/arch/x86_64/apic.rs` | 实现 BSP xAPIC 周期计时器，按 3.579545 MHz ACPI PM timer 校准；外部 I/O APIC 和传统 PIC 在驱动就绪前保持屏蔽。 |
| `kernel/src/arch/x86_64/clock.rs` | 稳定采样 binary/BCD CMOS UTC，并按已校准单调时钟推进 realtime。 |
| `kernel/src/arch/x86_64/cpu.rs` | 提供单 BSP 特权操作；此处不启用 FPU/SIMD。 |
| `kernel/src/arch/x86_64/interrupts.S` | 为 CPU traps、IRQ 和原生 syscall 入口构造统一 saved-GPR frame，用 IRETQ 恢复 checked frame；long mode 总保存 SS:RSP。 |
| `kernel/src/arch/x86_64/mod.rs` | 选择 x86_64 CPU、分页、中断和用户态切换实现。 |
| `kernel/src/arch/x86_64/paging.rs` | 对保留 Limine 页表提供有界 BSP-only 写时复制扩展；M3 使用该 mapper 并将全部活动表克隆到自有页存储。 |
| `kernel/src/arch/x86_64/traps.rs` | 在 IF=0 下分派内核诊断、原生用户异常、Linux 信号、syscall 和 checked usercopy 恢复，并使用 IRETQ。 |
| `kernel/src/arch/x86_64/user.S` | 恢复用户 GPR/FPU 状态并使用自有可信 SYSCALL 栈；每次用户 trap 返回 IF=0 的单 BSP scheduler continuation。 |
| `kernel/src/arch/x86_64/user.rs` | 管理 BSP GPR/segment/x87/SSE2 上下文和原生 SYSCALL/IRETQ；用户 trap 返回可信 scheduler root/stack，内核保持无 SIMD。 |
| `kernel/src/boot.rs` | 借用保留 Limine 数据；M3 仅分配 USABLE RAM，引导器数据、原始栈/页表、模块、firmware 和早期池保持保留；原生 ELF 模块支撑隔离测试，标记归档提供 M6 root。 |
| `kernel/src/drivers/gpu.rs` | 管理可选轮询 modern virtio-gpu 队列、带 fence 硬件读回、reset 和 DMA 隔离。 |
| `kernel/src/drivers/input.rs` | 管理 i8042 初始化/轮询/超时，向获准用户合成器独占交接物理事件，并保留诊断/生命周期检查。 |
| `kernel/src/drivers/mod.rs` | 管理 BSP 设备初始化、持久 VFS 交接及可选 scheduler-polled PS/2 输入。 |
| `kernel/src/drivers/network.rs` | 管理 modern virtio-net split queues、帧/DMA 边界、所有权屏障及 reset/quarantine。 |
| `kernel/src/drivers/pci.rs` | 枚举 PCI 桥和函数，准入有界 modern virtio MMIO capabilities/BAR。 |
| `kernel/src/drivers/virtio.rs` | 管理 coherent split block queue、checked transfers/FLUSH、超时、reset 和 DMA 隔离。 |
| `kernel/src/fs/mod.rs` | 将 checked native user buffers 桥接到共享 VFS 和轮询 canonical terminal；BSP IF=0 调用，阻塞仅保留 syscall frame 副本，不保留锁。 |
| `kernel/src/fs/tty.rs` | 实现有界 COM1/四 PTY line discipline、输入输出队列、配置、前台信号和 hangup。 |
| `kernel/src/graphics.rs` | 验证显示映射，显示/回收引导缓冲，管理诊断光标并复制已检查用户合成器损伤区。 |
| `kernel/src/gui.rs` | 认证 executable/server 和 process/client 身份，复制有界 IPC/scanout 并清理生命周期。 |
| `kernel/src/main.rs` | 启动自有 CPU/内存/设备、可选 checked 软件图形和隔离用户负载。 |
| `kernel/src/mm/lock.rs` | 提供不睡眠 BSP 锁，顺序先 memory manager 后 heap；固定堆不调用 frame allocator，IRQ/NMI/fatal 路径不得获取任一锁。 |
| `kernel/src/mm/mod.rs` | 管理 BSP 内存所有权：checked USABLE frames、全自有页表层级、guarded stacks、可回收 buddy heap 和私有进程内存集成。 |
| `kernel/src/mm/uaccess.rs` | 以页检查和 REP MOVSB 异常恢复复制有界用户字节；IF=0 固定共享映射，复制不跨调度/阻塞边界。 |
| `kernel/src/mm/user.rs` | 拥有共享用户根、引用计数私有/共享页、权限及带 guard 的任务栈。 |
| `kernel/src/mm/user/file_mapping.rs` | 固定只读共享文件映射，并结合物理页和 fork 所有权刷新变化的 VFS 字节。 |
| `kernel/src/mm/validation.rs` | 执行运行时不变量及隔离 QEMU 压力/异常验证。 |
| `kernel/src/net/mod.rs` | 集成有界 smoltcp IPv4 sockets、真实 readiness、可失败缓冲区和延迟 TCP close。 |
| `kernel/src/serial.rs` | 实现单 BSP COM1 输出和非阻塞字节输入，无堆、锁或 firmware 依赖。 |
| `kernel/src/task/bootstrap.rs` | 无需分配或公共 ABI，采样每个 scheduler 实例的页、堆、任务、描述符及 namespace 占用。 |
| `kernel/src/task/diagnostics.rs` | 安全复制有界用户 frame-pointer 链，报告捕获异常寄存器和资源。 |
| `kernel/src/task/linux.rs` | 分派有界 Linux 调用，包括 vfork、modes/umask 和资源查询；内存/线程/信号/terminal 布局委派给对应模块。 |
| `kernel/src/task/linux/io.rs` | I/O 前导入 Linux writev 描述符，后续缓冲异常时保留已完成字节数。 |
| `kernel/src/task/linux/memory.rs` | 转换有界 mmap/brk/munmap/mprotect、真实 PROT_NONE/W^X、私有文件 backing 和共享匿名页。 |
| `kernel/src/task/linux/select.rs` | 实现 checked select 描述符集合、真实 readiness、有限等待、剩余时间及可中断阻塞。 |
| `kernel/src/task/linux/signals.rs` | 转换 signal actions/masks/addressing，构造 AMD64 用户帧并导入 checked sigreturn 状态。 |
| `kernel/src/task/linux/sockets.rs` | 转换 checked Linux IPv4 socket 调用、指针布局、deadline 和可中断帧。 |
| `kernel/src/task/linux/terminal.rs` | 转换 poll/ppoll readiness、terminal ioctls、PTY 控制及有界进程组/session 接口。 |
| `kernel/src/task/linux/threads.rs` | 共享 clone 资源、挂起/唤醒 vfork 父进程，管理 futex wait/requeue/deadlines 和 group/clear-TID 生命周期。 |
| `kernel/src/task/linux/time.rs` | 适配 utimensat/futimesat、dirfd/descriptor 路径、NOW/OMIT 和 checked 时间戳精度。 |
| `kernel/src/task/loader.rs` | 验证/映射 native/Linux ELF 和真实 musl interpreter，构建 checked startup 状态。 |
| `kernel/src/task/mod.rs` | 调度有界 native/Linux tasks、挂起的 vfork 父进程和共享根线程；生命周期、复制和回收在内核页表上执行。 |
| `kernel/src/task/validation.rs` | 验证 loader/syscall/fault 隔离及事务式 process/fork/exec 失败。 |
| `linker/user-x86_64.ld` | 在镜像上限以下将原生静态用户 text/rodata/data 放入独立页。 |
| `linker/x86_64.ld` | 链接高半区内核，保留 Limine requests 并分离 RX/R/RW 段。 |
| `ports/bootstrap/Makefile.anova` | 用 guest GCC 顺序编译八个未修改 bzip2 C 单元，并测试编译拒绝时旧输出保留。 |
| `ports/bootstrap/README.md` | 说明锁定源码选择、guest 构建配方及宿主准备边界。 |
| `ports/bootstrap/README.zh-CN.md` | 翻译锁定源码配方、guest 重建、编译器前置条件评估和宿主准备边界。 |
| `ports/bootstrap/gcc_probe.py` | 验证七个锁定编译器依赖归档，发布源码清单及有界未修改 configure-startup 输入。 |
| `ports/bootstrap/inputs.json` | 锁定官方 bzip2 1.0.8 校验和、源码选择、版本及许可元数据。 |
| `ports/bootstrap/kernel-build.c` | 运行真实 offline guest Cargo 编译，验证并原子发布内核 ELF，测试编译失败时旧内核保留。 |
| `ports/bootstrap/kernel-inputs.json` | 锁定官方 Rust 1.90.0 bare-metal target 归档及继承二进制库的来源。 |
| `ports/bootstrap/kernel_prepare.py` | 快照准确内核源码，隔离元数据、认证 offline registry sources，并准备继承 bare-metal target 库。 |
| `ports/bootstrap/prepare.py` | 验证官方 source archives，安全发布纯源码 offline guest 工作区。 |
| `ports/bootstrap/rust-inputs.json` | 锁定官方完整 Rust 1.90.0 source archive、published checksum URL 和被评估 musl target。 |
| `ports/bootstrap/rust-probe/.cargo/config.toml` | 将隔离 Cargo prerequisite fixture 限制在继承 offline vendor 目录。 |
| `ports/bootstrap/rust-probe/bad.rs` | 触发真实 guest Rust 编译拒绝，同时保留先前成功 seed 可执行文件。 |
| `ports/bootstrap/rust-probe/cargo-dependency/Cargo.toml` | 固定从上游 Cargo 选取的 curl 依赖，作为隔离 offline closure 可用性探针。 |
| `ports/bootstrap/rust-probe/cargo-dependency/src/main.rs` | 为隔离 offline Cargo prerequisite probe 提供最小应用体。 |
| `ports/bootstrap/rust-probe/llvm_seed.cpp` | 用 fold expression、std::optional 和结果 42 测试继承 guest C++17 seed。 |
| `ports/bootstrap/rust-probe/probe.c` | 以 checked child errno/status 报告验证实际缺失 guest 工具和有界 Cargo offline 依赖失败。 |
| `ports/bootstrap/rust-probe/seed.rs` | 用 vector、worker thread、持久文件字节及结果 42 测试继承 guest Rust std seed。 |
| `ports/bootstrap/rust_probe.py` | 验证锁定 official Rust source distribution，发布独立组件清单及有界未修改构建输入和 notices。 |
| `ports/bootstrap/test_gcc_probe.py` | 拒绝危险/篡改编译器 archives、过大/缺失 probe inputs 及已变更或 symlink 输出缓存。 |
| `ports/bootstrap/test_kernel_prepare.py` | 拒绝源码变更、lock drift、危险归档、redirected/corrupt publication 和超长 guest 路径。 |
| `ports/bootstrap/test_prepare.py` | 拒绝篡改归档、危险成员、缺失输入及超出 guest 上限的选中文件。 |
| `ports/bootstrap/test_rust_probe.py` | 拒绝变更 Rust sources、危险 archive members 及已变更/重定向源码发布结果。 |
| `ports/gcc/build.py` | 验证归档，交叉构建 GCC/binutils/make 和 target runtimes，并验证完整已安装文件缓存。 |
| `ports/gcc/build.sh` | 从仓库根进入源码构建配方，使用严格 shell 错误处理。 |
| `ports/gcc/fetch.py` | 通过受检传输回退、缓存准入和原子发布获取锁定源码。 |
| `ports/gcc/inputs.json` | 锁定 compiler bootstrap 和上游 source URLs/hashes；JSON 以显式 purpose 字段代替注释。 |
| `ports/gcc/package.py` | 选择有界源码构建的 C/C++ sysroot、保留许可证，并为每个打包文件计算哈希。 |
| `ports/gcc/test_fetch.py` | 验证传输失败、摘要拒绝、缓存/链接准入和临时文件清理。 |
| `ports/rust/build.py` | 验证归档，隔离 strip 临时输出，发布带清单 guest Rust sysroot。 |
| `ports/rust/inputs.json` | 锁定官方 Rust musl 二进制归档及 vendor itoa registry 校验和。 |
| `rust-toolchain.toml` | 锁定编译器、lint/format 工具及 bare-metal 标准库产物。 |
| `rustfmt.toml` | 使用稳定 rustfmt 选项，使编辑器与命令行风格一致。 |
| `scripts/check_docs.py` | 无需联网检查双语页面覆盖、双向语言导航、用途注释和本地 Markdown 文件/标题链接。 |
| `sdk/README.md` | 说明原生 SDK 发布、checked 32 KiB startup block、模板、独立构建驱动和独立 GUI companion。 |
| `sdk/README.zh-CN.md` | 翻译原生 SDK 发布契约、checked startup 限制、独立模板及 GUI companion 边界。 |
| `sdk/build.py` | 构建独立的原生 SDK 工程，隔离用户程序编译参数，使用离线 Rust 输入并记录显式 ABI 元数据。 |
| `sdk/gui/README.md` | 说明 GUI ABI 版本、所有权、错误、链接规则以及有界 guest SDK 的使用方式。 |
| `sdk/gui/README.zh-CN.md` | 以中文说明 GUI ABI 布局、版本和能力检查、生命周期、错误、链接方式及实际 guest 多语言开发流程。 |
| `sdk/gui/generate.py` | 生成并校验 GUI v1 的固定 C 布局、能力常量和函数声明。 |
| `sdk/gui/include/anova_gui.h` | 声明生成的固定宽度 GUI v1 C 接口，并提供编译期布局断言。 |
| `sdk/gui/include/anova_gui.hpp` | 以不使用异常的 C++ 仅移动 RAII 封装转移 GUI 窗口的唯一所有权。 |
| `sdk/include/anova.hpp` | 用可安全移动的 C++ 所有权封装原生 C 描述符，不依赖 STL 或异常运行时。 |
| `sdk/native-v1.ld` | 在 GNU ld 和 rust-lld 下链接 SDK 用户镜像，分离满足 W^X 的页面并保证装载段有效且非空。 |
| `sdk/templates/c/main.c` | 用 C 验证原生 SDK 版本协商、控制台 I/O 和具有明确诊断的文件缺失错误。 |
| `sdk/templates/cpp/main.cpp` | 验证等价的原生 SDK 行为与 C++ 描述符所有权。 |
| `sdk/templates/rust/Cargo.toml` | 定义导出的独立 no_std Rust 工程，指定本地运行时和离线依赖路径。 |
| `sdk/templates/rust/main.rs` | 在不使用 Rust std 的情况下验证原生类型化错误和本地离线依赖。 |
| `sdk/templates/rust/offline-message/Cargo.toml` | 锁定原生 Rust 离线模板使用的本地自有依赖。 |
| `sdk/templates/rust/offline-message/src/lib.rs` | 通过模板的本地 Cargo 依赖提供 no_std 消息。 |
| `sdk/tests/contract.c` | 断言原生 C/C++ 的 syscall 编号、稳定错误、结构体大小、对齐和字段偏移。 |
| `tests/guest/gui/contract.c` | 使用真实合成器运行独立 guest 编译的 ABI 边界探针。 |
| `tests/guest/gui/contract.h` | 验证实际 C/C++ 版本、UTF-8、生命周期、布局和容量失败约定。 |
| `tests/guest/gui/fault.c` | 故意使 GUI 调用者的指针访问出错，验证进程隔离和资源清理。 |
| `tests/guest/gui/main.c` | 构建处理真实物理按键和按钮事件的独立原生 C GUI 客户端。 |
| `tests/guest/gui/main.cpp` | 构建独立 C++ GUI 客户端，验证移动与 RAII 所有权。 |
| `tests/guest/gui/main.rs` | 构建具有等价可见行为的独立安全 Rust GUI 客户端。 |
| `tests/guest/gui/note.rs` | 提供有界、可编辑的 Rust GUI 便笺，并通过显式操作复制到进程私有剪贴板。 |
| `tests/guest/gui/terminal.rs` | 通过 GUI 初始化前创建的辅助进程运行键盘输入的原生命令，并绘制管道输出。 |
| `tests/guest/m10_https_peer.py` | 仅用 Python 标准库提供受控 TLS 1.2 正常及证书过期测试服务。 |
| `tests/guest/m11/Makefile` | 在 guest 内执行分离 C 编译、链接、无操作 make 和选择性源码重建。 |
| `tests/guest/m11/bad.c` | 故意触发真实 guest C 语法解析失败，要求编译器返回非零状态。 |
| `tests/guest/m11/badlink.c` | 故意引用未定义符号，要求 guest 链接器返回非零状态。 |
| `tests/guest/m11/helper.c` | 提供单独编译的函数，由编辑器修改以验证 make 只重建受影响的部分。 |
| `tests/guest/m11/main.c` | 验证 guest C 的 stdio、分配、参数、文件输出和多源链接。 |
| `tests/guest/m11/main.cpp` | 验证 C++ string/vector、RAII、异常及真实标准库线程。 |
| `tests/guest/m11/runtime.c` | 探测真实 vfork 共享根与父进程暂停、延后信号、快照、文件模式、资源限制和临时文件清理。 |
| `tests/guest/m12/Cargo.lock` | 为固定的离线 guest registry 依赖生成的锁文件，不得手动修改。 |
| `tests/guest/m12/Cargo.toml` | 定义 M12 离线 Rust/Cargo 验收夹具及其声明的行为。 |
| `tests/guest/m12/bad.rs` | 故意触发 guest Rust 语法解析失败，验证非零状态并保留此前产物。 |
| `tests/guest/m12/build.rs` | 运行 guest GCC/ar，生成 OUT_DIR 输入并配置 Rust FFI 链接。 |
| `tests/guest/m12/cargo.rs` | 定义 M12 离线 Rust/Cargo 验收夹具及其声明的行为。 |
| `tests/guest/m12/config.toml` | 定义 M12 离线 Rust/Cargo 验收夹具及其声明的行为。 |
| `tests/guest/m12/helper.c` | 提供由 guest build.rs 编译并经 Rust FFI 静态链接的 C 函数。 |
| `tests/guest/m12/macro-support/Cargo.toml` | 定义过程宏与其构建脚本共用的 host 依赖。 |
| `tests/guest/m12/macro-support/src/lib.rs` | 提供可编辑的数值，以验证 Cargo 传递依赖的重建。 |
| `tests/guest/m12/macro.rs` | 导出实际过程宏，并将其编译成 guest 共享库。 |
| `tests/guest/m12/macro/Cargo.toml` | 定义 Cargo 过程宏及其运行和构建所用的路径依赖。 |
| `tests/guest/m12/macro/build.rs` | 从 host 支持 crate 生成宏输入及环境变量。 |
| `tests/guest/m12/macro/src/lib.rs` | 使用生成的输入展开 Cargo 宏，并拒绝意外 token。 |
| `tests/guest/m12/main.rs` | 定义 M12 离线 Rust/Cargo 验收夹具及其声明的行为。 |
| `tests/guest/m12/online/Cargo.toml.in` | 用精确 registry 版本和生成的完整 Git revision 实例化在线 guest 工程。 |
| `tests/guest/m12/online/main.rs` | 在 guest 构建的应用中运行分别下载的 registry 和 Git 依赖。 |
| `tests/guest/m12/plain/Cargo.lock` | Cargo 为无依赖 guest 工程生成的锁文件，不得手动修改。 |
| `tests/guest/m12/plain/Cargo.toml` | 定义独立且无外部依赖的离线 Cargo 验收工程。 |
| `tests/guest/m12/plain/src/main.rs` | 运行真正由 guest 构建的无依赖 Cargo 程序，并检查其输出。 |
| `tests/guest/m12/public/Cargo.toml` | 独立锁定来自公共 registry 和 Git 的 itoa，不使用 vendor 替换。 |
| `tests/guest/m12/public/main.rs` | 运行链接到真实 guest 构建程序中的公共依赖。 |
| `tests/guest/m12/runtime.c` | 在真实 guest 中验证非法返回、detached 线程退出、备用信号栈、定位与调整文件大小、持久卷/tmpfs 时间戳精度和 OFD 锁。 |
| `tests/guest/m12/use_macro.rs` | 要求实际装载并展开 guest 构建的宏，然后验证结果。 |
| `tests/guest/m12_online_peer.py` | 提供确定性源码归档和 revision 锁定的 Git smart HTTP，并测试真实 TLS 信任、域名、日期及传输失败。 |
| `tests/guest/m13/crash.c` | 通过命名栈帧触发真实原生用户故障，并验证损坏帧指针被拒绝。 |
| `tests/guest/m15/capacity.c` | 探测持久文件、描述符和任务限制，验证资源恢复及文件字节保留。 |
| `tests/guest/m15/gcc_probe.c` | 在 guest 中诊断真实缺失构建工具的 exec 错误和有界上游 configure 启动失败。 |
| `tests/guest/m15/gcc_seed.cpp` | 使用继承的 guest 种子工具验证 C++11 编译，不重建编译器。 |
| `tests/guest/m15/rejected.c` | 故意触发真实 guest 编译错误，并保留此前重建的可用工具。 |
| `user/Cargo.lock` | 生成的原生 SDK/应用依赖图锁文件，不得手动插入注释。 |
| `user/Cargo.toml` | 将原生用户 ELF 构建与裸机内核的目标编译参数分离。 |
| `user/build.rs` | 外部用户专用链接脚本变化时，重新构建原生应用二进制。 |
| `user/c/anova.h` | 定义原生 v1 控制台/进程的传输布局和 C syscall 封装，不依赖 libc 或 Linux。 |
| `user/c/crt.S` | 将原生启动栈转换成 C main 参数，并通过 SYSCALL 退出。 |
| `user/c/hello.c` | 演示 C SDK、参数、BSS/SSE2、调度和描述符调用，最后以状态 7 退出。 |
| `user/compositor/Cargo.toml` | 构建隔离的 Rust 原生软件窗口合成器。 |
| `user/compositor/src/gpu.rs` | 验证仅供服务端使用的 GPU API、带边界保护的读回比较和用户选择的 CPU 回退。 |
| `user/compositor/src/main.rs` | 在用户态接收已认证消息、合成私有表面并分发硬件输入。 |
| `user/editor/Cargo.toml` | 使用 no_std SDK 构建独立的原生串口源码编辑器。 |
| `user/editor/src/main.rs` | 打开和编辑有界 ASCII 文本，保存失败时保留缓冲区，成功时执行 fsync。 |
| `user/fs-test/Cargo.toml` | 使用共享 no_std 运行时构建独立的原生 fs-test 应用。 |
| `user/fs-test/src/disk.rs` | 执行真实 ext2 描述符、间接块、稀疏文件和 unlink 系统调用，验证持久源码/ELF 数据。 |
| `user/fs-test/src/foundation.rs` | 在 guest 验证真实 ext4 只读目录/文件 syscall、稀疏零值和修改拒绝。 |
| `user/fs-test/src/main.rs` | 验证真实 VFS 系统调用、继承的共享偏移、exec/cwd、用户缓冲区和阻塞管道；子进程模式检查 fork/exec 后资源，shell-check 读取脚本生成的文件。 |
| `user/gui-client/Cargo.toml` | 构建独立原生窗口客户端和用于检验传输边界的对抗夹具。 |
| `user/gui-client/src/main.rs` | 上传私有红/蓝表面、处理焦点输入，并测试崩溃、队列和所有权边界。 |
| `user/gui-common/Cargo.toml` | 在合成器和测试程序之间共享内部原生 GUI 传输辅助代码。 |
| `user/gui-common/src/lib.rs` | 封装同步复制的 GUI 消息，并保留明确的指针安全约定。 |
| `user/gui-link.rs` | 将独立 guest 客户端目标文件与 Rust GUI 库、原生 CRT 和 panic 边界链接。 |
| `user/gui-run/Cargo.toml` | 为 guest SDK 应用打包原生合成器/客户端生命周期监督程序。 |
| `user/gui-run/src/main.rs` | 启动隔离 GUI 进程，要求真实等待状态符合预期并完成回收。 |
| `user/gui-test/Cargo.toml` | 构建用于真实 GUI 生命周期验收的原生进程监督程序。 |
| `user/gui-test/src/main.rs` | 创建并回收隔离窗口和合成器会话，测试配额并保留 COM1 恢复通道。 |
| `user/hello/Cargo.toml` | 构建验证启动参数、BSS 清零和 SSE2 的 Rust hello ELF。 |
| `user/hello/src/main.rs` | 提供独立 Rust ELF，通过原生系统调用展示 argc/argv、BSS 和 CPU 状态。 |
| `user/init/Cargo.toml` | 使用共享 no_std 运行时构建独立原生 init 应用。 |
| `user/init/src/main.rs` | 启动原生 shell，等待子进程状态，并在退出后恢复交互会话；测试模式先运行独立文件系统测试，再执行 shell 脚本。 |
| `user/linux/busybox.config` | 从 allnoconfig 默认配置选择锁定的静态 BusyBox applet 范围。 |
| `user/linux/cc.c` | 设置受控离线编译器/make 环境，并执行实际 guest 工具。 |
| `user/linux/dns.c` | 解析有界 DNS A 响应，检查压缩、随机 ID、响应来源和超时。 |
| `user/linux/dns.h` | 共享 guest DNS 解析器和受检查响应解析接口。 |
| `user/linux/gui-build.c` | 运行真实 guest GCC/G++/rustc，仅在成功链接后发布原生 GUI 可执行文件。 |
| `user/linux/m9.c` | 提供独立 musl 静态/PIE/动态 Ring 3 夹具，验证线程、信号、PTY、exec/故障/退出负例及 UART 中断。 |
| `user/linux/musl.c` | 验证未修改的静态 musl 启动、编译器 TLS、分配、errno 和真实文件 I/O。 |
| `user/linux/network.c` | 在真实 Ring 3 中探测 TCP/UDP/DNS、超时、指针、信号及描述符复用。 |
| `user/linux/pkg.c` | 安装并检查原生包，验证哈希、ABI 和许可证，维护持久交替提交状态。 |
| `user/linux/probe.S` | 按 SysV 对齐将真实 Linux 启动栈传给 freestanding ABI 探针。 |
| `user/linux/probe.c` | 验证 Linux 栈、errno、内存、TLS、fork/exec/env/wait、CLOEXEC、SIGPIPE、随机数和 sleep。 |
| `user/linux/rust.c` | 配置离线 Rust/Cargo 执行，以及隔离的 guest sysroot 和链接器。 |
| `user/linux/shared.c` | 提供已初始化的共享对象 TLS，用于启动链接及工作线程存在之后的 dlopen 验证。 |
| `user/linux/tls.c` | 在强制验证 CA、域名和日期的条件下获取有界 HTTPS 内容，并检查负例的精确失败原因。 |
| `user/linux/tls.h` | 共享严格的用户态 HTTPS 获取接口。 |
| `user/probe/Cargo.toml` | 构建 QEMU 用户隔离验收门禁使用的原生负例 ELF。 |
| `user/probe/src/main.rs` | 触发真实 Ring 3 系统调用错误、保护故障和不安全返回；内核验收程序指定命名用例并核对结果。 |
| `user/process/Cargo.toml` | 构建用于调度和生命周期验收的真实原生多进程负载。 |
| `user/process/src/context.S` | 在不含系统调用的定时抢占和可选 eager fork 期间保留不同 GPR、全部 XMM 通道、x87 栈/控制字与 MXCSR 哨兵，随后验证其完整性。 |
| `user/process/src/main.rs` | 验证真实调度、阻塞、fork/exec、父进程等待和描述符生命周期；不同 CPU 哨兵跨无系统调用循环保持，故意触发的子进程故障不影响其他进程。 |
| `user/rt/Cargo.toml` | 为 Rust 应用打包 no_std 启动入口和原生 v1 系统调用封装。 |
| `user/rt/src/lib.rs` | 提供原生 Rust CRT、启动参数和受检进程/文件系统调用封装；使用 x86_64 用户 ABI/SSE2，不依赖 libc、分配器或 Linux 调用。 |
| `user/rt/src/memory.rs` | 提供独立运行的编译器内存内建函数，不链接宿主 libc。 |
| `user/rt/src/v1.rs` | 保留原始接口，同时为 native v1 增加类型化错误和拥有所有权的描述符封装。 |
| `user/shell/Cargo.toml` | 使用共享 no_std 运行时构建独立原生 shell 应用。 |
| `user/shell/src/lib.rs` | 解析有界 shell 子集：四级管道、引号/转义和逐级重定向；按现有 native v1 exec 约定向命令传递合并后的参数字符串。 |
| `user/shell/src/main.rs` | 通过 fork/exec/wait、管道和重定向运行原生 shell 与多调用工具；解析、命令策略和行组装均在用户态，不使用 libc 或分配器。 |
| `xtask/Cargo.toml` | 构建宿主侧依赖、原生应用、镜像、QEMU 和证据命令。 |
| `xtask/src/app.rs` | 验证外部包哈希、构建应用镜像、启动独立工作盘，并验收公开 SDK 安装流程。 |
| `xtask/src/apps.rs` | 以用户专用目标/链接参数构建独立原生 Rust/C ELF，校验产物并记录显式 ABI 身份和 SHA-256 清单。 |
| `xtask/src/assets.rs` | 获取、校验并在本地解包锁定的引导器/固件输入。 |
| `xtask/src/bootstrap.rs` | 准备锁定的 bzip2 工作盘，验证 guest 编译、独立压缩检查、容量限制及重启恢复。 |
| `xtask/src/bootstrap_gcc.rs` | 审计 GCC 源码容量，验证真实 guest 种子、缺头文件/工具/configure 拒绝和重启证据，不声称重建编译器。 |
| `xtask/src/bootstrap_kernel.rs` | 在 guest 进程中构建内核，将确切 ELF 保留并验证到宿主打包的 ISO 中，再检查启动身份和失败恢复。 |
| `xtask/src/bootstrap_repeat.rs` | 重复执行干净 guest bzip2/内核构建，保留独立产物并比较精确输入、字节和恢复证据。 |
| `xtask/src/bootstrap_rust.rs` | 独立审计 Rust/LLVM/Cargo 源码容量，验证真实 guest 种子、前置条件失败与重启证据，不声称重建组件。 |
| `xtask/src/debug.rs` | 对照确切 ELF 符号解析捕获的 guest 故障地址，验证真实且有界的 guest 崩溃回溯。 |
| `xtask/src/disk.rs` | 创建安全的 native/Linux ext2 镜像，并用 watchdog 限时驱动重启、完整性和设备错误场景。 |
| `xtask/src/foundation.rs` | 创建并保留受控 ext4 镜像，执行带源码/产物哈希的四次 QEMU 只读/重启/拒绝门禁。 |
| `xtask/src/gpu.rs` | 验收指定设备的真实硬件绘制、完整 scanout 比较、故障/reset 恢复和无头 CPU 回退。 |
| `xtask/src/graphics.rs` | 验证真实 QMP 像素、有界截图解析，以及有/无 VGA 时的串口恢复和正常关机。 |
| `xtask/src/gui.rs` | 注入真实 QMP 输入，比较原生窗口、故障恢复及每一个 scanout 像素。 |
| `xtask/src/gui_sdk.rs` | 验证 guest 编辑/构建、物理 GUI 输入、实际画面、故障和重启后保留文件哈希。 |
| `xtask/src/gui_sdk_build.rs` | 导出含双语指南和校验清单的 GUI 源码 SDK，打包 guest 编译器/显示镜像。 |
| `xtask/src/input.rs` | 注入 QMP 键鼠序列，比较全部光标/背景像素，并要求缺设备时串口恢复和正常关机。 |
| `xtask/src/linux.rs` | 构建并记录锁定 musl/BusyBox 静态/动态夹具哈希，打包上游源码，保存有界 Linux 宿主追踪。 |
| `xtask/src/main.rs` | 提供可复现内核/应用构建、ISO 组装和 QEMU 验证的宿主命令行入口。 |
| `xtask/src/network.rs` | 运行受控本地回环对端，用 watchdog 驱动真实 QEMU 网络验收。 |
| `xtask/src/packages.rs` | 锁定并构建用户态 TLS 客户端、生成锁定包，验证安装、重启和 HTTPS 行为。 |
| `xtask/src/purposes.rs` | 检查现存已跟踪/未跟踪文件的用途清单和自有文件头，忽略移动后已不存在的索引路径；职责是否准确仍由人工审阅。 |
| `xtask/src/rust.rs` | 构建 Rust 镜像、执行真实 guest 编译，并独立检查重启后的源码和 ELF 文件。 |
| `xtask/src/rust_online.rs` | 验证真实 guest HTTPS sparse-registry/Git 获取、严格传输失败及无网络重启后的缓存复用。 |
| `xtask/src/rust_public.rs` | 验证 crates.io/GitHub 公共源码获取、锁定 revision、实际 guest 编译和离线重启复用。 |
| `xtask/src/sdk.rs` | 导出包含双语指南的原生 SDK，创建/构建目标隔离的 C/C++/离线 Rust 项目。 |
| `xtask/src/smoke.rs` | 要求真实 CPU、内存、用户态和进程证据与 QEMU 退出结果同时符合约定。 |
| `xtask/src/terminal.rs` | 驱动普通 UART 编辑/EOF、动态 musl 启动、真实 Ctrl+C 中断及易失数据重启检查。 |
| `xtask/src/toolchain.rs` | 构建并打包可选开发 ISO，验证 guest 编辑、编译、make、失败场景与重启哈希。 |
| `xtask/src/transcript.rs` | 捕获有序 guest stdout，保留 UART 证据，独立于文件系统快照或不完整 UTF-8 匹配原始字节。 |

| `crates/storage/src/ext4/audit.rs` | 审计可写位图/计数、全局块归属和根目录/链接关系。 |
| `crates/storage/src/ext4/directory.rs` | 暂存 ext4 目录增长、重命名替换/移动、父目录链接和孤儿命名空间修改。 |
| `crates/storage/src/ext4/write.rs` | 暂存有界 ext4 分配/数据/extent 修改，发布错误停止服务，正常关机 flush 清洁标记。 |
| `crates/storage/tests/ext4_write.rs` | 验证真实 ext4 可写镜像、逐预检分配失败、ENOSPC、元数据和 I/O 边界。 |
| `user/linux/ext4_write.c` | 通过真实 musl 验证 ext4 写入、目录/cwd 移动、孤儿/映射/fork 生命周期、元数据和满盘复用。 |
| `xtask/src/foundation_write.rs` | 创建显式启用的 ext4 工作盘，保留六次 QEMU 写入/构建/重启/virtio 故障及宿主检查。 |
| `docs/en/validation/M16.4.md` | 说明受控 ext4 write-v1 契约、故障与独立检查的本地证据（英文）。 |
| `docs/zh-CN/validation/M16.4.md` | 提供受控 ext4 write-v1 契约与本地验收边界的中文对照。 |
| `crates/storage/src/ext4/journal.rs` | Implement bounded checksum-v3 JBD2 commit/replay/revoke and persistent orphan recovery. |
| `crates/storage/tests/journal.rs` | Verify sector/flush/half-sector power cuts, actual allocation failure and independent e2fsck interoperability. |
| `scripts/reconstruct_journal_cut.py` | Reconstruct any retained journal fault from an immutable baseline and exact sector event trace. |
| `xtask/src/foundation_journal.rs` | Create fresh JBD2 images and retain host matrix plus real guest write/power-cut/recovery evidence. |
| `xtask/src/foundation_migrate.rs` | Copy readonly ext2 sources to new JBD2 disks and verify actual builds, interruption and UART budgets. |
| `tests/guest/m16/workload.c` | Supervise actual guest make/GCC, parallel storage writes and UART command/save/log/interruption probes. |
| `docs/en/validation/M16.5.md` | Specify the bounded JBD2 profile, ordering, failure/replay and actual local acceptance. |
| `docs/zh-CN/validation/M16.5.md` | Translate the JBD2 profile, fault models, evidence and exact recovery limits. |
| `docs/en/validation/M16.6.md` | Specify fresh-disk migration, real build closure, foreground budgets and exact local evidence. |
| `docs/zh-CN/validation/M16.6.md` | Translate migration ownership, actual build/UART acceptance and independent remaining work. |

| `xtask/src/foundation_capacity.rs` | Accept separate ext4 kernel-build budgets, pressure recovery and independently checked allocation peaks. |
| `tests/guest/m16/capacity.c` | Exhaust pinned VFS directory slots twice and verify rejected creation, removal and reuse. |
| `docs/en/validation/M16_CAPACITY.md` | Record exact ext4 kernel capacity, recovery and remote CI execution evidence. |
| `docs/zh-CN/validation/M16_CAPACITY.md` | Translate the ext4 kernel capacity and remote CI closeout record with matching boundaries. |
| `docs/assets/validation/M16_CI.json` | Retain exact remote run/job/step observations, input failures, repairs and artifact identities. |
| `docs/assets/validation/M16_CORE_CI54.json` | Preserve digest-verified core CI artifact result records, exact source identity and bounded accepted scope. |
| `docs/assets/validation/M16_CAPACITY_LOCAL.json` | Preserve accepted local ext4 kernel capacity, budgets, complete provenance and digest-verified raw logs. |
| `docs/assets/validation/M16_CORE_CI57.json` | Retain verified current-code core CI artifact results, source identity and artifact digest. |
| `crates/abi/src/process.rs` | 定义独立版本化直接子进程监督契约并拒绝非法版本。 |
| `kernel/src/task/process_control.rs` | 授权直接子进程轮询/停止，复制失败保留状态并复用内核清理。 |
| `user/rt/src/process.rs` | 封装实验非阻塞子进程轮询与身份受检停止，保留 native v1。 |
| `user/desktop/Cargo.toml` | 通过原生进程与 GUI 接口构建 Rust 桌面会话监督器。 |
| `user/desktop/src/main.rs` | 管理准入客户端/合成器生命周期、有界会话替换及图形启动器操作。 |
| `xtask/src/desktop.rs` | 构建纯原生桌面输入，校验宿主构建的现有三语言客户端并打包显式镜像。 |
| `docs/en/validation/M17.1.md` | 记录桌面初版、已执行宿主检查和未执行 QEMU 验收。 |
| `docs/zh-CN/validation/M17.1.md` | 提供桌面初版范围、进程边界、检查和待执行验收的中文说明。 |
