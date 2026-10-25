<!-- Translate the M6 FILESYSTEM contract and preserve exact commands and historical evidence. -->
# M6：文件系统、init 与原生 shell

[English](../../en/milestones/M6_FILESYSTEM.md) | **简体中文**


本页记录原始 VFS、描述符、管道、控制台和 shell 契约，测量见 [M6 验证](../validation/M6.md)。[M5](M5_PROCESSES.md) 的 CPU、装载和调度规则继续有效。[M7](M7_STORAGE.md) 增加 ext2、持久调用和编辑器，[M11](M11_TOOLCHAIN.md) 扩大容量，[M12](M12_RUST.md) 增加受限重命名和目录移除。下述缺项属于历史 M6 范围。

## 启动与命名空间

`xtask apps` 构建七个独立 ELF 并打包 `initramfs.anovafs`。ISO 使用显式 `anova.initramfs.v1` 身份传递 Limine 保留模块，清单包含 SHA-256，宿主造镜像前验证全部原生可执行项。内核检查模块保留范围和归档结构，但信任宿主构建镜像，不形成签名或 Secure Boot 信任链。

`anova-vfs` 是使用可失败 `alloc` 的硬件无关 `no_std` 库，由 BSP 调度器唯一拥有。`/` 为只读 initramfs，提供普通文件、可执行文件、目录和归档符号链接；`/tmp` 为可写 tmpfs，支持创建、读写、稀疏定位、追加、截断、建目录和删除文件；`/dev` 提供规范模式 console 和返回 EOF、丢弃写入的 null。

仅启动时可挂载到已有空目录，挂载根保留父节点，因此 `/tmp/..` 回到 `/`。M6 无用户挂载卸载、重命名、硬链接、目录删除、身份权限、可执行 tmpfs 或磁盘。路径逐分量解析，先检查目录再处理点和双点，不词法消去 `file/..`；符号链接最多八层。绝对路径从根开始，相对路径从 cwd 开始，不能越过根。历史 UTF-8 路径为 1..128 字节，名称最多 63 字节，不含 NUL。

ANOVAFS1 是自有格式：8 字节魔数、小端 u32 条目数；每项依次为 u8 类型、零 u8 保留、u16 路径长度、u64 载荷长度和精确数据。类型 1/2/3/4 对应普通文件、目录、符号链接、原生可执行文件，目录载荷为空。路径必须绝对、规范且父项在前，不能穿过已声明符号链接。拒绝截断、溢出、非法类型路径、重名、坏链接和尾随字节；载荷最多 8 MiB、模块 16 MiB、条目 96 个，不直接把字节转换为结构体引用。

## 描述符与阻塞所有权

每进程 32 个描述符，初始 stdin0 可读、stdout1/stderr2 可写。它们引用全局最多 128 个打开描述对象，后者持有标志、偏移、目标和引用计数。独立 open 有独立偏移，dup2/fork/spawn 共享同一对象。exec 保留描述符和 cwd，M6 spawn 也继承；不能在 VFS 所有权机制之外随意复制描述符。

删除 tmpfs 名称后，已有打开对象继续访问数据，最后关闭才回收节点。追加在单所有者临界区选择当前 EOF，稀疏空隙清零，大小或堆耗尽失败保留旧数据与偏移。创建文件和管道先检查容量，再发布资源。历史上限为 96 节点、128 打开描述对象、16 管道、每 tmpfs 文件 64 KiB 和固定 2 MiB 堆；耗尽明确返回 NO_MEMORY，不能伪造成功。

管道是 1024 字节流。有数据立即读取，空但仍有写者则阻塞，最后写者关闭后 EOF；缓冲满且仍有读者时写入阻塞，无读者则 BROKEN_PIPE。允许部分写，由 `write_all` 重试，不可定位；合法零长 I/O 不等待。

阻塞任务只保留捕获的 syscall 帧，不跨等待保留锁、活跃用户根、借用切片或内核栈续体。每个事件扫描最多 32 个阻塞任务，数据传输或最后关闭改变就绪状态后唤醒并重试复制，无就绪任务则 STI/HLT。用户复制全程 IF=0，逐页检查权限，最多 4096 字节，不跨等待或切换；这不是 SMP 等待队列或非阻塞 I/O API。

## 串口和原生调用

COM1 RX 在调度器迭代中轮询，100 Hz LAPIC 唤醒空闲输入，每轮最多 64 字节；UART 和外设 IRQ 均关闭。规范输入有 255 字节编辑内容和 1024 字节就绪队列，支持 ASCII 回显、退格/DEL、CR/LF 归一化、Ctrl+D。空行 Ctrl+D 产生一次 EOF，有待输入时提交这些字节，队满停止读取。M6 不包含 termios、PTY、信号、作业控制或 USB 输入。

| 编号 | 调用 | 参数和成功值 |
| --- | --- | --- |
| 0 / 9 | write / close | 描述符、缓冲区、长度→字节数；描述符→0 |
| 10 | read | 描述符、输出、长度→字节数，0 为 EOF |
| 11 | open | 路径指针、长度、标志→描述符 |
| 12 | seek | 描述符、有符号偏移、whence 0/1/2→新偏移 |
| 13 | dup2 | 源和目标描述符→目标 |
| 14 | pipe | u64[2] 输出、大小 16→0 |
| 15 | readdir | 描述符、记录、大小 80→1 有条目，0 结束 |
| 16 / 17 | mkdir / chdir | 路径和长度→0 |
| 18 | getcwd | 输出和容量→不含 NUL 的字节数 |
| 19 | unlink | 路径和长度→0 |

RAX、RDI/RSI/RDX 和负错误约定不变，编号不是 Linux/POSIX 编码。open 位为 READ1、WRITE2、CREATE4、TRUNCATE8、APPEND16、DIRECTORY32，未知位拒绝。readdir 固定 80 字节：u64 inode、u32 kind、u32 name_len、完全初始化的 64 字节名称；类型 1/2/3/4 为普通文件、目录、链接、设备，不生成隐式点条目。无效或已关闭的写描述符返回 BAD_DESCRIPTOR。

spawn/exec 现在按路径寻找原生归档可执行文件，裸名使用 `/bin`；ExecRequest 仍是名称加单个最多 128 字节参数字符串，不是 POSIX argv/envp。Rust SDK 包装文件调用，C 可直接使用 `anova_syscall`。Linux 人格由后续阶段实现。

## init、shell 与管道链

最终会话启动 PID1 `/bin/init`，它启动并等待 `/bin/shell`，在 EOF 或退出后重启交互会话。孤儿仍由 M5 内核收养机制处理，init 不是完整 POSIX 服务管理器。测试模式先运行文件系统测试，再执行 shell 脚本。

shell 限制为 512 字节输入、四段管道、32 个解析 token，支持单双引号、转义和 `< > >> |`。参数沿原生契约合并为单串。父进程内置 cd/help/status/exit 拒绝管道和重定向；项目自有 ls/cat/echo/pwd/mkdir/rm/true/false 不是 BusyBox 或 Bash。ls/cat 接受一个可选路径，status 显示前次状态，管道采用末段状态。没有展开、通配符、环境变量、分号、可执行脚本和作业控制。

每个外部段 fork 后设置 stdin/stdout、关闭原端点、应用重定向并 exec；父进程先关闭所有端点，再等待全部孩子，支持超过缓冲容量的流并避免泄漏写者阻止 EOF。早退读者、子段失败或部分 fork/pipe 失败均清端点、唤醒同伴并回收孩子。下方保留会话示例；`/tmp` 在同次启动的 shell 重启后仍在，系统重启后丢失。

## 原始命令与输出

以下命令、源码与日志保留原文；执行位置及通过条件见上文。

```sh
ls /
mkdir /tmp/work
cd /tmp/work
echo hello > message
cat < message | cat
hello-c demo
status
```
