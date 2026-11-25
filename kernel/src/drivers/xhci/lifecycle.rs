// SPDX-License-Identifier: Apache-2.0
//! Retire USB input sources, reclaim stopped slots and enumerate root-port changes cooperatively.
//! Commands and control TDs share one serialized worker while unrelated HID traffic continues.
use super::{Controller, registers::wait_async, rings::Completion, task::next_turn};
use crate::serial;
use anova_usb::{MAX_INTERFACES, hid::Sample};
use core::cell::UnsafeCell;

const SOURCES: usize = 8 * MAX_INTERFACES;
struct Events {
    entries: [Option<(usize, Sample)>; 64],
    count: usize,
    available: bool,
}
struct Output(UnsafeCell<Events>);
// SAFETY: Only the BSP polls the USB task and then drains its output, at IF=0.
// No interrupt or callback enters this queue and no borrow survives either call.
unsafe impl Sync for Output {}
static OUTPUT: Output = Output(UnsafeCell::new(Events {
    entries: [None; 64],
    count: 0,
    available: false,
}));

#[cfg(feature = "graphics")]
pub(super) fn available() -> bool {
    // SAFETY: Only the BSP reads/writes this snapshot at IF=0.
    unsafe { &*OUTPUT.0.get() }.available
}

pub(super) fn emit(source: usize, sample: Sample) {
    // SAFETY: Sole non-reentrant BSP producer, disjoint from controller DMA.
    let output = unsafe { &mut *OUTPUT.0.get() };
    if output.count == output.entries.len() {
        // Invalidate all held sources instead of losing a key/button release.
        for i in 0..SOURCES {
            output.entries[i] = Some((i + 1, Sample::default()));
        }
        output.count = SOURCES;
        return;
    }
    output.entries[output.count] = Some((source, sample));
    output.count += 1;
}

pub(super) fn drain(deliver: &mut impl FnMut(usize, Sample)) {
    // SAFETY: The task has yielded and cannot produce while this BSP drains.
    let output = unsafe { &mut *OUTPUT.0.get() };
    for entry in &mut output.entries[..output.count] {
        if let Some((source, sample)) = entry.take() {
            deliver(source, sample);
        }
    }
    output.count = 0;
}

impl Controller {
    pub(super) async fn recover_control(&mut self, index: usize) -> Result<(), &'static str> {
        let device = self.devices[index]
            .as_ref()
            .ok_or("control-recovery-device")?;
        if device.control_error != 6 || device.output.read(self.regs.context_bytes) & 7 != 2 {
            return Err("control-not-stalled");
        }
        let slot = device.slot;
        let dequeue = device.control.dequeue();
        // EP0 accepts a new SETUP after STALL; no CLEAR_FEATURE request is sent to EP0.
        self.command_endpoint(14, slot, 0, 1).await?;
        self.command_endpoint(16, slot, dequeue, 1).await?;
        serial::write(format_args!(
            "[USB:CONTROL_RECOVER] slot={slot} reset=true dequeue=true\n"
        ));
        Ok(())
    }
    pub(super) fn update_available(&self) {
        let available = self
            .devices
            .iter()
            .flatten()
            .filter(|d| !d.retiring)
            .any(|d| {
                d.endpoints
                    .iter()
                    .flatten()
                    .any(|e| e.interface.class == 3 && e.bound && !e.failed)
            });
        // SAFETY: Sole BSP producer, no queue borrow held by the caller.
        unsafe { &mut *OUTPUT.0.get() }.available = available;
    }
    fn release(&mut self, index: usize) {
        if let Some(device) = self.devices[index].as_mut() {
            device.retiring = true;
            for (interface, endpoint) in device.endpoints.iter_mut().enumerate() {
                if let Some(endpoint) = endpoint {
                    emit(index * MAX_INTERFACES + interface + 1, Sample::default());
                    endpoint.state = anova_usb::hid::State::default();
                }
            }
        }
        self.update_available();
    }

    pub(super) fn port_event(&mut self, event: &Completion) {
        let port = ((event.pointer >> 24) & 255) as u8;
        if port == 0 || port > self.regs.ports || event.status >> 24 != 1 {
            return;
        }
        if self.regs.port_regs.read(u32::from(port - 1) * 16) & 3 != 3 {
            for index in 0..self.devices.len() {
                if self.devices[index].as_ref().is_some_and(|d| d.port == port) {
                    self.release(index);
                }
            }
        }
    }

    /// Stop every active endpoint, consume its events, then disable and unpublish the slot.
    pub(super) async fn retire(&mut self, index: usize) -> Result<(), &'static str> {
        self.release(index);
        let Some(d) = self.devices[index].as_ref() else {
            return Ok(());
        };
        let slot = d.slot;
        let generation = d.generation;
        for id in 1..=31 {
            let state = self.devices[index]
                .as_ref()
                .ok_or("retire-device")?
                .output
                .read(usize::from(id) * self.regs.context_bytes)
                & 7;
            match state {
                1 => {
                    self.command_endpoint(15, slot, 0, id).await?;
                }
                2 => {
                    self.command_endpoint(14, slot, 0, id).await?;
                }
                0 | 3 => {}
                _ => return Err("retire-endpoint-state"),
            }
        }
        // The command and all endpoint events use interrupter zero. Never reuse a DMA
        // address until the stopped-event stream has been drained before Disable Slot.
        self.drain_events().await?;
        self.command(10, slot).await?;
        self.rings.device(slot, 0);
        let pages = 3 + self.devices[index]
            .as_ref()
            .ok_or("retire-device")?
            .endpoints
            .iter()
            .flatten()
            .count();
        let before = crate::mm::user::free_frames();
        drop(self.devices[index].take());
        let restored = crate::mm::user::free_frames() == before + pages;
        serial::write(format_args!(
            "[USB:DETACH] slot={slot} generation={generation} dma_released={restored} devices={} pages={pages}\n",
            self.devices.iter().flatten().count()
        ));
        if !restored {
            return Err("retire-dma-accounting");
        }
        Ok(())
    }

    async fn drain_events(&mut self) -> Result<(), &'static str> {
        for _ in 0..64 {
            let Some(event) = self.pop_event() else {
                return Ok(());
            };
            if !matches!((event.control >> 10) & 63, 32 | 34) {
                return Err("runtime-event-type");
            }
            self.handle_event(event);
            next_turn().await;
        }
        Err("runtime-drain-limit")
    }

    async fn recover_endpoint(
        &mut self,
        index: usize,
        interface: usize,
    ) -> Result<(), &'static str> {
        let d = self.devices[index].as_ref().ok_or("recover-device")?;
        let ep = d.endpoints[interface].as_ref().ok_or("recover-endpoint")?;
        if ep.retries == 2 || !matches!(ep.error, 4 | 6) {
            return Err("endpoint-recovery-exhausted");
        }
        let slot = d.slot;
        let address = ep.interface.endpoint;
        let id = (address & 15) * 2 + 1;
        let error = ep.error;
        let state = d.output.read(usize::from(id) * self.regs.context_bytes) & 7;
        if error == 6 {
            self.control(index, 2, 1, 0, u16::from(address), &mut [])
                .await?;
        }
        match state {
            2 => {
                self.command_endpoint(14, slot, 0, id).await?;
            }
            1 => {
                self.command_endpoint(15, slot, 0, id).await?;
            }
            3 => {}
            _ => return Err("recover-endpoint-state"),
        }
        let ep = self.devices[index]
            .as_mut()
            .ok_or("recover-device")?
            .endpoints[interface]
            .as_mut()
            .ok_or("recover-endpoint")?;
        let dequeue = ep.ring.dequeue();
        self.command_endpoint(16, slot, dequeue, id).await?;
        let ep = self.devices[index]
            .as_mut()
            .ok_or("recover-device")?
            .endpoints[interface]
            .as_mut()
            .ok_or("recover-endpoint")?;
        ep.retries += 1;
        ep.failed = false;
        ep.pending = ep.ring.submit(
            ep.ring.buffer(),
            u32::from(ep.interface.packet),
            1 << 10 | 1 << 5 | 1 << 2,
        );
        self.regs.doorbell.write(u32::from(slot) * 4, u32::from(id));
        serial::write(format_args!(
            "[USB:RECOVER] slot={slot} endpoint={id} attempt={} code={error}\n",
            ep.retries
        ));
        Ok(())
    }

    async fn service(&mut self) -> Result<(), &'static str> {
        loop {
            next_turn().await;
            if self.regs.op.read(4) & (1 | 1 << 2 | 1 << 12) != 0 {
                return Err("runtime-controller-fault");
            }
            for _ in 0..16 {
                let Some(event) = self.pop_event() else {
                    break;
                };
                self.handle_event(event);
            }
            #[cfg(feature = "usb-recovery-test")]
            if core::mem::take(&mut self.recovery_test.1) {
                match self.recovery_test.0 {
                    0 => {
                        self.recovery_test.0 = 1;
                        let index = self
                            .devices
                            .iter()
                            .position(|d| {
                                d.as_ref().is_some_and(|d| {
                                    d.endpoints
                                        .iter()
                                        .flatten()
                                        .any(|e| e.bound && e.interface.protocol == 1)
                                })
                            })
                            .ok_or("test-keyboard")?;
                        let mut descriptor = [0; 18];
                        if self
                            .control(index, 0x80, 6, 0xff00, 0, &mut descriptor)
                            .await
                            .is_ok()
                        {
                            return Err("test-stall-not-observed");
                        }
                        self.recover_control(index).await?;
                        if self
                            .control(index, 0x80, 6, 0x100, 0, &mut descriptor)
                            .await?
                            != 18
                            || descriptor[1] != 1
                        {
                            return Err("test-control-retry");
                        }
                        serial::write(format_args!(
                            "[USB:RECOVERY_TEST] ep0_stall=true descriptor_retry=true\n"
                        ));
                    }
                    1 => {
                        self.recovery_test.0 = 2;
                        // Halt real hardware with its interrupt TDs still outstanding.
                        self.regs.op.write(0, self.regs.op.read(0) & !1);
                        return Err("test-controller-stop");
                    }
                    _ => {}
                }
            }
            for port in 1..=self.regs.ports {
                let offset = u32::from(port - 1) * 16;
                let status = self.regs.port_regs.read(offset);
                let connected = status & 1 != 0;
                let changed = status & (1 << 17) != 0;
                if changed {
                    self.root_retries[usize::from(port)] = 0;
                }
                let owned = self.devices.iter().position(|d| {
                    d.as_ref()
                        .is_some_and(|d| d.port == port && d.topology.parent == 0)
                });
                let retiring =
                    owned.is_some_and(|i| self.devices[i].as_ref().is_some_and(|d| d.retiring));
                if !changed && connected == self.ports_seen[usize::from(port)] && !retiring {
                    continue;
                }
                self.ports_seen[usize::from(port)] = connected;
                self.regs.port_regs.write(
                    offset,
                    (status & (1 << 9 | 3 << 14 | 7 << 25)) | (status & (0x7f << 17)),
                );
                if let Some(index) = owned {
                    self.retire_tree(index).await?;
                }
                if connected {
                    // Reset, descriptor controls and class requests all yield at waits.
                    let reset = self.regs.reset_port(port).await;
                    if let Err(reason) = reset {
                        serial::write(format_args!(
                            "[USB:ATTACH_REJECT] port={port} reason={reason}\n"
                        ));
                    }
                    if let Ok(Some(speed)) = reset {
                        match self.enumerate(port, speed).await {
                            Ok(()) => self.bind_boot().await?,
                            Err(reason) => {
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
                }
            }
            self.service_hubs().await?;
            for index in 0..self.devices.len() {
                for interface in 0..MAX_INTERFACES {
                    if self.devices[index]
                        .as_ref()
                        .and_then(|d| d.endpoints[interface].as_ref())
                        .is_some_and(|e| e.failed && e.bound)
                        && let Err(reason) = self.recover_endpoint(index, interface).await
                    {
                        serial::write(format_args!(
                            "[USB:DEVICE_DISABLED] slot={} reason={reason}\n",
                            index + 1
                        ));
                        let topology = self.devices[index]
                            .as_ref()
                            .ok_or("recover-device")?
                            .topology;
                        self.retire_tree(index).await?;
                        if topology.parent == 0 {
                            let attempts = &mut self.root_retries[usize::from(topology.root)];
                            if *attempts == 0 {
                                *attempts += 1;
                                if let Some(speed) = self.regs.reset_port(topology.root).await? {
                                    self.enumerate(topology.root, speed).await?;
                                    self.bind_boot().await?;
                                }
                            }
                        } else if let Some(hub) = self.devices[usize::from(topology.parent - 1)]
                            .as_mut()
                            .and_then(|d| d.hub.as_mut())
                        {
                            let attempts = &mut hub.retries[usize::from(topology.port)];
                            if *attempts == 0 {
                                *attempts += 1;
                                hub.seen &= !(1 << topology.port);
                                hub.pending |= 1 << topology.port;
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    pub(super) async fn runtime(mut self) {
        let mut initialization = Ok(());
        for restart in 0..=1 {
            let reason = match initialization {
                Ok(()) => self.service().await.unwrap_err(),
                Err(reason) => reason,
            };
            serial::write(format_args!("[USB:RUNTIME_STOP] reason={reason}\n"));
            for index in 0..self.devices.len() {
                self.release(index);
            }
            let regs = self.regs;
            regs.op.write(0, regs.op.read(0) & !1);
            let halted = wait_async(regs.timer, "halt-timeout", || regs.op.read(4) & 1 != 0)
                .await
                .is_ok();
            regs.pci.command((regs.pci.read(4) as u16) & !4);
            let reset = if halted {
                regs.op.write(0, 2);
                wait_async(regs.timer, "reset-timeout", || {
                    regs.op.read(0) & 2 == 0 && regs.op.read(4) & (1 << 11) == 0
                })
                .await
                .is_ok()
            } else {
                false
            };
            if !halted || !reset {
                core::mem::forget(self);
                serial::write(format_args!("[USB:QUARANTINE] runtime DMA retained\n"));
                return;
            } else {
                serial::write(format_args!(
                    "[USB:STOP] runtime halted=true reset=true dma_released=true\n"
                ));
            }
            if restart == 1 {
                return;
            }
            // Only a confirmed halt/reset permits replacing any controller-owned DMA.
            for device in &mut self.devices {
                drop(device.take());
            }
            let Ok(rings) = super::rings::Rings::new(regs.scratchpads) else {
                return;
            };
            self.rings = rings;
            self.command_failed = false;
            self.ports_seen = [false; 256];
            // Preserve lifetime generation and retry accounting across the bounded restart.
            initialization = self.run().await;
            serial::write(format_args!(
                "[USB:RESTART] attempt=1 result={initialization:?}\n"
            ));
        }
    }
}
