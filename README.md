<!-- Introduce AnovaOS, its verified capabilities, build entry points and project goals. -->
# AnovaOS

**English** | [简体中文](README.zh-CN.md)

An x86_64 operating system built from scratch in Rust. The goal is to edit,
compile, debug, and run applications inside AnovaOS itself, while gradually
supporting selected existing Linux software.

**Current status: M0–M13 (bounded offline and online profiles) are locally verified
on QEMU q35/TCG. Native apps,
VFS, pipes, init/shell, modern virtio-blk and checked ext2 storage at `/persist`
include source editing, sync/fsync and clean ACPI shutdown/reboot persistence.
The bounded Linux profile runs musl 1.2.4 static, static PIE and dynamic programs,
pthread/TLS/futex tests, real signal handlers and restart/interruption, poll,
nonblocking I/O and four PTY pairs. Startup shared libraries and dlopen with TLS
are tested alongside the pinned BusyBox 1.36.1 echo/ls/cat/ash profile.
There are 32 total process/thread slots on one CPU. M10 adds opt-in virtio-net IPv4 TCP/UDP/DNS, interruptible sockets,
verified user-space TLS and locked package installation, upgrades, rollback and removal.
M11 adds an optional offline GCC/G++/make development image: C/C++ applications
compile and run inside the guest, with exceptions, threads and reboot retention verified.
M12 adds guest Rust 1.90.0/std and offline Cargo with vendor dependencies, build scripts,
C FFI, standalone macros and a Cargo macro/build-script dependency graph.
Opt-in online Cargo adds strict HTTPS registry/Git downloads and persisted-cache
rebuilds; public crates.io/GitHub are tested through a configured host proxy.
M13 adds SDK 1.0.0, native C/C++/Rust templates, explicit ABI-bound packages,
installer target-policy checks and real user-fault source symbolization.
M14.1 adds opt-in software graphics, checked framebuffer mappings and actual
QEMU pixel/screenshot verification. M14.2 implements polled PS/2 physical input
and a clipped cursor. M14.3 adds an isolated Rust user-space compositor,
bounded private windows, focused input and checked crash/restart recovery.
M14.4 adds the Rust GUI runtime, C/C++/Rust SDKs and graphical applications,
with independent guest builds, physical input and reboot retention verified.
M14.5 verifies bounded hardware VirGL clear/readback with CPU fallback;
ordinary desktop composition remains software-rendered. M15.1/M15.2 add
measured capacity and guest-built bzip2 source reconstruction with reboot and
failure recovery. M15.3/M15.4 assess GCC and independent Rust/LLVM/Cargo
feasibility with actual guest probes; current dependency/capacity blockers
prevent compiler reconstruction. M15.5 reconstructs and boots the actual
kernel using inherited guest tools, with exact retained-ELF host-assisted
packaging. Compiler reconstruction and guest-only image generation remain
unverified.
General Linux/glibc remains future work; device IRQs stay masked.**

Repository: [ananwanan/AnovaOS](https://github.com/ananwanan/AnovaOS).

The default development branch and pull request target is **`dev`**. `main`
contains stable snapshots after milestone acceptance. See the
[collaboration guide](docs/GITHUB_SETUP.md).

## Project goals

1. Build a Rust kernel for x86_64 with UEFI and Limine, using QEMU/OVMF as the
   initial reference platform.
2. Implement memory management, interrupts, process isolation, preemptive
   scheduling, filesystems, a terminal, and networking.
3. Provide an application SDK for running cross-compiled Rust/C/C++ applications
   as user processes.
4. Validate Linux x86_64 ABI support per program and version, starting with
   statically linked musl command-line tools before dynamic linking and more
   complex software.
5. Run GCC/G++, rustc, and Cargo inside AnovaOS to support application development
   on the system itself.
6. Add a window system, graphical applications, multicore support, and more
   physical hardware drivers after the development environment is stable.

Rust is the primary language for the kernel and first-party system components.
Reused bootloaders, libc implementations, compilers, and other tools may use
C/C++ or other languages; rewriting mature tools is not a prerequisite.

## Technical baseline

| Area | Initial choice | Scope |
| --- | --- | --- |
| Architecture | x86_64, four-level paging, initially one CPU | No initial 32-bit or x32 ABI support |
| Boot | UEFI + Limine, ELF kernel | Secure Boot signing comes later |
| Kernel | Modular monolithic kernel, Rust `no_std` + `alloc` since M3 | `panic=abort`; minimal necessary assembly |
| Compilation target | `x86_64-unknown-none` for the kernel | Separate targets and linker configuration for user applications |
| Reference platform | QEMU q35, OVMF, serial output, modern virtio PCI | TCG must work; KVM is optional |
| User space | ELF64, System V AMD64 function calling convention | Document native and Linux ABIs separately |
| Software reuse | Linux ABI compatibility layer and pinned musl user space | No promise of general Linux distribution compatibility |
| Persistent storage | initramfs/tmpfs → virtio-blk + a limited ext2 implementation | Keep ESP/FAT separate from the root filesystem |
| Initial development host | Linux/WSL2 cross-development, followed by compilation inside AnovaOS | A graphical desktop is not required for compilation |

These are project design decisions. Pin the versions, commits, and checksums
actually verified during implementation. See the
[official references](docs/REFERENCES.md).

## Documentation

The detailed planning documents are currently in Simplified Chinese. The
development language and formatting guide is in English.

| File | Contents |
| --- | --- |
| [PROJECT_OUTLINE.md](PROJECT_OUTLINE.md) | Overall plan, delivery levels, scope, and risks |
| [AGENTS.md](AGENTS.md) | Codex constraints, execution order, coding rules, and acceptance requirements |
| [ROADMAP.md](ROADMAP.md) | M0–M15 dependencies, tasks, and exit criteria |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Kernel, application, ABI, driver, and directory boundaries |
| [COMPATIBILITY.md](COMPATIBILITY.md) | Software compatibility strategy, ABI requirements, and validation matrix |
| [docs/TOOLCHAINS.md](docs/TOOLCHAINS.md) | GCC/G++/Rust from cross-compilation to development inside AnovaOS |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | Host setup, implemented commands, outputs, and failure handling |
| [docs/CODING_STYLE.md](docs/CODING_STYLE.md) | English development conventions and Rust formatting |
| [docs/FIRST_TASK.md](docs/FIRST_TASK.md) | Ordered M0/M1 implementation steps and completion criteria |
| [docs/validation/M0-M1.md](docs/validation/M0-M1.md) | Actual local build and boot validation evidence |
| [docs/M2_CPU.md](docs/M2_CPU.md) | CPU tables, guarded stacks, ACPI/APIC, timing and context rules |
| [docs/validation/M2.md](docs/validation/M2.md) | Actual M2 timer and exception validation evidence |
| [docs/M3_MEMORY.md](docs/M3_MEMORY.md) | Frame/table ownership, stack, heap, permissions and OOM |
| [docs/validation/M3.md](docs/validation/M3.md) | Actual M3 memory and protection validation evidence |
| [docs/M4_USERSPACE.md](docs/M4_USERSPACE.md) | Ring 3, ELF, native v1 ABI, user copying and Rust/C SDK |
| [docs/validation/M4.md](docs/validation/M4.md) | Actual M4 isolation and application validation evidence |
| [docs/M5_PROCESSES.md](docs/M5_PROCESSES.md) | Scheduling, process ownership, native lifecycle ABI and limits |
| [docs/validation/M5.md](docs/validation/M5.md) | Actual multiprocess, CPU/FPU, OOM and reclamation evidence |
| [docs/M6_FILESYSTEM.md](docs/M6_FILESYSTEM.md) | VFS, descriptor/pipe ownership, serial terminal, init and native shell |
| [docs/validation/M6.md](docs/validation/M6.md) | Actual file, shell, pipe, terminal and volatile-reboot validation |
| [docs/M7_STORAGE.md](docs/M7_STORAGE.md) | PCI/virtio ownership, ext2 profile, durability, disk images and source editor |
| [docs/validation/M7.md](docs/validation/M7.md) | Shutdown/reboot, source/binary integrity, device errors and regression evidence |
| [docs/M8_LINUX.md](docs/M8_LINUX.md) | Static Linux ABI, pinned musl/BusyBox builds and supported CLI profile |
| [docs/M9_RUNTIME.md](docs/M9_RUNTIME.md) | Threads, signals, terminals, dynamic musl and ownership limits |
| [docs/validation/M9.md](docs/validation/M9.md) | Runtime hashes, host comparison, QEMU acceptance and regressions |
| [docs/M10_NETWORK_PACKAGES.md](docs/M10_NETWORK_PACKAGES.md) | Controlled IPv4 sockets, DNS, TLS and persistent package contracts |
| [docs/validation/M10.md](docs/validation/M10.md) | Network, package, TLS, reboot and regression evidence |
| [docs/M11_TOOLCHAIN.md](docs/M11_TOOLCHAIN.md) | Offline guest GCC/G++/make, sysroot, bounds and edit/build/run workflow |
| [docs/validation/M11.md](docs/validation/M11.md) | Source-built tools, guest C/C++/make, negatives and reboot evidence |
| [docs/M12_NETWORK.md](docs/M12_NETWORK.md) | Opt-in online Cargo, strict TLS, public sources and offline cache rebuilds |
| [docs/M12_RUST.md](docs/M12_RUST.md) | Guest Rust/std, offline Cargo, build scripts, C FFI, macros and resource bounds |
| [docs/validation/M12.md](docs/validation/M12.md) | Actual guest compilation, persisted hashes, reboot and regression evidence |
| [docs/M13_SDK.md](docs/M13_SDK.md) | Versioned SDK, application manifests, install/upgrade workflow and fault symbols |
| [docs/validation/M13.md](docs/validation/M13.md) | Actual SDK/package/diagnostic and preserved regression evidence |
| [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md) | Mandatory owner requirements, including file-purpose explanations |
| [docs/FILE_PURPOSES.md](docs/FILE_PURPOSES.md) | Purpose of every repository file |
| [docs/STATUS.md](docs/STATUS.md) | Current implementation status and next steps |
| [docs/GITHUB_SETUP.md](docs/GITHUB_SETUP.md) | Cloning, documentation maintenance, and contribution conventions |
| [docs/LICENSING.md](docs/LICENSING.md) | Apache-2.0 usage and third-party licensing boundaries |

## Development conventions

Use English for first-party code identifiers, comments, Rustdoc, safety
explanations, logs, errors, CLI help, tests, commit messages, pull request titles
and descriptions, and new development documentation. Explicit translations and
localization resources may use their target language. Preserve third-party text
and existing Chinese planning documents as described in the
[coding style guide](docs/CODING_STYLE.md).

Every new or modified first-party file must explain its purpose near the top
and remain listed in [FILE_PURPOSES.md](docs/FILE_PURPOSES.md). Run
`cargo xtask check-files`; see [DEV-001](docs/REQUIREMENTS.md).

Rust formatting is defined in [rustfmt.toml](rustfmt.toml): four spaces, a
preferred width of 100 columns, LF line endings, Rust 2024 parser/style editions,
and stable rustfmt options. Format and check the workspace from the repository root:

```sh
cargo fmt --all
cargo fmt --all -- --check
cargo fmt --manifest-path user/Cargo.toml --all -- --check
```

Keep this English README and [README.zh-CN.md](README.zh-CN.md) synchronized when
shared project information changes.

## What counts as developing applications inside AnovaOS?

During one AnovaOS session, use its own terminal and persistent storage to
complete the following without forwarding compilation requests to the host:

- Edit and save `main.c`, `main.cpp`, and `main.rs`.
- Compile, link, and run applications using `gcc`, `g++`, and `rustc` running
  inside AnovaOS.
- Build a small project with prepared dependencies using `cargo build --offline`,
  run the result, and verify its output.
- Read and write files, handle command-line arguments, and report errors. Source
  files and build artifacts must survive a reboot.
- Retrieve pinned registry/Git dependencies with `cargo-online build` and rebuild
  from the saved cache after reboot with `--offline --frozen`.
- Validate a full debugger, broader Cargo builds and compiler bootstrapping in
  later stages.

The Linux compatibility layer may initially provide the environment for these
tools; they still run on the AnovaOS kernel. An AnovaOS-specific target and a
native Rust `std` port have separate milestones.

## Build and boot

Use x86_64 Ubuntu 24.04 or WSL2, rustup, QEMU 8.2.2, xorriso, curl, dpkg-deb, GCC and e2fsprogs.
The repository selects Rust 1.90.0, rustfmt, Clippy, and the bare-metal target.
See [development setup](docs/DEVELOPMENT.md) for dependencies and firmware paths.

```sh
cargo xtask fetch       # Download and verify pinned Limine/OVMF inputs
cargo xtask doctor      # Report all missing or mismatched prerequisites
cargo xtask build       # Build and inspect the bare-metal kernel ELF
cargo xtask apps        # Build standalone native Rust/C ELFs and ABI/hash inventory
cargo xtask check-files # Check per-file purpose explanations and inventory
cargo xtask image       # Build build/images/anova-normal.iso, including native apps
cargo xtask run         # Run boot workloads, PID 1 init and the interactive native shell
cargo xtask smoke       # CPU/memory/process tests, VFS/pipe/shell/OOM suite AND exit 33
cargo xtask verify      # 13 cases: boot passes; fault, panic, timeout and OOM fail
cargo xtask terminal-test # Real serial editing/EOF and two-boot tmpfs-loss validation
cargo xtask persistence-test # Source/binary reboot integrity, clean FS and I/O failures
cargo xtask root-image build/disks/work.ext2 # Create a new persistent data image
cargo xtask run --disk build/disks/work.ext2 # Reuse that image; shutdown cleanly in guest
```

These commands now exist in the host `xtask` package. Images, caches, logs, and
disposable firmware variable copies stay in `build/`. Normal runs use a temporary data disk; networking is opt-in with `--network`. GitHub Actions is configured to run the same checks; its status
is reported separately from [local validation](docs/validation/M12.md).

## Next steps

M13 delivers the versioned SDK and application format described in
[M13 SDK](docs/M13_SDK.md). M14.1 delivers checked software drawing and M14.2
implements bounded PS/2 input/cursor diagnostics. M14.3 user-space composition
and M14.4 GUI runtime/SDKs are locally accepted. M14.5 verifies the bounded
[hardware clear/readback profile](docs/M14_GPU.md) with CPU fallback. Current Rust/Cargo limits remain in
[M12 Rust](docs/M12_RUST.md). See [current status](docs/STATUS.md) and
[roadmap](ROADMAP.md). M15.3 has a locally verified GCC feasibility assessment.
The GCC route next needs build utilities, arithmetic development inputs and sufficient capacity
before a first compiler stage. M15.4 independently assesses Rust/LLVM/Cargo
source routes; M15.5 now reconstructs and boots the kernel with the inherited guest rustc.

## M15 controlled source reconstruction

```sh
cargo xtask bootstrap-prepare
cargo xtask bootstrap-test
```

A fresh 16 MiB Linux-personality work disk carries checksum-locked official
bzip2 1.0.8 sources. The guest GCC compiles and links the compressor, whose
compressed bytes match the official fixture. Three QEMU boots verify resource
rejection/recovery, reconstruction, clean reboot and retained outputs after a
real compiler error. Measurements report scheduler-boundary observations and
end-of-boot disk use. See [M15 bootstrap](docs/M15_BOOTSTRAP.md) and
[local evidence](docs/validation/M15.1-M15.2.md).

`cargo xtask bootstrap-gcc-test` audits seven locked source archives and runs
real C++11 seed, missing-header/tool, configure-startup and capacity/reboot
probes. The successful assessment reports current GCC reconstruction blockers;
it produces no new compiler stage. See [M15 GCC](docs/M15_GCC.md) and
[local evidence](docs/validation/M15.3.md).

`cargo xtask bootstrap-rust-test` separately audits the official Rust 1.90.0
source distribution, included LLVM/Cargo components and prescribed stage0.
Two guest boots verify Rust std/C++17 seeds, actual build-tool and isolated
offline-dependency failures, rejected Rust compilation with the prior ELF
retained, and identical persisted hashes. Source reconstruction is blocked in
the current profile; no LLVM/rustc/Cargo component is rebuilt. See
[M15 Rust](docs/M15_RUST.md) and [local evidence](docs/validation/M15.4.md).
`cargo xtask bootstrap-kernel-test` reconstructs the actual bare-metal kernel
with inherited guest Cargo/rustc/rust-lld and locked sources/target libraries.
Three boots verify construction, exact retained-ELF host-assisted packaging,
actual kernel identity and failure/reboot recovery. Persistent files allow
4 MiB while tmpfs and shared caches remain at 1 MiB. See
[M15 kernel](docs/M15_KERNEL.md) and [local evidence](docs/validation/M15.5.md).
Compiler reconstruction, guest-only image generation and M15.6 independent
clean-build comparisons remain unverified.

## License

Original project code, documentation, and examples are licensed under the
**[Apache License 2.0](LICENSE)** (`Apache-2.0`). Copyright 2026 ananwanan and the
AnovaOS contributors.

The license permits commercial use, modification, and distribution, including
closed-source derivatives subject to its terms, and includes a contributor
patent grant. Distributions must satisfy the license, attribution, and change
notice requirements. See the [licensing guide](docs/LICENSING.md) and
[NOTICE](NOTICE).

Third-party bootloaders, libc implementations, compilers, libraries, and imported
code retain their respective licenses when included in this repository or a
system image.

See [M8 static Linux CLI](docs/M8_LINUX.md) for exact limits, build dependencies
and the compatibility commands `cargo xtask trace-compat` / `cargo xtask test-compat`.
The normal UART session can launch `/linux/sh`; inside it use `/linux/echo`,
`/linux/cat` and `/linux/ls`.

## M9 musl runtime

```sh
linux-threads
linux-pie
linux-dynamic
```

These are native-shell commands launching independent Linux user ELFs. They test
pthread/futex, shared memory, signals, readiness and PTYs; linux-dynamic also
loads actual shared objects and dynamic TLS. See [M9 runtime profile](docs/M9_RUNTIME.md)
and [M9 validation](docs/validation/M9.md) for supported flags, resource limits,
missing-library diagnostics and remaining job-control limitations. Native v1 and
the persistent disk ABI policy keep their existing contracts.

## M10 controlled networking and packages

```sh
cargo xtask network-test
cargo xtask package-test
cargo xtask run --network --disk build/disks/work.ext2
```

Inside the guest use `pkg install demo 1`, `pkg status demo`, `pkg install demo 2`
and `pkg remove demo`. Data under `/persist/data/demo` survives upgrades and removal.
`dns NAME` resolves IPv4 A records. HTTPS acquisition is currently restricted to
the controlled host repository with its generated test CA. See the
[M10 profile](docs/M10_NETWORK_PACKAGES.md) for exact commands and limitations.
M12 adds the offline Rust/Cargo development profile below.

## M11 offline C/C++ development

```sh
cargo xtask toolchain-build
cargo xtask toolchain-image
cargo xtask root-image --linux build/disks/devel.ext2
cargo xtask run --toolchain --disk build/disks/devel.ext2
cargo xtask toolchain-test
```

The optional development image uses 1 GiB RAM. In its guest shell, save source
with `edit`, then use `gcc`, `g++ -std=c++17 -pthread`, and `make` under `/persist`.
C stdio/files/malloc/argv and multi-file builds, C++ string/vector/RAII/exceptions/
threads, make incremental builds and clean reboot retention are tested. See the
[M11 profile](docs/M11_TOOLCHAIN.md) and [acceptance evidence](docs/validation/M11.md).


## M12 Rust/Cargo development

```sh
cargo xtask rust-build
cargo xtask rust-image
cargo xtask root-image --linux build/disks/rust-devel.ext2
cargo xtask run --rust --disk build/disks/rust-devel.ext2
cargo xtask rust-cross-test
cargo xtask rust-runtime-test
cargo xtask rust-test
```

The opt-in image uses 3 GiB RAM and checksum-locked official Rust 1.90.0 musl
binary distributions, alongside the source-built M11 C tools. Guest rustc
compiles std applications and a shared procedural macro; offline Cargo builds a
locked vendored crate, runs build.rs and links guest-compiled C. Source edits,
freshness, real syntax failures and execution across clean reboot are checked.
See [M12 profile](docs/M12_RUST.md) and [acceptance evidence](docs/validation/M12.md).
For online projects, add `--network` to `run --rust`, edit a project under
`/persist`, and use `cargo-online build`. Its cache defaults to
`/persist/cargo-home`; ordinary `cargo` retains the offline default. After a clean
shutdown and reboot without a NIC, use `cargo-online build --offline --frozen`.

`cargo xtask rust-online-test` verifies controlled HTTPS sparse registry/Git,
CA/name/date checks, corrupted/truncated downloads, timeouts and a fresh offline
rebuild. `cargo xtask rust-public-test` separately verifies pinned crates.io and
GitHub sources; local acceptance used a reachable host proxy and public PEM roots.
See [M12 networking](docs/M12_NETWORK.md) for configuration and resource limits.
A native AnovaOS std port, arbitrary Cargo projects and compiler bootstrap
remain separate work.

## M13 SDK and application packages

```sh
cargo xtask sdk
cargo xtask app-new c build/hello-c
cargo xtask app-build build/hello-c
cargo xtask app-pack build/hello-c/app.toml build/hello-c/hello-c.apkg
cargo xtask sdk-test
cargo xtask app-test
cargo xtask package-v2-test
cargo xtask diagnostics-test
```

Use `cpp` or `rust` for the other native templates. `app-run PACKAGE SHA256`
creates an image containing your checked package, selects a matching temporary
disk, and prints actual guest install/run commands; use `--disk` to reuse an
explicit persistent image. Follow the complete [create/build/install/upgrade
recipe](docs/M13_SDK.md). The versioned native SDK stays separate from the
M11/M12 Linux-musl guest compilers. Package permissions are admission metadata;
GUI runtime, GDB stubs, per-process sandboxes and native Rust std remain outside
this accepted profile.

## M14.1–M14.2 software display and input

```sh
cargo xtask graphics-image
cargo xtask graphics-test
cargo xtask input-test
cargo xtask run --graphics
```

The opt-in profile draws a bounded CPU test pattern and retains the COM1 shell.
It also polls the QEMU PS/2 keyboard and three-button relative mouse, delivers
physical events to one diagnostic consumer and draws a clipped cursor. Text
and window focus policy remain separate. Input and cursor storage allocate no heap.
Interactive runs need a GTK display.

`graphics-test` checks the boot screenshot and serial fallback with/without VGA.
`input-test` checks ordered QMP input, every pixel at five cursor positions,
bounded reset/reconnect cases and recovery with no display or no controller.
The M14.2 local QEMU gate passed. USB, physical hotplug, peripheral IRQs,
user-space windows and GUI SDKs remain outside this profile.
See [M14 GUI](docs/M14_GUI.md), [M14.1 evidence](docs/validation/M14.1.md)
and [M14.2 evidence](docs/validation/M14.2.md).

## M14.3 isolated native windows

```sh
cargo xtask gui-image
cargo xtask gui-test
cargo xtask run --gui
```

In the interactive profile, type `gui-test` in the COM1 shell to start the
supervised compositor and two isolated native windows. Click to focus, drag a
title to move, and drag the bottom-right corner to resize. The demonstration
uses X in the red window for a deliberate client fault, F12 for a compositor
fault/restart, and F11 after restart to finish and return to the serial shell.
Interactive display requires GTK; the automated gate runs headlessly.

The local three-boot gate passed with eight complete pixel comparisons,
focused input, window redraw, actual client/server faults, fresh-session
reconnection, heap-OOM rollback and serial/device fallback. Each client has
one private 32x24 through 320x240 RGB surface, with four total connections.
Presentation uses bounded CPU copies. M14.4 below delivers text/widgets,
a graphical terminal and the public C/C++/Rust GUI SDK. GPU acceleration
remains a separate stage.
See [M14 compositor](docs/M14_COMPOSITOR.md) and
[M14.3 evidence](docs/validation/M14.3.md).

## M14.4 GUI runtime and C/C++/Rust SDKs

```sh
cargo xtask gui-sdk
cargo xtask gui-sdk-image
cargo xtask gui-sdk-test
cargo xtask run --gui-sdk
```

The allocation-free Rust runtime implements a separate versioned C ABI,
original bitmap text, clipped drawing, layout, buttons, text fields and a
bounded process-private clipboard. C headers, C++ move-only RAII and safe Rust
wrappers share its semantics. The development image includes the accepted
GCC/G++ and Rust tools; `gui-build c`, `gui-build cpp` and `gui-build rust`
compile independent native client sources in the guest. A native data disk
keeps sources and executable output across clean reboot.

The Rust note application exercises editing/copying. The bounded graphical
terminal accepts `echo TEXT` and `pwd`, uses real native command processes and
pipes, and displays their output. It has no PTY/job control or ANSI emulation.
Run `gui-run /persist/gui-c` after building; use physical `a`, the button and
`q`, then F11 to stop the compositor and return to serial input. See the
[development walkthrough](docs/DEVELOPMENT.md), [runtime contract](docs/M14_RUNTIME.md)
and [M14.4 evidence](docs/validation/M14.4.md). The bounded GPU path is described below.

## M14.5 bounded GPU drawing and CPU fallback

```sh
cargo xtask gpu-test
cargo xtask gpu-fallback-test
```

The full gate uses a secondary virtio-gpu/VirGL device for actual offscreen
render-target clears and fenced readbacks. This exact device reported
`D3D12 (NVIDIA GeForce RTX 4060 Ti)` locally. It compares all returned pixels
with a CPU reference, checks guarded odd dimensions and invalid parameters,
then verifies actual timeout/reset, CPU fallback and GPU reinitialization.
The headless fallback gate covers 2D-only and absent GPU devices and runs in CI.

The authenticated Rust compositor presents the result through retained VGA
scanout. Ordinary desktop composition and display copying remain on the CPU;
this does not establish shader draws, a guest GL/Vulkan API or a speedup.
See [M14 GPU](docs/M14_GPU.md), [development setup](docs/DEVELOPMENT.md)
and [M14.5 evidence](docs/validation/M14.5.md).
