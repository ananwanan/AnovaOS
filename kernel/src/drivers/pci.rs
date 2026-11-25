// SPDX-License-Identifier: Apache-2.0
//! Walk PCI mechanism 1 bridges and validate memory BARs for virtio and optional xHCI.
//! BAR probing disables memory decoding/bus mastering; restored BARs are mapped UC and NX.
use crate::{
    arch::x86_64::{cpu, paging::EarlyMapper},
    serial,
};
use anova_abi::*;
use anova_platform::{PhysAddr, VirtAddr};
use core::arch::asm;

const WINDOW: u64 = 0xffff_c000_0060_0000;
#[derive(Clone, Copy)]
pub struct Function {
    bus: u8,
    device: u8,
    function: u8,
}
impl Function {
    pub fn read(self, offset: u8) -> u32 {
        let address = 0x8000_0000
            | u32::from(self.bus) << 16
            | u32::from(self.device) << 11
            | u32::from(self.function) << 8
            | u32::from(offset & 0xfc);
        out32(0xcf8, address);
        cpu::in32(0xcfc)
    }
    pub fn write(self, offset: u8, value: u32) {
        let address = 0x8000_0000
            | u32::from(self.bus) << 16
            | u32::from(self.device) << 11
            | u32::from(self.function) << 8
            | u32::from(offset & 0xfc);
        out32(0xcf8, address);
        out32(0xcfc, value);
    }
    pub fn command(self, value: u16) {
        let address = 0x8000_0004
            | u32::from(self.bus) << 16
            | u32::from(self.device) << 11
            | u32::from(self.function) << 8;
        out32(0xcf8, address);
        // SAFETY: CPL0, selected command word; 16-bit write avoids clearing PCI W1C status bits.
        unsafe {
            asm!("out dx, ax",in("dx") 0xcfcu16,in("ax") value,options(nomem,nostack,preserves_flags));
        }
    }
}
fn out32(port: u16, value: u32) {
    // SAFETY: CPL0; only PCI's config-address/data ports are written, serialized by IF=0.
    unsafe {
        asm!("out dx,eax",in("dx")port,in("eax")value,options(nomem,nostack,preserves_flags));
    }
}
#[derive(Clone, Copy)]
pub struct VirtioConfig {
    pub pci: Function,
    pub common: u64,
    pub notify: u64,
    pub notify_bytes: u32,
    pub multiplier: u32,
    pub device: u64,
}
#[derive(Clone, Copy, Default)]
struct Bar {
    base: u64,
    size: u64,
}

pub fn discover(mapper: &mut EarlyMapper<'_>, network: bool) -> Result<Option<VirtioConfig>, i64> {
    discover_device(
        mapper,
        if network { 0x1041_1af4 } else { 0x1042_1af4 },
        if network { 0x40000 } else { 0 },
        8,
    )
}

/// Keep GPU capabilities in a distinct supervisor UC/NX window from block/network MMIO.
#[cfg(feature = "gpu")]
pub fn discover_gpu(mapper: &mut EarlyMapper<'_>) -> Result<Option<VirtioConfig>, i64> {
    discover_device(mapper, 0x1050_1af4, 0x80000, 16)
}

fn discover_device(
    mapper: &mut EarlyMapper<'_>,
    identity: u32,
    window_offset: u64,
    device_bytes: u32,
) -> Result<Option<VirtioConfig>, i64> {
    let mut found = None;
    let count = walk(|f, id| {
        if id == identity {
            if found.is_some() {
                return Err(UNSUPPORTED);
            }
            serial::write(format_args!(
                "[PCI:BLK] bdf={:02x}:{:02x}.{} device={identity:#x}\n",
                f.bus, f.device, f.function
            ));
            found = Some(capabilities(f, mapper, window_offset, device_bytes)?);
        }
        Ok(())
    })?;
    serial::write(format_args!(
        "[PCI:ENUM] functions={count} modern_blk={}\n",
        found.is_some()
    ));
    Ok(found)
}
fn walk(mut visit: impl FnMut(Function, u32) -> Result<(), i64>) -> Result<usize, i64> {
    let mut buses = [false; 256];
    buses[0] = true;
    let mut count = 0;
    for bus in 0..256 {
        if !buses[bus] {
            continue;
        }
        for device in 0..32 {
            let first = Function {
                bus: bus as u8,
                device,
                function: 0,
            };
            if first.read(0) & 0xffff == 0xffff {
                continue;
            }
            let functions = if first.read(0x0c) & 0x0080_0000 != 0 {
                8
            } else {
                1
            };
            for function in 0..functions {
                let f = Function { function, ..first };
                let id = f.read(0);
                if id & 0xffff == 0xffff {
                    continue;
                }
                count += 1;
                if f.read(8) >> 16 == 0x0604 {
                    let range = f.read(0x18);
                    let secondary = ((range >> 8) & 255) as usize;
                    let subordinate = ((range >> 16) & 255) as usize;
                    if secondary <= bus || secondary > subordinate {
                        return Err(INVALID_ARGUMENT);
                    }
                    buses[secondary..=subordinate].fill(true);
                }
                visit(f, id)?;
            }
        }
    }
    Ok(count)
}

/// Caller holds boot-time configuration ownership and has disabled memory decode and mastering.
fn memory_bars(f: Function) -> Result<[Bar; 6], i64> {
    let mut bars = [Bar::default(); 6];
    let mut i = 0;
    while i < 6 {
        let offset = 0x10 + i as u8 * 4;
        let low = f.read(offset);
        if low & 1 != 0 {
            i += 1;
            continue;
        }
        let wide = low & 6 == 4;
        if low & 6 != 0 && !wide || wide && i == 5 {
            return Err(UNSUPPORTED);
        }
        let high = if wide { f.read(offset + 4) } else { 0 };
        f.write(offset, u32::MAX);
        if wide {
            f.write(offset + 4, u32::MAX);
        }
        let mask_low = f.read(offset) & !15;
        let mask_high = if wide { f.read(offset + 4) } else { u32::MAX };
        f.write(offset, low);
        if wide {
            f.write(offset + 4, high);
        }
        let mask = u64::from(mask_high) << 32 | u64::from(mask_low);
        let size = (!mask).wrapping_add(1);
        let base = u64::from(high) << 32 | u64::from(low & !15);
        if base != 0 && size != 0 && size.is_power_of_two() && base.is_multiple_of(size) {
            bars[i] = Bar { base, size };
        }
        i += if wide { 2 } else { 1 };
    }
    Ok(bars)
}

fn capabilities(
    f: Function,
    mapper: &mut EarlyMapper<'_>,
    window_offset: u64,
    device_bytes: u32,
) -> Result<VirtioConfig, i64> {
    if f.read(0x0c) & 0x007f_0000 != 0 || f.read(4) & (1 << 20) == 0 {
        return Err(UNSUPPORTED);
    }
    let original = f.read(4) as u16;
    f.command(original & !6);
    let bars = match memory_bars(f) {
        Ok(bars) => bars,
        Err(error) => {
            f.command(original);
            return Err(error);
        }
    };
    // Restore decoding only after every probed BAR has been restored. INTx stays disabled.
    f.command((original | 2 | 1 << 10) & !4);
    let mut result = VirtioConfig {
        pci: f,
        common: 0,
        notify: 0,
        notify_bytes: 0,
        multiplier: 0,
        device: 0,
    };
    let mut seen = [false; 256];
    let mut cap = (f.read(0x34) & 255) as u8;
    let start = WINDOW + window_offset;
    let mut window = start;
    while cap != 0 {
        if cap < 0x40 || !cap.is_multiple_of(4) || seen[cap as usize] {
            return Err(INVALID_ARGUMENT);
        }
        seen[cap as usize] = true;
        let header = f.read(cap);
        let next = ((header >> 8) & 255) as u8;
        // Firmware may have used MSI/MSI-X. This polling driver must explicitly
        // mask both transports instead of relying only on PCI's INTx-disable bit.
        match header & 255 {
            5 => f.write(cap, header & !(1 << 16)),
            17 => f.write(cap, (header | 1 << 30) & !(1 << 31)),
            _ => {}
        }
        if header & 255 == 9 {
            let length = ((header >> 16) & 255) as usize;
            let kind = (header >> 24) as u8;
            if length < 16 || cap as usize + length > 256 {
                return Err(INVALID_ARGUMENT);
            }
            if matches!(kind, 1 | 2 | 4) {
                let bar = (f.read(cap + 4) & 255) as usize;
                let offset = u64::from(f.read(cap + 8));
                let bytes = f.read(cap + 12);
                if bar >= 6
                    || bytes == 0
                    || offset
                        .checked_add(u64::from(bytes))
                        .is_none_or(|end| end > bars[bar].size)
                    || !offset.is_multiple_of(4)
                    || bytes > 16384
                {
                    return Err(INVALID_ARGUMENT);
                }
                let physical = bars[bar].base.checked_add(offset).ok_or(INVALID_ARGUMENT)?;
                if physical == 0 || physical + u64::from(bytes) > 1 << 52 {
                    return Err(INVALID_ARGUMENT);
                }
                let page = physical & !4095;
                let span = (physical % 4096 + u64::from(bytes)).next_multiple_of(4096);
                if window + span > start + 0x40000 {
                    return Err(NO_SPACE);
                }
                for off in (0..span).step_by(4096) {
                    mapper.mmio(PhysAddr(page + off), VirtAddr(window + off));
                }
                let virtual_address = window + physical % 4096;
                window += span;
                match kind {
                    1 if result.common == 0 && bytes >= 56 => result.common = virtual_address,
                    2 if result.notify == 0 && length >= 20 => {
                        result.notify = virtual_address;
                        result.notify_bytes = bytes;
                        result.multiplier = f.read(cap + 16);
                    }
                    4 if result.device == 0 && bytes >= device_bytes => {
                        result.device = virtual_address
                    }
                    _ => return Err(INVALID_ARGUMENT),
                }
            }
        }
        cap = next;
    }
    if result.common == 0 || result.notify == 0 || result.device == 0 {
        return Err(UNSUPPORTED);
    }
    Ok(result)
}

#[cfg(feature = "xhci")]
pub(super) fn first_xhci() -> Result<Option<Function>, i64> {
    let mut found = None;
    walk(|f, id| {
        if found.is_none() && f.read(8) >> 8 == 0x0c0330 && f.read(0x0c) & 0x007f_0000 == 0 {
            serial::write(format_args!(
                "[USB:PCI] bdf={:02x}:{:02x}.{} vendor={:04x} device={:04x} class=0c0330\n",
                f.bus,
                f.device,
                f.function,
                id & 0xffff,
                id >> 16
            ));
            found = Some(f);
        }
        Ok(())
    })?;
    Ok(found)
}

#[cfg(feature = "xhci")]
impl Function {
    pub(super) fn bar0_base(self) -> Result<u64, i64> {
        let low = self.read(0x10);
        let high = match low & 7 {
            0 => 0,
            4 => self.read(0x14),
            _ => return Err(UNSUPPORTED),
        };
        Ok(u64::from(high) << 32 | u64::from(low & !15))
    }

    /// Size the aperture only after the firmware handoff and a confirmed controller halt.
    pub(super) fn halted_bar0_size(self, expected: u64) -> Result<u64, i64> {
        let command = self.read(4) as u16;
        self.command(command & !6);
        let bars = memory_bars(self);
        self.command((command | 2 | 1 << 10) & !4);
        let bar = bars?[0];
        if bar.base != expected || bar.size < 4096 {
            return Err(INVALID_ARGUMENT);
        }
        Ok(bar.size)
    }
}
