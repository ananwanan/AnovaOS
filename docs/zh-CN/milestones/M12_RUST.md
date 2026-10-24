<!-- Translate the M12 RUST contract and preserve exact commands and historical evidence. -->
# M12：在 AnovaOS 内开发 Rust

[English](../../en/milestones/M12_RUST.md) | **简体中文**


本范围将真实 Rust1.90.0 musl 工具、匹配 std、LLVM 和 rust-lld 装到 `/rs`，与 M11 源码构建的 C/C++ `/tc` 并存。应用编译在客户系统进程执行，宿主交叉编译只是独立前置测试，不提供持久验收 ELF。实际结果见 [M12 验证](../validation/M12.md)。

## 构建、运行与来源

宿主沿 [M11](M11_TOOLCHAIN.md) 依赖并增加 x86_64-unknown-linux-musl 目标，依次准备工具、Rust 镜像和 Linux 开发盘。Rust ISO 为 build/images/anova-rust.iso，单 CPU、3 GiB RAM、TCG。普通镜像和内核目标独立；不选磁盘则临时 Linux 盘，持久保存需显式盘与客户系统 shutdown，编辑器沿 M7。

启动器配置 `/rs`、真实 LLVM 链接器、单代码生成/链接/构建任务、受限临时路径和离线 Cargo；不运行 rustup、不调用宿主编译服务，也不把内核链接脚本用于普通应用。默认输出静态 musl。开发盘为 16 MiB、单块组、128 字节 inode，Linux 为 512 inode，原生仍 128，旧盘不自动重格式化。M15.5 将持久文件从 1 MiB 扩至 4 MiB，tmpfs 和共享缓存仍为 1 MiB，活跃共享映射阻止超限增长。

[inputs.json](../../../ports/rust/inputs.json) 锁定 2025-09-18 官方 Rust1.90.0 musl rustc/Cargo/std 及 itoa1.0.15。这是上游二进制分发，不声称本项目源码重建 Rust/LLVM；M11 引导库提供动态展开库。先检查整归档 SHA-256 再解压，验证路径、选择组件并保留许可。strip 后安装摘要与原归档区分。准备过程串行化，完整候选才发布，镜像重验全部普通文件库存，最终归档另有 SHA-256。

真实 itoa vendor 归档与每文件校验、Cargo.lock、源替换独立检查，启动前依赖完整，无需联网，见 [第三方](../third-party/m12.md)。

## 启动、ELF 与映射容量

Linux argv/env 各最多 256 条，每向量载荷 8 KiB，每串 1023 字节，启动区域 32 KiB。原生仍最多 64 参数。VFS/Linux 路径 512 字节、分量 255 字节；原生路径 128 字节，80 字节目录记录不改，名称超过 63 字节返回 TOO_LONG 且不推进游标，Linux getdents 使用独立记录。

Linux ELF 文件和装载总量最多 256 MiB，initramfs 条目 256 MiB、模块 1 GiB，全部必须在受检保留启动 RAM 内。Linux mmap 窗口为 `[0x50000000, 0xc0000000)`，每次 256 MiB；brk、原生 ELF、16 MiB 内核堆和保护栈独立。每根页表保存查找提示，尾部无空洞则环绕全窗口；占用或 PROT_NONE 页不复用，成功后才提交提示，OOM 回滚。

## 线程、信号和文件语义

MAP_STACK 只是 Linux 提示；保护映射、sigaltstack、SA_ONSTACK、信号帧与返回使用真实线程状态，32 位 flags 不包含未定义填充。普通 fork 继承备用栈，共享地址空间的新线程初始禁用，exec 清除，autodisarm 拒绝。

musl posix_spawn 的精确 CLONE_VM|CLONE_VFORK|SIGCHLD 变体使用独立子栈并暂停父，保留 vfork 所有权、失败 exec 唤醒、独立描述符及信号规则。Linux wait 按线程组子进程寻址，工作线程启动并等待链接器时仅阻塞自身。线程可在 SYS_exit 前解除自身栈，返回只验规范 RSP，真正访问仍受页权限；原生返回继续要求映射栈。

pread/pwrite 即使失败也保共享偏移，FIONREAD/FIONBIO 反映真实管道字节和共享标志。ftruncate 保偏移、缩小保前缀、增长清零，ext2 预检空间并保留失败设备状态。rust-lld 使用 --no-mmap-output-file；无可写共享映射或回写。后续只读共享缓存最多 1 MiB，大持久文件可普通 I/O 或私有映射。

AF_UNIX SOCK_SEQPACKET 对占 16 管道池中的两个队列，包最多 1022 字节，保边界、原子发送、截断接收、双向流量、反压、EOF、NONBLOCK/CLOEXEC 和最后打开对象生命周期。无文件系统 Unix socket、附加控制数据、其他类型或向量包 I/O。

空目录删除检查孩子，已删除但打开的 inode 最后关闭才释放；挂载点、活跃 cwd、已删除且打开子目录的父返回 EBUSY。受检 dirfd 相对 open/stat/unlink 和 O_NOFOLLOW 支持 rustc 临时归档清理；忽略 mode_t 类型位，但拒绝不支持的特殊权限位。flock 属于打开描述对象，dup/fork 共享、最后关闭释放，可阻塞并被信号打断。普通文件重命名保留已打开旧目标；目录仅支持同父新名称，保留父链接和计数，拒绝跨父或目录替换，没有新增日志事务。

## 三个独立门禁

rust-cross-test 验证真实 std 分配、参数、文件、线程和子输出、自解除栈退出、备用信号栈、长名称、包对、目录清理、定位 I/O 和锁。rust-runtime-test 启动匹配 rustc/Cargo 后正常关机。

rust-test 经 UART 输入源码，执行客户系统 rustc、独立过程宏共享库、无依赖 Cargo、固定 vendor 加路径过程宏。build.rs 生成 OUT_DIR、调用客户系统 GCC/ar 并链接真实 C。第二构建须无改动复用，改源码须重建；过程宏普通依赖及构建依赖共用支持库，修改后结果 42→84，必须重建支持库、宏和应用，保留 vendor。非法宏输入返回 Cargo101 且旧程序仍能运行，修复后重建成功；独立语法错误不得替换先前 ELF。

宿主检查 23 文件的源码、ELF、模式和 SHA-256，独立重启执行保存应用，e2fsck 与摘要不变，横幅或宿主预制应用不能通过。过程宏使用该编译器的宿主 ABI，单 proc-macro 类型配置动态 musl CRT/PIC，lib/rlib 也 PIC，应用默认静态。固定图不代表任意 Cargo 工作区或宏依赖图；联网另见 [M12 网络](M12_NETWORK.md)，原生 std 与编译器自举独立。POSIX fcntl 记录锁未实现，非致命缓存警告保留，不能把独立 flock 当作记录锁成功。

## 原始命令与输出

以下命令、源码与日志保留原文；执行位置及通过条件见上文。

```sh
rustup target add x86_64-unknown-linux-musl
cargo xtask toolchain-build
cargo xtask rust-build
cargo xtask rust-image
cargo xtask root-image --linux build/disks/rust-devel.ext2
cargo xtask run --rust --disk build/disks/rust-devel.ext2
```

```sh
cd /persist
edit main.rs
rustc --edition=2024 main.rs -o hello-rust
./hello-rust
cargo build --offline --frozen
shutdown
```

```sh
cargo xtask rust-cross-test
cargo xtask rust-runtime-test
cargo xtask rust-test
```
