# Implementation status

This document records verified capabilities, remaining boundaries and the next
milestone. Recorded 2026-10-21 (Asia/Shanghai). M5 source baseline:
`9795803929c833569a87f9ec19c28d64caa09961` on `dev` (merged M4).
Task branch: `feat/m5-process-scheduler`. The commit introducing
[validation/M5.md](validation/M5.md) identifies the implementation revision.

## Implemented and locally verified

- M0/M1: pinned Rust 1.90.0 host/kernel workspace, verified Limine/OVMF inputs,
  native UEFI ISO, retained boot data, COM1 and failure-aware QEMU tooling.
- M2: BSP CPU/exception tables, guarded ISTs, returning breakpoints, ACPI parsing,
  calibrated LAPIC 100 Hz timer and real #UD/#GP/#PF/#DF diagnostics.
- M3: checked physical-page ownership, complete page-table ownership, local TLB
  invalidation, supervisor/NX/WP permissions, 64 KiB guarded normal stack and
  fixed 2 MiB reclaiming buddy heap. Bootloader/firmware reservations remain.
- M4: private static ET_EXEC Rust/C Ring 3 images, explicit native v1 personality,
  checked syscall/IRETQ boundary, bounded user copying with actual #PF fixups,
  fault containment, ELF/OOM rollback and standalone Rust/C SDK.
- M5: up to 32 single-threaded native processes including zombies; private user
  roots/pages, unique guarded kernel stacks and saved GPR/segment/base/x87/MMX/
  XMM0–15/MXCSR contexts. XSAVE/AVX and FSGSBASE stay disabled.
- User timer preemption and round-robin scheduling, yield, deadline sleep,
  blocking child wait and interruptible HLT when nothing is ready. Kernel code
  and usercopy remain non-preemptible at IF=0 on one BSP.
- Spawn/transactional exec/eager fork/exit/wait, parent relationships, fault exit
  status, kernel orphan adoption/reaping and process-slot reuse.
- Owned console descriptor slots with fork inheritance, exec retention, close
  and exit cleanup. General files/open descriptions/shared offsets are M6.
- Actual syscall-free CPU/FPU sentinel loops in two concurrent workers, child
  #UD with survivor progress, private fork memory, inherited fork CPU state,
  fresh exec state and retained closed fd, wait-any/target and invalid requests.
- Partial spawn/fork/exec allocation failures roll back exactly. Sixteen repeated
  create/wait cycles, full 32-slot exhaustion/reuse and complete teardown restore
  the exact physical-frame and heap counts.
- 37 host tests, root/user formatting, host/kernel/user Clippy with warnings
  denied, normal kernel build, source-purpose coverage and all 13 QEMU cases.
  Remote CI status is recorded separately in [M5 evidence](validation/M5.md).
- Normal boot runs Rust/C hello, two concurrent workers with parent waits, then
  idles interruptibly. Test images additionally run all isolation/lifecycle cases.
- DEV-001 requires English purpose explanations and a complete
  [per-file inventory](FILE_PURPOSES.md), checked by `cargo xtask check-files`.

| Capability | Status |
| --- | --- |
| M0 engineering / M1 UEFI and COM1 | Locally accepted on q35/TCG |
| M2 BSP exceptions and calibrated timer | Locally accepted on q35/TCG |
| M3 owned memory, guarded stack and reclaiming heap | Locally accepted on q35/TCG |
| M4 isolated native applications and SDK | Locally accepted on q35/TCG |
| M5 saved contexts, preemption/blocking and process lifecycle | Locally accepted on q35/TCG |
| Bootloader/firmware reclamation and heap resizing | Not implemented |
| External IRQ/NMI routing and device drivers | Deferred to corresponding drivers |
| VFS, init, shell, pipes and general file descriptors | M6; not implemented |
| Persistent storage and network | Not implemented |
| Linux ABI and existing software | Not implemented or verified |
| GCC/G++, rustc/Cargo running inside AnovaOS | Not implemented or verified |
| Multiple threads, signals, COW, SMP, GUI and physical hardware | Not implemented or verified |

See [M5_PROCESSES.md](M5_PROCESSES.md) for the precise process/locking/ABI
contract. Delivered ticks are coarse monotonic time, not wall time or real-time
scheduling. The kernel is non-preemptible; no address space is shared. HHDM
aliases of the kernel image retain write permission while being NX. Huge-leaf
editing, SMP shootdown, DMA and contiguous DMA pools remain outside scope.

## Next task

Implement M6: VFS with initramfs/tmpfs/devfs, init, a basic shell and file tools,
open-file descriptions/fd ownership, pipes and redirection, and basic serial
terminal input. Use the real M5 native spawn/exec/wait lifecycle and preserve
its negative cases. Do not claim persistence for tmpfs; disk support is M7.

Follow [DEV-001](REQUIREMENTS.md), keep English file headers and
[FILE_PURPOSES.md](FILE_PURPOSES.md) current, and synchronize both READMEs.
Keep Limine reservations until all live references are replaced. Add external
IRQ routing only with the corresponding driver and verified routing/EOI rules.
