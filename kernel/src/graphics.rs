// SPDX-License-Identifier: Apache-2.0
//! Present a bounded CPU test pattern through the validated, inherited WC video mapping.
//! The temporary back buffer is reclaimed before user startup; COM1 remains the recovery console.
use crate::{boot, mm, serial};
use alloc::vec::Vec;
use anova_graphics::Surface;
use core::arch::asm;

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
