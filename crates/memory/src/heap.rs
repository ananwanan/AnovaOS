// SPDX-License-Identifier: Apache-2.0
//! An intrusive buddy heap. Free blocks hold their own links; allocation and
//! coalescing never allocate metadata or call another allocator.
use crate::Error;
use core::{alloc::Layout, ptr};

const MIN_ORDER: usize = 4;
const ORDERS: usize = usize::BITS as usize;

pub struct BuddyHeap {
    base: usize,
    size: usize,
    heads: [usize; ORDERS],
    used: usize,
}

impl Default for BuddyHeap {
    fn default() -> Self {
        Self::new()
    }
}

impl BuddyHeap {
    pub const fn new() -> Self {
        Self {
            base: 0,
            size: 0,
            heads: [0; ORDERS],
            used: 0,
        }
    }

    /// # Safety
    /// `base..base+size` must be exclusively owned, mapped writable RAM for the
    /// heap's lifetime. No live objects may occupy it. Access must be serialized.
    pub unsafe fn init(&mut self, base: *mut u8, size: usize) -> Result<(), Error> {
        let address = base as usize;
        if self.size != 0
            || !size.is_power_of_two()
            || size < 1 << MIN_ORDER
            || address == 0
            || address & (size - 1) != 0
        {
            return Err(Error::Alignment);
        }
        address.checked_add(size).ok_or(Error::Overflow)?;
        self.base = address;
        self.size = size;
        self.heads[size.trailing_zeros() as usize] = address;
        // SAFETY: The caller provides exclusive writable RAM, at least 16 bytes
        // aligned. The first free block contains this aligned usize link.
        unsafe { (address as *mut usize).write(0) };
        Ok(())
    }

    fn order(&self, layout: Layout) -> Option<usize> {
        let size = layout
            .size()
            .max(layout.align())
            .max(1 << MIN_ORDER)
            .checked_next_power_of_two()?;
        (size <= self.size).then_some(size.trailing_zeros() as usize)
    }

    pub fn allocate(&mut self, layout: Layout) -> *mut u8 {
        let Some(order) = self.order(layout) else {
            return ptr::null_mut();
        };
        let Some(mut available) = (order..ORDERS).find(|index| self.heads[*index] != 0) else {
            return ptr::null_mut();
        };
        let block = self.heads[available];
        // SAFETY: Only initialized free blocks enter heads. They contain aligned
        // writable usize links and no live allocation. &mut self serializes use.
        self.heads[available] = unsafe { (block as *const usize).read() };
        while available > order {
            available -= 1;
            let buddy = block + (1 << available);
            // SAFETY: Splitting an owned free block creates two aligned disjoint
            // free blocks; each half is large enough for its intrusive link.
            unsafe { (buddy as *mut usize).write(self.heads[available]) };
            self.heads[available] = buddy;
        }
        self.used += 1 << order;
        block as *mut u8
    }

    /// # Safety
    /// `pointer` must be a live allocation from this heap with exactly `layout`.
    /// It must not be used after release; access to this heap must be serialized.
    pub unsafe fn release(&mut self, pointer: *mut u8, layout: Layout) {
        let Some(mut order) = self.order(layout) else {
            return;
        };
        let mut block = pointer as usize;
        self.used -= 1 << order;
        while (1_usize << order) < self.size {
            let buddy = self.base + ((block - self.base) ^ (1 << order));
            let mut previous = 0;
            let mut current = self.heads[order];
            while current != 0 && current != buddy {
                previous = current;
                // SAFETY: Free lists contain only aligned nodes in owned RAM.
                current = unsafe { (current as *const usize).read() };
            }
            if current == 0 {
                break;
            }
            // SAFETY: This free buddy is removed before merging. No allocation
            // aliases the node, and the caller serializes all free-list edits.
            let next = unsafe { (current as *const usize).read() };
            if previous == 0 {
                self.heads[order] = next;
            } else {
                // SAFETY: previous is a live aligned free-list node in this heap.
                unsafe { (previous as *mut usize).write(next) };
            }
            block = block.min(buddy);
            order += 1;
        }
        // SAFETY: The released (possibly merged) block is exclusively free and
        // has at least one aligned usize of storage for its next link.
        unsafe { (block as *mut usize).write(self.heads[order]) };
        self.heads[order] = block;
    }

    pub fn used(&self) -> usize {
        self.used
    }
    pub fn capacity(&self) -> usize {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::{alloc, vec::Vec};

    #[test]
    fn mixed_alignment_exhaustion_reuse_and_full_coalescing() {
        let backing = Layout::from_size_align(65536, 65536).unwrap();
        // SAFETY: The host allocation is exclusively retained until all heap
        // allocations are released. Its size/alignment satisfy buddy init.
        let memory = unsafe { alloc::alloc(backing) };
        assert!(!memory.is_null());
        let mut heap = BuddyHeap::new();
        // SAFETY: memory is a live exclusive allocation matching backing.
        unsafe { heap.init(memory, backing.size()).unwrap() };
        let mut allocations = Vec::new();
        for i in 0..80 {
            let layout = Layout::from_size_align(i * 7 + 1, 1 << (i % 9)).unwrap();
            let pointer = heap.allocate(layout);
            if pointer.is_null() {
                break;
            }
            assert_eq!(pointer as usize % layout.align(), 0);
            // SAFETY: pointer is live and has at least layout.size() bytes.
            unsafe { pointer.write_bytes(i as u8, layout.size()) };
            allocations.push((pointer, layout, i as u8));
        }
        assert!(!allocations.is_empty());
        assert!(heap.allocate(backing).is_null());
        // Release in a different order from allocation and check live contents.
        for (pointer, layout, value) in allocations.into_iter().rev() {
            // SAFETY: This allocation is still live and unaliased.
            unsafe {
                for index in 0..layout.size() {
                    assert_eq!(*pointer.add(index), value);
                }
                heap.release(pointer, layout);
            }
        }
        assert_eq!(heap.used(), 0);
        assert_eq!(heap.allocate(backing), memory);
        assert!(heap.allocate(Layout::new::<u64>()).is_null());
        // SAFETY: The full-size allocation is live; after release no heap
        // pointers are used, so the original backing can also be freed.
        unsafe {
            heap.release(memory, backing);
            alloc::dealloc(memory, backing);
        }
    }

    #[test]
    fn rejects_uninitialized_and_oversized_requests() {
        let mut heap = BuddyHeap::new();
        assert!(heap.allocate(Layout::new::<u64>()).is_null());
        assert!(
            heap.allocate(Layout::from_size_align(isize::MAX as usize, 1).unwrap())
                .is_null()
        );
    }

    #[test]
    fn fragmented_exhaustion_and_non_lifo_reuse_preserve_live_blocks() {
        let backing = Layout::from_size_align(65536, 65536).unwrap();
        let layout = Layout::from_size_align(256, 256).unwrap();
        // SAFETY: One exclusive aligned host allocation backs the heap until
        // every derived allocation has been released at the end of this test.
        let memory = unsafe { alloc::alloc(backing) };
        assert!(!memory.is_null());
        let mut heap = BuddyHeap::new();
        // SAFETY: The live host backing has the requested size/alignment.
        unsafe { heap.init(memory, backing.size()).unwrap() };
        let mut blocks = Vec::new();
        loop {
            let block = heap.allocate(layout);
            if block.is_null() {
                break;
            }
            // SAFETY: The successful allocation contains 256 writable bytes.
            unsafe { block.write_bytes(blocks.len() as u8, layout.size()) };
            blocks.push(block);
        }
        assert_eq!(blocks.len(), 256);
        assert_eq!(heap.used(), backing.size());
        for index in (0..blocks.len()).step_by(2) {
            // SAFETY: Even allocations are still live and released only once.
            unsafe { heap.release(blocks[index], layout) };
        }
        assert!(
            heap.allocate(Layout::from_size_align(512, 512).unwrap())
                .is_null()
        );
        // Fill every hole, keeping the odd allocations live as corruption probes.
        let mut holes = Vec::new();
        for _ in 0..128 {
            let block = heap.allocate(layout);
            assert!(!block.is_null());
            holes.push(block);
        }
        assert!(heap.allocate(layout).is_null());
        for index in (1..blocks.len()).step_by(2) {
            // SAFETY: Odd blocks remain live and were never freed or overwritten.
            unsafe {
                for offset in 0..256 {
                    assert_eq!(*blocks[index].add(offset), index as u8);
                }
                heap.release(blocks[index], layout);
            }
        }
        for block in holes {
            // SAFETY: Each replacement allocation remains live with this layout.
            unsafe { heap.release(block, layout) };
        }
        assert_eq!(heap.used(), 0);
        let whole = heap.allocate(backing);
        assert_eq!(whole, memory);
        // SAFETY: The full-size allocation is live; all derived pointers become
        // unused before the backing allocation is returned to the host.
        unsafe {
            heap.release(whole, backing);
            alloc::dealloc(memory, backing);
        }
    }
}
