# 官方参考与设计依据

查阅日期：2026-10-20。下列资料用于核对事实，项目阶段与架构选择由本仓库自行制定。链接的默认分支内容会变化，实施时必须将实际依赖固定到版本/提交。

| 官方资料 | 用途 |
| --- | --- |
| [Rust：x86_64-unknown-none](https://doc.rust-lang.org/rustc/platform-support/x86_64-unknown-none.html) | 裸机 ELF、无 std、默认 code model、无 red zone、浮点/SIMD 限制 |
| [Rust：Platform Support](https://doc.rust-lang.org/rustc/platform-support.html) | target 与 host tools 的区别、Linux-musl target 的支持分类 |
| [Rust：Custom Targets](https://doc.rust-lang.org/rustc/targets/custom.html) | 自定义 target 只是代码生成配置的一部分，接口有版本约束 |
| [Cargo：build-std](https://doc.rust-lang.org/cargo/reference/unstable.html#build-std) | 构建标准库的实验性机制与工具链要求 |
| [Limine Boot Protocol](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md) | 协议修订、入口状态、内存图、framebuffer、HHDM 等 |
| [Limine 官方仓库](https://github.com/limine-bootloader/limine) | 引导器、支持架构与对应发行版本 |
| [Linux：ABI description](https://docs.kernel.org/admin-guide/abi.html) | 用户态 ABI 的稳定性与接口记录范围 |
| [Linux：kernel driver interface](https://docs.kernel.org/process/stable-api-nonsense.html) | 区分用户态接口和内核内部驱动接口 |
| [musl 官方源码：INSTALL](https://git.musl-libc.org/cgit/musl/tree/INSTALL) | libc 构建环境与 Linux 系统调用依赖；实施时按所选版本重新核对 |
| [GCC：Configuration](https://gcc.gnu.org/install/configure.html) | build/host/target、sysroot 与配置要求 |
| [GCC：Building](https://gcc.gnu.org/install/build.html) | cross build、运行库与 bootstrap 的不同流程 |
| [OASIS：virtio 规范目录](https://docs.oasis-open.org/virtio/virtio/) | 选择正式规范版本，核对 PCI、队列、block/net 等设备语义 |

## 实施时应补充的规范清单

在对应功能真正开始时，从发布方核对并锁定适用版本：Intel/AMD 架构手册、System V AMD64 psABI、UEFI、ACPI、Linux x86_64 UAPI/syscall 表、所选 libc 的源码/测试、ext2 特性和格式、所选网络栈与编译器平台移植资料。

不把未经读取的第三方教程当成协议依据，不从网络文章直接复制带未知许可的内核实现。
