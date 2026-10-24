<!-- Inventory tracked file purposes and explain generated/legal exceptions to source headers. -->
# Repository file purposes

This inventory explains every tracked first-party file and the generated or
third-party exceptions to top-of-file comments. Maintain it with DEV-001;
`cargo xtask check-files` checks coverage and applicable headers.

| File | Purpose |
| --- | --- |
| `crates/graphics/src/virgl.rs` | Encode bounded VirGL clear commands with integer UNORM conversion and independent host reference tests. |
| `kernel/src/drivers/gpu.rs` | Own an optional polled modern virtio-gpu queue, fenced hardware readback, reset and DMA quarantine. |
| `user/compositor/src/gpu.rs` | Exercise the server-only GPU API, guarded readback comparisons and user-selected CPU fallback. |
| `xtask/src/gpu.rs` | Gate exact-device hardware rendering, full scanout comparisons, fault/reset recovery and headless CPU fallback. |
| `docs/M14_GPU.md` | Define the bounded offscreen GPU profile, ownership, user interface, synchronization and fallback. |
| `docs/validation/M14.5.md` | Record actual local hardware rendering, fallback, regression evidence and explicit limits. |
| `.cargo/config.toml` | Routes host commands through xtask and confines kernel code-generation flags to its target. |
| `.gitattributes` | Normalize first-party text to LF while preserving binary images and archives. |
| `.github/workflows/boot.yml` | Gate host/user/kernel checks, graphics/input/native composition and QEMU regressions, retain GUI evidence, and run a separate source-built guest compiler job. |
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
| `crates/elf/src/lib.rs` | Validate bounded native ET_EXEC and Linux ET_EXEC/ET_DYN segments, interpreter paths and TLS before mapping. |
| `crates/elf/src/stack.rs` | Encode bounded native/Linux arguments, environment and executable/interpreter auxv in checked startup storage. |
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
| `crates/vfs/src/fd.rs` | Own fallible shared descriptor tables, close-on-exec state and allocation-free empty placeholders. |
| `crates/vfs/src/lib.rs` | Own bounded memory/ext2 namespaces, explicit ABI inheritance, modes/timestamps, private mapping snapshots, shared opens, pipes and PTYs. |
| `crates/vfs/src/tests.rs` | Test VFS path/mount boundaries, descriptor ownership, private snapshots, modes/timestamps, pipe shutdown and capacity rollback. |
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
| `docs/M14_GUI.md` | Plan the Rust-native GUI, C ABI and C/C++/Rust SDK architecture, security boundaries, five stage gates and limits. |
| `docs/REFERENCES.md` | Primary architecture, protocol, ABI and toolchain reference links. |
| `docs/REQUIREMENTS.md` | Owner-mandated DEV-001 file-purpose rules and DEV-002 native GUI/SDK milestones. |
| `docs/STATUS.md` | Record verified operating-system/SDK profiles, independent graphics stages and the next development task. |
| `docs/TOOLCHAINS.md` | Distinguish guest C/C++/Rust application builds from online Cargo and optional compiler self-hosting. |
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
| `kernel/src/arch/x86_64/traps.rs` | Dispatch kernel diagnostics, native user faults, Linux signals, syscalls and checked usercopy recovery with IF=0 and IRETQ. |
| `kernel/src/arch/x86_64/user.S` | Resume saved user GPR/FPU state and use its own trusted SYSCALL stack. Every user trap returns to a single BSP scheduler continuation with IF=0. |
| `kernel/src/arch/x86_64/user.rs` | BSP saved GPR/segment/x87/SSE2 contexts, native SYSCALL and IRETQ transitions. Each user trap returns to the trusted scheduler root/stack; kernel code remains SIMD-free. |
| `kernel/src/boot.rs` | Borrow retained Limine data. M3 allocates only USABLE RAM; bootloader data, original stacks/tables, modules, firmware and the early pool remain reserved. Retained native ELF modules support isolation tests; a tagged archive supplies the M6 root. |
| `kernel/src/fs/mod.rs` | Bridge checked native user buffers to the shared VFS and polled canonical terminal. Calls run on the BSP at IF=0; blocking retains only a copied syscall frame, never a lock. |
| `kernel/src/main.rs` | Boot owned CPU/memory/devices, optional checked software graphics, and isolated user workloads. |
| `kernel/src/mm/lock.rs` | Non-sleeping BSP locks. Order: memory manager before heap; the fixed heap never calls the frame allocator. IRQ/NMI/fatal paths must not take either. |
| `kernel/src/mm/mod.rs` | BSP memory ownership: checked USABLE frames, fully owned table hierarchy, guarded stacks, reclaiming buddy heap and private process memory integration. |
| `kernel/src/mm/uaccess.rs` | Copy bounded user bytes with page checks and REP MOVSB fault recovery; IF=0 pins shared mappings and no copy spans a scheduling/blocking boundary. |
| `kernel/src/mm/user.rs` | Own shared user roots and reference-counted private/shared pages, permissions and guarded task stacks. |
| `kernel/src/mm/validation.rs` | Runtime invariants and isolated QEMU stress/fault validation. |
| `kernel/src/serial.rs` | Single-BSP COM1 output and nonblocking byte input; no heap, locks or firmware. |
| `kernel/src/task/loader.rs` | Validate and map native/Linux executables and actual musl interpreters, then construct checked startup state. |
| `kernel/src/task/mod.rs` | Schedule bounded native/Linux tasks, suspended vfork parents and shared-root threads; lifecycle, copying and reclamation run on the kernel root. |
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
| `xtask/src/terminal.rs` | Drive ordinary UART editing/EOF, dynamic musl launch, real Ctrl+C interruption and volatile reboot checks. |

## M14.1 software graphics files

| File | Purpose |
| --- | --- |
| `crates/graphics/Cargo.toml` | Build the dependency-free no_std layout and software drawing library for kernel and host checks. |
| `crates/graphics/src/lib.rs` | Validate bounded RGB modes, provide a fixed cursor raster and render clipped pixels, rectangles and blits into caller-owned buffers. |
| `crates/graphics/src/tests.rs` | Check RGB packing, bounds, canaries, overflow, clipping, literal cursor shape and deterministic pattern independently of hardware. |
| `kernel/src/graphics.rs` | Validate display mappings, present/reclaim the boot buffer, manage the diagnostic cursor and copy checked user compositor damage. |
| `xtask/src/graphics.rs` | Verify actual QMP pixels, bounded capture parsing, serial recovery and clean shutdown with and without VGA. |
| `docs/validation/M14.1.md` | Record actual graphics/negative tests, artifacts and the scope of preserved local regression checks. |

## M14.2 input and cursor files

| File | Purpose |
| --- | --- |
| `crates/input/Cargo.toml` | Build dependency-free no_std PS/2 decoding and bounded diagnostic event delivery for kernel and host checks. |
| `crates/input/src/lib.rs` | Normalize physical set-1 keys, three-byte mouse packets, bounded queue/reset/reconnect semantics and clipped pointer state without allocation or GUI policy. |
| `crates/input/src/tests.rs` | Check key/modifier/special-sequence state, full nine-bit motion, malformed/overflow recovery, consumer lifecycle and pointer bounds. |
| `kernel/src/drivers/input.rs` | Own i8042 setup/polling/expiry, hand off exclusive physical events to the admitted user compositor, and retain diagnostic/lifecycle checks. |
| `xtask/src/input.rs` | Inject QMP keyboard/mouse sequences, compare all cursor/background pixels and require missing-device serial recovery and clean shutdown. |
| `docs/validation/M14.2.md` | Record actual polled PS/2/cursor gate results, bounded recovery cases, artifacts and regression limits. |

## M7 storage and editor files

| File | Purpose |
| --- | --- |
| `crates/platform/src/power.rs` | Validate the restricted reference-QEMU PM1a and constant ACPI S5 handoff, with corruption tests. |
| `crates/storage/Cargo.toml` | Declare the no_std checked sector/ext2 crate and its native error dependency. |
| `crates/storage/src/lib.rs` | Define owned synchronous sector devices, read-only/flush/stop contracts and checked ranges. |
| `crates/storage/src/ext2.rs` | Implement the declared ext2 profile, checked allocation, retained modes/timestamps, ordered writes and clean shutdown. |
| `crates/storage/tests/ext2.rs` | Verify on-disk ext2 bytes, indirection, sparse data, exhaustion, reboot and injected I/O failures. |
| `docs/M7_STORAGE.md` | Define PCI/DMA/queue ownership, ext2 bounds, native durability ABI, disk images and source editor. |
| `docs/validation/M7.md` | Record actual M7 host/guest/editor/reboot/error checks, integrity evidence and remaining limits. |
| `kernel/src/drivers/mod.rs` | Own BSP device initialization, persistent VFS handoff and optional scheduler-polled PS/2 input. |
| `kernel/src/drivers/pci.rs` | Enumerate PCI bridges/functions and admit bounded modern virtio MMIO capabilities/BARs. |
| `kernel/src/drivers/virtio.rs` | Own a coherent split block queue, checked transfers/FLUSH, timeout, reset and DMA quarantine. |
| `user/editor/Cargo.toml` | Build the standalone native serial source editor with the no_std SDK. |
| `user/editor/src/main.rs` | Open/edit bounded ASCII text, retain failed-save buffers and fsync successful saves. |
| `user/fs-test/src/disk.rs` | Exercise real ext2 fd/indirect/sparse/unlink syscalls and verify persisted source/ELF data. |
| `xtask/src/disk.rs` | Create safe native/Linux ext2 profiles and drive watchdog-bounded reboot/integrity/device-error scenarios. |
| `crates/abi/src/linux.rs` | Translate native errors and supported Linux open flags without contaminating generic subsystems. |
| `kernel/src/task/linux.rs` | Dispatch bounded Linux calls including vfork, modes/umask and resource queries; delegate memory, thread, signal and terminal layouts. |
| `user/linux/probe.S` | Pass the real Linux startup stack to the freestanding ABI probe with SysV alignment. |
| `user/linux/probe.c` | Exercise Linux stack, errno, memory, TLS, fork/exec/env/wait, CLOEXEC, SIGPIPE, random and sleep. |
| `user/linux/musl.c` | Verify unmodified static musl startup, compiler TLS, allocation, errno and actual file I/O. |
| `user/linux/busybox.config` | Select the pinned static BusyBox applet profile from allnoconfig defaults. |
| `xtask/src/linux.rs` | Build and hash pinned static/dynamic musl/BusyBox fixtures, package upstream sources and record bounded Linux-host traces. |
| `docs/M8_LINUX.md` | Define supported Linux startup/syscalls, bounded semantics, build recipes and explicit unsupported behavior. |
| `docs/validation/M8.md` | Record actual M8 binaries, host traces, guest cases, regression evidence and limitations. |
| `docs/third-party/musl.md` | Preserve the unmodified tested musl package copyright and MIT notices for guest distribution. |
| `crates/abi/src/linux/strings.rs` | Decode page-bounded C strings and pointer vectors through a checked copy callback without allocation. |
| `kernel/src/task/linux/io.rs` | Import Linux writev descriptors before I/O and preserve completed bytes across later buffer faults. |
| `docs/validation/M8_OPTIMIZATION.md` | Record M8 string/stack/I/O improvements, measurable copy counts, boundary tests and regression evidence. |

## M9 runtime files

| File | Purpose |
| --- | --- |
| `crates/memory/src/shared.rs` | Fallible single-thread reference-counted metadata ownership with exact final destruction and allocation-failure tests. |
| `kernel/src/fs/tty.rs` | Bounded COM1/four-PTY line disciplines, input/output queues, settings, foreground signals and hangup. |
| `kernel/src/task/linux/memory.rs` | Translate bounded mmap/brk/munmap/mprotect, real PROT_NONE/W^X, private file backing and shared anonymous pages. |
| `kernel/src/task/linux/signals.rs` | Translate signal actions/masks and addressing, build AMD64 user frames and import checked signal-return state. |
| `kernel/src/task/linux/terminal.rs` | Translate poll/ppoll readiness, terminal ioctls, PTY control and bounded process-group/session interfaces. |
| `kernel/src/task/linux/threads.rs` | Share clone resources, suspend/wake vfork parents and manage futex waits/requeue/deadlines and group/clear-TID lifecycle. |
| `user/linux/m9.c` | Independent musl static/PIE/dynamic Ring 3 fixture for threading, signals, PTYs, exec/fault/exit negatives and UART interruption. |
| `user/linux/shared.c` | Initialized shared-object TLS used by startup linking and dlopen after worker threads already exist. |
| `docs/M9_RUNTIME.md` | Exact M9 ownership, flags, limits, unsupported operations, runtime checksum and build/verification contract. |
| `docs/validation/M9.md` | Measured M9 program hashes, host-trace boundaries, actual guest results and inherited regression evidence. |
| `xtask/src/transcript.rs` | Capture ordered guest stdout, preserve UART evidence and match raw bytes independently of filesystem snapshots or partial UTF-8. |

## M10 additions

| File | Purpose |
| --- | --- |
| `crates/platform/src/clock.rs` | Validate Gregorian UTC dates and Unix epoch conversion, with leap/boundary tests. |
| `kernel/src/arch/x86_64/clock.rs` | Sample stable binary/BCD CMOS UTC and advance realtime from calibrated monotonic ticks. |
| `kernel/src/drivers/network.rs` | Own modern virtio-net split queues, frame/DMA bounds, ownership fences and reset/quarantine. |
| `kernel/src/net/mod.rs` | Integrate bounded smoltcp IPv4 sockets, readiness, fallible buffers and deferred TCP close. |
| `kernel/src/task/linux/sockets.rs` | Translate checked Linux IPv4 socket calls, pointer layouts, deadlines and interruptible frames. |
| `user/linux/network.c` | Execute real Ring 3 TCP/UDP/DNS, timeout, pointer, signal and fd-reuse probes. |
| `user/linux/dns.c` | Resolve bounded DNS A replies with checked compression, randomized IDs, source validation and timeouts. |
| `user/linux/dns.h` | Share the guest DNS resolver and checked reply-parser interfaces. |
| `user/linux/pkg.c` | Install/check native packages, validate hashes/ABI/license and maintain persistent alternating commits. |
| `user/linux/tls.c` | Fetch bounded HTTPS with mandatory CA/name/date verification and exact negative-test reasons. |
| `user/linux/tls.h` | Share the strict user-space HTTPS acquisition interface. |
| `xtask/src/network.rs` | Run controlled loopback peers and watchdog-driven real QEMU network acceptance. |
| `xtask/src/packages.rs` | Pin/build user TLS clients, construct locked packages and verify install/reboot/HTTPS behavior. |
| `tests/guest/m10_https_peer.py` | Serve controlled TLS 1.2 positive/expired fixtures using only Python's standard library. |
| `docs/M10_NETWORK_PACKAGES.md` | Define accepted IPv4/socket, DNS, TLS and package contracts and explicit exclusions. |
| `docs/validation/M10.md` | Record exact M10 build inputs, guest evidence and complete regression results. |
| `docs/third-party/m10.md` | Record locked network/TLS sources, notices, redistribution and fixture-key boundaries. |
| `docs/third-party/network-licenses.md` | Preserve upstream license texts for locked network runtime dependencies. |

## M11 guest compiler files

| File | Purpose |
| --- | --- |
| `ports/gcc/inputs.json` | Lock compiler bootstrap and upstream source URLs/hashes; JSON carries an explicit purpose field instead of comments. |
| `ports/gcc/build.sh` | Enter the source-build recipe from the repository root with strict shell error handling. |
| `ports/gcc/build.py` | Verify archives, cross-build GCC/binutils/make and target runtimes, and verify the complete installed-file cache. |
| `ports/gcc/package.py` | Select the bounded source-built C/C++ sysroot, preserve licenses and hash every packaged file. |
| `user/linux/cc.c` | Set the controlled offline compiler/make environment and exec the real guest tools. |
| `tests/guest/m11/main.c` | Validate guest C stdio, allocation, arguments, file output and multi-source linking. |
| `tests/guest/m11/helper.c` | Supply a separately compiled function changed by the editor for selective make rebuild checks. |
| `tests/guest/m11/main.cpp` | Validate C++ string/vector, RAII, exceptions and a real standard-library thread. |
| `tests/guest/m11/Makefile` | Drive separate C compilation, linking, no-op make and a selective source rebuild inside the guest. |
| `tests/guest/m11/bad.c` | Deliberately fail actual guest C parsing and require a nonzero compiler status. |
| `tests/guest/m11/badlink.c` | Deliberately reference an undefined symbol and require a nonzero guest linker status. |
| `tests/guest/m11/runtime.c` | Probe real vfork/shared-root suspension, deferred signals, snapshots, modes, limits and temporary-file cleanup. |
| `xtask/src/toolchain.rs` | Build/package the optional development ISO and verify guest editing, compilation, make, failures and reboot hashes. |
| `docs/M11_TOOLCHAIN.md` | Define the offline source-built GCC profile, resource/ownership boundaries and acceptance workflow. |
| `docs/validation/M11.md` | Record actual M11 source-build inputs, guest outputs, negative cases and complete regression evidence. |
| `docs/third-party/m11.md` | Record pinned compiler licenses, source obligations and the local-validation distribution boundary. |

## M12 Rust development files

| File | Responsibility |
| --- | --- |
| `docs/third-party/m12.md` | Record official Rust/Cargo/LLVM distribution provenance, licenses and unwinder boundaries. |
| `ports/rust/build.py` | Verify archives, isolate strip scratch output and publish the inventoried guest Rust sysroot. |
| `ports/rust/inputs.json` | Lock official Rust musl binary archives and the vendored itoa registry checksum. |
| `tests/guest/m12/Cargo.lock` | Generated lockfile for the fixed offline guest registry dependency; do not hand-edit. |
| `tests/guest/m12/Cargo.toml` | Define the M12 offline Rust/Cargo acceptance fixture and its declared behavior. |
| `tests/guest/m12/bad.rs` | Deliberately fail guest Rust parsing and verify nonzero status and retained earlier outputs. |
| `tests/guest/m12/build.rs` | Run guest GCC/ar, generate OUT_DIR input and configure Rust FFI linkage. |
| `tests/guest/m12/cargo.rs` | Define the M12 offline Rust/Cargo acceptance fixture and its declared behavior. |
| `tests/guest/m12/config.toml` | Define the M12 offline Rust/Cargo acceptance fixture and its declared behavior. |
| `tests/guest/m12/main.rs` | Define the M12 offline Rust/Cargo acceptance fixture and its declared behavior. |
| `tests/guest/m12/runtime.c` | Exercise invalid return/detached exit, alternate signals, positioned/resized files and OFD locks in a real guest. |
| `user/linux/rust.c` | Configure offline Rust/Cargo execution and the isolated guest sysroot and linker. |
| `xtask/src/rust.rs` | Build Rust images, exercise real guest compilation and independently inspect rebooted source and ELF files. |
| `docs/M12_RUST.md` | Specify the optional Rust/Cargo profile, construction commands, ownership and separate acceptance gates. |
| `docs/validation/M12.md` | Record actual M12 guest compilation, feature-specific evidence and inherited regression results. |
| `tests/guest/m12/helper.c` | Provide the C function compiled by guest build.rs and statically linked through Rust FFI. |
| `tests/guest/m12/macro.rs` | Export a real procedural macro compiled into a guest shared library. |
| `tests/guest/m12/macro/Cargo.toml` | Define the Cargo procedural macro and its runtime/build path dependency. |
| `tests/guest/m12/macro/build.rs` | Generate macro input and environment from the host support crate. |
| `tests/guest/m12/macro/src/lib.rs` | Expand the Cargo macro from generated input and reject unexpected tokens. |
| `tests/guest/m12/macro-support/Cargo.toml` | Define the shared host dependency for the macro and its build script. |
| `tests/guest/m12/macro-support/src/lib.rs` | Supply the edited value used to verify transitive Cargo rebuilds. |
| `tests/guest/m12/use_macro.rs` | Require actual loading/expansion of the guest-built macro and validate its result. |
| `tests/guest/m12/plain/Cargo.toml` | Define an independent dependency-free offline Cargo acceptance project. |
| `tests/guest/m12/plain/Cargo.lock` | Cargo-generated lock for the dependency-free guest project; do not hand-edit. |
| `tests/guest/m12/plain/src/main.rs` | Run an actual guest-built dependency-free Cargo executable and check its output. |

## M12 online Cargo continuation

| File | Purpose |
| --- | --- |
| `docs/M12_NETWORK.md` | Describe the in-progress online Cargo commands, runtime limits, trust inputs and independent acceptance gates. |
| `xtask/src/rust_online.rs` | Verify real guest HTTPS sparse-registry/Git acquisition, strict transport failures and offline reboot cache reuse. |
| `tests/guest/m12_online_peer.py` | Serve deterministic source archives and revision-locked Git smart HTTP with real TLS trust/name/date and transfer-failure fixtures. |
| `tests/guest/m12/online/main.rs` | Run separately downloaded registry and Git dependencies in the guest-built application. |
| `tests/guest/m12/online/Cargo.toml.in` | Instantiate the online guest project with an exact registry version and the generated full Git revision. |
| `xtask/src/rust_public.rs` | Verify public crates.io/GitHub source acquisition, pinned revisions, actual guest compilation and offline reboot reuse. |
| `tests/guest/m12/public/Cargo.toml` | Pin independent public registry and Git copies of itoa without vendor replacement. |
| `tests/guest/m12/public/main.rs` | Run public dependencies linked into the actual guest-built executable. |
| `kernel/src/task/linux/select.rs` | Implement checked select descriptor sets, true readiness, finite waits, remaining time and interruptible blocking. |
| `kernel/src/task/linux/time.rs` | Adapt utimensat/futimesat, dirfd/descriptor paths, NOW/OMIT and checked timestamp precision. |
| `kernel/src/mm/user/file_mapping.rs` | Pin readonly shared-file mappings and refresh changed VFS bytes with physical-page and fork ownership. |
| `crates/vfs/src/file_mapping.rs` | Retain bounded shared-file bytes and version successful writes independently of descriptor lifetime. |

## M13 application SDK and package delivery

| File | Purpose |
| --- | --- |
| `docs/M13_SDK.md` | Define SDK/ABI versions, package policy, developer workflow, diagnostics and future GUI delivery boundaries. |
| `docs/validation/M13.md` | Record actual M13 SDK/package/crash diagnostics and preserved regression evidence. |
| `xtask/src/sdk.rs` | Export a checksummed SDK and create/build independent native C, C++ and offline Rust projects. |
| `xtask/src/debug.rs` | Resolve captured guest fault PCs against exact ELF symbols and verify bounded real guest crash traces. |
| `kernel/src/task/diagnostics.rs` | Copy bounded user frame-pointer chains safely and report captured fault registers and resources. |
| `user/rt/src/v1.rs` | Provide additive native v1 typed errors and owned descriptor wrappers while retaining raw interfaces. |
| `sdk/README.md` | Document the native SDK release, templates, standalone build driver and separate GUI ABI reservation. |
| `sdk/build.py` | Build independent native SDK projects with isolated user flags, offline Rust inputs and explicit ABI metadata. |
| `sdk/include/anova.hpp` | Wrap native C descriptors with move-safe C++ ownership without STL or exception dependencies. |
| `sdk/templates/c/main.c` | Exercise native SDK version negotiation, console I/O and a diagnosed missing-file error in C. |
| `sdk/templates/cpp/main.cpp` | Exercise equivalent native SDK behavior and C++ descriptor ownership. |
| `sdk/templates/rust/main.rs` | Exercise native typed errors and a local offline dependency without Rust std. |
| `sdk/templates/rust/offline-message/Cargo.toml` | Pin the local first-party dependency used by the offline native Rust template. |
| `sdk/templates/rust/offline-message/src/lib.rs` | Supply a no_std message through the template local Cargo dependency. |
| `sdk/tests/contract.c` | Assert native C/C++ syscall numbers, stable errors, structure sizes, alignments and field offsets. |
| `tests/guest/m13/crash.c` | Trigger real native user faults through named stack frames and reject a corrupted frame pointer. |
| `sdk/native-v1.ld` | Link SDK user images with distinct W^X pages and valid nonempty load segments under GNU ld and rust-lld. |
| `xtask/src/app.rs` | Validate external package hashes, build application images, launch isolated disks and verify the public SDK installation workflow. |
| `sdk/templates/rust/Cargo.toml` | Define the exported standalone no_std Rust project with local runtime and offline dependency paths. |

## M14.3 isolated native composition

| File | Purpose |
| --- | --- |
| `crates/gui/Cargo.toml` | Define the no_std internal GUI wire and software composition library. |
| `crates/gui/src/lib.rs` | Expose bounded user-space window policy and the experimental transport contract. |
| `crates/gui/src/wire.rs` | Serialize fixed-width versioned GUI requests/replies without pointers or padding. |
| `crates/gui/src/transport.rs` | Define copied kernel message, hardware-event and checked scanout layouts. |
| `crates/gui/src/policy.rs` | Validate private surfaces and implement damage, stacking, movement, resizing and focus. |
| `crates/gui/src/tests.rs` | Exercise adversarial wire packets, ownership, quotas, damage and routed input as pure logic. |
| `kernel/src/gui.rs` | Authenticate executable/server and process/client identities, copy bounded IPC/scanout and clean up lifetimes. |
| `user/compositor/Cargo.toml` | Build the isolated Rust native software window compositor. |
| `user/compositor/src/main.rs` | Receive authenticated packets, compose private surfaces and route hardware input in user space. |
| `user/gui-common/Cargo.toml` | Share internal native GUI transport helpers across the compositor and test programs. |
| `user/gui-common/src/lib.rs` | Wrap synchronous copied GUI packets and preserve explicit pointer safety contracts. |
| `user/gui-client/Cargo.toml` | Build independent native window clients and adversarial transport fixtures. |
| `user/gui-client/src/main.rs` | Upload private red/blue surfaces, handle focused input and test crash/queue/ownership boundaries. |
| `user/gui-test/Cargo.toml` | Build the native process supervisor for actual GUI lifecycle acceptance. |
| `user/gui-test/src/main.rs` | Spawn/reap isolated windows and compositor sessions, test quotas and retain COM1 recovery. |
| `xtask/src/gui.rs` | Inject actual QMP input and compare native windows, failure recovery and every scanout pixel. |
| `docs/M14_COMPOSITOR.md` | Specify experimental GUI authority, copied surfaces, damage, input and restart semantics. |
| `docs/validation/M14.3.md` | Record actual M14.3 execution, scanouts, fault/OOM recovery and regression evidence. |

## M14.4 Rust GUI runtime and independent SDKs

| File | Purpose |
| --- | --- |
| `crates/gui-runtime/Cargo.toml` | Package the dependency-free user-space GUI C ABI and safe Rust library. |
| `crates/gui-runtime/src/lib.rs` | Implement frozen GUI v1 FFI, bounded widgets, UTF-8 storage and Rust handle ownership. |
| `crates/gui-runtime/src/runtime.rs` | Own copied native RPC, private raster lifetime and ordered loss-reset input delivery. |
| `crates/gui-runtime/src/render.rs` | Clip RGB primitives and rasterize original ASCII glyphs with Unicode replacement boxes. |
| `crates/gui-runtime/src/tests.rs` | Check ABI rejection, lifecycle, rendering, UTF-8, resize and queue-pressure recovery. |
| `sdk/gui/generate.py` | Generate and verify fixed GUI v1 C layouts, capabilities and function declarations. |
| `sdk/gui/include/anova_gui.h` | Declare generated fixed-width GUI v1 C calls and compile-time layout assertions. |
| `sdk/gui/include/anova_gui.hpp` | Transfer GUI window ownership with exception-free C++ move-only RAII. |
| `sdk/gui/README.md` | Specify GUI ABI versions, ownership, errors, linkage and bounded guest SDK usage. |
| `tests/guest/gui/main.c` | Compile an independent native C GUI client with physical key and button handling. |
| `tests/guest/gui/main.cpp` | Compile an independent C++ GUI client and exercise move/RAII ownership. |
| `tests/guest/gui/main.rs` | Compile an independent safe Rust GUI client with equivalent visible behavior. |
| `tests/guest/gui/contract.h` | Exercise real C/C++ version, UTF-8, lifecycle, layout and capacity failure contracts. |
| `tests/guest/gui/contract.c` | Run an independent guest-compiled ABI boundary probe with a real compositor. |
| `tests/guest/gui/fault.c` | Deliberately fault a GUI caller pointer to verify process isolation and cleanup. |
| `tests/guest/gui/note.rs` | Provide a bounded editable Rust GUI note and explicit private clipboard copying. |
| `tests/guest/gui/terminal.rs` | Run keyboard-entered native commands through a pre-GUI helper and render pipe output. |
| `user/gui-link.rs` | Link independent guest client objects with the Rust GUI library, native CRT and panic boundary. |
| `user/gui-run/Cargo.toml` | Package the native compositor/client lifecycle supervisor for guest SDK applications. |
| `user/gui-run/src/main.rs` | Launch isolated GUI processes and require actual expected wait statuses and reaping. |
| `user/linux/gui-build.c` | Run real guest GCC/G++/rustc and publish native GUI executables only after successful linking. |
| `xtask/src/gui_sdk.rs` | Verify guest edits/builds, physical GUI input, literal scanout, faults and retained reboot hashes. |
| `xtask/src/gui_sdk_build.rs` | Export an independent checksummed source SDK and package its guest compiler/display image. |
| `docs/M14_RUNTIME.md` | Specify implemented GUI ABI, rendering, event ownership and native guest build boundaries. |
| `docs/validation/M14.4.md` | Record actual independent language, graphical terminal, fault and reboot validation evidence. |
