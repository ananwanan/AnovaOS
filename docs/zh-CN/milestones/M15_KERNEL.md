<!-- Define guest kernel reconstruction, exact retained-ELF packaging and reboot acceptance boundaries. -->
# M15.5：guest 内核重建

[English](../../en/milestones/M15_KERNEL.md) | **简体中文**

本配置用运行于 AnovaOS Linux-musl 进程的继承 M12 Rust/Cargo 重建真实内核。宿主准备锁定输入并把保留 guest ELF 打包成启动镜像；这与重建 Rust、目标 libraries 或 guest 内完整镜像生成分开。实际验收见[状态](../guides/STATUS.md)和[证据](../validation/M15.5.md)。

## 命令与依赖

使用锁定宿主 Rust 及 [DEVELOPMENT](../guides/DEVELOPMENT.md) 工具，包括 `musl-gcc`、`xorriso`、QEMU、OVMF、`e2fsck`。尚无 M12 工具时先准备：

```sh
cargo xtask fetch
cargo xtask rust-build
python3 ports/bootstrap/test_kernel_prepare.py
cargo xtask bootstrap-kernel-test
```

门禁在 `build/disks/` 建新盘，不格式化旧盘。与其他镜像构建门禁互斥顺序运行，因共享宿主 app/runtime 输出。单 BSP q35/TCG，3 GiB RAM，无 NIC。

源码准备也可独立检查：

```sh
python3 ports/bootstrap/kernel_prepare.py
python3 ports/bootstrap/kernel_prepare.py --verify
```

仅下载缺失锁定 bare-metal 目标组件，所需 registry 归档须在宿主 Cargo cache，准备过程不编译内核。缺归档先 `cargo fetch --locked`。已变 cache/重定向/修改发布结果明确失败；有合法第一方变更时可刷新完好的快照，`--verify` 只读且拒绝过期。

## 输入与机器角色

| 宿主输出 | guest 路径 | 角色 |
| --- | --- | --- |
| `build/bootstrap-kernel/source` | `/ks/source` | 只读准确 kernel/local crate/linker 字节、隔离元数据、完整 locked registry 源码 |
| `build/bootstrap-kernel/seeds/rs` | `/rs` overlay | 官方继承 bare-metal libraries 和未改声明 |
| M12 Rust 运行时 | `/rs` | 官方继承的 Linux-musl Rust/Cargo 1.90.0、rust-lld、宿主 std/runtime dependencies |
| 宿主 `kernel-build.elf` | `/bin/m15-kernel-build` | 构建 recipe、有界日志、ELF 准入和成功发布 |
| 新 ext2 disk | `/persist/kernel` | Cargo outputs/logs、保留 ELF 和失败测试样例 |

`kernel-inputs.json` 用 SHA-256 锁官方 Rust 1.90.0 `x86_64-unknown-none` 目标归档。precompiled `core`/`alloc`/`compiler_builtins` 是继承 seed，通过只读 initramfs 提供，独立于可写上限。[官方目标文档](https://doc.rust-lang.org/rustc/platform-support/x86_64-unknown-none.html)不提供 native AnovaOS std。

kernel/crate/linker 第一方文件保持当前准确字节。准备 workspace 排除 host-only xtask、默认 member 改为内核；隔离 Cargo lock 的 package versions/registry/checksum 须与仓库 lock 条目一致。kernel/local crate 清单不变；复制 config 保留内核 flags 并增加 offline 目录 replacement，保留完整 upstream crate archive/license，`.cargo-checksum.json` 验证准备内容。

`build/bootstrap-kernel/inputs.json` 分列原始 repo 哈希、generated 元数据、registry locks、继承的 library 哈希，记录源码 revision/worktree。未提交快照由精确 file 哈希标识，base 提交不足以标识修改源码。source/seed namespace 通过归档 path/component 检查和 1,500-entry budget；内核镜像去 `/tc` 以保持 2,048 VFS 上限。

## guest 编译与发布

驱动切到 `/ks/source` 加载 offline vendor config，实际命令：

```sh
cargo build --offline --locked --release \
  --target=x86_64-unknown-none --features=bootstrap-metrics \
  --manifest-path=/ks/source/Cargo.toml -p anova-kernel \
  --target-dir=/persist/kernel/target
```

驱动设置单 job、无 incremental、`opt-level=z`、一个 codegen unit、无 debug、strip symbols、panic abort；目标 flags 为 static relocation/kernel code model/no red zone。显式目标将其与 Linux-musl 构建 script flags 隔离。继承 launcher 选 `/rs` sysroot/rust-lld、一个 linker thread、file-write 而非 writable 共享 mmap。不替入宿主目标 object/rlib/kernel ELF。

构建 identity 覆盖 source/target-lib 清单、driver/launcher/orchestration 源码、继承的运行时清单，经 `ANOVA_KERNEL_BUILD_ID` 嵌入，serial 初始化后报告 `[BOOT:BUILD] id=...`，将实测启动绑定到输入/recipe；独立 ELF SHA-256 标识输出全部字节。

launcher 保留 `TMPDIR=/persist/kernel/tmp`，允许中间元数据超过 tmpfs 上限；最多 80 args 并为注入选项留余量，locked smoltcp invocation 需 71 args。

Cargo 输出写有界持久日志并同步 serial。仅成功编译后发布；校验 static ELF64 x86_64、program-header 边界、high-half load、entry executable coverage、无 W+X。先复制 `kernel.candidate`、sync/validate，再 rename `kernel.elf`，失败保留旧内核。

持久 ext2 上限为 4 MiB，使用原 direct/single-indirect 布局；tmpfs 1 MiB、disk format/16 MiB/512 inodes、16 MiB heap、32 tasks、2,048 nodes 不变。更大上限用于选定 smoltcp intermediate rlib。每次写操作保存一份私有间接表快照，减少小块写入造成的重复设备读取，data/pointer/length 发布顺序不变；这不解除 [GCC](M15_GCC.md)/[Rust](M15_RUST.md) 编译器 bootstrap 前置条件。

## 保留 ELF 打包与实际重启

三次 QEMU：

1. 启宿主 seed 内核，在 guest 编译/发布内核，正常关机；宿主重新挂盘、验证 ELF、保存准确字节到 `build/bootstrap-kernel/guest-kernel.elf`。
2. 用锁 bootloader/app 输入打包保留 ELF，此路径绝不宿主 rebuild 内核；宿主独立提取 ISO `/boot/kernel.elf`，启动前逐字节比较。新内核报预期 identity 并通过 boot/runtime 命令。真实 Rust 语法错误保留旧内核后正常关机；负例是向未发布 candidate 编译刻意非法小源文件，并非完整失败内核 Cargo rebuild。
3. 再启同 image、核 identity/app、正常关机，workspace 全 file 哈希须与前次恢复一致；每停盘独立 `e2fsck -f -n`。

记录 source/seed 清单、初始宿主内核、宿主驱动、initramfs/ISO、guest ELF/hash、serial/compiler 日志、shutdown/resources。scheduler 样本不是 RSS，IRQ clock 漏部分内核 execution；全部成功才发布 `build/logs/bootstrap-kernel.result.json`，`result=Ok`。

宿主仍建盘、提供 compiler/target libs、建首镜像、提 guest 输出、打包二镜像并验证。guest-only image、新编译器、GUI-enabled 内核和 general 可重现性需独立证据。[M15.6](M15_REPEAT.md) 已[验收](../validation/M15.6.md)两 clean 内核 ELF/bzip2 最终输出；原始内核 intermediate 差异保留，不声称整目录相同。正常重启不证明无日志 ext2 断电恢复。
