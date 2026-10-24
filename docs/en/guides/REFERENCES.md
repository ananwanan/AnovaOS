<!-- List primary reference materials and distinguish upstream specifications from project decisions. -->
# Official references and design sources

**English** | [简体中文](../../zh-CN/guides/REFERENCES.md)

Original reference review: 2026-10-21. This migration does not claim a new online
review. Upstream default branches change; implementation must lock the actual
version/commit. Project milestones and architecture are repository decisions.

| Primary reference | Purpose |
| --- | --- |
| [Rust x86_64-unknown-none](https://doc.rust-lang.org/rustc/platform-support/x86_64-unknown-none.html) | Bare-metal ELF, no std, code model, red-zone and SIMD limits |
| [Rust platform support](https://doc.rust-lang.org/rustc/platform-support.html) | Target versus host tools and Linux-musl support tiers |
| [Rust custom targets](https://doc.rust-lang.org/rustc/targets/custom.html) | Code-generation configuration and version constraints, not an automatic OS port |
| [Cargo build-std](https://doc.rust-lang.org/cargo/reference/unstable.html#build-std) | Experimental standard-library construction and toolchain requirements |
| [Limine boot protocol](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md) | Revisions, entry state, memory map, framebuffer and HHDM |
| [Limine upstream](https://github.com/limine-bootloader/limine) | Bootloader releases and supported architectures |
| [Linux ABI description](https://docs.kernel.org/admin-guide/abi.html) | User ABI stability and recorded interface scope |
| [Linux driver interface](https://docs.kernel.org/process/stable-api-nonsense.html) | User interfaces versus kernel-internal driver interfaces |
| [musl INSTALL](https://git.musl-libc.org/cgit/musl/tree/INSTALL) | Build environment and Linux syscall dependencies; recheck the chosen release |
| [GCC configuration](https://gcc.gnu.org/install/configure.html) | build/host/target, sysroot and configuration prerequisites |
| [GCC building](https://gcc.gnu.org/install/build.html) | Cross builds, runtimes and compiler bootstrap as separate workflows |
| [OASIS virtio specifications](https://docs.oasis-open.org/virtio/virtio/) | Released PCI, queue, block and network device semantics |

Exact CPU/ACPI references and the pinned Limine protocol for M2 are in
[M2 CPU](../milestones/M2_CPU.md#primary-specifications); no tutorial kernel
implementation was imported. Later source and graphics references are recorded
with their milestone contracts and third-party manifests.

When extending a subsystem, verify and lock applicable Intel/AMD manuals,
System V AMD64 psABI, UEFI, ACPI, Linux x86_64 UAPI/syscall tables, libc source/tests,
ext2 features/format and chosen network/compiler porting materials. Unread
third-party tutorials are not protocol evidence; do not copy implementation
code with unknown licensing.
