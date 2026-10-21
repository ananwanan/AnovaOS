// SPDX-License-Identifier: Apache-2.0
//! Single-BSP COM1 output. No heap, lock, or firmware calls.
use core::{
    arch::asm,
    fmt::{self, Write},
};

const COM1: u16 = 0x3f8;

pub fn out8(port: u16, value: u8) {
    // SAFETY: The BSP runs at CPL0. Callers restrict ports to the q35 UART,
    // legacy PIC masks, and the test-only QEMU exit device.
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

fn in8(port: u16) -> u8 {
    let value;
    // SAFETY: CPL0 and exclusive BSP ownership of the q35 COM1 registers.
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    }
    value
}

pub fn init() {
    out8(COM1 + 1, 0);
    out8(COM1 + 3, 0x80);
    out8(COM1, 1);
    out8(COM1 + 1, 0);
    out8(COM1 + 3, 3);
    out8(COM1 + 2, 0xc7);
    out8(COM1 + 4, 0x0b);
}

struct EmergencyWriter;

impl Write for EmergencyWriter {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        for byte in value.bytes() {
            if byte == b'\n' {
                write_byte(b'\r');
            }
            write_byte(byte);
        }
        Ok(())
    }
}

fn write_byte(byte: u8) {
    // This is a bounded attempt budget, not a calibrated time interval. A dead
    // UART must not permanently trap the panic path. Dropped output fails smoke.
    for _ in 0..100_000 {
        if in8(COM1 + 5) & 0x20 != 0 {
            out8(COM1, byte);
            return;
        }
        core::hint::spin_loop();
    }
}

pub fn write(args: fmt::Arguments<'_>) {
    crate::arch::x86_64::cpu::without_interrupts(|| emergency(args));
}

/// Fatal exception/panic output is bounded and bypasses normal serialization.
/// IRQ handlers never log. An NMI/fatal exception can interrupt this writer;
/// interleaved output is preferable to waiting for a lock held by the victim.
pub fn emergency(args: fmt::Arguments<'_>) {
    let _ = EmergencyWriter.write_fmt(args);
}
