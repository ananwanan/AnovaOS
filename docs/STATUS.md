<!-- Record locally verified subsystem capabilities, runtime limits and the next development task. -->
# Implementation status

Recorded 2026-10-22 (Asia/Shanghai). Baseline `7ce53fe` on dev.
Task branch: `feat/m12-rust-cargo`. See [M12 profile](M12_RUST.md) and
[validation/M12.md](validation/M12.md).
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
  [file inventory](FILE_PURPOSES.md). Current command results are in validation/M12.
- 81 host tests, host/kernel/user Clippy, formatting and all 13 QEMU scenarios
  pass. Ordinary UART and persistence/error sessions consume ordered stdout
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
| COW, shared persistent mmap/writeback, ASLR, SMP, GUI and physical hardware | Not implemented or verified |
| M10 IPv4 client networking and controlled native packages/TLS | Locally accepted within M10_NETWORK_PACKAGES |
| M11 offline GCC/G++ 11.2.0, binutils, make and matching musl/C++ sysroot | Locally accepted within M11_TOOLCHAIN |
| M12 guest Rust 1.90.0/std, offline Cargo/vendor, build.rs/C FFI and a procedural macro | Locally accepted within M12_RUST |
| General Linux networking/glibc, public package repositories and online Cargo registry/Git | Not implemented or verified |
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
  checked independently. Persistent shared mmap/
  writeback remains unsupported.
- Linux ELF/loaded image and per-mmap bounds are 256 MiB; the mapping window is
  `[0x50000000, 0xc0000000)`. A committed per-root gap hint avoids rescanning large
  immutable compiler mappings. Linux startup uses 256 argv/env entries and a
  checked 32 KiB block; native startup remains 64 entries. Linux/VFS paths and
  names support 512/255 bytes; native v1 path/directory wire capacities stay
  separate and reject unrepresentable names without consuming them.
- The Rust QEMU profile uses 3 GiB RAM. One BSP, 32 task slots, 16 MiB kernel
  heap, 1 MiB writable files and Linux stacks, 256 KiB scheduler stack and
  retained boot memory remain bounded. See M12_RUST for exact scope.

## Next task

M13: version the application SDK, manifest, package and developer experience
around the verified C/C++/Rust toolchains. Preserve every previous runtime,
storage, network, toolchain and Rust acceptance gate. Online Cargo registry/Git,
general Cargo projects, native AnovaOS std, LTO/OpenMP/sanitizers and compiler
self-bootstrap remain independently scoped.

## M14 graphics planning — not implemented (2026-10-22)

The project owner approved [DEV-002](REQUIREMENTS.md): a Rust-native GUI
runtime, user-space window compositor/desktop and stable versioned C ABI
with C, C++ and Rust application SDKs. The detailed architecture, security
constraints, candidate dependency policy and five independent gates are in
[M14_GUI](M14_GUI.md) and [ROADMAP](../ROADMAP.md):

| Planned stage | Intended observable result | Current status |
| --- | --- | --- |
| M14.1 | Validated framebuffer with bounded CPU/software graphics | Not started |
| M14.2 | Keyboard/pointer events and visible cursor | Not started |
| M14.3 | Isolated user-space compositor; two windows and correct focus | Not started |
| M14.4 | GUI runtime and C/C++/Rust SDK demos compiled/run in guest | Not started |
| M14.5 | Verified GPU-rendered path with CPU fallback | Not started |

This documentation change delivers **no GUI implementation, no graphics
test, no new SDK binaries, no GPU acceleration and no additional hardware
support**. The Limine framebuffer response is an early boot input, not a
window system. M12 supplies the bounded offline Rust/Cargo prerequisite; M13
SDK packaging is still required before M14.4's developer-experience acceptance. SMP, physical
devices and Linux GUI compatibility remain separately scoped workstreams.
