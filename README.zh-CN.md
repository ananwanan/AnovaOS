# AnovaOS

[English](README.md) | **简体中文**

用 Rust 从零开发的 x86_64 操作系统，目标是在自己的系统中编辑、编译、调试和运行应用，并逐步兼容部分现有 Linux 软件。

**当前状态：仅完成项目规划和 Codex 工作规范；尚无内核、启动镜像或已验证的软件兼容能力。**

项目仓库：[ananwanan/AnovaOS](https://github.com/ananwanan/AnovaOS)。

日常开发与 Pull Request 的目标分支为 **`dev`**；`main` 保留阶段验收后的稳定快照。分支约定见 [GitHub 协作说明](docs/GITHUB_SETUP.md)。

## 项目目标

1. Rust 自研内核，采用 x86_64、UEFI、Limine，先在 QEMU/OVMF 上建立稳定基线。
2. 实现内存管理、中断、进程隔离、抢占调度、文件系统、终端与网络。
3. 提供应用 SDK，让宿主机交叉编译的 Rust/C/C++ 程序以用户进程运行。
4. 按程序和版本逐个验证 Linux x86_64 ABI：先静态 musl 命令行程序，再动态链接和较复杂的软件。
5. 在 AnovaOS 中运行 GCC/G++、rustc 和 Cargo，完成应用开发闭环。
6. 在开发闭环稳定后，推进窗口系统、图形应用、多核和更多实机驱动。

这里的 Rust 主要指内核及自有系统组件。复用的引导器、libc、编译器等可以使用 C/C++ 等语言；不以重写成熟工具作为前置条件。

## 技术基线

| 项目 | 初始选择 | 边界 |
| --- | --- | --- |
| 架构 | x86_64，4 级页表，先单核 | 首版不承担 32 位/x32 ABI |
| 启动 | UEFI + Limine，内核 ELF | Secure Boot 签名链后置 |
| 内核 | 模块化单体内核，Rust `no_std` + `alloc` | 初期 `panic=abort`，少量必要汇编 |
| 编译目标 | 内核使用 `x86_64-unknown-none` | 用户程序使用独立目标和链接配置 |
| 参考平台 | QEMU q35、OVMF、串口、现代 virtio PCI | TCG 必须可用，KVM 可选 |
| 用户态 | ELF64、System V AMD64 函数调用约定 | native ABI 与 Linux ABI 独立建档 |
| 软件复用 | Linux ABI 兼容层 + 固定版本 musl 用户态 | 不承诺通用 Linux 发行版兼容 |
| 持久存储 | initramfs/tmpfs → virtio-blk + 受限 ext2 | ESP/FAT 与根文件系统分开 |
| 第一开发环境 | Linux/WSL2 交叉开发，后续系统内编译 | 图形桌面不是编译能力前提 |

选型属于项目设计决定；实施时须固定实际验证过的版本、提交和校验和。官方依据见 [参考资料](docs/REFERENCES.md)。

## 文档入口

详细规划文档当前以简体中文编写，开发语言与格式规范使用英文。

| 文件 | 内容 |
| --- | --- |
| [PROJECT_OUTLINE.md](PROJECT_OUTLINE.md) | 总体大纲、交付等级、范围与风险 |
| [AGENTS.md](AGENTS.md) | Codex 的约束、执行顺序、代码规范、验收要求 |
| [ROADMAP.md](ROADMAP.md) | M0–M15 的前置依赖、实现任务和退出条件 |
| [ARCHITECTURE.md](ARCHITECTURE.md) | 内核、应用、ABI、驱动和目录边界 |
| [COMPATIBILITY.md](COMPATIBILITY.md) | 现有软件兼容策略、ABI 需求和验证矩阵 |
| [docs/TOOLCHAINS.md](docs/TOOLCHAINS.md) | GCC/G++/Rust 从交叉编译到系统内开发 |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | 宿主环境、计划命令、验证与版本记录 |
| [docs/CODING_STYLE.md](docs/CODING_STYLE.md) | 英文开发语言约定与 Rust 格式规范 |
| [docs/FIRST_TASK.md](docs/FIRST_TASK.md) | 可直接交给 Codex 的第一批实施任务 |
| [docs/STATUS.md](docs/STATUS.md) | 当前已实现能力和下一步 |
| [docs/GITHUB_SETUP.md](docs/GITHUB_SETUP.md) | 仓库克隆、文档维护与提交约定 |
| [docs/LICENSING.md](docs/LICENSING.md) | Apache-2.0 选择、使用与第三方许可边界 |

## 开发约定

自有代码的标识符、注释、Rustdoc、安全说明、日志、错误消息、CLI 帮助、测试、提交消息、PR 标题/描述和新开发文档统一使用英文。明确的翻译与本地化资源可以使用目标语言；第三方文本及现有中文规划文档按 [代码规范](docs/CODING_STYLE.md) 保留相应语言。

Rust 格式由根目录 [rustfmt.toml](rustfmt.toml) 定义：4 空格缩进、100 列建议宽度、LF 换行、Rust 2024 parser/style edition，仅使用稳定版 rustfmt 选项。建立 Cargo workspace 后，在仓库根目录执行：

```sh
cargo fmt --all
cargo fmt --all -- --check
```

这些命令需要真实的 `Cargo.toml`，当前仓库尚无此文件。共享项目信息变更时，同步维护默认英文 [README.md](README.md) 与本中文版。

## 怎样才算“可以在系统上开发 app”

在一次 AnovaOS 启动期间，从系统自己的终端和持久磁盘完成以下工作，执行期间不把编译请求转发给宿主机：

- 编辑并保存 `main.c`、`main.cpp`、`main.rs`。
- 使用系统中运行的 `gcc`、`g++`、`rustc` 编译、链接并运行产物。
- 使用 `cargo build --offline` 构建带已准备依赖的小型项目，运行应用并核对输出。
- 应用能够读写文件、处理命令行参数、报告错误；重启后源码与产物保留。
- 后续再验证联网获取依赖、完整调试器、复杂 Cargo 构建与编译器自举。

可以先通过 Linux 兼容层运行这些工具链；这仍是应用在自研内核上的系统内开发。AnovaOS 专用 target 和原生 `std` 移植另设阶段，避免混淆进度。

## 下一步

从 [第一批任务](docs/FIRST_TASK.md) 实现 M0 与 M1：工程入口、锁定工具链、QEMU 启动、串口日志、可判定失败的冒烟验证。

目前文档中的 `cargo xtask ...` 是后续需要实现的接口，不能当作现有命令运行。当前没有 `Cargo.toml`，也没有 CI 工作流。

## 许可证

本项目的原创代码、文档和示例采用 **[Apache License 2.0](LICENSE)**，SPDX 标识为 `Apache-2.0`。Copyright 2026 ananwanan and the AnovaOS contributors.

该许可证允许商用、修改和分发，也允许在遵守许可条件的前提下发布闭源衍生作品，并包含贡献者专利授权。分发时须履行许可证、归属声明与修改标记等要求；详见 [许可说明](docs/LICENSING.md) 与 [NOTICE](NOTICE)。

第三方引导器、libc、编译器、依赖库与导入代码仍适用各自许可证，不因放入本仓库或系统镜像而改为 Apache-2.0。
