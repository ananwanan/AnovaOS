// SPDX-License-Identifier: Apache-2.0
//! Own one polled xHCI controller, enumerate bounded root devices and retire correlated HID DMA.
//! Keep the optional device independent of storage, PS/2, the compositor and boot success.

mod device;
mod registers;
mod rings;
mod transfer;

use super::pci;
use crate::{arch::x86_64::paging::EarlyMapper, serial};
use anova_platform::acpi::PmTimer;
use core::{
    cell::UnsafeCell,
    sync::atomic::{Ordering, fence},
};
use registers::{Registers, wait};
use rings::Rings;

struct Owner {
    prepared: Option<Registers>,
    active: Option<Controller>,
}

struct BootOwner(UnsafeCell<Owner>);
// SAFETY: Only the single BSP boot/scheduler uses this owner at IF=0 on the kernel root.
// No interrupt, AP or user callback accesses or reenters it.
unsafe impl Sync for BootOwner {}
static OWNER: BootOwner = BootOwner(UnsafeCell::new(Owner {
    prepared: None,
    active: None,
}));

pub fn prepare(mapper: &mut EarlyMapper<'_>, timer: PmTimer) {
    let result = pci::first_xhci()
        .map_err(|_| "pci-enumeration")
        .and_then(|function| {
            function
                .map(|pci| Registers::discover(pci, mapper, timer))
                .transpose()
        });
    match result {
        Ok(Some(regs)) => {
            // SAFETY: Sole early boot writer. The post-heap phase consumes this value once.
            unsafe { (*OWNER.0.get()).prepared = Some(regs) };
        }
        Ok(None) => serial::write(format_args!("[USB:ABSENT] class=0c0330\n")),
        Err(reason) => serial::write(format_args!("[USB:REJECT] phase=pci reason={reason}\n")),
    }
}

pub fn start() {
    // SAFETY: Called once from kernel_main with interrupts disabled after DMA32 is available.
    let owner = unsafe { &mut *OWNER.0.get() };
    let Some(regs) = owner.prepared.take() else {
        return;
    };
    match Controller::start(regs) {
        Ok(controller) => owner.active = Some(controller),
        Err(reason) => serial::write(format_args!("[USB:REJECT] phase=start reason={reason}\n")),
    }
}

struct Controller {
    regs: Registers,
    rings: Rings,
    devices: [Option<device::Device>; 8],
}

impl Controller {
    fn start(regs: Registers) -> Result<Self, &'static str> {
        regs.reset()?;
        let baseline = crate::mm::user::free_frames();
        let rings = Rings::new(regs.scratchpads)?;
        let mut controller = Self {
            regs,
            rings,
            devices: core::array::from_fn(|_| None),
        };
        if let Err(reason) = controller.run() {
            let halted = regs.halt().is_ok();
            regs.pci.command((regs.pci.read(4) as u16) & !4);
            if !halted || regs.reset().is_err() {
                // A timeout is not a DMA completion. Never recycle possibly referenced frames.
                core::mem::forget(controller);
                serial::write(format_args!("[USB:QUARANTINE] controller DMA retained\n"));
            } else {
                drop(controller);
                serial::write(format_args!(
                    "[USB:STOP] halted=true reset=true dma_released=true frames_restored={}\n",
                    crate::mm::user::free_frames() == baseline
                ));
            }
            return Err(reason);
        }
        Ok(controller)
    }

    fn run(&mut self) -> Result<(), &'static str> {
        let regs = self.regs;
        // ERSTBA publication may fetch the segment table even while the controller is halted.
        // All owned DMA pages are initialized before enabling PCI bus mastering.
        regs.pci.command(regs.pci.read(4) as u16 | 6 | 1 << 10);
        regs.op.write(0x38, u32::from(regs.slots));
        regs.op.address(0x30, self.rings.dcbaa());
        regs.op.address(0x18, self.rings.command_base() | 1);
        regs.interrupt.write(0, 1); // Clear pending, keep IE clear in the polled profile.
        regs.interrupt.write(8, 1);
        regs.interrupt.address(24, self.rings.dequeue());
        fence(Ordering::SeqCst);
        regs.interrupt.address(16, self.rings.erst());
        regs.op.write(0, 1);
        wait(regs.timer, "run-timeout", || regs.op.read(4) & 1 == 0)?;
        self.command(23, 0)?;
        serial::write(format_args!(
            "[USB:XHCI] noop=complete slots={} ports={} scratchpads={} irq=poll\n",
            regs.slots, regs.ports, regs.scratchpads
        ));
        #[cfg(feature = "qemu-test")]
        {
            for _ in 0..129 {
                self.command(23, 0)?;
            }
            let slot = self.command(9, 0)?;
            if slot == 0 || slot > regs.slots || self.command(10, slot)? != slot {
                return Err("slot-completion");
            }
            serial::write(format_args!(
                "[USB:RINGS] noops=130 slot={slot} disable=complete\n"
            ));
        }
        let mut connected = 0;
        let mut ready = [(0u8, 0u8); 8];
        for port in 1..=regs.ports {
            if let Some(speed) = regs.reset_port(port)? {
                if connected == ready.len() {
                    return Err("device-limit");
                }
                ready[connected] = (port, speed);
                connected += 1;
                serial::write(format_args!(
                    "[USB:PORT] port={port} protocol={} speed={speed} enabled=true\n",
                    regs.protocols[usize::from(port)]
                ));
            }
            // No endpoint requests exist yet. Retire boot port changes before the next reset
            // so a controller with many ports cannot fill the bounded event ring.
            for _ in 0..16 {
                let Some(event) = self.rings.pop() else {
                    break;
                };
                fence(Ordering::SeqCst);
                regs.interrupt.address(24, self.rings.dequeue() | 8);
                let changed_port = (event.pointer >> 24) & 255;
                if (event.control >> 10) & 63 != 34
                    || event.status >> 24 != 1
                    || changed_port == 0
                    || changed_port > u64::from(regs.ports)
                {
                    return Err("unexpected-port-event");
                }
            }
        }
        serial::write(format_args!(
            "[USB:PORTS] connected={connected} reset=complete\n"
        ));
        for &(port, speed) in &ready[..connected] {
            self.enumerate(port, speed)?;
        }
        self.bind_boot()?;
        Ok(())
    }

    fn command(&mut self, kind: u8, slot: u8) -> Result<u8, &'static str> {
        self.command_pointer(kind, slot, 0)
    }

    fn pop_event(&mut self) -> Option<rings::Completion> {
        let event = self.rings.pop()?;
        fence(Ordering::SeqCst);
        self.regs.interrupt.address(24, self.rings.dequeue() | 8);
        Some(event)
    }

    fn command_pointer(&mut self, kind: u8, slot: u8, pointer: u64) -> Result<u8, &'static str> {
        let expected = self.rings.submit(kind, slot, pointer);
        let regs = self.regs;
        regs.doorbell.write(0, 0);
        let mut result = Err("missing-completion");
        wait(regs.timer, "command-timeout", || {
            if regs.op.read(4) & (1 | 1 << 2 | 1 << 12) != 0 {
                serial::write(format_args!(
                    "[USB:CONTROLLER_ERROR] status={:#x} command={kind}\n",
                    regs.op.read(4)
                ));
                result = Err("controller-fault");
                return true;
            }
            let Some(event) = self.rings.pop() else {
                return false;
            };
            fence(Ordering::SeqCst);
            regs.interrupt.address(24, self.rings.dequeue() | 8);
            match (event.control >> 10) & 63 {
                34 => return false, // A port status change may precede the command completion.
                33 if event.pointer == expected
                    && event.status >> 24 == 1
                    && (slot == 0 || (event.control >> 24) as u8 == slot) =>
                {
                    result = Ok((event.control >> 24) as u8);
                }
                _ => {
                    serial::write(format_args!(
                        "[USB:COMMAND_ERROR] command={kind} control={:#x} status={:#x}\n",
                        event.control, event.status
                    ));
                    result = Err("unexpected-completion");
                }
            }
            true
        })?;
        result
    }
}

impl Controller {
    fn bind_boot(&mut self) -> Result<(), &'static str> {
        for device in 0..self.devices.len() {
            for interface in 0..anova_usb::MAX_INTERFACES {
                let Some(f) = self.devices[device]
                    .as_ref()
                    .and_then(|d| d.endpoints[interface].as_ref())
                    .map(|e| e.interface)
                else {
                    continue;
                };
                let boot = f.subclass == 1
                    && (f.protocol == 1 && f.packet >= 8 || f.protocol == 2 && f.packet >= 3);
                if boot {
                    self.control(device, 0x21, 11, 0, u16::from(f.number), &mut [])?;
                }
                let mut layout = alloc::vec::Vec::new();
                if !cfg!(feature = "usb-boot-only") && f.report_bytes > 0 {
                    let mut bytes = [0u8; anova_usb::MAX_DESCRIPTOR];
                    let count = usize::from(f.report_bytes);
                    let actual = self.control(
                        device,
                        0x81,
                        6,
                        0x2200,
                        u16::from(f.number),
                        &mut bytes[..count],
                    )?;
                    match anova_usb::hid::Layout::parse(&bytes[..actual]) {
                        Ok(parsed)
                            if actual == count
                                && parsed.max_input_bytes() <= usize::from(f.packet)
                                && (!boot || parsed.supports_boot(f.protocol)) =>
                        {
                            // Select Report only after the whole bounded layout is usable.
                            if boot {
                                self.control(device, 0x21, 11, 1, u16::from(f.number), &mut [])?;
                            }
                            layout
                                .try_reserve_exact(1)
                                .map_err(|_| "hid-layout-allocation")?;
                            layout.push(parsed);
                        }
                        _ => serial::write(format_args!(
                            "[USB:HID_FALLBACK] interface={} boot={boot} reason=report-layout\n",
                            f.number
                        )),
                    }
                }
                if layout.is_empty() && !boot {
                    continue;
                }
                let d = self.devices[device].as_mut().ok_or("device-record")?;
                let ep = d.endpoints[interface].as_mut().ok_or("endpoint-record")?;
                let identity = d.identity;
                ep.layout = layout;
                ep.bound = true;
                serial::write(format_args!(
                    "[USB:HID] slot={} interface={} protocol={} kind={} bound=true vid={:04x} pid={:04x}\n",
                    d.slot,
                    f.number,
                    if !ep.layout.is_empty() {
                        "report"
                    } else {
                        "boot"
                    },
                    f.protocol,
                    identity.0,
                    identity.1
                ));
            }
        }
        // All synchronous controls finish before any asynchronous transfer can complete.
        for d in self.devices.iter_mut().flatten() {
            for ep in d.endpoints.iter_mut().flatten().filter(|e| e.bound) {
                ep.pending = ep.ring.submit(
                    ep.ring.buffer(),
                    u32::from(ep.interface.packet),
                    1 << 10 | 1 << 5 | 1 << 2,
                );
                self.regs.doorbell.write(
                    u32::from(d.slot) * 4,
                    u32::from((ep.interface.endpoint & 15) * 2 + 1),
                );
            }
        }
        Ok(())
    }

    fn poll(&mut self, mut deliver: impl FnMut(usize, anova_usb::hid::Sample)) {
        for _ in 0..16 {
            if self.regs.op.read(4) & (1 | 1 << 2 | 1 << 12) != 0 {
                return;
            }
            let Some(event) = self.pop_event() else {
                break;
            };
            if (event.control >> 10) & 63 == 34 {
                continue;
            }
            let slot = (event.control >> 24) as u8;
            let id = ((event.control >> 16) & 31) as u8;
            let Some(d) = slot
                .checked_sub(1)
                .and_then(|i| self.devices.get_mut(usize::from(i)))
                .and_then(Option::as_mut)
            else {
                continue;
            };
            if (event.control >> 10) & 63 != 32 {
                continue;
            }
            for (i, endpoint) in d.endpoints.iter_mut().enumerate() {
                let Some(ep) = endpoint else {
                    continue;
                };
                let f = ep.interface;
                if (f.endpoint & 15) * 2 + 1 != id || ep.failed || ep.pending == 0 {
                    continue;
                }
                let residual = (event.status & 0xffffff) as usize;
                let code = event.status >> 24;
                if event.pointer != ep.pending
                    || !matches!(code, 1 | 13)
                    || residual > usize::from(f.packet)
                {
                    // No DMA is freed and no stopped/halted endpoint is rearmed.
                    ep.failed = true;
                    serial::write(format_args!(
                        "[USB:INPUT_ERROR] slot={slot} endpoint={id} code={code} retained=true\n"
                    ));
                    break;
                }
                let actual = usize::from(f.packet) - residual;
                #[cfg(feature = "usb-input-test")]
                serial::write(format_args!(
                    "[USB:TRANSFER] slot={slot} endpoint={id} trb={:#x} code={code} bytes={actual} vid={:04x} pid={:04x}\n",
                    event.pointer, d.identity.0, d.identity.1
                ));
                let mut bytes = [0u8; anova_usb::MAX_PACKET];
                ep.ring.page.copy(1024, &mut bytes[..actual]);
                let decoded = if let Some(layout) = ep.layout.first() {
                    layout.decode(&bytes[..actual])
                } else if f.protocol == 1 {
                    anova_usb::hid::boot_keyboard(&bytes[..actual]).map(|s| (0, s))
                } else {
                    anova_usb::hid::boot_mouse(&bytes[..actual]).map(|s| (0, s))
                };
                match decoded {
                    Ok((report, sample)) => {
                        let Ok(merged) = ep.state.accept(report, sample) else {
                            ep.failed = true;
                            break;
                        };
                        deliver(
                            usize::from(slot - 1) * anova_usb::MAX_INTERFACES + i + 1,
                            merged,
                        );
                    }
                    Err(reason) => serial::write(format_args!(
                        "[USB:REPORT_REJECT] slot={slot} interface={} bytes={actual} reason={reason}\n",
                        f.number
                    )),
                }
                ep.pending = ep.ring.submit(
                    ep.ring.buffer(),
                    u32::from(f.packet),
                    1 << 10 | 1 << 5 | 1 << 2,
                );
                self.regs.doorbell.write(u32::from(slot) * 4, u32::from(id));
                break;
            }
        }
    }
}

/// Sole BSP scheduler poll, IF=0. No IRQ or user callback can alias controller DMA.
pub fn poll(deliver: impl FnMut(usize, anova_usb::hid::Sample)) {
    // SAFETY: Only boot and scheduler on the permanent root access this owner at IF=0.
    if let Some(controller) = unsafe { &mut *OWNER.0.get() }.active.as_mut() {
        controller.poll(deliver);
    }
}

#[cfg(feature = "graphics")]
pub fn input_available() -> bool {
    // SAFETY: Sole BSP IF=0 reader; the controller and bindings are fixed after boot.
    unsafe { &*OWNER.0.get() }.active.as_ref().is_some_and(|c| {
        c.devices.iter().flatten().any(|d| {
            d.endpoints
                .iter()
                .flatten()
                .any(|e| e.pending != 0 && !e.failed)
        })
    })
}
