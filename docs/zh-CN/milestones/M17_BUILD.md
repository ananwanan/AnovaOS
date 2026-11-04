<!-- Describe the controlled compiler, bounded logs, ownership and retention contract in Simplified Chinese. -->
# M17.4 第一批：构建诊断与任务状态

[English](../../en/milestones/M17_BUILD.md) | **简体中文**

这是 [M17.4](M17_DESKTOP.md) 的首个有界交付，不代表整个资源、任务与服务阶段完成。
实际执行结果见 [M17.4 验收](../validation/M17.4.md)。

普通启动器获得焦点后按 `b` 进入诊断，`g` 显式启动固定命令：

```sh
/bin/gcc -fdiagnostics-color=never /persist/build.c -o /persist/build-app.next
```

通过 Files 或现有 PTY 创建并保存 `/persist/build.c`。使用已有 M11 准入工具链；
需要运行产物时选择 Linux personality 工作盘：

```sh
cargo xtask terminal-image --toolchain
cargo xtask ext4-journal-image --linux build/disks/build-work.ext4
cargo xtask run --desktop --toolchain --disk build/disks/build-work.ext4
cargo xtask desktop-build-test
```

未打包编译器的桌面会报告真实启动失败。`[`/`]` 切换前后记录，PgUp/PgDn 每次滚动
十行，Esc 返回启动器。原有 `f`/`t`、Files 未保存保护、COM1 恢复和共用三次替换
上限保留。诊断使用启动器原有窗口，不提高 GUI 或内核资源配额。

## 身份、所有者与生命周期

每次显式启动尝试均取得启动器进程内单调增加的编号，启动失败也独立记录。
成功 START 还返回当前内核启动周期内从不复用的编译器首进程 PID；exec 后身份保持不变。调度槽位
不是身份。启动失败没有任务 PID，不继承上一轮结果。私有 master 与句柄仅属于
准确的原生创建者；原生 fork 子进程、其他进程及交互 PTY 接口均不能使用。

内核明确按 `linux.x86_64.v1` 准入 `/bin/gcc`，传递固定 argv。没有交互 shell
命令、成功标记解析或任意命令输入。原有不可变可执行准入与工作盘 ABI 标记仍然
适用。首进程仅接收 slave 描述符 0/1/2、自己的会话和进程组、已有有界编译器环境
及创建者 cwd。Linux 后代在 fork/exec/setsid 后保留现有 PTY 族标签，不取得 GUI 权限。

全局同时只保留一个受控构建任务，直到 CLOSE；它与交互终端共用四个 PTY 槽位。
START 独占创建临时产物，已有临时或恢复文件会被拒绝，不会截断。启动部分失败时
关闭描述符、终止并回收已创建子进程，只删除本次新建的临时路径。文件系统 I/O
错误可能留下需检查的临时证据；资源回滚不承诺修复存储。其他写入者必须协调
这些固定路径，这不提供文件锁或文件系统沙箱。

STATUS 复制首进程真实的原生 `WaitStatus`，不回收它，也不报告交互 shell 状态，
不从输出推断状态。首进程结束后 UI 分轮排空缓冲输出；如果存活后代仍保持 slave
打开，UI 在队列暂时为空或取得首进程状态后最多三轮时结束排空；没有 EOF 就标记
`cut=true`，随后 CLOSE 终止并回收整个标签族，包括持续写入的后代。
不会无限等待脱离会话的写入者，也不承诺其未来输出。READ/STATUS 用户复制失败
保留内核中尚未交付的字节和状态；UI 重试状态读取，未知状态不能变成成功。

仅在真实 reason=0/code=0 且采集完整、没有错误后，通过已有受检文件接口将临时产物替换为
`/persist/build-app`。非零或异常构建保留原可用程序。发布失败独立于编译器退出
状态显示；发布结果不确定时需检查真实存储。CLOSE 删除未发布临时产物、关闭
master、回收完整进程族并使句柄失效。新任务可复用槽位，不能复用旧 PID。
创建者退出或故障同样清理任务族。CLOSE 在无条件回收任务、关闭描述符并使句柄失效后，才返回临时产物清理错误；应检查存储，不要重试旧句柄。

发布不是新的保存事务或持久性保证，不会自动 fsync 程序与父目录。使用普通
`sync`、干净关机走独立验收的保留路径。任意并发写入、发布期间设备故障和
断电需要单独验收。

## 日志与保留边界

slave stdout/stderr 共用真实 PTY 流，界面准确标为 **合并 stdout/stderr**。
保存原始字节；固定 GCC 选项禁用诊断颜色。显示时控制字符与非 ASCII 替换为
`?`，忽略 CR，按 50 列折行；不推断流来源，也不提供 ANSI 支持。

四条固定记录分别保留最新 8,192 个原始字节。容量满后逐字节丢弃最旧内容，
显示精确丢弃计数，可能截断半行。第五次尝试淘汰最早记录并增加可见记录淘汰
计数。任务元数据、真实退出 reason/code、发布结果和剩余日志均属于本条记录，
不关联可复用调度槽位。新任务运行或结束时仍可选择旧记录。

每轮事件循环最多处理 16 个 GUI 回复或事件、读取四批 1,024 字节、推进一次已确认
GUI 上传。每帧最多显示 12 行，最多扫描固定 8 KiB 日志两遍。日志没有堆增长，
上传期间栅格快照保持不变。启动器复用终端和 Files 的异步传输，不改变 GUI ABI。

| 边界 | 实际保留行为 |
| --- | --- |
| 构建结束 | 保留记录与日志尾部，关闭并回收任务资源 |
| 终端关闭或重启 | 启动器记录不受影响 |
| 合成器或启动器请求的会话替换 | 同一启动器进程保留记录与活动构建；重连时有限 PTY 缓冲提供背压 |
| 启动器退出、故障，或启动新进程 | 记录丢失，关闭并回收所属任务族 |
| 系统重启 | 记录丢失，只有明确保存的文件系统产物可能保留 |

不承诺持久日志文件或未保存缓冲的崩溃恢复。仍为 16 MiB 堆、32 个任务槽位、
每进程 32 FD、128 个共享打开描述、四个 PTY 与四个 GUI 窗口。全局资源面板、
通用任务终止、更广服务恢复、任意构建配方和 M17.5 留待后续。

## 实验性 native build v1

`0x104` 独立于冻结 native v1、进程控制 `0x101`、交互 PTY `0x102`、文件发布
`0x103` 和 GUI C ABI。RDI 为 `(1 << 32) | operation`，RSI 为 START 的零或准确
所属任务 PID，RDX 指向完整复制的 32 字节记录。四个小端 u64 依次为 buffer 地址、
length、保留零、保留零；大小 32，对齐 8。

| 操作 | 请求检查与结果 |
| --- | --- |
| 0 START | buffer、length、reserved 全零，返回任务 PID |
| 1 READ | 长度 0–1024、reserved 零；部分字节、WOULD_BLOCK，或排空 EOF 后的零 |
| 2 STATUS | 长度准确为 16、reserved 零；复制已有 WaitStatus，存活时 WOULD_BLOCK |
| 3 CLOSE | buffer、length、reserved 全零；关闭、回收并使准确所属任务失效 |

零长度 READ 不传输数据，直接返回零；只有非零长度 READ 返回零才表示 EOF。

版本、操作、保留字段、长度或 START ID 错误返回 INVALID_ARGUMENT。受检用户访问
拒绝不可访问、跨页故障或只读输出范围，不消耗待交付结果。外部、过期及跨接口
句柄返回 NO_CHILD；已有受控任务返回 BUSY；PTY 或共享打开描述耗尽返回 FILE_LIMIT；
任务耗尽返回 PROCESS_LIMIT。缺失或未准入编译器、文件预留失败保留真实错误。
没有扩展 native v1 或 GUI 布局。

PTY 写入、poll 就绪与阻塞写入者唤醒共用相同的 CRLF/回显预留和输入 EOF 条件。
输出只剩一个字节空间时，需要预留两个字节的写入者继续阻塞，直到读取释放足够空间，
避免无节流输出下虚假就绪循环使不可抢占内核无法调度；容量和写入策略保持不变。

原生启动器、Files 和终端连接使用已有的 100 tick 连接窗口等待异步槽位回收。四客户端配额持续占满仍返回 NO_SPACE；已停止进程可在该有界等待内释放槽位。不增加 GUI 配额，也不增加三次会话替换上限。
