<!-- Specify the allocation-free Rust GUI runtime, stable C ABI and guest native build boundary. -->
# M14.4 GUI 运行时与 SDK 契约

[English](../../en/milestones/M14_RUNTIME.md) | **简体中文**

本阶段在已验收的 [M14.3 合成器](M14_COMPOSITOR.md) 上增加用户态 Rust 库，不增加内核 syscall、显示映射、Linux GUI personality 或原生 Rust std。运行时管理私有光栅、文字、布局、控件和应用事件接口；合成器保留窗口策略、焦点、输入路由和 scanout。阶段划分见 [M14 GUI](M14_GUI.md)。

## 库 ABI 与所有权

独立版本化接口为 `anova.gui.v1`（`0x00010000`），与 native syscall v1、实验性 GUI wire 版本 1 和源码 SDK 发行版 1.0.0 分离。Rust 实现全部 `extern "C"` 导出函数；生成的 C 声明、仅移动 C++ RAII 和安全 Rust 包装调用同一实现。无回调注册，应用轮询有序事件并决定重绘时机。

版本、窗口描述、矩形、事件和布局记录均为固定 `repr(C)` 整数字段结构。C 头文件断言大小、对齐及部分偏移；独立语言客户端验证实际协商/链接。`sdk/gui/generate.py` 确定性生成头文件，`--check` 在导出前拒绝过期头。ABI 不传递 Rust 布局、异常、分配器所有权或内部借用指针。文字长度均为 UTF-8 字节数，输入仅调用期间借用，输出写入调用者存储。修改前检查 null、对齐、大小、版本、文本和颜色；非空指针仍须指向有效可访问内存，违约异常仅终止客户端而非内核。

每进程一个运行时、最多一个窗口，私有 raster 上限为 320×240 RGB 像素。本地句柄世代号单调递增，销毁后即使新建窗口旧句柄仍失效。句柄是进程本地整数，不是显存地址或服务器权限；合成器独立验证 owner/session/generation。不需要堆分配，容量不足返回稳定负错误码，不虚构堆分配。

应用须在一个 UI 线程串行调用。原子 guard 拒绝并发/重入，防止全局可变存储别名。本配置没有回调、用户线程或信号回调；GUI 初始化后不支持 fork，图形终端在创建 GUI 前创建命令子进程。destroy 释放远端窗口、关闭连接并失效本地状态；进程退出仍由内核清理。合成器故障后应用可清理本地状态并显式新建窗口。

## 渲染与输入

RPC 发送/回复阶段各等待最多 500 native timer ticks，轮询不等待。32 槽事件队列溢出时发 RESET 并保留最新 resize 几何。上传期间 resize 返回 `WOULD_BLOCK`，应用先轮询几何并重画再重试。

运行时提供裁剪矩形、自创 5×7 位图字形、竖直布局行、按钮和文本框。支持 ASCII，小写共用大写形状，其余有效 Unicode scalar 显示替代框；UTF-8 检查/存储保留原始字节，不提供 Unicode shaping、比例字体或 IME。

原生传输复制有界上传块并等待确认，再 present。RPC 期间事件保留在有界队列，溢出重置按住历史；空轮询返回 `WOULD_BLOCK`。焦点变化和设备重置清除按住输入。受测 US 键映射独立于原始 key code，文本框接受文字及有界编辑，按钮按窗口本地坐标命中。每调用上限和错误见 [SDK 契约](../../../sdk/gui/README.zh-CN.md)。

UTF-8 剪贴板有界、进程本地、不持久化，无跨进程选择 owner 或后台读取其他应用文字。桌面级剪贴板需独立 broker 和权限策略。CPU 复制不保证跨块原子帧或 vblank。

## guest 原生开发

`cargo xtask gui-sdk` 导出带校验和源码 SDK 到 `build/sdk/anova-gui-sdk-1.0.0`，独立于未改变的 native SDK 1.0.0。`gui-sdk-image` 与受测 M11 GCC/G++、锁定 M12 Rust musl 工具一起打包。原生卷不可变 `/persist/.anova-abi` 选择 `anova.native.v1`，编译工具则是显式 Linux-personality initramfs 程序。

镜像内 `gui-build c`、`gui-build cpp`、`gui-build rust` 使用真实 guest 工具独立编译 `/persist/main.c`、`main.cpp`、`main.rs`。GCC/G++ 生成 freestanding 对象；guest rustc 构建无依赖运行时 rlib，再用原生 CRT/panic/intrinsics anchor 链接。C++ 仅移动 RAII、禁用异常和 RTTI；Rust 使用 no_std 安全包装。编译器 musl 目标提供 core/代码生成，产物调用 native syscall 并按 native 策略准入，不代表有原生 libc/std。

`gui-run /persist/gui-c` 启动隔离合成器/客户端，报告真实 wait 状态并回收双方。demo 收到指定物理键/按钮后退出，F11 再停止合成器。Rust note 验证文本/控件；有界 Rust 图形终端接受 `echo TEXT` 和 `pwd`，用管道执行真实 native 命令并显示输出，关闭时回收预先创建的 helper。它不提供通用 PTY、shell job control、ANSI 模拟或无限制命令环境。

`gui-sdk-test` 经 guest 编辑器修改源码，独立构建各语言程序，注入 QMP 物理输入、检查 scanout，关机重启后比较源码/ELF。负例包括版本、生命周期、UTF-8、容量和刻意客户端指针异常，实际结果见 [M14.4 验收](../validation/M14.4.md)。M14.5 GPU 绘制单独验收。
