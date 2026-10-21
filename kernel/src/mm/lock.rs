// SPDX-License-Identifier: Apache-2.0
//! Non-sleeping BSP locks. Order: memory manager before heap; the fixed heap
//! never calls the frame allocator. IRQ/NMI/fatal paths must not take either.
use crate::arch::x86_64::cpu;
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};

pub struct IrqLock<T> {
    busy: AtomicBool,
    data: UnsafeCell<T>,
}

// SAFETY: Access is serialized by an acquire/release lock, with local IRQs
// disabled before acquisition. T may only move between serialized owners.
unsafe impl<T: Send> Sync for IrqLock<T> {}

impl<T> IrqLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            busy: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn with<R>(&self, action: impl FnOnce(&mut T) -> R) -> R {
        cpu::without_interrupts(|| {
            while self
                .busy
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                core::hint::spin_loop();
            }
            struct Release<'a>(&'a AtomicBool);
            impl Drop for Release<'_> {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::Release);
                }
            }
            let _release = Release(&self.busy);
            // SAFETY: The lock grants exclusive access until _release drops.
            // IRQs remain disabled and callers may not reenter this lock.
            action(unsafe { &mut *self.data.get() })
        })
    }
}
