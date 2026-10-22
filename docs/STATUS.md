<!-- Record locally verified subsystem capabilities, runtime limits and the next development task. -->
# Implementation status

Recorded 2026-10-22 (Asia/Shanghai). M11 baseline: `42cb018` on dev, after
the M10 merge. Task branch: feat/m11-guest-toolchain. See
[M11 profile](M11_TOOLCHAIN.md) and [validation/M11.md](validation/M11.md).
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
  [file inventory](FILE_PURPOSES.md). Current command results are in validation/M11.
- 77 host tests, host/kernel/user Clippy, formatting and all 13 QEMU scenarios
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
| Full terminal jobs, alternate signal stacks, realtime queues, robust/PI futexes | Not implemented or verified |
| COW, shared persistent mmap/writeback, ASLR, SMP, GUI and physical hardware | Not implemented or verified |
| M10 IPv4 client networking and controlled native packages/TLS | Locally accepted within M10_NETWORK_PACKAGES |
| M11 offline GCC/G++ 11.2.0, binutils, make and matching musl/C++ sysroot | Locally accepted within M11_TOOLCHAIN |
| General Linux networking/glibc, public package repositories and guest rustc/Cargo | Not implemented or verified |
| External IRQs, asynchronous block I/O, hotplug and boot-memory reclamation | Not implemented |

The BSP polls block I/O with a two-second deadline; long kernel I/O delays tasks
and delivered IRQ time. Root remains read-only initramfs and plain run uses a
throwaway disk. Persistent files need an explicit image and clean guest shutdown.
The editor's truncate/write save is not atomic. M9 does not change these limits.

## M11 implemented and locally verified

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

## Next task

M12: guest-hosted rustc and Cargo, beginning with the offline single-file std
and dependency-free/vendor Cargo stages. Preserve every M9/M10 test and
`cargo xtask toolchain-test`, file-purpose coverage, actual fault/OOM boundaries,
clean shutdown and exact resource reclamation. General projects, dynamic
application toolchains, LTO/OpenMP/sanitizers and compiler self-bootstrap remain
outside the accepted M11 profile.
