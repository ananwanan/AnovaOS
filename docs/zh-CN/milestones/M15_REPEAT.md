<!-- Define independent clean guest builds, retained evidence and bounded comparison/recovery acceptance. -->
# M15.6：重复 guest 重建

[English](../../en/milestones/M15_REPEAT.md) | **简体中文**

本配置在独立 clean work disks 重复已完成 [bzip2](M15_BOOTSTRAP.md) 和 [内核](M15_KERNEL.md) recipe，比较声明输入/输出并保留原功能、拒绝和重启检查。此处描述实现契约，实测见 [M15.6 记录](../validation/M15.6.md)和[状态](../guides/STATUS.md)。

## 准备与执行

按 [DEVELOPMENT](../guides/DEVELOPMENT.md) 准备启动输入、M11 GCC/sysroot、M12 运行时，并准备 locked registry 归档：

```sh
cargo fetch --locked
cargo xtask bootstrap-repeat-test
```

命令须独占运行。repeat 门禁与已有 image 门禁共用生成 apps/image/preparation 路径，并发会覆盖使用中的输入。

共两轮，每轮顺序完整运行 `bootstrap-test`、`bootstrap-kernel-test`。每组件建新盘而非清理/复用旧目录。bzip2 盘仅有源码、无 object/ELF；内核盘没有 Cargo target 目录、编译对象文件或保留内核。第二轮相同 source/recipe/seed，不接收第一轮 guest 输出。

bzip2 为 q35/TCG 单 CPU、1 GiB RAM、无 NIC；内核为单 CPU、3 GiB、无 NIC。guest paths/flags 沿用组件。本证据是同宿主环境下两独立 workspace，不是两独立 provisioned 宿主。

## 保留证据

每 invocation 在 `build/bootstrap-repeat/` 下独立目录，保留完整 repo `source/`、`source-files.json`、`environment.json` 和实测宿主 identity。在后续覆盖共享输出前保存 `round-1/`、`round-2/`、prepared/source/seed 清单、结果报告、完整 serial/compiler 日志、guest 输出 hashes/artifacts。每 `round-N/bzip2/`、`round-N/kernel/` 含 `result.json`、`prepared.json`、`inputs.json`、`invocation.log`、逐 session 日志、seed 清单、`initramfs.anovafs`、`seed.iso`、已停 `work.ext2` 副本；内核另含 `guest.iso`、`kernel.elf`、`kernel.readelf.txt`。每组件有完整 `inventory.json`，原 work disks 路径亦独立保留。

清单记 revision/worktree、精确源码哈希、seed/target libs、guest environment/target/flags/caps/host role；提交单独不标识未提交快照。嵌入内核构建 identity 覆盖输入/recipe 并在启动重验，ELF 哈希标识全部输出字节。

两轮和比较均成功后才发布 `build/logs/bootstrap-repeat.result.json`、`result=Ok`。失败/中断不能遗留旧 aggregate 成功；比较报告即使不通过也保留观测差异。原组件报告独立定界。失败 child 的 canonical logs/manifests 放明确 `unvalidated/`，可能 partial/stale，不构成验收。

## 比较契约

先要求 source/seed/recipe/environment 相同，再解释输出；变输入、缺 artifact、复用 work disk 均使比较无效。比较八个 bzip2 对象文件、链接后的 ELF、compressed golden 样本、decompressed 字节；组件已分别验证 ELF 准入、实际运行和宿主对官方样本独立解码。

比较完整内核 ELF bytes/hashes，各存 `readelf` header/section/segment/note。任一 mismatch 导致 aggregate 失败，不能删除 timestamp/path/build identity/metadata 强行一致。全文件相同与功能一致是两观察；有差异需记录位置/解释后才能限定验收。

fresh disk names、filesystem 元数据、宿主 wall time、采样的资源、诊断 timing 及 ISO creation timestamp 可变，后者可使 image 哈希不同而 packaged 内核字节相同。这些 environment/packaging 差异与最终 artifacts 分开报告。内核 intermediate libraries/generated config/Cargo cache 可变，其原始清单差异保留，不声称整构建 dir 复现。每 ISO 独立提 `/boot/kernel.elf`，要求等于该轮 guest ELF。

## 失败与重启恢复

每 bzip2 流构建/执行后重启，并触发刻意 C rejection，旧 ELF 仍可运行，完整 source/output 清单不变。每内核流构建/发布、精确打包且不宿主 rebuild、真实启动核 identity，Rust syntax rejection 保留旧内核，再重启保留恢复清单。

每组件三 QEMU sessions，两轮共十二。每正常关机 flush 存储、确认 DMA 重置，每停盘只读 `e2fsck -f -n` 通过。C/Rust 有界负例不证明所有中断链接、完整失败内核 Cargo 构建或无日志 ext2 断电场景的事务恢复。

## 范围

宿主仍准备 source/seed、编初始 kernel/apps/diagnostics、建盘、打包 ISO、提 artifact、驱动验证；guest 编译选定 bzip2 和真实内核。继承 GCC/Rust/Cargo/rust-lld/precompiled bare-metal libs 仍是 seeds。

[GCC](M15_GCC.md) 与 [Rust/LLVM/Cargo](M15_RUST.md) 仍是有 blocker 的 feasibility assessment。本门禁不重建这些编译器、不在 guest 完整生成 image、不重建 GUI-enabled 内核、不实现 native std。两次匹配仅证明声明条件下的比较，不代表 general 可重现性、摆脱继承的编译器或从纯源码 bootstrap。
