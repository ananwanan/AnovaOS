<!-- Explain toolchain roles, verified guest C/C++/Rust development and remaining online/bootstrap stages. -->
# 从交叉编译到系统内开发

## 1. 先分清三个机器角色

GCC 等构建系统中的 `build` 是执行构建工作的系统，`host` 是编译器本体将运行的系统，`target` 是编译器产生代码的系统。

| 阶段 | build | host | target | 证明了什么 |
| --- | --- | --- | --- | --- |
| 内核/应用交叉构建 | Linux 开发机 | Linux 开发机 | AnovaOS 内核/用户 ABI | 宿主能为新 OS 生成代码 |
| 构建 guest-hosted 编译器 | Linux 开发机 | AnovaOS 上受测的用户 ABI | AnovaOS 上受测的用户 ABI | 可以准备系统内编译器，仍需 guest 实测 |
| 系统内应用构建 | AnovaOS | AnovaOS | AnovaOS | 已具备本机应用开发能力 |
| 编译器自举 | AnovaOS | AnovaOS | AnovaOS | 系统能重建声明范围内的编译器 |

这里的 AnovaOS 用户 ABI 初期可以是 Linux x86_64 musl 兼容 ABI。编译器本体可由宿主交叉生成，然后在 guest 运行；不要求先能在 guest 从零构建完整 GCC/LLVM。

## 2. 两条用户态路线

### 路线 A：通过 Linux 兼容层复用工具链（优先）

- 内核仍为自研 Rust 内核。
- C/C++ 软件为 Linux x86_64 musl 受控 sysroot 构建；Rust 应用可从 `x86_64-unknown-linux-musl` 入手。
- 所产生程序仍调用 Linux ABI，AnovaOS 必须实现所用语义。
- 先运行静态或依赖受控的编译器包，随后提供编辑器、shell、libc、链接器和开发文件。
- 自定义 OS 不会自动满足官方 Linux target 的运行环境假设，应按功能逐项验证。

这条路线缩短首次开发闭环所需的专用平台移植工作，但兼容内核接口本身仍是大量工作。

### 路线 B：AnovaOS 专用原生 target（独立推进）

- 定义 native syscall、CRT、用户栈、动态加载与 libc 约定。
- C/C++ 为编译器增加配置、启动文件、链接规则与运行库支持；newlib 可评估，不能默认已有完整 POSIX。
- Rust 从自有 target 的 `core/alloc` 起步，再移植 `std` 平台层、libc 绑定、线程/文件/时间/进程接口。
- target 名称如 `x86_64-unknown-anovaos` 仅是候选本地标识，当前不是已存在的 Rust/GCC 上游 target。
- 自定义 target JSON 解决代码生成配置的一部分，不会自动提供平台运行时和标准库。
- 长期再争取上游 target/host 支持；维护本地补丁期间必须锁定版本并记录差异。

路线 A 的应用开发完成与路线 B 的原生 `std` 完成分别报告。

## 3. Rust 内核与第一批应用

### 3.1 内核

`x86_64-unknown-none` 为裸机 ELF target，不提供 `std`；先 `core`，完成 allocator 后使用 `alloc`。内核链接脚本和 no-red-zone 等设置不能污染用户程序和宿主 xtask。

工具链优先 stable；使用确需 nightly 的功能时固定实际验证的日期。`-Z build-std` 是独立的实验性构建能力，不是“自动为新 OS 移植 std”的开关。

### 3.2 native 最小应用

M4 提供 `_start`/runtime、用户链接脚本和 write/exit 包装，使 `no_std` 程序运行在 Ring 3。程序结束要通过 syscall 退出；不能掉回空地址或直接调用内核函数。

native 用户 target 的 code model、red zone、FPU/SSE 与链接布局需要独立定义；不直接套用内核裸机默认配置。

### 3.3 第一个带 `std` 的 Rust 应用

采用已锁定的 Linux-musl 用户工具链，先在宿主交叉编译小程序，再在 guest 验证分配、参数、文件与错误语义。静态/PIE 行为通过检查 ELF 产物确认，不能只看 target 字符串推断。

## 4. GCC/G++ 的完整链路

### 4.1 C 的前置设施

内核需具备进程执行/等待、地址空间、文件/目录、临时文件、管道、时间、信号等已测语义；用户态需具备 shell、CRT、头文件、libc、assembler/linker、编译器支持库和一致的 sysroot。

`x86_64-elf-gcc` 可以生成裸机代码，但不自带一个完整用户态 OS 的头文件、libc、启动文件和运行时，不能作为“C 应用开发已经完成”的证据。

### 4.2 编译器本体也有依赖

准备 `gcc` 驱动、`cc1`、汇编器、链接器、`collect2` 等实际用到的组件及其搜索路径；分析 GMP/MPFR/MPC 等构建依赖与运行时依赖。驱动能显示版本号不代表能启动子工具完成构建。

在宿主完成相应 cross/hosted 构建，必要时使用受控 sysroot/补丁；所有 configure 参数经验证后写入配方。不能机械更改 `--host` 字符串就认定 port 完成。

静态工具链包可以减少最初共享库依赖，但不消除 fork/exec、文件映射、信号、线程或 libc 需求。TCC 可以用作更早的编译实验，GCC/G++ 仍独立验收。

### 4.3 C++ 增量

补齐 `cc1plus`、libstdc++、libgcc/libgcc_s 或声明的替代、C++ ABI 支持、构造/析构和线程相关运行库。异常和栈展开可以先限制在明确禁用的构建配置，随后单独启用与测试；不能在禁用异常后声称完整 C++ 运行时支持。

### 4.4 M11 受测用例

M11 now implements the controlled offline guest profile in [M11_TOOLCHAIN](M11_TOOLCHAIN.md). The following commands run inside its development image:

```sh
gcc main.c -o hello-c
./hello-c
g++ main.cpp -o hello-cpp
./hello-cpp
make
```

- C：stdio、参数、文件读写、malloc、多个源文件、编译错误的非零退出。
- C++: string/vector, RAII, exception unwinding and a real worker thread are tested; broader standard-library use remains per-program.
- 构建链：子工具定位、头文件搜索、临时文件清理、增量构建和链接失败诊断。
- 所有源码在 guest 编辑保存，二进制在 guest 生成，重启后仍可执行。

## 5. rustc、Cargo 与 LLVM

### 5.1 运行 rustc 的设施

运行 LLVM/rustc 需要比 hello app 更多的内存、线程、TLS、同步、文件映射、信号和运行库支持。先限制目标后端和并行数，记录小项目的峰值资源，之后才评估完整 Rust 编译器自举。

编译器工具包应包括 rustc、匹配的标准库、目标支持文件、LLVM 依赖、C linker 与 sysroot。Rust 程序通常仍通过链接器完成最终 ELF 生成；不能只放一个 rustc 二进制。

### 5.2 Cargo 分项推进

| 层次 | 受测能力 | 额外条件 |
| --- | --- | --- |
| R1 | guest 内 rustc 编译单文件 `std` app | 工具本体、stdlib、链接器与基础 OS 语义 |
| R2 | Cargo 构建无外部依赖项目 | rustc 子进程、文件、环境、元数据与锁 |
| R3 | Cargo 离线构建依赖 | 预置 registry/vendor、Cargo.lock、cache |
| R4 | build.rs、C FFI | host/target 区分、build 脚本执行、C 工具链 |
| R5 | proc-macro | 编译器进程加载 host 动态库、匹配 ABI 与库搜索 |
| R6 | 在线 crates.io/Git 获取 | 网络、DNS、TLS、CA、时间及相关传输依赖 |

M12 offline acceptance covers R1–R3, build.rs/C FFI (R4) and a standalone
matching-ABI procedural macro (R5). R6 online registry/Git remains unverified.
See [M12_RUST](M12_RUST.md) and [actual evidence](validation/M12.md).
Rust 1.90.0/LLVM/Cargo/std are locked official musl binary distributions; this
is not a local Rust source build or compiler self-bootstrap.

build.rs 和 proc-macro 在“编译器 host”执行。交叉编译时与普通 target app 可能不同；系统内构建时也必须正确区分配置，不能把它们按内核 target 编译。

### 5.3 Implemented offline workflow

Use the optional `cargo xtask run --rust` image. These commands run in guest:

```sh
rustc main.rs -o hello-rust
./hello-rust
cargo build --offline
./target/debug/m12-app
```

The acceptance gate checks String/Vec, argv, files, threads, worker-thread child
spawn/wait, build scripts, a shared procedural macro, C FFI, a locked vendor
crate, no-op/source-change builds, a real syntax error and clean reboot.
Online downloads, general Cargo projects, a native std port and bootstrap remain
separate. Run `cargo xtask rust-build`, `rust-cross-test`, `rust-runtime-test`
and `rust-test`; complete commands and limits are in M12_RUST.

## 6. 系统根目录和包内容

规划 `/bin`、`/usr/bin`、`/usr/lib`、`/usr/include`、`/tmp`、`/dev` 与必要的 `/proc`，实际遵循选定工具链的搜索规则。动态解释器位置以 ELF PT_INTERP 为准，不能随意重命名。

每个工具链包记录编译器版本、host/target ABI、sysroot、libc、CRT、共享库、包校验和、补丁和许可证材料。应用构建产物记录目标 ABI，避免与内核 ELF 混装。

## 7. 开发完成与自举分界

能够在 AnovaOS 中使用预先交叉构建好的编译器，产出并运行自己的 app，已经达到用户的系统内开发目标。

能够在 AnovaOS 中重建 GCC、Rust、LLVM 或自身内核是更高等级的自举能力，单列 M15；需要额外资源和构建生态，不能作为应用开发的隐性前置条件。
