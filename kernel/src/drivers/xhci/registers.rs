// SPDX-License-Identifier: Apache-2.0
//! Validate xHCI BAR subregions, transfer firmware ownership and bound hardware waits.

use super::super::pci::Function;
use crate::arch::x86_64::{cpu, paging::EarlyMapper};
use anova_platform::{PM_HZ, PhysAddr, VirtAddr, acpi::PmTimer, pm_elapsed};
use core::ptr::{read_volatile, write_volatile};

// The fourth 256 KiB PCI window shares the existing virtio page-table branch.
const WINDOW: u64 = 0xffff_c000_006c_0000;
const LIMIT: u64 = 256 * 1024;

#[derive(Clone, Copy)]
pub(super) struct Region {
    base: u64,
    bytes: u64,
}

impl Region {
    fn region(self, offset: u32, bytes: u32) -> Result<Self, &'static str> {
        if offset & 3 != 0 || bytes < 4 || u64::from(offset) + u64::from(bytes) > self.bytes {
            return Err("register-range");
        }
        Ok(Self {
            base: self.base + u64::from(offset),
            bytes: u64::from(bytes),
        })
    }

    // Callers use only fixed DWORD offsets inside a subregion validated at discovery.
    pub(super) fn read(self, offset: u32) -> u32 {
        debug_assert!(offset & 3 == 0 && u64::from(offset) + 4 <= self.bytes);
        // SAFETY: The validated offset names an aligned UC register on the sole BSP.
        unsafe { read_volatile((self.base + u64::from(offset)) as *const u32) }
    }

    pub(super) fn write(self, offset: u32, value: u32) {
        debug_assert!(offset & 3 == 0 && u64::from(offset) + 4 <= self.bytes);
        // SAFETY: The owning driver writes a validated, aligned UC register at IF=0.
        unsafe { write_volatile((self.base + u64::from(offset)) as *mut u32, value) };
    }

    pub(super) fn address(self, offset: u32, value: u64) {
        self.write(offset, value as u32);
        self.write(offset + 4, (value >> 32) as u32);
    }
}

pub(super) fn wait(
    timer: PmTimer,
    reason: &'static str,
    mut ready: impl FnMut() -> bool,
) -> Result<(), &'static str> {
    let start = cpu::in32(timer.port);
    for _ in 0..4_000_000 {
        if ready() {
            return Ok(());
        }
        if u64::from(pm_elapsed(start, cpu::in32(timer.port), timer.mask)) >= PM_HZ {
            break;
        }
        core::hint::spin_loop();
    }
    Err(reason)
}

fn handoff(cap: Region, timer: PmTimer) -> Result<(), &'static str> {
    let mut offset = (cap.read(0x10) >> 16) * 4;
    while offset != 0 {
        // Claim the controller before probing BAR size. Reject capability chains
        // outside the bootstrap page rather than accessing an unmeasured aperture.
        let entry = cap.region(offset, 4)?;
        let header = entry.read(0);
        if header & 255 == 1 {
            let legacy = cap.region(offset, 8)?;
            legacy.write(0, header | 1 << 24);
            wait(timer, "bios-handoff-timeout", || {
                legacy.read(0) & (1 << 16 | 1 << 24) == 1 << 24
            })?;
            // Disable legacy SMIs. Write zeros to W1C status fields.
            legacy.write(4, 0);
        }
        let next = (header >> 8) & 255;
        if next == 0 {
            break;
        }
        offset += next * 4;
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(super) struct Registers {
    pub pci: Function,
    pub op: Region,
    pub interrupt: Region,
    pub doorbell: Region,
    pub slots: u8,
    pub ports: u8,
    pub scratchpads: usize,
    pub timer: PmTimer,
}

impl Registers {
    pub fn discover(
        pci: Function,
        mapper: &mut EarlyMapper<'_>,
        timer: PmTimer,
    ) -> Result<Self, &'static str> {
        let base = pci.bar0_base().map_err(|_| "bar-type")?;
        if base == 0 || base & 4095 != 0 || base >= 1 << 52 {
            return Err("bar-address");
        }
        mapper
            .try_mmio(PhysAddr(base), VirtAddr(WINDOW))
            .map_err(|_| "bar-map")?;
        pci.command(pci.read(4) as u16 | 2);
        let cap = Region {
            base: WINDOW,
            bytes: 4096,
        };
        let header = cap.read(0);
        let length = header & 255;
        if !(0x0096..=0x012f).contains(&(header >> 16)) || length < 0x20 {
            return Err("controller-version");
        }
        let op = cap.region(length, 0x3c)?;
        handoff(cap, timer)?;
        wait(timer, "controller-not-ready", || {
            op.read(4) & (1 << 11) == 0
        })?;
        op.write(0, op.read(0) & !(1 | 1 << 2 | 1 << 3));
        wait(timer, "halt-timeout", || op.read(4) & 1 != 0)?;
        let bytes = pci
            .halted_bar0_size(base)
            .map_err(|_| "bar-size")?
            .min(LIMIT);
        if base.checked_add(bytes).is_none_or(|end| end > 1 << 52) {
            return Err("bar-overflow");
        }
        for offset in (4096..bytes).step_by(4096) {
            mapper
                .try_mmio(PhysAddr(base + offset), VirtAddr(WINDOW + offset))
                .map_err(|_| "bar-map")?;
        }
        let bar = Region {
            base: WINDOW,
            bytes,
        };
        let params = bar.read(4);
        let slots = (params as u8).min(8);
        let ports = (params >> 24) as u8;
        if slots == 0 || ports == 0 || (params >> 8) & 0x7ff == 0 || op.read(8) & 1 == 0 {
            return Err("controller-capabilities");
        }
        let params2 = bar.read(8);
        let scratchpads = (((params2 >> 21) & 31) << 5 | (params2 >> 27)) as usize;
        if scratchpads > 32 {
            return Err("scratchpad-limit");
        }
        let rt = bar.read(0x18) & !31;
        let db = bar.read(0x14) & !3;
        let port_bytes = u32::from(ports) * 16;
        bar.region(length + 0x400, port_bytes)?;
        let op_end = length + 0x400 + port_bytes;
        let doorbell_bytes = (u32::from(slots) + 1) * 4;
        let interrupt = bar.region(rt.checked_add(32).ok_or("runtime-offset")?, 32)?;
        let doorbell = bar.region(db, doorbell_bytes)?;
        if rt < op_end || db < op_end || (rt < db + doorbell_bytes && db < rt + 64) {
            return Err("register-overlap");
        }
        Ok(Self {
            pci,
            op,
            interrupt,
            doorbell,
            slots,
            ports,
            scratchpads,
            timer,
        })
    }

    pub fn reset(self) -> Result<(), &'static str> {
        self.op.write(0, 2);
        wait(self.timer, "reset-timeout", || self.op.read(0) & 2 == 0)?;
        wait(self.timer, "controller-not-ready", || {
            self.op.read(4) & (1 << 11) == 0
        })
    }

    pub fn halt(self) -> Result<(), &'static str> {
        self.op.write(0, self.op.read(0) & !1);
        wait(self.timer, "halt-timeout", || self.op.read(4) & 1 != 0)
    }
}
