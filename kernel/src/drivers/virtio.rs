// SPDX-License-Identifier: Apache-2.0
//! Own one modern virtio-blk split queue and coherent DMA page, with bounded polled completion.
//! A timeout resets the device; DMA is freed only after reset acknowledgement, otherwise quarantined.
use super::pci::VirtioConfig;
use crate::{arch::x86_64::cpu, mm, serial};
use anova_abi::*;
use anova_platform::acpi::PmTimer;
use anova_storage::{BlockDevice, SECTOR, check_range};
use core::{
    ptr::{read_volatile, write_volatile},
    sync::atomic::{Ordering, fence},
};

const AVAIL: usize = 128;
const USED: usize = 256;
const HEADER: usize = 512;
const DATA: usize = 1024;
const STATUS: usize = 1536;
const QUEUE: u16 = 8;
pub struct Block {
    config: VirtioConfig,
    timer: PmTimer,
    dma: Option<mm::DmaPage>,
    notify: u64,
    capacity: u64,
    index: u16,
    failed: bool,
    read_only: bool,
}
impl Block {
    pub fn new(config: VirtioConfig, timer: PmTimer) -> Result<Self, i64> {
        let mut device = Self {
            config,
            timer,
            dma: None,
            notify: 0,
            capacity: 0,
            index: 0,
            failed: false,
            read_only: false,
        };
        device.reset()?;
        device.set8(20, 1);
        device.set8(20, 3);
        device.set32(0, 0);
        let low = device.get32(4);
        device.set32(0, 1);
        let high = device.get32(4);
        if high & 1 == 0 || low & (1 << 9) == 0 {
            return Err(UNSUPPORTED);
        }
        device.read_only = low & (1 << 5) != 0;
        device.set32(8, 0);
        device.set32(12, (1 << 9) | (low & (1 << 5)));
        device.set32(8, 1);
        device.set32(12, 1);
        device.set8(20, 11);
        if device.get8(20) & 8 == 0 {
            return Err(UNSUPPORTED);
        }
        // Config generation brackets the split capacity read; no atomic u64 MMIO assumption.
        let mut stable = false;
        for _ in 0..32 {
            let generation = device.get8(21);
            let low = read32(config.device);
            let high = read32(config.device + 4);
            if generation == device.get8(21) {
                device.capacity = u64::from(low) | u64::from(high) << 32;
                stable = true;
                break;
            }
        }
        if !stable || device.capacity == 0 {
            return Err(IO_ERROR);
        }
        device.set16(22, 0);
        device.set16(16, u16::MAX); // No device-configuration MSI-X vector in a polled driver.
        if device.get16(24) < QUEUE || device.get16(28) != 0 {
            return Err(UNSUPPORTED);
        }
        let offset = u64::from(device.get16(30))
            .checked_mul(u64::from(config.multiplier))
            .ok_or(INVALID_ARGUMENT)?;
        if !offset.is_multiple_of(2)
            || offset
                .checked_add(2)
                .is_none_or(|end| end > u64::from(config.notify_bytes))
        {
            return Err(INVALID_ARGUMENT);
        }
        device.notify = config.notify + offset;
        let page = mm::DmaPage::new()?;
        let physical = page.physical();
        device.dma = Some(page);
        device.set16(24, QUEUE);
        device.set16(26, u16::MAX);
        device.set_address(32, physical);
        device.set_address(40, physical + AVAIL as u64);
        device.set_address(48, physical + USED as u64);
        device.put16(AVAIL, 1); // VIRTQ_AVAIL_F_NO_INTERRUPT: completions are polled.
        config
            .pci
            .command((config.pci.read(4) as u16) | 6 | 1 << 10);
        device.set16(28, 1);
        device.set8(20, 15);
        if device.get8(20) != 15 {
            return Err(IO_ERROR);
        }
        serial::write(format_args!(
            "[BLK:READY] sectors={} queue=8 dma_pages=1 version=1 flush=true readonly={} irq=poll\n",
            device.capacity, device.read_only
        ));
        Ok(device)
    }
    fn now(&self) -> u32 {
        cpu::in32(self.timer.port) & self.timer.mask
    }
    fn expired(&self, start: u32) -> bool {
        self.now().wrapping_sub(start) & self.timer.mask > 7_159_090
    }
    fn reset(&mut self) -> Result<(), i64> {
        self.set8(20, 0);
        let start = self.now();
        while self.get8(20) != 0 {
            if self.expired(start) {
                return Err(DEVICE_TIMEOUT);
            }
            core::hint::spin_loop();
        }
        fence(Ordering::SeqCst);
        Ok(())
    }
    fn pointer(&self, offset: usize) -> *mut u8 {
        assert!(offset < 4096, "DMA offset outside owned page");
        self.dma
            .as_ref()
            .expect("Missing DMA page")
            .pointer()
            .wrapping_add(offset)
    }
    fn put16(&self, offset: usize, value: u16) {
        // SAFETY: Exclusive driver-owned aligned DMA field, within the retained page.
        unsafe {
            write_volatile(self.pointer(offset).cast::<u16>(), value);
        }
    }
    fn put32(&self, offset: usize, value: u32) {
        // SAFETY: Exclusive driver-owned aligned DMA field, within the retained page.
        unsafe {
            write_volatile(self.pointer(offset).cast::<u32>(), value);
        }
    }
    fn put64(&self, offset: usize, value: u64) {
        // SAFETY: Exclusive driver-owned aligned DMA field, within the retained page.
        unsafe {
            write_volatile(self.pointer(offset).cast::<u64>(), value);
        }
    }
    fn descriptor(&self, index: usize, address: u64, length: u32, flags: u16, next: u16) {
        let at = index * 16;
        self.put64(at, address);
        self.put32(at + 8, length);
        self.put16(at + 12, flags);
        self.put16(at + 14, next);
    }
    fn request(
        &mut self,
        kind: u32,
        sector: u64,
        input: Option<&[u8; SECTOR]>,
        output: Option<&mut [u8; SECTOR]>,
    ) -> Result<(), i64> {
        if self.failed {
            return Err(IO_ERROR);
        }
        let flush = kind == 4;
        if !flush {
            check_range(self.capacity, sector, SECTOR)?;
        }
        if kind == 1 && self.read_only {
            return Err(READ_ONLY);
        }
        let physical = self.dma.as_ref().ok_or(IO_ERROR)?.physical();
        self.put32(HEADER, kind);
        self.put32(HEADER + 4, 0);
        self.put64(HEADER + 8, sector);
        // SAFETY: The previous request completed before this page is edited. Only DATA is copied.
        unsafe {
            write_volatile(self.pointer(STATUS), 255);
            if let Some(input) = input {
                core::ptr::copy_nonoverlapping(input.as_ptr(), self.pointer(DATA), SECTOR);
            }
        }
        self.descriptor(
            0,
            physical + HEADER as u64,
            16,
            1,
            if flush { 2 } else { 1 },
        );
        if !flush {
            self.descriptor(
                1,
                physical + DATA as u64,
                SECTOR as u32,
                1 | if kind == 0 { 2 } else { 0 },
                2,
            );
        }
        self.descriptor(2, physical + STATUS as u64, 1, 2, 0);
        self.put16(AVAIL + 4 + usize::from(self.index % QUEUE) * 2, 0);
        fence(Ordering::SeqCst);
        self.index = self.index.wrapping_add(1);
        self.put16(AVAIL + 2, self.index);
        fence(Ordering::SeqCst);
        write16(self.notify, 0);
        let start = self.now();
        loop {
            // SAFETY: Device-owned used index in the retained coherent DMA page, aligned u16.
            let used = unsafe { read_volatile(self.pointer(USED + 2).cast::<u16>()) };
            if used == self.index {
                break;
            }
            if used != self.index.wrapping_sub(1) || self.get8(20) & (64 | 128) != 0 {
                self.failed = true;
                return Err(IO_ERROR);
            }
            if self.expired(start) {
                self.failed = true;
                // Keep the page owned even if reset cannot be acknowledged; Drop handles quarantine.
                let _ = self.reset();
                return Err(DEVICE_TIMEOUT);
            }
            core::hint::spin_loop();
        }
        fence(Ordering::SeqCst);
        let used_at = USED + 4 + usize::from(self.index.wrapping_sub(1) % QUEUE) * 8;
        // SAFETY: Used element/status belong to the completed request; acquire fence precedes reads.
        let (id, length, status) = unsafe {
            (
                read_volatile(self.pointer(used_at).cast::<u32>()),
                read_volatile(self.pointer(used_at + 4).cast::<u32>()),
                read_volatile(self.pointer(STATUS)),
            )
        };
        let expected = if kind == 0 { 513 } else { 1 };
        if id != 0
            || status == 255
            || length == 0
            || length > expected
            || status == 0 && length != expected
        {
            self.failed = true;
            return Err(IO_ERROR);
        }
        match status {
            0 => {}
            1 => {
                serial::write(format_args!(
                    "[BLK:ERROR] operation={kind} status=1 used={length}\n"
                ));
                return Err(IO_ERROR);
            }
            2 => return Err(UNSUPPORTED),
            _ => {
                self.failed = true;
                return Err(IO_ERROR);
            }
        }
        if let Some(output) = output {
            // SAFETY: DMA completion and acquire fence make all 512 DATA bytes CPU-owned again.
            unsafe {
                core::ptr::copy_nonoverlapping(self.pointer(DATA), output.as_mut_ptr(), SECTOR);
            }
        }
        Ok(())
    }
    pub fn verify_boundaries(&mut self) -> Result<(), i64> {
        let mut bytes = [0; SECTOR];
        if self.read_sector(self.capacity, &mut bytes) != Err(IO_ERROR)
            || self.read_sector(u64::MAX, &mut bytes) != Err(IO_ERROR)
        {
            return Err(IO_ERROR);
        }
        self.read_sector(self.capacity - 1, &mut bytes)?;
        self.flush()?;
        serial::write(format_args!(
            "[BLK:BOUNDS] last_sector=ok capacity=reject overflow=reject flush=ok\n"
        ));
        Ok(())
    }
    fn get8(&self, at: u64) -> u8 {
        read8(self.config.common + at)
    }
    fn get16(&self, at: u64) -> u16 {
        read16(self.config.common + at)
    }
    fn get32(&self, at: u64) -> u32 {
        read32(self.config.common + at)
    }
    fn set8(&self, at: u64, value: u8) {
        write8(self.config.common + at, value);
    }
    fn set16(&self, at: u64, value: u16) {
        write16(self.config.common + at, value);
    }
    fn set32(&self, at: u64, value: u32) {
        write32(self.config.common + at, value);
    }
    fn set_address(&self, at: u64, value: u64) {
        self.set32(at, value as u32);
        self.set32(at + 4, (value >> 32) as u32);
    }
}
impl BlockDevice for Block {
    fn sectors(&self) -> u64 {
        self.capacity
    }
    fn read_only(&self) -> bool {
        self.read_only
    }
    fn read_sector(&mut self, sector: u64, output: &mut [u8; SECTOR]) -> Result<(), i64> {
        self.request(0, sector, None, Some(output))
    }
    fn write_sector(&mut self, sector: u64, input: &[u8; SECTOR]) -> Result<(), i64> {
        self.request(1, sector, Some(input), None)
    }
    fn flush(&mut self) -> Result<(), i64> {
        self.request(4, 0, None, None)
    }
    fn stop(&mut self) -> Result<(), i64> {
        self.reset()?;
        self.config
            .pci
            .command((self.config.pci.read(4) as u16) & !4);
        drop(self.dma.take());
        serial::write(format_args!(
            "[BLK:RESET] acknowledged=true dma_released=true\n"
        ));
        Ok(())
    }
}
impl Drop for Block {
    fn drop(&mut self) {
        if self.dma.is_none() {
            return;
        }
        if self.reset().is_ok() {
            self.config
                .pci
                .command((self.config.pci.read(4) as u16) & !4);
            drop(self.dma.take());
            serial::write(format_args!(
                "[BLK:RESET] acknowledged=true dma_released=true\n"
            ));
        } else if let Some(page) = self.dma.take() {
            // A device that might still DMA must never access a recycled frame.
            core::mem::forget(page);
            self.config
                .pci
                .command((self.config.pci.read(4) as u16) & !4);
            serial::emergency(format_args!(
                "[BLK:QUARANTINE] reset timeout; DMA frame retained\n"
            ));
        }
    }
}
fn read8(address: u64) -> u8 {
    // SAFETY: PCI capability validation mapped this bounded register UC, NX and supervisor-only.
    unsafe { read_volatile(address as *const u8) }
}
fn read16(address: u64) -> u16 {
    // SAFETY: Validated aligned capability/notification register, mapped UC for the device lifetime.
    unsafe { read_volatile(address as *const u16) }
}
fn read32(address: u64) -> u32 {
    // SAFETY: Validated aligned capability register, mapped UC for the device lifetime.
    unsafe { read_volatile(address as *const u32) }
}
fn write8(address: u64, value: u8) {
    // SAFETY: Validated writable capability register; sole BSP driver serializes access.
    unsafe {
        write_volatile(address as *mut u8, value);
    }
}
fn write16(address: u64, value: u16) {
    // SAFETY: Validated aligned writable capability register; sole BSP driver serializes access.
    unsafe {
        write_volatile(address as *mut u16, value);
    }
}
fn write32(address: u64, value: u32) {
    // SAFETY: Validated aligned writable capability register; sole BSP driver serializes access.
    unsafe {
        write_volatile(address as *mut u32, value);
    }
}
