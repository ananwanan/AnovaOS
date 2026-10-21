// SPDX-License-Identifier: Apache-2.0
//! A bounded BSP-only copy-on-write extension of the retained Limine tables.
//! M3 must replace this pool with explicit address-space ownership before reclaiming memory.
use super::cpu;
use crate::{boot::BootInfo, serial};
use anova_platform::{PhysAddr, VirtAddr, acpi::MAX_TABLE};
use core::{
    arch::{asm, x86_64::__cpuid},
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};

const ADDRESS: u64 = 0x000f_ffff_ffff_f000;
const PRESENT: u64 = 1;
const WRITABLE: u64 = 2;
const HUGE: u64 = 1 << 7;
const NX: u64 = 1 << 63;
const WINDOW: u64 = 0xffff_c000_0000_0000;
pub const LAPIC_WINDOW: u64 = WINDOW + 0x20_0000;
pub const IOAPIC_WINDOW: u64 = WINDOW + 0x40_0000;
const POOL_SIZE: usize = 32;

#[derive(Clone, Copy)]
#[repr(C, align(4096))]
struct PageTable([u64; 512]);
struct Pool(UnsafeCell<[PageTable; POOL_SIZE]>);
// SAFETY: Access is granted once by TAKEN, then restricted to the BSP with IF=0.
// The CPU page walker only reads these retained, aligned table pages.
unsafe impl Sync for Pool {}
static POOL: Pool = Pool(UnsafeCell::new([PageTable([0; 512]); POOL_SIZE]));
static TAKEN: AtomicBool = AtomicBool::new(false);

pub struct EarlyMapper<'a> {
    info: &'a BootInfo,
    tables: &'static mut [PageTable; POOL_SIZE],
    used: usize,
    physical_base: u64,
}

impl<'a> EarlyMapper<'a> {
    pub fn new(info: &'a BootInfo) -> Self {
        assert_eq!(cpu::flags() & (1 << 9), 0, "Early mapping requires IF=0");
        assert!(
            !TAKEN.swap(true, Ordering::AcqRel),
            "Early mapper already taken"
        );
        // SAFETY: CPUID is available in long mode and has no memory side effects.
        let extended = unsafe { __cpuid(0x8000_0000) };
        assert!(
            extended.eax >= 0x8000_0001,
            "Extended CPU features unavailable"
        );
        // SAFETY: Leaf 1 exists on x86_64. PAT must exist before reading its MSR.
        assert!(
            unsafe { __cpuid(1) }.edx & (1 << 16) != 0,
            "PAT is required"
        );
        // SAFETY: The leaf is supported according to the preceding maximum.
        assert!(
            unsafe { __cpuid(0x8000_0001) }.edx & (1 << 20) != 0,
            "NX is required"
        );
        cpu::write_msr(0xc000_0080, cpu::read_msr(0xc000_0080) | (1 << 11));
        let original: u64;
        let cr4: u64;
        // SAFETY: CPL0; the bootloader supplied a valid four-level CR3.
        unsafe {
            asm!("mov {}, cr3", out(reg) original, options(nomem, nostack, preserves_flags));
            asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
        }
        assert_eq!(
            cr4 & ((1 << 12) | (1 << 17)),
            0,
            "LA57 and PCID must be disabled"
        );
        let physical_base = info.kernel_physical(VirtAddr(POOL.0.get() as u64)).0;
        // SAFETY: The atomic one-time claim grants unique mutable access. No AP
        // starts, and no handler accesses POOL while page-table edits run at IF=0.
        let tables = unsafe { &mut *POOL.0.get() };
        let mapper = Self {
            info,
            tables,
            used: 1,
            physical_base,
        };
        let source = info.table_pointer(PhysAddr(original & ADDRESS));
        for index in 0..512 {
            // SAFETY: table_pointer validates the retained 4 KiB table; each
            // aligned u64 is inside it. No one writes Limine's original table.
            mapper.tables[0].0[index] = unsafe { source.add(index).read_volatile() };
        }
        assert_eq!(
            mapper.tables[0].0[((WINDOW >> 39) & 511) as usize] & PRESENT,
            0,
            "Early ACPI/MMIO virtual slot overlaps an existing boot mapping"
        );
        // SAFETY: The cloned root preserves every active mapping; its aligned
        // physical page is permanently kernel-reserved. No PCID is enabled.
        unsafe {
            asm!("mov cr3, {}", in(reg) physical_base, options(nostack, preserves_flags));
        }
        serial::write(format_args!(
            "[CPU:PAGING] retained Limine maps; private root={physical_base:#x} NX=on\n"
        ));
        mapper
    }

    fn allocate(&mut self) -> usize {
        assert!(self.used < POOL_SIZE, "Early page table pool exhausted");
        let index = self.used;
        self.used += 1;
        self.tables[index] = PageTable([0; 512]);
        index
    }

    fn physical(&self, index: usize) -> u64 {
        self.physical_base + index as u64 * 4096
    }

    fn child(&mut self, parent: usize, index: usize, level: u32) -> usize {
        let entry = self.tables[parent].0[index];
        let address = entry & ADDRESS;
        if entry & PRESENT != 0
            && entry & HUGE == 0
            && (self.physical_base..self.physical_base + self.used as u64 * 4096).contains(&address)
        {
            return ((address - self.physical_base) / 4096) as usize;
        }
        let child = self.allocate();
        let flags = if entry & PRESENT == 0 {
            PRESENT | WRITABLE
        } else {
            entry & !ADDRESS & !HUGE
        };
        if entry & PRESENT != 0 {
            if entry & HUGE != 0 {
                assert!(level == 3 || level == 2, "Unexpected huge page level");
                let shift = 12 + 9 * (level - 1);
                let base = address & !((1_u64 << shift) - 1);
                let mut leaf_flags = entry & !ADDRESS;
                // Large-page PAT (bit 12) becomes PTE PAT (bit 7) at 4 KiB.
                if level == 2 {
                    leaf_flags &= !HUGE;
                    if entry & (1 << 12) != 0 {
                        leaf_flags |= 1 << 7;
                    }
                } else if entry & (1 << 12) != 0 {
                    leaf_flags |= 1 << 12;
                }
                for offset in 0..512 {
                    self.tables[child].0[offset] =
                        (base + ((offset as u64) << (shift - 9))) | leaf_flags;
                }
            } else {
                let source = self.info.table_pointer(PhysAddr(address));
                for offset in 0..512 {
                    // SAFETY: The validated, aligned retained table has 512
                    // entries. It is copied before changing the parent link.
                    self.tables[child].0[offset] = unsafe { source.add(offset).read_volatile() };
                }
            }
        }
        self.tables[parent].0[index] = self.physical(child) | flags;
        child
    }

    fn set_leaf(&mut self, address: VirtAddr, entry: u64) {
        assert_eq!(cpu::flags() & (1 << 9), 0, "Page edits require IF=0");
        assert_eq!(address.0 & 0xfff, 0, "Unaligned virtual mapping");
        let mut table = 0;
        for level in (2..=4).rev() {
            let index = ((address.0 >> (12 + 9 * (level - 1))) & 511) as usize;
            table = self.child(table, index, level);
        }
        self.tables[table].0[((address.0 >> 12) & 511) as usize] = entry;
        // SAFETY: CPL0; this invalidates the edited page on the sole CPU. The
        // copied ancestors preserve other mappings; no AP or PCID needs a flush.
        unsafe {
            asm!("invlpg [{}]", in(reg) address.0, options(nostack, preserves_flags));
        }
    }

    pub fn guard(&mut self, address: VirtAddr) {
        self.set_leaf(address, 0);
    }

    pub fn mmio(&mut self, physical: PhysAddr, virtual_address: VirtAddr) {
        assert!(
            physical.0 != 0 && physical.0 & 0xfff == 0 && physical.0 & !ADDRESS == 0,
            "Invalid MMIO physical page"
        );
        // PAT index 3 (PCD|PWT) must be UC before device mapping. Do not create
        // a conflicting alias through the restrictive base-revision-3 HHDM.
        assert_eq!(
            (cpu::read_msr(0x277) >> 24) & 0xff,
            0,
            "PAT index 3 must be UC"
        );
        self.info.check_mmio(physical);
        self.set_leaf(
            virtual_address,
            physical.0 | PRESENT | WRITABLE | NX | (1 << 3) | (1 << 4),
        );
    }

    pub fn firmware(&mut self, physical: PhysAddr, length: usize) -> &[u8] {
        assert!(
            length > 0 && length <= MAX_TABLE,
            "Firmware mapping too large"
        );
        self.info.check_firmware(physical, length);
        let offset = physical.0 & 0xfff;
        let first = physical.0 & !0xfff;
        let pages = (offset + length as u64).div_ceil(4096);
        for page in 0..pages {
            self.set_leaf(
                VirtAddr(WINDOW + page * 4096),
                (first + page * 4096) | PRESENT | NX,
            );
        }
        // SAFETY: The requested reserved firmware span is mapped read-only NX
        // above. Its bounded slice borrow prevents remapping the window until
        // all consumers release it. No firmware writes occur after handoff.
        unsafe { core::slice::from_raw_parts((WINDOW + offset) as *const u8, length) }
    }
}

// Keep all early table storage live; this assertion also fixes page stride.
const _: () = assert!(size_of::<PageTable>() == 4096);
