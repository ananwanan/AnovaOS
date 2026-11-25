// SPDX-License-Identifier: Apache-2.0
//! Drive bounded USB2 hub power/reset/change requests and retire descendants before parents.
//! Hub status bitmaps trigger serialized work; waits yield without blocking HID siblings or COM1.
use super::{Controller, task::next_turn};
use crate::{arch::x86_64::cpu, serial};
use anova_platform::{PM_HZ, pm_elapsed};
use anova_usb::topology::Hub;

pub(super) struct HubState {
    pub descriptor: Hub,
    pub pending: u16,
    pub seen: u16,
    pub retries: [u8; 16],
}
impl HubState {
    pub fn new(descriptor: Hub) -> Self {
        Self {
            descriptor,
            pending: 0,
            seen: 0,
            retries: [0; 16],
        }
    }
}

impl Controller {
    pub(super) async fn delay(&mut self, millis: u16) -> Result<(), &'static str> {
        let start = cpu::in32(self.regs.timer.port);
        let scheduler_start = crate::arch::x86_64::apic::monotonic_ns();
        for _ in 0..4_000_000 {
            if u64::from(pm_elapsed(
                start,
                cpu::in32(self.regs.timer.port),
                self.regs.timer.mask,
            )) >= PM_HZ * u64::from(millis) / 1000
                || crate::arch::x86_64::apic::monotonic_ns().saturating_sub(scheduler_start)
                    >= u64::from(millis) * 1_000_000
            {
                return Ok(());
            }
            // Serve unrelated reports while a hub's power or reset recovery delay elapses.
            for _ in 0..16 {
                let Some(event) = self.pop_event() else {
                    break;
                };
                self.handle_event(event);
            }
            next_turn().await;
        }
        Err("hub-timer-stalled")
    }

    pub(super) async fn power_hub(&mut self, index: usize) -> Result<(), &'static str> {
        let hub = self.devices[index]
            .as_ref()
            .and_then(|d| d.hub.as_ref())
            .ok_or("hub-record")?
            .descriptor;
        for port in 1..=hub.ports {
            self.control(index, 0x23, 3, 8, u16::from(port), &mut [])
                .await?;
        }
        self.delay(hub.power_ms.max(100)).await?;
        self.devices[index]
            .as_mut()
            .and_then(|d| d.hub.as_mut())
            .ok_or("hub-record")?
            .pending = ((1u32 << (hub.ports + 1)) - 2) as u16;
        serial::write(format_args!(
            "[USB:HUB] slot={} ports={} power_ms={} multi_tt={}\n",
            index + 1,
            hub.ports,
            hub.power_ms,
            hub.multi_tt
        ));
        Ok(())
    }

    pub(super) async fn retire_tree(&mut self, index: usize) -> Result<(), &'static str> {
        let Some(topology) = self.devices[index].as_ref().map(|d| d.topology) else {
            return Ok(());
        };
        // Fixed eight slots and five route levels avoid recursion and unbounded queues.
        for depth in (topology.depth + 1..=5).rev() {
            for child in 0..self.devices.len() {
                if self.devices[child]
                    .as_ref()
                    .is_some_and(|d| d.topology.depth == depth && d.topology.below(topology))
                {
                    self.retire(child).await?;
                }
            }
        }
        self.retire(index).await
    }

    async fn hub_status(&mut self, index: usize, port: u8) -> Result<(u16, u16), &'static str> {
        let mut bytes = [0; 4];
        if self
            .control(index, 0xa3, 0, 0, u16::from(port), &mut bytes)
            .await?
            != 4
        {
            return Err("hub-status-length");
        }
        Ok((
            u16::from_le_bytes([bytes[0], bytes[1]]),
            u16::from_le_bytes([bytes[2], bytes[3]]),
        ))
    }

    async fn hub_port(&mut self, index: usize, port: u8) -> Result<(), &'static str> {
        let d = self.devices[index].as_ref().ok_or("hub-device")?;
        let topology = d.topology;
        let slot = d.slot;
        let hub = d.hub.as_ref().ok_or("hub-record")?.descriptor;
        let (status, changes) = self.hub_status(index, port).await?;
        let record = self.devices[index]
            .as_mut()
            .and_then(|d| d.hub.as_mut())
            .ok_or("hub-record")?;
        let seen = record.seen & (1 << port) != 0;
        if status & 1 == 0 {
            record.seen &= !(1 << port);
        } else {
            record.seen |= 1 << port;
        }
        if changes & 1 != 0 {
            record.retries[usize::from(port)] = 0;
        }
        for bit in 0..5 {
            if changes & (1 << bit) != 0 {
                self.control(index, 0x23, 1, 16 + bit, u16::from(port), &mut [])
                    .await?;
            }
        }
        let child = self.devices.iter().position(|d| {
            d.as_ref()
                .is_some_and(|d| d.topology.parent == slot && d.topology.port == port)
        });
        if let Some(child) = child {
            if changes & 1 != 0 || status & 3 != 3 || status & 8 != 0 {
                self.retire_tree(child).await?;
            } else {
                return Ok(());
            }
        } else if seen && changes & 1 == 0 {
            return Ok(());
        }
        if status & 1 == 0 {
            return Ok(());
        }
        if status & (1 << 8) == 0 || status & 8 != 0 {
            return Err("hub-port-power");
        }
        self.delay(100).await?;
        if self.hub_status(index, port).await?.0 & 1 == 0 {
            return Ok(());
        }
        self.control(index, 0x23, 3, 4, u16::from(port), &mut [])
            .await?;
        let mut ready = None;
        for _ in 0..50 {
            self.delay(10).await?;
            let (status, changes) = self.hub_status(index, port).await?;
            if status & 1 == 0 {
                return Ok(());
            }
            if status & 0x13 == 3 && changes & (1 << 4) != 0 {
                self.control(index, 0x23, 1, 20, u16::from(port), &mut [])
                    .await?;
                ready = Some(status);
                break;
            }
        }
        let status = ready.ok_or("hub-port-reset-timeout")?;
        self.delay(10).await?;
        let speed = match status & (3 << 9) {
            0 => 1,
            0x200 => 2,
            0x400 => 3,
            _ => return Err("hub-port-speed"),
        };
        let child = topology.child(slot, hub, port, speed)?;
        self.enumerate_at(child).await?;
        self.bind_boot().await
    }

    pub(super) async fn service_hubs(&mut self) -> Result<(), &'static str> {
        for index in 0..self.devices.len() {
            if self.devices[index]
                .as_ref()
                .is_none_or(|d| d.retiring || d.hub.is_none())
            {
                continue;
            }
            let pending = self.devices[index]
                .as_mut()
                .and_then(|d| d.hub.as_mut())
                .ok_or("hub-record")?;
            let bits = core::mem::take(&mut pending.pending);
            let ports = pending.descriptor.ports;
            if bits & 1 != 0 {
                let mut status = [0; 4];
                if self.control(index, 0xa0, 0, 0, 0, &mut status).await? != 4 {
                    return Err("hub-local-status");
                }
                for bit in 0..2 {
                    if status[2] & (1 << bit) != 0 {
                        self.control(index, 0x20, 1, bit, 0, &mut []).await?;
                    }
                }
                if status[0] & 3 != 0 {
                    self.retire_tree(index).await?;
                    continue;
                }
            }
            for port in 1..=ports {
                if bits & (1 << port) == 0 {
                    continue;
                }
                if let Err(reason) = self.hub_port(index, port).await {
                    serial::write(format_args!(
                        "[USB:HUB_PORT_REJECT] slot={} port={port} reason={reason}\n",
                        index + 1
                    ));
                    if self.devices[index]
                        .as_ref()
                        .is_some_and(|d| d.control_error != 0 || d.retiring)
                        || reason == "control-timeout"
                    {
                        self.retire_tree(index).await?;
                        break;
                    }
                    if let Some(child) = self.devices.iter().position(|d| {
                        d.as_ref().is_some_and(|d| {
                            d.topology.parent == (index + 1) as u8 && d.topology.port == port
                        })
                    }) {
                        self.retire_tree(child).await?;
                    }
                }
            }
        }
        Ok(())
    }
}
