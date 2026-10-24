<!-- Translate the M10 NETWORK PACKAGES contract and preserve exact commands and historical evidence. -->
# M10：网络与包管理

[English](../../en/milestones/M10_NETWORK_PACKAGES.md) | **简体中文**


M10 增加 modern virtio-net、smoltcp 0.12.0、有界 Linux IPv4 客户端、真实用户态包管理器、本地校验锁仓库和受控 HTTPS，结果见 [M10 验证](../validation/M10.md)。[M11](M11_TOOLCHAIN.md) 编译器和 [M12](M12_NETWORK.md) 公网 Cargo 独立验收。

## 设备、时间与所有权

网络必须显式 `run --network`。普通 run、终端、持久盘和旧 13 场景不带网卡。QEMU 用户网络为 10.0.2.15/24、网关 10.0.2.2、DNS 10.0.2.3，无需桥、TAP、防火墙或特权网络设置。

PCI 驱动检查 `1af4:1041` 能力，网络 BAR 放独立 UC/NX 窗口，仅协商 VERSION_1/MAC。RX、TX 各拥有一个一致性 DMA32 页、两个描述符和 1536 字节缓冲区；设备头 12 字节，以太帧最多 1514 字节。屏障维护索引与载荷所有权，检查编号、长度、环推进和重复完成；RX 复制后才复用。确认重置后释放 DMA，失败隔离页面，关机在 S5 前停止网络和磁盘 DMA。

调度器在 syscall、抢占和空闲边界以校准 100 Hz 时间轮询，无用户引用跨切换，外设 IRQ 仍屏蔽。没有 SMP、异步 IRQ 网络、DHCP、IPv6、转发或实物 NIC。CMOS RTC 两次采样并检查更新状态、编码、12/24 小时和公历日期，QEMU 明确 UTC；墙钟从采样加 LAPIC 节拍推进，坏 RTC 使其不可用。单调时间不改，无 NTP、调时 API 或证书绕过。

## Socket 与 DNS

Linux 地址结构和 errno 留在适配器，VFS 只在真实打开描述对象中保存不透明 socket ID。dup/fork/共享线程表/CLOEXEC/exec/最后关闭遵循既有所有权。最多八个公开 socket，缓冲区可失败分配；另有 16 个传输槽限制关闭中的 TCP，最多五秒排空数据和 FIN，UDP 与创建失败立即回收。

支持 AF_INET STREAM/DGRAM、NONBLOCK/CLOEXEC、本地或通配 bind、TCP/UDP 客户端 connect、读写、sendto/recvfrom、端点查询、shutdown、SO_ERROR 和接收/发送超时。poll/ppoll、MSG_DONTWAIT、EAGAIN、EINPROGRESS、超时、EOF、拒绝连接均真实实现。阻塞调用可 EINTR，适用读写支持 SA_RESTART，MSG_NOSIGNAL 抑制 SIGPIPE。接收数据报前先验地址长度输出；UDP 载荷最多 1472 字节，单次复制最多 8 KiB，允许部分进展。

M10 不支持 TCP 监听接入、IPv6、Unix socket、socketpair、sendmsg/recvmsg、向量 socket I/O 或任意选项；M12 后增加的匿名 SEQPACKET 对是独立局部 IPC 范围。普通文件 readv 为 musl 读取 CA，部分进展规则同 writev。原生到 Linux 执行桥最多按 ASCII 空格拆 15 参数，复杂引号用 Linux ash。

Ring 3 的 `dns NAME [SERVER_IPV4 [PORT]]` 发送真实 UDP A 查询，检查随机 ID、来源、重复问题、标志、压缩名称和记录长度；循环、截断或无响应明确失败，截止两秒。默认 10.0.2.3:53，受控测试在 10.0.2.2:18082 回答 repo.anova。没有 AAAA、DNSSEC、搜索域或 TCP 回退，也不代表 musl getaddrinfo。

## 用户态 TLS

pkg 和 tls-test 链接未修改、锁定 Mbed TLS 2.28.10，TLS 不进入内核。要求可信 CA 链、预期名称和日期；无有效墙钟拒绝，熵来自受检 getrandom/RDRAND，受控对端协商 TLS 1.2。负例必须出现真实名称、不信任、过期标志，普通 I/O 失败不算。

`/etc/pkg-ca.pem` 是短期本地测试根，私钥只在宿主 build/compat/tls-fixture，绝不进入镜像或包；untrusted-ca 为负例，构建会刷新将过期证书。它不是公网 CA 集，客户端只连接 QEMU 网关并验证 repo.anova。任意公网仓库、重定向、代理、生产 TLS 和公共 registry 不在 M10 范围。

## 包格式、更新与数据

ANOVAPK1 为小端、最多 128 KiB，128 字节头、最多四个 96 字节描述符。记录名称、整数版本、x86_64、anova.native.v1、最低 SDK1、Apache-2.0 和每文件 SHA-256，强制 LICENSE。条目为有界单分量名称，遍历、重复、非法模式、ABI/依赖不匹配、额外或截断字节和摘要错误均在写盘前拒绝。

`/repo/INDEX` 独立锁整包摘要。demo1/2 含真实 hello-c、版本文本和许可证，安装到 `/persist/pkg/NAME-a/b`，内部 file0..3，清单保原名称、模式和摘要。demo 的 file0 为程序，file1 为 message.txt，file2 为 LICENSE。用户数据放 `/persist/data/NAME`，不能放安装槽。

安装先写入并 fsync 非活跃槽及清单，再发布带校验和的代际记录，选择最高有效代际。暂存失败删除非活跃输出并保留旧版，升级必须提高版本，卸载先发布墓碑再删文件，用户数据保留。故障测试真实写一文件后中止升级；install-file 还接受独立外部摘要。此契约仅覆盖正常关机与普通失败；无日志 ext2 不提供断电事务或物理 I/O 失败后的回滚保证，也没有依赖求解、发行版包、签名或并发安装器。

## 命令与验收

宿主另需 OpenSSL、Python3，无第三方 Python 模块。源码获取检查精确 Git 提交，Cargo.lock 锁网络库和传递校验和。下方保留命令和客户系统会话。DNS 示例需受控对端，pkg fetch 需 HTTPS 对端和与镜像相符的 CA。

network-test 检查真实 TCP/UDP/DNS、错误指针、就绪、超时、信号、fd 复用及拒绝连接。package-test 验证离线安装、升级、回滚、移除、真实程序执行、跨重启版本和数据、e2fsck、严格 TLS 正负例及 HTTPS 获取。既有 CPU、内存、M8/M9、终端和持久化门禁继续保留。

## 原始命令与输出

以下命令、源码与日志保留原文；执行位置及通过条件见上文。

```sh
cargo xtask network-test
cargo xtask package-test
cargo xtask run --network --disk build/disks/work.ext2
```

```text
dns repo.anova 10.0.2.2 18082
pkg install demo 1
pkg status demo
/persist/pkg/demo-a/file0
pkg install demo 2
pkg remove demo
```

## 精确来源与产物标识

- `2fc8413bfcb51354c8e679141b17b3f1a5942561`
