# Repository file purposes

This inventory explains every tracked first-party file and the generated or
third-party exceptions to top-of-file comments. Maintain it with DEV-001;
`cargo xtask check-files` checks coverage and applicable headers.

| File | Purpose |
| --- | --- |
| `.cargo/config.toml` | Routes host commands through xtask and confines kernel code-generation flags to its target. |
| `.gitattributes` | Normalize first-party text to LF while preserving binary images and archives. |
| `.github/workflows/boot.yml` | Gate file purposes, host/user/kernel checks, complete QEMU and real serial regressions. |
| `.gitignore` | Ignore generated host/user builds, images, toolchains and local machine secrets. |
| `AGENTS.md` | Define mandatory repository development, safety, validation and delivery conventions. |
| `ARCHITECTURE.md` | Describe implemented subsystem boundaries and the planned operating-system architecture. |
| `COMPATIBILITY.md` | Separate verified native support from planned existing-software ABI compatibility. |
| `Cargo.lock` | Generated locked kernel/host dependency graph and registry checksums; never insert manual comments. |
| `Cargo.toml` | Define kernel, host and shared policy/VFS packages; user binaries use their own workspace. |
| `LICENSE` | Original Apache-2.0 legal terms; preserve verbatim instead of adding a source header. |
| `NOTICE` | Project attribution and notice information; preserve existing legal content. |
| `PROJECT_OUTLINE.md` | Original project goals, delivery levels, scope, work packages and risks. |
| `README.md` | Introduce AnovaOS, its verified capabilities, build entry points and project goals. |
| `README.zh-CN.md` | Provide the synchronized Simplified Chinese project introduction and build guide. |
| `ROADMAP.md` | Track milestone dependencies, implementation checklists and acceptance criteria. |
| `boot/dependencies.toml` | Locks verified compiler/QEMU versions and bootloader/firmware sources and hashes. Change only with refreshed integrity checks and the complete M1 validation. |
| `boot/limine.conf` | Defines the UEFI kernel entry; xtask appends explicitly tagged native ELF modules. |
| `crates/abi/Cargo.toml` | Defines the allocation-free native ABI shared by the kernel and application SDK. |
| `crates/abi/src/lib.rs` | Native v1 constants, checked user addresses and padding-free process/query layouts. These are AnovaOS interfaces; they do not implement the Linux syscall ABI. |
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
| `crates/sched/Cargo.toml` | Shares bounded process state, parent/wait semantics and round-robin policy with host tests. |
| `crates/sched/src/lib.rs` | Bounded BSP scheduling policy and parent/child lifecycle without hardware or allocation. The kernel owns contexts/resources; this policy never switches stacks or takes locks. |
| `crates/vfs/Cargo.toml` | Share bounded in-memory VFS, archive, open-file and pipe semantics with host tests. |
| `crates/vfs/src/archive.rs` | Decode the project-owned ANOVAFS1 archive using checked byte ranges, never casts. Entries are parent-first absolute UTF-8 paths; payloads remain borrowed from boot RAM. |
| `crates/vfs/src/fd.rs` | Store process-local descriptor indices; Vfs alone manages open-description references. Tables cannot be copied or cloned without incrementing the associated shared ownership. |
| `crates/vfs/src/lib.rs` | Bounded VFS mounts, component-wise paths, shared open descriptions and byte-stream pipes. One non-preemptible owner serializes all mutations; no hardware, locks or user pointers. |
| `crates/vfs/src/tests.rs` | Test VFS path/mount boundaries, descriptor ownership, pipe shutdown and capacity rollback. |
| `docs/CODING_STYLE.md` | English development conventions, mandatory file purposes and stable Rust formatting. |
| `docs/DEVELOPMENT.md` | Host setup, target separation, app/image commands, validation and evidence handling. |
| `docs/FILE_PURPOSES.md` | Per-file purpose inventory and generated/legal file explanation exceptions. |
| `docs/FIRST_TASK.md` | Historical M0/M1 implementation order and concrete initial acceptance criteria. |
| `docs/GITHUB_SETUP.md` | Explain development branches, current subsystem entry points and contribution conventions. |
| `docs/LICENSING.md` | Apache-2.0 usage and third-party licensing boundaries. |
| `docs/M2_CPU.md` | BSP traps, ISTs, ACPI/APIC timer, interrupt context and M2 limitations. |
| `docs/M3_MEMORY.md` | Frame/table/heap ownership, virtual layout, locks and original M3 limitations. |
| `docs/M4_USERSPACE.md` | Historical M4 entry/loader/copy/SDK contracts, with M5 supersession and ownership notes. |
| `docs/M5_PROCESSES.md` | Define BSP scheduling, CPU/FPU state, native process syscalls and ownership/locking boundaries. |
| `docs/M6_FILESYSTEM.md` | Define VFS/archive, fd/pipe ownership, native ABI, terminal, init and shell contracts. |
| `docs/REFERENCES.md` | Primary architecture, protocol, ABI and toolchain reference links. |
| `docs/REQUIREMENTS.md` | Owner-mandated development requirements, including DEV-001 file explanations. |
| `docs/STATUS.md` | Record verified M0–M7 capabilities, exact storage bounds and the next M8 Linux ABI task. |
| `docs/TOOLCHAINS.md` | Planned path to guest-hosted GCC/G++/Rust/Cargo and optional self-hosting. |
| `docs/validation/M0-M1.md` | Historical local engineering/boot acceptance evidence and input hashes. |
| `docs/validation/M2.md` | Historical local CPU/timer/trap validation evidence and limitations. |
| `docs/validation/M3.md` | Historical local memory/protection/OOM validation evidence and limitations. |
| `docs/validation/M4.md` | Local native app/isolation/teardown results, environment, artifacts and limitations. |
| `docs/validation/M5.md` | Record actual M5 host/QEMU acceptance, resource recovery, inputs, outputs and limitations. |
| `docs/validation/M6.md` | Record actual host/guest/serial/two-boot M6 validation, resource recovery and limits. |
| `kernel/Cargo.toml` | Builds the bare-metal kernel and isolated QEMU failure-injection configurations. |
| `kernel/build.rs` | Reject host-target kernel builds and select the kernel-only linker script. |
| `kernel/src/abi/mod.rs` | Adapt native system queries; task owns process lifecycle and fs owns descriptor calls. |
| `kernel/src/arch/mod.rs` | Select the implemented x86_64 hardware architecture boundary. |
| `kernel/src/arch/x86_64/acpi.rs` | Read only the firmware tables needed by the q35 BSP interrupt baseline. |
| `kernel/src/arch/x86_64/apic.rs` | BSP xAPIC periodic timer, calibrated against the 3.579545 MHz ACPI PM timer. External I/O APIC lines and legacy PICs remain masked until device drivers exist. |
| `kernel/src/arch/x86_64/cpu.rs` | Privileged operations for the single BSP. No FPU/SIMD is enabled here. |
| `kernel/src/arch/x86_64/interrupts.S` | Build uniform saved-GPR frames for CPU traps, IRQs and the native syscall entry; restore the checked frame with IRETQ. Long mode always saves SS:RSP. |
| `kernel/src/arch/x86_64/mod.rs` | Select the x86_64 CPU, paging, interrupt and user-transition implementations. |
| `kernel/src/arch/x86_64/paging.rs` | A bounded BSP-only copy-on-write extension of the retained Limine tables. M3 consumes this mapper and clones every live table into owned frame storage. |
| `kernel/src/arch/x86_64/traps.rs` | Stable Rust uses explicit assembly entries, not the unstable x86-interrupt ABI. All gates clear IF. Kernel traps diagnose, user faults terminate one app; native syscalls and narrowly guarded usercopy faults return through IRETQ. |
| `kernel/src/arch/x86_64/user.S` | Resume saved user GPR/FPU state and use its own trusted SYSCALL stack. Every user trap returns to a single BSP scheduler continuation with IF=0. |
| `kernel/src/arch/x86_64/user.rs` | BSP saved GPR/segment/x87/SSE2 contexts, native SYSCALL and IRETQ transitions. Each user trap returns to the trusted scheduler root/stack; kernel code remains SIMD-free. |
| `kernel/src/boot.rs` | Borrow retained Limine data. M3 allocates only USABLE RAM; bootloader data, original stacks/tables, modules, firmware and the early pool remain reserved. Retained native ELF modules support isolation tests; a tagged archive supplies the M6 root. |
| `kernel/src/fs/mod.rs` | Bridge checked native user buffers to the shared VFS and polled canonical terminal. Calls run on the BSP at IF=0; blocking retains only a copied syscall frame, never a lock. |
| `kernel/src/main.rs` | Boot owned memory/CPU state and run isolated, preemptively scheduled native processes. |
| `kernel/src/mm/lock.rs` | Non-sleeping BSP locks. Order: memory manager before heap; the fixed heap never calls the frame allocator. IRQ/NMI/fatal paths must not take either. |
| `kernel/src/mm/mod.rs` | BSP memory ownership: checked USABLE frames, fully owned table hierarchy, guarded stacks, reclaiming buddy heap and private process memory integration. |
| `kernel/src/mm/uaccess.rs` | Copy bounded user bytes after page-wise checks, recovering only at REP MOVSB. IF=0 and scheduler-only map ownership pin the address space on the sole BSP. No thread shares an address space and no copy spans a scheduling/blocking boundary. |
| `kernel/src/mm/user.rs` | Own private user pages and independently allocated guarded process kernel stacks. BSP mutations occur on the permanent kernel root at IF=0; user roots share upper paths. |
| `kernel/src/mm/validation.rs` | Runtime invariants and isolated QEMU stress/fault validation. |
| `kernel/src/serial.rs` | Single-BSP COM1 output and nonblocking byte input; no heap, locks or firmware. |
| `kernel/src/task/loader.rs` | Load a validated native ET_EXEC into zeroed private pages and build argc/argv. Permission decisions precede activation; physical aliases initialize RX data. |
| `kernel/src/task/mod.rs` | Schedule bounded native processes on one BSP with private roots/stacks and saved CPU state. User traps only capture state; lifecycle, copying and reclamation run on the kernel root. |
| `kernel/src/task/validation.rs` | Verify loader/syscall/fault containment plus transactional process/fork/exec failures. |
| `linker/user-x86_64.ld` | Place native static user text/rodata/data in separate pages below the image limit. |
| `linker/x86_64.ld` | Link the high-half kernel with retained Limine requests and separate RX/R/RW segments. |
| `rust-toolchain.toml` | Pins the compiler, lint/format tools and bare-metal standard-library artifacts. |
| `rustfmt.toml` | Use stable rustfmt options and the same style in editors and on the command line. |
| `user/Cargo.lock` | Generated locked native SDK/application dependency graph; never insert manual comments. |
| `user/Cargo.toml` | Keeps native user ELF builds separate from the bare-metal kernel's target flags. |
| `user/build.rs` | Rebuild native app binaries when their external user-only linker script changes. |
| `user/c/anova.h` | Define native v1 console/process wire layouts and C syscall wrappers without libc/Linux. |
| `user/c/crt.S` | Convert the native startup stack to C main arguments and exit through SYSCALL. |
| `user/c/hello.c` | Demonstrate the C SDK, arguments, BSS/SSE2, scheduling and descriptor calls before exit 7. |
| `user/fs-test/Cargo.toml` | Build the standalone native fs-test application with the shared no_std runtime. |
| `user/fs-test/src/main.rs` | Exercise real VFS syscalls, inherited open offsets, exec/cwd, user buffers and blocking pipes. Child modes validate resources after fork/exec; shell-check reads files produced by its script. |
| `user/hello/Cargo.toml` | Builds the Rust hello ELF that verifies startup arguments, zeroed BSS and SSE2. |
| `user/hello/src/main.rs` | Independent Rust ELF showing argc/argv, BSS and CPU state through native syscalls. |
| `user/init/Cargo.toml` | Build the standalone native init application with the shared no_std runtime. |
| `user/init/src/main.rs` | Start the native shell, wait for child status and restart interactive sessions after exit. Test mode first runs the independent filesystem suite and then a shell script. |
| `user/probe/Cargo.toml` | Builds the native negative-case ELF used by the QEMU isolation acceptance gate. |
| `user/probe/src/main.rs` | Trigger real Ring 3 syscall errors, protection faults and unsafe-return attempts. The kernel acceptance runner supplies one named case and verifies its outcome. |
| `user/process/Cargo.toml` | Builds the real native multiprocess workload used for scheduling/lifecycle acceptance. |
| `user/process/src/context.S` | Keep distinct GPR, all XMM lanes, x87 stack/control and MXCSR sentinels live across syscall-free timer preemption and optional eager fork, then verify them. |
| `user/process/src/main.rs` | Exercise real scheduling, blocking, fork/exec, parent waits and descriptor lifetimes. Distinct CPU sentinels survive syscall-free loops; intentional child faults stay isolated. |
| `user/rt/Cargo.toml` | Packages the no_std startup entry and native v1 syscall wrappers for Rust apps. |
| `user/rt/src/lib.rs` | Native Rust CRT, startup arguments and checked process/file syscall wrappers. Built with the x86_64 user ABI and SSE2, without libc, allocation or Linux calls. |
| `user/rt/src/memory.rs` | Supply freestanding compiler memory intrinsics without linking host libc. |
| `user/shell/Cargo.toml` | Build the standalone native shell application with the shared no_std runtime. |
| `user/shell/src/lib.rs` | Parse a bounded shell subset: four stages, quotes/escapes and per-stage redirection. Commands receive one joined argument string under the existing native v1 exec contract. |
| `user/shell/src/main.rs` | Run the native shell and small multicall tools with fork/exec/wait, pipes and redirection. Keep all parsing, command policy and line assembly in user space, without a libc or allocator. |
| `xtask/Cargo.toml` | Builds host-side dependency, native-app, image, QEMU and evidence commands. |
| `xtask/src/apps.rs` | Build standalone native Rust/C ELF files with user-only target/linker flags. Validate each output and record its explicit ABI identity and SHA-256 manifest. |
| `xtask/src/assets.rs` | Fetch, checksum and locally extract locked bootloader/firmware inputs. |
| `xtask/src/main.rs` | Host CLI for reproducible kernel/app builds, ISO assembly and QEMU validation. |
| `xtask/src/purposes.rs` | Check file-purpose inventory coverage and top-of-file comments for first-party code. Human review still verifies that each explanation describes its actual responsibility. |
| `xtask/src/smoke.rs` | Require real CPU/memory/user/process evidence together with the QEMU exit result. |
| `xtask/src/terminal.rs` | Drive real COM1 input in two ordinary boots, checking line editing, shell EOF and volatile tmpfs. Watchdogs kill only these disposable QEMU guests; a prompt alone is never success evidence. |

## M7 storage and editor files

| File | Purpose |
| --- | --- |
| `crates/platform/src/power.rs` | Validate the restricted reference-QEMU PM1a and constant ACPI S5 handoff, with corruption tests. |
| `crates/storage/Cargo.toml` | Declare the no_std checked sector/ext2 crate and its native error dependency. |
| `crates/storage/src/lib.rs` | Define owned synchronous sector devices, read-only/flush/stop contracts and checked ranges. |
| `crates/storage/src/ext2.rs` | Implement the declared ext2 profile, checked allocation, ordered writes and clean shutdown. |
| `crates/storage/tests/ext2.rs` | Verify on-disk ext2 bytes, indirection, sparse data, exhaustion, reboot and injected I/O failures. |
| `docs/M7_STORAGE.md` | Define PCI/DMA/queue ownership, ext2 bounds, native durability ABI, disk images and source editor. |
| `docs/validation/M7.md` | Record actual M7 host/guest/editor/reboot/error checks, integrity evidence and remaining limits. |
| `kernel/src/drivers/mod.rs` | Hand off validated boot PCI mappings to the one BSP-owned persistent VFS device. |
| `kernel/src/drivers/pci.rs` | Enumerate PCI bridges/functions and admit bounded modern virtio MMIO capabilities/BARs. |
| `kernel/src/drivers/virtio.rs` | Own a coherent split block queue, checked transfers/FLUSH, timeout, reset and DMA quarantine. |
| `user/editor/Cargo.toml` | Build the standalone native serial source editor with the no_std SDK. |
| `user/editor/src/main.rs` | Open/edit bounded ASCII text, retain failed-save buffers and fsync successful saves. |
| `user/fs-test/src/disk.rs` | Exercise real ext2 fd/indirect/sparse/unlink syscalls and verify persisted source/ELF data. |
| `xtask/src/disk.rs` | Create safe regular ext2 images and drive independent reboot/integrity/device-error scenarios. |
| `crates/abi/src/linux.rs` | Translate native errors and supported Linux open flags without contaminating generic subsystems. |
| `kernel/src/task/linux.rs` | Dispatch the static Linux personality, checked wire formats, TLS, memory, process and limited signal operations. |
| `user/linux/probe.S` | Pass the real Linux startup stack to the freestanding ABI probe with SysV alignment. |
| `user/linux/probe.c` | Exercise Linux stack, errno, memory, TLS, fork/exec/env/wait, CLOEXEC, SIGPIPE, random and sleep. |
| `user/linux/musl.c` | Verify unmodified static musl startup, compiler TLS, allocation, errno and actual file I/O. |
| `user/linux/busybox.config` | Select the pinned static BusyBox applet profile from allnoconfig defaults. |
| `xtask/src/linux.rs` | Build verified BusyBox sources, preserve notices/source bundle and trace exact guest ELFs on Linux. |
| `docs/M8_LINUX.md` | Define supported Linux startup/syscalls, bounded semantics, build recipes and explicit unsupported behavior. |
| `docs/validation/M8.md` | Record actual M8 binaries, host traces, guest cases, regression evidence and limitations. |
| `docs/third-party/musl.md` | Preserve the unmodified tested musl package copyright and MIT notices for guest distribution. |
| `crates/abi/src/linux/strings.rs` | Decode page-bounded C strings and pointer vectors through a checked copy callback without allocation. |
| `kernel/src/task/linux/io.rs` | Import Linux writev descriptors before I/O and preserve completed bytes across later buffer faults. |
| `docs/validation/M8_OPTIMIZATION.md` | Record M8 string/stack/I/O improvements, measurable copy counts, boundary tests and regression evidence. |
