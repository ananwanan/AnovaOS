<!-- Introduce AnovaOS, current accepted profiles, development commands and bilingual documentation. -->
# AnovaOS

**English** | [简体中文](README.zh-CN.md)

An x86_64 operating system built from scratch in Rust. Its goal is to edit,
compile, debug and run applications inside AnovaOS, while gradually supporting
selected existing Linux software.

Daily development and pull requests target **`dev`**; `main` holds accepted stable
snapshots. Repository: [ananwanan/AnovaOS](https://github.com/ananwanan/AnovaOS).

## Current status

The accepted local profiles cover **M0–M13, M14.1–M14.5 and M15.1–M15.6**.
M15.3/M15.4 deliver compiler feasibility assessments with explicit blockers;
they do not deliver reconstructed compilers. These are bounded results, not a
claim of a complete desktop OS or complete self-hosting.

| Milestone | Locally validated capability |
| --- | --- |
| M0–M7 | UEFI/Limine boot, memory protection, Ring 3, preemptive user scheduling, native apps, VFS/pipes/init/shell, modern virtio-blk, checked ext2 `/persist`, source editor, sync/fsync and clean shutdown/reboot retention |
| M8–M9 | Pinned BusyBox 1.36.1 CLI subset; musl 1.2.4 static, static PIE and dynamic ELF; pthread/TLS/futex, signal handlers and restart/interruption, poll/nonblocking I/O, four PTYs, startup shared libraries and dlopen/TLS |
| M10 | Opt-in polled virtio-net IPv4 TCP/UDP, bounded DNS, strict user TLS and checksum-locked package install/upgrade/rollback/removal with user data retained |
| M11 | Source-built guest GCC/G++ 11.2.0, binutils/make and matching musl/C++ sysroot; real guest edit/build/run, exceptions, threads and reboot retention |
| M12 | Official Rust 1.90.0/std/Cargo binary tools running in the guest; offline vendor/build.rs/C FFI/procedural macros; strict online registry/Git and offline cache rebuilds; pinned public sources tested through a configured host proxy |
| M13 | SDK 1.0.0, native C/C++/Rust templates, ANOVAPK2 ABI-bound packages, installation policy and source symbolization of actual user faults |
| M14.1–M14.4 | Checked CPU framebuffer, polled PS/2 input/cursor, isolated Rust user compositor, versioned `anova.gui.v1` C/C++/Rust SDK, independently guest-built clients, note and bounded graphical terminal |
| M14.5 | Hardware VirGL offscreen clear/readback, guarded pixel comparison, timeout/reset/reinitialization and CPU fallback; ordinary desktop composition remains software-rendered |
| M15.1–M15.2 | Measured capacity, locked bzip2 source reconstruction inside the guest, exact fixture comparison and failure/reboot recovery |
| M15.3–M15.4 | Independent GCC and Rust/LLVM/Cargo source audits and real guest prerequisite probes; compiler reconstruction remains blocked |
| M15.5 | Actual guest kernel compilation with inherited tools/target libraries, exact retained-ELF host-assisted packaging and verified boot identity/recovery |
| M15.6 | Two independent clean builds each of bzip2 and kernel, identical final artifacts, archived evidence and all 12 boot/rejection/recovery sessions |

Evidence is recorded per stage in the [validation index](docs/en/index.md#validation).
This documentation update does not rerun those guest gates. Configured CI,
actual remote CI execution and local acceptance are separate facts.

The current system uses one BSP, 32 process/thread slots, a non-preemptible
kernel, a fixed 16 MiB heap and masked peripheral IRQs with polled device I/O.
Persistent files are bounded at 4 MiB; tmpfs and readonly shared-file caches
remain at 1 MiB. Ext2 has no journal or power-loss recovery. General Linux/glibc,
SMP, USB/physical hardware support, native Rust `std`, reconstructed compilers
and guest-only boot-image generation remain unverified. See
[status](docs/en/guides/STATUS.md) and [compatibility](docs/en/COMPATIBILITY.md).

## Next development work

1. Resolve the measured compiler prerequisites: guest build utilities,
   GMP/MPFR/MPC development inputs, Rust's prescribed stage0/offline dependency
   closure and sufficient memory/storage. Reassess GCC, LLVM, rustc and Cargo
   independently before claiming a reconstructed stage.
2. Extend M15.5's exact guest-built kernel handoff toward guest-only image
   generation; current ISO preparation and packaging still run on the host.
3. Keep M15.6's clean-build, rejection and recovery gates as regression coverage.
   Scope native `std`, broader Cargo/GUI APIs and real hardware as independently
   tested follow-up work.

These priorities follow the [roadmap](docs/en/ROADMAP.md),
[M15 GCC](docs/en/milestones/M15_GCC.md),
[M15 Rust](docs/en/milestones/M15_RUST.md) and
[M15 repeat](docs/en/milestones/M15_REPEAT.md).

## Build and boot

Use x86_64 Ubuntu 24.04 or WSL2 with rustup, QEMU 8.2.2, xorriso, curl,
dpkg-deb, GCC/G++, make, musl-tools, QEMU-user, Python 3 and e2fsprogs.
The repository pins Rust 1.90.0, rustfmt, Clippy and `x86_64-unknown-none`.
The [development guide](docs/en/guides/DEVELOPMENT.md) gives exact setup,
optional compiler dependencies, output paths and failure behavior.

```sh
git clone --branch dev https://github.com/ananwanan/AnovaOS.git
cd AnovaOS
cargo xtask fetch
cargo xtask doctor
cargo xtask build
cargo xtask apps
cargo xtask image
cargo xtask run
```

Default QEMU runs use a temporary data disk. To retain work, create an explicit
disk and use guest `shutdown`:

```sh
cargo xtask root-image build/disks/work.ext2
cargo xtask run --disk build/disks/work.ext2
```

All images, caches and logs stay under `build/`; commands operate on ordinary
image files. The kernel uses `no_std` + `alloc`, UEFI/Limine, four-level paging
and separate kernel/user compilation flags. The native ABI and Linux-musl
personality are explicit and independent.

## Development profiles and checks

| Profile | Commands and scope |
| --- | --- |
| Core | `cargo xtask test-host`, `verify`, `terminal-test`, `persistence-test`, `test-compat`, `trace-compat` |
| Network/packages | `cargo xtask network-test`, `package-test`; interactive `cargo xtask run --network` |
| Guest C/C++ | `cargo xtask toolchain-build`, `toolchain-image`, `toolchain-test`; `run --toolchain` with a Linux-personality disk |
| Guest Rust/Cargo | `cargo xtask rust-build`, `rust-image`, `rust-cross-test`, `rust-runtime-test`, `rust-test`, `rust-online-test`, `rust-public-test`; `run --rust` |
| Native SDK | `cargo xtask sdk`, `app-new <c\|cpp\|rust> PROJECT`, `app-build PROJECT`, `app-pack MANIFEST OUTPUT`, `app-image PACKAGE SHA256`, `app-run PACKAGE SHA256`, `symbolize ELF ADDRESS...`; `sdk-test`, `app-test`, `package-v2-test`, `diagnostics-test` |
| Display/input/compositor | `cargo xtask graphics-test`, `input-test`, `gui-test`; interactive `run --graphics` or `run --gui` |
| GUI runtime | `cargo xtask gui-sdk`, `gui-sdk-image`, `gui-sdk-test`, `gui-sdk-replay`; `run --gui-sdk` with a native disk |
| GPU | `cargo xtask gpu-test` requires a verified hardware GL renderer; `gpu-fallback-test` is headless |
| Source reconstruction | `cargo xtask bootstrap-prepare`, `bootstrap-test`, `bootstrap-gcc-test`, `bootstrap-rust-test`, `bootstrap-kernel-test`, `bootstrap-repeat-test` |

Image-building gates share verified output snapshots: **run them sequentially**.
Interactive graphics require a working display; headless QMP acceptance is a
separate profile. Guest application compilation runs on the AnovaOS kernel;
installed seed tools alone do not establish compiler source reconstruction.

## Documentation

Full matching document trees are available in [English](docs/en/index.md) and
[简体中文](docs/zh-CN/index.md). Each page links to its translation. Shared images
and diagrams belong under `docs/assets/` when added.

| Entry | Contents |
| --- | --- |
| [AGENTS.md](AGENTS.md) | Mandatory development, safety and delivery rules |
| [Project outline](docs/en/PROJECT_OUTLINE.md), [roadmap](docs/en/ROADMAP.md) | Goals, dependencies, acceptance and remaining work |
| [Architecture](docs/en/ARCHITECTURE.md), [compatibility](docs/en/COMPATIBILITY.md) | Subsystem ownership, ABIs, limits and tested software |
| [Guides](docs/en/index.md#guides) | Setup, style, toolchains, licensing, requirements, status and file inventory |
| [Milestones](docs/en/index.md#milestone-designs), [validation](docs/en/index.md#validation) | Implemented contracts and historical execution evidence through M15.6 |
| [Third-party inputs](docs/en/index.md#third-party-inputs-and-notices) | Provenance and preserved upstream license texts |
| [Native SDK](sdk/README.md), [GUI SDK](sdk/gui/README.md), [bootstrap recipes](ports/bootstrap/README.md) | Adjacent development entry points, each with a Chinese companion |

Update affected documentation **in the same development batch**, including both
languages, both READMEs, status, roadmap, commands and validation boundaries.
First-party identifiers, source comments, logs and commit messages remain English.
Every first-party file requires a purpose explanation and inventory entry.

```sh
python3 scripts/check_docs.py
cargo xtask check-files
cargo fmt --all -- --check
cargo fmt --manifest-path user/Cargo.toml --all -- --check
git diff --check
```

Original code, documentation and examples use [Apache-2.0](LICENSE);
[NOTICE](NOTICE) and third-party licenses retain their own attribution and terms.
See [licensing](docs/en/guides/LICENSING.md).
