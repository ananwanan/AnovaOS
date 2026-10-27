<!-- Record the ext4 kernel workload, declared budgets, exhaustion recovery and actual CI evidence. -->
# M16 内核容量与 CI 结项

[English](../../en/validation/M16_CAPACITY.md) | **简体中文**

记录于 2026-10-27，分支 `feat/m16-foundation`。本地容量验收已在
`e2db30f9752d3e29cb6c6734498cda418071036c` 的干净独立检出中通过。
后续 `1b63f42` 仅增加同摘要种子传输地址与 CI 证据，未改变内核/运行时代码。
第 57 次远端运行已结束：核心通过，ext4 镜像准入失败；完整远端验收尚未通过。
随后按所有者要求，Actions 仅限 main 推送，暂停功能分支重跑。历史 [M16.5](M16.5.md) 与
[M16.6](M16.6.md) 保留原有恢复/bzip2 范围。默认存储仍为 ext2。

## 复现声明的工作负载

```sh
cargo xtask fetch
cargo xtask toolchain-build
cargo xtask rust-build
cargo fetch --locked
cargo xtask foundation-capacity-test
```

使用已验收的 [M15 内核配方](../milestones/M15_KERNEL.md)：精确内核/本地 crate/
链接脚本、锁定离线 Cargo 依赖，以及继承的官方 Rust/Cargo/rust-lld 1.90.0 和匹配的
预编译 core/alloc/compiler_builtins。build/host 是 AnovaOS 的 x86_64 Linux-musl
personality；target 为 `x86_64-unknown-none`，特性为 `ext4-write,bootstrap-metrics`，
单 job/codegen unit、opt-level z、去符号、panic abort。此过程实际重建内核 ELF。

全部构建产物与临时文件使用**新建 64 MiB JBD2 工作盘**：4 KiB 块、4,096 个 inode、
8 MiB 内部日志。输入/工具保留在独立只读 initramfs 中；64 MiB 并不表示编译器和源码也
放在该工作盘。本地 initramfs 为 717,902,503 字节；额外内核源码/目标库闭包包含
957 个文件、82,073,888 字节。

执行前写入预算：q35/TCG/OVMF、单 BSP、3 GiB 客体 RAM、无网络；16 MiB 内核堆、
2,048 个 VFS 槽、共 32 个进程/线程槽、每进程 32 个 FD、额外 524,288 个 4 KiB
物理页（2 GiB），每会话 3,600 秒。宿主工具为 Rust 1.90.0、QEMU 8.2.2、
e2fsprogs 1.47.0。

## 实际本地测量

| 资源 | 声明上限 | 主动压力 | 干净内核构建 |
| --- | ---: | ---: | ---: |
| 采样堆字节 | 16,777,216 | 16,774,240；实际分配失败 | 9,494,480 |
| 采样额外 4 KiB 物理页 | 524,288 | 8,764 | 142,825 |
| VFS 槽 | 2,048 | 2,048 | 1,564 |
| 活跃 / 占用任务槽 | 32 | 32 / 32 | 14 / 14 |
| 每进程 FD | 32 | 32 | 16 |
| 已分配 4 KiB 磁盘块峰值 | 16,384 | 16,384；实际 ENOSPC | 6,072 |
| 已分配 inode 峰值 | 4,096 | 13 | 241 |
| 含启动/关机的宿主墙钟毫秒 | 每会话 3,600,000 | 558,133 | 971,675 |

构建的物理页增量为 585,011,200 字节；磁盘分配峰值为 24,870,912 字节，
**包含文件系统元数据与日志**。构建工作区保留 179 个普通文件、14,618,936 字节，
最大文件为 2,202,508 字节。这些是此配方的实测需求，不是最小容量，也不证明更广的
编译器/桌面负载可用。

内存在调度边界采样，不是精确 RSS 峰值；物理页增量包含内核分配，但可能漏掉 syscall
内部瞬时峰值。ext4 在每次成功变更后更新分配峰值，包含之后删除的临时文件；
拒绝/失败事务不发布计量，重新挂载时峰值重置。每次最终计数都与独立读取的停机
superblock 相符，且 `e2fsck -f -n` 通过。

## 实际执行的耗尽与恢复

1. tmpfs 真实堆分配失败，既有字节和文件偏移保持；释放资源后继续运行。
2. EFBIG 保留既有文件/偏移；EMFILE 达到 32 个 FD（新增 29 个及继承描述符）；
   EAGAIN 达到 32 个任务槽（29 个子任务及既有任务）。探针关闭/回收后成功复用。
3. 两轮 VFS 目录分配各创建 712 个额外固定目录，达到全部 2,048 个全局槽。
   被拒绝的创建没有残留 inode；删除后成功创建文件，第二轮未丢失任何槽。
4. 真实写入用尽全部 16,384 个磁盘块；ENOSPC 保留原数据/偏移；截断/删除后块可复用。
   清理后磁盘回到 2,318 块 / 12 个 inode，原生应用可运行，并正常关机。
5. 在**同一张恢复后的盘**上，干净真实内核构建成功；guest 中 Cargo 报告 10m 57s。
   宿主独立检查持久 ELF，并直接打包这些精确字节，没有在宿主重编该内核。
6. 新内核启动并报告精确嵌入身份；真实 Rust 语法错误返回编译器状态 1，仍保留可启动
   ELF，原生应用继续运行。最终恢复启动保持工作区全部文件摘要。两次新内核会话均
   正常关机，并通过独立文件系统检查。

语法拒绝会话耗时 36,725 ms，峰值为 91,549 页 / 6,301,088 堆字节 / 7 个任务 /
8 个 FD / 1,341 个 VFS 槽。恢复会话耗时 18,509 ms，峰值为 140 页 / 1,274,144
堆字节 / 3 个任务 / 3 个 FD / 1,335 个 VFS 槽。两次最终均为 6,074 块 /
243 个 inode（构建后新增两个保留诊断文件）。

## 保留的本地证据

完整的[本地结果与原始日志](../../assets/validation/M16_CAPACITY_LOCAL.json)包含预先预算、
全部输入/实现摘要、四次会话结果、完整宿主驱动及 UART 日志，以及各日志长度与 SHA-256。
从已保存 JSON 解码再编码每份日志后，均独立匹配原始 SHA-256；整个门禁期间实现摘要不变。

| 身份 | 值 |
| --- | --- |
| 构建身份 | `c6b411cd182e1d9277e091b49c1234db8544e054b45bab476d9af264b890b573` |
| 客体内核 | 411,008 字节 |
| 客体内核 SHA-256 | `dfe9b864571f0c681b64633a661339c5a262c0755e00de88647c4ce46b30f44b` |
| 打包 ISO SHA-256 | `90a29e80b9b7937ed7a901ddb35e5d4e35d42d7aef2a84307252f012aa0fb67d` |
| Initramfs SHA-256 | `f7b1e19d1ca5099923ffed457b7e4e92c4e45433f4c06db8d40702d0d2f4bff7` |
| 实现清单 SHA-256 | `baba52da3d37545c6c1de7adb5cc3a2d54dbd04ec206302f7fddaf6829b36147` |

本地原始磁盘/ELF/ISO 仍位于独立验收检出的
`build/foundation/capacity-1793082851991418782/`。每次 CI 无论成功或失败，
均上传自己的 `build/foundation/` 与日志。

## 远端 CI 执行与修复

完整工作流保留 `boot`、依赖它的 `guest-toolchain` 及所有旧门禁。
**当前所有者策略：仅推送 `main` 时触发 Actions。** 暂时移除 PR 与手动 dispatch 入口；
`dev` 和功能分支推送不触发本工作流。下表运行使用此前的分支触发策略。
guest 任务 watchdog 保持 180 分钟；实际基线运行中不容许门禁失败，
失败后的依赖步骤明确记录为跳过。

| 运行 / 尝试 | 测试提交 | 核心 | 客体工具链 | 实际观察 |
| --- | --- | --- | --- | --- |
| [#54 / 1](https://github.com/ananwanan/AnovaOS/actions/runs/40457781527) | `92b8a38` | 通过 | 失败 | 客体门禁前锁定 OVMF 主站 HTTP 503 |
| [#55 / 1](https://github.com/ananwanan/AnovaOS/actions/runs/40458309803/attempts/1) | `8eee351` | 失败 | 跳过 | 相同 OVMF 传输失败 |
| [#55 / 2](https://github.com/ananwanan/AnovaOS/actions/runs/40458309803/attempts/2) | `8eee351` | 失败 | 跳过 | 手动重跑根任务和依赖任务，仍返回 HTTP 503 |
| [#56 / 1](https://github.com/ananwanan/AnovaOS/actions/runs/40460883750) | `e2db30f` | 通过 | 失败 | 固件回退已实际执行；两个 musl 种子源超时 |
| [#57 / 1](https://github.com/ananwanan/AnovaOS/actions/runs/40463666492) | `1b63f42` | 通过 | 失败 | 源码工具链/C++ 通过；复制 UID/GID 1001 导致 ext4 挂载拒绝 |

手动重试通过 GitHub job rerun 执行完整依赖链；原始事件仍为 `push`，并非
`workflow_dispatch`。实际运行/任务/步骤
边界及失败保存在 [M16 CI 证据](../../assets/validation/M16_CI.json)。

官方 Launchpad OVMF 回退包已下载并匹配既有 SHA-256：
`a094c13d06f2740691ff57d108dff32aa087179363ddb0de42d463b4f7f9bc13`。
#56 在主站 HTTP 503 后实际使用此镜像。固件版本/提取摘要不变。仅传输错误允许回退；
错误摘要、损坏或重定向缓存均拒绝；独立 partial 文件会清理，发布检查并发冲突。
新增五项获取测试覆盖这些边界，#56 通过 230 项宿主测试。

#56 随后在编译器构建前失败，musl.cc 和 more.musl.cc 各有两次 HTTPS 连接超时。
新增社区镜像
`https://joeyparrish.github.io/musl-cc-mirror/x86_64-linux-musl-native.tgz`
已独立完整下载：89,080,066 字节，SHA-256 为
`eb1db6f0f3c2bdbdbfb993d7ef7e2eeef82ac1259f6a6e1757c33a97dbcef3ad`。
其字节符合既有种子锁，版本/许可证/摘要均不变。

#54 核心 ZIP（artifact `12712834879`、16,179,053 字节）已下载并匹配 SHA-256：
`0f876894f490bfd21784d9b6f6711569cd831fdd10a6af518f31957c9c8af44a`。
其中 45 份[精确结果记录](../../assets/validation/M16_CORE_CI54.json)已保留。
它仅证明核心任务，不将失败/跳过的客体门禁视为通过。

## 执行修正与剩余范围

首个 VFS 探针使用了 Linux 配置尚不接受的 mkdir 0700；修正为 0755，
未扩大权限范围。一次探索构建在模块装载时触及独立宿主 8 GiB 内存上限；
驱动现于 QEMU 前主动释放打包字节缓冲。上述中断会话不算验收；
本页成功的独立运行使用修复后代码。

声明的新 ext4 内核工作负载在真实耗尽/恢复后满足原有软件配额。
默认存储仍为 ext2；任意 ext4 格式、编译器源码重建、guest 独立镜像生成、更大 GUI
负载、SMP 和实机均需独立验收。完整远端基线通过后，下一桌面工作为
[M17](../milestones/M17_DESKTOP.md)。

#57 成功核心任务的 45 份[当前代码结果记录](../../assets/validation/M16_CORE_CI57.json)也已保留。
Artifact `12715692453`（16,179,359 字节）已独立下载并匹配 SHA-256：
`76d51dbf684ce5a86dbed3ef07b8281cefecb7a06355fd12f3778cb3076c58d9`。
依赖的客体任务随后在镜像所有权检查失败，此 ZIP 仅验收核心范围。

## 最终远端观察与仅 main 策略

#57 成功构建锁定的源码工具链并通过 guest C/C++ 门禁，随后 ext4 测试盘在写入前被拒绝。
`mke2fs -d` 将 runner 的 UID/GID 1001 复制到种子文件中，超出既有的 root 所有权
可写配置。guest artifact（ID `12717126551`、47,318,051 字节）已下载并匹配 SHA-256：
`8c0d62cede96b215d74c38dfb4791bc26240fcff341d582629fea4240379e6aa`。
完整失败 UART 及已执行/跳过步骤均保留在 CI JSON 中。

新建 write/journal 镜像构造现明确设定 root 所有权，仅对声明的种子路径使用 debugfs
规范化，再独立审计最终镜像。内核所有权准入不变。新增回归向两种格式注入 UID/GID
1001，确认拒绝，再修正所有权、核对 marker 字节并运行 e2fsck。54 项 xtask 测试
和严格 xtask Clippy 全部通过。

随后所有者要求仅在 main 构建，因此不再启动功能分支工作流。此修复与后续门禁的完整
远端复验留待 `main`；本地内核容量通过不等于完整远端 CI 通过。
