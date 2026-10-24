<!-- Translate the M12 NETWORK contract and preserve exact commands and historical evidence. -->
# M12：联网 Cargo

[English](../../en/milestones/M12_NETWORK.md) | **简体中文**


有界受控和公网门禁已在 q35/TCG 本地验收。crates.io/GitHub 使用记录的宿主代理和公开 PEM 根证书，直接公网 DNS/传输及远端 CI 另行记录，实际结果见 [M12 验证](../validation/M12.md)。

本增量为 M12 增加明确联网模式。编译器和 Cargo 仍是校验锁定、未修改的官方 Rust1.90.0 musl 二进制。获取、解压、依赖解析、编译和链接都在 AnovaOS 用户进程执行，宿主测试对端只提供源数据，内核目标仍为 x86_64-unknown-none。

## 命令与配置

先 rust-build，创建 Linux 开发盘，再以 run --rust --network --disk 启动。客户系统在 `/persist` 建项目，运行 cargo-online build、--locked 或明确 --offline --frozen，正常 shutdown 保存缓存。具体命令保留下方。

普通 cargo 默认离线。cargo-online 选择联网及 `/persist/cargo-home`，保留显式 CARGO_HOME/CARGO_NET_OFFLINE。Cargo 调用 rustc 和构建脚本时环境不丢失，仍限制单构建、代码生成和链接任务及既有容量。无 NIC 时验证已保存项目须显式指定 --offline --frozen。

启动器用 SSL_CERT_FILE 提供锁定 Mozilla CA 集，Cargo 的 http.cainfo、http.proxy、registry 和 source 配置仍有效。上游用户态库检查 CA 链、日期和服务端名称，不增加绕过；来源和 MPL-2.0 notice 锁在 [inputs.json](../../../ports/rust/inputs.json)。

## 运行时增量

select 每集合最多导入 32 个描述符位，处理真实 VFS/socket 就绪、EOF、无效描述符、受检可写输出、单调截止时间、剩余 timeval 和 EINTR；阻塞时保留原掩码。select/poll 不因 SA_RESTART 被错误当作 read 重启。

TCP 建连上限仍三秒，建立后的协议栈无活动上限为 30 秒，让更短应用传输截止时间先报告自身超时，避免提前断管。

只读文件 MAP_SHARED 支持最多 1 MiB 的 tmpfs/ext2 普通文件。每个映射 inode 使用可失败分配的 1 MiB 版本缓存，身份独立于描述符；成功写入或增长后，变更页在执行用户代码前刷新。fork 共享后备和映射所有权，munmap/exec/exit 释放；删除或替换的 inode 在最后映射释放后才回收。

拒绝可写共享映射和 mprotect 增加写权限，活跃映射时缩小或截断返回 EBUSY，增长超过 1 MiB 也返回 EBUSY。最后映射释放后，可丢弃空闲缓存再增长持久文件。拒绝超过文件最后一页的映射、固定共享位置和大于缓存范围的文件；这不是 msync 或页缓存回写实现。

utimensat/futimesat 更新真实访问和修改时间，支持描述符、受检 dirfd 相对路径、UTIME_NOW/OMIT，拒绝错误指针、范围和标志。ext2 的 128 字节 inode 保存整秒，tmpfs 保留纳秒，显式时间跨干净重启保存；负纪元和超 u32 秒拒绝。ext2 改为有界、可失败的迭代目录导入，Git 深缓存不再受旧八层递归限制，仍受 VFS 节点和 512 字节路径限制。

堆 16 MiB、tmpfs/共享缓存 1 MiB、开发盘 16 MiB/512 inode、2048 节点、32 任务和描述符、八个 Internet socket、单 BSP 和轮询保持不变。M15.5 仅将持久文件扩大到 4 MiB，见 [M15 内核](M15_KERNEL.md)。大 crate、Git pack 或依赖图可能超限。POSIX fcntl 记录锁未实现，Cargo 非致命缓存清理警告保留，不能伪报锁成功。

## 独立门禁

rust-cross-test 增加 select/poll 的信号、EOF、指针、超时负例，共享只读文件一致性、fork、inode 生命周期，真实堆耗尽恢复和时间戳检查。

rust-online-test 在宿主回环端口 18443–18445 提供真实 HTTPS sparse registry 和 Git smart HTTP，归档有独立 SHA-256，Git 锁完整提交，客户系统初始没有依赖。须验证真实 CA/名称/日期失败、损坏和截断归档拒绝、应用超时、修复校验后成功、客户系统构建和无改动构建。第二次无对端、无 NIC 启动只用持久缓存，在全新目标目录重建两个依赖再执行；宿主检查源码、锁文件、缓存和 ELF 摘要，e2fsck 必须通过，私钥只留宿主 build。

rust-public-test 独立获取 crates.io itoa1.0.15 和 GitHub itoa 完整提交 `e2766b868e4ac1ae2bf5bea1ac43d4c0da23b899`，不使用 vendor 替代，再构建和无网重启重建。需要时 ANOVA_CARGO_TEST_PROXY 只提供无凭据、客户系统可达 HTTP 代理，ANOVA_CARGO_TEST_CA 提供公开根；只改变测试镜像，代理传输 HTTPS，不替客户系统编译。

保留原三个 M12 Rust 门禁和所有早期 OS/工具链门禁。原生 libc/std、编译器自举仍独立，不代表 SSH、认证私有 registry、Cargo publish、任意工作区、IPv6 或通用 Linux。

## 原始命令与输出

以下命令、源码与日志保留原文；执行位置及通过条件见上文。

```sh
cargo xtask rust-image
cargo xtask root-image --linux build/disks/rust-online.ext2
cargo xtask run --rust --network --disk build/disks/rust-online.ext2
```

```sh
cargo-online build
cargo-online build --locked
cargo-online build --offline --frozen
shutdown
```

```sh
cargo xtask rust-cross-test
cargo xtask rust-online-test
cargo xtask rust-public-test
```
