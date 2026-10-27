// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Define owned sector devices, fallible backend ownership and checked ext2/ext4 filesystem profiles.
extern crate alloc;
pub mod ext2;
pub mod ext4;
pub mod filesystem;
use alloc::boxed::Box;
use anova_abi::{INVALID_ARGUMENT, IO_ERROR, NO_MEMORY};

/// Allocate an owning backend without invoking the allocator's infallible OOM handler.
pub fn try_box<T>(value: T) -> Result<Box<T>, i64> {
    let layout = core::alloc::Layout::new::<T>();
    if layout.size() == 0 {
        return Ok(Box::new(value));
    }
    // SAFETY: The nonzero layout is derived from T and accepted by the global allocator.
    // A null allocation is returned as NO_MEMORY before writing or publishing any ownership.
    let pointer = unsafe { alloc::alloc::alloc(layout) } as *mut T;
    let pointer = core::ptr::NonNull::new(pointer).ok_or(NO_MEMORY)?;
    // SAFETY: This uniquely owned allocation has T's size/alignment. Initialize exactly once,
    // then transfer the allocation and its destructor to Box using the matching allocator/layout.
    unsafe {
        pointer.as_ptr().write(value);
        Ok(Box::from_raw(pointer.as_ptr()))
    }
}

pub const SECTOR: usize = 512;

/// Implementations keep DMA buffers alive until completion or confirmed device reset.
/// Success means a complete sector transfer; flush must report durable-write failures.
pub trait BlockDevice {
    fn sectors(&self) -> u64;
    fn read_only(&self) -> bool {
        false
    }
    fn read_sector(&mut self, sector: u64, output: &mut [u8; SECTOR]) -> Result<(), i64>;
    fn write_sector(&mut self, sector: u64, input: &[u8; SECTOR]) -> Result<(), i64>;
    fn flush(&mut self) -> Result<(), i64>;
    /// Stop DMA before releasing the owning disk object. Memory/file devices need no extra work.
    fn stop(&mut self) -> Result<(), i64> {
        Ok(())
    }
}
impl<T: BlockDevice + ?Sized> BlockDevice for Box<T> {
    fn sectors(&self) -> u64 {
        (**self).sectors()
    }
    fn read_only(&self) -> bool {
        (**self).read_only()
    }
    fn read_sector(&mut self, sector: u64, output: &mut [u8; SECTOR]) -> Result<(), i64> {
        (**self).read_sector(sector, output)
    }
    fn write_sector(&mut self, sector: u64, input: &[u8; SECTOR]) -> Result<(), i64> {
        (**self).write_sector(sector, input)
    }
    fn flush(&mut self) -> Result<(), i64> {
        (**self).flush()
    }
    fn stop(&mut self) -> Result<(), i64> {
        (**self).stop()
    }
}

pub fn check_range(capacity: u64, sector: u64, bytes: usize) -> Result<(), i64> {
    if bytes == 0 || !bytes.is_multiple_of(SECTOR) {
        return Err(INVALID_ARGUMENT);
    }
    let count = u64::try_from(bytes / SECTOR).map_err(|_| IO_ERROR)?;
    if sector.checked_add(count).is_none_or(|end| end > capacity) {
        return Err(IO_ERROR);
    }
    Ok(())
}
