# Implementation status

Recorded: 2026-10-21 (Asia/Shanghai). M2 source baseline:
`39b3625b239edeef35abed299ba4cb7babbe4f27` on `dev`.
Task branch: `feat/m2-exceptions-timer`.
See [M2 evidence](validation/M2.md) and [CPU design](M2_CPU.md).
The earlier [M0/M1 evidence](validation/M0-M1.md) remains historical.

## Implemented and locally verified

- M0/M1: Rust 1.90.0 workspace, locked inputs, kernel ELF, UEFI/Limine ISO,
  retained boot data, COM1, host tooling and failure-aware QEMU runner.
- M2 BSP baseline: GDT/TSS, all 256 IDT gates, stable assembly trap entries,
  returning breakpoints, three guarded 16 KiB IST stacks, fatal diagnostics.
- Real #UD, #GP, write-to-guard #PF, and stack-delivery #DF reach their expected
  vector, error, IST and QEMU exit 37. They remain smoke failures.
- Bounded copy-on-write early page-table paths, NX, explicit read-only ACPI
  mapping, UC/NX APIC mappings, and retained original tables. No frame allocation.
- Checked RSDP/XSDT/RSDT/MADT/FADT parsing shared with host tests.
- ACPI PM-calibrated 100 Hz LAPIC IRQs with EOI, coarse monotonic tick time,
  IF restoration and interruptible normal idle. Eight IRQs are required to pass.
- Both legacy PICs and all discovered I/O APIC entries remain masked. External
  IRQ activation, source overrides and NMI routing are pending device drivers.
- Commands remain fetch, doctor, build, image, run, smoke, verify and test-host.
  Verify now covers seven images: boot, panic, timeout, #UD, #GP, #PF and #DF.
- Formatting, host/kernel Clippy, host tests and local q35/TCG verification pass.
  CI uses the same expanded gate; remote results are recorded separately.

| Capability | Status |
| --- | --- |
| M0 engineering baseline | Locally accepted |
| M1 UEFI/COM1/QEMU and failure detection | Locally accepted on q35/TCG |
| M2 BSP exceptions, IST guards, LAPIC timer | Locally accepted on q35/TCG |
| External device IRQ delivery, NMI routing | Deferred to corresponding device drivers |
| Page allocator, heap, full address-space ownership | Not implemented; all boot memory retained |
| User isolation, scheduling, syscalls and apps | Not implemented |
| Filesystem, terminal, persistent disk and network | Not implemented |
| Linux ABI and existing software | Not implemented or verified |
| GCC/G++, rustc/Cargo inside AnovaOS | Not implemented or verified |
| GUI, SMP, physical hardware and bootstrap | Not implemented or verified |

The time API counts delivered IRQs, with 10 ms resolution. It is not wall time
and can lose elapsed time while interrupts remain disabled for long intervals.
No initramfs, module execution, memory reclamation or framebuffer drawing exists.
The original bootloader execution stack is still retained; the three new IST
stacks have guards, not a newly owned/guarded normal execution stack.

## Next task

Implement M3: explicit physical/virtual/reserved ownership, a checked frame
allocator, owned page tables, a guarded normal kernel stack and a heap/OOM path.
Keep original Limine data/tables reserved until every live reference is replaced
or copied. The early 32-page table pool and the three IST stacks are permanent
kernel reservations; guard mappings do not release their backing frames.

External device IRQ routing must be enabled and tested with the first relevant
driver, including MADT overrides, trigger/polarity, vector ownership and EOI.
