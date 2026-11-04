<!-- Translate native PTY ownership, Linux admission, incremental GUI transport and bounded ANSI behavior. -->
# M17.2：真实图形 PTY 终端

[English](../../en/milestones/M17_PTY.md) | **简体中文**

本文实现 [M17](M17_DESKTOP.md) 的终端契约。本地证据和剩余限制单独记录在
[M17.2 验证](../validation/M17.2.md)。

## 启动及可执行程序准入

显式桌面归档将 `/bin/terminal` 标记为 `anova.native.v1`，将单独构建的
BusyBox 1.36.1/musl 1.2.4 ash 标记为 `linux.x86_64.v1`。源码提交仍为
`1a64f6a20aaf6ea4dbba68bbfa8cc1ab7e5c57c4`。终端配置额外启用
`CONFIG_ASH_JOB_CONTROL=y`，原 M8/M9 BusyBox 配置和构建不变。分发桌面 ISO 时保留
包含实际配置的 `build/images/busybox-terminal-1.36.1-source.tar.gz`；许可证也放在
`/etc/busybox.LICENSE`。

先聚焦启动器，再按 `t`。终端替换第三个应用槽，保持四窗口限额。初始表面在
(450,20)，大小 312×232，对应 50×26 字符。`F4` 关闭，`F5` 关闭当前 PTY 进程族并
启动新 shell；shell 退出或 EOF 后窗口保留，可显式重启。Shift-PgUp/PgDn 每次移动
十行回滚，普通输入返回实时画面。

```sh
cargo xtask desktop-image
cargo xtask run --desktop
# 尚无现成工具链时，先构建已有锁定源码的 M11 工具链：
cargo xtask toolchain-build
cargo xtask ext4-journal-image --linux build/disks/pty-work.ext4
cargo xtask terminal-image --toolchain
cargo xtask run --desktop --toolchain --disk build/disks/pty-work.ext4
cargo xtask desktop-pty-test
```

默认桌面包含 PTY shell，不下载编译器种子。可选编译器归档复用 M11 完整配方、安装
树验证和显式 Linux 条目，不代表编译器自举。`/bin/gcc` 选择已有静态工具配置。例如
`/bin/gcc /etc/terminal-hello.c -o /persist/pty-app`，然后
`/persist/pty-app; /linux/echo RUN-$?`，可真实编译并运行 C 应用。

原生 GUI 程序保留在启动归档。native personality 磁盘可保存源码和日志，但其可执行
标记不准入 Linux 编译输出；应选择新的 Linux personality 工作盘。挂载的 `.anova-abi`
保持不可变，后缀、路径或 ELF 头都不能绕过准入。Linux shell/编译器不获得 GUI 权限。
COM1 仍为独立监督的原生 shell。

## 实验性原生 PTY 接口

`crates/abi/src/pty.rs` 定义独立系统调用 `0x102`，与冻结的 native v1、GUI C ABI 和
原生进程控制 `0x101` 分开。RDI 为 `(1 << 32) | operation`，RSI 为准确会话 ID
（START 时为零），RDX 指向完整复制的 32 字节记录，各字段为小端：

| 偏移 | 字段 | 含义 |
| --- | --- | --- |
| 0 | buffer，u64 | 用户传输/状态地址，其余操作为零 |
| 8 | length，u64 | 传输 0–1024 字节；STATUS 必须为 16 |
| 16 | rows，u32 | START/RESIZE 为 1–128，其余为零 |
| 20 | columns，u32 | START/RESIZE 为 1–256，其余为零 |
| 24 | reserved，u64 | 必须为零 |

| 操作 | 返回值与生命周期 |
| --- | --- |
| 0 START | 分配真实空闲 PTY 和准入的交互式 Linux shell，返回不复用的 PID/会话 ID |
| 1 READ | 非阻塞部分输出；空且仍存活时 WOULD_BLOCK，slave EOF 且缓冲排空后为零 |
| 2 WRITE | 经真实行规程非阻塞部分输入，或 WOULD_BLOCK |
| 3 RESIZE | 更新 winsize，并向真实前台进程组生成 SIGWINCH |
| 4 STATUS | 复制原生 16 字节 WaitStatus，不回收；shell 存活时 WOULD_BLOCK |
| 5 CLOSE | 关闭 master，终止并回收全部带标签的进程族，使句柄失效 |

仅准确创建者可操作句柄。master 保存在内核私有描述符表，不由 native fork/spawn
继承。shell 只获得 slave 的 0/1/2 描述符、独立 SID/PGID、有界 PATH/TERM/PS1/HOME
环境及创建者 cwd。Linux `/dev/tty` 解析为该会话的控制 slave。stdout/stderr 共用
同一 PTY 输出队列，不能根据合流后的字节判断原始流身份。

Linux fork/clone 后代跨 exec、setsid 保留独立成员标签。创建者退出、故障或被监督终止
时关闭并回收整个进程族，包括脱离原会话的后代，不针对无关任务。这是立即清理，不是
未保存内容对话框或全局任务管理接口。START 分配失败会回滚 master/child；READ 用户
复制失败保留数据，STATUS 复制失败保留状态。native `0x101` 仍拒绝存活 Linux 子进程。

所选 ash 需要 WUNTRACED。Linux 适配层现在对真实子进程停止提供一次状态通知，成功
复制后才消费，并唤醒阻塞等待；可与 WNOHANG 组合。继续执行会清除过期停止通知。
未增加 WCONTINUED、按进程组等待、完整多线程作业控制或后台 TTIN/TTOU 强制策略。

## 事件循环与存储限额

每轮 UI 最多处理 16 个 GUI 回复/事件、提交一批排队输入、读取四批 1,024 字节输出，
并推进一个需确认的 GUI 上传。UI 不等待 slave I/O、子进程退出或整窗呈现。私有像素
快照在 PRESENT 前不变；尺寸变化使旧上传失效，并按新尺寸重画。焦点/输入重置丢弃
本地未提交按键。COM1 诊断与 PTY 输出独立，不是第二套 stdout/stderr 捕获通道。

规范输入行沿用既有 255 字节内容加换行的限额。输入缓冲 256 字节，部分写保留剩余内容，满时给出诊断。Ctrl+C 丢弃未提交本地输入，
发送真实中断字符；Ctrl+D 发送真实 EOF；退格使用行规程擦除字符。支持美式 set-1
可打印输入。方向/Home/End/Delete 序列转发给使用 raw 输入的程序；shell 规范输入
支持退格，不是完整 readline。有限呈现/输入队列仍可能拒绝无限物理输入洪泛，不能
宣称超出容量后仍无损。

字符模型最多 52×26 可见字符加固定 256 行历史，总大小小于 45,000 字节且不随输出
分配堆。私有像素最多 320×240×4 字节。缩放保留左上交集，裁剪/补空而不重新折行，
并限制光标；历史满后淘汰最旧行，退出终端后历史丢失。

全局仍为 16 MiB 堆、32 任务槽、每进程 32 FD、128 共享打开描述、2,048 VFS 槽和
四组 PTY/四窗口。稳定启动器/合成器、两个 GUI 同伴、终端和 Linux shell，与 init、
COM1 共用这些限额。编译峰值单独测量；内核边界采样不等于准确 RSS/CPU 时间。

## ANSI 子集

流解析器跨任意读取边界保留状态，不分配内存。

| 输入 | 已实现行为 |
| --- | --- |
| CR、LF、BS、TAB、BEL | 回车、换行/滚动、退格、八列制表、静默响铃 |
| 可打印 ASCII | 延迟折行；不支持的 UTF-8 字符显示为 `?` |
| CSI A/B/C/D、G、H/f | 有界光标移动/定位，标准默认参数 |
| CSI J/K，参数 0/1/2 | 擦除屏幕/行的相应区域，不自动回到原点 |
| CSI m | 重置、粗体/普通、反色/普通、基础及明亮 16 色、默认前景/背景 |
| CSI s/u、ESC 7/8 | 保存/恢复一组有界光标位置 |
| CSI ?25h/l | 显示/隐藏光标 |
| OSC/DCS/PM/APC | 消费并丢弃至 BEL 或 ST，不修改标题、剪贴板或执行任意控制副作用 |

CSI 最多八个四位数字参数；畸形/过长序列丢弃至结束字节，新 ESC 或 CAN/SUB 可取消
未完成 CSI。包含不支持参数的 SGR 整体丢弃，包括扩展颜色操作数。备用屏幕、滚动
区域、插入/删除行、终端查询、完整 xterm、Unicode 排版/输入法及终端全屏编辑器不在
本配置内。字体复用原创 GUI 位图，拉丁字母显示为大写。GUI C/C++/Rust ABI 及记录
布局不变。

规范参考：[XTerm 控制序列](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)、
[Linux 终端尺寸](https://man7.org/linux/man-pages/man2/TIOCSWINSZ.2const.html)及
[waitpid](https://man7.org/linux/man-pages/man2/waitpid.2.html)。未引入第三方终端模拟器实现。
