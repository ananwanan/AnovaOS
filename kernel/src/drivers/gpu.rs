// SPDX-License-Identifier: Apache-2.0
//! Render bounded offscreen VirGL clears through an optional modern virtio-gpu control queue.
//! Every command is fenced; readback is checked before copying RGB pixels to the caller.
//! The sole BSP at IF=0 owns all DMA. Reset precedes release; failed reset quarantines all
//! exposed pages and permanently disables the device. No user command stream reaches the host.

use super::pci::{self, VirtioConfig};
use crate::{
    arch::x86_64::{cpu, paging::EarlyMapper},
    mm, serial,
};
use anova_abi::{DEVICE_TIMEOUT, INVALID_ARGUMENT, IO_ERROR, UNSUPPORTED};
use anova_graphics::virgl::{MAX_DIMENSION, clear_stream};
use anova_platform::acpi::PmTimer;
use core::{
    cell::UnsafeCell,
    ptr::{read_volatile, write_volatile},
    sync::atomic::{Ordering, fence},
};

const QUEUE: u16 = 8;
const AVAIL: usize = 128;
const USED: usize = 256;
const REQUEST: usize = 512;
const RESPONSE: usize = 1024;
const RESPONSE_BYTES: usize = 4096 - RESPONSE;
const HEADER_BYTES: usize = 24;
const BACKING_PAGES: usize = 4;
const DEADLINE_TICKS: u32 = 7_159_090;
const POLL_LIMIT: usize = 10_000_000;
const OK_NODATA: u32 = 0x1100;
const OK_CAPSET_INFO: u32 = 0x1102;
const OK_CAPSET: u32 = 0x1103;

#[derive(Clone, Copy)]
struct Handoff {
    config: Option<(VirtioConfig, PmTimer)>,
    disabled: bool,
}
struct Prepared(UnsafeCell<Handoff>);
// SAFETY: Boot and all runtime access occur on the sole BSP with IF=0, outside IRQ handlers.
unsafe impl Sync for Prepared {}
static PREPARED: Prepared = Prepared(UnsafeCell::new(Handoff {
    config: None,
    disabled: false,
}));

/// Map only the optional device's checked PCI capabilities before owned paging is imported.
pub fn prepare(mapper: &mut EarlyMapper<'_>, timer: PmTimer) {
    assert_eq!(cpu::flags() & (1 << 9), 0, "GPU preparation requires IF=0");
    match pci::discover_gpu(mapper) {
        Ok(config) => {
            // SAFETY: Boot is the exclusive writer; no driver or scheduler is active yet.
            unsafe {
                (*PREPARED.0.get()).config = config.map(|config| (config, timer));
            }
            if config.is_none() {
                serial::write(format_args!("[GPU:ABSENT] cpu_fallback_available=true\n"));
            }
        }
        Err(error) => serial::write(format_args!("[GPU:REJECT] pci_error={error}\n")),
    }
}

/// Produce tightly packed RGB pixels through the GPU, or return an error for CPU fallback.
/// The driver never fills a reference image on failure or accepts a user-provided command stream.
pub fn render(width: u32, height: u32, rgb: u32, pixels: &mut [u32]) -> Result<(), i64> {
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || pixels.len() != (width * height) as usize
    {
        return Err(INVALID_ARGUMENT);
    }
    with_device(|device| device.render(width, height, rgb & 0x00ff_ffff, pixels))
}

/// Publish a real request while suppressing its doorbell, then exercise the watchdog and reset.
#[cfg(feature = "gpu-test")]
pub fn force_timeout() -> Result<(), i64> {
    with_device(|device| {
        serial::write(format_args!(
            "[GPU:INJECT] lost_doorbell=true published=true\n"
        ));
        let mut command = Command::new(0x0108, 0, 32); // GET_CAPSET_INFO.
        command.word(24, 0);
        device.exchange(&mut command, 40, false).map(|_| ())
    })
}

fn handoff() -> Handoff {
    assert_eq!(cpu::flags() & (1 << 9), 0, "GPU access requires IF=0");
    // SAFETY: Sole BSP, IF=0 and no retained borrow across device operations.
    unsafe { *PREPARED.0.get() }
}

fn disable() {
    // SAFETY: Sole BSP at IF=0; disable is called outside any handoff borrow or lock.
    unsafe {
        (*PREPARED.0.get()).disabled = true;
    }
}

fn with_device(operation: impl FnOnce(&mut Gpu) -> Result<(), i64>) -> Result<(), i64> {
    let prepared = handoff();
    if prepared.disabled {
        return Err(IO_ERROR);
    }
    let (config, timer) = prepared.config.ok_or(UNSUPPORTED)?;
    let frames_before = mm::user::free_frames();
    let heap_before = mm::heap_used();
    let result = match Gpu::new(config, timer) {
        Ok(mut device) => {
            let outcome = operation(&mut device);
            let stopped = device.stop();
            outcome.and(stopped)
        }
        Err(error) => Err(error),
    };
    assert_eq!(
        mm::heap_used(),
        heap_before,
        "GPU operation leaked heap storage"
    );
    if !handoff().disabled {
        assert_eq!(
            mm::user::free_frames(),
            frames_before,
            "GPU operation leaked DMA frames"
        );
        serial::write(format_args!(
            "[GPU:RECLAIM] frames_restored=true heap_restored=true\n"
        ));
    }
    result
}

struct Command {
    bytes: [u8; REQUEST],
    length: usize,
}
impl Command {
    fn new(kind: u32, context: u32, length: usize) -> Self {
        assert!((HEADER_BYTES..=REQUEST).contains(&length));
        let mut command = Self {
            bytes: [0; REQUEST],
            length,
        };
        command.word(0, kind);
        command.word(16, context);
        command
    }
    fn word(&mut self, at: usize, value: u32) {
        self.bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn wide(&mut self, at: usize, value: u64) {
        self.bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }
}

struct Gpu {
    config: VirtioConfig,
    timer: PmTimer,
    queue: Option<mm::DmaPage>,
    backing: [Option<mm::DmaPage>; BACKING_PAGES],
    notify: u64,
    available: u16,
    next_fence: u64,
    failed: bool,
    stopped: bool,
}
impl Gpu {
    fn new(config: VirtioConfig, timer: PmTimer) -> Result<Self, i64> {
        let mut device = Self {
            config,
            timer,
            queue: None,
            backing: core::array::from_fn(|_| None),
            notify: 0,
            available: 0,
            next_fence: 0,
            failed: false,
            stopped: false,
        };
        device.reset()?;
        device.write8(20, 1); // ACKNOWLEDGE.
        device.write8(20, 3); // DRIVER.
        device.write32(0, 0);
        let low = device.read32(4);
        device.write32(0, 1);
        let high = device.read32(4);
        if high & 1 == 0 || low & 1 == 0 {
            serial::write(format_args!(
                "[GPU:UNSUPPORTED] version1={} virgl={}\n",
                high & 1 != 0,
                low & 1 != 0
            ));
            return Err(UNSUPPORTED);
        }
        device.write32(8, 0);
        device.write32(12, 1); // VIRGL only; no blobs or unvalidated context protocols.
        device.write32(8, 1);
        device.write32(12, 1); // VERSION_1.
        device.write8(20, 11); // FEATURES_OK.
        if device.read8(20) & 8 == 0 {
            return Err(UNSUPPORTED);
        }
        device.write16(16, u16::MAX); // No configuration MSI-X vector.
        device.write16(22, 0); // controlq; the unused cursorq remains disabled.
        if device.read16(24) < QUEUE || device.read16(28) != 0 {
            return Err(UNSUPPORTED);
        }
        let offset = u64::from(device.read16(30))
            .checked_mul(u64::from(config.multiplier))
            .ok_or(INVALID_ARGUMENT)?;
        if !offset.is_multiple_of(2)
            || offset
                .checked_add(2)
                .is_none_or(|end| end > u64::from(config.notify_bytes))
        {
            return Err(INVALID_ARGUMENT);
        }
        device.notify = config.notify.checked_add(offset).ok_or(INVALID_ARGUMENT)?;
        let queue = mm::DmaPage::new()?;
        let physical = queue.physical();
        device.queue = Some(queue);
        device.write16(24, QUEUE);
        device.write16(26, u16::MAX); // No queue MSI-X vector.
        device.write_address(32, physical);
        device.write_address(40, physical + AVAIL as u64);
        device.write_address(48, physical + USED as u64);
        device.put16(AVAIL, 1); // VIRTQ_AVAIL_F_NO_INTERRUPT.
        config
            .pci
            .command((config.pci.read(4) as u16) | 6 | (1 << 10));
        device.write16(28, 1);
        device.write8(20, 15); // DRIVER_OK.
        if device.read8(20) != 15 {
            return Err(IO_ERROR);
        }
        device.capabilities()?;
        #[cfg(feature = "gpu-test")]
        device.reject_invalid_capset()?;
        Ok(device)
    }

    fn capabilities(&mut self) -> Result<(), i64> {
        let mut count = None;
        for _ in 0..32 {
            let generation = self.read8(21);
            // SAFETY: GPU PCI admission requires a device capability of at least 16 bytes.
            let value = unsafe { read_volatile((self.config.device + 12) as *const u32) };
            if generation == self.read8(21) {
                count = Some(value);
                break;
            }
        }
        let count = count.ok_or(IO_ERROR)?;
        if count == 0 || count > 8 {
            return Err(UNSUPPORTED);
        }
        let mut selected = None;
        for index in 0..count {
            let mut command = Command::new(0x0108, 0, 32); // GET_CAPSET_INFO.
            command.word(24, index);
            self.command(&mut command, OK_CAPSET_INFO, 40)?;
            let id = self.get32(RESPONSE + 24);
            let version = self.get32(RESPONSE + 28);
            let bytes = self.get32(RESPONSE + 32) as usize;
            if id == 2 && version >= 2 && (760..=RESPONSE_BYTES - HEADER_BYTES).contains(&bytes) {
                selected = Some((version.min(2), bytes));
                break;
            }
        }
        let (version, bytes) = selected.ok_or(UNSUPPORTED)?;
        let mut command = Command::new(0x0109, 0, 32); // GET_CAPSET, VIRGL2.
        command.word(24, 2);
        command.word(28, version);
        self.command(&mut command, OK_CAPSET, HEADER_BYTES + bytes)?;
        // VirGL caps v1 begins with max_version and four 16-word format masks.
        // The render mask is at byte 68; B8G8R8A8_UNORM is format 1.
        if self.get32(RESPONSE + HEADER_BYTES) < 2
            || self.get32(RESPONSE + HEADER_BYTES + 68) & 2 == 0
        {
            return Err(UNSUPPORTED);
        }
        // VirGL caps v2 appends fields, retaining this 64-byte GL_RENDERER field at byte 696.
        let mut renderer = [0_u8; 64];
        for (index, byte) in renderer.iter_mut().enumerate() {
            // SAFETY: The completed, length-checked capset contains all 760 required bytes.
            *byte = unsafe { read_volatile(self.pointer(RESPONSE + HEADER_BYTES + 696 + index)) };
        }
        let end = renderer
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(IO_ERROR)?;
        if end == 0 || !renderer[..end].iter().all(|byte| (32..=126).contains(byte)) {
            return Err(IO_ERROR);
        }
        let renderer = core::str::from_utf8(&renderer[..end]).map_err(|_| IO_ERROR)?;
        serial::write(format_args!("[GPU:RENDERER] name={renderer}\n"));
        serial::write(format_args!(
            "[GPU:READY] virtio=modern virgl=true queue=8 irq=poll\n"
        ));
        Ok(())
    }

    #[cfg(feature = "gpu-test")]
    fn reject_invalid_capset(&mut self) -> Result<(), i64> {
        let mut command = Command::new(0x0109, 0, 32); // Unknown GET_CAPSET must return an error.
        command.word(24, u32::MAX);
        command.word(28, 1);
        let (kind, bytes) = self.exchange(&mut command, HEADER_BYTES, true)?;
        if kind != 0x1205 || bytes != HEADER_BYTES {
            return Err(IO_ERROR);
        }
        serial::write(format_args!(
            "[GPU:REJECT] invalid_capset=true response=0x1205\n"
        ));
        Ok(())
    }

    fn render(&mut self, width: u32, height: u32, rgb: u32, output: &mut [u32]) -> Result<(), i64> {
        let mut command = Command::new(0x0200, 1, 96); // CTX_CREATE.
        command.word(24, 5);
        command.bytes[32..37].copy_from_slice(b"anova");
        self.command(&mut command, OK_NODATA, HEADER_BYTES)?;

        let mut command = Command::new(0x0204, 0, 72); // RESOURCE_CREATE_3D.
        for (at, value) in [
            (24, 1), // Resource id.
            (28, 2), // PIPE_TEXTURE_2D.
            (32, 1), // B8G8R8A8_UNORM.
            (36, 2), // PIPE_BIND_RENDER_TARGET.
            (40, width),
            (44, height),
            (48, 1), // Depth.
            (52, 1), // Array size.
        ] {
            command.word(at, value);
        }
        self.command(&mut command, OK_NODATA, HEADER_BYTES)?;

        let mut command = Command::new(0x0106, 0, 32 + BACKING_PAGES * 16); // ATTACH_BACKING.
        command.word(24, 1);
        command.word(28, BACKING_PAGES as u32);
        for index in 0..BACKING_PAGES {
            let page = mm::DmaPage::new()?;
            // A distinct poison in every pixel prevents a missing readback from matching a clear.
            for pixel in 0..1024 {
                let poison = (0xff00_0000 | rgb) ^ (0x0100_0001 + (index * 1024 + pixel) as u32);
                // SAFETY: Newly owned, aligned DMA page; it has not been exposed to the device.
                unsafe {
                    write_volatile(page.pointer().add(pixel * 4).cast::<u32>(), poison);
                }
            }
            command.wide(32 + index * 16, page.physical());
            command.word(40 + index * 16, 4096);
            self.backing[index] = Some(page);
        }
        self.command(&mut command, OK_NODATA, HEADER_BYTES)?;

        let mut command = Command::new(0x0202, 1, 32); // CTX_ATTACH_RESOURCE.
        command.word(24, 1);
        self.command(&mut command, OK_NODATA, HEADER_BYTES)?;

        let start = self.now();
        let stream = clear_stream(rgb);
        let mut command = Command::new(0x0207, 1, 32 + stream.len() * 4); // SUBMIT_3D.
        command.word(24, (stream.len() * 4) as u32);
        for (index, value) in stream.into_iter().enumerate() {
            command.word(32 + index * 4, value);
        }
        self.command(&mut command, OK_NODATA, HEADER_BYTES)?;

        let mut command = Command::new(0x0206, 1, 72); // TRANSFER_FROM_HOST_3D.
        command.word(36, width); // Box width, x/y/z are zero.
        command.word(40, height);
        command.word(44, 1); // Box depth.
        command.word(56, 1); // Resource id; level and byte offset remain zero.
        command.word(64, width * 4); // Tight row stride.
        command.word(68, width * height * 4); // Layer stride.
        self.command(&mut command, OK_NODATA, HEADER_BYTES)?;
        let elapsed = self.now().wrapping_sub(start) & self.timer.mask;

        for (index, pixel) in output.iter_mut().enumerate() {
            let page = self.backing[index / 1024].as_ref().ok_or(IO_ERROR)?;
            // SAFETY: Fenced transfer completed, acquire fence orders this aligned retained DMA
            // word. The checked dimensions bound the tightly packed output to four pages.
            let value =
                unsafe { read_volatile(page.pointer().add((index % 1024) * 4).cast::<u32>()) };
            if value != 0xff00_0000 | rgb {
                serial::write(format_args!(
                    "[GPU:MISMATCH] pixel={index} expected={:08x} actual={value:08x}\n",
                    0xff00_0000 | rgb
                ));
                return Err(IO_ERROR);
            }
            *pixel = value & 0x00ff_ffff;
        }
        serial::write(format_args!(
            "[GPU:READBACK] width={width} height={height} pixels={} rgb={rgb:08x} fence={} verified=true\n",
            output.len(),
            self.next_fence
        ));
        serial::write(format_args!(
            "[GPU:RENDER] commands=submit3d,transfer_from_host3d elapsed_pm_ticks={elapsed} dma_pages=5\n"
        ));
        Ok(())
    }

    fn command(&mut self, command: &mut Command, expected: u32, bytes: usize) -> Result<(), i64> {
        let (kind, returned) = self.exchange(command, bytes, true)?;
        if kind != expected || returned != bytes {
            serial::write(format_args!(
                "[GPU:RESPONSE] expected={expected:#06x} actual={kind:#06x} expected_bytes={bytes} actual_bytes={returned}\n"
            ));
            self.failed = true;
            return Err(IO_ERROR);
        }
        Ok(())
    }

    fn exchange(
        &mut self,
        command: &mut Command,
        bytes: usize,
        notify: bool,
    ) -> Result<(u32, usize), i64> {
        if self.failed || self.stopped || !(HEADER_BYTES..=RESPONSE_BYTES).contains(&bytes) {
            return Err(IO_ERROR);
        }
        self.next_fence = self.next_fence.checked_add(1).ok_or(IO_ERROR)?;
        command.word(4, 1); // VIRTIO_GPU_FLAG_FENCE.
        command.wide(8, self.next_fence);
        let physical = self.queue.as_ref().ok_or(IO_ERROR)?.physical();
        // SAFETY: Only one outstanding request; the previous fenced response completed before
        // these retained, disjoint request/response regions are overwritten by the BSP.
        unsafe {
            core::ptr::copy_nonoverlapping(
                command.bytes.as_ptr(),
                self.pointer(REQUEST),
                command.length,
            );
            self.pointer(RESPONSE).write_bytes(0, bytes);
        }
        self.descriptor(0, physical + REQUEST as u64, command.length as u32, 1, 1);
        self.descriptor(1, physical + RESPONSE as u64, bytes as u32, 2, 0);
        self.put16(AVAIL + 4 + usize::from(self.available % QUEUE) * 2, 0);
        fence(Ordering::SeqCst);
        self.available = self.available.wrapping_add(1);
        self.put16(AVAIL + 2, self.available);
        fence(Ordering::SeqCst);
        if notify {
            // SAFETY: PCI admission checked this aligned, writable queue notification register.
            unsafe { write_volatile(self.notify as *mut u16, 0) };
        }
        let start = self.now();
        let mut completed = false;
        for _ in 0..POLL_LIMIT {
            let used = self.get16(USED + 2);
            if used == self.available {
                completed = true;
                break;
            }
            if used != self.available.wrapping_sub(1) || self.read8(20) & (64 | 128) != 0 {
                self.failed = true;
                return Err(IO_ERROR);
            }
            if self.expired(start) {
                break;
            }
            core::hint::spin_loop();
        }
        if !completed {
            self.failed = true;
            let elapsed = self.now().wrapping_sub(start) & self.timer.mask;
            let reason = if elapsed >= DEADLINE_TICKS {
                "pm-timer"
            } else {
                "iteration-limit"
            };
            serial::write(format_args!(
                "[GPU:TIMEOUT] watchdog=pm-timer deadline_ms=2000 reset_required=true reason={reason} elapsed_pm_ticks={elapsed}\n"
            ));
            return Err(DEVICE_TIMEOUT);
        }
        fence(Ordering::SeqCst);
        let used_at = USED + 4 + usize::from(self.available.wrapping_sub(1) % QUEUE) * 8;
        let id = self.get32(used_at);
        let returned = self.get32(used_at + 4) as usize;
        if id != 0 || !(HEADER_BYTES..=bytes).contains(&returned) {
            self.failed = true;
            return Err(IO_ERROR);
        }
        let flags = self.get32(RESPONSE + 4);
        let fence_id =
            u64::from(self.get32(RESPONSE + 8)) | (u64::from(self.get32(RESPONSE + 12)) << 32);
        if flags != 1
            || fence_id != self.next_fence
            || self.get32(RESPONSE + 16)
                != u32::from_le_bytes(command.bytes[16..20].try_into().unwrap())
            || self.get32(RESPONSE + 20) != 0
        {
            self.failed = true;
            return Err(IO_ERROR);
        }
        Ok((self.get32(RESPONSE), returned))
    }

    fn now(&self) -> u32 {
        cpu::in32(self.timer.port) & self.timer.mask
    }
    fn expired(&self, start: u32) -> bool {
        self.now().wrapping_sub(start) & self.timer.mask >= DEADLINE_TICKS
    }
    fn reset(&self) -> Result<(), i64> {
        self.write8(20, 0);
        let start = self.now();
        for _ in 0..POLL_LIMIT {
            if self.read8(20) == 0 {
                fence(Ordering::SeqCst);
                return Ok(());
            }
            if self.expired(start) {
                break;
            }
            core::hint::spin_loop();
        }
        Err(DEVICE_TIMEOUT)
    }
    fn stop(&mut self) -> Result<(), i64> {
        if self.stopped {
            return Ok(());
        }
        let reset = self.reset();
        self.config
            .pci
            .command((self.config.pci.read(4) as u16) & !4);
        self.stopped = true;
        if reset.is_err() {
            disable();
            if let Some(page) = self.queue.take() {
                core::mem::forget(page);
            }
            for page in &mut self.backing {
                if let Some(page) = page.take() {
                    core::mem::forget(page);
                }
            }
            serial::emergency(format_args!(
                "[GPU:QUARANTINE] reset_acknowledged=false dma_retained=true device_disabled=true\n"
            ));
            return reset;
        }
        drop(self.queue.take());
        for page in &mut self.backing {
            drop(page.take());
        }
        serial::write(format_args!(
            "[GPU:RESET] acknowledged=true dma_released=true\n"
        ));
        Ok(())
    }

    fn pointer(&self, offset: usize) -> *mut u8 {
        assert!(offset < 4096, "GPU DMA offset outside owned page");
        self.queue
            .as_ref()
            .expect("Missing GPU queue page")
            .pointer()
            .wrapping_add(offset)
    }
    fn get16(&self, at: usize) -> u16 {
        // SAFETY: Call sites select aligned, bounded queue fields inside the retained DMA page.
        unsafe { read_volatile(self.pointer(at).cast::<u16>()) }
    }
    fn get32(&self, at: usize) -> u32 {
        // SAFETY: Call sites select aligned, bounded completed-response/used-ring fields.
        unsafe { read_volatile(self.pointer(at).cast::<u32>()) }
    }
    fn put16(&self, at: usize, value: u16) {
        // SAFETY: The BSP exclusively writes this aligned, bounded driver-owned DMA field.
        unsafe { write_volatile(self.pointer(at).cast::<u16>(), value) };
    }
    fn put32(&self, at: usize, value: u32) {
        // SAFETY: The BSP exclusively writes this aligned, bounded driver-owned DMA field.
        unsafe { write_volatile(self.pointer(at).cast::<u32>(), value) };
    }
    fn descriptor(&self, index: usize, address: u64, length: u32, flags: u16, next: u16) {
        let at = index * 16;
        self.put32(at, address as u32);
        self.put32(at + 4, (address >> 32) as u32);
        self.put32(at + 8, length);
        self.put16(at + 12, flags);
        self.put16(at + 14, next);
    }
    fn read8(&self, at: u64) -> u8 {
        // SAFETY: PCI admission checked the bounded UC/NX common capability, sole BSP owner.
        unsafe { read_volatile((self.config.common + at) as *const u8) }
    }
    fn read16(&self, at: u64) -> u16 {
        // SAFETY: Call sites select aligned fields within the validated 56-byte common capability.
        unsafe { read_volatile((self.config.common + at) as *const u16) }
    }
    fn read32(&self, at: u64) -> u32 {
        // SAFETY: Call sites select aligned fields within the validated common capability.
        unsafe { read_volatile((self.config.common + at) as *const u32) }
    }
    fn write8(&self, at: u64, value: u8) {
        // SAFETY: Validated common capability; sole BSP owner serializes device status writes.
        unsafe { write_volatile((self.config.common + at) as *mut u8, value) };
    }
    fn write16(&self, at: u64, value: u16) {
        // SAFETY: Validated aligned common register; no interrupt/AP accesses this device.
        unsafe { write_volatile((self.config.common + at) as *mut u16, value) };
    }
    fn write32(&self, at: u64, value: u32) {
        // SAFETY: Validated aligned common register; no interrupt/AP accesses this device.
        unsafe { write_volatile((self.config.common + at) as *mut u32, value) };
    }
    fn write_address(&self, at: u64, value: u64) {
        self.write32(at, value as u32);
        self.write32(at + 4, (value >> 32) as u32);
    }
}
impl Drop for Gpu {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
