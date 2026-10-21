# M2 BSP exceptions and timer

This baseline targets one q35/TCG CPU with the pinned OVMF/Limine inputs.
It describes the early-boot exception/timer boundary before M3. M3 now follows
it with [owned frame/table/stack/heap management](M3_MEMORY.md). M2 setup itself
does not allocate or reclaim physical pages. See [validation/M2.md](validation/M2.md).

## CPU tables and entry boundary

The GDT contains null, kernel code/data, and a two-slot 64-bit TSS descriptor.
The TSS layout is asserted to be 104 bytes; IDT gates are 16 bytes. All 256
gates are CPL0 interrupt gates and clear IF on entry. There is no user entry.
The TSS I/O bitmap starts past the descriptor limit. RSP0 is provisioned but
must be replaced with the appropriate kernel stack before implementing Ring 3.

The assembly stubs normalize hardware error-code and no-error-code vectors,
save all 15 general registers, clear DF, and align the stack for a System V
AMD64 Rust call. The C-layout 176-byte frame includes vector/error and the
five 64-bit hardware words RIP/CS/RFLAGS/RSP/SS. `iretq` restores the original
flags and stack. There is no unstable Rust interrupt ABI or FPU/SIMD use.

| Entry | Stack | Behavior |
| --- | --- | --- |
| Breakpoint, vector 3 | Current BSP stack | Record receipt, bounded output, return with GPR/IF checks |
| #DF, vector 8 | IST1 | Fatal diagnostics on an independent stack |
| #UD/#GP/#PF, vectors 6/13/14 | IST2 | Fatal diagnostics, decode applicable error bits |
| NMI / machine check, vectors 2/18 | IST3 | Fatal handler installed; hardware delivery untested |
| LAPIC timer, vector 64 | Current BSP stack | Atomic tick increment, exactly one EOI, return |
| LAPIC spurious, vector 255 | Current BSP stack | Return without EOI |
| Other vectors | Current BSP stack | Fatal diagnostic; no recovery or device driver |

Each IST has 16 KiB of kernel-reserved stack plus a 4 KiB unmapped guard below.
No Rust reference spans an unmapped guard. Exception output includes RIP,
RSP, CS/SS, RFLAGS, all GPRs, error, CR2, current IF and the actual IST containing
the frame. A fatal nested #PF on IST2 can still escalate to #DF on IST1.
IST3 hardware faults and exhausted IST1 are not claimed to be recoverable.

## Early mapping and reservations

Limine base revision 3 maps usable RAM, bootloader-reclaimable memory, the
kernel/modules and framebuffer through HHDM. It does not map ACPI RAM or APIC
MMIO. Adding an HHDM offset is therefore insufficient for firmware/device reads.

The early mapper takes a single BSP claim with IF=0. It checks four-level paging,
PCID disabled, NX and PAT support, clones CR3, and copies only ancestors of paths
it changes. The ACPI/MMIO PML4 slot must be absent in the original mappings.
A permanently kernel-reserved pool contains 32 aligned table pages
(128 KiB). Existing table reads require retained/HHDM-mapped memory. Modified
paths can split existing 1 GiB/2 MiB leaves, preserving addresses and translating
the large-page PAT bit when producing 4 KiB leaves. Each edit invalidates its VA.

| Virtual range | Purpose | Leaf permissions / cache |
| --- | --- | --- |
| `0xffffc00000000000` + at most 1 MiB and page offset | Reused ACPI read window | Read-only, NX, normal firmware RAM |
| `0xffffc00000200000` + 4 KiB | BSP local APIC | Writable, NX, UC |
| `0xffffc00000400000` + at most four pages | I/O APIC registers | Writable, NX, UC |
| Kernel IST guard VAs reported at boot | Stack overflow detection | Not present |

Firmware spans must fit a reserved firmware/ACPI/bootloader entry, with checked
ends. The temporary slice borrows the mapper so the window cannot be remapped
while that slice is live. Device pages must not overlap RAM, firmware or a
framebuffer; PAT index 3 is checked to be UC. Device addresses come from checked
MADT records and, for LAPIC, must match the CPU APIC-base MSR.

All original mappings and boot data remain retained. Removing a guard mapping
does not free its physical backing; HHDM can still alias kernel data. This is
limited early ownership. M3 subsequently clones every reachable table, while
retaining the pool, boot data and original backing reservations; see
[M3_MEMORY.md](M3_MEMORY.md).

## ACPI and interrupt controllers

`anova-platform` is a no_std crate reused by the kernel and host tests. It decodes
byte slices without packed references. RSDP checks both applicable checksums;
XSDT is preferred, with RSDT fallback. Tables used by MADT/FADT parsing have
signature, length and checksum checks. Unknown child tables are not interpreted.
The root allows at most 128 entries, tables at most 1 MiB, and MADT at most four
I/O APICs. Invalid lengths, zero-length records, duplicate controllers and
unsupported x2APIC records fail explicitly.

The BSP checks CPUID APIC support, APIC ID/base, and local LVT capacity. Both
8259 PICs and every I/O APIC redirection are masked. Masked entries have a valid
kernel vector and BSP destination, with readback of their mask. No external
source override, polarity, trigger mode, NMI route or device IRQ is enabled.
Those routes require the first actual device driver and separate delivery tests.
This explicit M2 boundary avoids enabling interrupts without a servicing driver.

## Time and execution contexts

The FADT must expose a System I/O PM timer with a four-byte register. A usable
extended address takes precedence over the legacy port. The 24/32-bit wrap mask
follows the FADT flag. LAPIC divide-by-16 calibration measures at least 20 ms
against the specified 3,579,545 Hz counter. A finite polling budget diagnoses a
stuck source; loop iterations do not define elapsed time.

The calibrated periodic LAPIC interval is 10 ms (100 Hz). The IRQ only increments
an atomic counter and writes EOI. Eight delivered IRQs, forward-only nanoseconds,
and a PM-counter cadence within a broad 20 ms–1 s tolerance are required before
BOOT:OK. Checking at IF=0 followed by `sti; hlt` avoids a lost wakeup. This gate
detects missing EOI, absent timer delivery and gross interval errors.

`monotonic_ns()` is delivered ticks multiplied by 10 ms, saturating on overflow.
It is a coarse monotonic IRQ time, not wall time or a lossless clock while IF=0.
Normal idle is interruptible. Scheduling, deadlines, TSC/HPET support, finer time,
and compensation for delayed IRQs are future work.

| Context | Output and concurrency contract |
| --- | --- |
| Normal BSP | Normal serial writer saves IF, clears it, emits bounded output and restores IF |
| Timer IRQ | IF remains clear; no output, allocation, locks, sleep or nested IRQ enable |
| Breakpoint | Bounded emergency output, return through unchanged hardware RFLAGS |
| Fatal exception / panic | No logging lock; bounded emergency writer; terminal halt/CLI |

There are no thread contexts or APs. Emergency/NMI output may interleave with
interrupted output rather than deadlocking. Future threads/SMP must introduce
their own context, locking, per-CPU and table ownership rules.

## Fault validation

The original M2 gate booted seven independent images. M3 expands the current
`cargo xtask verify` gate to 13 images; the original seven keep these requirements. Standalone negative cases
always return 1; QEMU exit 37 never counts as successful smoke. The gate requires
the actual expected vector, error, stack, injection and terminal exit. #PF also
matches CR2 to the injected guard address and checks decoded error bits. #DF
requires the saved bad RSP to match the injected guard top and the frame to
reside on IST1, not on the failing stack.

The #DF image temporarily removes #PF's IST selection, points RSP to a guard top,
and executes a real failing push. A second stack fault during #PF delivery
escalates to #DF. The normal image keeps #PF on IST2. All injection features are
mutually exclusive and imply the test-only QEMU exit feature.

## Primary specifications

- [Intel SDM, Volume 3A, revision 090](https://cdrdv2-public.intel.com/874249/253668-090-sdm-vol-3a.pdf):
  IA-32e TSS/IDT, interrupt frames, IST, page tables, PAT and local APIC.
- [ACPI 6.6, software programming model](https://uefi.org/specs/ACPI/6.6/05_ACPI_Software_Programming_Model.html):
  RSDP, XSDT/RSDT, FADT and MADT byte layouts and validation.
- [ACPI 6.6, hardware specification](https://uefi.org/specs/ACPI/6.6/04_ACPI_Hardware_Specification.html):
  PM timer frequency, width and counter semantics.
- [Pinned Limine protocol](https://github.com/Limine-Bootloader/limine-protocol/blob/3a0526b700e356f0eac1b71a77697b3fd1c707a3/PROTOCOL.md):
  base revision 3 HHDM coverage and physical RSDP response.
