<!-- Translate the M7 STORAGE contract and preserve exact commands and historical evidence. -->
# M7：持久化存储与源码编辑

[English](../../en/milestones/M7_STORAGE.md) | **简体中文**


本页定义 PCI/virtio DMA、有界 ext2、原生持久化调用、镜像生命周期和串口编辑器。历史结果见 [M7 验证](../validation/M7.md)，[M5](M5_PROCESSES.md)、[M6](M6_FILESYSTEM.md) 的进程与 VFS 契约继续适用。M11/M12 后续扩大命名空间并实现受限权限、时间戳和重命名；M15.5 将持久文件扩至 4 MiB，不能把历史 M7 限制当作当前总览。

## 设备、映射与 DMA

参考平台为 QEMU 8.2.2 q35/TCG、单 qemu64 BSP、256 MiB RAM。PCI 配置机制 1 枚举总线 0、已配置下游桥和多功能设备，仅接受一个非过渡型 `1af4:1042` virtio-blk。驱动验证能力链、区域、BAR 大小和通知范围；探测 BAR 时关闭解码与总线主控，恢复后再启用解码。设备页先排除 RAM/固件重叠，再映射为内核可写、UC、NX，供 M3 导入。

设备协商 VERSION_1 和 FLUSH，并尊重只读特性；不协商 packed ring、间接描述符、EVENT_IDX 或多队列。INTx、MSI/MSI-X 均禁用或屏蔽，向量设为 NO_VECTOR。分离队列有八个描述符，同时只保留一个请求。描述符、可用环、已用环、请求头、512 字节中转缓冲区和状态共用一个清零、连续的 DMA32 页，由 `Owner::Dma` 管理，不能把用户指针或借用栈交给 DMA。

volatile 和 x86 一致性内存屏障保证先发布描述符再通知、先获取完成再复制数据。检查已用索引、头编号、长度、状态、扇区范围和溢出，并在附加时实读最后扇区。FLUSH 为真实设备请求。完成和重置使用 ACPI PM timer，独立于 LAPIC 节拍，超时两秒；IF=0 轮询期间不持自旋锁或堆锁。确认重置后才释放 DMA，失败则关闭总线主控并隔离页面。外设 IRQ 仍屏蔽，轮询会延迟用户执行，不是异步块层。

## ext2 挂载范围

只读 initramfs 仍为 `/`，tmpfs 为 `/tmp`，devfs 为 `/dev`，整张无分区表的 ext2 数据盘挂到 `/persist`，与 ISO/ESP 分离。

| 属性 | 支持范围 |
| --- | --- |
| 布局 | revision 1、单块组、4 KiB 块和分片、首数据块 0 |
| inode | 128 字节、首普通 inode 11、最多 4096 个 |
| 卷 | 64..32768 块，不超设备；工具创建 16 MiB，原生盘 128、Linux 开发盘 512 个 inode |
| 元数据 | 块位图 2、inode 位图 3、inode 表 4，元数据连续保留 |
| 特性位 | COMPAT=0，INCOMPAT 仅 FILETYPE=2，RO_COMPAT=0 |
| 对象 | 普通文件和目录，无 ext2 链接、硬链接、xattr、ACL 或设备节点 |
| 文件 | 直接块加一个一级间接块，M15.5 起持久普通文件最多 4 MiB |
| 原 M7 目录 | 既有目录最多 12 个直接块，新目录一个块，不增长或删除目录 |
| 原 M7 命名空间 | 96 节点、名称 63 字节、路径 128 字节、导入深度八层 |
| 编辑器 | 16 KiB ASCII 缓冲区，规范串口输入每行最多 255 字节 |

拒绝未知特性、非法 inode 标志/类型、脏卷、不可能几何/计数、坏目录记录、未分配指针、元数据别名、重复块和 EOF 外指针。不尝试降级 ext3/ext4 特性。拒盘后 initramfs shell 继续工作，`/persist` 不可用。

启动只导入命名空间，文件数据仍在磁盘。读写、追加、定位、截断、建目录和删除使用真实 inode；dup/fork/spawn/exec 保留共享偏移。已删除但打开的文件在最后关闭前可读，随后释放 inode 和块。临时块缓冲区用可失败堆分配，ELF 装载只建立一次自有字节副本，不扩大历史 64 KiB 栈。

原生盘 `.anova-abi` 必须精确为 `anova.native.v1\n`，它决定可执行磁盘文件的 ABI，install 复制后设置执行位；仍需验证 ELF，不从 OSABI 猜测。Linux 使用独立明确的磁盘策略。

M15.5 在同一直接块/一级间接块格式内将持久普通文件从 1 MiB 扩为 4 MiB，不重格式化旧盘。tmpfs 与只读共享映射缓存仍为 1 MiB；活跃共享映射阻止超过缓存上限的增长并返回 EBUSY，最后映射释放后可丢弃空闲缓存再增长。大文件仍可普通 I/O 或私有映射，见 [M15 内核重建](M15_KERNEL.md)。

## 写入顺序、错误与关机

首次修改先清除超级块 clean 位并刷写。新块先清零再发布指针；数据先刷写再扩大 inode 大小；新 inode 和位图先刷写再发布目录项。截断先移除引用并刷写，再释放块；删除先移除目录引用并刷写，最后关闭才回收内容。

没有脏数据缓存，sync/fsync 发送真实 FLUSH。已发布写入后的修改失败，包括临时缓冲区 OOM，会阻止后续文件系统 I/O，不能再声称干净关机。普通容量预检查保留旧内容，但物理部分写入失败不回滚或伪报成功。

shutdown 先在描述符完整时刷写，已坏磁盘返回错误而不禁用终端；随后停止写者、关闭描述符及孤儿文件、刷写数据、记录并刷写 clean 位、确认重置后释放 DMA，最后请求 ACPI S5。晚期失败只诊断并停机，不输出关机成功。仅支持锁定 QEMU 固件的 PM1a 和常量 `_S5`，不是完整 AML 解释器。运行器还要求 QEMU 真正成功退出。

ext2 无日志。强制终止或断电可能损坏元数据、泄漏分配或部分覆盖数据；脏盘拒绝，需要宿主离线 e2fsck 恢复。本阶段没有原子保存、重命名、在线恢复或任意断电保证；M12 的受限重命名也没有增加事务日志。

## 原生接口与使用

增量调用为 20 sync、21 fsync、22 shutdown、23 set_executable。fsync 只接受普通文件或目录并刷写卷，shutdown 成功不返回，执行位仅允许可写原生磁盘文件。open 位 64 为 EXCLUSIVE，须配 CREATE，既有目标在修改前返回 ALREADY_EXISTS。-21/-22/-23 分别是 IO_ERROR、INVALID_FILESYSTEM、DEVICE_TIMEOUT，与 Linux errno 分离。Rust 有封装，C 可用 `anova_syscall`；当前单用户系统没有额外关机权限模型。

root-image 只创建新普通 16 MiB 文件，加入标记、`/src/hello.c` 和 `/bin/hello-c`，拒绝覆盖。选盘必须是 `build/disks/` 下的直接普通文件，拒绝符号链接或真实设备。普通 run 用临时盘，不隐式格式化旧盘。保留数据需显式磁盘并在客户系统执行 shutdown，Ctrl+C 可能留下脏盘。

编辑器 p 打印、a 追加到单独的点、d N 删除、c N 替换、w 保存并 fsync、q 拒绝未保存修改、q! 丢弃；失败保留内存缓冲区。保存先截断再写，不能保证原子替换。cp 和 install 均排他创建，后者还设置执行位，拒绝已有目标或源别名。

persistence-test 经真实串口编辑和 syscall，在独立盘上正常关机重启，宿主比对源码、文本、ELF SHA-256，运行 e2fsck，并覆盖只读盘、未知特性和真实块写入/刷写失败。它证明存储和编辑，不代替 M11/M12 的系统内编译验收。没有导入第三方驱动或文件系统实现。

## 原始命令与输出

以下命令、源码与日志保留原文；执行位置及通过条件见上文。

```sh
cargo xtask root-image build/disks/work.ext2
cargo xtask run --disk build/disks/work.ext2
```

```text
edit /persist/src/demo.c
a
int main(void) { return 42; }
.
w
q
install /bin/hello-c /persist/demo
/persist/demo saved-native-app
status
sync
shutdown
```

## 规范与来源链接

- [OASIS Virtio 1.2](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html),
- [Ext2 on-disk description](https://www.nongnu.org/ext2-doc/ext2.html) and
- [ACPI hardware registers](https://uefi.org/specs/ACPI/6.5/04_ACPI_Hardware_Specification.html)
- [QEMU blkdebug](https://www.qemu.org/docs/master/devel/testing/blkdebug.html),

## 补充规范链接

- [kernel ext2 documentation](https://docs.kernel.org/filesystems/ext2.html)
