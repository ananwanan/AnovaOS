# Implementation status

**M7 feature branch checkpoint (unverified):** `feat/m7-persistent-storage`
now contains a separately packaged native line editor and a host-testable
`anova-storage` crate for checked sector access, constrained ext2 direct-file
I/O, PCI capability parsing, a BSP read-only PCI device probe, and modern virtio\nprotocol layouts. This is **not**
a connected disk driver or an accepted persistent mount; the M6 boot status
below remains unchanged. See [M7_STORAGE.md](M7_STORAGE.md) and
[validation/M7.md](validation/M7.md) for outstanding work and test status.

This document records verified capabilities, remaining boundaries and the next
milestone. Recorded 2026-10-21 (Asia/Shanghai). M6 source baseline:
`fdc852c906647cd58623d43c119e1c637d870fb1` on `dev` (merged M5).
Task branch: `feat/m6-vfs-shell`. The change introducing
[validation/M6.md](validation/M6.md) identifies the implementation revision.

## Implemented and locally verified

- M0/M1: pinned Rust 1.90.0 host/kernel workspace, checked Limine/OVMF inputs,
  UEFI ISO, retained boot data, COM1 and failure-aware QEMU tooling.
- M2: BSP CPU/exception tables, guarded ISTs, returning breakpoints, ACPI parsing,
  calibrated LAPIC 100 Hz timer and real #UD/#GP/#PF/#DF diagnostics.
- M3: checked USABLE physical frames, owned private/kernel page tables, NX/WP,
  guarded 64 KiB stacks and a fixed 2 MiB fallible/reclaiming buddy heap.
- M4/M5: native Rust/C Ring 3 ET_EXEC, safe syscall/IRETQ and user-copy fixups;
  up to 32 single-threaded processes with saved GPR/segment/base/x87/MMX/XMM0–15/
  MXCSR state; preemption, yield, blocking sleep/wait and interruptible HLT;
  spawn/transactional exec/eager fork, faults, parent waits, orphan reaping,
  partial OOM rollback and complete physical-page/heap reclamation.
- M6: host-validated read-only archive, boot mounts for initramfs/tmpfs/devfs,
  component-wise paths and bounded links, cwd, regular files/directories/devices,
  read/write/create/seek/append/truncate/mkdir/unlink/readdir and standard fds.
- Shared open descriptions and offsets across dup2/fork/spawn; independent opens
  retain independent offsets. Exec preserves cwd/fds, exit/fault closes every
  reference, and unlinked file data remains live until all opens close.
- 1 KiB byte-stream pipes, partial writes, blocking read/write, readiness wakeup,
  buffered drain/final-close EOF and broken readers. No user busy-loop or lock
  survives a blocked syscall.
- Bounded polled COM1 canonical input with echo, backspace/DEL, CRLF handling and
  Ctrl+D. Input-blocked processes use scheduler HLT; external IRQs stay masked.
- Native PID 1 supervises an interactive shell with four pipeline stages,
  quotes/escapes and file redirection; native ls/cat/echo/pwd/mkdir/rm/true/false,
  cd/help/status/exit and real native executable paths.
- File/path/user-pointer/offset/inheritance/cwd negatives, descriptor capacity
  rollback/reuse, actual tmpfs heap exhaustion/recovery and blocked pipe shutdown
  are checked in the guest. A 5,000-byte three-stage shell pipeline and expected
  exit/error codes pass, followed by exact physical-frame and heap restoration.
- 47 host tests (including two shell-parser tests), root/user formatting,
  host/kernel/user Clippy with warnings denied, source-purpose coverage and all
  13 QEMU regression classifications pass. Two normal UART-driven boots verify
  editing, EOF/restart and tmpfs loss. Remote CI has not been run for M6.
- DEV-001 requires English purpose explanations and a complete
  [per-file inventory](FILE_PURPOSES.md), checked by `cargo xtask check-files`.

| Capability | Status |
| --- | --- |
| M0 engineering / M1 UEFI and COM1 | Locally accepted on q35/TCG |
| M2 BSP exceptions and calibrated timer | Locally accepted on q35/TCG |
| M3 owned memory, guarded stack and reclaiming heap | Locally accepted on q35/TCG |
| M4 isolated native applications and SDK | Locally accepted on q35/TCG |
| M5 CPU/FPU, preemption/blocking and process lifecycle | Locally accepted on q35/TCG |
| M6 VFS, init, native shell, terminal, pipes and redirection | Locally accepted on q35/TCG; tmpfs is volatile |
| Bootloader/firmware reclamation and heap resizing | Not implemented |
| External IRQ/NMI routing and device IRQ drivers | Deferred to corresponding drivers |
| Persistent storage, editable disk environment and network | Not implemented |
| Linux ABI and existing Linux software | Not implemented or verified |
| GCC/G++, rustc/Cargo running inside AnovaOS | Not implemented or verified |
| Multiple threads, signals, PTY/termios, COW, SMP, GUI, physical hardware | Not implemented or verified |

See [M6_FILESYSTEM.md](M6_FILESYSTEM.md) for precise resource bounds, native
wire layouts, terminal behavior and the deliberately limited shell syntax.
The kernel is non-preemptible on one BSP; delivered IRQ ticks are coarse
monotonic time, not wall time. HHDM aliases of the kernel image remain writable
and NX. Huge-leaf editing, SMP shootdown, DMA and DMA pools remain outside scope.

## Next task

Continue M7: connect PCI BAR/DMA/modern virtio-blk and extend ext2 allocation and VFS mounting beyond the currently isolated storage policies; then validate. Original target: PCI and modern virtio-blk, checked block/DMA/queue ownership,
a supported ext2 subset with explicit feature rejection, flush/fsync/controlled
shutdown and a minimal text editor. Keep default temporary/overlay disks
separate from persistence tests; verify saved sources and binaries after reboot.
The M6 native shell and VFS provide the user-visible entry point.

Follow [DEV-001](REQUIREMENTS.md), preserve all earlier negative cases, keep
file-purpose explanations current, and synchronize both READMEs. Keep Limine
reservations until every live reference is replaced. Never claim tmpfs or the
read-only boot ISO provides writable persistent storage.
