// SPDX-License-Identifier: Apache-2.0
//! Own a single application's private lower-half pages and guarded kernel stack.
//! Upper kernel tables are borrowed only while the permanent mappings stay stable.
use super::*;
use alloc::vec::Vec;
use anova_abi::UserAddr;

const GUARD: u64 = MANAGED_BASE + 0x100_0000;
const BOTTOM: u64 = GUARD + PAGE_SIZE;
pub const KERNEL_TOP: u64 = BOTTOM + STACK_BYTES;
#[cfg(feature = "qemu-test")]
static BUDGET: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(usize::MAX);
#[cfg(feature = "qemu-test")]
pub fn set_test_budget(value: usize) {
    BUDGET.store(value, core::sync::atomic::Ordering::Relaxed);
}

pub struct UserSpace {
    tables: Option<PageTables>,
    pages: Vec<(VirtAddr, PhysAddr)>,
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

impl UserSpace {
    pub fn new() -> Result<Self, Error> {
        MEMORY.with(|slot| {
            let manager = slot.as_mut().expect("Memory not initialized");
            manager.map_range(VirtAddr(BOTTOM), STACK_BYTES, Owner::ProcessStack)?;
            let mut store = Store {
                frames: &mut manager.frames,
                direct: manager.direct,
                boot: None,
                active: false,
            };
            match manager.tables.new_user(&mut store) {
                Ok(tables) => Ok(Self {
                    tables: Some(tables),
                    pages: Vec::new(),
                }),
                Err(error) => {
                    for address in (BOTTOM..KERNEL_TOP).step_by(PAGE_SIZE as usize) {
                        manager.unmap_page(VirtAddr(address), Owner::ProcessStack)?;
                    }
                    Err(error)
                }
            }
        })
    }

    pub fn root(&self) -> PhysAddr {
        self.tables.as_ref().expect("Closed user space").root()
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
        self.pages.try_reserve(1).map_err(|_| Error::OutOfMemory)?;
        permissions.user = true;
        let frame = MEMORY.with(|slot| {
            let manager = slot.as_mut().expect("Memory not initialized");
            let frame = manager.frames.allocate(Owner::User, Zone::Any)?;
            // SAFETY: An exclusively owned usable frame; its permanent HHDM alias is valid.
            unsafe { direct_pointer(manager.direct, frame).write_bytes(0, PAGE_SIZE as usize) };
            let mut store = Store {
                frames: &mut manager.frames,
                direct: manager.direct,
                boot: None,
                active: false,
            };
            if let Err(error) = self.tables.as_mut().expect("Closed user space").map(
                VirtAddr(address),
                frame,
                permissions,
                &mut store,
            ) {
                manager.frames.release(frame, Owner::User)?;
                return Err(error);
            }
            Ok(frame)
        })?;
        self.pages.push((VirtAddr(address), frame));
        Ok(())
    }

    /// Loader-only initialization through owned physical backing, before activation.
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
                    .pages
                    .iter()
                    .find(|(v, _)| v.0 == base)
                    .ok_or(Error::NotMapped)?
                    .1;
                let offset = (virtual_address - base) as usize;
                let count = (bytes.len() - done).min(PAGE_SIZE as usize - offset);
                // SAFETY: pages lists exclusively owned, live RAM; initialization runs
                // before Ring 3, bytes cannot alias these private physical pages.
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

impl Drop for UserSpace {
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
            // CR3 was restored without PCID/PGE before reaching Drop. No CPU can
            // retain these lower translations, and borrowed upper paths survive.
            for (_, frame) in self.pages.drain(..) {
                manager
                    .frames
                    .release(frame, Owner::User)
                    .expect("Invalid user page release");
            }
            for address in (BOTTOM..KERNEL_TOP).step_by(PAGE_SIZE as usize) {
                manager
                    .unmap_page(VirtAddr(address), Owner::ProcessStack)
                    .expect("Invalid process stack release");
            }
        });
    }
}

pub fn restore_kernel_stack() {
    traps::set_kernel_stack(STACK_TOP);
}
