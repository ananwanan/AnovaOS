# M3 memory ownership and allocation

M3 targets the single q35/TCG BSP with four-level paging. It adds real frame
allocation/release, a fully owned page-table hierarchy, a guarded execution
stack and a reclaiming kernel heap. See [validation/M3.md](validation/M3.md)
for the measured results and [M2_CPU.md](M2_CPU.md) for trap/IRQ contracts.

## Physical ownership

Only Limine `USABLE` entries enter `FrameAllocator`. Boot validation rejects
overlaps between usable RAM and every other entry. Kernel/module storage,
the early 32-page table pool, all IST storage including guard backing, ACPI,
framebuffer, firmware, bad memory, original boot tables/stacks and response
storage remain outside the allocator. No bootloader-reclaimable memory is
released: live response references still point into it.

The allocator accepts at most 128 sorted, page-aligned usable ranges. It checks
address/length overflow and the CPU's physical address width. Each usable page
has one state byte. Metadata is placed in an initially usable HHDM-mapped span;
its complete page-rounded physical reservation is excluded before the first
allocation. Memory exhaustion or an unsupported map fails explicitly rather
than truncating the map. Metadata consumes about one byte per 4 KiB of usable
RAM, plus page rounding.

| State/domain | Meaning |
| --- | --- |
| Free | Eligible usable RAM, not reserved or owned |
| Reserved | Metadata pages and physical page zero if present |
| PageTable | An allocated table frame; only the table backend may edit it |
| Stack | Permanently pinned normal kernel stack backing |
| Heap | Permanently pinned backing of the fixed kernel heap |
| Test | Temporary frames used only by isolated verification |
| Outside all usable ranges | Never allocatable, regardless of memory-map label |

Allocation returns a 4 KiB `PhysAddr`. Release checks alignment, membership,
reservation, allocation state and the expected owner before changing state.
Immediate double release, a foreign address, a reserved page or a wrong owner
returns an error. Kernel callers must unlink mappings before returning backing;
the low-level allocator is not a capability or generation-based public API.
Its callers are private to the memory manager/table backend.

`Zone::Dma32` limits allocation to frames wholly below 4 GiB. This is an address
constraint only: no DMA driver, contiguous DMA allocator, IOMMU, coherence or
device ownership protocol is implemented. MMIO is excluded from RAM allocation.

## Page tables and permissions

After ACPI/APIC discovery, M3 consumes `EarlyMapper` and clones **every** live
non-leaf, including unchanged paths. The new root has no pointers to Limine or
early-pool tables. Imported leaf backing keeps its prior reservation and mapping;
it is never implicitly freed. An ownership walk verifies every reachable table
frame and its count after switching CR3.

The shared `anova-memory` crate implements import, query, 4 KiB map/protect/unmap
and empty-table reclamation. A kernel `TableStore` validates owned frame access,
uses aligned volatile entries, and supplies invalidation. Import and mapping
failures release partially built private table chains. New paths are published
only after all allocations succeed. An unmap flushes TLB/paging-structure caches
before releasing any removed table and before releasing leaf backing.

Imported 1 GiB/2 MiB leaves are preserved and queryable, including PAT selection.
Dynamic edits into huge leaves return `HugePage`; M3 does not add a general
huge-page splitter or a user address space. Effective ancestor permissions are
checked: an RW or executable request blocked by an ancestor is rejected.

Activation retains four-level paging, disables PGE, requires PCID off, enables
CR0.WP and retains EFER.NXE from feature-checked M2 setup. With no APs, CR3 reload
provides complete local invalidation. This is not an SMP TLB-shootdown design.
New mappings are supervisor-only and reject writable+executable permissions.
All HHDM root slots are additionally NX, including their huge leaves; otherwise
the bootloader's direct alias could execute heap or stack contents. The complete
HHDM span must fit below the separate ACPI/MMIO window.

| Virtual range | Ownership / permission / cache |
| --- | --- |
| Retained HHDM below `0xffffc00000000000` | Supervisor, NX; RAM aliases retain their boot cache attributes |
| `0xffffc00000000000` firmware window | Retained M2 read-only, NX ACPI mapping |
| `0xffffc00000200000` LAPIC and `0xffffc00000400000` I/O APIC | Retained supervisor RW/NX, UC, PAT index 3 |
| `0xffffd00000010000` | Absent lower normal-stack guard |
| `0xffffd00000011000..0xffffd00000021000` | Owned 64 KiB normal stack, supervisor RW/NX, WB |
| `0xffffd00000021000` | Absent upper normal-stack guard |
| `0xffffd00000200000..0xffffd00000400000` | Owned 2 MiB buddy heap, supervisor RW/NX, WB |
| `0xffffd00000800000` and following test pages | Temporary test mappings, reclaimed after each check |
| `0xffffd08000000000` | Unused slot for the partial-map OOM rollback test |
| Kernel ELF / three M2 ISTs | Original kernel reservations; ELF RX/R/RW and existing IST guards |

PAT index 0 must be WB and index 3 UC. Mapping queries expose the selected PAT
index; the kernel checks RAM and controller attributes. MMIO is mapped only
after M2's RAM/firmware/framebuffer overlap checks. No new device mapping or
cache-changing alias is created by M3.

Kernel text has an RX virtual mapping, and RAM direct aliases are NX. Privileged
HHDM aliases of kernel image data still retain their existing write permission;
M3 does not claim physical-page-wide immutability, user isolation or complete
address-space teardown. Further direct-map write hardening is separate work.

## Execution stack and heap

The BSP moves once from Limine's retained stack to the new 64 KiB stack through
a stable naked assembly boundary. It supplies System V call alignment, clears
RBP and never returns into the old stack. Only copied timer arguments cross
the boundary. TSS.RSP0 is updated; IST1/2/3 remain independent. Actual RSP and
both absent guard mappings are checked after entry.

The fixed 2 MiB heap uses an intrusive buddy allocator with a 16-byte minimum
block. Layout size/alignment are rounded to a fitting power of two; free blocks
split and buddies coalesce on release. No allocator operation allocates metadata.
`GlobalAlloc` enables stable Rust `alloc`, including `Box`, `Vec` and `String`.
The heap does not grow, shrink or return backing pages to the frame allocator.
Internal fragmentation is the expected buddy tradeoff.

OOM returns null and emits a bounded allocation-free diagnostic. Fallible
`Vec::try_reserve_exact` returns an error and leaves existing objects usable.
Infallible allocation reaches Rust 1.90's default no_std allocation error handler,
then the existing lock-free panic path; no nightly allocation-error attribute
is used. Heap exhaustion never returns overlapping live blocks.

Both memory-manager and heap locks save/clear IF before acquisition and restore
the prior IF after release. They cannot sleep. Lock order is manager before heap;
the fixed heap never calls the manager. IRQ/NMI/exception/panic paths may not
acquire either lock or allocate. Deliberate protection faults can occur while
holding the manager lock, and their diagnostics must still finish without it.
This M3 contract predates scheduling. [M5](M5_PROCESSES.md) now preempts user
processes; the kernel and these lock-protected sections remain non-preemptible.
SMP remains unimplemented.

## Validation boundaries

Normal startup checks stack, table ownership, CPU controls and mapping/cache
permissions. Full RAM exhaustion, repeated mapping and container/coalescing
stress run only in `qemu-test` images. `cargo xtask verify` requires all memory
markers in order, then the existing returning traps and eight timer IRQs.

Host tests exercise allocator boundaries, reservations, DMA32, fragmentation,
coalescing, live contents, table import ownership, every partial-map OOM depth,
huge-leaf queries, W^X and ancestor restrictions. QEMU tests touch real mapped
RAM, exhaust and restore every free frame, verify rollback and trigger actual
#PF for stack guard, read-only writes, NX fetches through both managed and HHDM
addresses, and access after unmap. A separate infallible OOM image must reach
the allocation error panic. No injected fault is accepted as successful smoke.

## Primary references

- [Intel SDM Volume 3A](https://cdrdv2-public.intel.com/874249/253668-090-sdm-vol-3a.pdf),
  paging permissions, CR0.WP, NXE, PAT and translation invalidation.
- [Pinned Limine protocol](https://github.com/Limine-Bootloader/limine-protocol/blob/3a0526b700e356f0eac1b71a77697b3fd1c707a3/PROTOCOL.md),
  memory-map ownership and base-revision-3 HHDM coverage.
- [Rust GlobalAlloc](https://doc.rust-lang.org/core/alloc/trait.GlobalAlloc.html)
  and [handle_alloc_error](https://doc.rust-lang.org/alloc/alloc/fn.handle_alloc_error.html),
  layout/lifetime requirements, null failure and no_std's default panic path.
