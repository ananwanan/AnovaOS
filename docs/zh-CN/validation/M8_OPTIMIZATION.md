<!-- Record measured M8 copy reductions, stack refactoring and corrected vectored-I/O boundaries. -->
# M8 代码优化验证

[English](../../en/validation/M8_OPTIMIZATION.md) | **简体中文**

记录于 2026-10-22（Asia/Shanghai），分支 `feat/m8-linux-static`，源码基线 `b65ac15`。[原 M8 证据](M8.md) 保持历史，本记录描述后续优化及检查，最新进度见[状态](../guides/STATUS.md)。

## 变更及可观察结果

| 范围 | 修改前 | 修改后 |
| --- | --- | --- |
| Linux C 字符串 | 每个字节一次 checked usercopy/页表检查，最多 129 次调用 | 每个触及页读取一个有界块，单页 128 字节字符串加 NUL 只需一次调用 |
| argv/envp导入 | 独立指针/字符串循环及临时数组 | 统一解码器原位填充调用者缓冲，超大参数返回 E2BIG |
| 启动栈 | 先构造 native header，再解码其指针，最后编码 Linux header | native/Linux 的 payload、指针和 auxv 共用经检查的向下放置，每个 header 仅编码一次 |
| writev 描述符 | 写前面元素时逐个读取后续描述符 | 一次复制整个有界数组，提交写入前检查长度和地址范围 |
| writev 异常 | 后续描述符不可读时，前面字节可能已写才报错 | 数组导入失败不改变文件 offset/data；后续 payload 异常保留已完成字节数 |
| 空随机请求 | 零字节也验指针和 CPU 熵源 | 直接返回 0，不访问用户内存/熵源 |
| mmap 状态 | 保存和复制 first-fit 搜索从未读取的 cursor | 删除未用字段及 fork/load/write赋值 |


复制调用次数由 decoder 宿主测试回调实测，不据此推断全系统提速，也不声明启动时间或吞吐百分比。每次读取仍限单页，因此终止 NUL 位于映射页最后一个字节时，即使下一页未映射也可成功；跨页字符串仅在必要时再复制下一块。UTF-8/NUL/长度检查、checked 地址运算、BSP 固定和真实 usercopy 异常 fixup 均保留。

debug 构建还核对固定 64 KiB 内核栈。最初重构让大参数结构跨 activation closure 返回，exec 时超栈；最终实现原位填充两个缓冲区，只返回标量长度，没有引入 heap 分配或扩栈。

包含 16 个参数和 16 个环境字符串、每串 106 字节的样例现在可装入真实 4 KiB 启动页。原中间 native header 曾不必要地占用空间。超限的 128-byte vectors 仍失败，并检查对齐、终止符、环境字节和 AT_RANDOM 位置。

## 验证

- 65 项宿主测试通过。新增 decoder 用例覆盖最长单页字符串、页末 NUL、必要但未映射的下一页、合法跨页、非法 UTF-8、缺 NUL、指针溢出和 16/17 项 vectors。
- 真实 Ring 3 Linux probe 增加页末路径 open/unlink、非法 exec vector/E2BIG、不可读 writev 数组且 offset 不动、I/O 前拒绝非法有符号长度、后续 payload fault 前保留部分完成字节、空 vector 仍检查非法 fd，以及 getrandom(NULL, 0, 0)。
- smoke 必须看到成功 probe 的以下标记；仅 startup banner 不满足新边界用例：

```text
[LINUX:BOUNDARY] page_end=true vector_fault=true writev_partial=true empty_random=true
```

- 格式、用途清单、拒绝 warnings 的 host/kernel Clippy 和 native app 检查通过。
- 完整 QEMU 保留全部 13 个成功/失败分类及历史用户隔离、CPU/FPU、进程、VFS 和存储回收。
- 普通 UART 保留 musl/ash 启动、真实管道/重定向输出、状态 0、EOF 重启 shell 及跨启动 tmpfs 丢失。
- 六个持久化场景保留保存/重启字节和哈希一致性、native 执行、干净 e2fsck、只读/未知特性拒绝和真实 I/O 错误。

构建命令及 M8 剩余限制不变，日志在 `build/logs/`。不声明远端 CI 或推送。vector import 行为按 Linux v6.16 [iov_iter 边界](https://github.com/torvalds/linux/blob/v6.16/lib/iov_iter.c)核对，适配器仍在既有 M8 有界范围独立实现。

本次更新 freestanding probe 的 SHA-256 为 `ebb2f2764f2caafd268ba287b2e4448dd64bb2abc3d706eb1e68c834e8568f25`，生成镜像清单同时记录显式 Linux personality；原 M8 记录保留原始哈希。
