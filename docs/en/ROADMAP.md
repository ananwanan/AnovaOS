<!-- Track accepted milestones and planned resource, ext4, desktop and conditional self-hosting acceptance. -->
# Development roadmap and acceptance

**English** | [简体中文](../zh-CN/ROADMAP.md)

Plan reviewed 2026-10-26 against synchronized `dev` at `05a5fb1`. This updates documentation;
it does not rerun the historical gates. M0–M13's bounded offline/online profiles and
M14.1–M14.5 are locally accepted. M15.1/M15.2 reconstruct bzip2; M15.3/M15.4
complete feasibility assessments while compiler reconstruction remains blocked;
M15.5 reconstructs/boots the kernel with host packaging; M15.6 accepts independent
repeated clean builds and exact declared final artifacts. Peripheral IRQ activation
still requires corresponding drivers. See [STATUS](guides/STATUS.md),
[M13](validation/M13.md), [M14.1](validation/M14.1.md), [M14.2](validation/M14.2.md),
[M14.3](validation/M14.3.md), [M14.4](validation/M14.4.md), [M14.5](validation/M14.5.md),
[M15.1-M15.2](validation/M15.1-M15.2.md), [M15.3](validation/M15.3.md),
[M15.4](validation/M15.4.md), [M15.5](validation/M15.5.md) and [M15.6](validation/M15.6.md).
[FIRST_TASK](guides/FIRST_TASK.md) preserves the historical M0/M1 sequence.

## Milestone overview

| Stage | Observable result | Dependencies | Current status |
| --- | --- | --- | --- |
| M0 | Reproducible workspace and host entry points | None | Locally accepted |
| M1 | Reliable UEFI boot/logs | M0 | Locally accepted |
| M2 | Diagnosed exceptions/interrupts/time | M1 | BSP accepted; peripheral IRQs await drivers |
| M3 | Checked pages and heap | M2 | Accepted; boot reservations retained |
| M4 | Isolated native application | M3 | Rust/C ELF sequential baseline accepted |
| M5 | Processes, blocking/user preemption | M4/M2 | One-CPU process baseline accepted; M9 adds threads |
| M6 | VFS/init/shell | M5 | Accepted; tmpfs volatile |
| M7 | Persistent source/apps | M6 | Bounded ext2, normal shutdown accepted |
| M8 | First static Linux CLI | M6/M7 | Selected static CLI accepted |
| M9 | Threads/signals/dynamic musl | M8 | Bounded runtime/PTY accepted |
| M10 | Networking and controlled packages | M7/M9 | Controlled IPv4/TLS/packages accepted |
| M11 | Guest GCC/G++ app development | M7–M9, offline first | Offline C/C++/make accepted |
| M12 | Guest Rust/Cargo development | M11/M9; online M10 | Offline and bounded HTTPS accepted; native std separate |
| M13 | Versioned SDK/packages | M4/M11/M12 | SDK/packages/diagnostics accepted |
| M14 | Rust GUI/C/C++/Rust SDKs | M9/M13 and individual prerequisites | Five stages locally accepted; GPU scope bounded |
| M15 | Optional source/kernel bootstrap | M11–M13 and measured resources | bzip2/kernel/repeats accepted; compiler assessment complete, reconstruction blocked |
| M16 | Resources, backend-independent VFS and controlled ext4 | Existing storage/runtime; restored CI and measured budgets | Partial: backend/cache and controlled ext4 readonly verified locally; opt-in writes, JBD2 and migration/build/UART gates accepted locally; ext4 kernel capacity/recovery accepted locally; complete remote CI pending |
| M17 | Usable development desktop and GUI-only app workflow | M16 accepted workspace/resources; M9/M14 and inherited tools | M17.1–M17.3 locally accepted within declared profiles, including real GUI storage faults; M17.4–M17.5 pending; full compiler bootstrap is not required |
| M18 | Conditional compiler reconstruction and guest-only images | Measured component prerequisites after M16/M17 | Planned; each component accepted independently |

Independent work may proceed in parallel, without skipping actual prerequisites.
Networking, desktop graphics and complete bootstrap do not block first offline app development.

## M0: engineering baseline

Goal: unified build/dependency/diagnostic entry points.

- [x] Minimal kernel/host-xtask workspace with only actual required crates.
- [x] Pin tested Rust and prefer built-in bare-metal target.
- [x] Kernel-only link/panic/early logs; isolate host target.
- [x] Lock Limine/protocol/OVMF/QEMU sources, checksums and download cache.
- [x] `cargo xtask doctor` reports missing dependencies/nonzero exit without changing host setup.
- [x] Define and implement image/run/smoke contracts alongside M1.
- [x] Real format/Clippy/build/host/QEMU CI commands; remote results separate.

Exit: documented clean-environment kernel ELF construction, host tools without bare-metal std,
actionable missing-input failures and recorded versions.

## M1: UEFI, Limine and logging

Goal: a diagnosable boot loop.

- [x] Retain Limine requests/link sections and validate base revision.
- [x] COM1, phase/error panic diagnostics and bounded lock-free fallback.
- [x] Validate map/HHDM/RSDP/framebuffer/modules; optional missing inputs degrade safely.
- [x] Retain boot/map data read-only and report reservations; no M1 allocation/reclamation.
- [x] Generate UEFI image and run q35/OVMF/TCG.
- [x] Implement build/image/run/smoke with disposable firmware variables.
- [x] Timeout, ordered markers and decoded test-device exit results.

Exit: actual version/architecture/boot-complete output; panic/timeout injection must fail.
Screenshots supplement serial/exit evidence.

## M2: CPU, exceptions, interrupts and time

- [x] GDT/TSS/IDT, exception stacks, double-fault IST and guards.
- [x] Registers/addresses/error-bit diagnostics for #PF/#GP/#UD.
- [x] Checked RSDP/XSDT-or-RSDT/MADT/FADT, LAPIC timer and masked IOAPIC routes.
- [ ] Enable/verify peripheral IRQ, MADT source overrides/NMI with the first relevant drivers.
- [x] Calibrate LAPIC with ACPI PM clock; periodic IRQ time and proper EOI.
- [x] Separate trap/IRQ/BSP contexts, IF state and allocation/locking rules in the historical pre-thread baseline.

Exit: consecutive timer IRQs and comprehensible real fault output, with emergency logging surviving
recursive-lock hazards. Pre-user fault injection uses separate test images.
Current contract: [M2_CPU](milestones/M2_CPU.md).

## M3: memory and allocators

- [x] Reserved-range-aware frame allocation/reclamation with invalid/double-free rejection.
- [x] Owned kernel tables/address spaces/permissions, NX/WP where available.
- [x] Distinguish RAM/MMIO/DMA and track cache attributes.
- [x] Enable alloc only after heap creation; explicit OOM diagnostics.
- [x] Check mapping/allocation/address overflow and bounds.

The historical [M3 design](milestones/M3_MEMORY.md) allocates only USABLE RAM, clones all live tables,
uses a 64 KiB double-guard execution stack and originally a 2 MiB fixed buddy heap. DMA32 initially
only constrained addresses; later stages add drivers and raise the heap to 16 MiB.
Exit: stress/reuse/boundaries, intact reservations, actual guard/readonly/NX faults and clean OOM failure.

## M4: user space, ELF and minimal SDK

Goal: host-built first-party apps run as actual user processes.

- [x] Ring 3, private user roots/kernel stack and checked syscall/return.
- [x] Bounded ELF64 ET_EXEC/segments/BSS/stack/argv.
- [x] Versioned native write/exit/query ABI with checked layouts.
- [x] Checked user copies and actual copy-fault recovery; bad user access terminates the app.
- [x] Minimal no_std Rust/C runtime, linker and hello applications.
- [x] Initialize application FPU/SSE; kernel remains SIMD-free.
- [x] Explicit ABI personality inventory, not OSABI guesses.

Exit: independent Ring-3 ELF prints argv and exits; bad ELF/pointers/syscalls/#PF do not corrupt kernel.
[M4 design](milestones/M4_USERSPACE.md) and [evidence](validation/M4.md) cover native v1 modules,
SYSCALL/IRETQ, 11 user cases and eight repeated teardown executions. M5 supersedes sequential
execution; M8/M9 later add Linux ABI. [DEV-001](guides/REQUIREMENTS.md) applies to subsequent files.

## M5: processes and scheduling

- [x] Task/process/PID/context and one thread/guarded kernel stack per original process slot.
- [x] Preserve GPR/selectors/FS/GS/x87/MMX/XMM0–15/MXCSR; disable AVX.
- [x] Single-CPU user preemption, blocking wait/sleep/wakeup, round robin and STI/HLT idle.
- [x] spawn/exec/exit/wait, parent/status/orphan ownership and reclamation.
- [x] Eager-copy fork with private user pages; COW separate.
- [x] Address-space/console-fd ownership across fork/exec/close/exit; VFS offsets arrive in M6.

Accepted exit: two compute processes retain CPU/FPU state, wait/sleep block with HLT idle,
a faulting child does not stop peers, and sixteen lifecycles/32-slot exhaustion/reuse/partial
OOM restore exact frames/heap. [M5 design](milestones/M5_PROCESSES.md), [evidence](validation/M5.md).
The original baseline was 32 single-thread processes including zombies, no kernel preemption/SMP,
explicit native module entry. M9 now shares 32 slots between processes and threads.

## M6: VFS, init and shell

- [x] VFS/mount/path/cwd and file/directory evolution.
- [x] initramfs/tmpfs/devfs and standard descriptors.
- [x] Open descriptions, seek and dup/fork shared offsets.
- [x] Native init/shell and ls/cat/echo commands.
- [x] Native execution/pipes/redirection.
- [x] Edited serial input and terminal boundary, later extended with termios/PTY.

Accepted exit: shell lists/creates/reads files, executes apps/statuses, pipes/redirection close
without EOF deadlocks; tmpfs is explicitly volatile. [Design](milestones/M6_FILESYSTEM.md),
[evidence](validation/M6.md). Historical M6 bounds: boot-only mounts, 32 fd/process, 128 opens,
16×1 KiB pipes, 64 KiB tmpfs files and four-stage shell pipelines. termios/PTY/signals/jobs/Linux
were not part of that historical gate; later stages supersede selected bounds/features.

## M7: persistence and editing

- [x] PCI/modern virtio-blk, DMA buffers and queue lifetime.
- [x] Sector/capacity/error/flush checks.
- [x] Declared ext2 read/write features; reject unknown/unsupported features.
- [x] Ordered data/metadata, sync/fsync, controlled shutdown and image tooling.
- [x] Minimal open/edit/save text editor.
- [x] Disposable default disks separated from explicit persistent test images.

Accepted exit: save source/copy native ELF, clean ACPI shutdown/reboot, byte/source and ELF SHA-256
retention, execute retained disk app, clean e2fsck; bounds/features/readonly/real write/flush failures.
[Design](milestones/M7_STORAGE.md), [evidence](validation/M7.md). `/persist` is the data volume and
root remains initramfs. The historical gate retained 32 single-thread slots/2 MiB heap/masked
peripheral IRQs. Journaling/power-loss recovery remain unsupported despite later capacity increases.

## M8: POSIX foundations and static Linux CLI

- [x] Linux syscall/errno/layout/flags, stack and auxv.
- [x] Trace-driven memory/file/directory/time/random/exec semantics.
- [x] arch_prctl/TLS and required CPU facilities.
- [x] Initial signals/terminal/pipes/redirection/wait/fork/exec.
- [x] Tiny Linux syscall ELF, static musl hello/file and selected BusyBox applets.
- [x] Version/build-options/SHA-256 test inventories.

Exit: pinned static hello/file tools and BusyBox echo/ls/cat plus declared sh pipe/redirection cases;
unsupported syscalls return real errors. Other applets remain unverified.
[M8 design](milestones/M8_LINUX.md), [evidence](validation/M8.md) preserve the historical gate.
M9 adds selected handlers/termios/PTY/dynamic musl; general applets and full shell jobs remain unverified.

## M9: threads, signals, terminals and dynamic musl

- [x] clone/TLS/futex/lifetime/wakeup/deadlines.
- [x] sigaction/masks/delivery/return and declared SA_RESTART behavior.
- [x] poll/ppoll, nonblocking I/O, PTY/termios and basic job-control primitives.
- [x] PT_INTERP/musl, shared mappings/mprotect/dynamic TLS.
- [x] Static PIE loading/self-relocation according to actual build requirements.
- [x] Required `/dev/ptmx` and four `/dev/pts`; no fake `/proc` data.

Exit: dynamic musl/pthread/futex, interrupted calls, declared Ctrl+C and missing-library diagnostics;
no glibc claim. [Design](milestones/M9_RUNTIME.md), [evidence](validation/M9.md) cover 32 shared
process/thread slots, shared anonymous fork pages, actual requeue/deadlines, fallible metadata,
static PIE/startup libraries/dlopen after threads/dynamic TLS, signal restart and reclamation.
Historical M9 excluded altstacks and persistent mmap; M12 later accepts alternate stacks and bounded
readonly file mappings. Robust/PI futexes, realtime queues, full jobs, writable shared-file mapping
and glibc remain outside current acceptance.

## M10: networking and packages

- [x] virtio-net and tested no_std protocol-stack integration.
- [x] IP/TCP/UDP/DNS, blocking/nonblocking sockets and time sources.
- [x] Offline/local then online installation, locked dependencies and verified bytes.
- [x] User-space TLS, CA/time; never bypass certificate verification.
- [x] Package/license/install paths, upgrade/rollback/remove and retained user data.

Exit: controlled peer send/receive/timeout/DNS and package lifecycle; report online scope separately.
Networking does not gate offline GCC. [Design](milestones/M10_NETWORK_PACKAGES.md),
[evidence](validation/M10.md): opt-in polled IPv4 TCP/UDP clients, bounded DNS A, native SDK v1,
whole/per-file hashes, alternating persistent slots, controlled CA/name/date positives/negatives.
Public native repositories, general networking, arbitrary dependency solving and crash-safe ext2
transactions remain outside the profile.

## M11: C/C++ application development in AnovaOS

- [x] Host-construct guest gcc/g++/cc1/cc1plus/assembler/linker/build tools.
- [x] Matching CRT/headers/libc/libgcc/libstdc++/dependencies/sysroot.
- [x] Actual compiler file/mmap/temp/process/signal/thread semantics.
- [x] Static/controlled tool packages first; TCC cannot replace GCC acceptance.
- [x] Edit C/C++ projects and compile/run them in guest.

Exit: C stdio/files/multiple sources; C++ string/vector/RAII plus declared exceptions/threads;
guest source/output persistence. Host compilation followed only by guest execution is insufficient.
[Toolchains](guides/TOOLCHAINS.md), [M11 design](milestones/M11_TOOLCHAIN.md), [evidence](validation/M11.md).
Accepted source-built GCC/G++/binutils/make/musl/CRT/libgcc/libstdc++ also cover malloc/argv,
make incremental behavior, real failures and reboot hashes. General projects, dynamic application
toolchains, LTO/OpenMP/sanitizers and self-bootstrap are excluded.

## M12: Rust and Cargo inside AnovaOS

- [x] Preliminary Linux-musl std cross-build without changing kernel target.
- [x] Host-prepare locked official musl rustc/LLVM/Cargo/std/rust-lld binaries;
  M11 C sysroot is independently source-built, Rust source reconstruction not implied.
- [x] Guest compile/link/run std Vec/String/files/argv applications.
- [x] Locked vendor/Cargo.lock/checksums/configuration for `cargo build --offline --frozen`.
- [x] Independently accept threads/worker-child wait/build.rs/proc-macro .so/C FFI.
- [x] Optional HTTPS registry/Git with strict CA/TLS, controlled negatives, public sources through
  configured host proxy, and full rebuild from retained cache after no-NIC reboot.
- [ ] Native AnovaOS libc/std port; changing target JSON is insufficient.

Exit: guest edit/build/run/persist, no host remote compilation. Report Cargo layers separately;
native target port is independent. [Rust](milestones/M12_RUST.md), [network](milestones/M12_NETWORK.md),
[evidence](validation/M12.md).

## M13: SDK, packages and developer experience

- [x] Version SDK/runtime ABI and stable errors from the M4 foundation.
- [x] Architecture/ABI/dependencies/entry/permissions/minimum capabilities in manifests.
- [x] Cross/guest workflows, logs/fault symbols/resource diagnostics.
- [x] ELF symbols/frame traces; GDB stub/user debugger remain future work.
- [x] C/C++/Rust templates, offline dependencies, install/upgrade/data rules.
- [x] Reserve an independent versioned C GUI ABI/three SDKs, subsequently accepted in M14.4.
- [x] ABI version policy and compatibility regressions.

Exit: a new developer creates/builds/installs/runs/diagnoses/upgrades an app; native/Linux ABI
mismatches reject. Delivered SDK 1.0.0/ANOVAPK2/native templates/static ET_EXEC Linux packages/
bounded frame-pointer diagnostics. Permissions are admission metadata, not a sandbox.
[M13 design](milestones/M13_SDK.md), [evidence](validation/M13.md); GUI acceptance remains separate.

## M14: Rust GUI and three language SDKs

All five stages have local acceptance. Rust user compositor/runtime and one stable C ABI back
C headers/C++ RAII/safe Rust. The kernel supplies checked devices/mappings/IPC/isolation, not
window policy. [M14 GUI design](milestones/M14_GUI.md) defines ownership, errors and negative gates.

### M14.1: checked framebuffer and software drawing

- [x] Checked Limine format/dimensions/pitch/ownership/bounds.
- [x] Pixels/rectangles/bitmaps/clipping/backbuffer/presentation with serial recovery.
- [x] Deterministic actual image/pixel checks, malformed bounds/formats and failure paths.

Exit: repeatable safe software output, not GPU/desktop proof. QEMU 8.2.2 standard VGA
800×600×32: all 480,000 pixels/fixed checksum/readback/heap restoration, no-VGA fallback,
serial recovery and shutdown pass. Host tests cover 24/32-bit RGB/BGR/clipping. No vblank/page flip.
[M14.1 evidence](validation/M14.1.md).

### M14.2: input and pointer

- [x] QEMU i8042 native set-1 keyboard/ID-0 three-byte mouse, physical/repeat/buttons/nine-bit motion.
- [x] 64-event queue, 7×9 clipped cursor, one diagnostic consumer; reset malformed/overflow/detach state.
- [x] Scheduler reads at most 64 bytes/pass with masked IRQs, bounded PM initialization/retries,
  per-device partial-packet expiration.

Exit: ordered actual injected key/motion/button events and cursor response, no real USB claim.
`cargo xtask input-test` accepts standard VGA/no display/no i8042 boots, QMP repeats/extended keys/
buttons, center/four-corner full pixel comparisons, malformed/overflow/reconnect/controller reset
selfchecks, COM1 recovery/shutdown. No text conversion or user focus policy at this stage.
USB/physical hotplug/IRQs/real hardware/GUI SDK excluded. [Evidence](validation/M14.2.md).

### M14.3: user-space window compositor

- [x] Independent Rust compositor and versioned/owned IPC/surfaces.
- [x] Software damage/stack/move/resize/focus/input routing.
- [x] Two isolated apps, rejected buffers/quotas and client/server crash recovery.

Exit: independently owned overlapping/movable windows and routed events; faults spare kernel/peers.
`cargo xtask gui-test`: three profiles, real QMP focus/move/resize, eight full pixel screenshots,
client/server real #UD/reconnect/recovery; bounded copied transport, four connections, one
32×24–320×240 private RGB surface per client, actual heap OOM rollback. Chunk copying provides
no shared surface mappings or atomic multichunk commit. [Design](milestones/M14_COMPOSITOR.md),
[evidence](validation/M14.3.md).

### M14.4: runtime and C/C++/Rust SDKs

- [x] Rust widgets/layout/text/draw/events, graphical terminal and native GUI applications.
- [x] Frozen versioned C ABI: extern C/repr C, fixed widths/opaque handles/status/lifetime/
  thread/callback semantics, no cross-ABI unwind.
- [x] C headers/link library, C++ RAII/safe Rust and independent templates.
- [x] Guest edit/compile/link/run in all three languages; versions/stale handles/invalid inputs/
  fault/resource recovery.

Exit: each independently opens/draws/handles events/closes. Host cross compilation does not
substitute for guest acceptance; actual M11/M12/M13 prerequisites apply.
`cargo xtask gui-sdk-test` passes guest sources/builds/physical input, note/echo-pwd terminal,
ABI/UTF-8/handle/quota negatives, actual user #PF, retained previous output after failed compilation,
and two-boot hashes. Twenty-eight screenshots use independent pixel samples, not full-pixel comparison.
ASCII bitmap/replacement glyphs, process-private clipboard, no PTY/ANSI/dynamic GUI service packages.
[Runtime](milestones/M14_RUNTIME.md), [evidence](validation/M14.4.md).

### M14.5: optional hardware drawing

- [x] Modern virtio-gpu/VirGL bounded render-target clear/readback separately from scanout.
- [x] Modern PCI/controlq, compositor-only API, private DMA, fences and real timeout/reset;
  no raw user commands/shared mappings.
- [x] Actual RTX 4060 Ti/D3D12 renderer, four 64×64 and 7×5 color readbacks, timeout/CPU fallback,
  reset/reinitialization and no-device/2D-only profiles.
- [x] Record QEMU/Mesa/VirGL/renderer, latencies and pixels; no accelerated desktop/shader/guest GL claim.

Exit: at least one proven real GPU drawing path plus normal CPU fallback; scanout alone is insufficient.
The accepted GPU only clears/readbacks offscreen; CPU copies to retained VGA and composes ordinary windows.
No speedup/shader/physical GPU driver/unacknowledged-reset recovery claim.
[GPU contract](milestones/M14_GPU.md), [evidence](validation/M14.5.md).

SMP/per-CPU/TLB shootdown/affinity, physical ACPI/power/NVMe/AHCI/USB, selected glibc and
Wayland/X11/SDL/Qt are independent follow-ups, not implicit completion percentages for M14.
Each needs hardware/dependency/evidence scope.

## M15: optional bootstrap and kernel reconstruction

M15 is independent of already accepted M11–M13 application development. Reuse tested Linux-musl
seeds; native libc/std/SMP/general Linux remain separate. Execute M15.1→M15.2, then M15.3/M15.4/
M15.5 as prerequisites permit, with M15.6 comparing declared reconstructed outputs.
M15.5 may use the accepted guest rustc before rebuilding any compiler. Retain locked inputs,
commands/logs/hashes. Full multistage GCC/Rust/LLVM bootstrap is conditional on feasibility.
Assessments, actual construction and unexecuted levels must remain distinct.

### M15.1: build environment and capacity

- [x] Measure selected target memory/disk/inodes/files/fds/task slots/time.
- [x] Assess/resolve actual resource bottlenecks while preserving images/errors/rollback.
- [x] Independent persistent disk, locked offline inputs/recipe, build/host/target/ABI/sysroot.

The selected bzip2 recipe fits original capacity without enlarging it; excluded optional PostScript
is not a required build input and the HTML manual remains. Current developer disk is 16 MiB/512 inodes,
persistent files 4 MiB, tmpfs/shared caches 1 MiB, VFS 2,048 nodes, heap 16 MiB, 32 slots.
More QEMU RAM does not remove these bounds. [STATUS](guides/STATUS.md), [M12](milestones/M12_RUST.md).
Exit: reproducible target workspace/environment, recorded demand and diagnosable limits; preserve old gates.

### M15.2: reconstruct a small C/C++ tool

- [x] Pin an actual small controlled-dependency tool, source hashes/license/patches.
- [x] Guest compiler performs source build/link/function tests; record subtools/dependencies.
- [x] Persist source/recipe/output; real failure is nonzero and preserves prior usable tool.

Exit: guest source→build→run→save→shutdown→reboot run, exact source/output hashes; no host-prebuilt
replacement. `cargo xtask bootstrap-test` passes three sessions for capacity/reuse, eight-unit guest
bzip2 1.0.8 build, reboot and compiler rejection. Compressed bytes equal official golden sample;
guest/host independently decompress, stopped disks pass e2fsck. Metrics are scheduler-boundary
samples and shutdown storage, not exact peak RSS/transient storage.
[Design](milestones/M15_BOOTSTRAP.md), [evidence](validation/M15.1-M15.2.md).

### M15.3: GCC route

- [x] Independently assess pinned GCC/GMP/MPFR/MPC/build tools/sysroot resources and feasibility.
- [ ] When viable, reconstruct one declared compiler stage and use the new compiler on actual apps.
- [ ] Decide on stage2/stage3; if attempted, record stage relationships/comparisons/differences.

Exit: reviewable assessment, actual probes and remaining conditions; constructed compilers need
functional tests and multistage work needs stage comparisons. Assessment alone leaves compiler
reconstruction/bootstrap unverified. [Design](milestones/M15_GCC.md), [evidence](validation/M15.3.md):
C++11 seed, separate missing arithmetic headers, nine missing tool execs, original configure startup
failure, capacity recovery and reboot hashes. Full archive sizes are not minimum build requirements.
Next: POSIX utilities, arithmetic development inputs and adequate declared source/disk/inode/VFS capacity.

### M15.4: Rust/LLVM/Cargo route

- [x] Independently assess pinned component sources, stage0, build utilities and resources.
- [ ] Progress viable controlled source stages with explicit LLVM/rustc/Cargo and host/target scope.
- [ ] Test reconstructed tools, record inherited seeds/unrebuilt components/blockers/conditions.

Exit: component-specific feasibility and actual probes; reconstructed tools must run their declared
functional tests. Existing M12 binaries are not Rust/LLVM source construction or bootstrap evidence.
[Design](milestones/M15_RUST.md), [evidence](validation/M15.4.md) lock Rust 1.90.0 sources and separately
audit LLVM 20.1.8/rustc/Cargo. Two boots verify std/C++17 seeds, six missing tools, rejected Python
bootstrap entry, unavailable isolated offline curl dependency, retained ELF after real compilation
failure and reboot hashes. Full distribution size is not a minimum source closure. Prescribed 1.89.0
stage0 is audited, not executed; no component rebuilt. M15.5 independently uses accepted guest rustc.

### M15.5: guest kernel construction and reboot

- [x] Guest kernel target/core/alloc/linker/offline graph isolated from ordinary user builds.
- [x] Actual guest compile/link, exact source/config/logs/ELF hash/embedded boot identity.
- [x] Boot guest ELF; distinguish host packaging and unimplemented guest-only image generation.
- [x] Package retained guest ELF without invoking a host kernel rebuild.

Exit: next actual QEMU boot runs that kernel and passes applicable checks; guest compilation,
host-assisted packaging and guest-only image generation reported separately.
`cargo xtask bootstrap-kernel-test` passes build/retained ELF/ISO-byte comparison/new identity/
real Rust rejection/old-kernel retention/recovery across three boots.
[Design](milestones/M15_KERNEL.md), [evidence](validation/M15.5.md). Accepted stripped opt-z kernel has
bootstrap-metrics and no GUI; core/alloc/compiler_builtins are inherited seeds. Host still packages.
Persistent ext2 now supports 4 MiB, tmpfs/shared caches 1 MiB; per-write private indirect-table
snapshots reduce repeated reads without changing disk format or ordered writes.
Guest-only complete image generation remains unimplemented.

### M15.6: repeated builds, comparison and recovery

`cargo xtask bootstrap-repeat-test` is implemented and locally accepted; see
[contract](milestones/M15_REPEAT.md) and [evidence](validation/M15.6.md).

- [x] At least two independent clean builds per declared target with input/seed/environment/log/hash evidence.
- [x] Compare artifacts/functions, retain timestamp/path/identity nondeterminism and host participation.
- [x] Compiler failure preserves usable tools/bootable kernel; verify shutdown/reboot/recovery.

Exit: repeatable recipes, exact declared final artifacts, functional/reboot/failure evidence,
and explicit unimplemented compiler/image-bootstrap boundaries. Both bzip2/kernel rounds and all
12 sessions passed. Kernel intermediate differences remain recorded; entire build directories are
not identical. Two matching outputs prove only the recorded comparison, not universal reproducibility
or seed-free bootstrap. M15 completion reports accepted, assessed and unverified capabilities separately;
it is not required to develop applications inside AnovaOS.

## M16: resources, VFS and controlled ext4

Owner-approved sequence dated 2026-10-26, implementation updated 2026-10-27:
**M16 is partially implemented.** Backend/cache and controlled ext4 readonly pass local
checks; dependency acquisition and the source toolchain build work locally. Opt-in writes,
[JBD2](validation/M16.5.md) and [migration/build/UART](validation/M16.6.md) now pass local gates.
The fresh-ext4 kernel capacity/recovery profile is also locally accepted; complete remote CI is pending.
See [executed evidence](validation/M16.md).
The detailed contract is split into [M16 foundation](milestones/M16_FOUNDATION.md).
Existing ext2 limits describe our implementation subset, not the format's maximum capacity.
Use ext4 as the medium-term work-volume format; retain ext2 for existing images and recovery.
A new AnovaFS format is an independent research project outside this sequence.

| Stage | Planned deliverable | Acceptance boundary |
| --- | --- | --- |
| M16.1 | Recover CI and measure a declared build/workspace profile | Verified dependency inputs; actual downstream gates; memory/storage/inode/task/fd and latency budgets |
| M16.2 | Isolate filesystem backends and expand VFS/resource management | On-demand namespace/cache policy, bounded allocation/reclamation and old ext2 regressions |
| M16.3 | Controlled ext4 read-only backend | Multigroup/extents and admitted metadata checks; malformed/unsupported/dirty-without-replay images rejected |
| M16.4 | Controlled ext4 writes and file semantics | Opt-in write-v1 implemented: create/read/write/resize/rename/unlink/open lifetime and exhaustion/error recovery; [evidence](validation/M16.4.md); default unchanged |
| M16.5 | JBD2 replay and crash consistency | Locally accepted opt-in profile: recorded transaction/flush contract, write-point crash injection and independent filesystem checks |
| M16.6 | New work-volume migration and actual development load | Locally accepted bzip2 profile; default unchanged: copy/hash/reboot verification, retained ext2 fallback and builds with measured foreground responsiveness |

Run M16.1 before capacity choices; complete M16.2–M16.5 before M16.6 promotes a new
default work-volume profile. Unknown incompatible features are rejected rather than silently
downgraded. Do not convert user disks in place. Extra QEMU RAM alone does not remove VFS,
file, cache or fixed-heap bounds. Journal acceptance does not imply that unsynced application
data survives a crash. Record local acceptance and remote CI separately.

## M17: development desktop

M17.1 is locally accepted: three-language input/windows, quota/task/FD reuse, actual client/compositor faults, bounded replacement and five clean persistence/fallback boots. M17.2 now has bounded real-PTY shell/GCC/status/interrupt/resize/scrollback/exhaustion/reboot acceptance; see [M17.2](validation/M17.2.md) and [PTY contract](milestones/M17_PTY.md). M17.3 now has local file/editor and real GUI storage-fault acceptance; M17.4–M17.5 remain pending. See [M17 desktop](milestones/M17_DESKTOP.md) and [M17.1 evidence](validation/M17.1.md).
Implement after the M16 workspace/resource contract, using the accepted guest GCC/G++ and
Rust/Cargo seeds. Compiler source reconstruction is not a prerequisite.

| Stage | Planned deliverable | Acceptance boundary |
| --- | --- | --- |
| M17.1 | Supervised user-space desktop session | Launch/close, window focus/quotas and restartable compositor/session lifecycle |
| M17.2 | Real graphical PTY terminal | Declared Linux-musl shell, build tools, input/output, Ctrl+C, resize, scrolling and explicit ANSI subset |
| M17.3 | File browser and reliable source editing | Real persistent files, save/error handling, declared fsync/rename durability and retained source |
| M17.4 | Logs, resource inspection and task/service controls | Actual status/capacity data, checked task termination and service restart, no fabricated accounting |
| M17.5 | Entirely graphical developer acceptance | C/C++/Rust edit/save/build/run/error-repair/reboot, build-time responsiveness and GUI fault recovery |

The existing echo/pwd graphical demo is not a general shell. Preserve explicit native/Linux
execution admission when connecting the native GUI to tool processes. Application/service faults
should be diagnosable from the desktop; kernel panic, boot or display/input failure still need
COM1/QEMU/host recovery. Screenshots alone do not establish the workflow.

## M18: conditional self-hosting

This is a **planned follow-up**, not a claim that M15's blocked compiler goals are complete.
Continue the original evidence contracts after M16/M17, independently for each route:

- [ ] M18.1: reassess a declared GCC source closure, POSIX tools and GMP/MPFR/MPC inputs;
  construct one viable stage, test actual applications, then decide whether stage2/stage3 is justified.
- [ ] M18.2: independently scope LLVM, rustc and Cargo, including working build utilities,
  selected prescribed stage0 and offline dependency closure; identify every inherited component.
- [ ] M18.3: generate a bootable image inside AnovaOS from the retained guest-built kernel;
  compare packaged bytes and boot identity, documenting any remaining host participation.
- [ ] M18.4: repeat accepted component builds from clean workspaces and test failure/reboot
  recovery, preserving previous tools and kernels and all observed artifact differences.

M18.3 may use existing guest compilers without waiting for M18.1/M18.2. If a compiler route is
still infeasible, retain the assessment and blocker; do not mark construction accepted.
See [M15 GCC](milestones/M15_GCC.md), [M15 Rust](milestones/M15_RUST.md),
[M15 kernel](milestones/M15_KERNEL.md) and [M15 repeat](milestones/M15_REPEAT.md).

## Current follow-up order

Owner update 2026-10-27: defer M16 remote CI closeout and start M17.1 development using the locally accepted bounded M16 foundation. M16 remote acceptance remains open; M17 acceptance still requires its own measured gates.
Advance M18's deeper self-hosting when its component prerequisites permit. Native libc/std,
GUI-enabled kernel reconstruction, SMP, device IRQ/async I/O, USB/NVMe/AHCI, broader GPU APIs
and physical hardware remain independently scoped; none is a blanket prerequisite for M17.
Add new commands only with functioning implementations and evidence. Keep both language trees,
README, status and acceptance boundaries current under [DEV-003/DEV-004](guides/REQUIREMENTS.md).

The additional ext4 kernel-capacity gate and current remote CI execution are tracked
in [M16 capacity](validation/M16_CAPACITY.md), separately from historical bzip2
acceptance. Completion requires actual successful runs and retained evidence.

Current Actions policy is main-push only; complete remote acceptance remains pending under that policy.
