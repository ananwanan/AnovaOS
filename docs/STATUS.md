<!-- Record locally verified subsystem capabilities, runtime limits and the next development task. -->
# Implementation status

Recorded 2026-10-22 (Asia/Shanghai). M10 baseline: `cbab786` on dev, after
the M9 merge. Task branch: feat/m10-network-packages. See
[M10 profile](M10_NETWORK_PACKAGES.md) and [validation/M10.md](validation/M10.md).
Remote CI and publication are separate from local acceptance.

## Implemented and locally verified

- M0–M3: Rust 1.90.0, locked Limine/OVMF, q35/TCG UEFI, COM1, BSP tables/ISTs,
  diagnosed faults, calibrated LAPIC timer, owned frames/tables, NX/WP and a
  fixed 2 MiB reclaiming buddy heap. Boot/firmware reservations remain live.
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
  [file inventory](FILE_PURPOSES.md). Exact command results are in validation/M9.
- 76 host tests, host/kernel/user Clippy, formatting and all 13 QEMU scenarios
  pass. Ordinary UART and persistence/error sessions consume ordered stdout
  directly; diagnostic-log snapshots cannot invalidate their matching cursors.

| Capability | Status |
| --- | --- |
| M0–M8 engineering, isolated apps, VFS, storage and selected static Linux CLI | Locally accepted on q35/TCG |
| M9 pthread/futex/signals, readiness, PTYs and dynamic musl | Locally accepted within M9_RUNTIME |
| Total scheduler pool | 32 process/thread/zombie slots, one BSP |
| Stacks and heap | 64 KiB guarded trap stacks; 128 KiB guarded scheduler stack; fixed 2 MiB heap |
| VFS namespace | 128 nodes; 32 fds per task/table; 128 open descriptions; four PTY pairs |
| ext2 | Normal shutdown persistence; no journal, atomic save or power-loss guarantee |
| Full terminal jobs, alternate signal stacks, realtime queues, robust/PI futexes | Not implemented or verified |
| COW, persistent-file mmap/writeback, ASLR, SMP, GUI and physical hardware | Not implemented or verified |
| M10 IPv4 client networking and controlled native packages/TLS | Locally accepted within M10_NETWORK_PACKAGES |
| General Linux networking/glibc, public package repositories, guest GCC/G++ or rustc/Cargo | Not implemented or verified |
| External IRQs, asynchronous block I/O, hotplug and boot-memory reclamation | Not implemented |

The BSP polls block I/O with a two-second deadline; long kernel I/O delays tasks
and delivered IRQ time. Root remains read-only initramfs and plain run uses a
throwaway disk. Persistent files need an explicit image and clean guest shutdown.
The editor's truncate/write save is not atomic. M9 does not change these limits.

## Next task

M11: real guest-hosted GCC/G++, assembler/linker and a matching libc/CRT/sysroot,
starting with an offline edit/compile/link/run/reboot loop. Revisit current heap,
file-size and namespace limits against the actual compiler trace. Cross-built
ELFs alone do not establish guest compilation. Preserve all M9 runtime and
M10 network/package/TLS tests, file purposes, storage negatives and exact
ownership/reclamation gates.
