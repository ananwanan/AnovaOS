<!-- Define the bounded M15 build environment, guest bzip2 reconstruction and measurement contract. -->
# M15：受控源码重建

[English](../../en/milestones/M15_BOOTSTRAP.md) | **简体中文**

本配置提供 M15 首个构建环境和小型 C 工具重建，选择官方 bzip2 1.0.8 作为容量目标，复用 [M11 guest 工具](M11_TOOLCHAIN.md)，创建独立持久工作盘。实现契约与实测验收分离，后者见[当前状态](../guides/STATUS.md)。M15.3 的 [GCC](M15_GCC.md)、M15.4 的 [Rust/LLVM/Cargo](M15_RUST.md)独立评估；编译器重建、M15.5/M15.6 不属于本 bzip2 配置。独立 [M15.5 内核重建](M15_KERNEL.md)已把持久 ext2 文件上限提高到 4 MiB；下述原始 bzip2 验收保留测得的 1 MiB 配置，tmpfs/shared cache 仍为 1 MiB。

## 准备与验证

安装 [DEVELOPMENT](../guides/DEVELOPMENT.md) 宿主工具，获取锁定引导输入，先准备源码构建的 M11 compiler/sysroot：

```sh
cargo xtask fetch
cargo xtask doctor
cargo xtask toolchain-build
cargo xtask bootstrap-prepare
cargo xtask bootstrap-test
```

`bootstrap-prepare` 验证官方归档，把所需未修改源码、声明和样本放到 `build/bootstrap/source/`，将 M11 编译器和独立容量诊断打包到 `build/images/anova-bootstrap.iso`，创建含 `/persist/bootstrap` 的全新 `build/disks/bootstrap-TIMESTAMP.ext2`。准确路径、输入哈希、源码 commit/worktree、seed 清单和配置记入 `build/bootstrap/prepared.json`，不重格式化旧工作盘。

`bootstrap-test` 自行准备新盘并进行三次 QEMU 启动；全部场景、独立磁盘检查及文件系统检查成功后才写 `build/logs/bootstrap.result.json`。各启动的 serial/QEMU/result 日志前缀为 `bootstrap-capacity`、`bootstrap-build`、`bootstrap-reboot`；持久源码/输出哈希与后两组日志一起保存。缺少成功结果不算验收；最终结果嵌入准备清单，避免后续准备覆盖已验收运行的 source/image/seed 身份。

纯源码步骤可不构建/启动镜像单独检查：

```sh
python3 ports/bootstrap/prepare.py
python3 ports/bootstrap/prepare.py --verify
python3 ports/bootstrap/test_prepare.py
```

缓存归档或选中源码变化会失败。移除改变的生成缓存并重建前，先把有意修改另存；脚本不静默覆盖。

## 构建角色与来源

build/host 均为 x86_64 AnovaOS，通过显式 `linux.x86_64.v1` personality，目标是 static non-PIE `x86_64-linux-musl` 用户程序，独立于原生 SDK 和 bare-metal 内核目标。

M11 seed 为 GCC 11.2.0、binutils 2.37、make 4.4.1、musl 1.2.4 和 `/tc`。宿主提供 seed、下载/检查/选择源码、导入工作盘、打包 ISO 并检查输出，不编译 bzip2。宿主另行编译的 `/bin/m15-capacity` 是诊断测试样例，不是重建工具或 compiler-bootstrap 证据。

`ports/bootstrap/inputs.json` 锁定[官方归档](https://sourceware.org/pub/bzip2/bzip2-1.0.8.tar.gz)的 SHA-256 及 [Sourceware checksum](https://sourceware.org/pub/bzip2/sha512.sum) 中 SHA-512，二者在读取成员前验证。仅接受准确发行版 root 内规范 regular-file，拒绝链接、重复路径、traversal 和超过可写文件上限的选中输入。生成 `INPUTS.json` 记录机器角色、seed 和每文件哈希，`SOURCE.sha256` 记录选中源码/fixture。

选中内容含八个 C 编译单元、两个头文件、原始 Makefile/LICENSE/README/CHANGES/HTML manual，以及三组压缩/参考样本，不修改上游源码。1,766,625 字节的可选 PostScript manual 超出原始 1 MiB 选择预算，故排除；无编译单元因此缺失。自定义 recipe/负例明确属于第一方。许可见[第三方边界](../third-party/m15.md)。

## 容量与测量

环境为 QEMU q35/TCG、单 CPU、1 GiB RAM、无 NIC；Linux work disk 为 16 MiB、512 inodes、4 KiB blocks、128-byte inodes。原验收持久文件上限 1 MiB，M15.5 后为 4 MiB；tmpfs/shared cache 1 MiB。VFS 2,048 nodes、每进程 32 fd、128 共享 open descriptions、32 task/thread slots、16 MiB 内核 heap 保持不变。bzip2 保留原源码选择，磁盘格式和 ABI 标记兼容。

首启动真正触发容量拒绝：超大持久 resize 返回 `EFBIG` 且旧长度/字节保留；fd 耗尽返回 `EMFILE`，close/reopen 恢复；子进程等待 pipe EOF 直到 task 上限返回 `EAGAIN`，随后全部退出回收并可新 fork/wait。这些是刻意耗尽测试，不代表普通 bzip2 所需资源。

可选 `bootstrap-metrics` 仅在 scheduler boundary 采样，不增加 syscall ABI。关机汇总含初始/观测最小 free 物理 frames、frame delta、heap use、live/occupied task slots、每进程 fd、VFS node slots、样本 count。每 scheduler instance 重新计数，capacity/build/reboot 分开，防止压力污染构建指标。

采样遗漏 syscall 内暂时分配；frame delta 含内核分配，并非进程 RSS，VFS slot 可含保留复用槽。IRQ 耗时 clock 不计全部内核时间，不是编译器 CPU benchmark。宿主 wall time 另含 firmware/boot/UART/shutdown。关机后检查 disk blocks/inodes、逻辑字节、最大文件和准确哈希；最终盘占用不等于瞬时峰值。

## guest 重建与恢复

第二次启动在原生 serial shell 执行：

```sh
cd /persist/bootstrap
make -f Makefile.anova -j1
./bzip2 -1 -kc sample1.ref > sample1.out.bz2
./bzip2 -dc sample1.out.bz2 > roundtrip
./bzip2 -t sample1.out.bz2
./bzip2 -t README
shutdown
```

recipe 用 guest GCC 顺序编译八个对象并链接 `/persist/bootstrap/bzip2`，使用 `-O1 -g0`、既有 wrapper static non-PIE flags 和 `/tc`。不运行 upstream configure、不重建编译器、不生成独立 libbz2 归档。检查全部状态，把 README 当压缩输入必须返回 2。

正常关机后宿主解析持久 ELF、检查 executable mode/输入哈希，将 round-trip 与官方样本比较，并用 Python bzip2 独立解码。block size 1 还要求与官方 `sample1.bz2` 压缩字节完全一致。

第三次启动运行保留 compressor，再执行 `make -f Makefile.anova fail`，对 `rejected.c` 真正编译至刻意 `#error`、退出 2；旧 compressor 必须仍可校验保存的压缩流。再次正常关机后所有源码/输出哈希须不变。该证据仅涵盖指定编译器 rejection，不涵盖中断链接或任意失败的事务性替换。每启动都要求 clean 关机和只读 `e2fsck -f -n` 通过。

## M15 其余范围

本受控 C 工具配置不证明通用 configure 项目、GCC 单/多阶段 bootstrap、LLVM/rustc/Cargo 源码重建、guest 内核、guest image 生成或两次独立 clean 构建。独立 [GCC](M15_GCC.md)/[Rust](M15_RUST.md)提供源码审计与真实 prerequisite/blocker probe，而不重建编译器。独立 [M15.5](M15_KERNEL.md) 已有[验收](../validation/M15.5.md)，证明保留 guest ELF 和实际启动；[M15.6](M15_REPEAT.md) 已有[验收](../validation/M15.6.md)，证明两轮 clean bzip2/kernel、声明最终产物比较和失败/重启恢复。本单项配置不替代这些验收。
