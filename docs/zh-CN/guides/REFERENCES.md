<!-- Translate the primary reference catalog and its version/provenance boundaries into Simplified Chinese. -->
# 官方参考与设计依据

[English](../../en/guides/REFERENCES.md) | **简体中文**

原始查阅日期：2026-10-21。本次迁移不代表重新在线核查；默认分支内容会变化，实施时须锁定
实际版本/提交。项目阶段和架构由本仓库自行决定。

| 官方资料 | 用途 |
| --- | --- |
| [Rust x86_64-unknown-none](https://doc.rust-lang.org/rustc/platform-support/x86_64-unknown-none.html) | 裸机 ELF、无 std、code model、red zone 与 SIMD 限制 |
| [Rust 平台支持](https://doc.rust-lang.org/rustc/platform-support.html) | target/host tools 区别、Linux-musl 支持等级 |
| [Rust 自定义目标](https://doc.rust-lang.org/rustc/targets/custom.html) | 代码生成配置和版本约束，不等于自动完成 OS 移植 |
| [Cargo build-std](https://doc.rust-lang.org/cargo/reference/unstable.html#build-std) | 实验性标准库构建和工具链要求 |
| [Limine 引导协议](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md) | 协议修订、入口状态、内存图、framebuffer、HHDM |
| [Limine 上游](https://github.com/limine-bootloader/limine) | 引导器版本与支持架构 |
| [Linux ABI 说明](https://docs.kernel.org/admin-guide/abi.html) | 用户 ABI 稳定性和接口记录范围 |
| [Linux 驱动接口](https://docs.kernel.org/process/stable-api-nonsense.html) | 用户接口和内核内部驱动接口的区别 |
| [musl INSTALL](https://git.musl-libc.org/cgit/musl/tree/INSTALL) | 构建环境、Linux syscall 依赖；实施时复核所选版本 |
| [GCC 配置](https://gcc.gnu.org/install/configure.html) | build/host/target、sysroot 与配置前提 |
| [GCC 构建](https://gcc.gnu.org/install/build.html) | 交叉构建、运行库和编译器自举是不同流程 |
| [OASIS virtio 规范](https://docs.oasis-open.org/virtio/virtio/) | 正式 PCI、队列、block/net 设备语义 |

M2 使用的确切 CPU/ACPI 依据和固定 Limine 协议见 [M2 CPU](../milestones/M2_CPU.md#primary-specifications)，
没有导入教程内核实现。后续源码和图形依据保留于对应阶段契约与第三方清单。

扩展子系统时，核对并锁定适用的 Intel/AMD 手册、System V AMD64 psABI、UEFI、ACPI、
Linux x86_64 UAPI/syscall 表、libc 源码/测试、ext2 特性/格式，以及所选网络栈和编译器移植资料。
未读过的第三方教程不能作为协议证据；不得复制许可不明的实现代码。
