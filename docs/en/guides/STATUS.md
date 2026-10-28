<!-- Record verified capabilities, historical acceptance, current blockers and timely bilingual documentation maintenance. -->
# Implementation status

**English** | [简体中文](../../zh-CN/guides/STATUS.md)

## M17.3 files/editor and storage-fault local acceptance (2026-10-28)

`desktop-files-test` passed the original two ordinary editor/PTY boots, and
`desktop-files-fault-test` now passes all 16 cases/43 boots on independent regular
ext4/JBD2 images: exact 16 KiB boundaries and rejected opens/edits, real zero/one
inode/block pressure, real virtio write/flush errors across temporary, backup,
publication and cleanup stages. Genuine error/dirty/saved pixels, exact pending
buffer bytes, target/recovery copies and normal shutdown/reboot contents are
checked; 32 clean shutdown records and all expected recovered copies were audited.
See [file/editor policy](../milestones/M17_FILES.md) and
[executed validation and retained failures](../validation/M17.3.md).

A real cleanup-error case exposed a UI defect: an unnecessary directory refresh
replaced a successful save/cleanup-error notice. The client now preserves that
outcome and clean state, and requests storage inspection because cleanup may have
already removed the backup. Save diagnostics observe actual operations and buffer
bytes without replacing filesystem or device results.

Device errors keep the existing journal mount fail-closed after external fault
removal. Recovery uses an independently copied image and ordinary fresh mount,
explicit pending-byte re-entry and graphical retry; unsaved-buffer crash recovery
and in-save power cuts are unverified. M17.4–M17.5, all-GUI three-language compiler
workflows, compiler-load latency, remote CI and physical hardware remain separate.
Resource quotas and main-push-only Actions policy are unchanged.
## M17.2 real PTY terminal locally accepted (2026-10-27)

`desktop-pty-test` passes the declared two-boot QEMU profile: real Linux-musl ash, physical command entry/backspace, known stdout/stderr contents, child status 23, Ctrl+C status 130, real stopped-child waits, resize/SIGWINCH during 600-line output, 256-line history eviction, EOF/F5 restart, four-PTY/task/global-description exhaustion and exact reclamation snapshots. The native GUI and Linux shell/compiler retain separate explicit personalities. The master is private; creator termination reaps tagged descendants even across setsid.

The graphical shell compiles the selected C source with existing source-built GCC 11.2.0, runs it with status 7 and repeats after clean reboot. Independent e2fsck and exact saved-file/executable/ABI-marker comparisons pass. UI frames upload incrementally, keeping input and PTY I/O active. See [PTY contract](../milestones/M17_PTY.md) and [M17.2 evidence](../validation/M17.2.md) for fixed budgets, actual measurements and retained logs/scanouts. GUI/native v1 and global capacities are unchanged. M17.3 reliable graphical editing is accepted above; broader job control, all-GUI C/C++/Rust acceptance, remote CI and hardware remain separate.

## M17.1 supervised desktop locally accepted (2026-10-27)

The owner deferred M16 remote closeout and authorized M17 on `feat/m17-desktop`. `desktop-test` now passes five QEMU boots: three existing host-built C/C++/Rust GUI clients with physical input/focus/movement, close/relaunch, quotas, actual client/compositor UD2 faults, fresh-session reconnection and the shared three-replacement limit. Direct-child/Linux rejection, real task/FD exhaustion/reuse, independent COM1 restart, exact ext4/reboot/ext2 bytes and missing-device recovery pass. A disconnection-during-render restart defect was found and fixed.

Declared 5,000 ms interaction bounds passed: selected C/C++/Rust gestures took 2,360/1,763/2,988 ms through verified scanout; COM1 recovery peaked at 72 ms. Boundary samples peaked at 625,952 heap bytes, 4,533 additional frames, 52 VFS slots and deliberate 32-task/32-FD pressure. No quotas were raised. See [M17.1](../validation/M17.1.md) for complete retained evidence and limits. Remote CI and hardware remain unverified. M17.2 is now locally accepted as described above; M17.3 now has local bounded file/editor and real GUI storage-fault acceptance; M17.4–M17.5 remain pending.

## M16 foundation progress (2026-10-27)

The fresh-ext4 kernel-capacity gate passed locally at `e2db30f`: real heap/VFS/FD/
task/disk exhaustion and reuse, clean actual kernel construction on the recovered
64 MiB disk, exact ELF packaging, syntax-error retention and two new-kernel boots.
Build peaks were 9,494,480 heap bytes, 142,825 additional frames, 1,564 VFS slots,
14 tasks, 16 FDs/process and 6,072 disk blocks / 241 inodes.
[M16 capacity](../validation/M16_CAPACITY.md) retains the complete measurements,
input identities and raw logs. Run #57 at `1b63f42` passed core, source-toolchain construction and guest C/C++,
then failed ext4 fixture admission due to copied runner UID/GID 1001. Fresh-fixture
ownership is fixed locally (54 xtask tests and strict Clippy pass). Full remote
revalidation is pending: the owner now permits Actions only on `main` pushes,
with PR/manual and feature-branch triggers disabled.


M16 development is consolidated on `feat/m16-foundation`, including the foundation
and M16.4 write-support commits. All remaining M16 work continues on this branch;
pull requests target `dev`.

Based on `dev` `616ff47`, the `feat/m16-foundation` delivery implements verified
source transport fallback/cache admission, a filesystem backend boundary, on-demand
directories, reclaimable persistent file nodes and the controlled ext4 readonly-v1
profile. See [M16 validation](../validation/M16.md) for exact scope and checks.
The default ext2 image, 16 MiB heap, 32 task/fd and 2,048 cache-slot bounds remain.
Opt-in non-journal ext4 write-v1 is implemented; see [M16.4](../validation/M16.4.md).
Opt-in [JBD2 recovery](../validation/M16.5.md) and [fresh-disk migration/builds](../validation/M16.6.md)
are locally accepted for the declared bzip2 profile, including foreground UART budgets.
The declared ext4 kernel capacity is also locally accepted; full remote CI is pending.
Ext2 stays the default.

Plan reviewed 2026-10-26 (Asia/Shanghai) on synchronized `dev` at `05a5fb1`,
which includes M15.6 delivery `16c0ddf`. The acceptance records below describe their
original executed revisions; the M15.6 gate used implementation base `55e3751`
with its retained source snapshot. This documentation audit does not rerun
runtime/QEMU gates or establish remote CI acceptance.
Historical M15 development branch: `feat/m15-bootstrap`. M15.1/M15.2 are locally accepted for the
bounded bzip2 reconstruction profile; see [M15 bootstrap](../milestones/M15_BOOTSTRAP.md) and
[validation/M15.1-M15.2](../validation/M15.1-M15.2.md). M15.3 feasibility is locally assessed;
GCC reconstruction is blocked by current dependencies/capacity. See
[M15 GCC](../milestones/M15_GCC.md) and [validation/M15.3](../validation/M15.3.md). M15.4 has
independent locally verified Rust/LLVM/Cargo source audits and guest prerequisite
probes; reconstruction remains blocked. See [M15 Rust](../milestones/M15_RUST.md) and
[validation/M15.4](../validation/M15.4.md). M15.5 is locally accepted for actual
guest kernel reconstruction, exact retained-ELF host-assisted packaging and
boot/failure/recovery checks; see [M15 kernel](../milestones/M15_KERNEL.md) and
[validation/M15.5](../validation/M15.5.md). See [M14 GUI](../milestones/M14_GUI.md),
[validation/M14.1](../validation/M14.1.md), [validation/M14.2](../validation/M14.2.md)
[validation/M14.3](../validation/M14.3.md), [validation/M14.4](../validation/M14.4.md)
and [validation/M14.5](../validation/M14.5.md).
Remote CI and publication are separate from local acceptance.

## Planned M16–M18 follow-up (2026-10-26)

The owner selected **resources/VFS/ext4 → usable development desktop → conditional
compiler/image self-hosting**. That planning-only change had no implementation; the M16 progress above supersedes
its unimplemented status. M17.1 has since passed its bounded local acceptance as described above; M17.3 now has local bounded file/editor and real GUI storage-fault acceptance; M17.4–M17.5/M18 remain pending and M16 remote CI is pending.

| Plan | Scope | Next prerequisite |
| --- | --- | --- |
| [M16 foundation](../milestones/M16_FOUNDATION.md) | CI/budgets, backend isolation and resources, ext4 read-only, writes, JBD2 recovery and new-work-disk adoption | Finish remote CI; JBD2, migration, bzip2/UART and ext4 kernel capacity accepted locally |
| [M17 desktop](../milestones/M17_DESKTOP.md) | Supervised session, PTY terminal, files/editor, diagnostics and all-GUI C/C++/Rust workflow | Accepted M16 workspace/resources; use inherited guest compilers |
| [M18 self-hosting](../ROADMAP.md#m18-conditional-self-hosting) | Separate compiler component stages, guest-only image generation and repeated recovery | Per-component tools, source closure, stage0/dependencies and measured capacity after M16/M17 |

Ext4 is the planned medium-term work-volume format; retain existing ext2 images
and recovery gates. A new AnovaFS format is outside this critical path. VFS
node/cache policy, heap and task bounds require separate changes whichever disk
format is selected. The historical M14 graphical terminal supports only echo/pwd;
M17.2 separately adds the bounded PTY/ANSI development terminal. Full compiler
bootstrap, SMP and physical drivers are not desktop prerequisites.

Remote baseline checked on 2026-10-26: [run 40116492704 at `05a5fb1`](https://github.com/ananwanan/AnovaOS/actions/runs/40116492704)
completed with `boot=success`, `guest-toolchain=failure`. Its source-build step
could not download the musl.cc bootstrap seed (`curl` exit 28, HTTPS connection
timeout); downstream guest-toolchain, bootstrap and GUI SDK steps were skipped.
This is a host dependency-fetch failure, not evidence of a guest kernel fault.
It does not revoke historical local acceptance or establish remote success.
Recover verified input acquisition and execute the downstream gates in M16.1;
do not weaken checks or infer success from skipped steps.

This planning delivery checks bilingual coverage, language navigation and local
links with `python3 scripts/check_docs.py`, file-purpose inventory with
`cargo xtask check-files`, workspace/user formatting and `git diff --check`.
Those checks passed locally; no kernel/QEMU gate was rerun for the documentation
change. Its subsequent remote CI result must be checked on the resulting commit.

## Documentation maintenance (2026-10-25)

Development documentation now has mirrored `docs/en` and `docs/zh-CN` trees,
with root English/Chinese READMEs and per-language navigation. [DEV-003](REQUIREMENTS.md)
and [AGENTS](../../../AGENTS.md) require same-delivery updates to affected
README, architecture/ABI, roadmap, status, commands and evidence. Completed
features must leave the next-step list; historical measurements and unexecuted
capabilities must retain their scope. The reorganization changes documentation
and its consumers, not the historical acceptance results below.

This batch contains 67 matched document pairs and 143 Markdown files including
root/component READMEs and AGENTS. `python3 scripts/check_docs.py` passes language
coverage/navigation and local file/heading links; isolated negative fixtures reject
missing translations, broken files/fragments and missing language links. The file
inventory covers 422 existing files. `cargo test --locked -p xtask --all-targets`
passes 48 host-tool tests; strict xtask Clippy, workspace/user format checks,
changed Python parsing and workflow YAML parsing pass. These are host/documentation
checks, not a new execution of the kernel or QEMU regression matrix.

Actual `cargo xtask sdk` and `cargo xtask gui-sdk` exports both include English
and Chinese READMEs, whose source bytes and SHA-256 entries were checked. Upstream
musl/network legal blocks and full-length evidence digests are retained across
languages. The CI workflow now runs the local documentation check; no remote CI
execution or publication is claimed for this working-tree update.

## Implemented and locally verified

- M0–M3: Rust 1.90.0, locked Limine/OVMF, q35/TCG UEFI, COM1, BSP tables/ISTs,
  diagnosed faults, calibrated LAPIC timer, owned frames/tables, NX/WP and a
  fixed 16 MiB reclaiming buddy heap. Boot/firmware reservations remain live.
- M4/M5: isolated native Rust/C ELF, checked syscall/usercopy, full enabled
  GPR/x87/SSE2 state, user preemption, blocking wait/sleep/HLT, eager fork,
  transactional exec/spawn, faults/orphans, bounded slots and exact reclamation.
- M6: initramfs/tmpfs/devfs, checked paths/cwd, shared open descriptions and
  offsets, regular files, directories, unlink-open lifetime, 1 KiB pipes,
  redirection, native init/shell and edited COM1 input. tmpfs remains volatile.
- M7: actual PCI/modern virtio-blk queues and owned DMA, checked sectors/errors,
  flush/reset, strict one-group 4 KiB ext2 at /persist, sparse/indirect files,
  append/truncate/unlink, native disk image policy, sync/fsync and clean ACPI S5.
  Source editing, byte/ELF-hash retention across reboot, read-only/unknown-feature
  rejection and real write/flush errors remain covered by persistence tests.
- M8: separate Linux syscall/errno/flag/layout translation, argv/envp/auxv,
  static musl 1.2.4 and selected BusyBox 1.36.1 echo/ls/cat/ash pipelines,
  CLOEXEC, FS TLS, random/time, fork/exec/wait and negative boundary tests.
- M9: Linux pthread/clone threads with shared roots/files/cwd/actions, real
  per-thread TLS, futex wait/wake/bitset/requeue and monotonic deadlines,
  clear-child-tid wake, group exit/faults and thread-exec identity transfer.
- Actual rt_sigframes/rt_sigreturn and x87/SSE restoration, signal masks,
  sender/fault metadata, EINTR/SA_RESTART, ppoll mask restoration and nanosleep
  remaining time. Malformed sigreturn ends the user group without a kernel fault.
- poll/ppoll and O_NONBLOCK, shared file-status flags, four actual PTY pairs,
  slave locks, raw/canonical input, termios/window ioctls and foreground Ctrl+C.
  Ordinary UART sessions also launch dynamic musl and interrupt a blocked read.
- Actual ET_DYN/static PIE and PT_INTERP musl loading, correct executable/interpreter
  auxv, initramfs file mappings, mprotect/PROT_NONE/W^X, shared anonymous fork
  pages, startup shared objects and dlopen/dynamic TLS after thread creation.
  Missing interpreters and libraries fail with explicit diagnostics.
- Fallible Shared control blocks for roots, backing, fd tables and dispositions;
  actual complete heap exhaustion verifies fork/spawn rollback and failed exec
  retention with exact page/heap recovery. Futex backing identities never reuse
  a freed allocation's key. No user reference or lock survives a switch.
- M10: opt-in modern virtio-net with separately owned RX/TX DMA, smoltcp 0.12.0,
  bounded IPv4 TCP/UDP client sockets, VFS fd inheritance, nonblocking/poll,
  timeouts, signals, UDP pointer/MTU negatives, bounded reuse and reset teardown.
- User-space DNS A with actual controlled replies, compression/ID/source bounds
  and timeouts; validated CMOS UTC and monotonic deadlines.
- User-space Mbed TLS 2.28.10, actual HTTPS and mandatory CA/hostname/date checks;
  negative tests inspect the real X.509 reason. Test keys never enter images.
- Actual native package install/upgrade/remove, archive/per-file SHA-256,
  ABI/SDK/license/path validation, inactive-slot failure rollback, clean reboot
  retention, installed ELF execution and preserved user data. Online acquisition
  is restricted to the controlled host peer and generated test CA.
- DEV-001 remains mandatory: English purpose headers plus the checked
  [file inventory](FILE_PURPOSES.md). Historical M14.5 command results are in
  [validation/M14.5](../validation/M14.5.md).
- Historical M12 acceptance recorded 83 host tests, host/kernel/user Clippy,
  formatting and all 13 QEMU scenarios passing. Ordinary UART and persistence/error sessions consume ordered stdout
  directly; diagnostic-log snapshots cannot invalidate their matching cursors.

| Capability | Status |
| --- | --- |
| M0–M8 engineering, isolated apps, VFS, storage and selected static Linux CLI | Locally accepted on q35/TCG |
| M9 pthread/futex/signals, readiness, PTYs and dynamic musl | Locally accepted within M9_RUNTIME |
| Total scheduler pool | 32 process/thread/zombie slots, one BSP |
| Stacks and heap | 64 KiB guarded trap stacks; 256 KiB guarded scheduler stack; fixed 16 MiB heap |
| VFS namespace | 2,048 nodes; 32 fds per task/table; 128 open descriptions; four PTY pairs |
| ext2 | Normal shutdown persistence; no journal, atomic save or power-loss guarantee |
| Per-thread alternate signal stacks and SA_ONSTACK | Locally accepted in the bounded M12 runtime |
| Full terminal jobs, realtime queues, robust/PI futexes | Not implemented or verified |
| COW, writable shared-file mmap/writeback, ASLR, SMP and physical hardware | Not implemented or verified |
| M14.3 isolated native software compositor/windows | Locally accepted within M14_COMPOSITOR |
| M14.4 Rust GUI runtime and guest-built C/C++/Rust SDK applications | Locally accepted within M14_RUNTIME |
| M14.5 bounded hardware VirGL clear/readback and CPU fallback | Locally accepted within M14_GPU; ordinary desktop remains software-rendered |
| M10 IPv4 client networking and controlled native packages/TLS | Locally accepted within M10_NETWORK_PACKAGES |
| M11 offline GCC/G++ 11.2.0, binutils, make and matching musl/C++ sysroot | Locally accepted within M11_TOOLCHAIN |
| M12 guest Rust 1.90.0/std, offline Cargo/vendor, build.rs/C FFI and the fixed Cargo macro graph | Locally accepted within M12_RUST |
| M12 opt-in HTTPS Cargo sparse-registry/Git and persisted-cache rebuilds | Locally accepted within M12_NETWORK; public sources through a configured proxy |
| General Linux networking/glibc and public native package repositories | Not implemented or verified |
| External IRQs, asynchronous block I/O, hotplug and boot-memory reclamation | Not implemented |

The BSP polls block I/O with a two-second deadline; long kernel I/O delays tasks
and delivered IRQ time. Root remains read-only initramfs and plain run uses a
throwaway disk. Persistent files need an explicit image and clean guest shutdown.
The editor's truncate/write save is not atomic. M9 does not change these limits.

## M11 implemented and locally verified

The following construction is retained; M12 supersedes the Linux size/startup
bounds recorded here. Native limits remain separate.

- Optional development ISO with source-built GCC/G++ 11.2.0, binutils 2.37,
  make 4.4.1 and musl 1.2.4/CRT/libgcc/libstdc++ 11.2.0 at `/tc`.
- Actual UART editing, guest C stdio/malloc/argv/files and multi-source linking;
  C++17 string/vector, RAII/exception unwinding and a real std::thread.
- make's no-op and selective incremental build, real syntax/link failures,
  temporary-file cleanup and source/ELF SHA-256 equality across clean reboot.
- Suspended-parent vfork with shared root and deferred parent handlers;
  failed exec/exit wakeups, private tmpfs/ext2 snapshots and preserved offsets,
  regular-file modes/umask, resource queries and persisted modification times.
- Bounded 64 MiB Linux ELF files, 128 MiB loaded image, 128 MiB brk and
  512 MiB mmap window. Linux stack 1 MiB, scheduler stack 256 KiB, heap 16 MiB,
  tmpfs file 1 MiB, VFS 2,048 nodes. Startup vectors use 64 entries, 1,023-byte
  strings and 8 KiB per-vector payload with checked 16 KiB startup storage.
- Source archive locks, complete installed-file inventory verification,
  per-file/archive hashes and original toolchain licenses. Ordinary disk and
  initramfs policy remain explicit; development disks declare Linux personality.
- Development Rust builds use opt-level 1 with debug information. Kernel
  membership checks use owned lower-root page-table queries without relaxing
  user bounds, guard pages or W^X. All prior ownership/OOM gates remain required.

## M12 implemented and locally verified

- Optional `/rs` image with checksum-locked official Rust 1.90.0 musl rustc,
  embedded LLVM, rust-lld, Cargo, std and vendored itoa 1.0.15. These upstream
  binaries are distinct from the source-built M11 C tools; Rust/LLVM source
  construction and self-bootstrap are not established.
- Guest UART source editing, std allocation/argv/files, real threads and child
  output/wait from a worker thread; a guest-built procedural macro shared object.
- Dependency-free Cargo and frozen builds with locked vendor checksums, build.rs generated
  input/environment, real guest GCC/ar and C FFI, no-op and source-change builds,
  real syntax failure, exact persisted source/ELF hashes and clean reboot.
- A fixed Cargo procedural-macro graph with a shared host/build dependency,
  generated source/environment, dependency-change expansion from 42 to 84,
  macro-error status 101 with retained prior output and successful source repair.
  All 23 saved source/ELF files remain identical across clean reboot.
- Linux development disks allocate 512 inodes within the existing 16 MiB
  one-group profile; the accepted Cargo build uses 146 allocated inodes.
  Native disks keep their 128-inode profile and existing images are not resized.
- Actual per-thread sigaltstack/SA_ONSTACK delivery/return and rejected changes,
  musl posix_spawn's shared-root vfork variant, thread-group child waits,
  detached-thread self-unmap exit and forced invalid-return fault handling.
- Bounded AF_UNIX sequenced-packet pairs for Rust pre-exec error channels, with
  duplex message boundaries, backpressure, truncation, checked copies and EOF.
- Positioned file I/O, bounded ftruncate, regular-file rename with open replaced
  inode lifetime, OFD flock and pipe FIONREAD/FIONBIO. Directory rename is
  limited to an absent destination in the same parent. Empty-directory removal
  retains open identities and reclaims on final close; live cwd/removed-child
  references return EBUSY. Relative dirfd open/stat/unlink and O_NOFOLLOW are
  checked independently. Writable shared-file mmap/
  writeback remains unsupported; readonly coherent mappings are described below.
- Linux ELF/loaded image and per-mmap bounds are 256 MiB; the mapping window is
  `[0x50000000, 0xc0000000)`. A committed per-root gap hint avoids rescanning large
  immutable compiler mappings. Linux startup uses 256 argv/env entries and a
  checked 32 KiB block; native startup remains 64 entries. Linux/VFS paths and
  names support 512/255 bytes; native v1 path/directory wire capacities stay
  separate and reject unrepresentable names without consuming them.
- The Rust QEMU profile uses 3 GiB RAM. One BSP, 32 task slots, 16 MiB kernel
  heap, 1 MiB tmpfs files/Linux stacks, 4 MiB persistent files, 256 KiB scheduler stack and
  retained boot memory remain bounded. See M12_RUST for exact scope.

## M12 online Cargo implemented and locally verified

- Opt-in `cargo-online` uses the official unmodified Cargo 1.90.0, strict TLS,
  the pinned public CA store and `/persist/cargo-home`. Ordinary Cargo remains
  offline by default; explicit environment/project settings are preserved.
- Controlled HTTPS sparse-registry and Git smart-HTTP acquisition, full-revision
  and crate checksum checks, actual CA/name/expiry failures, corrupt/truncated
  transfer rejection, timeout diagnostics, repaired retry and guest compilation.
- Public crates.io itoa 1.0.15 and GitHub dtolnay/itoa at the pinned full revision,
  without source replacement. Local public acceptance uses the documented host
  proxy/public CA configuration; direct public DNS/transport is not established.
- Both gates inspect persisted source/cache/lockfile/ELF hashes, require e2fsck,
  and recompile dependencies into a fresh target directory after an independent
  boot without a peer or NIC. Host peers/proxy transport sources only.
- Bounded select with real readiness, monotonic deadlines, checked sets and
  signal EINTR; actual utimensat/futimesat and persisted ext2 timestamps.
- Coherent readonly file-backed MAP_SHARED with retained inode ownership across
  fork, close, unlink and name replacement; real readonly write fault and kernel
  heap OOM/reuse. Writable shared-file mappings and truncating live mapped files
  are rejected. Nested ext2 cache directories import iteratively within the node
  bound instead of the former eight-level recursion bound.
- Historical M12 acceptance recorded 83 host tests, 228-file purpose coverage,
  strict host/user/kernel Clippy, formatting, all 13 QEMU scenarios and every
  earlier terminal, storage, network, package and C/C++/Rust gate passing. Complete results and remaining resource limits
  are in [validation/M12](../validation/M12.md) and [M12_NETWORK](../milestones/M12_NETWORK.md).
  The deterministic online gate is included in CI; remote execution is separate.

## M13 implemented and locally verified

- Source SDK 1.0.0 freezes native v1 numbers/layouts/errors and exports independent
  C/C++/Rust templates with normal user target flags. Rust uses a locked local
  dependency and typed errors; C++ demonstrates move-safe descriptor ownership.
  Runtime argument helpers follow the existing 64-entry/1,023-byte native limit.
- ANOVAPK2 records ABI, architecture, entry, bounded dependency minima and
  permission/capability admission. It validates hashes and executable metadata
  before staging. Native/Linux disk mismatches, including legacy native v1 on
  Linux disks, are rejected. The VFS pins the mounted ABI marker against writes,
  truncation, chmod, unlink and replacement/rename.
- Actual installed C/C++/Rust and static musl Linux execution, 31 rehashed parser
  negatives with no-write controls, a rebuilt C upgrade, staged-write failure,
  downgrade rejection, four boots/e2fsck checks and retained user data.
- Public create/build/pack/image/run commands allow a new developer to install
  their own externally hash-locked package. An actual guest application returns
  the stable missing-file error and status 0 through the public image path.
- Captured hardware faults produce a bounded checked-usercopy frame trace,
  registers and global resource counters. Host symbolization of the exact debug
  ELF resolves five actual function/source locations; corrupt RBP stops safely
  and the shell continues. No GDB stub or general DWARF unwinder is provided.
- The independent GUI C ABI/C++/Rust companion SDK was subsequently accepted in
  M14.4; its evidence remains separate from M13. Permissions are admission metadata, not process sandboxing. The package
  Linux profile is static ET_EXEC; the broader M9 runtime remains separately
  verified. See [M13 SDK](../milestones/M13_SDK.md) and [validation/M13](../validation/M13.md).

## Next task

The next task is M17.4 observations and controlled recovery, followed by M17.5 all-GUI compiler workflows. M17.1–M17.3 passed their bounded local profiles; M16 remote CI remains paused by owner request.

The independently deferred compiler route starts with the measured POSIX build-utility,
arithmetic dependency and storage/inode/VFS capacity blockers, followed by a
scoped first compiler-source stage. Guest-only image generation, native libc/std,
GUI-enabled kernel rebuilding and broader graphics/runtime/hardware support
remain independent unverified follow-ups. M15.6 repeat builds are already
locally accepted and are not the next unimplemented milestone.

The following paragraphs preserve the accepted stage details and explain the
remaining boundaries.


The five M14 stages now have separate local acceptance records, with M14.5
limited to hardware offscreen VirGL clear/readback and CPU display copying.
Shader drawing, GPU window composition and broader device support remain
independent extensions; the ordinary desktop uses its accepted software path.
M15 is an independent optional roadmap stage. M15.1 and M15.2 now have local
acceptance for official bzip2 1.0.8: a locked source-only work disk, sampled
resource observations, actual guest compilation, compression/decompression,
limit recovery and identical source/output hashes after reboot and compiler
rejection. See [M15 bootstrap](../milestones/M15_BOOTSTRAP.md) and its
[local evidence](../validation/M15.1-M15.2.md). M15.3 now has a reproducible locked-source
assessment and two-boot guest probe:
C++11 seed compilation, separate missing arithmetic headers, nine missing tool
exec errors, upstream configure startup rejection, capacity recovery and exact
persisted hashes. See [M15 GCC](../milestones/M15_GCC.md) and [local evidence](../validation/M15.3.md).
The GCC route next needs actual POSIX build utilities, arithmetic development
inputs and a declared source closure with sufficient large-file/disk/inode/VFS
capacity before the first compiler stage. M15.4 now separately audits the
complete official Rust source distribution and LLVM/Cargo/vendor components,
records the prescribed 1.89.0 stage0, and verifies real guest Rust std/C++17
seed applications, six missing tools, Python entry exec rejection, an isolated
offline curl dependency failure and retained ELF/workspace bytes after compiler
failure and reboot. See [M15 Rust](../milestones/M15_RUST.md) and
[local evidence](../validation/M15.4.md). Source reconstruction remains blocked;
no LLVM/rustc/Cargo component or prescribed stage0 execution is verified.
The broader Rust regression also corrected ext2 cached timestamp precision to
match persisted whole seconds, preserving tmpfs nanoseconds and the original
exact Cargo dependency-rebuild assertions. Focused host/guest timestamp checks
and the full offline Rust/Cargo reboot gate passed; see the same validation
record for the initial failure and correction.
M15.5 now has three-boot local acceptance for real guest Cargo/rustc/rust-lld
kernel construction using exact sources, locked offline crates and inherited
bare-metal target libraries. The retained 349,464-byte kernel is packaged
without rebuilding it on the host, independently extracted from the ISO for
byte comparison and booted with its matching embedded identity. Actual Rust
syntax rejection preserves the old kernel, and another recovery boot preserves
all 181 workspace files. Each stopped disk passes e2fsck. See
[M15 kernel](../milestones/M15_KERNEL.md) and [local evidence](../validation/M15.5.md).
The persistent file ceiling is now 4 MiB within the same ext2 format; tmpfs and
readonly shared caches stay at 1 MiB. Per-write private indirect-table snapshots
reduce the counted 2 MiB/512-byte-append fixture from 16,211,368 to 175,376 sector
reads while preserving ordering and rejection/recovery behavior.
M15.6 now has local acceptance from `cargo xtask bootstrap-repeat-test`: two
independent clean bzip2 builds and two kernel builds, retained per-round evidence,
matching declared inputs, byte-identical bzip2 outputs/final kernel ELFs and all
12 failure/reboot/recovery sessions passing. The complete bzip2 workspace file
inventory matches; ten kernel log/cache/generated/intermediate files differ,
including reordered smoltcp configuration constants. Raw differences are retained;
this accepts the declared final artifacts, not all kernel intermediate bytes.
See [M15 repeat](../milestones/M15_REPEAT.md) and [validation/M15.6](../validation/M15.6.md). Compiler
reconstruction, guest-only image generation and GUI-enabled kernel rebuilding
remain unverified; the original M15.3/M15.4 audits retain their historical
1 MiB capacity profile rather than claiming to reassess a pruned source route.
Preserve `cargo xtask bootstrap-test`, `bootstrap-gcc-test`, `bootstrap-rust-test`,
`bootstrap-kernel-test`, `bootstrap-repeat-test`, all four source rejection suites and the `bootstrap-metrics` kernel check,
as well as every previous runtime/storage/network, C/C++/Rust/Cargo, native SDK
and graphical gate, plus `gpu-test` and headless `gpu-fallback-test`.
Native libc/std, general Unicode/IME, terminal behavior beyond the M17.2 ANSI subset,
shared clipboard policy, dynamic GUI package-service admission, physical
hardware and broad Linux GUI compatibility remain independently scoped.

### M15 bootstrap status

| Planned stage | Intended result | Current status |
| --- | --- | --- |
| M15.1 | Measured capacity, sufficient build environment and persistent work disk | Locally accepted for the original bzip2 1.0.8 capacity profile |
| M15.2 | Controlled C/C++ tool built in guest and executed again after reboot | Locally accepted: guest-built bzip2, golden sample, reboot and failure recovery |
| M15.3 | GCC feasibility evidence, then a compiler stage or multistage bootstrap if viable | Assessment locally verified; current profile blocked; no compiler stage rebuilt |
| M15.4 | Separate LLVM/rustc/Cargo feasibility and source builds where viable | Assessment locally verified; current profile blocked; no component rebuilt |
| M15.5 | Guest-built kernel actually booted; image-generation scope reported separately | Locally accepted: guest ELF, exact host-assisted packaging, boot identity and failure/recovery; guest-only image generation unverified |
| M15.6 | Repeated clean builds, artifact comparison and failure recovery | Locally accepted for two clean bzip2/kernel builds, exact declared artifacts and 12 boot/recovery sessions; kernel intermediate differences retained |

Complete M15.1 before M15.2; then advance M15.3, M15.4 and M15.5 as their
actual prerequisites permit. M15.5 may use the already accepted guest rustc;
compiler source bootstrap is not its prerequisite. Full GCC and Rust/LLVM
bootstrap remain conditional on feasibility, with unexecuted capabilities
explicitly unverified. Retain input manifests, commands, logs and hashes at
each stage, and consolidate the declared scope in M15.6.

The current Rust development disk is 16 MiB with 512 inodes; persistent files
are bounded at 4 MiB, tmpfs/shared caches at 1 MiB, and the VFS has 2,048 nodes. The 16 MiB kernel heap and
32 task slots remain bounded as documented above. Additional QEMU RAM alone
does not remove these limits. The selected M15.1 bzip2 target fits without
capacity changes; larger targets still require independent measurements and
real blocker removal. Boundary samples are not exact peak RSS; disk usage is
measured at clean shutdown. Separate exhaustion probes verify EFBIG, EMFILE
and EAGAIN rejection and subsequent resource reuse.
Guest kernel compilation, host-assisted image packaging and image generation
inside the guest require separate evidence. Two matching builds establish
only the recorded comparison, not general reproducibility or seed-free bootstrap.

## M14.1 software display — locally verified (2026-10-23)

The project owner approved [DEV-002](REQUIREMENTS.md): a Rust-native GUI
runtime, user-space window compositor/desktop and stable versioned C ABI
with C, C++ and Rust application SDKs. The detailed architecture, security
constraints, candidate dependency policy and five independent gates are in
[M14_GUI](../milestones/M14_GUI.md) and [ROADMAP](../ROADMAP.md):

| Planned stage | Intended observable result | Current status |
| --- | --- | --- |
| M14.1 | Validated framebuffer with bounded CPU/software graphics | Local QEMU gate passed |
| M14.2 | Keyboard/pointer events and visible cursor | Local QEMU gate passed |
| M14.3 | Isolated user-space compositor; two windows and correct focus | Local QEMU gate passed |
| M14.4 | GUI runtime and C/C++/Rust SDK demos compiled/run in guest | Local QEMU gate passed |
| M14.5 | Bounded hardware GPU clear/readback with CPU fallback | Local three-boot QEMU gate passed |

The optional graphics feature validates the reserved framebuffer span and each
supervisor RW/NX WC mapping before writing. A dependency-free `no_std` library
provides clipped pixel/rectangle/blit primitives, checked 24/32-bit RGB packing
and a temporary caller-owned back buffer. The actual standard-VGA screenshot's
480,000 pixels and fixed checksum match the reference; full device readback and
heap restoration also pass. Another boot without VGA retains the COM1 shell.
Both boots execute a recovery command and shut down cleanly. See
[validation/M14.1](../validation/M14.1.md) for commands, artifacts and the exact
regression scope. `graphics-image`, `graphics-test` and `run --graphics` exist.

This M14.1 diagnostic alone provides no user-space input routing or windows.
M14.3 composition is delivered separately below. The GUI SDK and bounded GPU profile are accepted separately under M14.4/M14.5;
the boot framebuffer diagnostic alone is not a window system. CPU presentation has no
vblank/atomic page-flip guarantee. Only QEMU 800×600×32 is accepted as a scanout;
other pixel formats are host-tested. M12 supplies the bounded Rust/Cargo prerequisite and M13
supplies SDK/package delivery; M14.4 has its own separate GUI developer acceptance below. SMP, physical
devices and Linux GUI compatibility remain separately scoped workstreams.

## M14.2 polled PS/2 input and cursor — locally verified (2026-10-23)

The optional `graphics` profile now owns the QEMU i8042 keyboard and standard
ID-0 relative mouse. Initialization explicitly selects native scan-code set 1,
checks controller sources and device ACKs, and bounds PM-timer waits/retries.
The BSP scheduler polls at IF=0, reading at most 64 bytes and draining at most
64 events per pass. IRQ1/12 remain masked; per-device incomplete sequences expire
before late bytes are accepted. No input or cursor allocation uses the heap.

A dependency-free `no_std` decoder separates physical press/release/repeat and
extended keys from text. It handles Pause/PrintScreen, modifiers, full nine-bit
mouse axes and three button transitions. Malformed input, queue overflow and
consumer detach/reconnect discard stale history and invalidate held state.
A fixed diagnostic target is the only consumer; application focus and routing
remain M14.3 user-space responsibilities.

The 7×9 cursor saves/restores at most 63 packed device pixels, including unused
channels, and clips every framebuffer access. The boot pattern stays unchanged until a pointer event. The implemented
`cargo xtask input-test` runs standard-VGA, no-VGA and i8042-disabled boots. It
injects ordered QMP key/button/motion events, compares all 480,000 pixels at
five cursor positions and requires COM1 recovery plus clean ACPI shutdown.
Test-only selfchecks exercise overflow, malformed input, consumer disconnect/
reconnect, independent PM-timer partial-sequence expiry and actual controller-port
disable/reinitialization.

The local three-boot gate passed; [validation/M14.2](../validation/M14.2.md)
records actual acceptance and regression scope independently. At that stage, host
coverage was 149 tests, including 22 pure input tests, the pinned cursor raster
and host input-gate negatives. USB, physical hotplug, external IRQ delivery,
real-hardware input, graphical text/terminal, compositor and GUI ABI/SDK remain
outside this M14.2 profile. M14.3 and M14.4 are accepted separately below.

## M14.3 native software compositor — locally verified (2026-10-23)

A separate Rust native process owns bounded software surfaces, window stacking,
damage, move/resize, visibility, focus and physical input routing. The kernel
admits only the immutable compositor executable for display/input authority and
provides authenticated copied messages with four process-owned connections.
Each client owns one private 32x24 through 320x240 RGB window; no client obtains
video memory or another process's mappings. The 800x600 compositor fits the
existing 4 MiB native ELF limit. Native v1 remains frozen; this experimental
transport is separate from M14.4's accepted public GUI library ABI.

`cargo xtask gui-test` passed standard-VGA/input, no-VGA and no-i8042 boots.
Eight full screenshots compare 3,840,000 pixels against an independent host
reference. Two real isolated clients receive focused QMP keyboard/pointer input,
move, resize and redraw after occlusion. Both client and compositor separately
execute UD2, with exact PID/vector/wait-status checks, surviving-client input,
fresh-session reconnection, serial recovery and clean ACPI shutdown. Real heap
OOM rollback, quota admission, malformed surfaces/handles, foreign ownership,
reply backpressure/eviction and copy-failure retention are checked separately.

`gui-image` and `run --gui` provide the optional interactive profile; run
`gui-test` in COM1 to start the supervised demonstration. See
[M14_COMPOSITOR](../milestones/M14_COMPOSITOR.md) for lifecycle/ownership and
[validation/M14.3](../validation/M14.3.md) for exact artifacts and regression scope.
The pure GUI library adds eleven host tests; total host coverage is 163 tests.
CPU stores have no vblank/atomic multi-upload guarantee. USB, device IRQs,
physical hardware, writable shared surfaces and GPU rendering remain outside
this profile. M14.4 GUI runtime/text/widgets/terminal and the independent C/C++/Rust GUI SDK
are accepted separately below; M14.5 has its own bounded GPU drawing acceptance.

## M14.4 GUI runtime and SDK — locally verified (2026-10-23)

The allocation-free Rust runtime implements `anova.gui.v1` (`0x00010000`),
fixed C layouts, generated declarations, process-local generation handles,
C++ move-only RAII and safe Rust wrappers. It owns clipped raster drawing,
original bitmap text, layout, buttons/text fields, ordered polling and a
bounded private UTF-8 clipboard. Overflow retains resize geometry and sends
RESET; server loss invalidates handles. No kernel ABI or native std is added.

`cargo xtask gui-sdk-test` passed two real q35/TCG boots with 3 GiB memory,
standard VGA and PS/2. The UART editor saved seven exact sources, guest
GCC/G++/rustc built them, and independent C/C++/Rust clients rendered and
processed physical keys/buttons before clean close. Rust note exercised copy;
the interactive Rust terminal executed keyboard-entered `echo hi` via a
pre-GUI helper, actual pipes/native processes and checked child waits.
A deliberate GUI caller page fault remained isolated, and the next client
worked. Both boots reaped all compositor/client pairs and shut down cleanly.

All 28 screenshots passed independent literal pixel/glyph samples. Three
language clients also produced identical complete screenshot hashes for their
three matched phases. This is sampled visual verification, not a claim that
the M14.4 gate compares every screen pixel. Exact source/native-ELF hashes
matched across reboot; a failed compile retained its previous executable;
both filesystem checks passed. A final-driver retained-output replay also passed,
including the terminal presentation acknowledgement and unchanged file hashes. Sixteen runtime host tests and three new host
gate tests passed. All 26 selected preexisting checks pass after the recorded
reruns; see [M14.4 validation](../validation/M14.4.md) for original failures,
commands, artifact hashes and the exact regression scope.

The source SDK exports independently with checksum/header checks and its own
Cargo manifest. CI includes the gate, but no remote CI or physical-hardware
result is claimed. Current limits include one bounded private window per
client, ASCII bitmap shapes/Unicode fallback boxes, process-local clipboard,
US key text, no GUI callbacks/threads, a bounded echo/pwd command terminal,
static linkage and no dynamic GUI package-service admission. Bounded GPU drawing
is accepted independently under M14.5 below.

## M14.5 bounded hardware drawing — locally verified (2026-10-24)

An optional modern virtio-gpu control queue submits actual VirGL offscreen
render-target clears and fenced host-to-guest readback. This exact device
reports `D3D12 (NVIDIA GeForce RTX 4060 Ti)` through capset 2. A compositor-only
checked operation returns at most 64×64 RGB pixels; it exposes no raw commands,
user DMA addresses or shared device mappings. The independent user compositor
selects CPU fallback and uses the existing checked VGA presentation path.

`cargo xtask gpu-test` passed three boots: hardware VirGL, a 2D-only GPU and
no GPU. The hardware boot verifies four 64×64 colors, a 7×5 intermediate-color
readback with a guarded output tail, calibrated lost-doorbell timeout, actual
reset/DMA reclamation, CPU fallback and subsequent hardware reinitialization.
Five complete 800×600 screenshots match the CPU reference across these boots.
All cases require admitted-operation negatives, successful compositor exits,
COM1 recovery and clean shutdown. The independent headless fallback command
is included in CI; remote execution remains unverified.

This accepts GPU clear/readback drawing only. Scanout copying and ordinary
window composition remain on the CPU; no shader draws, guest OpenGL/Vulkan API,
zero-copy scanout, performance speedup, physical GPU passthrough or recovery
from an unacknowledged reset is established. Exact scope and current regression
evidence are in [M14 GPU](../milestones/M14_GPU.md) and [validation/M14.5](../validation/M14.5.md).

## Regression checks for this delivery

Local gates passed: `foundation-journal-test`, `foundation-migrate-test`, the
existing `foundation-write-test`, `foundation-test`, `persistence-test` and all
13 `verify` boot/fault cases. `test-host` passed 224 tests; the final xtask-only
rerun passed all 48 tests. Strict host Clippy and the bare-metal
`ext4-write,bootstrap-metrics` kernel Clippy passed, as did workspace/user
formatting, Python/workflow parsing, bilingual documentation, 454-file purpose
coverage and `git diff --check`. Remote CI was configured, not accepted by these
local results; at that historical delivery the workflow triggered on `dev` pushes/PRs or manual dispatch; current policy is main-push only.
