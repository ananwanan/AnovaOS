// SPDX-License-Identifier: Apache-2.0
//! Map bounded xHCI capability pages for firmware handoff, then validate measured BAR subregions.
//! Decode shared protocol speed maps and separate boot ownership from cooperative port resets.

use super::super::pci::Function;
use crate::arch::x86_64::{cpu, paging::EarlyMapper};
use crate::serial;
use anova_platform::{PM_HZ, PhysAddr, VirtAddr, acpi::PmTimer, pm_elapsed};
use anova_usb::xhci::{REGISTER_LIMIT, extended_capabilities, register_end};
use core::ptr::{read_volatile, write_volatile};

// The fourth 256 KiB PCI window shares the existing virtio page-table branch.
const WINDOW: u64 = 0xffff_c000_006c_0000;

#[derive(Clone, Copy)]
pub(super) struct Region {
    base: u64,
    bytes: u32,
}

impl Region {
    fn region(self, offset: u32, bytes: u32) -> Result<Self, &'static str> {
        checked_end(offset, bytes, self.bytes)?;
        Ok(Self {
            base: self.base + u64::from(offset),
            bytes,
        })
    }

    // Callers use only fixed DWORD offsets inside a subregion validated at discovery.
    pub(super) fn read(self, offset: u32) -> u32 {
        debug_assert!(register_end(offset, 4, self.bytes).is_ok());
        // SAFETY: The validated offset names an aligned UC register on the sole BSP.
        unsafe { read_volatile((self.base + u64::from(offset)) as *const u32) }
    }

    pub(super) fn write(self, offset: u32, value: u32) {
        debug_assert!(register_end(offset, 4, self.bytes).is_ok());
        // SAFETY: The owning driver writes a validated, aligned UC register at IF=0.
        unsafe { write_volatile((self.base + u64::from(offset)) as *mut u32, value) };
    }

    pub(super) fn address(self, offset: u32, value: u64) {
        self.write(offset, value as u32);
        self.write(offset + 4, (value >> 32) as u32);
    }
}

fn checked_end(offset: u32, bytes: u32, limit: u32) -> Result<u32, &'static str> {
    register_end(offset, bytes, limit).inspect_err(|_| {
        serial::write(format_args!(
            "[USB:REG_RANGE] offset={offset:#x} bytes={bytes:#x} limit={limit:#x}\n"
        ));
    })
}

/// Firmware may place xECP outside the first page. Before ownership is transferred,
/// trust only advertised register spans within our ceiling and firmware MMIO map;
/// probing BAR size while firmware uses the controller would disturb its decoding.
/// Track touched bytes and revalidate against the measured BAR before allocating DMA.
struct Bootstrap {
    physical: u64,
    mapped: u64,
    used: u32,
}

impl Bootstrap {
    fn region(
        &mut self,
        mapper: &mut EarlyMapper<'_>,
        offset: u32,
        bytes: u32,
    ) -> Result<Region, &'static str> {
        let end = checked_end(offset, bytes, REGISTER_LIMIT)?;
        for page in offset / 4096..=(end - 1) / 4096 {
            let bit = 1u64 << page;
            if self.mapped & bit != 0 {
                continue;
            }
            let delta = u64::from(page) * 4096;
            let physical = self.physical.checked_add(delta).ok_or("bar-overflow")?;
            if physical >= 1 << 52 {
                return Err("bar-overflow");
            }
            mapper
                .try_mmio(PhysAddr(physical), VirtAddr(WINDOW + delta))
                .map_err(|_| "bar-map")?;
            self.mapped |= bit;
        }
        self.used = self.used.max(end);
        Ok(Region {
            base: WINDOW + u64::from(offset),
            bytes,
        })
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

fn handoff(
    bootstrap: &mut Bootstrap,
    mapper: &mut EarlyMapper<'_>,
    first: u32,
    timer: PmTimer,
) -> Result<(), &'static str> {
    extended_capabilities(first, REGISTER_LIMIT, |offset| {
        let entry = bootstrap.region(mapper, offset, 4)?;
        let header = entry.read(0);
        if header & 255 == 1 {
            let legacy = bootstrap.region(mapper, offset, 8)?;
            serial::write(format_args!(
                "[USB:HANDOFF] legacy={offset:#x} bios={} os={}\n",
                (header >> 16) & 1,
                (header >> 24) & 1
            ));
            legacy.write(0, header | 1 << 24);
            wait(timer, "bios-handoff-timeout", || {
                legacy.read(0) & (1 << 16 | 1 << 24) == 1 << 24
            })?;
            // Disable legacy SMIs. Write zeros to W1C status fields.
            legacy.write(4, 0);
        }
        Ok(header)
    })
}

#[derive(Clone, Copy)]
pub(super) struct Registers {
    pub pci: Function,
    pub op: Region,
    pub interrupt: Region,
    pub doorbell: Region,
    pub port_regs: Region,
    pub protocols: [u8; 256],
    pub speeds: [[u8; 16]; 8],
    pub speed_groups: [u8; 256],
    pub context_bytes: usize,
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
        let mut bootstrap = Bootstrap {
            physical: base,
            mapped: 0,
            used: 0,
        };
        let cap = bootstrap.region(mapper, 0, 0x20)?;
        pci.command(pci.read(4) as u16 | 2);
        let header = cap.read(0);
        let length = header & 255;
        if !(0x0096..=0x012f).contains(&(header >> 16)) || length < 0x20 {
            return Err("controller-version");
        }
        let first = (cap.read(0x10) >> 16) * 4;
        serial::write(format_args!(
            "[USB:REGS] bar={base:#x} cap={length:#x} ecp={first:#x}\n"
        ));
        serial::write(format_args!(
            "[USB:REGS] runtime={:#x} doorbells={:#x}\n",
            cap.read(0x18) & !31,
            cap.read(0x14) & !3
        ));
        let op = bootstrap.region(mapper, length, 0x3c)?;
        handoff(&mut bootstrap, mapper, first, timer)?;
        wait(timer, "controller-not-ready", || {
            op.read(4) & (1 << 11) == 0
        })?;
        op.write(0, op.read(0) & !(1 | 1 << 2 | 1 << 3));
        wait(timer, "halt-timeout", || op.read(4) & 1 != 0)?;
        let bytes = pci
            .halted_bar0_size(base)
            .map_err(|_| "bar-size")?
            .min(u64::from(REGISTER_LIMIT)) as u32;
        serial::write(format_args!(
            "[USB:APERTURE] bytes={bytes:#x} bootstrap_end={:#x}\n",
            bootstrap.used
        ));
        checked_end(0, bootstrap.used, bytes).map_err(|_| "capability-bar-range")?;
        if base
            .checked_add(u64::from(bytes))
            .is_none_or(|end| end > 1 << 52)
        {
            return Err("bar-overflow");
        }
        let bar = bootstrap.region(mapper, 0, bytes)?;
        let params = bar.read(4);
        let slots = (params as u8).min(8);
        let ports = (params >> 24) as u8;
        if slots == 0 || ports == 0 || (params >> 8) & 0x7ff == 0 || op.read(8) & 1 == 0 {
            return Err("controller-capabilities");
        }
        let params2 = bar.read(8);
        let scratchpads = (((params2 >> 21) & 31) << 5 | (params2 >> 27)) as usize;
        serial::write(format_args!(
            "[USB:SCRATCH] requested={scratchpads} limit=512 hcsparams2={params2:#x}\n"
        ));
        if scratchpads > 512 {
            return Err("scratchpad-limit");
        }
        let rt = bar.read(0x18) & !31;
        let db = bar.read(0x14) & !3;
        let port_bytes = u32::from(ports) * 16;
        let port_regs = bar.region(length + 0x400, port_bytes)?;
        let mut protocols = [0; 256];
        let mut speeds = [[0; 16]; 8];
        let mut speed_groups = [0; 256];
        let mut groups = 0;
        extended_capabilities(first, bytes, |offset| {
            let entry = bar.region(offset, 4)?;
            let header = entry.read(0);
            if header & 255 == 2 {
                let protocol = bar.region(offset, 16)?;
                let major = (header >> 24) as u8;
                let range = protocol.read(8);
                let first = (range & 255) as usize;
                let count = ((range >> 8) & 255) as usize;
                if protocol.read(4) != 0x2042_5355
                    || !matches!(major, 2 | 3)
                    || first == 0
                    || count == 0
                    || first + count > usize::from(ports) + 1
                {
                    return Err("port-protocol-range");
                }
                let count_psi = (range >> 28) as usize;
                let next = (header >> 8) & 255;
                if next != 0 && next * 4 < 16 + count_psi as u32 * 4 {
                    return Err("protocol-speed-overlap");
                }
                let table = bar.region(offset, 16 + count_psi as u32 * 4)?;
                if protocol.read(12) & 31 != 0 {
                    return Err("protocol-slot-type");
                }
                let mut entries = [0; 15];
                for (index, entry) in entries[..count_psi].iter_mut().enumerate() {
                    *entry = table.read(16 + index as u32 * 4);
                }
                let map = anova_usb::xhci::speed_map(major, &entries[..count_psi])?;
                serial::write(format_args!(
                    "[USB:PROTOCOL] major={major} first={first} ports={count} psi={count_psi}\n"
                ));
                if groups == speeds.len() {
                    return Err("protocol-group-limit");
                }
                speeds[groups] = map;
                for group in &mut speed_groups[first..first + count] {
                    *group = groups as u8;
                }
                groups += 1;
                for owner in &mut protocols[first..first + count] {
                    if *owner != 0 {
                        return Err("port-protocol-overlap");
                    }
                    *owner = major;
                }
            }
            Ok(header)
        })?;
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
            port_regs,
            protocols,
            speeds,
            speed_groups,
            context_bytes: if bar.read(0x10) & 4 != 0 { 64 } else { 32 },
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

    /// Yield between port reset/recovery observations so other tasks remain runnable.
    pub async fn reset_port(self, port: u8) -> Result<Option<u8>, &'static str> {
        if port == 0 || port > self.ports {
            return Err("root-port-range");
        }
        let offset = u32::from(port - 1) * 16;
        let read = || self.port_regs.read(offset);
        let status = read();
        if status & 1 == 0 {
            return Ok(None);
        }
        let protocol = self.protocols[usize::from(port)];
        if !matches!(protocol, 2 | 3) {
            return Err("root-port-protocol");
        }
        // Preserve only RW power/indicator/wake fields. Never echo PED (RW1CS),
        // change bits (RW1C), reset or link-state strobes from a PORTSC read.
        const RW: u32 = 1 << 9 | 3 << 14 | 7 << 25;
        const CHANGES: u32 = 0x7f << 17;
        self.port_regs.write(offset, (status & RW) | 1 << 9);
        let start = cpu::in32(self.timer.port);
        wait_async(self.timer, "port-power-timeout", || {
            u64::from(pm_elapsed(
                start,
                cpu::in32(self.timer.port),
                self.timer.mask,
            )) >= PM_HZ / 10
        })
        .await?;
        if read() & 1 == 0 {
            return Err("root-port-disconnected");
        }
        if protocol == 2 || read() & 2 == 0 {
            let reset = if protocol == 2 { 1 << 4 } else { 1 << 31 };
            self.port_regs.write(offset, (read() & RW) | reset);
            wait_async(self.timer, "port-reset-timeout", || {
                let current = read();
                current & 1 == 0 || current & (1 << 4 | 1 << 31) == 0 && current & 2 != 0
            })
            .await?;
        }
        let status = read();
        let id = ((status >> 10) & 15) as usize;
        let speed = self.speeds[usize::from(self.speed_groups[usize::from(port)])][id];
        if status & 3 != 3 || status & (15 << 5) != 0 {
            return Err("root-port-not-enabled");
        }
        if !(protocol == 2 && (1..=3).contains(&speed) || protocol == 3 && speed == 4) {
            serial::write(format_args!(
                "[USB:PORT_SKIP] port={port} speed_id={id} reason=unsupported-speed\n"
            ));
            return Ok(None);
        }
        self.port_regs
            .write(offset, (status & RW) | (status & CHANGES));
        // USB 2.0 7.1.7.5 requires at least 10 ms reset recovery before device requests.
        let start = cpu::in32(self.timer.port);
        wait_async(self.timer, "port-recovery-timeout", || {
            u64::from(pm_elapsed(
                start,
                cpu::in32(self.timer.port),
                self.timer.mask,
            )) >= PM_HZ / 100
        })
        .await?;
        if read() & 3 != 3 {
            return Err("root-port-disconnected");
        }
        Ok(Some(speed))
    }
}

/// One bounded observation per scheduler turn; the PM timer bounds lost completions.
pub(super) async fn wait_async(
    timer: PmTimer,
    reason: &'static str,
    mut ready: impl FnMut() -> bool,
) -> Result<(), &'static str> {
    let start = cpu::in32(timer.port);
    let scheduler_start = crate::arch::x86_64::apic::monotonic_ns();
    for _ in 0..4_000_000 {
        if ready() {
            return Ok(());
        }
        if u64::from(pm_elapsed(start, cpu::in32(timer.port), timer.mask)) >= PM_HZ
            || crate::arch::x86_64::apic::monotonic_ns().saturating_sub(scheduler_start)
                >= 1_000_000_000
        {
            break;
        }
        super::task::next_turn().await;
    }
    Err(reason)
}
