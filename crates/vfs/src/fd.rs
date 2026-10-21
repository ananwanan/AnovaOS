// SPDX-License-Identifier: Apache-2.0
//! Store process-local descriptor indices; Vfs alone manages open-description references.
//! Tables cannot be copied or cloned without incrementing the associated shared ownership.
use anova_abi::{BAD_DESCRIPTOR, FILE_LIMIT};
pub const FD_LIMIT: usize = 32;
pub struct Descriptors {
    pub(crate) slots: [Option<usize>; FD_LIMIT],
}
impl Default for Descriptors {
    fn default() -> Self {
        Self {
            slots: [None; FD_LIMIT],
        }
    }
}
impl Descriptors {
    pub fn count(&self) -> usize {
        self.slots.iter().flatten().count()
    }
    pub fn contains(&self, fd: u64) -> bool {
        self.handle(fd).is_ok()
    }
    pub(crate) fn handle(&self, fd: u64) -> Result<usize, i64> {
        usize::try_from(fd)
            .ok()
            .and_then(|i| self.slots.get(i))
            .copied()
            .flatten()
            .ok_or(BAD_DESCRIPTOR)
    }
    pub(crate) fn free(&self) -> Result<usize, i64> {
        self.slots
            .iter()
            .position(Option::is_none)
            .ok_or(FILE_LIMIT)
    }
}
