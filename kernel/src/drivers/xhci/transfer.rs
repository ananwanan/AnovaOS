// SPDX-License-Identifier: Apache-2.0
//! Own one bounded transfer ring and its non-overlapping payload, plus checked context words.
use crate::mm::DmaPage;
use core::{
    ptr::{read_volatile, write_volatile},
    sync::atomic::{Ordering, fence},
};

pub(super) struct Page(pub DmaPage);
impl Page {
    pub fn new() -> Result<Self, &'static str> {
        Ok(Self(DmaPage::new().map_err(|_| "usb-dma-allocation")?))
    }
    pub fn physical(&self) -> u64 {
        self.0.physical()
    }
    pub fn write(&self, offset: usize, value: u32) {
        assert!(offset <= 4092 && offset.is_multiple_of(4));
        // SAFETY: Checked aligned word in the retained owned coherent DMA frame.
        unsafe {
            write_volatile(self.0.pointer().add(offset).cast::<u32>(), value);
        }
    }
    pub fn read(&self, offset: usize) -> u32 {
        assert!(offset <= 4092 && offset.is_multiple_of(4));
        // SAFETY: Checked aligned word, DMA lifetime is owned by the controller.
        unsafe { read_volatile(self.0.pointer().add(offset).cast::<u32>()) }
    }
    pub fn address(&self, offset: usize, value: u64) {
        self.write(offset, value as u32);
        self.write(offset + 4, (value >> 32) as u32);
    }
    pub fn clear(&self) {
        for offset in (0..4096).step_by(4) {
            self.write(offset, 0);
        }
    }
    pub fn copy(&self, offset: usize, bytes: &mut [u8]) {
        assert!(offset.checked_add(bytes.len()).is_some_and(|n| n <= 4096));
        fence(Ordering::Acquire);
        for (i, b) in bytes.iter_mut().enumerate() {
            // SAFETY: Checked byte range; only read after a correlated transfer completion.
            *b = unsafe { read_volatile(self.0.pointer().add(offset + i)) };
        }
    }
}

pub(super) struct TransferRing {
    pub page: Page,
    index: usize,
    cycle: bool,
}
impl TransferRing {
    pub fn new() -> Result<Self, &'static str> {
        Ok(Self {
            page: Page::new()?,
            index: 0,
            cycle: true,
        })
    }
    pub fn base(&self) -> u64 {
        self.page.physical()
    }
    pub fn buffer(&self) -> u64 {
        self.base() + 1024
    }
    /// The next unsubmitted TRB and its producer cycle, used only after endpoint quiescence.
    pub fn dequeue(&self) -> u64 {
        (self.base() + self.index as u64 * 16) | u64::from(self.cycle)
    }
    /// Caller serializes one TD per ring. A published cycle bit transfers ownership to hardware.
    pub fn submit(&mut self, pointer: u64, status: u32, control: u32) -> u64 {
        let offset = self.index * 16;
        let cycle = u32::from(self.cycle);
        self.page.address(offset, pointer);
        self.page.write(offset + 8, status);
        fence(Ordering::Release);
        self.page.write(offset + 12, control | cycle);
        self.index += 1;
        if self.index == 15 {
            self.page.address(240, self.base());
            self.page.write(248, 0);
            fence(Ordering::Release);
            self.page.write(252, 6 << 10 | 2 | cycle);
            self.index = 0;
            self.cycle = !self.cycle;
        }
        fence(Ordering::SeqCst);
        self.base() + offset as u64
    }
}
