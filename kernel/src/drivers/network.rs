// SPDX-License-Identifier: Apache-2.0
//! Own modern virtio-net RX/TX split queues; copy bounded Ethernet frames before recycling DMA.
//! Only the BSP scheduler accesses the device at IF=0. Device reset precedes DMA release.
use super::pci::{self, VirtioConfig};
use crate::{
    arch::x86_64::{cpu, paging::EarlyMapper},
    mm, serial,
};
use anova_platform::acpi::PmTimer;
use core::{
    cell::UnsafeCell,
    ptr::{read_volatile, write_volatile},
    sync::atomic::{Ordering, fence},
};
use smoltcp::{
    phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken},
    time::Instant,
};
const AVAIL: usize = 128;
const USED: usize = 256;
const DATA: usize = 1024;
const BUFFER: usize = 1536;
const HEADER: usize = 12;
struct Prepared(UnsafeCell<Option<(VirtioConfig, PmTimer)>>);
// SAFETY: Boot writes once; the sole BSP scheduler consumes it with IF=0, never an IRQ.
unsafe impl Sync for Prepared {}
static PREPARED: Prepared = Prepared(UnsafeCell::new(None));
pub fn prepare(mapper: &mut EarlyMapper<'_>, timer: PmTimer) {
    match pci::discover(mapper, true) {
        // SAFETY: Exclusive BSP boot handoff, no active scheduler or IRQ accesses this cell.
        Ok(config) => unsafe {
            *PREPARED.0.get() = config.map(|c| (c, timer));
        },
        Err(e) => serial::write(format_args!("[NET:REJECT] PCI error={e}\n")),
    }
}
struct Queue {
    page: mm::DmaPage,
    notify: u64,
    available: u16,
    consumed: u16,
    number: u16,
    in_flight: [bool; 2],
}
impl Queue {
    fn get16(&self, at: usize) -> u16 {
        // SAFETY: Aligned queue index inside a retained coherent DMA page.
        unsafe { read_volatile(self.page.pointer().add(at).cast()) }
    }
    fn put16(&self, at: usize, value: u16) {
        // SAFETY: Aligned driver-owned queue field in retained DMA; serialized on the BSP.
        unsafe {
            write_volatile(self.page.pointer().add(at).cast(), value);
        }
    }
    fn publish(&mut self, id: u16) {
        assert!(
            !self.in_flight[id as usize],
            "Republishing a device-owned descriptor"
        );
        self.in_flight[id as usize] = true;
        self.put16(AVAIL + 4 + (self.available as usize % 2) * 2, id);
        fence(Ordering::Release);
        self.available = self.available.wrapping_add(1);
        self.put16(AVAIL + 2, self.available);
        fence(Ordering::SeqCst);
        // SAFETY: Validated aligned notify capability, one live queue and IF=0.
        unsafe {
            write_volatile(self.notify as *mut u16, self.number);
        }
    }
    fn used(&mut self) -> Result<Option<(usize, usize)>, i64> {
        let end = self.get16(USED + 2);
        if end == self.consumed {
            return Ok(None);
        }
        if end.wrapping_sub(self.consumed) > 2 {
            return Err(-5);
        }
        fence(Ordering::Acquire);
        let at = USED + 4 + (self.consumed as usize % 2) * 8;
        // SAFETY: Used entry is within the DMA ring; acquire orders payload after used-index read.
        let (id, len) = unsafe {
            (
                read_volatile(self.page.pointer().add(at).cast::<u32>()),
                read_volatile(self.page.pointer().add(at + 4).cast::<u32>()),
            )
        };
        if id >= 2 || len > BUFFER as u32 || !self.in_flight[id as usize] {
            return Err(-5);
        }
        self.in_flight[id as usize] = false;
        self.consumed = self.consumed.wrapping_add(1);
        Ok(Some((id as usize, len as usize)))
    }
}
pub struct Network {
    config: VirtioConfig,
    timer: PmTimer,
    rx: Option<Queue>,
    tx: Option<Queue>,
    free: [bool; 2],
    pub mac: [u8; 6],
    failed: bool,
}
impl Network {
    pub fn attach() -> Result<Option<Self>, i64> {
        // SAFETY: Sole BSP scheduler, IF=0; take prevents a second device owner.
        let Some((config, timer)) = (unsafe { (&mut *PREPARED.0.get()).take() }) else {
            return Ok(None);
        };
        let mut n = Self {
            config,
            timer,
            rx: None,
            tx: None,
            free: [true; 2],
            mac: [0; 6],
            failed: false,
        };
        n.reset()?;
        n.w8(20, 1);
        n.w8(20, 3);
        n.w32(0, 0);
        let low = n.r32(4);
        n.w32(0, 1);
        let high = n.r32(4);
        if low & (1 << 5) == 0 || high & 1 == 0 {
            return Err(-95);
        }
        n.w32(8, 0);
        n.w32(12, 1 << 5);
        n.w32(8, 1);
        n.w32(12, 1);
        n.w8(20, 11);
        if n.r8(20) & 8 == 0 {
            return Err(-95);
        }
        let mut stable = false;
        for _ in 0..32 {
            let generation = n.r8(21);
            for k in 0..6 {
                // SAFETY: The device config capability covers the six MAC bytes.
                n.mac[k] = unsafe { read_volatile((config.device + k as u64) as *const u8) };
            }
            if generation == n.r8(21) {
                stable = true;
                break;
            }
        }
        if !stable || n.mac[0] & 1 != 0 || n.mac == [0; 6] {
            return Err(-5);
        }
        n.w16(16, u16::MAX);
        n.rx = Some(n.queue(0)?);
        n.tx = Some(n.queue(1)?);
        config.pci.command(config.pci.read(4) as u16 | 6 | 1 << 10);
        n.w8(20, 15);
        if n.r8(20) != 15 {
            return Err(-5);
        }
        for id in 0..2 {
            n.rx.as_mut().unwrap().publish(id);
        }
        serial::write(format_args!(
            "[NET:READY] virtio=modern rx=2 tx=2 dma_pages=2 irq=poll mac={:02x?}\n",
            n.mac
        ));
        Ok(Some(n))
    }
    fn queue(&self, number: u16) -> Result<Queue, i64> {
        self.w16(22, number);
        if self.r16(24) < 2 || self.r16(28) != 0 {
            return Err(-95);
        }
        let offset = u64::from(self.r16(30))
            .checked_mul(u64::from(self.config.multiplier))
            .ok_or(-22)?;
        if offset % 2 != 0
            || offset
                .checked_add(2)
                .is_none_or(|e| e > u64::from(self.config.notify_bytes))
        {
            return Err(-22);
        }
        let q = Queue {
            page: mm::DmaPage::new()?,
            notify: self.config.notify + offset,
            available: 0,
            consumed: 0,
            number,
            in_flight: [false; 2],
        };
        for id in 0..2 {
            let at = id * 16;
            let address = q.page.physical() + (DATA + id * BUFFER) as u64;
            // SAFETY: Two aligned descriptors point wholly inside this retained DMA page.
            unsafe {
                write_volatile(q.page.pointer().add(at).cast::<u64>(), address);
                write_volatile(q.page.pointer().add(at + 8).cast::<u32>(), BUFFER as u32);
            }
            q.put16(at + 12, if number == 0 { 2 } else { 0 });
        }
        q.put16(AVAIL, 1);
        self.w16(24, 2);
        self.w16(26, u16::MAX);
        for (offset, value) in [
            (32, q.page.physical()),
            (40, q.page.physical() + AVAIL as u64),
            (48, q.page.physical() + USED as u64),
        ] {
            self.w32(offset, value as u32);
            self.w32(offset + 4, (value >> 32) as u32);
        }
        self.w16(28, 1);
        Ok(q)
    }
    fn reset(&self) -> Result<(), i64> {
        self.w8(20, 0);
        let start = cpu::in32(self.timer.port);
        while self.r8(20) != 0 {
            if cpu::in32(self.timer.port).wrapping_sub(start) & self.timer.mask > 7_159_090 {
                return Err(-110);
            }
            core::hint::spin_loop();
        }
        fence(Ordering::SeqCst);
        Ok(())
    }
    fn reap(&mut self) {
        if self.failed {
            return;
        }
        while let Some(q) = self.tx.as_mut() {
            match q.used() {
                Ok(Some((id, _))) => {
                    if self.free[id] {
                        self.failed = true;
                        break;
                    }
                    self.free[id] = true;
                }
                Ok(None) => break,
                Err(_) => {
                    self.failed = true;
                    break;
                }
            }
        }
    }
    fn send(&mut self, bytes: &[u8]) {
        self.reap();
        if self.failed || bytes.len() > 1514 {
            return;
        }
        let Some(id) = self.free.iter().position(|v| *v) else {
            return;
        };
        let q = self.tx.as_mut().unwrap();
        // SAFETY: Free TX descriptor, owned buffer; publish releases the complete header and payload.
        unsafe {
            core::ptr::write_bytes(q.page.pointer().add(DATA + id * BUFFER), 0, HEADER);
            core::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                q.page.pointer().add(DATA + id * BUFFER + HEADER),
                bytes.len(),
            );
            write_volatile(
                q.page.pointer().add(id * 16 + 8).cast::<u32>(),
                (bytes.len() + HEADER) as u32,
            );
        }
        self.free[id] = false;
        q.publish(id as u16);
    }
    pub fn stop(&mut self) -> Result<(), i64> {
        self.reset()?;
        self.rx = None;
        self.tx = None;
        Ok(())
    }
    fn r8(&self, o: u64) -> u8 {
        // SAFETY: Validated common capability field, aligned to its access width.
        unsafe { read_volatile((self.config.common + o) as *const u8) }
    }
    fn r16(&self, o: u64) -> u16 {
        // SAFETY: Validated common capability field, aligned to its access width.
        unsafe { read_volatile((self.config.common + o) as *const u16) }
    }
    fn r32(&self, o: u64) -> u32 {
        // SAFETY: Validated common capability field, aligned to its access width.
        unsafe { read_volatile((self.config.common + o) as *const u32) }
    }
    fn w8(&self, o: u64, v: u8) {
        // SAFETY: Sole driver writes validated common capability field.
        unsafe {
            write_volatile((self.config.common + o) as *mut u8, v);
        }
    }
    fn w16(&self, o: u64, v: u16) {
        // SAFETY: Sole driver writes validated aligned common capability field.
        unsafe {
            write_volatile((self.config.common + o) as *mut u16, v);
        }
    }
    fn w32(&self, o: u64, v: u32) {
        // SAFETY: Sole driver writes validated aligned common capability field.
        unsafe {
            write_volatile((self.config.common + o) as *mut u32, v);
        }
    }
}
impl Drop for Network {
    fn drop(&mut self) {
        if self.reset().is_err() {
            if let Some(q) = self.rx.take() {
                core::mem::forget(q);
            }
            if let Some(q) = self.tx.take() {
                core::mem::forget(q);
            }
            serial::write(format_args!(
                "[NET:QUARANTINE] reset timeout; DMA retained\n"
            ));
        } else {
            serial::write(format_args!(
                "[NET:RESET] acknowledged=true dma_released=true\n"
            ));
        }
    }
}
pub struct Rx {
    bytes: [u8; 1514],
    len: usize,
}
pub struct Tx<'a>(&'a mut Network);
impl RxToken for Rx {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.bytes[..self.len])
    }
}
impl TxToken for Tx<'_> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut bytes = [0; 1514];
        let result = f(&mut bytes[..len]);
        self.0.send(&bytes[..len]);
        result
    }
}
impl Device for Network {
    type RxToken<'a> = Rx;
    type TxToken<'a> = Tx<'a>;
    fn receive(&mut self, _: Instant) -> Option<(Rx, Tx<'_>)> {
        self.reap();
        if self.failed || self.free.iter().all(|v| !*v) {
            return None;
        }
        let q = self.rx.as_mut()?;
        let (id, len) = match q.used() {
            Ok(Some(v)) => v,
            Ok(None) => return None,
            Err(_) => {
                self.failed = true;
                return None;
            }
        };
        if !(HEADER + 14..=HEADER + 1514).contains(&len) {
            self.failed = true;
            return None;
        }
        let mut rx = Rx {
            bytes: [0; 1514],
            len: len - HEADER,
        };
        // SAFETY: Completed RX descriptor and validated length; copy before returning DMA ownership.
        unsafe {
            core::ptr::copy_nonoverlapping(
                q.page.pointer().add(DATA + id * BUFFER + HEADER),
                rx.bytes.as_mut_ptr(),
                rx.len,
            );
        }
        q.publish(id as u16);
        Some((rx, Tx(self)))
    }
    fn transmit(&mut self, _: Instant) -> Option<Tx<'_>> {
        self.reap();
        if self.failed || self.free.iter().all(|v| !*v) {
            None
        } else {
            Some(Tx(self))
        }
    }
    fn capabilities(&self) -> DeviceCapabilities {
        let mut c = DeviceCapabilities::default();
        c.medium = Medium::Ethernet;
        c.max_transmission_unit = 1514;
        c.max_burst_size = Some(2);
        c
    }
}
