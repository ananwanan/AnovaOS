# 许可证与第三方内容

## 项目许可证

AnovaOS 的原创代码、文档、测试和示例采用 [Apache License, Version 2.0](../LICENSE)，SPDX 标识为 `Apache-2.0`。版权归相应作者所有，项目归属声明见 [NOTICE](../NOTICE)。单独标明其他许可证的内容按其声明处理。

选择它是为了允许学习、修改、商用和再分发，同时为贡献者与使用者建立明确的版权和专利许可规则。它允许在符合条件的情况下分发闭源衍生作品，不要求所有使用者公开自己的修改。许可授予范围、专利条款及其终止条件以完整英文许可证为准。

项目以该许可证下的条款提供，不保证适销性或特定用途适用性。此说明便于理解，不替代或修改根目录许可证。

## 使用和分发

分发原创内容及其衍生作品时，应按许可证要求：

- 向接收者提供 Apache-2.0 许可证副本。
- 保留适用的版权、专利、商标与归属声明。
- 对修改过的文件保留显著的修改说明。
- 在规定的位置保留适用的 NOTICE 归属内容。

许可证不授予项目名称、标志等商标的一般使用权。贡献规则采用 Apache-2.0 第 5 条：有意提交并纳入项目的贡献，除贡献者明确另作声明外，按相同条款提供；不额外引入 CLA 签署流程。

## 新建文件和 Cargo package

自有 Rust 源文件使用简短标记，例如：

```rust
// SPDX-License-Identifier: Apache-2.0
```

未来创建的自有 Cargo package 元数据应包含：

```toml
license = "Apache-2.0"
```

当前 workspace 的自有 package 已声明 Apache-2.0。导入第三方文件时保留原声明，不能把其标记批量替换为 Apache-2.0。

## 第三方边界

引导器、固件、crate、libc、编译器和工具包等均按各自许可证处理。记录来源、版本、补丁、版权声明、许可文本和分发要求；本项目 LICENSE 不会自动重新许可这些组件。

特别是未来分发 GCC/G++、运行库或其他有源码提供要求的工具时，应针对实际组件和分发方式保留所需材料、相应源码或有效提供方式，以及适用的运行库例外声明。不能由“系统采用 Apache-2.0”推断所有随附软件均适用同一条款。

应用可以选择自己的许可证；其使用的 SDK、库或其他组件仍需分别遵守相应条款。软件兼容层的实现也不等于取得第三方程序的分发许可。

## 官方依据

- [Apache-2.0 完整许可](https://www.apache.org/licenses/LICENSE-2.0.txt)
- [Apache 许可常见问题](https://www.apache.org/foundation/license-faq.html)

上述链接用于解释许可；仓库中的正式文本为根目录 [LICENSE](../LICENSE)。

## Boot implementation dependencies

M0/M1 uses limine 0.5.0 (MIT OR Apache-2.0) and its locked registry dependencies.
Host xtask dependencies retain their upstream licenses; Cargo.lock records exact
versions and archive hashes. Limine 9.5.0 binary inputs retain the upstream BSD
license, fetched with SHA-256 verification and included at boot/limine/LICENSE in
the ISO. Its additional component acknowledgments are available in the
[matching source release](https://github.com/Limine-Bootloader/Limine/blob/e05e09072f04aef0b58ec2c95600b679b767cce4/3RDPARTY.md).

OVMF is an external host firmware input, extracted from a pinned Ubuntu package
and excluded from the ISO. The extracted package retains its copyright and
license files under build/firmware/usr/share/doc/ovmf/. QEMU and xorriso are host
tools and are not embedded in the kernel or ISO.

## M8 guest software

The unmodified BusyBox 1.36.1 build remains GPL-2.0-only and is a separate guest
ELF, not kernel-linked code. Its exact source revision, configuration and rebuild
instructions are bundled as build/images/busybox-1.36.1-source.tar.gz alongside
the generated ISO. Preserve that corresponding-source bundle when distributing
the image. The ISO contains /etc/busybox.LICENSE and /etc/musl.LICENSE.
The tested musl 1.2.4 package notices are preserved in
[third-party/musl.md](third-party/musl.md). First-party syscall probes and the
compatibility adapter retain Apache-2.0; no Linux kernel implementation is imported.

## M9 runtime packaging

The shipped ld-musl-x86_64.so.1 is the unmodified musl 1.2.4-2 runtime with the
same MIT notice already included at /etc/musl.LICENSE. Its exact SHA-256 is
verified by xtask before packaging. The M9 C probes and TLS shared objects are
first-party Apache-2.0 sources. Dynamic linkage does not import musl or BusyBox
implementation code into the kernel. Preserve the existing BusyBox exact-source
bundle and musl notices when distributing the ISO.

## M10 network and TLS distribution

See [M10 input boundaries](third-party/m10.md) and the preserved
[network dependency licenses](third-party/network-licenses.md). The pinned
smoltcp BSD-0-Clause stack is kernel-linked; its enabled transitive notices are
included at /etc/network.LICENSE. Mbed TLS is unmodified and user-linked under
its Apache-2.0 option; /etc/mbedtls.LICENSE preserves its upstream license.
The generated demo package includes LICENSE and declares its native SDK/ABI.
Test CA private keys are never included in the image or application packages.


## Optional guest compiler profiles (M11/M12)

The compiler tools remain separate guest processes. M11's source-built GCC,
binutils, make, musl and runtime boundaries are recorded in
[third-party/m11](third-party/m11.md); the optional `/tc/licenses` bundle retains
their original texts and input/rebuild records.

M12 uses checksum-locked official musl Rust/Cargo/std binary distributions,
with embedded LLVM/rust-lld, rather than claiming a local Rust source build.
`/rs/licenses` retains LICENSE/COPYRIGHT and Cargo third-party notices. The
separate GCC unwinder's COPYING3 and COPYING.RUNTIME texts are also included,
even in the Rust-only runtime image. The real vendored itoa crate retains its
own upstream license files. See [third-party/m12](third-party/m12.md) for exact
inputs and boundaries; first-party launchers and fixtures remain Apache-2.0.
