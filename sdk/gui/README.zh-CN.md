<!-- Translate GUI v1 ABI layout, lifetime, linkage and the bounded three-language development profile. -->
# AnovaOS GUI SDK 1.0.0

[English](README.md) | **简体中文**

源码 SDK 面向小端 x86_64、System V 函数调用约定的 `anova.gui.v1`。
GUI ABI 为 `0x00010000`，与 native syscall v1、SDK 发行版本独立。
Rust `no_std` runtime 实现全部 C 导出；C 使用 `include/anova_gui.h`，C++ 使用无异常、
仅移动的 `include/anova_gui.hpp`，Rust 使用安全 `anova_gui_runtime` crate。
三语言示例绘制相同图元、文字和控件；能力位为矩形/文字 `CAP_RASTER`（1）、控件/布局
`CAP_WIDGETS`（2）、有界进程内剪贴板 `CAP_CLIPBOARD`（4）。

`python3 sdk/gui/generate.py` 生成 C 头文件，`--check` 拒绝过期输出。
生成器固定 schema 定义声明，Rust 布局测试和独立 guest C/C++/Rust 契约检查大小、对齐、
偏移、版本协商及按值矩形参数。C/C++ 保留负数状态值，Rust 将确切 v1 的已知状态映射为
`Error` 枚举。

调用限单 UI 线程，无回调、重入或展开。客户端为指定长度提供有效、对齐、存活的指针；
错误指针的故障限于该进程，runtime 参数检查不会自动使任意指针可信。
借用文字是 UTF-8，非 NUL 终止，仅需存活到调用返回；长度/容量以字节计。
分配器和 runtime 所有指针不跨 ABI，文本框存储/输出缓冲由调用方持有。

每进程最多一个活窗口，尺寸从 32×24 到 320×240。句柄为带 generation 检查的非零不透明
64 位值。destroy 释放服务端窗口并使句柄失效；陈旧、重复销毁、移后使用返回 `STALE`。
C++ 析构尽力清理，`close()` 可观察错误；移动构造/赋值转移唯一所有权并使源失效。
Rust 使用 `Result`、`Drop`、可变文字切片和 UI 线程归属。需明确清理结果时显式 close。

颜色为 `0x00RRGGBB`，高位非零非法。矩形裁剪到窗口；每次文字最多 1024 UTF-8 字节，
附带位图字体绘制 ASCII，其他字形显示替代框。布局提供固定高度竖排和间隔，耗尽返回
`LIMIT`。按钮在区域内鼠标左键按下时触发；文本框在调用方容量内追加有效文字事件并处理
退格。控件为有界单行，进一步焦点/选择策略后置。

poll 保留合成器顺序，空队列返回 `WOULD_BLOCK`。待处理 resize 时，present 也可返回
`WOULD_BLOCK`，应用先 poll resize/reset、重绘后重试。按键包含物理键码、状态和受测美式
布局对应的可选 Unicode scalar；指针为窗口内坐标。焦点变化/队列丢失的 `RESET` 清除按住
状态，客户端也须清理本地状态；runtime 内不执行客户端回调。
剪贴板最多复制 1024 UTF-8 字节到进程内存，与窗口生命周期独立；不持久、不跨进程。
输出太小返回 `BUFFER`，不能截断有效 UTF-8。

guest 配置使用已验收 GCC/G++ 编译普通用户 `.o`，禁用 C++ exceptions/RTTI，然后由 guest
Rust 编译器将所有客户端与 runtime rlib、原生 CRT/链接脚本链接。
独立 `no_std` link anchor 提供 panic 终止和内存 intrinsics；Rust 示例独立编译为 `no_std`
object，其 `main` 由同一 CRT 调用。runtime 不需 libc、C++ 标准库、原生 Rust `std`、
Linux GUI 或宿主预编译应用；guest 编译必须与预备宿主交叉构建分开验收。

`tests/guest/gui/main.c`、`main.cpp`、`main.rs` 为独立语言模板。guest 编辑器先保存源码，
再编译/运行。每个在 (80,80) 打开 240×160 窗口并输出 `READY`，物理 `a` 写入文本框后输出
`KEY`，在局部 (40,130) 接受物理点击后输出 `POINTER`，最后按 `q` 关闭并输出 `DONE`。
C/C++ 模板需附契约头文件。

`note.rs` 是支持显式剪贴板复制的有界便笺。`terminal.rs` 在建立 GUI 状态前启动 native
helper；图形命令框接受 `echo TEXT`、`pwd`，Enter 通过管道提交有界命令，helper 实际
fork/exec `/bin/echo` 或 `/bin/pwd`，从另一管道取回输出并检查退出码。GUI 显示结果后可继续
执行；命令/输出最多 128 字节，空命令框按 `q` 关闭并回收 helper。
不支持 shell 展开、历史、PTY 作业控制、任意可执行程序或转义序列。

源码导出和校验清单同时包含 `README.md`、`README.zh-CN.md`，导出的 SDK 也可切换语言。
