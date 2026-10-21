# File purpose inventory

This document explains the purpose of every repository file and supplements its
top-of-file explanation. It implements [DEV-001](REQUIREMENTS.md). New or changed
files must update both the local explanation and this inventory as appropriate.
Run `cargo xtask check-files`; CI checks inventory and source-header coverage.

Generated lockfiles and legal text are documented here without modifying their
required content. Build outputs, binary inputs and generated manifests stay out
of Git; their recipes, roles and lifetime are documented in DEVELOPMENT and M4.
Unmodified third-party material keeps its upstream comments and license.

| File | Purpose |
| --- | --- |
| `.cargo/config.toml` | Routes host commands through xtask and confines kernel code-generation flags to its target. |
| `.gitattributes` | Normalize first-party text to LF while preserving binary images and archives. |
| `.github/workflows/boot.yml` | Gates development changes on source purpose, native apps and complete QEMU boot regressions. |
| `.gitignore` | Rust and generated builds |
| `AGENTS.md` | Mandatory Codex workflow, architecture, safety, language and delivery rules. |
| `ARCHITECTURE.md` | Implemented and planned subsystem boundaries, ownership and directory design. |
| `COMPATIBILITY.md` | Linux ABI strategy and software/version validation limits. |
| `Cargo.lock` | Generated locked kernel/host dependency graph and registry checksums; never insert manual comments. |
| `Cargo.toml` | Defines kernel/host packages and shared metadata; user binaries have a separate workspace. |
| `LICENSE` | Original Apache-2.0 legal terms; preserve verbatim instead of adding a source header. |
| `NOTICE` | Project attribution and notice information; preserve existing legal content. |
| `PROJECT_OUTLINE.md` | Original project goals, delivery levels, scope, work packages and risks. |
| `README.md` | Default English project entry, current status, build commands and documentation links. |
| `README.zh-CN.md` | Simplified Chinese translation of the shared README facts and commands. |
| `ROADMAP.md` | M0–M15 dependencies, implementation tasks, milestone status and exit criteria. |
| `boot/dependencies.toml` | Locks verified compiler/QEMU versions and bootloader/firmware sources and hashes. |
| `boot/limine.conf` | Defines the UEFI kernel entry; xtask appends explicitly tagged native ELF modules. |
| `crates/abi/Cargo.toml` | Defines the allocation-free native ABI shared by the kernel and application SDK. |
| `crates/abi/src/lib.rs` | Native v1 wire constants, checked user addresses and padding-free query layout. These are AnovaOS interfaces; they do not implement the Linux syscall ABI. |
| `crates/elf/Cargo.toml` | Hosts bounded ELF validation and native startup-stack construction without hardware. |
| `crates/elf/src/lib.rs` | Validate a deliberately bounded static ELF64 ET_EXEC subset before allocation. No host casts, untrusted references, relocation, interpreter or TLS processing. |
| `crates/elf/src/stack.rs` | Encode a bounded native argc/argv/envp startup block without host pointers. |
| `crates/memory/Cargo.toml` | Packages reusable no_std frame, heap and page-table policies for kernel/host tests. |
| `crates/memory/src/frames.rs` | Only explicitly supplied usable RAM enters the allocator. All other memory stays reserved, including holes and every bootloader-reclaimable byte. |
| `crates/memory/src/heap.rs` | An intrusive buddy heap. Free blocks hold their own links; allocation and coalescing never allocate metadata or call another allocator. |
| `crates/memory/src/lib.rs` | Allocation and page-table policies shared by the kernel and host tests. |
| `crates/memory/src/paging.rs` | Four-level kernel/private-user tables, permission checks and table reclamation. User roots borrow supervisor upper entries; only their lower tables are owned. |
| `crates/platform/Cargo.toml` | Packages hardware-independent ACPI parsing and typed physical/virtual addresses. |
| `crates/platform/src/acpi.rs` | ACPI 6.6, sections 5.2.5, 5.2.8, 5.2.9 and 5.2.12. Decode bytes instead of creating references to packed firmware structures. |
| `crates/platform/src/lib.rs` | Hardware-independent address types and bounded ACPI decoding. |
| `docs/CODING_STYLE.md` | English development conventions, mandatory file purposes and stable Rust formatting. |
| `docs/DEVELOPMENT.md` | Host setup, target separation, app/image commands, validation and evidence handling. |
| `docs/FILE_PURPOSES.md` | Per-file purpose inventory and generated/legal file explanation exceptions. |
| `docs/FIRST_TASK.md` | Historical M0/M1 implementation order and concrete initial acceptance criteria. |
| `docs/GITHUB_SETUP.md` | Repository branches, cloning, documentation maintenance and contribution workflow. |
| `docs/LICENSING.md` | Apache-2.0 usage and third-party licensing boundaries. |
| `docs/M2_CPU.md` | BSP traps, ISTs, ACPI/APIC timer, interrupt context and M2 limitations. |
| `docs/M3_MEMORY.md` | Frame/table/heap ownership, virtual layout, locks and original M3 limitations. |
| `docs/M4_USERSPACE.md` | Current Ring 3, native ABI, loader, copying, FPU and minimal SDK contracts. |
| `docs/REFERENCES.md` | Primary architecture, protocol, ABI and toolchain reference links. |
| `docs/REQUIREMENTS.md` | Owner-mandated development requirements, including DEV-001 file explanations. |
| `docs/STATUS.md` | Current verified capabilities, remaining boundaries and the next implementation task. |
| `docs/TOOLCHAINS.md` | Planned path to guest-hosted GCC/G++/Rust/Cargo and optional self-hosting. |
| `docs/validation/M0-M1.md` | Historical local engineering/boot acceptance evidence and input hashes. |
| `docs/validation/M2.md` | Historical local CPU/timer/trap validation evidence and limitations. |
| `docs/validation/M3.md` | Historical local memory/protection/OOM validation evidence and limitations. |
| `docs/validation/M4.md` | Local native app/isolation/teardown results, environment, artifacts and limitations. |
| `kernel/Cargo.toml` | Builds the bare-metal kernel and isolated QEMU failure-injection configurations. |
| `kernel/build.rs` | Reject host-target kernel builds and select the kernel-only linker script. |
| `kernel/src/abi/mod.rs` | Adapt native ABI v1 syscalls to console, process lifecycle and system queries. Linux personality and filesystem descriptors are intentionally not implemented. |
| `kernel/src/arch/mod.rs` | Select the implemented x86_64 hardware architecture boundary. |
| `kernel/src/arch/x86_64/acpi.rs` | Read only the firmware tables needed by the q35 BSP interrupt baseline. |
| `kernel/src/arch/x86_64/apic.rs` | BSP xAPIC periodic timer, calibrated against the 3.579545 MHz ACPI PM timer. External I/O APIC lines and legacy PICs remain masked until device drivers exist. |
| `kernel/src/arch/x86_64/cpu.rs` | Privileged operations for the single BSP. No FPU/SIMD is enabled here. |
| `kernel/src/arch/x86_64/interrupts.S` | Build uniform saved-GPR frames for CPU traps, IRQs and the native syscall |
| `kernel/src/arch/x86_64/mod.rs` | Select the x86_64 CPU, paging, interrupt and user-transition implementations. |
| `kernel/src/arch/x86_64/paging.rs` | A bounded BSP-only copy-on-write extension of the retained Limine tables. M3 consumes this mapper and clones every live table into owned frame storage. |
| `kernel/src/arch/x86_64/traps.rs` | Stable Rust uses explicit assembly entries, not the unstable x86-interrupt ABI. All gates clear IF. Kernel traps diagnose, user faults terminate one app; native syscalls and narrowly guarded usercopy faults return through IRETQ. |
| `kernel/src/arch/x86_64/user.S` | Enter one Ring 3 context, switch SYSCALL to its trusted stack, and return to |
| `kernel/src/arch/x86_64/user.rs` | BSP Ring 3 transition and native SYSCALL configuration with IRETQ returns. User code gets baseline x87/SSE2 only; no kernel SIMD or concurrent tasks. |
| `kernel/src/boot.rs` | Borrow retained Limine data. M3 allocates only USABLE RAM; bootloader data, original stacks/tables, modules, firmware and the early pool remain reserved. Validated native ELF modules provide the M4 applications without a filesystem. |
| `kernel/src/main.rs` | Boot the BSP, initialize owned memory/CPU state, then run isolated native apps. |
| `kernel/src/mm/lock.rs` | Non-sleeping BSP locks. Order: memory manager before heap; the fixed heap never calls the frame allocator. IRQ/NMI/fatal paths must not take either. |
| `kernel/src/mm/mod.rs` | BSP memory ownership: checked USABLE frames, fully owned table hierarchy, guarded stacks, reclaiming buddy heap and private process memory integration. |
| `kernel/src/mm/uaccess.rs` | Copy bounded user bytes after page-wise checks, recovering only at REP MOVSB. IF stays clear and mappings cannot change on this BSP; M5 must add pinning. |
| `kernel/src/mm/user.rs` | Own a single application's private lower-half pages and guarded kernel stack. Upper kernel tables are borrowed only while the permanent mappings stay stable. |
| `kernel/src/mm/validation.rs` | Runtime invariants and isolated QEMU stress/fault validation. |
| `kernel/src/serial.rs` | Single-BSP COM1 output. No heap, lock, or firmware calls. |
| `kernel/src/task/loader.rs` | Load a validated native ET_EXEC into zeroed private pages and build argc/argv. Permission decisions precede activation; physical aliases initialize RX data. |
| `kernel/src/task/mod.rs` | Run one isolated native process at a time and return to a trusted continuation. This owns lifecycle/teardown, not a scheduler, fork, signals or Linux processes. |
| `kernel/src/task/validation.rs` | Exercise the real loader/syscall/fault boundaries and verify exact reclamation. |
| `linker/user-x86_64.ld` | Place native static user text/rodata/data in separate pages below the image limit. |
| `linker/x86_64.ld` | Link the high-half kernel with retained Limine requests and separate RX/R/RW segments. |
| `rust-toolchain.toml` | Pins the compiler, lint/format tools and bare-metal standard-library artifacts. |
| `rustfmt.toml` | Use stable rustfmt options and the same style in editors and on the command line. |
| `user/Cargo.lock` | Generated locked native SDK/application dependency graph; never insert manual comments. |
| `user/Cargo.toml` | Keeps native user ELF builds separate from the bare-metal kernel's target flags. |
| `user/build.rs` | Rebuild native app binaries when their external user-only linker script changes. |
| `user/c/anova.h` | Native v1 C wire layout and syscall wrappers; no libc or Linux ABI dependency. |
| `user/c/crt.S` | Convert the native startup stack to C main arguments and exit through SYSCALL. |
| `user/c/hello.c` | Show the standalone C SDK, command line, zeroed BSS, SSE2 and exit status 7. |
| `user/hello/Cargo.toml` | Builds the Rust hello ELF that verifies startup arguments, zeroed BSS and SSE2. |
| `user/hello/src/main.rs` | Independent Rust ELF showing argc/argv, BSS and CPU state through native syscalls. |
| `user/probe/Cargo.toml` | Builds the native negative-case ELF used by the QEMU isolation acceptance gate. |
| `user/probe/src/main.rs` | Trigger real Ring 3 syscall errors, protection faults and unsafe-return attempts. The kernel acceptance runner supplies one named case and verifies its outcome. |
| `user/rt/Cargo.toml` | Packages the no_std startup entry and native v1 syscall wrappers for Rust apps. |
| `user/rt/src/lib.rs` | Minimal native Rust CRT, borrowed startup arguments and v1 syscall wrappers. Built with the x86_64 user ABI and SSE2, without libc, allocation or Linux calls. |
| `user/rt/src/memory.rs` | Supply freestanding compiler memory intrinsics without linking host libc. |
| `xtask/Cargo.toml` | Builds host-side dependency, native-app, image, QEMU and evidence commands. |
| `xtask/src/apps.rs` | Build standalone native Rust/C ELF files with user-only target/linker flags. Validate each output and record its explicit ABI identity and SHA-256 manifest. |
| `xtask/src/assets.rs` | Fetch, checksum and locally extract locked bootloader/firmware inputs. |
| `xtask/src/main.rs` | Host CLI for reproducible kernel/app builds, ISO assembly and QEMU validation. |
| `xtask/src/purposes.rs` | Check file-purpose inventory coverage and top-of-file comments for first-party code. Human review still verifies that each explanation describes its actual responsibility. |
| `xtask/src/smoke.rs` | Interpret the guest transcript and isa-debug-exit result together. |
