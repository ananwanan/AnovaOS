// SPDX-License-Identifier: Apache-2.0
//! Enumerate root/USB2 hub devices with mapped speed IDs and bounded cooperative EP0 requests.
//! Device DMA remains owned until stopped-slot retirement or confirmed controller halt/reset.
use super::{
    Controller,
    registers::wait_async as wait,
    transfer::{Page, TransferRing},
};
use crate::{arch::x86_64::cpu, serial};
use anova_platform::{PM_HZ, pm_elapsed};
use anova_usb::{Configuration, MAX_DESCRIPTOR, MAX_INTERFACES};

pub(super) struct Endpoint {
    pub interface: anova_usb::Interface,
    pub ring: TransferRing,
    pub pending: u64,
    pub failed: bool,
    pub error: u8,
    pub retries: u8,
    pub bound: bool,
    // A zero/one-element Vec provides fallible stable-Rust allocation for the layout.
    pub layout: alloc::vec::Vec<anova_usb::hid::Layout>,
    pub state: anova_usb::hid::State,
}
pub(super) struct Device {
    pub slot: u8,
    pub port: u8,
    pub topology: anova_usb::topology::Topology,
    pub hub: Option<super::hub::HubState>,
    pub generation: u64,
    pub retiring: bool,
    pub control_error: u8,
    pub identity: (u16, u16, u8),
    pub input: Page,
    pub output: Page,
    pub control: TransferRing,
    pub config: Option<Configuration>,
    pub endpoints: [Option<Endpoint>; MAX_INTERFACES],
}
impl Device {
    fn new(
        slot: u8,
        topology: anova_usb::topology::Topology,
        generation: u64,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            slot,
            port: topology.root,
            topology,
            hub: None,
            generation,
            retiring: false,
            control_error: 0,
            identity: (0, 0, 0),
            input: Page::new()?,
            output: Page::new()?,
            control: TransferRing::new()?,
            config: None,
            endpoints: core::array::from_fn(|_| None),
        })
    }
}
impl Controller {
    pub(super) async fn enumerate(&mut self, port: u8, speed: u8) -> Result<(), &'static str> {
        self.enumerate_at(anova_usb::topology::Topology::root(port, speed)?)
            .await
    }

    pub(super) async fn enumerate_at(
        &mut self,
        topology: anova_usb::topology::Topology,
    ) -> Result<(), &'static str> {
        if self.devices.iter().flatten().count() >= usize::from(self.regs.slots) {
            return Err("device-limit");
        }
        let port = topology.root;
        let speed = topology.speed;
        let map = self.regs.speeds[usize::from(self.regs.speed_groups[usize::from(port)])];
        let speed_id = if topology.depth == 0 {
            let id = ((self.regs.port_regs.read(u32::from(port - 1) * 16) >> 10) & 15) as usize;
            if map[id] != speed {
                return Err("device-speed-changed");
            }
            id as u8
        } else {
            map.iter()
                .position(|value| *value == speed)
                .ok_or("device-speed-id")? as u8
        };
        let slot = self.command(9, 0).await?;
        if slot == 0 || slot > self.regs.slots {
            return Err("device-slot");
        }
        let index = usize::from(slot - 1);
        // Store ownership before publishing any pointer: an early error must not drop active DMA.
        if self.devices[index].is_some() {
            return Err("slot-still-owned");
        }
        self.generation = self.generation.checked_add(1).ok_or("device-generation")?;
        match Device::new(slot, topology, self.generation) {
            Ok(device) => self.devices[index] = Some(device),
            Err(error) => {
                self.command(10, slot).await?;
                return Err(error);
            }
        }
        let size = self.regs.context_bytes;
        let d = self.devices[index].as_ref().ok_or("device-record")?;
        self.rings.device(slot, d.output.physical());
        d.input.write(4, 3);
        d.input.write(
            size,
            topology.route
                | u32::from(speed_id) << 20
                | u32::from(topology.multi_tt) << 25
                | 1 << 27,
        );
        d.input.write(size + 4, u32::from(port) << 16);
        d.input.write(
            size + 8,
            u32::from(topology.tt_slot) | u32::from(topology.tt_port) << 8,
        );
        let initial = match speed {
            1 | 2 => 8,
            4 => 512,
            _ => 64,
        };
        d.input.write(size * 2 + 4, 3 << 1 | 4 << 3 | initial << 16);
        d.input.address(size * 2 + 8, d.control.base() | 1);
        d.input.write(size * 2 + 16, 8);
        self.command_pointer(11, slot, d.input.physical()).await?;
        // USB2 address recovery: do not send a separate USB SET_ADDRESS request.
        let start = cpu::in32(self.regs.timer.port);
        wait(self.regs.timer, "address-recovery", || {
            u64::from(pm_elapsed(
                start,
                cpu::in32(self.regs.timer.port),
                self.regs.timer.mask,
            )) >= PM_HZ / 500
        })
        .await?;
        let d = self.devices[index].as_ref().ok_or("device-record")?;
        let state = d.output.read(12);
        if (state >> 27) & 31 != 2
            || state & 255 == 0
            || state & 255 > 127
            || d.output.read(size) & 7 != 1
        {
            return Err("addressed-context");
        }
        let mut bytes = [0u8; MAX_DESCRIPTOR];
        #[cfg(feature = "usb-test-control-stall")]
        self.control(index, 0x80, 6, 0xff00, 0, &mut bytes[..8])
            .await?;
        if self
            .control(index, 0x80, 6, 0x100, 0, &mut bytes[..8])
            .await?
            != 8
            || bytes[0] != 18
            || bytes[1] != 1
        {
            return Err("device-prefix");
        }
        let packet = anova_usb::ep0_packet(speed, bytes[7])?;
        if u32::from(packet) != initial {
            let d = self.devices[index].as_ref().ok_or("device-record")?;
            d.input.clear();
            d.input.write(4, 2);
            for word in (0..size).step_by(4) {
                d.input.write(2 * size + word, d.output.read(size + word));
            }
            d.input.write(
                2 * size + 4,
                (d.input.read(2 * size + 4) & 0xffff) | u32::from(packet) << 16,
            );
            self.command_pointer(13, slot, d.input.physical()).await?;
            let actual = self.devices[index]
                .as_ref()
                .ok_or("device-record")?
                .output
                .read(size + 4)
                >> 16;
            if actual != u32::from(packet) {
                return Err("evaluated-packet-context");
            }
            serial::write(format_args!(
                "[USB:EP0] slot={slot} old={initial} new={packet} evaluate=complete\n"
            ));
        }
        if self
            .control(index, 0x80, 6, 0x100, 0, &mut bytes[..18])
            .await?
            != 18
            || bytes[0] != 18
            || bytes[1] != 1
            || bytes[17] == 0
        {
            return Err("device-descriptor");
        }
        let vid = u16::from_le_bytes([bytes[8], bytes[9]]);
        let pid = u16::from_le_bytes([bytes[10], bytes[11]]);
        let class = bytes[4];
        let device_protocol = bytes[6];
        if anova_usb::ep0_packet(speed, bytes[7])? != packet {
            return Err("device-packet-changed");
        }
        self.devices[index]
            .as_mut()
            .ok_or("device-record")?
            .identity = (vid, pid, class);
        if self
            .control(index, 0x80, 6, 0x200, 0, &mut bytes[..9])
            .await?
            != 9
            || bytes[0] != 9
            || bytes[1] != 2
        {
            return Err("configuration-prefix");
        }
        let length = usize::from(u16::from_le_bytes([bytes[2], bytes[3]]));
        if !(9..=MAX_DESCRIPTOR).contains(&length)
            || self
                .control(index, 0x80, 6, 0x200, 0, &mut bytes[..length])
                .await?
                != length
        {
            return Err("configuration-size");
        }
        let config = anova_usb::configuration(&bytes[..length])?;
        let hub = if class == 9 {
            let f = config.interfaces[..config.count]
                .iter()
                .find(|f| f.class == 9 && f.endpoint != 0)
                .ok_or("hub-interface")?;
            let actual = self
                .control(index, 0xa0, 6, 0x2900, 0, &mut bytes[..64])
                .await?;
            Some(anova_usb::topology::Hub::parse_default(
                &bytes[..actual],
                speed,
                device_protocol,
                f.protocol,
            )?)
        } else {
            None
        };
        let d = self.devices[index].as_mut().ok_or("device-record")?;
        d.hub = hub.map(super::hub::HubState::new);
        d.input.clear();
        for word in (0..size).step_by(4) {
            d.input.write(size + word, d.output.read(word));
        }
        if let Some(hub) = hub {
            d.input.write(
                size,
                d.input.read(size) | 1 << 26 | u32::from(hub.multi_tt) << 25,
            );
            d.input.write(
                size + 4,
                d.input.read(size + 4) | u32::from(hub.ports) << 24,
            );
            d.input.write(
                size + 8,
                d.input.read(size + 8) | u32::from(hub.think_time) << 16,
            );
        }
        let mut flags = 1u32;
        let mut highest = 1;
        for (i, f) in config.interfaces[..config.count].iter().enumerate() {
            if !matches!(f.class, 3 | 9) || f.endpoint == 0 || f.class == 9 && hub.is_none() {
                continue;
            }
            let interval = match speed {
                1 | 2 => (7 - f.interval.leading_zeros() as u8) + 3,
                _ if f.interval <= 16 => f.interval - 1,
                _ => return Err("endpoint-interval"),
            };
            let ring = TransferRing::new()?;
            let id = usize::from((f.endpoint & 15) * 2 + 1);
            let offset = (id + 1) * size;
            flags |= 1 << id;
            highest = highest.max(id);
            d.input.write(offset, u32::from(interval) << 16);
            d.input
                .write(offset + 4, 3 << 1 | 7 << 3 | u32::from(f.packet) << 16);
            d.input.address(offset + 8, ring.base() | 1);
            d.input
                .write(offset + 16, u32::from(f.packet) | u32::from(f.packet) << 16);
            d.endpoints[i] = Some(Endpoint {
                interface: *f,
                ring,
                pending: 0,
                failed: false,
                error: 0,
                retries: 0,
                bound: f.class == 9,
                layout: alloc::vec::Vec::new(),
                state: anova_usb::hid::State::default(),
            });
        }
        d.input.write(
            size,
            (d.input.read(size) & !(31 << 27)) | (highest as u32) << 27,
        );
        d.input.write(4, flags);
        let input = d.input.physical();
        if flags != 1 {
            self.command_pointer(12, slot, input).await?;
        }
        self.control(index, 0, 9, u16::from(config.value), 0, &mut [])
            .await?;
        serial::write(format_args!(
            "[USB:DEVICE] port={port} slot={slot} vid={vid:04x} pid={pid:04x} class={class} config={} interfaces={} ep0={packet}\n",
            config.value, config.count
        ));
        for f in &config.interfaces[..config.count] {
            serial::write(format_args!(
                "[USB:INTERFACE] slot={slot} interface={} alternate={} class={} subclass={} protocol={} endpoint={} packet={} interval={} report_bytes={}\n",
                f.number,
                f.alternate,
                f.class,
                f.subclass,
                f.protocol,
                f.endpoint,
                f.packet,
                f.interval,
                f.report_bytes
            ));
        }
        self.devices[index].as_mut().ok_or("device-record")?.config = Some(config);
        if hub.is_some() {
            self.power_hub(index).await?;
        }
        serial::write(format_args!(
            "[USB:ATTACH] slot={slot} generation={} root={port} route={:#x} depth={}\n",
            self.generation, topology.route, topology.depth
        ));
        Ok(())
    }

    /// Complete the whole control TD; short DATA events never masquerade as STATUS completion.
    pub(super) async fn control(
        &mut self,
        index: usize,
        request_type: u8,
        request: u8,
        value: u16,
        interface: u16,
        bytes: &mut [u8],
    ) -> Result<usize, &'static str> {
        super::task::next_turn().await;
        if bytes.len() > MAX_DESCRIPTOR || (request_type & 0x80 == 0 && !bytes.is_empty()) {
            return Err("control-bounds");
        }
        let d = self.devices[index].as_mut().ok_or("device-record")?;
        let slot = d.slot;
        d.control_error = 0;
        let setup = u64::from(request_type)
            | u64::from(request) << 8
            | u64::from(value) << 16
            | u64::from(interface) << 32
            | (bytes.len() as u64) << 48;
        d.control.submit(
            setup,
            8,
            2 << 10 | 1 << 6 | if bytes.is_empty() { 0 } else { 3 << 16 },
        );
        let data = if bytes.is_empty() {
            0
        } else {
            d.control.submit(
                d.control.buffer(),
                bytes.len() as u32,
                3 << 10 | 1 << 16 | 1 << 2,
            )
        };
        let status = d.control.submit(
            0,
            0,
            4 << 10 | 1 << 5 | if bytes.is_empty() { 1 << 16 } else { 0 },
        );
        self.regs.doorbell.write(u32::from(slot) * 4, 1);
        #[cfg(feature = "usb-test-control-timeout")]
        {
            // Submit to real hardware, then deliberately withhold completion observation.
            // Timeout must enter confirmed halt/reset or retain every published DMA frame.
            wait(self.regs.timer, "control-timeout", || false).await?;
        }
        let mut actual = bytes.len();
        let mut outcome = Err("control-missing-status");
        wait(self.regs.timer, "control-timeout", || {
            let Some(event) = self.pop_event() else {
                return false;
            };
            let kind = (event.control >> 10) & 63;
            if kind == 34
                || kind == 32
                    && ((event.control >> 24) as u8 != slot || (event.control >> 16) & 31 != 1)
            {
                self.handle_event(event);
                return false;
            }
            if kind != 32 {
                outcome = Err("control-event-identity");
                return true;
            }
            let code = event.status >> 24;
            if event.pointer == data && code == 13 {
                let residual = (event.status & 0xffffff) as usize;
                if residual > bytes.len() {
                    outcome = Err("control-residual");
                    return true;
                }
                actual = bytes.len() - residual;
                return false;
            }
            if event.pointer == status && code == 1 {
                outcome = Ok(actual);
            } else {
                if let Some(device) = self.devices[index].as_mut() {
                    device.control_error = code as u8;
                }
                serial::write(format_args!(
                    "[USB:TRANSFER_ERROR] slot={slot} code={code} pointer={:#x}\n",
                    event.pointer
                ));
                outcome = Err("control-completion");
            }
            true
        })
        .await?;
        let actual = outcome?;
        if self.devices[index].as_ref().is_none_or(|d| d.retiring) {
            return Err("device-disconnected");
        }
        self.devices[index]
            .as_ref()
            .ok_or("device-record")?
            .control
            .page
            .copy(1024, &mut bytes[..actual]);
        Ok(actual)
    }
}
