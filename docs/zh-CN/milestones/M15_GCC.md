<!-- Define the locked GCC feasibility assessment, real guest blockers and prerequisites for compiler reconstruction. -->
# M15.3：GCC 源码构建可行性

[English](../../en/milestones/M15_GCC.md) | **简体中文**

本配置可重复评估 GCC 11.2.0 路线，审计锁定源码归档，在 guest 运行继承 C++ 编译器，并诊断缺失工具、开发头文件和容量。成功评估可得出“重建受阻”，不产出新 GCC 阶段。实际执行见 [M15.3 验收](../validation/M15.3.md)。

## 运行评估

按 [M11_TOOLCHAIN](M11_TOOLCHAIN.md) 准备宿主工具和验证过的 M11 compiler/sysroot：

```sh
cargo xtask fetch
cargo xtask doctor
cargo xtask toolchain-build
python3 ports/bootstrap/test_gcc_probe.py
cargo xtask bootstrap-gcc-test
```

纯源码评估可独立运行：

```sh
python3 ports/bootstrap/gcc_probe.py
python3 ports/bootstrap/gcc_probe.py --verify
```

要求 `toolchain-build` 已将七个归档缓存在 `build/toolchain/`，不补下载、不信任已解压树。URL/SHA-256 锁在 `ports/gcc/inputs.json`，继承的 `bootstrap.tgz` 二进制包不列入源码清单。先验归档哈希再读取成员，拒绝歧义/重复路径、traversal、links、special files，仅发布不超过 guest 可写文件上限的选中常规 files。

生成 `build/bootstrap-gcc/source-audit.json` 和有界纯源码 `build/bootstrap-gcc/source/`；拒绝已变更输出/重定向路径。删除生成缓存前另存有意修改；`--verify` 只读。每文件记录 archive/member/length/SHA-256，另有 `INPUTS.json`/`SOURCE.sha256`。原始 GCC configure 启动初期、发行版 prerequisites 和声明原样保留；这不是完整 GCC 树或已配置构建。

## 机器角色与继承工具

被评估 build/compiler host/application 目标均为 AnovaOS x86_64 Linux-musl personality，独立于内核 bare-metal 目标和 native SDK。候选编译器用 `/tc` 为 `x86_64-linux-musl` 受控 static 配置构建 C/C++。

seed 为宿主准备并全清单核验的 GCC/G++ 11.2.0、binutils 2.37、make 4.4.1、musl 1.2.4。锁定依赖 GMP 6.2.1、MPFR 4.1.0、MPC 1.2.1 满足保留的 GCC `INSTALL/prerequisites.html` 要求：C++11 编译器、GNU make ≥3.80、GMP ≥4.3.2、MPFR ≥3.1.0、MPC ≥1.0.1 等，但实际可用性/运行语义仍需 guest 验证。

宿主审计/选择源码、打包 seed、编译两个第一方诊断、建盘、启 QEMU 和检查输出，不重建 GCC/数学依赖，也不编译 seed-test app。`/bin/m15-gcc-probe`、`/bin/m15-capacity` 是诊断；仅 guest 编译 C++11 测试样例。

## guest probe 与恢复契约

`bootstrap-gcc-test` 创建新 16 MiB Linux work disk（512 inodes、4 KiB blocks、128-byte inodes），源码/notices/fixtures 放 `/persist/gcc-probe`。两次 q35/TCG 启动：单 CPU、1 GiB RAM、无 NIC，不格式化旧盘，不改变普通镜像/ABI。

首次启动要求：

- guest `g++ -std=c++11 -O1` 编译 `seed.cpp`，static ELF 运行 lambda、initializer-list/vector 和 unique_ptr，结果 42，仅证明这个 seed 测试样例。
- 分别真正编译引用 `gmp.h`、`mpfr.h`、`mpc.h` 的源码，均以 status 1 和对应 missing-header 失败，旧 seed ELF 仍能运行。
- 在 `/bin:/tc/bin:/linux` 下对 `sed`、`awk`、`grep`、`expr`、`tr`、`sort`、`cmp`、`mv`、`ln` 调用 `execvp`，每 child 经经检查的 pipe 返回真实 `ENOENT` 并退出 127。
- 用 `/linux/sh` 启动未改 GCC configure，明确 `--srcdir=.`、匹配 build/host/target、`--disable-bootstrap --enable-languages=c,c++ --disable-multilib`。须捕获缺少 `sed` 且 configure 失败；缺少源码目录不算满足。
- 真容量诊断验证 `EFBIG`/`EMFILE`/`EAGAIN` 与 file/fd/fork-wait 恢复，随后 seed 仍可运行。

这仅是 configure **启动初期** 子集，未执行后续配置、编译器源码、生成 Makefile 或 dependency 测试；补 sed 不代表 configure 可完成。输出有界且宿主有超时，overflow/hang/意外退出/缺少预期错误均失败。

正常关机后宿主独立检查全部导入字节、guest seed ELF 和 executable 模式，运行只读 `e2fsck -f -n`。第二次启动再执行保留 seed、正常关机并检查盘，所有输入/输出哈希一致。没有新编译器阶段安装/晋升；恢复针对旧 seed 和该诊断应用。

## 源码容量审计及解释

以下锁定完整发行版归档含测试、文档、其他 target/language，**不是** C/C++ 编译器最小闭包：

| 版本 | 普通文件数 | 逻辑文件字节数 | 超过 1 MiB 的文件数 |
| --- | ---: | ---: | ---: |
| GCC 11.2.0 | 108,258 | 620,324,686 | 65 |
| binutils 2.37 | 25,646 | 259,643,675 | 26 |
| make 4.4.1 | 395 | 8,780,526 | 0 |
| musl 1.2.4 | 2,622 | 3,407,989 | 0 |
| GMP 6.2.1 | 2,094 | 15,594,737 | 1 |
| MPFR 4.1.0 | 539 | 11,381,943 | 1 |
| MPC 1.2.1 | 340 | 3,693,749 | 0 |
| 合计 | 139,894 | 922,827,305 | 93 |


含隐式目录的合计 namespace 至少 145,684 nodes；仅 regular-file 按 4 KiB 向上取整需 1,358,241,792 字节，尚无目录、inode tables、构建 output/temp。这超过 16 MiB/512-inode 盘及 2,048-node VFS，GCC 单独也超过；当前无法导入整树。复用 seed binutils/make/musl 或裁剪需重新声明并审计闭包。

原 1 MiB 可写文件上限是独立障碍，GCC `Makefile.in` 为 2,145,896 字节，即使删测试/manual 也无法按原配置持久保存。增加 QEMU RAM 不改变限制。后续须支持真实大输入、生成文件、归档、编译器 ELF，并验证安全拒绝/rollback/旧盘兼容；不能从一个文件推断足够的新 file/disk 上限。

首次启动指标含刻意耗尽，不是 GCC 普通构建 demand；scheduler frame/heap 采样遗漏 syscall 瞬时，不是 RSS；宿主 wall time 含 firmware/boot/UART/shutdown；停机盘检查不是峰值。未运行完整编译器构建，因此其 memory/task/fd/temp/time demand 均未测。

## 证据与后续实施条件

全部成功才写 `build/logs/bootstrap-gcc.result.json`，嵌入 `build/bootstrap-gcc/prepared.json` 的源码 audit、commit/worktree、导入哈希、seed 清单、diagnostic/ISO/initramfs identities 和配置。每启动日志在 `build/logs/`，前缀 `bootstrap-gcc-probe`/`bootstrap-gcc-reboot`。实际验收/产物见[记录](../validation/M15.3.md)。

进入首个 GCC 阶段前需声明源码 closure/实测配置，提供有效 POSIX shell/build utilities、数学 headers/compatible 链接 libraries 或 guest 构建依赖，以及足够 file/disk/inode/VFS。static 编译器内嵌 GMP/MPFR/MPC 不提供可复用开发头文件。解除首批障碍后新 shell/syscall 缺口须诊断，不伪造 configure answers。

先顺序 single-stage 构建并保留旧工具；新编译器须编译/运行应用后才考虑 stage2/3，多阶段声明需真实阶段关系/比较。当前 single/multistage GCC、Rust/LLVM 源码重建仍未验证。guest 内核已由 M15.5 独立验收，完全在 guest 内生成镜像仍未验证。

## M15.5 之后的容量

评估保留最初 1 MiB file profile/source audit 以便同原验收比较。[M15.5](M15_KERNEL.md) 将当前持久 ext2 上限提至 4 MiB，tmpfs/readonly shared-file cache 仍 1 MiB。未裁剪编译器树仍超出 disk/inode/VFS，缺构建依赖仍独立存在；旧审计是历史容量证据，不声称当前持久上限仍 1 MiB，也不是对裁剪闭包的新评估。
