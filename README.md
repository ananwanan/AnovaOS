# AnovaOS

**English** | [简体中文](README.zh-CN.md)

An x86_64 operating system built from scratch in Rust. The goal is to edit,
compile, debug, and run applications inside AnovaOS itself, while gradually
supporting selected existing Linux software.

**Current status: project planning and development conventions only. No kernel,
bootable image, or software compatibility has been implemented or verified.**

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
| Kernel | Modular monolithic kernel, Rust `no_std` + `alloc` | Initially `panic=abort`; minimal necessary assembly |
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
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | Host environment, planned commands, validation, and version records |
| [docs/CODING_STYLE.md](docs/CODING_STYLE.md) | English development conventions and Rust formatting |
| [docs/FIRST_TASK.md](docs/FIRST_TASK.md) | First implementation tasks for Codex |
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

Rust formatting is defined in [rustfmt.toml](rustfmt.toml): four spaces, a
preferred width of 100 columns, LF line endings, Rust 2024 parser/style editions,
and stable rustfmt options. Once a Cargo workspace exists, format and check it
from the repository root:

```sh
cargo fmt --all
cargo fmt --all -- --check
```

These commands require a real `Cargo.toml`; the repository does not have one yet.
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
- Validate online dependency retrieval, a full debugger, complex Cargo builds,
  and compiler bootstrapping in later stages.

The Linux compatibility layer may initially provide the environment for these
tools; they still run on the AnovaOS kernel. An AnovaOS-specific target and a
native Rust `std` port have separate milestones.

## Next steps

Start with the [first implementation tasks](docs/FIRST_TASK.md) for M0 and M1:
project scaffolding, a pinned toolchain, QEMU boot, serial logging, and smoke
tests that reliably detect failure.

The documented `cargo xtask ...` interfaces are planned and have not been
implemented. There is currently no `Cargo.toml` or CI workflow.

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
