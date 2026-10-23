<!-- Record locally verified subsystem capabilities, runtime limits and the next development task. -->
# Implementation status

Recorded 2026-10-23 (Asia/Shanghai). M14.3 continuation baseline `e3854bb`.
Task branch: `feat/m14-framebuffer`. See [M14 GUI](M14_GUI.md),
[validation/M14.1](validation/M14.1.md), [validation/M14.2](validation/M14.2.md)
and [validation/M14.3](validation/M14.3.md).
Remote CI and publication are separate from local acceptance.

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
  [file inventory](FILE_PURPOSES.md). Current command results are in
  [validation/M14.3](validation/M14.3.md).
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
  heap, 1 MiB writable files and Linux stacks, 256 KiB scheduler stack and
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
  are in [validation/M12](validation/M12.md) and [M12_NETWORK](M12_NETWORK.md).
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
- The separate future GUI C ABI/C++/Rust delivery contract remains reserved for
  M14.4. Permissions are admission metadata, not process sandboxing. The package
  Linux profile is static ET_EXEC; the broader M9 runtime remains separately
  verified. See [M13 SDK](M13_SDK.md) and [validation/M13](validation/M13.md).

## Next task

M14.4: Rust GUI runtime, text/widgets/graphical terminal and a versioned
C ABI with independent C/C++/Rust GUI SDK acceptance under the five
[M14 GUI stages](M14_GUI.md). M14.3 native composition, focused input and
client/server fault recovery passed their local three-boot gate. Preserve
every previous runtime/storage/network,
C/C++/Rust/Cargo and new SDK/workflow/package/diagnostic gate. Broader Cargo
projects, native AnovaOS std, full user debugging, LTO/OpenMP/sanitizers and
compiler self-bootstrap remain independently scoped.

## M14.1 software display — locally verified (2026-10-23)

The project owner approved [DEV-002](REQUIREMENTS.md): a Rust-native GUI
runtime, user-space window compositor/desktop and stable versioned C ABI
with C, C++ and Rust application SDKs. The detailed architecture, security
constraints, candidate dependency policy and five independent gates are in
[M14_GUI](M14_GUI.md) and [ROADMAP](../ROADMAP.md):

| Planned stage | Intended observable result | Current status |
| --- | --- | --- |
| M14.1 | Validated framebuffer with bounded CPU/software graphics | Local QEMU gate passed |
| M14.2 | Keyboard/pointer events and visible cursor | Local QEMU gate passed |
| M14.3 | Isolated user-space compositor; two windows and correct focus | Local QEMU gate passed |
| M14.4 | GUI runtime and C/C++/Rust SDK demos compiled/run in guest | Not started |
| M14.5 | Verified GPU-rendered path with CPU fallback | Not started |

The optional graphics feature validates the reserved framebuffer span and each
supervisor RW/NX WC mapping before writing. A dependency-free `no_std` library
provides clipped pixel/rectangle/blit primitives, checked 24/32-bit RGB packing
and a temporary caller-owned back buffer. The actual standard-VGA screenshot's
480,000 pixels and fixed checksum match the reference; full device readback and
heap restoration also pass. Another boot without VGA retains the COM1 shell.
Both boots execute a recovery command and shut down cleanly. See
[validation/M14.1](validation/M14.1.md) for commands, artifacts and the exact
regression scope. `graphics-image`, `graphics-test` and `run --graphics` exist.

This M14.1 diagnostic alone provides no user-space input routing or windows.
M14.3 composition is delivered separately below. GUI SDK binaries and GPU
acceleration remain future stages; the boot framebuffer diagnostic is not a window system. CPU presentation has no
vblank/atomic page-flip guarantee. Only QEMU 800×600×32 is accepted as a scanout;
other pixel formats are host-tested. M12 supplies the bounded Rust/Cargo prerequisite and M13
supplies SDK/package delivery; M14.4 still requires its own GUI developer acceptance. SMP, physical
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

The local three-boot gate passed; [validation/M14.2](validation/M14.2.md)
records actual acceptance and regression scope independently. Current host
coverage is 149 tests, including 22 pure input tests, the pinned cursor raster
and host input-gate negatives. USB, physical hotplug, external IRQ delivery,
real-hardware input, graphical text/terminal, compositor and GUI ABI/SDK remain
outside this M14.2 profile. M14.3 is accepted separately below; M14.4 is next.

## M14.3 native software compositor — locally verified (2026-10-23)

A separate Rust native process owns bounded software surfaces, window stacking,
damage, move/resize, visibility, focus and physical input routing. The kernel
admits only the immutable compositor executable for display/input authority and
provides authenticated copied messages with four process-owned connections.
Each client owns one private 32x24 through 320x240 RGB window; no client obtains
video memory or another process's mappings. The 800x600 compositor fits the
existing 4 MiB native ELF limit. Native v1 remains frozen; this experimental
transport is separate from M14.4's future public GUI library ABI.

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
[M14_COMPOSITOR](M14_COMPOSITOR.md) for lifecycle/ownership and
[validation/M14.3](validation/M14.3.md) for exact artifacts and regression scope.
The pure GUI library adds eleven host tests; total host coverage is 163 tests.
CPU stores have no vblank/atomic multi-upload guarantee. USB, device IRQs,
physical hardware, writable shared surfaces and GPU rendering remain outside
this profile. M14.4 GUI runtime/text/widgets/terminal and the independently
validated C/C++/Rust GUI SDK are next; M14.5 acceleration remains open.
