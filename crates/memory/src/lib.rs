// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Allocation and page-table policies shared by the kernel and host tests.

pub mod frames;
pub mod heap;
pub mod paging;
pub mod shared;

pub const PAGE_SIZE: u64 = 4096;
pub const PHYSICAL_LIMIT: u64 = 1 << 52;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Overflow,
    Alignment,
    InvalidAddress,
    InvalidRange,
    Overlap,
    Capacity,
    OutOfMemory,
    NotAllocated,
    Reserved,
    WrongOwner,
    AlreadyMapped,
    NotMapped,
    HugePage,
    Permissions,
}

pub fn align_up(value: u64, alignment: u64) -> Result<u64, Error> {
    if !alignment.is_power_of_two() {
        return Err(Error::Alignment);
    }
    value
        .checked_add(alignment - 1)
        .map(|v| v & !(alignment - 1))
        .ok_or(Error::Overflow)
}

pub fn canonical(address: u64) -> bool {
    !((1 << 47)..0xffff_8000_0000_0000).contains(&address)
}
