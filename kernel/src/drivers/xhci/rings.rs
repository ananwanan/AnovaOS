// SPDX-License-Identifier: Apache-2.0
//! Own DMA32 rings and a separate scratchpad pointer page for up to 512 controller buffers.

use crate::mm::DmaPage;
use alloc::vec::Vec;
use core::{
    ptr::{read_volatile, write_volatile},
    sync::atomic::{Ordering, fence},
};

const DCBAA: usize = 0;
const ERST: usize = 128;
const SCRATCHPAD_LIMIT: usize = 4096 / 8;
const COMMAND: usize = 1024;
const EVENTS: usize = 2048;
const COMMAND_ENTRIES: usize = 16;
const EVENT_ENTRIES: usize = 64;
const _: () = assert!(EVENT_ENTRIES > 8 * anova_usb::MAX_INTERFACES + 16);
const STRIDE: usize = 16;
const _: () = assert!(9 * 8 <= ERST && ERST + 16 <= COMMAND);
const _: () = assert!(COMMAND + COMMAND_ENTRIES * STRIDE <= EVENTS);
const _: () = assert!(EVENTS + EVENT_ENTRIES * STRIDE <= 4096);

pub(super) struct Rings {
    page: DmaPage,
    scratchpads: Vec<DmaPage>,
    scratch_array: Option<DmaPage>,
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
    #[cfg(feature = "qemu-test")]
    pub fn test_scratchpads() -> Result<(), &'static str> {
        for count in [0, 33, 512] {
            let rings = Self::new(count)?;
            assert_eq!(rings.scratchpads.len(), count);
            for (index, page) in rings.scratchpads.iter().enumerate() {
                let array = rings
                    .scratch_array
                    .as_ref()
                    .ok_or("scratchpad-array-missing")?;
                // SAFETY: The checked index addresses a live aligned entry; no DMA is published.
                let actual = unsafe { read_volatile(array.pointer().add(index * 8).cast::<u64>()) };
                assert_eq!(actual, page.physical());
            }
            assert_eq!(rings.read(COMMAND), 0);
            assert_eq!(rings.read(EVENTS), 0);
        }
        assert!(matches!(Self::new(513), Err("scratchpad-limit")));
        crate::serial::write(format_args!(
            "[USB:SCRATCH_TEST] counts=0,33,512 overflow=reject passed=true\n"
        ));
        Ok(())
    }

    pub fn new(count: usize) -> Result<Self, &'static str> {
        if count > SCRATCHPAD_LIMIT {
            return Err("scratchpad-limit");
        }
        let mut rings = Self {
            page: DmaPage::new().map_err(|_| "ring-allocation")?,
            scratchpads: Vec::new(),
            scratch_array: None,
            producer: 0,
            producer_cycle: true,
            consumer: 0,
            consumer_cycle: true,
        };
        rings
            .scratchpads
            .try_reserve_exact(count)
            .map_err(|_| "scratchpad-allocation")?;
        if count > 0 {
            rings.scratch_array = Some(DmaPage::new().map_err(|_| "scratchpad-array-allocation")?);
        }
        for i in 0..count {
            let page = DmaPage::new().map_err(|_| "scratchpad-allocation")?;
            let array = rings
                .scratch_array
                .as_ref()
                .ok_or("scratchpad-array-missing")?;
            // SAFETY: i < count <= 512; each aligned u64 lies in the owned 4 KiB DMA page.
            // Hardware has not received DCBAA yet; partial allocation drops all owned pages.
            unsafe { write_volatile(array.pointer().add(i * 8).cast::<u64>(), page.physical()) };
            rings.scratchpads.push(page);
        }
        if count > 0 {
            rings.write_address(
                DCBAA,
                rings
                    .scratch_array
                    .as_ref()
                    .ok_or("scratchpad-array-missing")?
                    .physical(),
            );
        }
        rings.write_address(ERST, rings.physical(EVENTS));
        rings.write(ERST + 8, EVENT_ENTRIES as u32);
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

    pub fn device(&self, slot: u8, pointer: u64) {
        assert!((1..=8).contains(&slot));
        self.write_address(DCBAA + usize::from(slot) * 8, pointer);
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
    pub fn submit(&mut self, kind: u8, slot: u8, pointer: u64, endpoint: u8) -> u64 {
        let offset = COMMAND + self.producer * STRIDE;
        let cycle = u32::from(self.producer_cycle);
        self.producer += 1;
        if self.producer == COMMAND_ENTRIES - 1 {
            let link = COMMAND + self.producer * STRIDE;
            self.write_address(link, self.command_base());
            self.write(link + 8, 0);
            fence(Ordering::Release);
            self.write(link + 12, 6 << 10 | 2 | cycle);
            self.producer = 0;
            self.producer_cycle = !self.producer_cycle;
        }
        self.write_address(offset, pointer);
        self.write(offset + 8, 0);
        fence(Ordering::Release);
        self.write(
            offset + 12,
            u32::from(slot) << 24 | u32::from(endpoint) << 16 | u32::from(kind) << 10 | cycle,
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
        if self.consumer == EVENT_ENTRIES {
            self.consumer = 0;
            self.consumer_cycle = !self.consumer_cycle;
        }
        Some(event)
    }
}
