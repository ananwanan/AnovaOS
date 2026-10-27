<!-- Record the ext4 kernel workload, declared budgets, exhaustion recovery and actual CI evidence. -->
# M16 内核容量与 CI 结项

[English](../../en/validation/M16_CAPACITY.md) | **简体中文**

记录于 2026-10-27（Asia/Shanghai），分支 `feat/m16-foundation`，实现基线 `91d2b32`。
新门禁已经实现，实际验收结果在执行后记录于下文。既有 [M16.5](M16.5.md)/
[M16.6](M16.6.md) 保留原本的 bzip2 与恢复范围。默认存储仍为 ext2。

## 可复现负载与预算

```sh
cargo xtask fetch
cargo xtask toolchain-build
cargo xtask rust-build
cargo fetch --locked
cargo xtask foundation-capacity-test
```

使用既有 [M15 内核配方](../milestones/M15_KERNEL.md)：精确的当前内核/本地 crate/链接脚本、
校验锁定的离线 Cargo 依赖，以及继承的官方 Rust/Cargo/rust-lld 1.90.0 和匹配的预编译
裸机库。build/host 是 AnovaOS 的 x86_64 Linux-musl personality，target 是
`x86_64-unknown-none`；单 job/codegen unit、opt-level z、去符号、panic abort。
特性选择 `ext4-write,bootstrap-metrics`。输入源码位于只读 initramfs；全部构建产物与
临时文件写入新盘。此负载重建内核，不重建继承编译器，也不独立生成完整启动镜像。

比较前，`budgets.json` 写入以下上限：

| 资源 | 上限 |
| --- | --- |
| 虚拟机 | q35/TCG/OVMF、1 BSP、3 GiB RAM、无网络 |
| 内核堆 | 16 MiB |
| 采样物理页增量 | 524,288 个 4 KiB 页（2 GiB） |
| VFS 槽 | 2,048 |
| 进程/线程槽；每进程 FD | 32；32 |
| 新建 JBD2 工作盘 | 64 MiB、4,096 inode、8 MiB 内部日志 |
| 每会话看门狗 | 3,600 秒 |

内存在调度边界采样，遗漏 syscall 内临时峰值；物理页增量包含内核分配，不是精确峰值 RSS。
ext4 在每次成功修改后记录分配计数，即使临时文件随后删除，也保留本次挂载的块/inode
高水位。这包含文件系统元数据和日志，排除被拒绝或失败的事务，并在重新挂载时重置。
最终当前计数必须与独立读取的停机超级块一致，镜像必须通过 `e2fsck -fn`。

## 验收顺序与证据

1. 新建带明确 Linux ABI 标记的 JBD2 盘，要求没有旧内核工作区。启动并触发真实堆 OOM、
   EFBIG/EMFILE/EAGAIN、两轮 2,048 VFS 槽耗尽及磁盘 ENOSPC。核对字节/偏移保持、
   删除与资源复用；执行原生应用并正常关机。
2. 在同一张恢复后的盘上，从干净工作区实际构建内核。构建的内存/VFS/FD/任务采样和
   磁盘临时占用峰值与主动压力分开记录。独立检查保留的 guest ELF 和工作区全部字节。
3. 不调用宿主内核重建，直接打包保留 ELF，提取并比较 ISO 字节后启动新内核。
   运行真实 rustc 语法拒绝，保留旧可用 ELF，执行原生应用并正常关机。
4. 再次启动同一新内核，要求嵌入标识精确一致、工作区清单不变。每轮停机镜像必须通过 fsck。

证据保留于 `build/foundation/capacity-TIMESTAMP/`：预算、完整输入/实现哈希、诊断程序、
initramfs、初始/新内核 ISO、工作盘、guest ELF、各会话 UART/QEMU/结果文件和结果 JSON。
CI 无论成功失败均随既有 foundation 证据上传此目录。失败会话保留诊断，不发布成功结果。

## 远端 CI 执行

工作流支持手动触发、推送 `dev`/`feat/m16-foundation` 和目标为 `dev` 的 PR。
两个完整 job 与全部既有门禁保留。guest-toolchain 看门狗调整为 180 分钟，以容纳额外的
独立 ext4 内核重建；不忽略失败。已连接的 GitHub 插件可读取运行和发布提交，但未提供
workflow-dispatch 操作。浏览器登录已尝试；认证没有返回可验证的登录成功页面，后续浏览器
操作超时。因此使用分支 push 提供独立可执行的触发入口，实际事件必须写作 `push`，不能
写作 `workflow_dispatch`。未取得实际运行前不声称手动触发成功。

实际运行/提交/job/artifact 标识与最终结果等待执行后填写。
