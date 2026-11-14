<!-- Translate the desktop monitor and fixed three-language build, publication and recovery profiles. -->
# M17 桌面监控与编译流程

[English](../../en/milestones/M17_WORKFLOW.md) | **简体中文**

## 资源、任务与服务

在启动器按 `m`，再用 `1`、`2`、`3` 选择资源、任务或服务。资源页读取受管理物理页、
分配器堆、调度状态、占用的 VFS 节点与已分配位置、打开描述、PTY master、GUI
保留位置及已挂载工作盘。合成器异步处理断开时，已连接 GUI 客户端可能少于保留位置。
页池包括分配器内部保留页，范围以外的内存不计入。堆用量为 buddy 已分配字节。
磁盘计数包含文件系统元数据和日志。挂载进入故障状态时空闲量不可用。
面板可见时每 50 个已交付定时 tick 最多采样一次。CPU 时间、逐任务 RSS 和精确瞬时峰值
均不可用。既有调度点与存储变更峰值保持各自的统计口径。

独立原生监控 ABI 使用 `0x105`，RDI 为 `(1 << 32) | operation`。
操作 0 复制 176 字节 `Resources`，操作 1 复制 32 条各 64 字节的 `Task`，
将未使用条目清零并返回可见条数。RSI 为目标缓冲区，RDX 为精确长度。
操作 2 的 RSI 为精确 PID，RDX 为零。成功终止返回 0，已退出的原生直接子进程返回 1，
原有等待接口仍可领取实际状态。详见[记录布局](../../../crates/abi/src/monitor.rs)。

原生调用者可以读取全局计数。任务列表只展示自身、直接子进程、可见 Linux 子进程的线程
以及调用者创建的带标签 PTY 家族。记录含 PID、父 PID、线程组首进程、PTY 家族和创建 tick。
僵尸任务保留 personality 和实际退出状态，直至被等待或由调度器回收。停止仅接受原生
直接子进程首任务，拒绝越权、陈旧 PID、Linux 进程及线程。原生后代按现有规则重新挂接，
构建家族由独立所有者接口取消。启动器保留最近一次客户端或服务退出，显示停止错误。

在 Files 或终端自身窗口按 `F4` 可协作关闭。Files 继续通过原有对话框保护脏缓冲。
启动器原有的 `3` 键也可终止终端并回收带标签的 PTY 家族。
服务页 `c` 更换合成器，`r` 重启会话，`q` 退出桌面，`p` 请求现有正常关机。
完整会话重启、退出与关机要求先关闭 Files。图形关机还要求活动构建先完成或取消。
主动更换和故障恢复共用三次合成器更换预算。启动器显示应用启动错误，服务页显示更换额度耗尽。
存活的 Files 用原有编辑缓冲重连，终端保留 PTY 和回滚历史。其他客户端用新句柄重新启动。
客户端进程自身丢失或恢复预算耗尽仍可能丢失未保存字节。COM1 独立运行。

## 固定开发配方

使用可写且明确标记为 Linux personality 的工作盘。`--toolchain` 加入 GCC/G++ 11.2.0，
`--rust` 在桌面镜像中加入经过校验的 Rust 1.90.0 和匹配的 musl std。

```sh
cargo xtask fetch
cargo xtask toolchain-build
cargo xtask rust-build
cargo xtask ext4-journal-image --linux build/disks/development.ext4
cargo xtask run --desktop --rust --disk build/disks/development.ext4
```

通过 Files 创建或编辑源文件，再在启动器按 `b` 并选择配方。在图形终端执行已发布路径，
通过 `$?` 检查退出码。在启动器按 `t` 可打开终端。保存后在服务页关机。

| 按键 | 配方和精确源码路径 | 发布路径 |
| --- | --- | --- |
| `g` | GCC 与 `/persist/build.c` | `/persist/build-app` |
| `c` | C++11 模式的 G++ 与 `/persist/build.cpp`、`/persist/helper.cpp` | `/persist/cpp-app` |
| `r` | Rust 2024 与 `/persist/build.rs` | `/persist/rust-app` |

构建配方 v1 使用 `0x106` 和[固定参数表](../../../crates/abi/src/workflow.rs)。
START 在 RSI 接收配方 ID 并独占预留对应 `.next` 文件。READ、STATUS 和 CLOSE
沿用 32 字节构建请求，目标为精确首进程 PID。CANCEL 使用全零请求，终止带标签的整个家族，
并保留首进程的信号退出状态。PUBLISH 使用全零请求，核对所有者、真实零退出、已排空 EOF、
家族无存活成员及非空链接输出。即使暂存清理报告设备错误，CLOSE 仍回收家族和资源。
两个构建接口共用一个全局构建位置和四组 PTY。原生 v1 与既有 GUI、进程、PTY、文件和旧
构建接口保留已有布局和语义。

编译器写入独占预留的易失路径，受原有 tmpfs 单文件 1 MiB 上限限制，避免链接器每次小写入
都触发一次 JBD2 事务。PUBLISH 每次最多向持久暂存文件复制 64 KiB，成功复制后返回
WOULD_BLOCK，全部复制后请求原有受检重命名与替换。源码、持久暂存与发布文件位于同一工作盘。
已存在的易失或持久暂存路径需要检查。这些固定路径位于普通共享 VFS 命名空间。
内核保留两份预留文件的身份，拒绝发布或移除占用原路径的替换文件。
构建者需协调其他写入者对活动任务文件的修改。
发布结果和编译退出分别报告。正常关机应用既有同步和卸载语义。设备错误可能导致挂载失败关闭并
遗留清理证据。断电保证取决于独立验收过的文件系统配置。

启动器保存四条各 8 KiB 的合并 stdout/stderr 尾部，通过尝试号和首进程身份关联。
每轮最多处理十六个 GUI 事件、四次各 1 KiB 的传输和一次确认上传。
`x` 请求取消，`[` 和 `]` 查看其他尝试，PgUp 和 PgDn 滚动，Esc 返回启动器。
启动器存活时日志在合成器更换后保留。
图形终端仅上传变动的字符行和新旧光标所在行，每轮仍最多一次确认上传。几何变化和重连时整窗重绘。

资源、任务和服务证据见 [M17.4 验收](../validation/M17.4.md)，三语言门禁见
[M17.5 验收](../validation/M17.5.md)。
