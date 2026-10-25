// SPDX-License-Identifier: Apache-2.0
//! Present a bounded CPU test pattern through the validated, inherited WC video mapping.
//! Reclaim the boot buffer, then save/restore only the clipped diagnostic cursor rectangle.
//! All video access stays on the permanent kernel root; COM1 remains the recovery console.
//! M14.3 additionally copies validated compositor damage without exposing device addresses.
use crate::{boot, mm, serial};
use alloc::vec::Vec;
use anova_graphics::Surface;
use core::arch::asm;

#[cfg(feature = "gui")]
pub fn present_region(
    display: &boot::Display,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    pixels: &[u32],
) {
    for row in 0..height {
        for column in 0..width {
            let offset = display
                .layout
                .pixel_offset(x + column, y + row)
                .expect("Validated GUI rectangle");
            let pixel = display
                .layout
                .pack_rgb(pixels[(row * width + column) as usize]);
            for byte in 0..display.layout.bytes_per_pixel() {
                // SAFETY: GUI admission checks the reserved supervisor WC mapping;
                // the caller checked the complete rectangle and owns copied pixels.
                // Only the sole BSP scheduler writes here on its permanent kernel root.
                unsafe {
                    (display.address.0 as *mut u8)
                        .add(offset + byte)
                        .write_volatile((pixel >> (byte * 8)) as u8);
                }
            }
        }
    }
    Cursor::fence();
}

/// Fixed-size device-pixel save area: no retained full-screen allocation or user mapping.
pub struct Cursor {
    display: boot::Display,
    saved: [u32; (anova_graphics::CURSOR_WIDTH * anova_graphics::CURSOR_HEIGHT) as usize],
    position: Option<(u32, u32)>,
}

impl Cursor {
    pub fn new() -> Result<Self, &'static str> {
        let display = boot::display()?;
        mm::check_display(&display)?;
        Ok(Self {
            display,
            saved: [0; (anova_graphics::CURSOR_WIDTH * anova_graphics::CURSOR_HEIGHT) as usize],
            position: None,
        })
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.display.layout.width(), self.display.layout.height())
    }

    pub fn hide(&mut self) {
        if let Some((x, y)) = self.position.take() {
            self.visit(x, y, false);
            Self::fence();
        }
    }

    pub fn show(&mut self, x: u32, y: u32) {
        self.hide();
        self.visit(x, y, true);
        self.position = Some((x, y));
        Self::fence();
    }

    fn visit(&mut self, x: u32, y: u32, draw: bool) {
        let layout = self.display.layout;
        for cy in 0..anova_graphics::CURSOR_HEIGHT {
            for cx in 0..anova_graphics::CURSOR_WIDTH {
                let Some((px, py)) = x.checked_add(cx).zip(y.checked_add(cy)) else {
                    continue;
                };
                let Some(offset) = layout.pixel_offset(px, py) else {
                    continue;
                };
                let index = (cy * anova_graphics::CURSOR_WIDTH + cx) as usize;
                let mut packed = self.saved[index];
                if draw {
                    packed = 0;
                    for byte in 0..layout.bytes_per_pixel() {
                        // SAFETY: The checked visible pixel offset and admitted 24/32-bit
                        // layout bound each byte. The retained mapping is supervisor RW/NX
                        // WC, exclusively used on the BSP kernel root outside IRQ handlers.
                        packed |= u32::from(unsafe {
                            (self.display.address.0 as *const u8)
                                .add(offset + byte)
                                .read_volatile()
                        }) << (byte * 8);
                    }
                    self.saved[index] = packed;
                    if let Some(color) = anova_graphics::cursor_color(cx, cy) {
                        packed = layout.pack_rgb(color);
                    }
                }
                for byte in 0..layout.bytes_per_pixel() {
                    // SAFETY: Same checked, exclusively owned visible pixel as the read
                    // above. Restore saved device bytes exactly, including unused channels.
                    unsafe {
                        (self.display.address.0 as *mut u8)
                            .add(offset + byte)
                            .write_volatile((packed >> (byte * 8)) as u8);
                    }
                }
            }
        }
    }

    fn fence() {
        // SAFETY: x86-64 MFENCE orders WC stores before the next cursor/readback operation.
        unsafe { asm!("mfence", options(nostack, preserves_flags)) };
    }
}

pub fn show() {
    let before = mm::heap_used();
    if let Err(reason) = draw() {
        serial::write(format_args!(
            "[GRAPHICS:FALLBACK] reason={reason}; COM1 available\n"
        ));
    }
    assert_eq!(mm::heap_used(), before, "Graphics back buffer leaked");
    serial::write(format_args!("[GRAPHICS:RECLAIM] heap_restored=true\n"));
}

fn draw() -> Result<(), &'static str> {
    let display = boot::display()?;
    mm::check_display(&display)?;
    let layout = &display.layout;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(layout.pixel_count())
        .map_err(|_| "back-buffer-oom")?;
    pixels.resize(layout.pixel_count(), 0_u32);
    let mut surface = Surface::new(layout.width(), layout.height(), &mut pixels)
        .map_err(|_| "invalid-back-buffer")?;
    surface.draw_test_pattern();
    let pointer = display.address.0 as *mut u8;
    surface
        .present(layout, |offset, byte| {
            // SAFETY: Layout bounds each visible byte by its checked pitch*height;
            // display admission reserves all pages and check_display verifies their
            // supervisor RW/NX WC mappings. Only this BSP boot path writes video.
            // Byte stores allow packed 24-bit modes and unaligned row strides.
            unsafe { pointer.add(offset).write_volatile(byte) };
        })
        .map_err(|_| "present-layout-mismatch")?;
    // SAFETY: x86-64 always supplies MFENCE. Order WC stores before readback
    // and reporting completion; this is not a vblank or atomic scanout switch.
    unsafe { asm!("mfence", options(nostack, preserves_flags)) };
    let mut matches = true;
    surface
        .present(layout, |offset, expected| {
            // SAFETY: Same checked, exclusively presented span as the writes above;
            // each visible byte was initialized before this device readback.
            matches &= unsafe { pointer.add(offset).read_volatile() } == expected;
        })
        .map_err(|_| "readback-layout-mismatch")?;
    if !matches {
        return Err("video-readback-mismatch");
    }
    serial::write(format_args!(
        "[GRAPHICS:PRESENT] width={} height={} pitch={} bpp={} checksum={:#018x} readback=true cache=WC supervisor=true NX=true\n",
        layout.width(),
        layout.height(),
        layout.pitch(),
        display.bpp,
        surface.checksum()
    ));
    Ok(())
}
