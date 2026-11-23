// SPDX-License-Identifier: Apache-2.0
//! Own coherent DMA32 storage for the DCBAA, scratchpad array, ERST and two single-segment rings.

use crate::mm::DmaPage;
use alloc::vec::Vec;
use core::{
    ptr::{read_volatile, write_volatile},
    sync::atomic::{Ordering, fence},
};

const DCBAA: usize = 0;
const ERST: usize = 128;
const SCRATCH_ARRAY: usize = 256;
const COMMAND: usize = 1024;
const EVENTS: usize = 2048;
const ENTRIES: usize = 16;
const STRIDE: usize = 16;
const _: () = assert!(9 * 8 <= ERST && ERST + 16 <= SCRATCH_ARRAY);
const _: () = assert!(SCRATCH_ARRAY + 32 * 8 <= COMMAND && COMMAND + ENTRIES * STRIDE <= EVENTS);
const _: () = assert!(EVENTS + ENTRIES * STRIDE <= 4096);

pub(super) struct Rings {
    page: DmaPage,
    scratchpads: Vec<DmaPage>,
    producer: usize,
    producer_cycle: bool,
    consumer: usize,
    consumer_cycle: bool,
}

pub(super) struct Completion {
    pub pointer: u64,
    pub status: u32,
    pub control: u32,
}

impl Rings {
    pub fn new(count: usize) -> Result<Self, &'static str> {
        if count > 32 {
            return Err("scratchpad-limit");
        }
        let mut rings = Self {
            page: DmaPage::new().map_err(|_| "ring-allocation")?,
            scratchpads: Vec::new(),
            producer: 0,
            producer_cycle: true,
            consumer: 0,
            consumer_cycle: true,
        };
        rings
            .scratchpads
            .try_reserve_exact(count)
            .map_err(|_| "scratchpad-allocation")?;
        for i in 0..count {
            let page = DmaPage::new().map_err(|_| "scratchpad-allocation")?;
            rings.write_address(SCRATCH_ARRAY + i * 8, page.physical());
            rings.scratchpads.push(page);
        }
        if count > 0 {
            rings.write_address(DCBAA, rings.physical(SCRATCH_ARRAY));
        }
        rings.write_address(ERST, rings.physical(EVENTS));
        rings.write(ERST + 8, ENTRIES as u32);
        Ok(rings)
    }

    fn physical(&self, offset: usize) -> u64 {
        self.page.physical() + offset as u64
    }

    fn write(&self, offset: usize, value: u32) {
        debug_assert!(offset <= 4092 && offset.is_multiple_of(4));
        // SAFETY: Fixed layout or bounded ring cursor, aligned DWORD in a retained DMA32 frame.
        unsafe { write_volatile(self.page.pointer().add(offset).cast::<u32>(), value) };
    }

    fn read(&self, offset: usize) -> u32 {
        debug_assert!(offset <= 4092 && offset.is_multiple_of(4));
        // SAFETY: Bounded event index and word offsets stay inside the coherent retained segment.
        unsafe { read_volatile(self.page.pointer().add(offset).cast::<u32>()) }
    }

    fn write_address(&self, offset: usize, value: u64) {
        self.write(offset, value as u32);
        self.write(offset + 4, (value >> 32) as u32);
    }

    pub fn dcbaa(&self) -> u64 {
        self.physical(DCBAA)
    }

    pub fn command_base(&self) -> u64 {
        self.physical(COMMAND)
    }

    pub fn erst(&self) -> u64 {
        self.physical(ERST)
    }

    pub fn dequeue(&self) -> u64 {
        self.physical(EVENTS + self.consumer * STRIDE)
    }

    /// Boot commands are serialized until the matching completion retires this TRB.
    pub fn submit(&mut self, kind: u8, slot: u8) -> u64 {
        let offset = COMMAND + self.producer * STRIDE;
        let cycle = u32::from(self.producer_cycle);
        self.producer += 1;
        if self.producer == ENTRIES - 1 {
            let link = COMMAND + self.producer * STRIDE;
            self.write_address(link, self.command_base());
            self.write(link + 8, 0);
            fence(Ordering::Release);
            self.write(link + 12, 6 << 10 | 2 | cycle);
            self.producer = 0;
            self.producer_cycle = !self.producer_cycle;
        }
        self.write_address(offset, 0);
        self.write(offset + 8, 0);
        fence(Ordering::Release);
        self.write(
            offset + 12,
            u32::from(slot) << 24 | u32::from(kind) << 10 | cycle,
        );
        fence(Ordering::SeqCst);
        self.physical(offset)
    }

    pub fn pop(&mut self) -> Option<Completion> {
        let offset = EVENTS + self.consumer * STRIDE;
        let control = self.read(offset + 12);
        if control & 1 != u32::from(self.consumer_cycle) {
            return None;
        }
        fence(Ordering::Acquire);
        let event = Completion {
            pointer: u64::from(self.read(offset)) | u64::from(self.read(offset + 4)) << 32,
            status: self.read(offset + 8),
            control,
        };
        self.consumer += 1;
        if self.consumer == ENTRIES {
            self.consumer = 0;
            self.consumer_cycle = !self.consumer_cycle;
        }
        Some(event)
    }
}
