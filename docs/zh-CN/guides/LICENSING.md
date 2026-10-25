<!-- Translate project licensing, contributions and third-party distribution boundaries into Simplified Chinese. -->
# 许可证与第三方内容

[English](../../en/guides/LICENSING.md) | **简体中文**

## 项目许可证

AnovaOS 的原创代码、文档、测试和示例采用 [Apache License, Version 2.0](../../../LICENSE)，
SPDX 标识为 `Apache-2.0`。版权归相应作者，项目归属声明见 [NOTICE](../../../NOTICE)。
单独标明其他许可证的内容继续按其声明处理。

Apache-2.0 允许学习、修改、商用和再分发，在版权和专利许可条件下允许闭源衍生作品，
不要求所有使用者公开修改。专利授予及终止条件以完整英文许可证为准。
项目不保证适销性或特定用途适用性；此指南用于解释，不能替代许可证。

## 使用、分发与贡献

分发原创内容及衍生作品时，应遵守许可证：

- 向接收者提供 Apache-2.0 副本。
- 保留适用的版权、专利、商标与归属声明。
- 显著说明文件已经修改。
- 在规定位置保留适用的 NOTICE 归属内容。

许可证不授予项目名称、标志的一般商标使用权。有意提交并纳入项目的贡献按 Apache-2.0
第 5 条处理，除非贡献者另作明确声明；不要求额外签署 CLA。

新建自有 Rust 源文件使用 `// SPDX-License-Identifier: Apache-2.0`；自有 Cargo package
声明 `license = "Apache-2.0"`，当前 workspace 已如此设置。不得将第三方声明批量替换为
项目 SPDX 标识。

引导器、固件、crate、libc、编译器和工具分别保留原许可证；记录来源、版本、补丁、声明、
完整许可文本和实际分发要求。应用可选择自己的许可证，同时遵守 SDK/库条款。
ABI 兼容不等于取得第三方软件分发权。GCC/运行库分发应保留相应源码材料、适用提供方式和
运行库例外；项目许可证不能为随附软件重新许可。

官方依据：[Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0.txt)、
[Apache 许可常见问题](https://www.apache.org/foundation/license-faq.html)。
仓库正式文本为根目录 [LICENSE](../../../LICENSE)。

## 引导依赖

M0/M1 使用 limine 0.5.0（MIT OR Apache-2.0）及锁定 registry 依赖；宿主 xtask 依赖保留
上游条款，Cargo.lock 固定版本/校验和。Limine 9.5.0 二进制输入保留 BSD 许可，校验 SHA-256
后打包至 ISO 的 `boot/limine/LICENSE`，另保留
[对应组件声明](https://github.com/Limine-Bootloader/Limine/blob/e05e09072f04aef0b58ec2c95600b679b767cce4/3RDPARTY.md)。

OVMF 从固定 Ubuntu 包提取，属于外部宿主固件，不包含在 ISO；声明保留在
`build/firmware/usr/share/doc/ovmf/`。QEMU、xorriso 为宿主工具，不属于内核或 ISO。

## M8/M9 用户态运行库

未修改的 BusyBox 1.36.1 采用 GPL-2.0-only，作为独立 guest ELF 运行。确切源码修订、配置和
重建步骤打包在 `build/images/busybox-1.36.1-source.tar.gz`；分发镜像时须保留该对应源码包。
ISO 包含 `/etc/busybox.LICENSE` 与 `/etc/musl.LICENSE`，受测 musl 1.2.4 包的原声明见
[musl 声明](../third-party/musl.md)。

M9 的 `ld-musl-x86_64.so.1` 是未修改的 musl 1.2.4-2，xtask 在打包前校验确切 SHA-256，
并沿用 MIT 声明。C 探针和 TLS 共享对象测试为自有 Apache-2.0。动态链接不会将 musl、
BusyBox 实现引入内核；兼容适配器也没有导入 Linux 内核实现。

## M10 网络与 TLS

见 [M10 输入](../third-party/m10.md)与[网络依赖许可证](../third-party/network-licenses.md)。
内核链接的 smoltcp 为 BSD-0-Clause，启用的传递依赖声明进入 `/etc/network.LICENSE`。
未修改的用户态 Mbed TLS 选择 Apache-2.0，保留 `/etc/mbedtls.LICENSE`。
演示包包含 LICENSE 和原生 SDK/ABI 声明，测试 CA 私钥不得进入镜像或包。

## M11/M12 可选编译器配置

源码构建的 GCC/binutils/make/musl 均为独立 guest 进程；[M11 输入](../third-party/m11.md)
和 `/tc/licenses` 保留原条款及输入/重建记录。种子运行库分发另有源码义务，本地开发镜像
不代表可公开发布的源码完备工具链。

M12 使用锁定校验和的官方 musl Rust/Cargo/std 二进制，包含 LLVM/rust-lld，不能声称本地
从源码重建 Rust。`/rs/licenses` 保留 LICENSE、COPYRIGHT、Cargo 声明；即使 Rust-only 镜像
也保留独立 GCC unwinder 的 COPYING3、COPYING.RUNTIME。vendor 中的 itoa 保留原声明。
Mozilla/curl 公共根证书库保留 MPL-2.0 和归属信息。详见 [M12 输入](../third-party/m12.md)。

## M15 重建源码和种子

[M15 输入](../third-party/m15.md)记录 bzip2、编译器源码评估、内核 registry 源码和继承的裸机
Rust 库。保留其原声明、源码/种子清单及配方；系统内重建的 bzip2 不能改用项目许可证。
源码审计不等于重建编译器；M15.5/M15.6 内核仍依赖继承工具/库与宿主打包。
自有编排和诊断代码继续采用 Apache-2.0。
