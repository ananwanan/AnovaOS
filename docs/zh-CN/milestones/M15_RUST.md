<!-- Define independent Rust, LLVM and Cargo source-build assessments and their seed/provenance boundaries. -->
# M15.4：Rust、LLVM 与 Cargo 可行性

[English](../../en/milestones/M15_RUST.md) | **简体中文**

本配置用官方 Rust 1.90.0 源码 distribution 分别评估三条源码路线，审计完整归档并在真实 AnovaOS guest 验证部分前置条件。成功报告可判定当前受阻；不重建 LLVM、rustc、Cargo、库或内核。本地执行见 [M15.4 验收](../validation/M15.4.md)。

## 执行评估

先准备宿主工具、M11 GCC/sysroot 和 M12 官方 musl tools：

```sh
cargo xtask fetch
cargo xtask doctor
cargo xtask toolchain-build
cargo xtask rust-build
python3 ports/bootstrap/test_rust_probe.py
cargo xtask bootstrap-rust-test
```

纯源码准备/只读验证：

```sh
python3 ports/bootstrap/rust_probe.py
python3 ports/bootstrap/rust_probe.py --verify
```

[`rust-inputs.json`](../../../ports/bootstrap/rust-inputs.json) 锁定带日期官方归档、published checksum URL 和 SHA-256。读取前验归档；记录整树/各组件清单，只把有界未修改上游文件发布到 `build/bootstrap-rust/source/`，`INPUTS.json`/`SOURCE.sha256` 记录 member、size、digest，不导入整份 distribution。拒绝变更 cache、危险路径和重定向输出；`--verify` 不写入。

## 组件边界与前置条件

拟议 build/host/target 通过显式 Linux personality 使用 `x86_64-unknown-linux-musl`，独立于 native SDK 和 `x86_64-unknown-none` 内核。M12 seed 是官方 Rust 1.90.0 binaries；M11 seed 是 GCC/G++ 11.2.0、binutils 2.37、make 4.4.1、musl 1.2.4。小应用编译只证明该测试样例内 seed 可工作。

| 组件 | 源码路线 | 前置条件及解释 |
| --- | --- | --- |
| LLVM | Rust 内含 LLVM 20.1.8 fork，独立清单 | 有效 C++17 编译器、CMake ≥3.20.0、构建 generator；支持 make generator 时 Ninja 可选。小 C++17 测试样例不证明完整 LLVM 兼容。 |
| rustc | Rust 1.90.0 compiler/bootstrap | 锁定 `src/stage0` 指定 2025-08-07 Rust 1.89.0。Python 入口先构建 Rust bootstrap，再用 Cargo 构建 std/compiler 阶段。M12 1.90.0 不证明指定 stage0 已安装或可工作。 |
| Cargo | distribution 内 package 0.91.0 | 清单要求 rustc ≥1.88，workspace 默认 1.86；需要 Rust/Cargo seeds、自身 locked graph 和 C 构建 dependencies。须选定 musl 下 system-library discovery、OpenSSL/vendored 方案。M12 小应用 vendor cache 不是 Cargo 闭包。 |

保留的发行版构建 files/manuals 是权威要求；移动分支/通用指南不覆盖已审计发行版：

- [Rust 1.90.0 安装](https://github.com/rust-lang/rust/blob/1.90.0/INSTALL.md)。
- [stage0 lock](https://github.com/rust-lang/rust/blob/1.90.0/src/stage0)。
- [bootstrap guide](https://github.com/rust-lang/rust/blob/1.90.0/src/bootstrap/README.md)。
- [组件声明](https://github.com/rust-lang/rust/blob/1.90.0/.gitmodules)。

Cargo 源码 package 版本不同于分发命令 `cargo 1.90.0`。归档内 Rust identity 为 `1159e78c4747b02ef996e55082b704c09b970588`，不依赖可变 LLVM/Cargo 分支。评估不安装/晋升任何源码组件；复用 prebuilt LLVM 须注明继承的，替代 stage0 须显式配置并实测，不能静默替换上游发行版 seed。

## 真实 guest 检查与重启

`bootstrap-rust-test` 新建 16 MiB/512-inode Linux work disk，两次 q35/TCG 启动，单 CPU、3 GiB RAM、无 NIC。普通 ABI/runtime/fs limits 不变；增加 RAM 仅容纳 M12 tool image，不提高 file/namespace 上限。

首次启动用继承的编译器分别编译/运行 Rust std 和 C++17 app，各得 42；诊断真实执行 `python3`、`cmake`、`ninja`、`git`、`perl`、`pkg-config`，逐 child 验 `ENOENT`/127，并捕获 Cargo offline dependency 失败。这是选取 upstream 清单一个依赖的隔离 probe，不是完整 Cargo 构建；缺 Ninja/Git/Perl/pkg-config 不说明每种构建配置都必需。

对原样 `rust-x.py` 尝试 `python3 rust-x.py --help`，exec 失败证明当前入口不能启动，未执行 Python、未验证后续 bootstrap 失败。

依赖 `curl = "=0.4.48"` 与 audited manifest/lock 匹配，测试样例 `.cargo/config.toml` 选择继承 `/rs/vendor`，要求 Cargo 101 并真实报告缺少 curl，只证明该 vendor 配置缺此依赖，不评估完整发行版 vendor 或所有 prerequisite。`src/stage0` 静态审计并分开记录 seed 版本，不执行 1.89.0 stage0。

预期失败均须有指定非零状态；缺诊断、意外成功、输出溢出或超时都失败。负例后成功应用仍能运行；第二次启动再执行并要求全部 workspace 字节相同。两次 clean 关机后宿主解析 guest ELF、核 executable modes/输入字节，并要求只读 `e2fsck -f -n` 通过。

刻意 Rust 编译错误须非零且保留旧 ELF。Rust seed 测 std vector、工作线程 thread、持久字节；C++17 seed 测 fold expression/std::optional，均不含编译器组件源码。宿主审计源码、构建诊断、打包 seed、建盘/驱动/检查；guest 只编译声明的第一方测试 app，不以宿主 app 替代，也不把它们算编译器重建。

## 容量与证据

审计整 distribution/各组件的 file count、logical 字节、namespace nodes、4 KiB-rounded data 和超 1 MiB 文件。完整发行版含 tests/docs/其他 targets/vendor，不是最小闭包或真实编译器构建测量；裁剪需独立闭包和 generated-output budget。下表组件划分整个归档，remaining Rust 含 compiler/std/bootstrap/tests/docs/tools：

| 归档分区 | 普通文件数 | 逻辑字节数 | 超过 1 MiB 的文件数 |
| --- | ---: | ---: | ---: |
| LLVM 项目 | 72,514 | 830,210,925 | 48 |
| Cargo 源码 | 2,665 | 16,316,850 | 1 |
| vendor 依赖 | 92,988 | 1,736,948,141 | 118 |
| 其余 Rust 及其他文件 | 74,244 | 760,681,217 | 75 |
| 完整发行版 | 242,411 | 3,344,157,133 | 242 |


全 namespace 至少 279,266 nodes，仅 regular-file 4 KiB 向上取整为 3,994,664,960 字节，尚无 metadata/output。142 个 in-root symlink 仅 inventory/验证、不解压/跟随，发布内容只含普通文件。Cargo lock 共 537 packages（512 registry），也含特定目标闭包外依赖。

原评估独立限制是 16 MiB/512-inode work disk、2,048 VFS nodes、1 MiB writable files、16 MiB heap、32 task slots，增 RAM 不解除。完整源码构建的 temp/memory/fd/task/time demand 未测；scheduler 样本不是 RSS，停机盘分配不是瞬时峰值。

全部成功才发布 `build/logs/bootstrap-rust.result.json` 并嵌入 audit/preparation identity。`build/bootstrap-rust/prepared.json` 记录 branch/commit/worktree、implementation 哈希、导入字节、seed 清单、宿主诊断、ISO/initramfs/config。逐启动日志前缀 `bootstrap-rust-`，应整体保存。

重建前先选闭包、安装/测实际工具和 native dependencies、提供所需 stage0，再以 overflow/rollback/旧盘兼容测试扩容，顺序实测首构建并验证工具功能。完整 Rust bootstrap、新 LLVM/Cargo、native AnovaOS std 与 guest-only image 仍独立待验证；guest 内核已由 M15.5 使用原受测 rustc 单独验收。

## M15.5 后的容量

保留原 1 MiB file profile/source audit 与历史验收比较。[M15.5](M15_KERNEL.md) 把当前持久 ext2 上限提到 4 MiB，tmpfs/readonly 共享 cache 仍 1 MiB。完整编译器树仍超 disk/inode/VFS，工具依赖缺失仍存在。旧审计不声称当前持久 limit 仍 1 MiB，也不是裁剪闭包的新容量评估。
