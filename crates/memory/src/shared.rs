// SPDX-License-Identifier: Apache-2.0
//! Provide fallible, single-thread reference-counted ownership for kernel resource metadata.
//! Allocation failure returns an error; this type has no weak references and is never Send or Sync.
extern crate alloc;
use crate::Error;
use alloc::alloc::{alloc, dealloc};
use core::{alloc::Layout, cell::Cell, marker::PhantomData, ops::Deref, ptr::NonNull};
struct Control<T> {
    count: Cell<usize>,
    value: T,
}
pub struct Shared<T> {
    pointer: NonNull<Control<T>>,
    single_thread: PhantomData<alloc::rc::Rc<()>>,
}
impl<T> Shared<T> {
    pub fn try_new(value: T) -> Result<Self, Error> {
        Self::allocate(value, alloc)
    }
    fn allocate(value: T, allocator: unsafe fn(Layout) -> *mut u8) -> Result<Self, Error> {
        // SAFETY: The only allocators accepted internally are the global allocator and a
        // test allocator returning null. Non-null memory has this exact layout/provenance.
        let pointer =
            NonNull::new(unsafe { allocator(Layout::new::<Control<T>>()) }.cast::<Control<T>>())
                .ok_or(Error::OutOfMemory)?;
        // SAFETY: Fresh exclusive allocation, aligned and large enough for Control<T>.
        unsafe {
            pointer.as_ptr().write(Control {
                count: Cell::new(1),
                value,
            });
        }
        Ok(Self {
            pointer,
            single_thread: PhantomData,
        })
    }
    pub fn as_ptr(this: &Self) -> *const T {
        &**this
    }
    pub fn strong_count(this: &Self) -> usize {
        // SAFETY: Each live Shared retains a reference to this initialized control block.
        unsafe { this.pointer.as_ref() }.count.get()
    }
    pub fn get_mut(this: &mut Self) -> Option<&mut T> {
        if Self::strong_count(this) != 1 {
            return None;
        }
        // SAFETY: The unique strong owner is mutably borrowed and no weak owner exists.
        Some(unsafe { &mut this.pointer.as_mut().value })
    }
}
impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        let next = Self::strong_count(self)
            .checked_add(1)
            .expect("Shared reference count overflow");
        // SAFETY: This live owner pins the control block; !Send/!Sync excludes races.
        unsafe { self.pointer.as_ref() }.count.set(next);
        Self {
            pointer: self.pointer,
            single_thread: PhantomData,
        }
    }
}
impl<T> Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: This strong owner pins initialized T for the returned borrow's lifetime.
        unsafe { &self.pointer.as_ref().value }
    }
}
impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        let count = Self::strong_count(self);
        if count > 1 {
            // SAFETY: Other owners retain this block; !Send/!Sync excludes races.
            unsafe { self.pointer.as_ref() }.count.set(count - 1);
        } else {
            // SAFETY: Last strong owner, no weak references or outstanding borrows.
            // Drop T once, then free the global allocation with its original layout.
            unsafe {
                core::ptr::drop_in_place(self.pointer.as_ptr());
                dealloc(self.pointer.as_ptr().cast(), Layout::new::<Control<T>>());
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Value<'a>(&'a Cell<u32>);
    impl Drop for Value<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    #[test]
    fn cloned_owners_drop_the_payload_once_and_mutation_requires_exclusivity() {
        let drops = Cell::new(0);
        let mut value = Shared::try_new(Value(&drops)).unwrap();
        let copy = value.clone();
        assert!(Shared::get_mut(&mut value).is_none());
        assert_eq!(Shared::as_ptr(&value), Shared::as_ptr(&copy));
        drop(copy);
        assert!(Shared::get_mut(&mut value).is_some());
        assert_eq!(drops.get(), 0);
        drop(value);
        assert_eq!(drops.get(), 1);
    }
    #[test]
    fn allocation_failure_drops_input_and_publishes_no_reference() {
        unsafe fn fail(_: Layout) -> *mut u8 {
            core::ptr::null_mut()
        }
        let drops = Cell::new(0);
        assert!(matches!(
            Shared::allocate(Value(&drops), fail),
            Err(Error::OutOfMemory)
        ));
        assert_eq!(drops.get(), 1);
    }
}
