// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Hardware-independent address types and bounded ACPI decoding.

pub mod acpi;
pub mod power;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysAddr(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtAddr(pub u64);

pub const PM_HZ: u64 = 3_579_545;

pub fn pm_elapsed(start: u32, end: u32, mask: u32) -> u32 {
    end.wrapping_sub(start) & mask
}

#[cfg(test)]
mod tests {
    #[test]
    fn pm_counter_wraps_at_its_declared_width() {
        assert_eq!(super::pm_elapsed(0x00ff_fff0, 0x10, 0x00ff_ffff), 32);
        assert_eq!(super::pm_elapsed(0xffff_fff0, 0x10, u32::MAX), 32);
    }
}
