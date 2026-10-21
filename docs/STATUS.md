# Implementation status

This document records verified capabilities, remaining limits and the next task.
Recorded 2026-10-22 (Asia/Shanghai). M8 source baseline:
`3dfdf6e` on `dev` (merged M7). Task branch: `feat/m8-linux-static`.
Optimization baseline: `b65ac15`; see [optimization validation](validation/M8_OPTIMIZATION.md).
See [M8 design](M8_LINUX.md) and [validation/M8.md](validation/M8.md).

## Implemented and locally verified

- M0–M3: pinned Rust 1.90.0, checked Limine/OVMF, q35/TCG UEFI boot, COM1,
  BSP CPU tables/guarded ISTs, real fault diagnostics, calibrated LAPIC timer,
  owned frames/page tables, NX/WP and a fixed 2 MiB reclaiming buddy heap.
- M4/M5: isolated native Rust/C ELF, checked syscall/user-copy boundaries,
  up to 32 single-threaded processes, full enabled CPU/FPU state, preemption,
  blocking wait/sleep/HLT, eager fork, transactional exec/spawn, faults,
  orphan handling, OOM rollback and exact frame/heap reclamation.
- M6: initramfs/tmpfs/devfs, checked paths/cwd, shared open descriptions and
  offsets, regular file operations, directory iteration, unlink-open lifetime,
  1 KiB blocking pipes, redirection, polled canonical COM1 input, PID 1 and shell.
- M7: PCI enumeration and bounded modern virtio-blk MMIO, split queue, one owned
  coherent DMA32 page, capacity/overflow/error checks, actual FLUSH, PM-timer
  deadlines and acknowledged reset before DMA release. Device IRQs stay masked.
- A strict single-group, 4 KiB/128-byte-inode ext2 data volume at `/persist`,
  direct/single-indirect files up to 1 MiB, directories, sparse writes, append,
  truncate, unlink-open reuse and existing shared fd/process semantics.
  Unknown features, dirty images and invalid allocation profiles are rejected.
- Explicit native-v1 disk manifest and executable-mode files; copied native
  ELF loads through the existing isolation/ELF validator and executes after reboot.
- sync/fsync, ordered updates, fail-closed mutation errors, clean superblock
  unmount, DMA reset and restricted reference-QEMU ACPI S5 shutdown.
- A 16 KiB serial line editor opens/edits/saves source; cp/install/sync/shutdown
  commands and safe temporary versus explicit persistent image tooling.
- Actual serial-driven save/reboot: edited source bytes, redirected text and
  copied ELF SHA-256 match; the app executes after reboot; e2fsck passes.
  Read-only devices, unsupported ext2 features and actual virtio write/flush
  failures produce errors while the kernel remains usable.
- M8: explicit static Linux ELF identity, checked argc/argv/envp/auxv, separate
  syscall/errno/flags/layout translation, bounded anonymous memory, FS TLS,
  RDRAND, descriptor CLOEXEC, fork/exec/wait, basic signal masks/default/ignore
  and the selected static musl/BusyBox CLI tests. Full signal handlers remain
  unsupported and return ENOSYS.
- 65 host tests (including storage, ACPI-power and shell-parser coverage),
  formatting, Clippy with warnings denied, source-purpose coverage, all 13
  QEMU regression classifications and both terminal-input boots pass.
  Remote CI execution is recorded separately and has not been run for M7.
- DEV-001 remains mandatory: English top-of-file purpose explanations and a
  complete [file inventory](FILE_PURPOSES.md), checked by `cargo xtask check-files`.

| Capability | Status |
| --- | --- |
| M0–M3 engineering, boot, BSP CPU and owned memory | Locally accepted on q35/TCG |
| M4/M5 isolated apps, CPU/FPU and process lifecycle | Locally accepted on q35/TCG |
| M6 VFS, init, native shell, terminal, pipes/redirection | Locally accepted; tmpfs is volatile |
| M7 PCI/virtio, ext2, source editing and clean reboot persistence | Locally accepted within the declared profile |
| Atomic rename/save, journaling and crash recovery | Not implemented |
| External IRQ/NMI delivery, hotplug and asynchronous block I/O | Not implemented |
| Bootloader/firmware reclamation, heap growth and general DMA pool | Not implemented |
| M8 static Linux ABI, musl and selected BusyBox CLI | Locally accepted within the documented profile |
| General Linux compatibility and networking | Not implemented or verified |
| GCC/G++, rustc/Cargo running inside AnovaOS | Not implemented or verified |
| Threads, custom signal handlers, PTY/termios, COW, SMP, GUI and physical hardware | Not implemented or verified |

The non-preemptible BSP polls each block request with a two-second deadline;
long kernel I/O delays processes and delivered IRQ time. Root stays read-only
initramfs, `/tmp` remains volatile and plain `run` uses a disposable data disk.
Persistent files require an explicit image and normal guest `shutdown`.
Non-journaled ext2 can be inconsistent after forced termination; dirty images
need offline recovery. The editor's truncate/write save is not atomic.
See [M7_STORAGE.md](M7_STORAGE.md) for exact limits and ownership contracts.

## Next task

Implement M9: real signal frames/rt_sigreturn and handlers, interruption/restart
semantics, threads/clone/futex, terminal behavior and controlled dynamic musl.
Preserve the pinned static CLI profile and add each capability with negative
and reclamation tests. Guest-hosted compilers remain M11/M12; running these
host-built static ELFs does not claim compilation inside AnovaOS.

Follow [DEV-001](REQUIREMENTS.md), preserve existing negative cases, maintain
English purpose explanations and synchronize both READMEs. Keep Limine
reservations until every live reference is replaced. Do not claim arbitrary
power-loss safety, general Linux compatibility or guest compilers from these tests.
