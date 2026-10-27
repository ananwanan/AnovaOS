<!-- Plan measured resource growth, filesystem isolation and independently validated ext4 adoption. -->
# M16：资源、VFS 与受控 ext4

[English](../../en/milestones/M16_FOUNDATION.md) | **简体中文**

本计划于 2026-10-26（Asia/Shanghai）基于 `dev` 的 `05a5fb1` 制定。
**2026-10-27 基于 `616ff47` 更新：后端/缓存解耦和受控 ext4 只读配置已实现并通过本地验证。**
依赖获取已修复；远端 CI、完整负载/延迟验收、JBD2 恢复和迁移仍待完成。
详见 [M16 证据](../validation/M16.md)。下文保留各阶段完整退出条件。已实现边界见[路线图](../ROADMAP.md)、
[当前状态](../guides/STATUS.md)和[架构](../ARCHITECTURE.md)。

M16 先扩展开发基础，再进入 [M17 开发者桌面](M17_DESKTOP.md)。完整编译器
自举作为有条件的 M18 后续任务，不阻塞可用的开发者桌面。SMP、物理存储
驱动、通用 Linux 软件和新的 AnovaOS 磁盘格式分别规划；AnovaFS 不在本次
关键路径。保留已验收的 ext2 开发和恢复路线。

## 基线与顺序

当前开发盘为 16 MiB、512 inodes；持久文件上限 4 MiB，tmpfs 和只读共享
文件缓存上限 1 MiB。VFS 有 2,048 nodes、128 个共享 open descriptions，
每进程 32 descriptors；内核 heap 固定 16 MiB，单 BSP 共有 32 个进程/线程槽。
内核不可抢占、设备 I/O 采用轮询。增加 QEMU RAM 不会改变这些软件上限；
Linux/native ELF 和启动参数的限制也必须分别处理。

当前 ext2 实现仅支持一个 4 KiB block group，最多 32,768 blocks、4,096 inodes，
普通文件使用 direct/single-indirect 寻址。这些是本项目实现子集的限制，
不是 ext2 格式极限。M16 前 VFS 直接持有 `Ext2`，挂载时导入整个命名空间并预留节点容量。
现在通过能力明确的后端按需加载目录，缓存从 32 槽可失败扩展，在原有 2,048 槽限额内
回收不活跃普通文件；目录/cwd、打开文件和映射保持固定身份。仅扩大磁盘无法使命名空间和缓存具备可扩展行为。
历史 [GCC](M15_GCC.md)、[Rust](M15_RUST.md) 审计保留原始 1 MiB 文件配置；
M15.5 已将当前持久文件上限提高到 4 MiB。

2026-10-26 核对基线 `05a5fb1` 的[现有 CI 运行](https://github.com/ananwanan/AnovaOS/actions/runs/40116492704)：
`boot` job 成功，`guest-toolchain` job 在下载 musl.cc native seed 时，
连接 HTTPS 443 端口出现 curl error 28；其后依赖步骤被跳过。这是 seed
获取问题的实测记录，不是内核故障或 guest 工具链回归通过的证据。

按 M16.1 至 M16.6 顺序执行，同时保留可独立使用的 ext2 配置。实际 ext4/构建
结果发现新需求时，可迭代资源与 POSIX 工作；每阶段单独保存证据和失败边界。

| 阶段 | 计划结果 | 状态 |
| --- | --- | --- |
| M16.1 | 恢复 CI 基线，确定实测工作负载和资源预算 | 获取已修复；本地源码构建；远端 CI/预算待验收 |
| M16.2 | 文件系统后端边界与有界、可扩展的资源管理 | 后端/缓存已本地验证；限额不变 |
| M16.3 | 受控 ext4 只读挂载与数据访问 | 只读 v1 已本地验证；见证据 |
| M16.4 | 受控 ext4 写入和明确的 POSIX 语义 | 已实现显式启用的 write-v1；[配置与证据](../validation/M16.4.md) |
| M16.5 | JBD2 恢复和持久性验证 | 计划中；未实现 |
| M16.6 | 新盘迁移、真实构建和可响应的前台操作 | 计划中；未实现 |

## M16.1：CI 基线与实测工作负载

- 恢复锁定 seed 的可复现获取，保留来源、许可、清单和哈希。分别诊断网络与
  构建/运行时失败；下载成功或旧报告不能代替未通过的测试。
- 执行适用的现有 CI jobs，保存真实 run/commit/job 身份、已执行检查和跳过项；
  本地验收与远端 CI 分别记录。
- 定容前声明真实源码构建工作负载、准确源码闭包、工具、依赖、build/host/target、
  ABI/sysroot 和顺序构建 recipe。
- 测量 file/inode/node 数、最大输入/输出、临时存储、heap/pages、descriptors、
  task slots 和 wall time；对比前确定资源上限和前台命令/输入延迟预算。

退出条件：对实际执行范围有可审阅的成功 CI 基线、可复现负载和有依据的资源预算。
scheduler boundary 采样不等于准确峰值 RSS；正常关机后盘占用不等于瞬时存储需求。
不得根据完整上游归档清单承诺任意容量。

## M16.2：文件系统边界与资源管理

- 将持久文件系统操作与具体 ext2 类型、根 inode 编号、文件上限和时间精度分离。
  显式表示 mount/node 身份、文件系统能力和不支持操作，不改变冻结的 native v1
  ABI，也不改变挂载期间不可变的 executable-ABI 标记。
- 实现有界按需 lookup/目录迭代和可回收 node/cache，替代全盘导入；缓存淘汰时
  保持 open/unlinked 身份、共享 offset、mapping 一致性及名称/路径检查。
- 仅按 M16.1 依据扩展 heap、VFS/cache、task/fd 和用户 mapping 容量。定义增长/
  配额、回收和可失败的元数据路径；保留 guard pages、W^X、所有权和完整分配回滚。
  更多槽位不代表 SMP。
- 保留 ext2 字节兼容、已验收错误行为及独立旧镜像挂载/构建/重启 gates；
  验证低内存、满缓存和资源耗尽后的复用。

退出条件：既有 ext2 负载经后端边界仍通过，命名空间和元数据开销符合预算；
压力不能破坏其他挂载/进程或使保留的恢复路线失效。

## M16.3：受控 ext4 只读访问

- 锁定明确的 ext4 镜像配置及 e2fsprogs 版本/创建选项；列举允许的 compat/
  incompat/readonly-compatible features。未知或不支持组合明确拒绝，不静默放开写入。
- 检查 geometry、设备边界、多 block groups、descriptors、inode 布局、extents、
  分配引用及所需元数据 checksum；对不可信镜像限制树遍历、环、重叠和资源用量。
- 对该配置实现真正只读 lookup/readdir/stat/read，覆盖跨组、碎片、大文件和稀疏
  测试样例，并提供独立哈希。
- 在恢复实现前拒绝需要 journal replay 的 dirty 文件系统；只读挂载不能把未回放
  元数据当作一致视图。

退出条件：guest 字节和声明的元数据与独立宿主检查一致；坏镜像、不支持特性、
需要恢复的镜像均在不修改磁盘的情况下失败。只读验收不证明写入、journal 或崩溃恢复。

## M16.4：受控写入与 POSIX 语义

显式 `ext4-write` 内核已实现有界无日志写入，准入、容量、语义、命令和实测见 [M16.4](../validation/M16.4.md)。
M16.1 预算/远端 CI 和 M16.5/M16.6 仍独立验收。

- 为声明配置加入有界分配/释放、create/read/write/resize、目录、rename、
  unlink/open 生命周期、executable modes 和 timestamps。按实际负载声明替换、
  补零/稀疏、links 和不支持操作，不伪造成功来通过构建探测。
- 保留 file mappings、descriptor/cwd 生命周期、挂载身份及多进程行为；每条修改
  路径明确容量限制与回滚。
- 明确可写 feature set 与 journal 状态。M16.5 前排除必须使用 journal 的配置，
  不绕过 replay 要求；临时 non-journal 配置必须声明恢复限制并有独立样例。
- 该配置保持可选。测试准确持久字节/元数据、ENOSPC/OOM、坏镜像、真实 read/
  write/flush 错误以及带宿主检查的正常重启。

退出条件：真实文件工具和选定构建子集可在该可写配置运行，声明的失败保留旧输出，
停止的镜像通过指定 e2fsprogs 检查。正常重启只证明声明范围内的持久化；
此阶段不将 ext4 设为默认工作盘。

## M16.5：JBD2 恢复与持久性

- 声明支持的 journal 位置/特性、transaction 边界、ordered data/metadata 策略、
  checksum、commit/replay/revoke 行为和 abort 状态；以有界解析和资源所有权
  实现选定 JBD2 配置。
- 定义真实块设备路径中的 fsync/sync、rename 持久性及 flush/barrier 假设；
  报告 write/flush 失败，禁止把失败 commit 标记为 clean。
- 在每个相关 write/commit/flush 点注入中断，并覆盖部分设备写入、flush 失败、
  journal 撕裂/损坏。使用独立磁盘重启和 replay，保留原始失败镜像供检查。
- 比较允许的操作前/后状态、命名空间和分配不变量、文件哈希及 replay 幂等性；
  用 e2fsck 交叉检查停止后的副本，禁止重复分配、遗失存活所有权及暴露未初始化数据。

退出条件：声明的 journal/持久性契约通过完整故障矩阵，包括损坏拒绝和中断恢复。
正常关机或两次相同 clean build 不能代替崩溃/replay 证据。

## M16.6：迁移与可用开发

- 创建全新 ext4 工作盘，从保留的 ext2 源盘复制文件；保留声明的路径、字节、
  modes、timestamps 和 ABI policy，保存清单/哈希并验证重启/恢复，确保源盘可恢复。
  不进行不可逆原地转换，也不静默重格式化用户磁盘。
- 安装并执行选定负载真正需要的 POSIX build utilities 和开发输入；在 guest 重建
  真实工具并运行，失败后保留先前可用版本，比较源码/输出清单。
- 构建和存储压力期间，按 M16.1 延迟预算验证前台命令/输入、进程中断、日志获取、
  保存和正常关机；按测量结果处理阻塞 I/O/内核路径，不预设必须先完成 SMP。
- 所有依赖阶段通过前仅允许显式选用；以后切换默认配置时，必须有验收记录、文档
  和保留的 ext2 启动/恢复选项。不由此推导 guest-only image 生成或编译器自举。

退出条件：可独立恢复的开发盘在资源和响应预算内支持声明的真实构建；为 M17 的
编辑/构建/运行/排障桌面闭环提供基础，自举后置至 M18。

## 持续验收与实现参考

保留 [DEVELOPMENT](../guides/DEVELOPMENT.md) 既有 gates，包括 runtime/storage/
persistence、C/C++/Rust/Cargo/network、SDK 和 M14 GUI/GPU。保留五项 M15
`bootstrap-*-test`/`bootstrap-test` gates、源码拒绝测试及 `bootstrap-metrics`
feature check。只有可运行实现交付时才引入 M16 命令；`cargo xtask foundation-test` 已实现只读 v1 验收，不代表 M16.4–M16.6 完成；独立的 `foundation-write-test` 仅验证 M16.4 write-v1。
记录准确输入、feature profiles、commits/worktree hashes、QEMU 配置、日志、产物和
独立磁盘检查；同步维护翻译和状态。

格式参考：Linux kernel [ext4 overview](https://www.kernel.org/doc/html/latest/filesystems/ext4/overview.html)、
[inode data/extents](https://www.kernel.org/doc/html/latest/filesystems/ext4/ifork.html)、
[journal/JBD2](https://www.kernel.org/doc/html/latest/filesystems/ext4/journal.html) 及
[e2fsprogs](https://e2fsprogs.sourceforge.net/)。实际实现输入和测试工具版本仍须锁定；
参考文档本身不构成兼容性证据。
