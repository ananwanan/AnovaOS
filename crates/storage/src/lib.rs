// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Validate disk bounds and on-disk ext2 metadata without kernel pointers or hardware access.
//! Callers own DMA, interrupts and the block device; a successful flush is an explicit operation.

pub mod block;
pub mod ext2;
pub mod pci;
pub mod virtio;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    OutOfRange,
    Overflow,
    Corrupt,
    Unsupported,
    NotFound,
    NotFile,
    NoSpace,
    TooLarge,
    Io,
}
