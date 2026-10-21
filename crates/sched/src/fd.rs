// SPDX-License-Identifier: Apache-2.0
//! Own the native console descriptor slots until M6 supplies open-file objects and VFS.
//! Console sinks are stateless; fork copies slot ownership and exec preserves the table.
use anova_abi::BAD_DESCRIPTOR;

#[derive(Clone, Copy)]
pub struct Descriptors {
    open: u8,
}
impl Default for Descriptors {
    fn default() -> Self {
        Self { open: 0b110 }
    }
}
impl Descriptors {
    pub fn writable(&self, fd: u64) -> bool {
        fd < 3 && self.open & (1 << fd) != 0
    }
    pub fn close(&mut self, fd: u64) -> Result<(), i64> {
        if !self.writable(fd) {
            return Err(BAD_DESCRIPTOR);
        }
        self.open &= !(1 << fd);
        Ok(())
    }
    pub fn count(&self) -> u32 {
        self.open.count_ones()
    }
    pub fn clear(&mut self) {
        self.open = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forked_console_slots_close_independently_and_exit_clears_ownership() {
        let parent = Descriptors::default();
        let mut child = parent;
        assert_eq!(child.close(1), Ok(()));
        assert!(parent.writable(1));
        assert!(!child.writable(1));
        assert_eq!(child.close(1), Err(BAD_DESCRIPTOR));
        assert_eq!(child.close(u64::MAX), Err(BAD_DESCRIPTOR));
        child.clear();
        assert_eq!(child.count(), 0);
        assert_eq!(parent.count(), 2);
    }
}
