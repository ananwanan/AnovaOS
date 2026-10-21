// SPDX-License-Identifier: Apache-2.0
//! Discover modern virtio-blk PCI functions and validate the vendor capability chain.
//! The caller supplies configuration-space I/O and owns any BAR mappings and DMA frames.
use crate::Error;

pub const VIRTIO_VENDOR: u16 = 0x1af4;
pub const MODERN_BLOCK: u16 = 0x1042;
const VENDOR_CAP: u8 = 0x09;

pub trait Configuration {
    /// Return one naturally aligned PCI configuration dword, or all ones if absent.
    fn read32(&mut self, bus: u8, device: u8, function: u8, offset: u8) -> u32;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

pub fn find_block<C: Configuration>(access: &mut C) -> Option<Location> {
    for bus in 0..=255u8 {
        for device in 0..32u8 {
            let first = access.read32(bus, device, 0, 0);
            if first == u32::MAX || first & 0xffff == 0xffff {
                continue;
            }
            let header = access.read32(bus, device, 0, 0x0c);
            let functions = if header & (1 << 23) != 0 { 8 } else { 1 };
            for function in 0..functions {
                let id = access.read32(bus, device, function, 0);
                if (id & 0xffff) as u16 == VIRTIO_VENDOR
                    && (id >> 16) as u16 == MODERN_BLOCK
                {
                    return Some(Location { bus, device, function });
                }
            }
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub bar: u8,
    pub offset: u32,
    pub length: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub common: Region,
    pub notify: Region,
    pub isr: Region,
    pub device: Region,
    pub notify_multiplier: u32,
}

/// Recognize only the standard modern PCI capability types; reject duplicate
/// or malformed configuration structures rather than trusting the firmware.
pub fn capabilities<C: Configuration>(
    access: &mut C,
    at: Location,
) -> Result<Capabilities, Error> {
    let rd = |access: &mut C, offset| {
        access.read32(at.bus, at.device, at.function, offset)
    };
    let status = rd(access, 0x04);
    if status & (1 << 20) == 0 {
        return Err(Error::Unsupported);
    }
    let mut next = (rd(access, 0x34) & 0xff) as u8;
    let mut seen = [false; 64];
    let mut regions = [None; 4];
    let mut multiplier = 0;
    let mut hops = 0;
    while next != 0 {
        if next < 0x40 || next & 3 != 0 || seen[(next / 4) as usize] || hops >= 48 {
            return Err(Error::Corrupt);
        }
        seen[(next / 4) as usize] = true;
        hops += 1;
        let header = rd(access, next);
        let cap = (header & 0xff) as u8;
        let length = ((header >> 16) & 0xff) as u8;
        let following = ((header >> 8) & 0xff) as u8;
        if cap == VENDOR_CAP {
            if length < 16 {
                return Err(Error::Corrupt);
            }
            let kind = (header >> 24) as u8;
            if (1..=4).contains(&kind) {
                if kind == 2 && length < 20 {
                    return Err(Error::Corrupt);
                }
                let bar = (rd(access, next + 4) & 0xff) as u8;
                let offset = rd(access, next + 8);
                let length = rd(access, next + 12);
                if bar >= 6 || length == 0 || offset.checked_add(length).is_none() {
                    return Err(Error::Corrupt);
                }
                let region = Region { bar, offset, length };
                let slot = &mut regions[(kind - 1) as usize];
                if slot.replace(region).is_some() {
                    return Err(Error::Corrupt);
                }
                if kind == 2 {
                    multiplier = rd(access, next + 16);
                }
            }
        }
        next = following;
    }
    Ok(Capabilities {
        common: regions[0].ok_or(Error::Unsupported)?,
        notify: regions[1].ok_or(Error::Unsupported)?,
        isr: regions[2].ok_or(Error::Unsupported)?,
        device: regions[3].ok_or(Error::Unsupported)?,
        notify_multiplier: multiplier,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Config { regs: [u32; 64] }
    impl Configuration for Config {
        fn read32(&mut self, bus: u8, dev: u8, fun: u8, offset: u8) -> u32 {
            if bus != 0 || dev != 3 || fun != 0 {
                return u32::MAX;
            }
            self.regs[(offset / 4) as usize]
        }
    }
    fn mock() -> Config {
        let mut regs = [0u32; 64];
        regs[0] = ((MODERN_BLOCK as u32) << 16) | VIRTIO_VENDOR as u32;
        regs[1] = 1 << 20;
        regs[0x34 / 4] = 0x40;
        for (i, kind) in (1..=4).enumerate() {
            let offset = 0x40 + i * 0x18;
            let next = if i == 3 { 0 } else { offset + 0x18 };
            regs[offset / 4] = 9 | ((next as u32) << 8) | (20 << 16) | ((kind as u32) << 24);
            regs[offset / 4 + 1] = 0;
            regs[offset / 4 + 2] = (i * 256) as u32;
            regs[offset / 4 + 3] = 128;
            regs[offset / 4 + 4] = 4;
        }
        Config { regs }
    }
    #[test]
    fn probe_and_parse_vendor_capabilities() {
        let mut config = mock();
        let loc = find_block(&mut config).unwrap();
        assert_eq!(loc, Location { bus: 0, device: 3, function: 0 });
        let caps = capabilities(&mut config, loc).unwrap();
        assert_eq!(caps.notify.offset, 256);
        assert_eq!(caps.notify_multiplier, 4);
    }
    #[test]
    fn reject_capability_cycle_or_duplicates() {
        let mut config = mock();
        config.regs[0x40 / 4] = 9 | (0x40 << 8) | (20 << 16) | (1 << 24);
        let loc = find_block(&mut config).unwrap();
        assert_eq!(capabilities(&mut config, loc), Err(Error::Corrupt));
    }
}
