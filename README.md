<!-- Introduce AnovaOS, current accepted profiles, development commands and bilingual documentation. -->
# AnovaOS

**English** | [简体中文](README.zh-CN.md)

An x86_64 operating system built from scratch in Rust. Its goal is to edit,
compile, debug and run applications inside AnovaOS, while gradually supporting
selected existing Linux software.

Daily development is based on **`dev`**, and pull requests target it; `main` holds accepted stable
snapshots. Repository: [ananwanan/AnovaOS](https://github.com/ananwanan/AnovaOS).

All remaining M16 development continues on **`feat/m16-foundation`**, with pull
requests targeting `dev`.


GitHub Actions temporarily run **only on pushes to `main`**. PR, manual and other-branch
triggers are disabled. The latest feature-branch CI failure and locally verified
fixture correction are recorded separately from local capacity acceptance.

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
| M11 | Host-built GCC/G++ 11.2.0, binutils/make and matching musl/C++ sysroot for the guest; real guest edit/build/run, exceptions, threads and reboot retention |
| M12 | Official Rust 1.90.0/std/Cargo binary tools running in the guest; offline vendor/build.rs/C FFI/procedural macros; strict online registry/Git and offline cache rebuilds; pinned public sources tested through a configured host proxy |
| M13 | SDK 1.0.0, native C/C++/Rust templates, ANOVAPK2 ABI-bound packages, installation policy and source symbolization of actual user faults |
| M14.1–M14.4 | Checked CPU framebuffer, polled PS/2 input/cursor, isolated Rust user compositor, versioned `anova.gui.v1` C/C++/Rust SDK, independently guest-built clients, note and bounded graphical terminal |
| M14.5 | Hardware VirGL offscreen clear/readback, guarded pixel comparison, timeout/reset/reinitialization and CPU fallback; ordinary desktop composition remains software-rendered |
| M15.1–M15.2 | Measured capacity, locked bzip2 source reconstruction inside the guest, exact fixture comparison and failure/reboot recovery |
| M15.3–M15.4 | Independent GCC and Rust/LLVM/Cargo source audits and real guest prerequisite probes recorded the blockers at that milestone |
| M15.5 | Actual guest kernel compilation with inherited tools/target libraries, exact retained-ELF host-assisted packaging and verified boot identity/recovery |
| M15.6 | Two independent clean builds each of bzip2 and kernel, identical final artifacts, archived evidence and all 12 boot/rejection/recovery sessions |
| M16 (bounded local profile) | Verified inputs, backend/cache isolation, controlled ext4 readonly/writes and JBD2 recovery; fresh-disk bzip2/UART acceptance and actual kernel reconstruction after heap/VFS/fd/task/disk exhaustion; remote CI tracked separately |
| M17.1 | Supervised desktop, simultaneous C/C++/Rust input/windows, actual client/compositor faults, bounded restart, resource reuse and five persistence/fallback boots |
| M17.2 | Real Linux-musl PTY, bounded ANSI/history, Ctrl+C/resize, actual GCC compilation, exhaustion cleanup and exact reboot bytes |
| M17.3 | File browsing, bounded source editing and protected saves on the PTY work disk; real GUI size/inode/disk-full/write/flush failures, recovery copies, retry and clean reboot accepted locally |
| M17.4 | Bounded build diagnostics, current resource and task monitoring, scoped stops and live compositor recovery |
| M17.5 | Fixed graphical C, C++ and Rust workflows, concurrent interaction, cancellation, and publication/reboot recovery verified locally in QEMU |
| M18.1 locally accepted | GCC/G++ 11.2.0 built from source in the guest compiled, linked and ran C and C++ programs. A separate reboot verified the compiler and seed bytes. [Build evidence](docs/en/validation/M18.1.md) |
| M18.2 standalone LLVM component | Two guest builds of `LLVMDemangle` from the locked Rust LLVM fork produced exact library and linked fixture bytes. Compile/link failure and reboot recovery passed. |
| M18.3 base optical image | AnovaOS generated two exact UEFI/Limine ISOs from a retained guest-built kernel. Independent extraction, actual boot, publication failure, cancellation and full-disk recovery passed locally. |

Evidence is recorded per stage in the [validation index](docs/en/index.md#validation).
Each record identifies its executed checks. Configured CI, actual remote CI execution
and local acceptance are separate facts.

The early-boot I/O APIC path tolerates firmware/register ID differences while
keeping all external interrupts masked and checking every mask readback.
`cargo xtask smoke --case ioapic-id-mismatch` reproduces the reported 128/0
case using a parsed-MADT test injection and real QEMU registers, and is included
in `verify`; see the dated
[boot-fix evidence](docs/en/validation/M2.md).

The current system uses one BSP, 32 process/thread slots, a non-preemptible
kernel, a fixed 16 MiB heap and masked peripheral IRQs with polled device I/O.
Writable ext2 files are bounded at 4 MiB; controlled ext4 files allow 256 MiB; tmpfs and readonly shared-file caches
remain at 1 MiB. Ext2 has no journal or power-loss recovery. General Linux/glibc,
SMP, USB/physical hardware support, native Rust `std` and reconstructed compilers
remain unverified. The guest-generated UEFI optical image has bounded non-GUI
acceptance. See [M18.3](docs/en/validation/M18.3.md),
[status](docs/en/guides/STATUS.md) and [compatibility](docs/en/COMPATIBILITY.md).

`foundation-capacity-test` passed locally on a fresh 64 MiB ext4 work disk after
real resource exhaustion/recovery. The actual kernel build used 9,494,480 heap bytes,
1,564 VFS slots, 14 tasks, 16 FDs/process and 6,072 allocated 4 KiB blocks at peak.
[M16 capacity](docs/en/validation/M16_CAPACITY.md) retains complete results and CI status.

## Next development work

The owner-approved order is **M16 resources/VFS/ext4 → M17 development desktop →
M18 conditional compiler and image self-hosting**. M16 now has a locally verified backend/cache and
controlled ext4 readonly/writes, JBD2 recovery and opt-in migration/build acceptance.
M17.1–M17.5 have local QEMU evidence for their declared desktop profiles,
including live resource/task controls and the ten-boot compiler workflow.
M18 has started on `feat/m18-bootstrap`. Remote CI and physical hardware remain independent.

1. [M16 foundation](docs/en/milestones/M16_FOUNDATION.md): remote CI acceptance is paused by owner request; the declared kernel capacity profile passed locally.
   Broader workload sizing remains independent. JBD2 and fresh-image migration now have separate
   [M16.5](docs/en/validation/M16.5.md)/[M16.6](docs/en/validation/M16.6.md) local gates. Backend
   isolation, on-demand directories, reclaimable file nodes and controlled ext4 readonly
   access are implemented; see [local evidence](docs/en/validation/M16.md) and [M16.4 writes](docs/en/validation/M16.4.md).
2. [M17 desktop](docs/en/milestones/M17_DESKTOP.md): use the monitor and the fixed C/C++/Rust recipes, then verify concurrent interaction and fault recovery.
3. [M18 reconstruction](docs/en/milestones/M18_BOOTSTRAP.md) uses
   `cargo xtask m18-gcc-test` for the bounded GCC build-entry profile. Continue with the
   measured C/C++ source closure. `cargo xtask m18-llvm-test` verifies the standalone
   LLVM demangler. Full LLVM, rustc and Cargo keep separate acceptance. The guest
   boot-image gate `cargo xtask m18-image-test REPORT GUEST_ELF` accepts a verified retained
   guest-built kernel and inherited tools. GUI-enabled construction remains separate.

Preserve prior runtime, storage, GUI and M15 reconstruction gates. Native `std`,
SMP, device IRQ/async I/O, physical hardware and a new filesystem format remain
separate work. [STATUS](docs/en/guides/STATUS.md) records the observed failed
toolchain dependency download separately from local acceptance.

The owner deferred M16 remote closeout and authorized M17 development on `feat/m17-desktop`. `cargo xtask desktop-build` builds the launcher and existing host-built C/C++/Rust clients, plus an explicit Linux boundary fixture; `cargo xtask run --desktop [--disk build/disks/work.ext4]` selects the opt-in session. `cargo xtask desktop-test` verifies the accepted bounded profile. See [M17.1 local acceptance](docs/en/validation/M17.1.md). M17.2 adds the real PTY terminal: focus the launcher and press `t`; use `F5` for a new shell and `F4` to close. `cargo xtask run --desktop --toolchain --disk build/disks/pty-work.ext4` adds the verified M11 tools to an explicitly Linux-personality work disk. See [PTY contract](docs/en/milestones/M17_PTY.md) and [M17.2 evidence](docs/en/validation/M17.2.md). M17.3 adds the [bounded file/editor loop](docs/en/milestones/M17_FILES.md); `cargo xtask desktop-files-fault-test` verifies real GUI boundary/storage failures, exact recovery copies and retry. See [local validation](docs/en/validation/M17.3.md).

## Build and boot

A normal desktop keeps its windows visible when both input devices are unavailable. Read the boot snapshot with `cat /boot.log` on COM1. To enable copies on a writable virtio work disk, run `mkdir /persist/logs` once before the next boot.

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

M17.4 first-batch [build diagnostics](docs/en/milestones/M17_BUILD.md) uses launcher `b`/`g` for saved `/persist/build.c`; `cargo xtask desktop-build-test` verifies the controlled GCC/failure loop. See [actual validation](docs/en/validation/M17.4.md).

Press `m` in the launcher for Resources, Tasks and Services. With a Linux-personality
JBD2 work disk, `cargo xtask run --desktop --rust --disk build/disks/development.ext4`
admits the three fixed compiler recipes. See the [workflow](docs/en/milestones/M17_WORKFLOW.md)
and [local acceptance](docs/en/validation/M17.5.md).

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
| Desktop | `cargo xtask desktop-test`, `desktop-pty-test`, `desktop-files-test`, `desktop-files-fault-test`, `desktop-build-test`; `run --desktop --toolchain` with a Linux work disk |
| GPU | `cargo xtask gpu-test` requires a verified hardware GL renderer; `gpu-fallback-test` is headless |
| Writable ext4 (opt-in) | `cargo xtask foundation-write-test`; `ext4-image [--linux] build/disks/work.ext4`, then `run --ext4-write [--toolchain] --disk build/disks/work.ext4`; [profile and evidence](docs/en/validation/M16.4.md) |
| JBD2 / fresh work disk | `cargo xtask foundation-journal-test`; `cargo xtask foundation-migrate-test`; [M16.5](docs/en/validation/M16.5.md), [M16.6](docs/en/validation/M16.6.md) |
| Filesystem foundation | `cargo xtask foundation-test`: controlled ext4 readonly, 10,001-entry directory, 20 MiB file, 200 MiB sparse file, disk execution, reboot and rejection |
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
| [Milestones](docs/en/index.md#milestone-designs), [validation](docs/en/index.md#validation) | Accepted contracts/evidence through M15.6, bounded local M16 and M17 evidence, and remaining self-hosting work |
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
