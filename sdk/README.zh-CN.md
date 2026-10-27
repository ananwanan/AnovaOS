<!-- Translate the released native SDK contract, offline templates and independent GUI companion boundaries. -->
# AnovaOS SDK 1.0.0

[English](README.md) | **简体中文**

发布的 SDK 面向小端 x86_64 `anova.native.v1`，wire version 保持 1，与 SDK 版本独立。
既有 syscall 编号、负数错误和公开结构布局冻结；新增封装可沿用 v1，wire 语义变化必须使用
新的 personality 并独立做兼容回归。未知负数错误保留数值，诊断名为 `unknown_native_error`。

原生启动最多 64 个 argv 项，每字符串最多 1023 字节，字符串和指针表合计必须放入装载器
受检的 32 KiB 启动块。runtime 参数迭代器按当前边界拒绝超量/未终止字符串，不能静默截断。

`cargo xtask sdk` 导出独立 SDK 至 `build/sdk/anova-sdk-1.0.0`，包含 CRT、链接脚本、C/C++
头文件、Rust 源码、三种模板和 SHA-256 清单。`cargo xtask app-new c build/my-app` 创建自带
SDK 快照的项目；换用 `cpp` 或 `rust` 选择其他模板。
`cargo xtask app-build build/my-app` 生成 `app.elf` 与受检的 `app.toml` 包清单。
`project.toml` 中的项目版本为正整数升级序号，与 SDK 发行版本独立。

全部模板使用原生 syscall 封装和普通 x86_64 用户 code model、red zone、SSE2；不依赖 libc、
C++ 标准库/异常 runtime、分配器、Rust `std` 或 Linux ABI。C++ 展示作用域资源所有权，Rust
展示类型化错误和实际本地依赖；生成一次本地锁文件后，以 `--offline --locked` 构建。
保留调试信息、ELF 符号、帧指针供 M13 诊断使用；宿主需 GCC/G++、Rust 1.90.0、rust-lld。

导出的 Python 驱动可独立运行：`python3 SDK/build.py c /absolute/project`，也支持
`cpp`/`rust`。驱动生成相同原生 ELF/ABI 尾部，`app-build` 额外验证 ELF 并创建包清单。
发行采用源码与校验和；校验清单不提供发布者认证。

Rust 模板包含完整 Cargo 清单和离线生成 lockfile。独立构建前先复制发行目录，避免生成文件
污染不可变 SDK：

```sh
cp -a build/sdk/anova-sdk-1.0.0 build/sdk/standalone-consumer
python3 build/sdk/standalone-consumer/build.py rust build/sdk/standalone-consumer/templates/rust
```

C/C++ 可在同一副本中使用 `c`/`templates/c` 或 `cpp`/`templates/cpp`。
后续 `cargo xtask sdk` 发布前暂存全新纯源码发行；旧导出出现未知文件（含 Cargo target）时
明确报错。先将这些文件移出导出目录再重试，工具不会把它们混入校验清单或静默删除。

guest Linux-musl 开发仍是独立验证的 M11/M12 配置；原生 SDK 交叉构建不证明原生 `std`、
原生 guest 编译器或任意 C++ 库支持。应用权限/能力声明为安装策略元数据，不是安全沙箱。

M14.4 配套 GUI 源码 SDK 单独提供版本化 `anova.gui.v1` C ABI、C 头文件、C++ 所有权封装和
Rust 安全封装，共享同一语义。协议协商、句柄生命周期/错误映射、跨语言一致性及资源失败
单独验收。本原生 SDK 1.0.0 不含 GUI runtime；使用 `cargo xtask gui-sdk` 导出独立配套发行。
见 [GUI SDK](gui/README.zh-CN.md)与 [runtime 范围](../docs/zh-CN/milestones/M14_RUNTIME.md)。

源码导出和校验清单同时包含 `README.md`、`README.zh-CN.md`，导出的 SDK 也可切换语言。

## 实验性桌面接口

`anova-rt::process`（`0x101`）和 `anova-rt::pty`（`0x102`）是单独版本化的桌面实验接口，不改变冻结的 SDK/native-v1 调用记录。PTY 句柄要求准确创建者、有界非阻塞传输，并由内核清理准入的 Linux 进程族。GUI C/C++/Rust ABI 仍为 1.0；可复用的底层 Rust Canvas 不授予传输权限。详见 [PTY 契约](../docs/zh-CN/milestones/M17_PTY.md)。
