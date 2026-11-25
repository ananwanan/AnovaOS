// SPDX-License-Identifier: Apache-2.0
//! Own one polled xHCI controller, enumerate bounded root/hub devices and retire correlated HID DMA.
//! Keep the optional device independent of storage, PS/2, the compositor and boot success.

mod device;
mod hub;
mod lifecycle;
mod registers;
mod rings;
mod task;
mod transfer;

use super::pci;
use crate::{arch::x86_64::paging::EarlyMapper, serial};
use anova_platform::acpi::PmTimer;
use core::{
    cell::UnsafeCell,
    sync::atomic::{Ordering, fence},
};
use registers::{Registers, wait_async as wait};
use rings::Rings;

struct Owner {
    prepared: Option<Registers>,
    active: Option<task::Task>,
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
    match task::boot(Controller::start(regs)) {
        Ok(controller) => {
            let future = controller.runtime();
            match task::allocate(future) {
                Ok(task) => owner.active = Some(task),
                Err(future) => {
                    let halted = regs.halt().is_ok();
                    regs.pci.command((regs.pci.read(4) as u16) & !4);
                    if halted && regs.reset().is_ok() {
                        drop(future);
                    } else {
                        core::mem::forget(future);
                        serial::write(format_args!("[USB:QUARANTINE] task allocation failed\n"));
                    }
                    serial::write(format_args!("[USB:REJECT] phase=task reason=allocation\n"));
                }
            }
        }
        Err(reason) => serial::write(format_args!("[USB:REJECT] phase=start reason={reason}\n")),
    }
}

struct Controller {
    regs: Registers,
    rings: Rings,
    devices: [Option<device::Device>; 8],
    ports_seen: [bool; 256],
    generation: u64,
    command_failed: bool,
    root_retries: [u8; 256],
    #[cfg(feature = "usb-recovery-test")]
    recovery_test: (u8, bool),
}

impl Controller {
    async fn start(regs: Registers) -> Result<Self, &'static str> {
        regs.reset()?;
        let baseline = crate::mm::user::free_frames();
        #[cfg(feature = "qemu-test")]
        Rings::test_scratchpads()?;
        let rings = Rings::new(regs.scratchpads)?;
        let mut controller = Self {
            regs,
            rings,
            devices: core::array::from_fn(|_| None),
            ports_seen: [false; 256],
            generation: 0,
            command_failed: false,
            root_retries: [0; 256],
            #[cfg(feature = "usb-recovery-test")]
            recovery_test: (0, false),
        };
        if let Err(reason) = controller.run().await {
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

    async fn run(&mut self) -> Result<(), &'static str> {
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
        wait(regs.timer, "run-timeout", || regs.op.read(4) & 1 == 0).await?;
        self.command(23, 0).await?;
        serial::write(format_args!(
            "[USB:XHCI] noop=complete slots={} ports={} scratchpads={} irq=poll\n",
            regs.slots, regs.ports, regs.scratchpads
        ));
        #[cfg(feature = "qemu-test")]
        {
            for _ in 0..129 {
                self.command(23, 0).await?;
            }
            let slot = self.command(9, 0).await?;
            if slot == 0 || slot > regs.slots || self.command(10, slot).await? != slot {
                return Err("slot-completion");
            }
            serial::write(format_args!(
                "[USB:RINGS] noops=130 slot={slot} disable=complete\n"
            ));
        }
        let mut connected = 0;
        let mut ready = [(0u8, 0u8); 8];
        for port in 1..=regs.ports {
            if let Some(speed) = regs.reset_port(port).await? {
                if connected == ready.len() {
                    return Err("device-limit");
                }
                ready[connected] = (port, speed);
                self.ports_seen[usize::from(port)] = true;
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
            #[cfg(feature = "qemu-test")]
            self.enumerate(port, speed).await?;
            #[cfg(not(feature = "qemu-test"))]
            if let Err(reason) = self.enumerate(port, speed).await {
                {
                    serial::write(format_args!(
                        "[USB:ATTACH_REJECT] port={port} reason={reason}\n"
                    ));
                    if let Some(index) = self
                        .devices
                        .iter()
                        .position(|d| d.as_ref().is_some_and(|d| d.port == port))
                    {
                        self.retire_tree(index).await?;
                    }
                }
            }
        }
        self.bind_boot().await?;
        for _ in 0..8 {
            self.service_hubs().await?;
        }
        Ok(())
    }

    async fn command(&mut self, kind: u8, slot: u8) -> Result<u8, &'static str> {
        self.command_pointer(kind, slot, 0).await
    }

    fn pop_event(&mut self) -> Option<rings::Completion> {
        let event = self.rings.pop()?;
        fence(Ordering::SeqCst);
        self.regs.interrupt.address(24, self.rings.dequeue() | 8);
        Some(event)
    }

    async fn command_pointer(
        &mut self,
        kind: u8,
        slot: u8,
        pointer: u64,
    ) -> Result<u8, &'static str> {
        self.command_endpoint(kind, slot, pointer, 0).await
    }

    async fn command_endpoint(
        &mut self,
        kind: u8,
        slot: u8,
        pointer: u64,
        endpoint: u8,
    ) -> Result<u8, &'static str> {
        task::next_turn().await;
        if self.command_failed {
            return Err("command-ring-unavailable");
        }
        let expected = self.rings.submit(kind, slot, pointer, endpoint);
        let regs = self.regs;
        regs.doorbell.write(0, 0);
        let mut result = Err("missing-completion");
        let waited = wait(regs.timer, "command-timeout", || {
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
                32 | 34 => {
                    self.handle_event(event);
                    return false;
                }
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
        })
        .await;
        if let Err(reason) = waited {
            result = Err(reason);
        }
        self.command_failed = result.is_err();
        result
    }
}

impl Controller {
    async fn bind_boot(&mut self) -> Result<(), &'static str> {
        for device in 0..self.devices.len() {
            for interface in 0..anova_usb::MAX_INTERFACES {
                let Some(f) = self.devices[device]
                    .as_ref()
                    .filter(|d| !d.retiring)
                    .and_then(|d| d.endpoints[interface].as_ref())
                    .filter(|e| e.interface.class == 3 && !e.bound && !e.failed)
                    .map(|e| e.interface)
                else {
                    continue;
                };
                let boot = f.subclass == 1
                    && (f.protocol == 1 && f.packet >= 8 || f.protocol == 2 && f.packet >= 3);
                if boot {
                    self.control(device, 0x21, 11, 0, u16::from(f.number), &mut [])
                        .await?;
                }
                let mut layout = alloc::vec::Vec::new();
                if !cfg!(feature = "usb-boot-only") && f.report_bytes > 0 {
                    let mut bytes = [0u8; anova_usb::MAX_DESCRIPTOR];
                    let count = usize::from(f.report_bytes);
                    let result = self
                        .control(
                            device,
                            0x81,
                            6,
                            0x2200,
                            u16::from(f.number),
                            &mut bytes[..count],
                        )
                        .await;
                    let actual = match result {
                        Ok(actual) => actual,
                        Err(_)
                            if self.devices[device]
                                .as_ref()
                                .is_some_and(|d| d.control_error == 6) =>
                        {
                            self.recover_control(device).await?;
                            serial::write(format_args!(
                                "[USB:HID_FALLBACK] interface={} boot={boot} reason=report-stall\n",
                                f.number
                            ));
                            0
                        }
                        Err(reason) => return Err(reason),
                    };
                    match anova_usb::hid::Layout::parse(&bytes[..actual]) {
                        Ok(parsed)
                            if actual == count
                                && parsed.max_input_bytes() <= usize::from(f.packet)
                                && (!boot || parsed.supports_boot(f.protocol)) =>
                        {
                            // Select Report only after the whole bounded layout is usable.
                            if boot {
                                self.control(device, 0x21, 11, 1, u16::from(f.number), &mut [])
                                    .await?;
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
                    if let Some(endpoint) = self.devices[device]
                        .as_mut()
                        .and_then(|d| d.endpoints[interface].as_mut())
                    {
                        endpoint.failed = true;
                    }
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
        // Arm only newly bound endpoints; existing siblings keep their outstanding TDs.
        for d in self.devices.iter_mut().flatten() {
            for ep in d
                .endpoints
                .iter_mut()
                .flatten()
                .filter(|e| e.bound && e.pending == 0 && !e.failed)
            {
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
        self.update_available();
        Ok(())
    }

    fn handle_event(&mut self, event: rings::Completion) {
        if (event.control >> 10) & 63 == 34 {
            self.port_event(&event);
            return;
        }
        let slot = (event.control >> 24) as u8;
        let id = ((event.control >> 16) & 31) as u8;
        let Some(d) = slot
            .checked_sub(1)
            .and_then(|i| self.devices.get_mut(usize::from(i)))
            .and_then(Option::as_mut)
        else {
            return;
        };
        if (event.control >> 10) & 63 != 32 || d.retiring {
            return;
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
                ep.error = code as u8;
                lifecycle::emit(
                    usize::from(slot - 1) * anova_usb::MAX_INTERFACES + i + 1,
                    anova_usb::hid::Sample::default(),
                );
                ep.state = anova_usb::hid::State::default();
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
            if f.class == 9 {
                if let Some(hub) = d.hub.as_mut() {
                    if actual == (usize::from(hub.descriptor.ports) + 1).div_ceil(8) {
                        hub.pending |= u16::from(bytes[0]) | u16::from(bytes[1]) << 8;
                    } else {
                        ep.failed = true;
                        ep.error = 0;
                        break;
                    }
                }
                ep.pending = ep.ring.submit(
                    ep.ring.buffer(),
                    u32::from(f.packet),
                    1 << 10 | 1 << 5 | 1 << 2,
                );
                self.regs.doorbell.write(u32::from(slot) * 4, u32::from(id));
                break;
            }
            let decoded = if let Some(layout) = ep.layout.first() {
                layout.decode(&bytes[..actual])
            } else if f.protocol == 1 {
                anova_usb::hid::boot_keyboard(&bytes[..actual]).map(|s| (0, s))
            } else {
                anova_usb::hid::boot_mouse(&bytes[..actual]).map(|s| (0, s))
            };
            match decoded {
                Ok((report, sample)) => {
                    #[cfg(feature = "usb-recovery-test")]
                    if sample.keys[1] & (1 << 5) != 0 {
                        self.recovery_test.1 = true;
                    }
                    let Ok(merged) = ep.state.accept(report, sample) else {
                        ep.failed = true;
                        ep.error = 0;
                        lifecycle::emit(
                            usize::from(slot - 1) * anova_usb::MAX_INTERFACES + i + 1,
                            anova_usb::hid::Sample::default(),
                        );
                        break;
                    };
                    lifecycle::emit(
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

/// Sole BSP scheduler poll, IF=0. No IRQ or user callback can alias controller DMA.
pub fn poll(mut deliver: impl FnMut(usize, anova_usb::hid::Sample)) {
    use core::task::{Context, Waker};
    // SAFETY: Sole BSP scheduler reader/writer at IF=0; no borrow crosses this call.
    let owner = unsafe { &mut *OWNER.0.get() };
    if let Some(task) = owner.active.as_mut() {
        let mut context = Context::from_waker(Waker::noop());
        if task.as_mut().poll(&mut context).is_ready() {
            owner.active = None;
        }
    }
    lifecycle::drain(&mut deliver);
}

#[cfg(feature = "graphics")]
pub fn input_available() -> bool {
    lifecycle::available()
}

#[cfg(feature = "graphics")]
pub fn controller_available() -> bool {
    // An empty live controller must keep the input consumer alive for later attachment.
    // SAFETY: Sole BSP IF=0 reader, disjoint from the task's controller ownership.
    unsafe { &*OWNER.0.get() }.active.is_some()
}
