<!-- Translate the bounded graphical file/editor workflow and actual save failure guarantees. -->
# M17.3：文件与可靠源码编辑

[English](../../en/milestones/M17_FILES.md) | **简体中文**

本实现推进 [M17](M17_DESKTOP.md) 的最小文件/编辑闭环，接续已验收的
[PTY 图形终端](M17_PTY.md)。实现与已执行的本地检查分别记录于
[M17.3 验证](../validation/M17.3.md)。完整 M17.3 存储故障验收和 M17.5
全图形编译流程仍是独立工作。

## 共享工作盘与启动

原生文件/编辑器客户端与 Linux-musl PTY shell 使用同一个挂载的 VFS
`/persist`。保存为 `/persist/project/main.c` 的文件，可以直接在图形终端用
相同路径读取，无需宿主同步目录或编辑器私有存储。源码是普通数据；可执行文件
准入仍由不可变的磁盘 ABI 标记决定。需要在 PTY shell 执行编译产物时，应选择
新的 Linux 工作盘。

```sh
cargo xtask desktop-image
cargo xtask ext4-journal-image --linux build/disks/editor-work.ext4
cargo xtask run --desktop --disk build/disks/editor-work.ext4
cargo xtask desktop-files-test
# 可选：复用已有且经过验证的 M11 客体编译器种子。
cargo xtask run --desktop --toolchain --disk build/disks/editor-work.ext4
```

指定的持久镜像须通过客体正常关机。原桌面和串口恢复 shell 保持独立可用。
现有桌面、PTY 回归入口仍为 `desktop-test`、`desktop-pty-test`。

## 操作

聚焦启动器按 `f`；Files 替换 C++ 示例槽，PTY 终端仍由 `t` 启动。Files 初始窗口
位置为 (20,260)，尺寸 320×240。

| 按键 | 操作 |
| --- | --- |
| 上/下、Enter、Backspace | 选择目录项、打开/进入、回到父目录 |
| F1 | 打开绝对文件路径或进入目录 |
| F2 / F3 | 新建源码路径 / 重命名编辑视图中打开的文件 |
| F4 | 检查未保存缓冲区后关闭 |
| F5 或 Ctrl+S | 保存；磁盘基线变化时要求明确覆盖 |
| F6 | 按 `y` 确认后删除编辑视图中打开的文件 |
| F7 / F8 | 返回目录视图 / 创建目录 |
| 路径提示中 Enter / Esc | 提交 / 取消 |
| 未保存提示中 `s` / `d` / Esc | 保存并继续 / 放弃并继续 / 取消 |
| 覆盖提示中 `y` / Esc | 确认覆盖 / 取消 |

停止、重启、退出会话前，启动器要求先关闭 Files，以便编辑器检查未保存工作。
这不代表客户端崩溃、合成器失效或宿主终止后的未保存缓冲区恢复。

## 缓冲区与路径边界

单源码缓冲区最多保存 16,384 个精确字节。支持可打印 ASCII、tab、CR 和 LF；
不支持的二进制字节与超长内容会被拒绝，不替换可用缓冲区。路径必须是在词法上以 `/persist`
为根的绝对路径，最多 120 字节；中间路径分量最多 63 字节，可编辑文件名最多 55 字节，为恢复后缀预留八字节。目录视图最多显示 64 项，超出后可用 F1 打开精确路径。拒绝路径穿越、
非法名称和保留的事务后缀。父目录符号链接按普通 VFS 规则解析；词法路径限制
不代表文件系统安全沙箱。缓冲区与目录视图都有固定边界，不扩大文件系统容量。继承的位图字体把拉丁字母
显示为大写，存储字节仍保留大小写。源码视图按光标显示 16 行、50 列，tab 显示为
单个空白格。

成功载入/保存建立精确基线。编辑标为脏状态；切换文件、新建缓冲区、删除及关闭
都要求明确处理未保存工作。其他进程修改磁盘文件后，保存覆盖须明确确认。这是保存前快照检查，未跨全部 syscall 持有排他锁；并发写入者
须协调，以免检查后再次修改。
保存失败保留脏缓冲区并报告实际失败操作，任何错误都不会假装已保存。

## 保存与旧副本策略

保存会排他创建同目录 `.m17-tmp`，写完全部字节，fsync 文件并检查 close。文件及目录 fsync 当前会全局刷新共享
文件系统/设备，不提供独立的每目录日志。
目标已存在时，还会排他创建、fsync 含磁盘当前精确字节的 `.m17-bak`，并在发布前
持久化父目录。准备好的临时文件通过 rename 替换目标，再 fsync 父目录。只有全部
必要操作成功才清除脏状态。

发布前失败保留旧目标；替换后失败保留备份与脏缓冲区，因为命名空间发布或持久化
结果可能不确定。已有事务/恢复文件会引发明确错误，绝不静默截断。成功后的备份
清理为尽力操作，清理错误可能留下可用副本。重试前先检查这些文件，再明确删除。

不确定的保存失败后，先在图形 PTY 查看两个兄弟文件，把选定字节恢复到另一个
路径，再清理证据：

```sh
/linux/cat /persist/src/main.c.m17-bak
/linux/cat /persist/src/main.c.m17-tmp
/linux/cat /persist/src/main.c.m17-bak > /persist/src/recovered.c
/linux/cat /persist/src/recovered.c
```

检查恢复文件之后，再用独立 COM1 原生 shell 只删除已经检查的事务文件，然后
重试编辑器：

```sh
rm /persist/src/main.c.m17-tmp
rm /persist/src/main.c.m17-bak
```

选定图形 BusyBox 配置没有 `rm` 或 `mv`；恢复使用已有 `cat`/shell 重定向和
COM1 原生 `rm`。兄弟文件不存在会报告普通文件缺失，不能假定每个失败阶段都
创建了两个文件。

这些仅是已实现 ext2/ext4 后端的检查型 syscall 保证。rename/fsync 成功不代表任意
原子持久化或断电恢复。正常关机/重启后的字节保持须独立实测；突然断电、块设备失败
及所有容量耗尽点也需要独立证据。本工作不重复或替代 M16 文件系统验收。
