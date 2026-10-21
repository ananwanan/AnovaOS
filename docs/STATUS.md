# Implementation status

Recorded: 2026-10-21 (Asia/Shanghai). M3 source baseline:
`9910a7261d6b144e305fd8e7a72841a4db6a0db1` on `dev`.
Task branch: `feat/m3-memory-management`. The implementation is the commit
introducing [M3 evidence](validation/M3.md); its Git history identifies the revision.
See [memory design](M3_MEMORY.md), [CPU design](M2_CPU.md) and the historical
[M2 evidence](validation/M2.md) / [M0/M1 evidence](validation/M0-M1.md).

## Implemented and locally verified

- M0/M1: Rust 1.90.0 workspace, locked boot inputs, kernel ELF, UEFI/Limine ISO,
  retained boot data, COM1 and failure-aware host tooling.
- M2: BSP GDT/TSS/IDT, independent guarded ISTs, real #UD/#GP/#PF/#DF diagnostics,
  returning breakpoints, ACPI parsing and a calibrated 100 Hz LAPIC timer.
- M3: checked 4 KiB frame allocation/release with explicit metadata reservation,
  owner tags, invalid/double/wrong-owner release rejection and DMA32 constraints.
  Only USABLE RAM is admitted; every bootloader/non-usable reservation remains.
- Every active page-table frame is owned after full hierarchy cloning. Dynamic
  map/protect/unmap, effective permissions, empty-table reclamation and OOM
  rollback use the shared host-tested algorithms. Imported huge leaves remain.
- Supervisor mappings, NX including HHDM aliases, CR0.WP, explicit WB RAM / UC
  MMIO attributes, and local TLB/paging-structure invalidation before reuse.
- The BSP executes on a new 64 KiB normal stack with two absent guards and owned
  RSP0. The three M2 ISTs remain independent and permanently kernel-reserved.
- A fixed 2 MiB buddy heap enables `Box`, `Vec` and `String`, block reuse and
  coalescing, fallible allocation errors and terminal infallible OOM diagnostics.
- `verify` now checks 13 QEMU images. The boot test requires memory ownership,
  physical exhaustion/recovery, rollback, mapping/container stress, two returning
  breakpoint IF states and at least eight timer IRQs. Actual protection faults,
  OOM panic and the prior negative cases must fail at their expected injection.
- Full pressure tests run only in test images. Normal startup still checks stack,
  control registers, table ownership and RAM/device mapping permissions.
- Formatting, host/kernel Clippy, 23 host tests and local q35/TCG verification
  pass. Configured CI uses the same gate; remote CI is recorded separately.

| Capability | Status |
| --- | --- |
| M0 engineering baseline | Locally accepted |
| M1 UEFI/COM1/QEMU and failure detection | Locally accepted on q35/TCG |
| M2 BSP exceptions, IST guards, LAPIC timer | Locally accepted on q35/TCG |
| M3 checked frames, owned tables/stack, reclaiming heap | Locally accepted on q35/TCG |
| Bootloader/firmware reclamation and heap resizing | Not implemented; reservations/backing retained |
| External device IRQ delivery, NMI routing | Deferred to corresponding device drivers |
| User isolation, scheduling, syscalls and apps | Not implemented |
| Filesystem, terminal, persistent disk and network | Not implemented |
| Linux ABI and existing software | Not implemented or verified |
| GCC/G++, rustc/Cargo inside AnovaOS | Not implemented or verified |
| GUI, SMP, physical hardware and bootstrap | Not implemented or verified |

The time API remains a coarse delivered-IRQ count, not wall time. HHDM aliases
of the kernel image retain write permission while being NX. Physical-page-wide
immutability, general huge-leaf editing, user address spaces and SMP shootdown
are not claimed. There is no DMA device implementation or contiguous DMA pool.
See [M3_MEMORY.md](M3_MEMORY.md) for exact bounds and lock/lifetime rules.

## Next task

Implement M4: the first isolated Ring 3 ELF64 ET_EXEC app, per-process mappings,
native syscall/return and safe user copying, a minimal Rust/C runtime/SDK and
negative ELF/pointer/syscall tests. Initialize CPU state required by user SSE2
before running user programs, and preserve it before M5 task switching.

Keep original Limine data/tables reserved until every live reference is copied
or replaced. Enable external IRQ routing only with the corresponding device
driver, including MADT overrides, polarity/trigger, vector ownership and EOI.
