// SPDX-License-Identifier: Apache-2.0
//! Own user roots, coherent readonly file/anonymous shared pages and guarded task kernel stacks.
//! BSP mutations occur on the permanent kernel root at IF=0; user roots share upper paths.
use super::*;
use alloc::vec::Vec;
use anova_abi::UserAddr;
use anova_memory::shared::Shared;
use core::cell::RefCell;
mod file_mapping;
use file_mapping::FilePage;

// Trap capture returns to the separate 256 KiB scheduler stack before deep exec/ELF work.
const STACK_BYTES: u64 = 64 * 1024;
const GUARD: u64 = MANAGED_BASE + 0x100_0000;
const STRIDE: u64 = STACK_BYTES + 2 * PAGE_SIZE;

pub struct KernelStack {
    bottom: u64,
}
impl KernelStack {
    pub fn new(slot: usize) -> Result<Self, Error> {
        assert!(slot < anova_sched::CAPACITY);
        let bottom = GUARD + slot as u64 * STRIDE + PAGE_SIZE;
        MEMORY.with(|memory| {
            memory.as_mut().expect("Memory not initialized").map_range(
                VirtAddr(bottom),
                STACK_BYTES,
                Owner::ProcessStack,
            )
        })?;
        Ok(Self { bottom })
    }
    pub fn top(&self) -> u64 {
        self.bottom + STACK_BYTES
    }
}
impl Drop for KernelStack {
    fn drop(&mut self) {
        MEMORY.with(|slot| {
            let manager = slot.as_mut().expect("Memory not initialized");
            for address in (self.bottom..self.top()).step_by(PAGE_SIZE as usize) {
                manager
                    .unmap_page(VirtAddr(address), Owner::ProcessStack)
                    .expect("Invalid process stack release");
            }
        });
    }
}
#[cfg(feature = "qemu-test")]
static BUDGET: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(usize::MAX);
#[cfg(feature = "qemu-test")]
pub fn set_test_budget(value: usize) {
    BUDGET.store(value, core::sync::atomic::Ordering::Relaxed);
}

struct OwnedSpace {
    tables: Option<PageTables>,
    pages: Vec<(VirtAddr, Shared<UserFrame>)>,
    next_mapping: u64,
    file_pages: Vec<FilePage>,
}

struct UserFrame {
    identity: usize,
    address: PhysAddr,
    shared: bool,
}
impl Drop for UserFrame {
    fn drop(&mut self) {
        MEMORY.with(|slot| {
            slot.as_mut()
                .expect("Memory not initialized")
                .frames
                .release(self.address, Owner::User)
                .expect("Invalid user frame release")
        });
    }
}

/// Shared roots are mutated only at IF=0 on the scheduler root; RefCell enforces borrow exclusivity.
pub struct UserSpace(Shared<RefCell<OwnedSpace>>);
impl UserSpace {
    pub fn new() -> Result<Self, Error> {
        Ok(Self(Shared::try_new(RefCell::new(OwnedSpace::new()?))?))
    }
    pub fn share(&self) -> Self {
        Self(self.0.clone())
    }
    pub fn identity(&self) -> usize {
        Shared::as_ptr(&self.0) as usize
    }
    pub fn root(&self) -> PhysAddr {
        self.0.borrow().root()
    }
    pub fn clone_eager(&self) -> Result<Self, Error> {
        Ok(Self(Shared::try_new(RefCell::new(
            self.0.borrow().clone_eager()?,
        ))?))
    }
    pub fn with_active<R>(&self, action: impl FnOnce() -> R) -> R {
        // Only immutable borrows survive activation; mutations run on the permanent root.
        self.0.borrow().with_active(action)
    }
    pub fn map(&mut self, address: u64, permissions: Permissions) -> Result<(), Error> {
        self.0.borrow_mut().map(address, permissions)
    }
    pub fn unmap(&mut self, address: u64) -> Result<(), Error> {
        self.0.borrow_mut().unmap(address)
    }
    pub fn map_anonymous(&mut self, start: u64, end: u64) -> Result<(), Error> {
        self.0.borrow_mut().map_anonymous(start, end)
    }
    pub fn initialize(&self, address: u64, bytes: &[u8]) -> Result<(), Error> {
        self.0.borrow().initialize(address, bytes)
    }
    pub fn accessible(&self, address: UserAddr, length: usize, write: bool, execute: bool) -> bool {
        self.0.borrow().accessible(address, length, write, execute)
    }
    pub fn mapped(&self, address: u64) -> bool {
        self.0.borrow().mapped(address)
    }
    /// Search after the last successful mapping, then wrap over the entire window.
    /// PROT_NONE pages remain occupied. No reservation is published before allocation succeeds.
    pub fn find_gap(&self, start: u64, end: u64, size: u64) -> Option<u64> {
        if size == 0 || start >= end || (start | end | size) & 4095 != 0 || size > end - start {
            return None;
        }
        let space = self.0.borrow();
        let cursor = if (start..end).contains(&space.next_mapping) {
            space.next_mapping
        } else {
            start
        };
        MEMORY.with(|slot| {
            let manager = slot.as_mut().expect("Memory not initialized");
            let store = manager.store();
            let search = |from| {
                let mut base = from;
                let mut length = 0;
                for page in (from..end).step_by(4096) {
                    if space
                        .tables
                        .as_ref()
                        .unwrap()
                        .query(VirtAddr(page), &store)
                        .is_ok()
                    {
                        base = page + 4096;
                        length = 0;
                    } else {
                        length += 4096;
                        if length == size {
                            return Some(base);
                        }
                    }
                }
                None
            };
            search(cursor).or_else(|| if cursor != start { search(start) } else { None })
        })
    }
    pub fn commit_mapping_hint(&mut self, end: u64) {
        self.0.borrow_mut().next_mapping = end;
    }
    pub fn share_mapping(&mut self, start: u64, end: u64) {
        for (address, frame) in &mut self.0.borrow_mut().pages {
            if (start..end).contains(&address.0) {
                Shared::get_mut(frame)
                    .expect("New mapping already shared")
                    .shared = true;
            }
        }
    }
    pub fn futex_key(&self, address: u64, private: bool) -> Option<(usize, u64)> {
        if private {
            return Some((self.identity(), address));
        }
        self.0
            .borrow()
            .pages
            .iter()
            .find(|(v, _)| v.0 == address & !4095)
            .map(|(_, frame)| (frame.identity, address & 4095))
    }
    pub fn shared_backing(&self, address: u64) -> bool {
        self.0
            .borrow()
            .pages
            .iter()
            .any(|(v, frame)| v.0 == address && frame.shared)
    }
    pub fn protect(
        &mut self,
        start: u64,
        end: u64,
        permissions: Option<Permissions>,
    ) -> Result<(), Error> {
        let mut space = self.0.borrow_mut();
        if permissions.is_some_and(|p| p.writable)
            && space
                .file_pages
                .iter()
                .any(|p| (start..end).contains(&p.address.0))
        {
            return Err(Error::InvalidRange);
        }
        if end
            .checked_sub(start)
            .and_then(|len| UserAddr(start).checked_end(len as usize))
            != Some(end)
            || (start..end).step_by(4096).any(|a| !space.mapped(a))
        {
            return Err(Error::NotMapped);
        }
        MEMORY.with(|slot| {
            let manager = slot.as_mut().expect("Memory not initialized");
            let mut store = Store {
                frames: &mut manager.frames,
                direct: manager.direct,
                boot: None,
                active: false,
            };
            for page in (start..end).step_by(4096) {
                if let Some(permissions) = permissions {
                    space.tables.as_mut().unwrap().protect(
                        VirtAddr(page),
                        Permissions {
                            user: true,
                            ..permissions
                        },
                        &mut store,
                    )?;
                } else {
                    space
                        .tables
                        .as_mut()
                        .unwrap()
                        .deny_user(VirtAddr(page), &mut store)?;
                }
            }
            Ok(())
        })
    }
}

pub fn free_frames() -> usize {
    MEMORY.with(|slot| {
        slot.as_ref()
            .expect("Memory not initialized")
            .frames
            .stats()
            .free
    })
}

impl OwnedSpace {
    pub fn new() -> Result<Self, Error> {
        MEMORY.with(|slot| {
            let manager = slot.as_mut().expect("Memory not initialized");
            let mut store = Store {
                frames: &mut manager.frames,
                direct: manager.direct,
                boot: None,
                active: false,
            };
            Ok(Self {
                tables: Some(manager.tables.new_user(&mut store)?),
                pages: Vec::new(),
                next_mapping: 0,
                file_pages: Vec::new(),
            })
        })
    }

    pub fn root(&self) -> PhysAddr {
        self.tables.as_ref().expect("Closed user space").root()
    }

    fn mapped(&self, address: u64) -> bool {
        MEMORY.with(|slot| {
            let manager = slot.as_mut().expect("Memory not initialized");
            self.tables
                .as_ref()
                .unwrap()
                .query(VirtAddr(address), &manager.store())
                .is_ok()
        })
    }

    /// Eager fork copy: duplicate every leaf and permission; drop rolls back partial OOM.
    pub fn clone_eager(&self) -> Result<Self, Error> {
        let mut child = Self::new()?;
        child
            .file_pages
            .try_reserve_exact(self.file_pages.len())
            .map_err(|_| Error::OutOfMemory)?;
        for (address, source) in &self.pages {
            let (permissions, accessible) = MEMORY.with(|slot| {
                let manager = slot.as_mut().expect("Memory not initialized");
                let mapping = self
                    .tables
                    .as_ref()
                    .unwrap()
                    .query(*address, &manager.store())?;
                Ok::<_, Error>((
                    Permissions {
                        writable: mapping.writable,
                        executable: mapping.executable,
                        user: true,
                        ..Permissions::READ_ONLY
                    },
                    mapping.user,
                ))
            })?;
            if source.shared {
                child.map_frame(address.0, source.clone(), permissions)?;
            } else {
                child.map(address.0, permissions)?;
                let target = child.pages.last().unwrap().1.address;
                MEMORY.with(|slot| {
                    let direct = slot.as_ref().expect("Memory not initialized").direct;
                    // SAFETY: Distinct owned live RAM frames; IF=0 excludes every user writer.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            direct_pointer(direct, source.address),
                            direct_pointer(direct, target),
                            PAGE_SIZE as usize,
                        );
                    }
                });
            }
            if !accessible {
                MEMORY.with(|slot| {
                    let manager = slot.as_mut().unwrap();
                    let mut store = Store {
                        frames: &mut manager.frames,
                        direct: manager.direct,
                        boot: None,
                        active: false,
                    };
                    child
                        .tables
                        .as_mut()
                        .unwrap()
                        .deny_user(*address, &mut store)
                })?;
            }
        }
        child.file_pages.extend(self.file_pages.iter().cloned());
        Ok(child)
    }

    /// User copy runs as a non-preemptible BSP critical section, never across a switch.
    pub fn with_active<R>(&self, action: impl FnOnce() -> R) -> R {
        assert_eq!(
            cpu::flags() & (1 << 9),
            0,
            "Address-space access requires IF=0"
        );
        let saved: u64;
        // SAFETY: Private root and supervisor upper paths are live; IF=0 prevents scheduling.
        unsafe {
            asm!("mov {},cr3", out(reg) saved, options(nomem,nostack,preserves_flags));
            asm!("mov cr3,{}", in(reg) self.root().0, options(nostack,preserves_flags));
        }
        let result = action();
        // SAFETY: The caller's root remains live; reload also invalidates lower translations.
        unsafe {
            asm!("mov cr3,{}", in(reg) saved, options(nostack,preserves_flags));
        }
        result
    }

    pub fn map(&mut self, address: u64, mut permissions: Permissions) -> Result<(), Error> {
        #[cfg(feature = "qemu-test")]
        {
            let budget = BUDGET.load(core::sync::atomic::Ordering::Relaxed);
            if budget == 0 {
                return Err(Error::OutOfMemory);
            }
            if budget != usize::MAX {
                BUDGET.store(budget - 1, core::sync::atomic::Ordering::Relaxed);
            }
        }
        permissions.user = true;
        let frame = MEMORY.with(|slot| {
            let manager = slot.as_mut().expect("Memory not initialized");
            let frame = manager.frames.allocate(Owner::User, Zone::Any)?;
            // SAFETY: This exclusive usable RAM frame has a permanent HHDM alias.
            unsafe {
                direct_pointer(manager.direct, frame).write_bytes(0, PAGE_SIZE as usize);
            }
            Ok::<_, Error>(frame)
        })?;
        static NEXT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(1);
        let identity = NEXT.fetch_update(
            core::sync::atomic::Ordering::Relaxed,
            core::sync::atomic::Ordering::Relaxed,
            |n| n.checked_add(1),
        );
        let identity = match identity {
            Ok(id) => id,
            Err(_) => {
                MEMORY.with(|slot| slot.as_mut().unwrap().frames.release(frame, Owner::User))?;
                return Err(Error::OutOfMemory);
            }
        };
        self.map_frame(
            address,
            Shared::try_new(UserFrame {
                identity,
                address: frame,
                shared: false,
            })?,
            permissions,
        )
    }
    fn map_frame(
        &mut self,
        address: u64,
        frame: Shared<UserFrame>,
        mut permissions: Permissions,
    ) -> Result<(), Error> {
        self.pages.try_reserve(1).map_err(|_| Error::OutOfMemory)?;
        permissions.user = true;
        MEMORY.with(|slot| {
            let manager = slot.as_mut().unwrap();
            let mut store = Store {
                frames: &mut manager.frames,
                direct: manager.direct,
                boot: None,
                active: false,
            };
            self.tables.as_mut().unwrap().map(
                VirtAddr(address),
                frame.address,
                permissions,
                &mut store,
            )
        })?;
        self.pages.push((VirtAddr(address), frame));
        Ok(())
    }

    /// Remove only owned private leaves on the inactive scheduler root.
    pub fn unmap(&mut self, address: u64) -> Result<(), Error> {
        let Some(index) = self.pages.iter().position(|(v, _)| v.0 == address) else {
            return Ok(());
        };
        MEMORY.with(|slot| {
            let manager = slot.as_mut().expect("Memory not initialized");
            let mut store = Store {
                frames: &mut manager.frames,
                direct: manager.direct,
                boot: None,
                active: false,
            };
            self.tables
                .as_mut()
                .expect("Closed user space")
                .unmap(VirtAddr(address), &mut store)?;
            Ok::<_, Error>(())
        })?;
        self.file_pages.retain(|p| p.address.0 != address);
        self.pages.swap_remove(index);
        Ok(())
    }
    /// Eager anonymous mapping with complete leaf rollback on partial allocation failure.
    pub fn map_anonymous(&mut self, start: u64, end: u64) -> Result<(), Error> {
        if start & 4095 != 0 || end & 4095 != 0 || start >= end || end - start > 128 * 1024 * 1024 {
            return Err(Error::InvalidRange);
        }
        for page in (start..end).step_by(4096) {
            if self.mapped(page) {
                return Err(Error::InvalidRange);
            }
        }
        for page in (start..end).step_by(4096) {
            if let Err(error) = self.map(page, Permissions::DATA) {
                for rollback in (start..page).step_by(4096) {
                    self.unmap(rollback)?;
                }
                return Err(error);
            }
        }
        Ok(())
    }

    /// Initialize owned user backing while the scheduler is stopped on the kernel root.
    pub fn initialize(&self, address: u64, bytes: &[u8]) -> Result<(), Error> {
        if UserAddr(address).checked_end(bytes.len()).is_none() {
            return Err(Error::InvalidRange);
        }
        MEMORY.with(|slot| {
            let manager = slot.as_mut().expect("Memory not initialized");
            let mut done = 0;
            while done < bytes.len() {
                let virtual_address = address + done as u64;
                let base = virtual_address & !(PAGE_SIZE - 1);
                let frame = self
                    .tables
                    .as_ref()
                    .unwrap()
                    .query(VirtAddr(base), &manager.store())?
                    .physical;
                let offset = (virtual_address - base) as usize;
                let count = (bytes.len() - done).min(PAGE_SIZE as usize - offset);
                // SAFETY: the checked lower-root range maps live user-owned RAM. The
                // scheduler is stopped on the kernel root, including for shared vfork
                // roots; the kernel-owned source bytes cannot alias these user pages.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        bytes.as_ptr().add(done),
                        direct_pointer(manager.direct, frame).add(offset),
                        count,
                    )
                };
                done += count;
            }
            Ok(())
        })
    }

    pub fn accessible(&self, address: UserAddr, length: usize, write: bool, execute: bool) -> bool {
        let Some(end) = address.checked_end(length) else {
            return false;
        };
        if length == 0 {
            return true;
        }
        MEMORY.with(|slot| {
            let manager = slot.as_mut().expect("Memory not initialized");
            let tables = self.tables.as_ref().expect("Closed user space");
            let store = manager.store();
            let mut page = address.0 & !(PAGE_SIZE - 1);
            while page < end {
                if !tables
                    .query(VirtAddr(page), &store)
                    .is_ok_and(|m| m.user && (!write || m.writable) && (!execute || m.executable))
                {
                    return false;
                }
                page += PAGE_SIZE;
            }
            true
        })
    }
}

impl Drop for OwnedSpace {
    fn drop(&mut self) {
        MEMORY.with(|slot| {
            let manager = slot.as_mut().expect("Memory not initialized");
            let active: u64;
            // SAFETY: CPL0 reads current CR3 to enforce inactive-only reclamation.
            unsafe {
                asm!("mov {}, cr3",out(reg) active,options(nomem,nostack,preserves_flags));
            }
            assert_eq!(
                active,
                manager.tables.root().0,
                "Cannot destroy an active process"
            );
            let mut store = Store {
                frames: &mut manager.frames,
                direct: manager.direct,
                boot: None,
                active: false,
            };
            self.tables
                .take()
                .expect("Closed user space")
                .destroy_user(&mut store);
        });
        // Page tables are gone before the last backing reference can return a frame to the allocator.
        self.pages.clear();
    }
}

pub fn kernel_root() -> u64 {
    MEMORY.with(|slot| {
        slot.as_ref()
            .expect("Memory not initialized")
            .tables
            .root()
            .0
    })
}

pub fn restore_kernel_stack() {
    traps::set_kernel_stack(STACK_TOP);
}
