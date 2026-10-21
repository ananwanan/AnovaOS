# Implementation status

Recorded: 2026-10-21 (Asia/Shanghai). Source baseline:
847f7c66ddef25616c27a088d0c7044c6f9e8c0d on dev.
See [M0/M1 evidence](validation/M0-M1.md) for actual commands and scope.

## Implemented and locally verified

- M0: kernel/host xtask workspace, Rust 1.90.0, Rust 2024, committed Cargo.lock,
  kernel-only linker script, pinned boot inputs and SHA-256 checks.
- M1: OVMF/Limine UEFI ISO, x86_64 bare-metal ELF, COM1 logging, required response
  validation, map/reservation reporting, optional framebuffer metadata and idle.
- Commands: fetch, doctor, build, image, run, smoke, verify and test-host.
- Boot stages plus QEMU 33 pass. Controlled panic produces QEMU 35; controlled
  hang is killed by the watchdog. Standalone negative cases return 1.
- Formatting, separate host/kernel Clippy and five meaningful host tests pass.
- Both READMEs, architecture and implementation-order documentation are updated.
- CI is configured to use these commands; remote execution is tracked separately.

| Capability | Status |
| --- | --- |
| M0 engineering baseline | Locally accepted |
| M1 UEFI/COM1/QEMU and failure detection | Locally accepted on q35/TCG |
| M2 CPU exceptions, interrupts and timer | Not implemented |
| Page allocator, heap, owned page tables | Not implemented; Limine memory retained |
| User isolation, scheduling, syscalls and apps | Not implemented |
| Filesystem, terminal, persistent disk and network | Not implemented |
| Linux ABI and existing software | Not implemented or verified |
| GCC/G++, rustc/Cargo inside AnovaOS | Not implemented or verified |
| GUI, SMP, physical hardware and bootstrap | Not implemented or verified |

Framebuffer metadata is reported without drawing. RSDP is reported as a physical
address; ACPI mapping/checksums belong to M2. There is no initramfs, module
execution, allocation or memory reclamation. Normal runs attach no data disk.

## Next task

Implement M2: GDT/TSS/IDT, diagnostic CPU exceptions and an isolated double fault
path, followed by ACPI/APIC and timer interrupts. Keep panic logging usable when
introducing IRQ contexts. M3 must define ownership and reservation lifetimes
before reclaiming boot memory.
