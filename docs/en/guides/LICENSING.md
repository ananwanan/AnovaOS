<!-- Explain project licensing, contribution terms and preserved third-party distribution boundaries. -->
# Licensing and third-party content

**English** | [简体中文](../../zh-CN/guides/LICENSING.md)

## Project license

Original AnovaOS code, documentation, tests and examples use
[Apache License, Version 2.0](../../../LICENSE), SPDX `Apache-2.0`.
Copyright belongs to the respective authors; project attribution is in
[NOTICE](../../../NOTICE). Separately identified licenses remain applicable.

Apache-2.0 allows study, modification, commercial use and redistribution under
its copyright and patent terms, including closed-source derivatives subject to
its conditions. It does not require every user to publish modifications. The
complete English license governs patent grants and termination. The project
provides no warranty of merchantability or fitness for a particular purpose;
this guide explains rather than replaces the license.

## Use, distribution and contributions

When distributing original content or derivatives, follow the license:

- Give recipients a copy of Apache-2.0.
- Retain applicable copyright, patent, trademark and attribution notices.
- Prominently identify modified files as changed.
- Retain applicable NOTICE attribution in the locations required by the license.

The license does not grant general trademark rights to the project name or logo.
Contributions intentionally submitted for inclusion follow Apache-2.0 section 5
unless the contributor explicitly states otherwise; no separate CLA is required.

New first-party Rust sources use `// SPDX-License-Identifier: Apache-2.0`.
First-party Cargo packages declare `license = "Apache-2.0"` as the current
workspace already does. Never replace imported third-party notices with the
project's SPDX label.

Bootloaders, firmware, crates, libc, compilers and tools retain their own licenses.
Record source, version, patches, notices, complete license texts and applicable
redistribution requirements. Applications may choose their own license while
respecting SDK/library terms. ABI compatibility does not confer distribution
rights to third-party software. GCC/runtime redistribution must preserve actual
corresponding-source materials or applicable offers and runtime exceptions;
the project's license does not relicense bundled software.

Official references: [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0.txt)
and [Apache license FAQ](https://www.apache.org/foundation/license-faq.html).
The repository's authoritative text is [LICENSE](../../../LICENSE).

## Boot inputs

M0/M1 uses limine 0.5.0 (MIT OR Apache-2.0) and locked registry dependencies.
Host xtask dependencies keep their licenses; Cargo.lock fixes versions/hashes.
Limine 9.5.0 binary inputs retain the upstream BSD license, verified by SHA-256
and included at `boot/limine/LICENSE` in the ISO. Preserve its
[matching component acknowledgments](https://github.com/Limine-Bootloader/Limine/blob/e05e09072f04aef0b58ec2c95600b679b767cce4/3RDPARTY.md).

OVMF is extracted from a pinned Ubuntu package, is external host firmware and
is excluded from the ISO. Notices remain under
`build/firmware/usr/share/doc/ovmf/`. QEMU and xorriso are host tools, not kernel
or ISO components.

## M8/M9 guest runtime

Unmodified BusyBox 1.36.1 is GPL-2.0-only and runs as a separate guest ELF.
Its exact revision, configuration and rebuild instructions are bundled in
`build/images/busybox-1.36.1-source.tar.gz`; retain that corresponding-source
bundle with image redistribution. The ISO contains `/etc/busybox.LICENSE`
and `/etc/musl.LICENSE`. The tested musl 1.2.4 package's original notices are
in [musl notices](../third-party/musl.md).

The M9 `ld-musl-x86_64.so.1` is the unmodified musl 1.2.4-2 runtime, whose
exact SHA-256 xtask checks before packaging. It shares the retained MIT notice.
C probes and TLS shared-object fixtures are first-party Apache-2.0. Dynamic
linkage imports neither musl nor BusyBox implementation into the kernel; no
Linux kernel implementation is imported by the compatibility adapter.

## M10 network and TLS

See [M10 inputs](../third-party/m10.md) and
[network licenses](../third-party/network-licenses.md). The kernel-linked
smoltcp stack is BSD-0-Clause; enabled transitive notices enter
`/etc/network.LICENSE`. Unmodified user-linked Mbed TLS uses its Apache-2.0
option with `/etc/mbedtls.LICENSE`. The demo package includes LICENSE and its
native SDK/ABI declaration. Test CA private keys never enter images or packages.

## M11/M12 optional compiler profiles

The source-built GCC/binutils/make/musl tools remain separate guest processes;
[M11 inputs](../third-party/m11.md) and `/tc/licenses` retain original terms and
input/rebuild records. Bootstrap runtime redistribution has its own source
obligations; the local development image is not a source-complete public release.

M12 uses checksum-locked official musl Rust/Cargo/std binaries with LLVM/rust-lld,
not a locally source-built Rust toolchain. `/rs/licenses` retains LICENSE,
COPYRIGHT and Cargo notices. The separate GCC unwinder's COPYING3 and
COPYING.RUNTIME are retained even in Rust-only runtime images. Vendored itoa
keeps its own notices. The Mozilla/curl public root store retains its MPL-2.0
text and attribution. See [M12 inputs](../third-party/m12.md).

## M15 reconstruction sources and seeds

[M15 inputs](../third-party/m15.md) records bzip2, the compiler source assessments,
kernel registry sources and inherited bare-metal Rust libraries. Preserve their
original notices, source/seed inventories and recipes. Guest-built bzip2 is not
relicensed as project code. Compiler audits do not reconstruct compilers; the
M15.5/M15.6 kernel uses inherited tools/libraries and host packaging. First-party
orchestration and diagnostics remain Apache-2.0.
