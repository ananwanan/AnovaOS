<!-- Translate locked reconstruction recipes, compiler feasibility preparation and host/guest responsibilities. -->
# 有界 bzip2 重建

[English](README.md) | **简体中文**

配方从官方 bzip2 1.0.8 选择未修改的 C 源码、头文件、声明、上游 Makefile/手册及三对
样本/参考数据。`inputs.json` 固定 SHA-256 与 [Sourceware](https://sourceware.org/pub/bzip2/sha512.sum)
公布的 SHA-512。上游 `LICENSE` 保持原 bzip2 许可，不修改源码；自有 `Makefile.anova`
采用 Apache-2.0。

先运行 `python3 ports/bootstrap/prepare.py`，再加 `--verify` 检查完整源码清单。
准备步骤下载至 `build/bootstrap`，先验证两项校验和再读取成员，仅接受精确 archive 根下的
普通文件，从不运行编译器。文件哈希、机器职责、种子工具记录在 `inputs.json`、
`source/INPUTS.json`、`source/SOURCE.sha256`。已变更缓存会失败，不会静默替换。
排除可选大型 PostScript 手册及无关工具，保留 HTML 手册、许可及所有编译单元。

在独立准备的 Linux personality 工作盘中，于 guest 执行：

```sh
cd /persist/bootstrap
make -f Makefile.anova -j1
./bzip2 -1 -c sample1.ref > sample1.guest.bz2
./bzip2 -dc sample1.guest.bz2 > sample1.guest.ref
./bzip2 -t sample1.guest.bz2
./bzip2 -t invalid.bz2
make -f Makefile.anova failure
```

最后两条必须失败。`failure` 对 `rejected.c` 调用真实 guest 编译器，故意的 `#error` 必须保留
先前压缩器。正常目标直接链接八个 guest object，不重建 GCC/libc 或单独 libbz2 archive。
GCC launcher 使用 `/tc` sysroot 和静态非 PIE；`clean` 只删除声明的 object/压缩器产物。
验收在正常关机、重启后独立比较参考字节、压缩字节及保留工具/源码哈希。
本配方不证明任意上游 configure/Makefile 兼容或编译器自举。

## 编译器源码评估

`gcc_probe.py` 审计七份 M11 编译器依赖源码包，发布 [GCC gate](../../docs/zh-CN/milestones/M15_GCC.md)
需要的有界未修改输入。`rust_probe.py` 独立审计锁定官方 Rust 1.90.0 源码发行包的 LLVM fork、
Cargo、vendor；选定内容支持 [Rust/LLVM/Cargo gate](../../docs/zh-CN/milestones/M15_RUST.md)。
二者支持 `--verify` 只读核查生成源码缓存，拒绝变化或重定向输出，保留上游声明；
纯源码准备不运行上游构建脚本，不重建编译器组件。`test_gcc_probe.py`、`test_rust_probe.py`
检查完整性、archive 安全与发布拒绝。

自有 `rust-probe/` 测试继承的 guest Rust/C++ 种子、实际缺工具 exec 错误和隔离离线依赖失败。
成功应用保留并在重启后执行，与源码重建 LLVM/rustc/Cargo 分开。
各 gate 的生成结果记录精确产物身份和宿主参与。

## 系统内内核重建

`kernel_prepare.py` 保存自有内核/本地 crate 源码快照、隔离 Cargo 元数据、验证锁定离线
registry 图并准备继承的官方裸机目标库。`--verify` 为只读，拒绝陈旧/被修改发布；
`test_kernel_prepare.py` 检查 archive 安全、锁漂移、源码完整性和发布边界。

`kernel-build.c` 是宿主编译的配方/诊断程序，实际调用 guest Cargo/rustc/rust-lld，只发布
受检内核 ELF；捕获真实语法失败并保留旧内核。`cargo xtask bootstrap-kernel-test` 将保留
ELF 打包，宿主不重编内核；比较提取 ISO 字节，再验证真实启动身份和恢复。
种子/源码职责及限制见 [M15 内核](../../docs/zh-CN/milestones/M15_KERNEL.md)。

`cargo xtask foundation-capacity-test` 在新建 JBD2 ext4 工作盘上复用内核配方，
分开测量主动耗尽与干净构建，并保留精确内核的启动/失败恢复证据。预算和实际结果见
[M16 容量](../../docs/zh-CN/validation/M16_CAPACITY.md)。
