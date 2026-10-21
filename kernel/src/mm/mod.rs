// SPDX-License-Identifier: Apache-2.0
//! BSP memory ownership: checked USABLE frames, fully owned table hierarchy,
//! guarded stacks, buddy heap, private process memory and explicitly owned coherent DMA pages.
mod lock;
pub mod uaccess;
pub mod user;
mod validation;

pub use validation::{inject_fault, verify};

use crate::{
    arch::x86_64::{
        cpu,
        paging::{EarlyMapper, IOAPIC_WINDOW, LAPIC_WINDOW},
        traps,
    },
    boot::BootInfo,
    serial,
};
use anova_memory::{
    Error, PAGE_SIZE, align_up, canonical,
    frames::{self, FrameAllocator, Owner, PhysRange, Zone},
    heap::BuddyHeap,
    paging::{PageTables, Permissions, TableStore},
};
use anova_platform::{PhysAddr, VirtAddr};
use core::{
    alloc::{GlobalAlloc, Layout},
    arch::{asm, naked_asm, x86_64::__cpuid},
};
use limine::memory_map::EntryType;
use lock::IrqLock;

const MANAGED_BASE: u64 = 0xffff_d000_0000_0000;
pub const STACK_GUARD: u64 = MANAGED_BASE + 0x10000;
const STACK_BOTTOM: u64 = STACK_GUARD + PAGE_SIZE;
const STACK_BYTES: u64 = 64 * 1024;
const STACK_TOP: u64 = STACK_BOTTOM + STACK_BYTES;
const HEAP_BASE: u64 = MANAGED_BASE + 0x20_0000;
const HEAP_BYTES: usize = 2 * 1024 * 1024;
const TEST_BASE: u64 = MANAGED_BASE + 0x80_0000;
#[cfg(feature = "qemu-test")]
const ROLLBACK_BASE: u64 = MANAGED_BASE + (1 << 39);

struct MemoryManager {
    frames: FrameAllocator<'static>,
    tables: PageTables,
    direct: VirtAddr,
}
static MEMORY: IrqLock<Option<MemoryManager>> = IrqLock::new(None);
static HEAP: IrqLock<BuddyHeap> = IrqLock::new(BuddyHeap::new());

pub fn heap_used() -> usize {
    HEAP.with(|heap| heap.used())
}

/// One physical, WB-coherent x86 DMA page, never borrowed from the heap or a process.
pub struct DmaPage {
    physical: PhysAddr,
    pointer: *mut u8,
}
impl DmaPage {
    pub fn new() -> Result<Self, i64> {
        MEMORY.with(|slot| {
            let manager = slot.as_mut().ok_or(anova_abi::NO_MEMORY)?;
            let physical = manager
                .frames
                .allocate(Owner::Dma, Zone::Dma32)
                .map_err(|_| anova_abi::NO_MEMORY)?;
            let pointer = direct_pointer(manager.direct, physical);
            // SAFETY: Exclusively owned, HHDM-mapped USABLE RAM; no device has seen this address yet.
            unsafe {
                pointer.write_bytes(0, PAGE_SIZE as usize);
            }
            Ok(Self { physical, pointer })
        })
    }
    pub fn physical(&self) -> u64 {
        self.physical.0
    }
    pub fn pointer(&self) -> *mut u8 {
        self.pointer
    }
}
impl Drop for DmaPage {
    fn drop(&mut self) {
        // The device owner must confirm reset/completion before releasing this wrapper.
        MEMORY.with(|slot| {
            slot.as_mut()
                .expect("Missing memory manager")
                .frames
                .release(self.physical, Owner::Dma)
                .expect("Invalid DMA frame release")
        });
    }
}

struct KernelAllocator;
#[global_allocator]
static ALLOCATOR: KernelAllocator = KernelAllocator;

// SAFETY: The buddy heap exclusively owns its mapped RAM. The IRQ-saving lock
// serializes allocation/free, all layouts are honored, and no code here unwinds
// or allocates recursively. Release requires the GlobalAlloc caller's contract.
unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        HEAP.with(|heap| {
            let pointer = heap.allocate(layout);
            if pointer.is_null() {
                serial::emergency(format_args!(
                    "[MM:OOM] size={} align={} used={} capacity={}\n",
                    layout.size(),
                    layout.align(),
                    heap.used(),
                    heap.capacity()
                ));
            }
            pointer
        })
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        HEAP.with(|heap| {
            // SAFETY: The GlobalAlloc caller provides a live pointer from this
            // heap and the original layout. HEAP grants exclusive access.
            unsafe { heap.release(pointer, layout) };
        });
    }
}

struct Store<'a, 'b> {
    frames: &'a mut FrameAllocator<'static>,
    direct: VirtAddr,
    boot: Option<&'b BootInfo>,
    active: bool,
}

impl Store<'_, '_> {
    fn pointer(&self, frame: PhysAddr) -> *mut u64 {
        if self.frames.owner(frame) == Ok(Some(Owner::PageTable)) {
            direct_pointer(self.direct, frame).cast()
        } else {
            self.boot
                .expect("Unowned table after import")
                .table_pointer(frame)
                .cast_mut()
        }
    }
}

impl TableStore for Store<'_, '_> {
    fn allocate(&mut self) -> Result<PhysAddr, Error> {
        let frame = self.frames.allocate(Owner::PageTable, Zone::Any)?;
        // SAFETY: This exclusively allocated USABLE frame is present in HHDM.
        // It contains no live data; zeroing prepares all 512 absent entries.
        unsafe { direct_pointer(self.direct, frame).write_bytes(0, PAGE_SIZE as usize) };
        Ok(frame)
    }
    fn release(&mut self, frame: PhysAddr) {
        self.frames
            .release(frame, Owner::PageTable)
            .expect("Invalid table release");
    }
    fn read(&self, frame: PhysAddr, index: usize) -> u64 {
        assert!(index < 512, "Page table index overflow");
        // SAFETY: pointer validates this retained or owned 4 KiB table. Reads
        // are aligned/bounded; volatile accounts for CPU-set accessed bits.
        unsafe { self.pointer(frame).add(index).read_volatile() }
    }
    fn write(&mut self, frame: PhysAddr, index: usize, entry: u64) {
        assert_eq!(
            self.frames.owner(frame),
            Ok(Some(Owner::PageTable)),
            "Write to borrowed table"
        );
        assert!(index < 512, "Page table index overflow");
        // SAFETY: Only owned aligned table entries are edited, serialized at
        // IF=0. No AP runs; invalidation follows publication or unlinking.
        unsafe { self.pointer(frame).add(index).write_volatile(entry) };
    }
    fn invalidate(&mut self) {
        assert_eq!(cpu::flags() & (1 << 9), 0, "Table edits require IF=0");
        if self.active {
            // SAFETY: CPL0, sole CPU, PCID/PGE disabled. Reload flushes both
            // TLB and paging-structure caches before removed frames are freed.
            unsafe {
                asm!("mov {root}, cr3", "mov cr3, {root}", root = out(reg) _, options(nostack, preserves_flags));
            }
        }
    }
}

fn direct_pointer(direct: VirtAddr, frame: PhysAddr) -> *mut u8 {
    let address = direct
        .0
        .checked_add(frame.0)
        .expect("HHDM address overflow");
    assert!(canonical(address), "Noncanonical HHDM address");
    address as *mut u8
}

impl MemoryManager {
    fn store(&mut self) -> Store<'_, 'static> {
        Store {
            frames: &mut self.frames,
            direct: self.direct,
            boot: None,
            active: true,
        }
    }

    fn map_page(&mut self, address: VirtAddr, owner: Owner) -> Result<PhysAddr, Error> {
        let frame = self.frames.allocate(owner, Zone::Any)?;
        // SAFETY: This newly owned USABLE page has no live contents, is mapped
        // through the retained HHDM, and is not allocator metadata.
        unsafe { direct_pointer(self.direct, frame).write_bytes(0, PAGE_SIZE as usize) };
        let mut store = Store {
            frames: &mut self.frames,
            direct: self.direct,
            boot: None,
            active: true,
        };
        if let Err(error) = self
            .tables
            .map(address, frame, Permissions::DATA, &mut store)
        {
            self.frames.release(frame, owner)?;
            return Err(error);
        }
        Ok(frame)
    }

    fn unmap_page(&mut self, address: VirtAddr, owner: Owner) -> Result<PhysAddr, Error> {
        let store = Store {
            frames: &mut self.frames,
            direct: self.direct,
            boot: None,
            active: true,
        };
        let physical = self.tables.query(address, &store)?.physical;
        if self.frames.owner(physical)? != Some(owner) {
            return Err(Error::WrongOwner);
        }
        let mut store = Store {
            frames: &mut self.frames,
            direct: self.direct,
            boot: None,
            active: true,
        };
        let frame = self.tables.unmap(address, &mut store)?;
        self.frames.release(frame, owner)?;
        Ok(frame)
    }

    fn map_range(&mut self, start: VirtAddr, bytes: u64, owner: Owner) -> Result<(), Error> {
        let end = start.0.checked_add(bytes).ok_or(Error::Overflow)?;
        if bytes == 0 || (start.0 | bytes) & (PAGE_SIZE - 1) != 0 || !canonical(end) {
            return Err(Error::InvalidRange);
        }
        let mut mapped = 0;
        while mapped < bytes {
            if let Err(error) = self.map_page(VirtAddr(start.0 + mapped), owner) {
                for offset in (0..mapped).step_by(PAGE_SIZE as usize) {
                    self.unmap_page(VirtAddr(start.0 + offset), owner)?;
                }
                return Err(error);
            }
            mapped += PAGE_SIZE;
        }
        Ok(())
    }
}

pub fn init(info: &BootInfo, early: EarlyMapper<'_>) -> u64 {
    assert_eq!(
        cpu::flags() & (1 << 9),
        0,
        "Memory initialization requires IF=0"
    );
    // SAFETY: CPUID exists in long mode. A missing address-width leaf uses the
    // architectural 36-bit baseline; all admitted RAM must fit the CPU width.
    let max = unsafe { __cpuid(0x8000_0000) }.eax;
    // SAFETY: The maximum extended leaf was checked above.
    let width = if max >= 0x8000_0008 {
        unsafe { __cpuid(0x8000_0008) }.eax & 255
    } else {
        36
    };
    assert!(
        (36..=52).contains(&width),
        "Unsupported physical address width"
    );
    assert_eq!(cpu::read_msr(0x277) & 255, 6, "PAT index 0 must be WB");
    assert_eq!(
        (cpu::read_msr(0x277) >> 24) & 255,
        0,
        "PAT index 3 must be UC"
    );

    let empty = PhysRange {
        start: PhysAddr(0),
        length: 0,
    };
    let mut ranges = [empty; frames::MAX_REGIONS];
    let mut count = 0;
    for entry in info.entries() {
        if entry.entry_type == EntryType::USABLE {
            assert!(count < ranges.len(), "Too many usable RAM regions");
            let end = entry
                .base
                .checked_add(entry.length)
                .expect("RAM range overflow");
            assert!(end <= 1_u64 << width, "RAM exceeds physical CPU width");
            let virtual_end = info
                .direct_map()
                .0
                .checked_add(end - 1)
                .expect("HHDM span overflow");
            assert!(canonical(virtual_end), "HHDM RAM span is noncanonical");
            ranges[count] = PhysRange {
                start: PhysAddr(entry.base),
                length: entry.length,
            };
            count += 1;
        }
    }
    let ranges = &ranges[..count];
    let metadata_bytes = frames::metadata_bytes(ranges).expect("Invalid usable RAM map");
    let metadata_span =
        align_up(metadata_bytes as u64, PAGE_SIZE).expect("Frame metadata overflow");
    let start = ranges
        .iter()
        .find(|r| r.start.0 != 0 && r.length >= metadata_span)
        .expect("Insufficient RAM for frame metadata")
        .start;
    let reservation = PhysRange {
        start,
        length: metadata_span,
    };
    // SAFETY: The selected page-aligned USABLE span is disjoint from all boot
    // reservations and has never been allocated. It stays permanently reserved,
    // HHDM-mapped and exclusively borrowed by the frame allocator's metadata.
    let states = unsafe {
        core::slice::from_raw_parts_mut(direct_pointer(info.direct_map(), start), metadata_bytes)
    };
    let mut frames =
        FrameAllocator::new(ranges, &[reservation], states).expect("Frame allocator setup failed");
    serial::write(format_args!(
        "[MM:FRAMES] total={} reserved={} free={} metadata={:#x}..{:#x} boot_reclaim=false\n",
        frames.stats().total,
        frames.stats().reserved,
        frames.stats().free,
        start.0,
        start.0 + metadata_span
    ));

    let original = early.into_root();
    let mut tables = PageTables::import(
        original,
        &mut Store {
            frames: &mut frames,
            direct: info.direct_map(),
            boot: Some(info),
            active: false,
        },
    )
    .expect("Owned page table import failed");
    // Limine's HHDM may allow instruction fetches. Restrict all its root slots,
    // including huge mappings, so owned RW RAM has no executable direct alias.
    // Keep the complete direct region below the separate ACPI/MMIO window.
    let direct = info.direct_map().0;
    let mut direct_end = direct;
    for entry in info.entries() {
        if matches!(
            entry.entry_type,
            EntryType::USABLE
                | EntryType::BOOTLOADER_RECLAIMABLE
                | EntryType::EXECUTABLE_AND_MODULES
                | EntryType::FRAMEBUFFER
        ) {
            let end = direct
                .checked_add(entry.base)
                .and_then(|start| start.checked_add(entry.length))
                .expect("Direct mapping span overflow");
            direct_end = direct_end.max(end);
        }
    }
    assert!(
        direct < direct_end && direct_end <= 0xffff_c000_0000_0000,
        "Direct mapping overlaps reserved virtual ranges"
    );
    let mut import_store = Store {
        frames: &mut frames,
        direct: info.direct_map(),
        boot: None,
        active: false,
    };
    for slot in (direct >> 39)..=((direct_end - 1) >> 39) {
        tables
            .restrict_slot_to_nx(VirtAddr(slot << 39), &mut import_store)
            .expect("Direct mapping NX restriction failed");
    }
    let root = tables.root();
    // SAFETY: All present paths have been cloned to owned pages; leaves still
    // map code, the current boot stack, HHDM, IST and APIC. Disable PGE (flush
    // global entries), then activate the root and enable supervisor write checks.
    unsafe {
        let mut cr4: u64;
        let mut cr0: u64;
        asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
        cr4 &= !(1 << 7);
        asm!("mov cr4, {}", in(reg) cr4, options(nostack, preserves_flags));
        asm!("mov cr3, {}", in(reg) root.0, options(nostack, preserves_flags));
        asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack, preserves_flags));
        cr0 |= 1 << 16;
        asm!("mov cr0, {}", in(reg) cr0, options(nostack, preserves_flags));
    }
    let mut manager = MemoryManager {
        frames,
        tables,
        direct: info.direct_map(),
    };
    // A new PML4 slot must be wholly unused before assigning managed ranges.
    let managed_slot = ((MANAGED_BASE >> 39) & 511) as usize;
    assert_eq!(
        manager.store().read(root, managed_slot) & 1,
        0,
        "Managed address range overlaps boot maps"
    );
    manager
        .map_range(VirtAddr(STACK_BOTTOM), STACK_BYTES, Owner::Stack)
        .expect("Kernel stack allocation failed");
    manager
        .map_range(VirtAddr(HEAP_BASE), HEAP_BYTES as u64, Owner::Heap)
        .expect("Heap mapping failed");
    serial::write(format_args!(
        "[MM:TABLES] root={:#x} owned_tables={} boot_table_refs=0 NX=on WP=on PGE=off direct_map=NX\n",
        root.0,
        manager.tables.table_count()
    ));
    MEMORY.with(|slot| {
        assert!(slot.is_none(), "Memory manager already initialized");
        *slot = Some(manager);
    });
    HEAP.with(|heap| {
        // SAFETY: The complete, aligned 2 MiB virtual span is newly zeroed,
        // exclusively heap-owned RW/NX RAM. No allocation has occurred yet.
        unsafe {
            heap.init(HEAP_BASE as *mut u8, HEAP_BYTES)
                .expect("Heap initialization failed")
        };
    });
    traps::set_kernel_stack(STACK_TOP);
    serial::write(format_args!(
        "[MM:HEAP] base={HEAP_BASE:#x} bytes={HEAP_BYTES} buddy=true RW=true NX=true\n"
    ));
    STACK_TOP
}

/// Enter a fresh System V stack without returning into the retained boot stack.
///
/// # Safety
/// `top` must be the aligned, owned stack returned by init. The BSP must have
/// IF=0 and must never return to or reference local data on the old stack.
#[unsafe(naked)]
pub unsafe extern "C" fn enter_stack(_top: u64, _timer_port: u64, _timer_mask: u64) -> ! {
    // SAFETY: init mapped this 16-byte-aligned, exclusively owned stack with
    // guards at both ends. CALL supplies the ABI return-address alignment; the
    // diverging entry owns all further stack frames. No old frame is referenced.
    naked_asm!("mov rsp, rdi", "xor ebp, ebp", "mov rdi, rsi", "mov rsi, rdx", "call {}", "ud2", sym crate::kernel_main);
}
