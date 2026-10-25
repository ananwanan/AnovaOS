<!-- Translate the M11 TOOLCHAIN contract and preserve exact commands and historical evidence. -->
# M11：在 AnovaOS 内开发 C/C++ 应用

[English](../../en/milestones/M11_TOOLCHAIN.md) | **简体中文**


可选开发镜像运行真实 GCC/G++ 11.2.0、cc1/cc1plus、collect2、binutils 2.37 和 GNU make 4.4.1，目标系统根目录含源码构建的 musl 1.2.4、CRT、libgcc、libstdc++ 11.2.0。用户在客户系统编辑源码，编译、汇编和链接都由客户系统进程执行，宿主不提供验收应用二进制。Rust/Cargo 和源码重建分别由 M12、M15 验收。

## 构建与运行

Linux/WSL 宿主需 DEVELOPMENT 中的依赖，另加 Python3.12+、make、GCC/G++、m4、bison、flex、texinfo。编译器构建需数 GiB 宿主磁盘和内存，ANOVA_BUILD_JOBS 允许 1–8 并行任务，默认 6；生成文件仅放 build/toolchain。命令和客户系统操作保留下方。

编辑器支持 make 所需制表符，使用 p、a（单独点结束）、c N、d N、w、q，离开前先 w 保存。普通 run 仍使用基础镜像；run --toolchain 不选磁盘则创建临时 Linux 盘，持久保存需显式磁盘和正常 shutdown。开发 ISO 为 build/images/anova-devel.iso，单 CPU、1 GiB RAM。

ports/gcc/inputs.json 在解压前锁定全部归档，包含宿主引导用 musl.cc GCC11.2.1、GMP6.2.1、MPFR4.1.0、MPC1.2.1。build.py 从上游源码构建客户系统工具及目标库，记录安装文件库存；打包重验库存、可执行 ELF 和每文件/归档 SHA-256，建立独立只读 `/tc`。不将宿主编译器装入客户系统 `/bin`，也不改变基础镜像，见 [第三方边界](../third-party/m11.md)。

小型 gcc/g++/make 启动器配置 PATH、TMPDIR、locale、静态非 PIE 应用链接及 GCC GC 参数（最小堆 32 MiB、扩展 100%）。make 使用真实 `/linux/sh` BusyBox ash，源码构建选 fork 而不是 posix_spawn；工具自身可以使用已验收静态 PIE。

## 内核和文件系统增量

M11 历史 Linux ELF 文件最多 64 MiB、装载 128 MiB，保留模块 128 MiB，均需验证处于保留启动内存。Linux argv/env 各 64 串，每串 1023 字节，每向量载荷 8 KiB，启动区域 16 KiB，路径 128 字节；Linux 用户栈 1 MiB，原生用户栈 64 KiB，调度器保护栈 256 KiB。后续 [M12](M12_RUST.md) 替代这些 Linux 容量。

内核堆 16 MiB，位于独立对齐区，不与栈或故障测试映射重叠。tmpfs 文件最多 1 MiB、VFS 2048 节点，描述符/打开对象/任务仍为 32/128/32。历史 brk 窗口 128 MiB，mmap 窗口 512 MiB、每次 128 MiB，全部立即分配，保持 W^X、PROT_NONE 和 OOM 回滚。自有低半根允许有界页表归属查询，开发构建使用 opt-level 1 和调试信息。

只读 initramfs 映射借用不可变字节；tmpfs/ext2 的私有映射复制文件快照，不改变共享偏移，之后文件与映射修改独立。M11 无共享持久文件回写；M12 后续只增加受限只读一致性共享缓存。

vfork 共享活跃用户根，父进程暂停直到子成功 exec 或退出/故障，父待处理信号延迟到子释放根后。普通 fork 仍复制私有页；M11 不包含后续 clone/posix_spawn 变体。

单 root 的 umask、普通文件 chmod/fchmod、权限位、readlink 和受支持 getrlimit/prlimit 服务实际编译器调用，不支持的资源修改返回错误。修改时间来自受检 UTC 和节拍，ext2 持久化整秒供 make 使用，没有多用户权限模型。开发盘 `.anova-abi` 明确为 linux.x86_64.v1，导入及新建后代继承，原生盘仍是 anova.native.v1，不根据 ELF 猜测。

## 验收和限制

toolchain-test 通过带看门狗的真实 UART 编辑 C、C++、Makefile 和错误样例，验证标准 I/O、分配、参数、文件、多源码 C、C++ 容器、异常传播、RAII 和真正 std::thread。make 须无改动不重建、改一个源码只选择性重建；语法和链接失败须非零且清临时文件。还测 vfork、权限和快照负例。

干净关机后宿主独立检查精确源码、ELF 摘要与模式，第二次启动运行保存应用，摘要保持，e2fsck 干净，见 [M11 验证](../validation/M11.md)。这是小型离线静态应用范围，不代表 LTO、OpenMP、sanitizer、任意 configure 项目、编译器自举、SMP、公开工具链发布或通用 Linux/glibc。GCC 可选 sysinfo/getrusage/madvise 返回 ENOSYS，测试使用明确 GC 参数，不伪造成功。

M15.5 将同一 ext2 格式的持久普通文件从 1 MiB 扩为 4 MiB，tmpfs 与只读共享缓存仍为 1 MiB，活跃共享映射超限增长返回 EBUSY；不增加日志或断电保证。历史 M11 验证保留当时上限，当前重建见 [M15 内核](M15_KERNEL.md)。

## 原始命令与输出

以下命令、源码与日志保留原文；执行位置及通过条件见上文。

```sh
cargo xtask toolchain-build
cargo xtask toolchain-image
cargo xtask root-image --linux build/disks/devel.ext2
cargo xtask run --toolchain --disk build/disks/devel.ext2
```

```sh
cd /persist
edit main.c
gcc main.c -o hello-c
./hello-c
edit main.cpp
g++ -std=c++17 -pthread main.cpp -o hello-cpp
./hello-cpp
make
shutdown
```

```sh
cargo xtask toolchain-test
```
