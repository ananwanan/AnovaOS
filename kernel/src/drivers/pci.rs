// SPDX-License-Identifier: Apache-2.0
//! Probe q35 PCI configuration mechanism #1 for a modern virtio-blk function.
/// Device discovery intentionally does not enable bus mastering or touch BAR memory.
use anova_storage::pci::{self, Configuration};
use core::arch::asm;

use crate::serial;

struct ConfigPort;

impl Configuration for ConfigPort {
    fn read32(&mut self, bus: u8, device: u8, function: u8, offset: u8) -> u32 {
        assert!(device < 32 && function < 8 && offset & 3 == 0);
        let address = 0x8000_0000_u32
            | (u32::from(bus) << 16)
            | (u32::from(device) << 11)
            | (u32::from(function) << 8)
            | u32::from(offset);
        let result: u32;
        // SAFETY: This BSP-only probe runs with IF=0. Ports CF8/CFC are
        // PCI configuration mechanism #1 on the q35 reference platform.
        // No guest device memory or unowned DMA buffers are dereferenced.
        unsafe {
            asm!("out dx, eax", in("dx") 0xcf8_u16, in("eax") address, options(nomem, nostack, preserves_flags));
            asm!("in eax, dx", in("dx") 0xcfc_u16, out("eax") result, options(nomem, nostack, preserves_flags));
        }
        result
    }
}

pub fn probe() {
    let mut io = ConfigPort;
    if let Some(location) = pci::find_block(&mut io) {
        match pci::capabilities(&mut io, location) {
            Ok(caps) => serial::write(format_args!(
                "[M7:PCI] modern-virtio-blk={:02x}:{:02x}.{} common-bar={} notify-bar={} device-bar={} dma=disabled\n",
                location.bus,
                location.device,
                location.function,
                caps.common.bar,
                caps.notify.bar,
                caps.device.bar
            )),
            Err(error) => serial::write(format_args!(
                "[M7:PCI] virtio-blk rejected: {error:?}; dma=disabled\n"
            )),
        }
    } else {
        serial::write(format_args!(
            "[M7:PCI] no modern virtio-blk; dma=disabled\n"
        ));
    }
}
