<!-- Translate the independent M18 compiler, image-generation and repeat/recovery profiles. -->
# M18：编译器与启动镜像重建

[English](../../en/milestones/M18_BOOTSTRAP.md) | **简体中文**

从 `dev` 建立 `feat/m18-bootstrap` 开发分支，Pull Request 目标为 `dev`。M16 远端收尾
仍暂缓，GitHub Actions 仍仅由 `main` 推送触发。[路线图](../ROADMAP.md#m18有条件的自举)
分别跟踪各路线的退出条件。

## GCC 第一阶段

GCC 11.2.0 阶段在 AnovaOS x86_64 下使用显式 Linux-musl personality。上游 build、host、
target 参数均为 `x86_64-linux-musl`。C/C++ 阶段使用配套的 `/tc` musl 和 C++ sysroot，关闭
bootstrap 和 multilib，以单个 make 作业构建。新驱动和前端保存在独立工作区，实际编译、
链接并运行两种语言的程序后，才评估 stage2/stage3。
[原 M15 GCC 配置](M15_GCC.md)继续保留原工具及前置条件缺失的检查。

`cargo xtask m18-gcc-test` 执行第一阶段。`--entry` 诊断保留原有 12 文件配置子集和缺源边界。
两种配置均由宿主用原固定源码提交构建静态
BusyBox 1.36.1，准入 `/bootstrap`，保留单独的解析后配置、二进制摘要、许可证及对应源码
归档。夹具检查 `sed`、`awk`、`grep`、`expr`、`tr`、`sort` 和 `cmp` 的实际输出和状态，
以及 `mv` 替换后旧打开描述符仍能读取。ash 的内建 `printf` 处理 13,312 字节夹具操作数，
`install` 保留执行权限和精确字节，BusyBox `kill -0` 用于 COM1 响应检查中的活跃 configure
进程观测。Guest 经 ash 内联管道路径比较 heredoc 精确的 2,049 字节。夹具核验归档内的链接能解析，并通过 `ln` 记录源文件
缺失及运行时硬链接/符号链接创建不受支持。上游 configure 提供复制回退时，配置可采用此回退。

GMP 6.2.1、MPFR 4.1.0 和 MPC 1.2.1 使用[原源码锁](../../../ports/gcc/inputs.json)。
宿主准备 Linux-musl 静态库、头文件和许可证说明；guest 内实际编译并链接测试应用，以验证
输入可被使用。若在 AnovaOS 内重建依赖，须另行提供证据。GCC/G++、binutils、make 和
sysroot 继续继承 M11 种子。每轮保留版本、配方和清单。

源码准备先校验上游归档摘要，再保留逐文件清单。Stage1 将选定的 C/C++ 源码、x86 生成器和
宿主支持库放到新建 ext4 盘上。保留原始 `configure` 脚本、`Makefile.in`、发行输入时间及
执行权限。Guest 用选定的 ash、种子 GCC/G++ 和准备好的算术输入运行原始顶层 `configure`。
配方用 `/dev/null` 禁用 site 和探测缓存，设置 `--prefix=/persist/m18/gcc-stage1`、
`--with-sysroot=/tc` 和 `--disable-nls --disable-shared --disable-lto --without-isl`，并沿
原有 `/tc/bin/make -j1 SHELL=/bootstrap/sh all-gcc` 目标图执行。日志、退出状态和停机工作盘
用来核对实际到达的目标。make 成功后，门禁要求 `xgcc`、`xg++` 选中 guest 构建的 `cc1`、
`cc1plus`，用种子 binutils 和 sysroot 汇编链接，运行两种语言程序，拒绝非法 C 源码及未定义
C++ 符号，并在重新启动后核对保留产物。夹具将不可变的 ELF `as`、`real-ld` 别名置于驱动搜索路径
首位并检查实际选中的工具。两个别名均指向现有 `/tc/bin` 工具。实测见 [M18.1](../validation/M18.1.md)。

本地首阶段验收在保留的 ext4 工作盘上接续原始 guest 构建目标图，再通过独立 KVM 应用与重启验证。
独立干净 TCG 门禁有单独的实测 watchdog 记录。Stage2 和 stage3 各自需要编译器阶段证据。

声明的 TCG 配置均为单 BSP、1 GiB guest RAM、无网卡、采样物理帧增量上限 196,608 页、COM1
探测上限 3 秒。Stage1 使用新建 512 MiB JBD2 ext4 工作盘，其中有 131,072 个 4 KiB 块和
16,384 个 inode，构建时限为 14,400 秒。入口诊断使用新建 64 MiB 工作盘，其中有 16,384 块
和 4,096 个 inode，时限为 900 秒。首次 [300 秒失败](../validation/M18.1.md) 单独记录。
现有约束包括 16 MiB 内核堆、32 个任务槽、每进程 32 个用户 FD、
2,048 个 VFS 节点、受控 ext4 文件 256 MiB、ext2 文件 4 MiB、tmpfs 文件及只读共享文件缓存
1 MiB。门禁保留正常关机/重启，并独立检查工作盘内容。工作盘峰值覆盖成功的分配更新。调度边界
物理帧采样反映观测压力，无法确定精确 RSS 峰值。

## 独立的 Rust 组件

锁定的 Rust 1.90.0 源码中，`src/stage0` 要求 Rust 1.89.0。M12 的 Rust 1.90.0 musl
二进制工具可编译应用，仍为继承种子。保留[M15 Rust 可行性评估](M15_RUST.md)，分别记录
LLVM、rustc、Cargo、标准库和链接器身份。完整 LLVM 构建需要可用的 CMake、C++ 和实际构建，
以及选定源码和实测预算。rustc 阶段需要发行指定的 stage0、匹配的 host 库和受检离线源码/
依赖闭包。Cargo 也需要自己的锁定离线闭包及构建/测试证据。单个组件通过无法证明其他组件或
多阶段 bootstrap 已完成。

`cargo xtask m18-llvm-test` 仅从锁定 Rust 发行版的 LLVM fork 选取原始、独立的
`LLVMDemangle` 库。继承的 guest G++ 和 binutils 编译六个翻译单元并归档，再编译、链接
及运行独立测试程序。门禁将编译器实际记录的非系统依赖与源码选集对比，并逐个比较归档成员
与对象文件。使用独立 64 MiB JBD2 ext4 工作盘、单 BSP、3 GiB RAM、每会话 900 秒及
5 秒内活跃负载 COM1 检查。实测结果和剩余 Rust 组件见 [M18.2 证据](../validation/M18.2.md)。
两轮构建的归档和夹具精确匹配，真实语法/链接失败及全新启动重链接后保留旧字节。宿主实测的
最大汇编中间产物超过 1 MiB tmpfs 文件上限，编译采用 `-pipe`。

## 在 guest 内构建启动镜像

M18.3 首批配置使用独立检查过的保留 guest 基础内核，以及现有 UEFI/Limine 光盘引导路线。
guest 镜像构建器读取受检普通文件，包括精确的内核 ELF、initramfs、原生应用模块、Limine
配置、EFI 可执行文件及光盘启动映像。构建器写出规范固定的 ECMA-119 ISO，含 Rock Ridge
名称和 El Torito UEFI 引导目录。宿主仍明确承担输入及运行用 ISO 准备、导出 guest
产物、独立镜像检查及运行 QEMU。最终 ISO 必须由 guest 构建器生成。独立提取内核，对比
逐字节内容与 SHA-256，使用 QEMU 实际启动产物，检查嵌入身份及相关回归标记。保留内核的
[M15](M15_KERNEL.md)/[M16](../validation/M16_CAPACITY.md) 检查继续单独运行。带 GUI 的
内核构建另设配置。

`cargo xtask m18-image-test REPORT GUEST_ELF` 接受保留的无 GUI 内核配置（`release`、
`opt-level=z`、`ext4-write,bootstrap-metrics`），校验 M16 容量报告，或匹配的保留重放回执、
UART 和停机工作盘。宿主准备锁定 Limine 文件并编译自有构建器。guest 用受检输入运行构建器并
发布最终镜像。构建器每次流式处理 4 KiB，单个输入上限 16 MiB，输出上限 32 MiB。候选同步后
重命名，支持显式清理旧候选。门禁声明 64 MiB JBD2 ext4 工作盘、单 BSP、256 MiB RAM、
构建会话 600 秒/启动态会话 90 秒、COM1 取消响应上限 5 秒。见 [M18.3 证据](../validation/M18.3.md)。

## 重复构建和恢复

每轮重复使用独立干净盘，按路径和 SHA-256 比较精确输入输出；保留实际状态、内核/串口日志
及观察到的差异。记录失败后可见的状态以及全新启动能恢复的内容。保留旧工具和内核路径及
独立 ext2/COM1 恢复路线。重复预检或 ISO 构建的证据只覆盖实际执行的组件。编译失败、取消、
资源耗尽及发布失败要分别提供对应组件的证明后，才能宣布 M18.4 完成。
