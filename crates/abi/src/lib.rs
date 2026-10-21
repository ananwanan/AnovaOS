// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Native v1 wire constants, checked user addresses and padding-free query layout.
//! These are AnovaOS interfaces; they do not implement the Linux syscall ABI.

pub const VERSION: u32 = 1;
pub const PERSONALITY: &str = "anova.native.v1";
pub const SYS_WRITE: u64 = 0;
pub const SYS_EXIT: u64 = 1;
pub const SYS_INFO: u64 = 2;
pub const BAD_ADDRESS: i64 = -1;
pub const UNSUPPORTED: i64 = -2;
pub const INVALID_ARGUMENT: i64 = -3;
pub const TOO_LONG: i64 = -4;
pub const MAX_WRITE: usize = 4096;
pub const USER_MIN: u64 = 0x1_0000;
pub const USER_LIMIT: u64 = 1 << 47;
pub const STACK_TOP: u64 = USER_LIMIT - 0x1_0000;
pub const STACK_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserAddr(pub u64);

impl UserAddr {
    /// Empty copies do not dereference an address, including null.
    pub fn checked_end(self, length: usize) -> Option<u64> {
        if length == 0 {
            return Some(self.0);
        }
        let end = self.0.checked_add(u64::try_from(length).ok()?)?;
        (self.0 >= USER_MIN && end <= USER_LIMIT).then_some(end)
    }
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct SystemInfo {
    pub abi_version: u32,
    pub size: u32,
    pub page_size: u64,
    pub ticks: u64,
    pub process_id: u64,
}

impl SystemInfo {
    /// Serialize fields explicitly, never expose Rust padding or kernel pointers.
    pub fn bytes(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        bytes[0..4].copy_from_slice(&self.abi_version.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.size.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.page_size.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.ticks.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.process_id.to_le_bytes());
        bytes
    }
}

const _: () = assert!(size_of::<SystemInfo>() == 32 && align_of::<SystemInfo>() == 8);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_range_rejects_overflow_kernel_and_noncanonical_addresses() {
        for address in [0, USER_MIN - 1, USER_LIMIT, 0xffff_8000_0000_0000, u64::MAX] {
            assert!(UserAddr(address).checked_end(1).is_none());
        }
        assert_eq!(UserAddr(USER_LIMIT - 1).checked_end(1), Some(USER_LIMIT));
        assert!(UserAddr(USER_LIMIT - 1).checked_end(2).is_none());
        assert_eq!(UserAddr(0).checked_end(0), Some(0));
    }
    #[test]
    fn info_wire_layout_has_no_unspecified_bytes() {
        let bytes = SystemInfo {
            abi_version: VERSION,
            size: 32,
            page_size: 4096,
            ticks: 8,
            process_id: 1,
        }
        .bytes();
        assert_eq!(u64::from_le_bytes(bytes[24..32].try_into().unwrap()), 1);
        assert_eq!(&bytes[..8], &[1, 0, 0, 0, 32, 0, 0, 0]);
    }
}
