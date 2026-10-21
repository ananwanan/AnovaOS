// SPDX-License-Identifier: Apache-2.0
//! Own private user pages and independently allocated guarded process kernel stacks.
//! BSP mutations occur on the permanent kernel root at IF=0; user roots share upper paths.
use super::*;
use alloc::vec::Vec;
use anova_abi::UserAddr;

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
            let mut store = Store {
                frames: &mut manager.frames,
                direct: manager.direct,
                boot: None,
                active: false,
            };
            Ok(Self {
                tables: Some(manager.tables.new_user(&mut store)?),
                pages: Vec::new(),
            })
        })
    }

    pub fn root(&self) -> PhysAddr {
        self.tables.as_ref().expect("Closed user space").root()
    }

    /// Eager fork copy: duplicate every leaf and permission; drop rolls back partial OOM.
    pub fn clone_eager(&self) -> Result<Self, Error> {
        let mut child = Self::new()?;
        for (address, source) in &self.pages {
            let permissions = MEMORY.with(|slot| {
                let manager = slot.as_mut().expect("Memory not initialized");
                let mapping = self
                    .tables
                    .as_ref()
                    .expect("Closed user space")
                    .query(*address, &manager.store())?;
                Ok::<_, Error>(Permissions {
                    writable: mapping.writable,
                    executable: mapping.executable,
                    user: true,
                    ..Permissions::READ_ONLY
                })
            })?;
            child.map(address.0, permissions)?;
            let target = child.pages.last().expect("Missing copied leaf").1;
            MEMORY.with(|slot| {
                let direct = slot.as_ref().expect("Memory not initialized").direct;
                // SAFETY: Both leaves are exclusively owned, distinct live RAM frames.
                // No process can run while the scheduler copies with IF=0 on this BSP.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        direct_pointer(direct, *source),
                        direct_pointer(direct, target),
                        PAGE_SIZE as usize,
                    );
                }
            });
        }
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
        });
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
