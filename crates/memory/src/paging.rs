// SPDX-License-Identifier: Apache-2.0
//! Four-level supervisor tables. The backend owns table frames and performs
//! hardware invalidation. Leaf physical storage has a separate lifetime/owner.
use crate::{Error, PAGE_SIZE, PHYSICAL_LIMIT, canonical};
use anova_platform::{PhysAddr, VirtAddr};

pub const ADDRESS: u64 = 0x000f_ffff_ffff_f000;
pub const PRESENT: u64 = 1;
pub const WRITABLE: u64 = 2;
pub const HUGE: u64 = 1 << 7;
pub const NX: u64 = 1 << 63;
const USER: u64 = 1 << 2;
const GLOBAL: u64 = 1 << 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheType {
    WriteBack,
    Uncacheable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Permissions {
    pub writable: bool,
    pub executable: bool,
    pub cache: CacheType,
}

impl Permissions {
    pub const DATA: Self = Self {
        writable: true,
        executable: false,
        cache: CacheType::WriteBack,
    };
    pub const READ_ONLY: Self = Self {
        writable: false,
        ..Self::DATA
    };
    pub const TEXT: Self = Self {
        writable: false,
        executable: true,
        cache: CacheType::WriteBack,
    };
    pub const MMIO: Self = Self {
        cache: CacheType::Uncacheable,
        ..Self::DATA
    };

    fn bits(self) -> Result<u64, Error> {
        if self.writable && self.executable {
            return Err(Error::Permissions);
        }
        Ok(PRESENT
            | if self.writable { WRITABLE } else { 0 }
            | if self.executable { 0 } else { NX }
            | if self.cache == CacheType::Uncacheable {
                (1 << 3) | (1 << 4)
            } else {
                0
            })
    }
}

/// A trusted storage boundary: allocate returns a zeroed, exclusive table;
/// release returns it to its frame owner. Reads may include retained boot
/// tables during import; writes are restricted to owned tables. Invalidation
/// must complete before a removed table frame can be reused on any CPU.
pub trait TableStore {
    fn allocate(&mut self) -> Result<PhysAddr, Error>;
    fn release(&mut self, frame: PhysAddr);
    fn read(&self, frame: PhysAddr, index: usize) -> u64;
    fn write(&mut self, frame: PhysAddr, index: usize, entry: u64);
    fn invalidate(&mut self);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    pub physical: PhysAddr,
    pub page_size: u64,
    pub writable: bool,
    pub executable: bool,
    pub user: bool,
    pub cache_index: u8,
}

pub struct PageTables {
    root: PhysAddr,
    count: usize,
}

impl PageTables {
    /// Clone every non-leaf, including unchanged paths. No imported table frame
    /// remains referenced. Imported leaves retain their original reservations.
    pub fn import(root: PhysAddr, store: &mut impl TableStore) -> Result<Self, Error> {
        check_frame(root)?;
        let mut count = 0;
        let root = clone_table(root, 4, store, &mut count)?;
        Ok(Self { root, count })
    }

    pub fn root(&self) -> PhysAddr {
        self.root
    }
    pub fn table_count(&self) -> usize {
        self.count
    }

    /// Restrict an entire PML4 slot, including inherited huge direct-map leaves.
    /// This only removes execution permission and never creates a new mapping.
    pub fn restrict_slot_to_nx(
        &mut self,
        address: VirtAddr,
        store: &mut impl TableStore,
    ) -> Result<(), Error> {
        if !canonical(address.0) {
            return Err(Error::InvalidAddress);
        }
        let slot = index(address, 4);
        let entry = store.read(self.root, slot);
        if entry & PRESENT != 0 {
            store.write(self.root, slot, entry | NX);
            store.invalidate();
        }
        Ok(())
    }

    pub fn query(&self, address: VirtAddr, store: &impl TableStore) -> Result<Mapping, Error> {
        if !canonical(address.0) {
            return Err(Error::InvalidAddress);
        }
        let mut table = self.root;
        let mut writable = true;
        let mut executable = true;
        let mut user = true;
        for level in (1..=4).rev() {
            let entry = store.read(table, index(address, level));
            if entry & PRESENT == 0 {
                return Err(Error::NotMapped);
            }
            writable &= entry & WRITABLE != 0;
            executable &= entry & NX == 0;
            user &= entry & USER != 0;
            if level == 1 || entry & HUGE != 0 {
                if level > 3 {
                    return Err(Error::HugePage);
                }
                let size = 1_u64 << (12 + 9 * (level - 1));
                let physical = (entry & ADDRESS & !(size - 1)) | (address.0 & (size - 1));
                let pat = if level == 1 {
                    (entry >> 7) & 1
                } else {
                    (entry >> 12) & 1
                };
                return Ok(Mapping {
                    physical: PhysAddr(physical),
                    page_size: size,
                    writable,
                    executable,
                    user,
                    cache_index: (((entry >> 3) & 3) | (pat << 2)) as u8,
                });
            }
            table = PhysAddr(entry & ADDRESS);
        }
        Err(Error::NotMapped)
    }

    pub fn map(
        &mut self,
        address: VirtAddr,
        physical: PhysAddr,
        permissions: Permissions,
        store: &mut impl TableStore,
    ) -> Result<(), Error> {
        check_virtual(address)?;
        check_frame(physical)?;
        let bits = permissions.bits()?;
        insert(
            self.root,
            4,
            address,
            physical.0 | bits,
            store,
            &mut self.count,
        )?;
        store.invalidate();
        Ok(())
    }

    pub fn protect(
        &mut self,
        address: VirtAddr,
        permissions: Permissions,
        store: &mut impl TableStore,
    ) -> Result<(), Error> {
        check_virtual(address)?;
        let bits = permissions.bits()?;
        let table = self.leaf_table(address, bits, store)?;
        let entry = store.read(table, index(address, 1));
        if entry & PRESENT == 0 {
            return Err(Error::NotMapped);
        }
        store.write(table, index(address, 1), (entry & ADDRESS) | bits);
        store.invalidate();
        Ok(())
    }

    fn leaf_table(
        &self,
        address: VirtAddr,
        bits: u64,
        store: &impl TableStore,
    ) -> Result<PhysAddr, Error> {
        let mut table = self.root;
        for level in (2..=4).rev() {
            let entry = store.read(table, index(address, level));
            if entry & PRESENT == 0 {
                return Err(Error::NotMapped);
            }
            if entry & HUGE != 0 {
                return Err(Error::HugePage);
            }
            check_ancestors(entry, bits)?;
            table = PhysAddr(entry & ADDRESS);
        }
        Ok(table)
    }

    pub fn unmap(
        &mut self,
        address: VirtAddr,
        store: &mut impl TableStore,
    ) -> Result<PhysAddr, Error> {
        check_virtual(address)?;
        remove(self.root, 4, address, store, &mut self.count)
    }

    pub fn visit_tables(&self, store: &impl TableStore, mut visit: impl FnMut(PhysAddr)) {
        fn walk(
            frame: PhysAddr,
            level: u32,
            store: &impl TableStore,
            visit: &mut impl FnMut(PhysAddr),
        ) {
            visit(frame);
            if level == 1 {
                return;
            }
            for index in 0..512 {
                let entry = store.read(frame, index);
                if entry & PRESENT != 0 && entry & HUGE == 0 {
                    walk(PhysAddr(entry & ADDRESS), level - 1, store, visit);
                }
            }
        }
        walk(self.root, 4, store, &mut visit);
    }
}

fn check_frame(frame: PhysAddr) -> Result<(), Error> {
    if frame.0 & (PAGE_SIZE - 1) != 0 {
        return Err(Error::Alignment);
    }
    if frame.0 >= PHYSICAL_LIMIT {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

fn check_virtual(address: VirtAddr) -> Result<(), Error> {
    if address.0 & (PAGE_SIZE - 1) != 0 {
        return Err(Error::Alignment);
    }
    if !canonical(address.0) || address.0 < 0xffff_8000_0000_0000 {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

fn index(address: VirtAddr, level: u32) -> usize {
    ((address.0 >> (12 + 9 * (level - 1))) & 511) as usize
}

fn destroy(frame: PhysAddr, level: u32, store: &mut impl TableStore, count: &mut usize) {
    if level > 1 {
        for index in 0..512 {
            let entry = store.read(frame, index);
            if entry & PRESENT != 0 && entry & HUGE == 0 {
                destroy(PhysAddr(entry & ADDRESS), level - 1, store, count);
            }
        }
    }
    store.release(frame);
    *count -= 1;
}

fn clone_table(
    source: PhysAddr,
    level: u32,
    store: &mut impl TableStore,
    count: &mut usize,
) -> Result<PhysAddr, Error> {
    let frame = store.allocate()?;
    *count += 1;
    for index in 0..512 {
        let mut entry = store.read(source, index);
        if entry & PRESENT == 0 {
            continue;
        }
        if level == 4 && entry & HUGE != 0 {
            destroy(frame, level, store, count);
            return Err(Error::HugePage);
        }
        if level > 1 && entry & HUGE == 0 {
            match clone_table(PhysAddr(entry & ADDRESS), level - 1, store, count) {
                Ok(child) => entry = (entry & !ADDRESS) | child.0,
                Err(error) => {
                    destroy(frame, level, store, count);
                    return Err(error);
                }
            }
        }
        // BSP-only supervisor address space; PGE is disabled by activation.
        store.write(frame, index, entry & !USER & !GLOBAL);
    }
    Ok(frame)
}

fn insert(
    table: PhysAddr,
    level: u32,
    address: VirtAddr,
    leaf: u64,
    store: &mut impl TableStore,
    count: &mut usize,
) -> Result<(), Error> {
    let slot = index(address, level);
    let entry = store.read(table, slot);
    if level == 1 {
        if entry & PRESENT != 0 {
            return Err(Error::AlreadyMapped);
        }
        store.write(table, slot, leaf);
        return Ok(());
    }
    if entry & PRESENT != 0 {
        if entry & HUGE != 0 {
            return Err(Error::HugePage);
        }
        check_ancestors(entry, leaf)?;
        return insert(
            PhysAddr(entry & ADDRESS),
            level - 1,
            address,
            leaf,
            store,
            count,
        );
    }
    let child = store.allocate()?;
    *count += 1;
    if let Err(error) = insert(child, level - 1, address, leaf, store, count) {
        destroy(child, level - 1, store, count);
        return Err(error);
    }
    // Publish the complete chain only after every allocation succeeds.
    store.write(table, slot, child.0 | PRESENT | WRITABLE);
    Ok(())
}

fn check_ancestors(entry: u64, leaf: u64) -> Result<(), Error> {
    if (leaf & WRITABLE != 0 && entry & WRITABLE == 0) || (leaf & NX == 0 && entry & NX != 0) {
        return Err(Error::Permissions);
    }
    Ok(())
}

fn remove(
    table: PhysAddr,
    level: u32,
    address: VirtAddr,
    store: &mut impl TableStore,
    count: &mut usize,
) -> Result<PhysAddr, Error> {
    let slot = index(address, level);
    let entry = store.read(table, slot);
    if entry & PRESENT == 0 {
        return Err(Error::NotMapped);
    }
    if level == 1 {
        store.write(table, slot, 0);
        store.invalidate();
        return Ok(PhysAddr(entry & ADDRESS));
    }
    if entry & HUGE != 0 {
        return Err(Error::HugePage);
    }
    let child = PhysAddr(entry & ADDRESS);
    let physical = remove(child, level - 1, address, store, count)?;
    if (0..512).all(|index| store.read(child, index) & PRESENT == 0) {
        store.write(table, slot, 0);
        store.invalidate(); // Flush paging-structure caches before frame reuse.
        store.release(child);
        *count -= 1;
    }
    Ok(physical)
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::{collections::BTreeMap, vec, vec::Vec};

    struct Store {
        pages: BTreeMap<u64, Vec<u64>>,
        next: u64,
        budget: usize,
        flushes: usize,
    }
    impl Store {
        fn new() -> Self {
            Self {
                pages: BTreeMap::new(),
                next: 4096,
                budget: usize::MAX,
                flushes: 0,
            }
        }
    }
    impl TableStore for Store {
        fn allocate(&mut self) -> Result<PhysAddr, Error> {
            if self.budget == 0 {
                return Err(Error::OutOfMemory);
            }
            self.budget -= 1;
            let frame = self.next;
            self.next += PAGE_SIZE;
            self.pages.insert(frame, vec![0; 512]);
            Ok(PhysAddr(frame))
        }
        fn release(&mut self, frame: PhysAddr) {
            assert!(self.pages.remove(&frame.0).is_some());
        }
        fn read(&self, frame: PhysAddr, index: usize) -> u64 {
            self.pages[&frame.0][index]
        }
        fn write(&mut self, frame: PhysAddr, index: usize, entry: u64) {
            self.pages.get_mut(&frame.0).unwrap()[index] = entry;
        }
        fn invalidate(&mut self) {
            self.flushes += 1;
        }
    }

    const VA: VirtAddr = VirtAddr(0xffff_d000_0000_0000);

    fn empty(store: &mut Store) -> PageTables {
        let root = store.allocate().unwrap();
        PageTables { root, count: 1 }
    }

    #[test]
    fn maps_protects_unmaps_and_releases_empty_tables() {
        let mut store = Store::new();
        let mut tables = empty(&mut store);
        tables
            .map(VA, PhysAddr(0x80000), Permissions::DATA, &mut store)
            .unwrap();
        assert_eq!(tables.table_count(), 4);
        let map = tables.query(VirtAddr(VA.0 + 19), &store).unwrap();
        assert_eq!(map.physical, PhysAddr(0x80013));
        assert!(map.writable && !map.executable && !map.user);
        assert_eq!(map.cache_index, 0);
        assert_eq!(
            tables.map(VA, PhysAddr(0x90000), Permissions::DATA, &mut store),
            Err(Error::AlreadyMapped)
        );
        tables.protect(VA, Permissions::TEXT, &mut store).unwrap();
        let map = tables.query(VA, &store).unwrap();
        assert!(!map.writable && map.executable);
        assert_eq!(tables.unmap(VA, &mut store).unwrap(), PhysAddr(0x80000));
        assert_eq!(tables.table_count(), 1);
        assert_eq!(store.pages.len(), 1);
        assert!(store.flushes >= 6);
        assert_eq!(tables.unmap(VA, &mut store), Err(Error::NotMapped));
        tables
            .map(VA, PhysAddr(0xfee00000), Permissions::MMIO, &mut store)
            .unwrap();
        assert_eq!(tables.query(VA, &store).unwrap().cache_index, 3);
    }

    #[test]
    fn map_oom_at_each_depth_does_not_publish_or_leak() {
        for budget in 0..3 {
            let mut store = Store::new();
            let mut tables = empty(&mut store);
            store.budget = budget;
            assert_eq!(
                tables.map(VA, PhysAddr(0x80000), Permissions::DATA, &mut store),
                Err(Error::OutOfMemory)
            );
            assert_eq!(tables.table_count(), 1);
            assert_eq!(store.pages.len(), 1);
            assert_eq!(tables.query(VA, &store), Err(Error::NotMapped));
        }
    }

    #[test]
    fn import_owns_every_table_preserves_huge_leaves_and_rolls_back_oom() {
        let mut store = Store::new();
        let mut source = empty(&mut store);
        source
            .map(VA, PhysAddr(0x80000), Permissions::DATA, &mut store)
            .unwrap();
        let root = source.root();
        let pdpt = PhysAddr(store.read(root, index(VA, 4)) & ADDRESS);
        store.write(pdpt, 1, 0x4000_0000 | PRESENT | HUGE | WRITABLE | NX);
        let before = store.pages.len();
        for budget in 0..4 {
            store.budget = budget;
            assert!(matches!(
                PageTables::import(root, &mut store),
                Err(Error::OutOfMemory)
            ));
            assert_eq!(store.pages.len(), before);
        }
        store.budget = usize::MAX;
        let imported = PageTables::import(root, &mut store).unwrap();
        let mut old = Vec::new();
        source.visit_tables(&store, |frame| old.push(frame));
        imported.visit_tables(&store, |frame| assert!(!old.contains(&frame)));
        assert_eq!(imported.table_count(), 4);
        assert_eq!(imported.query(VA, &store), source.query(VA, &store));
        let huge = imported
            .query(VirtAddr(VA.0 + (1 << 30) + 99), &store)
            .unwrap();
        assert_eq!(huge.page_size, 1 << 30);
        assert_eq!(huge.physical, PhysAddr(0x4000_0063));
    }

    #[test]
    fn rejects_bad_addresses_permissions_and_huge_edits() {
        let mut store = Store::new();
        let mut tables = empty(&mut store);
        for (address, physical, permissions, error) in [
            (
                VirtAddr(VA.0 + 1),
                PhysAddr(0),
                Permissions::DATA,
                Error::Alignment,
            ),
            (
                VirtAddr(0x8000_0000_0000),
                PhysAddr(0),
                Permissions::DATA,
                Error::InvalidAddress,
            ),
            (
                VirtAddr(0),
                PhysAddr(0),
                Permissions::DATA,
                Error::InvalidAddress,
            ),
            (
                VA,
                PhysAddr(1 << 52),
                Permissions::DATA,
                Error::InvalidAddress,
            ),
            (VA, PhysAddr(1), Permissions::DATA, Error::Alignment),
            (
                VA,
                PhysAddr(0),
                Permissions {
                    writable: true,
                    ..Permissions::TEXT
                },
                Error::Permissions,
            ),
        ] {
            assert_eq!(
                tables.map(address, physical, permissions, &mut store),
                Err(error)
            );
            assert_eq!(tables.table_count(), 1);
        }
        let child = store.allocate().unwrap();
        store.write(tables.root, index(VA, 4), child.0 | PRESENT | WRITABLE);
        store.write(child, index(VA, 3), PRESENT | HUGE);
        assert_eq!(
            tables.map(VA, PhysAddr(0), Permissions::DATA, &mut store),
            Err(Error::HugePage)
        );
        assert_eq!(tables.unmap(VA, &mut store), Err(Error::HugePage));
        assert_eq!(
            tables.protect(VA, Permissions::TEXT, &mut store),
            Err(Error::HugePage)
        );
        assert!(crate::align_up(u64::MAX, 4096).is_err());
    }

    #[test]
    fn ancestor_permissions_cannot_be_silently_bypassed() {
        let mut store = Store::new();
        let mut tables = empty(&mut store);
        tables
            .map(VA, PhysAddr(0x80000), Permissions::READ_ONLY, &mut store)
            .unwrap();
        let slot = index(VA, 4);
        let parent = store.read(tables.root, slot);
        store.write(tables.root, slot, parent & !WRITABLE);
        tables.restrict_slot_to_nx(VA, &mut store).unwrap();
        assert_eq!(
            tables.protect(VA, Permissions::DATA, &mut store),
            Err(Error::Permissions)
        );
        assert_eq!(
            tables.protect(VA, Permissions::TEXT, &mut store),
            Err(Error::Permissions)
        );
        assert_eq!(
            tables.map(
                VirtAddr(VA.0 + 4096),
                PhysAddr(0x90000),
                Permissions::DATA,
                &mut store
            ),
            Err(Error::Permissions)
        );
        let mapping = tables.query(VA, &store).unwrap();
        assert!(!mapping.writable && !mapping.executable);
        assert_eq!(tables.table_count(), 4);
    }
}
