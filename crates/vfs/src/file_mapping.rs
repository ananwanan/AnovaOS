// SPDX-License-Identifier: Apache-2.0
//! Retain bounded file bytes independently of descriptors and version successful file mutations.
//! The single BSP serializes VFS writes and readonly shared-map refreshes; no user pointer is held.
use crate::FILE_BYTES;
use alloc::vec::Vec;
use anova_abi::NO_MEMORY;
use anova_memory::shared::Shared;
use core::cell::RefCell;

struct Contents {
    bytes: Vec<u8>,
    length: usize,
    version: u64,
}

#[derive(Clone)]
pub struct MappedFile(Shared<RefCell<Contents>>);
impl MappedFile {
    pub(crate) fn new(bytes: &[u8]) -> Result<Self, i64> {
        let mut data = Vec::new();
        data.try_reserve_exact(FILE_BYTES).map_err(|_| NO_MEMORY)?;
        data.resize(FILE_BYTES, 0);
        data[..bytes.len()].copy_from_slice(bytes);
        Shared::try_new(RefCell::new(Contents {
            bytes: data,
            length: bytes.len(),
            version: 1,
        }))
        .map(Self)
        .map_err(|_| NO_MEMORY)
    }
    pub(crate) fn active(&self) -> bool {
        Shared::strong_count(&self.0) > 1
    }
    pub fn length(&self) -> usize {
        self.0.borrow().length
    }
    pub fn version(&self) -> u64 {
        self.0.borrow().version
    }
    /// Copy a previously bounded page, zero-filled beyond the current EOF within its last page.
    pub fn copy_into(&self, offset: usize, output: &mut [u8]) -> u64 {
        let data = self.0.borrow();
        output.copy_from_slice(&data.bytes[offset..offset + output.len()]);
        data.version
    }
    pub(crate) fn write(&self, offset: usize, input: &[u8]) {
        let mut data = self.0.borrow_mut();
        data.bytes[offset..offset + input.len()].copy_from_slice(input);
        data.length = data.length.max(offset + input.len());
        data.version = data.version.wrapping_add(1);
    }
    pub(crate) fn resize(&self, length: usize) {
        let mut data = self.0.borrow_mut();
        let previous = data.length;
        data.bytes[length.min(previous)..length.max(previous)].fill(0);
        data.length = length;
        data.version = data.version.wrapping_add(1);
    }
}
