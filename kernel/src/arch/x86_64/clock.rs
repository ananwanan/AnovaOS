// SPDX-License-Identifier: Apache-2.0
//! Sample the QEMU UTC CMOS RTC once, then advance realtime using the calibrated monotonic timer.
//! Invalid or unstable RTC data leaves wall time unavailable; TLS must fail rather than ignore dates.
use crate::{
    arch::x86_64::{apic, cpu},
    serial,
};
use core::{
    arch::asm,
    sync::atomic::{AtomicU64, Ordering},
};
static EPOCH: AtomicU64 = AtomicU64::new(0);
static BASE: AtomicU64 = AtomicU64::new(0);
fn read(register: u8) -> u8 {
    // SAFETY: The sole BSP owns CMOS access at IF=0. NMI remains enabled as established at boot.
    unsafe {
        asm!("out dx,al",in("dx")0x70u16,in("al")register,options(nomem,nostack,preserves_flags));
        let value: u8;
        asm!("in al,dx",in("dx")0x71u16,out("al")value,options(nomem,nostack,preserves_flags));
        value
    }
}
fn sample() -> Option<[u8; 7]> {
    if read(0x0a) & 0x80 != 0 {
        return None;
    }
    let values = [
        read(0x09),
        read(0x08),
        read(0x07),
        read(0x04),
        read(0x02),
        read(0x00),
        read(0x0b),
    ];
    if read(0x0a) & 0x80 != 0 {
        None
    } else {
        Some(values)
    }
}
pub fn init() {
    cpu::without_interrupts(|| {
        for _ in 0..100000 {
            let Some(mut a) = sample() else {
                continue;
            };
            if sample() != Some(a) {
                continue;
            }
            let mode = a[6];
            let pm = a[3] & 0x80 != 0;
            a[3] &= 0x7f;
            if mode & 4 == 0 {
                for n in &mut a[..6] {
                    if *n & 15 > 9 || *n >> 4 > 9 {
                        return;
                    }
                    *n = (*n >> 4) * 10 + (*n & 15);
                }
            }
            if mode & 2 == 0 {
                if !(1..=12).contains(&a[3]) {
                    return;
                }
                a[3] = a[3] % 12 + if pm { 12 } else { 0 };
            }
            if let Some(epoch) = anova_platform::clock::unix_seconds(
                2000 + u16::from(a[0]),
                a[1],
                a[2],
                a[3],
                a[4],
                a[5],
            ) {
                BASE.store(apic::ticks(), Ordering::Relaxed);
                EPOCH.store(epoch, Ordering::Relaxed);
                serial::write(format_args!(
                    "[CLOCK:UTC] source=CMOS epoch={epoch} monotonic=LAPIC\n"
                ));
            }
            return;
        }
    });
}
pub fn realtime() -> Option<(u64, u64)> {
    let epoch = EPOCH.load(Ordering::Relaxed);
    if epoch == 0 {
        None
    } else {
        let elapsed = apic::ticks().saturating_sub(BASE.load(Ordering::Relaxed));
        Some((epoch + elapsed / 100, (elapsed % 100) * 10_000_000))
    }
}
