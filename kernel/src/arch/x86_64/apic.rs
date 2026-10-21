// SPDX-License-Identifier: Apache-2.0
//! BSP xAPIC periodic timer, calibrated against the 3.579545 MHz ACPI PM timer.
//! External I/O APIC lines and legacy PICs remain masked until device drivers exist.
use super::{
    acpi::Platform,
    cpu,
    paging::{EarlyMapper, IOAPIC_WINDOW, LAPIC_WINDOW},
};
use crate::serial;
use anova_platform::{PM_HZ, PhysAddr, VirtAddr, acpi::PmTimer, pm_elapsed};
use core::{
    arch::x86_64::__cpuid,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};

pub const TICK_NS: u64 = 10_000_000;
static TICKS: AtomicU64 = AtomicU64::new(0);

fn read(offset: u64) -> u32 {
    // SAFETY: init maps this aligned architectural LAPIC page UC, NX, RW before
    // any caller runs. Offsets are fixed 16-byte-aligned xAPIC register offsets.
    unsafe { ptr::read_volatile((LAPIC_WINDOW + offset) as *const u32) }
}
fn write(offset: u64, value: u32) {
    // SAFETY: Same retained UC mapping as read; only the BSP owns this LAPIC.
    unsafe {
        ptr::write_volatile((LAPIC_WINDOW + offset) as *mut u32, value);
    }
}

fn io_read(base: u64, register: u32) -> u32 {
    // SAFETY: init maps the MADT I/O APIC page UC. The BSP accesses IOREGSEL
    // and IOWIN serially at IF=0; there are no concurrent register transactions.
    unsafe {
        ptr::write_volatile(base as *mut u32, register);
        ptr::read_volatile((base + 0x10) as *const u32)
    }
}
fn io_write(base: u64, register: u32, value: u32) {
    // SAFETY: Same serialized UC I/O APIC mapping as io_read.
    unsafe {
        ptr::write_volatile(base as *mut u32, register);
        ptr::write_volatile((base + 0x10) as *mut u32, value);
    }
}

fn pm_now(timer: PmTimer) -> u32 {
    cpu::in32(timer.port) & timer.mask
}

pub fn init(platform: &Platform, mapper: &mut EarlyMapper<'_>) {
    assert_eq!(cpu::flags() & (1 << 9), 0, "APIC setup requires IF=0");
    // SAFETY: CPUID leaf 1 exists on x86_64; no memory is touched.
    let features = unsafe { __cpuid(1) };
    assert!(features.edx & (1 << 9) != 0, "CPU lacks local APIC");
    let msr = cpu::read_msr(0x1b);
    assert_eq!(msr & (1 << 10), 0, "x2APIC mode is unsupported");
    assert_eq!(
        msr & 0x000f_ffff_ffff_f000,
        platform.madt.lapic,
        "MADT/MSR LAPIC mismatch"
    );
    mapper.mmio(PhysAddr(platform.madt.lapic), VirtAddr(LAPIC_WINDOW));
    cpu::write_msr(0x1b, msr | (1 << 11));
    let apic_id = read(0x20) >> 24;
    assert_eq!(apic_id, features.ebx >> 24, "BSP APIC identity mismatch");
    let max_lvt = (read(0x30) >> 16) & 0xff;
    assert!(max_lvt >= 5, "Insufficient local APIC LVT entries");
    // No legacy or external device IRQ is enabled by this milestone.
    serial::out8(0x21, 0xff);
    serial::out8(0xa1, 0xff);
    for offset in [0x320, 0x330, 0x340, 0x350, 0x360, 0x370] {
        write(offset, 1 << 16);
    }
    write(0x80, 0); // Accept all unmasked fixed interrupts.
    write(0xf0, (1 << 8) | 0xff); // Software enable, spurious vector 255.
    let mut gsi_end = 0;
    for (index, io) in platform.madt.ioapics.iter().flatten().enumerate() {
        let base = IOAPIC_WINDOW + index as u64 * 4096;
        mapper.mmio(PhysAddr(io.address), VirtAddr(base));
        assert_eq!(
            io_read(base, 0) >> 24,
            u32::from(io.id),
            "MADT/I/O APIC identity mismatch"
        );
        let count = ((io_read(base, 1) >> 16) & 0xff) + 1;
        assert!(
            count <= 120 && io.gsi_base >= gsi_end,
            "Unsupported or overlapping I/O APIC GSI range"
        );
        gsi_end = io.gsi_base.checked_add(count).expect("GSI range overflow");
        for pin in 0..count {
            // Mask before modifying destination to prevent transient routing.
            io_write(base, 0x10 + pin * 2, 1 << 16 | 0x50);
            io_write(base, 0x11 + pin * 2, apic_id << 24);
            assert!(
                io_read(base, 0x10 + pin * 2) & (1 << 16) != 0,
                "I/O APIC mask did not latch"
            );
        }
        serial::write(format_args!(
            "[CPU:IOAPIC] id={} gsi={}..{gsi_end} destination={apic_id} all_masked=true\n",
            io.id, io.gsi_base
        ));
    }
    // Divide by 16. Count for at least 20 ms of the independent PM clock.
    write(0x3e0, 3);
    write(0x320, (1 << 16) | 0x40);
    let start = pm_now(platform.pm_timer);
    write(0x380, u32::MAX);
    let target = (PM_HZ / 50) as u32;
    let mut elapsed = 0;
    // This is only a failure budget for a stuck device; elapsed time comes
    // from the PM hardware counter, never from the loop iteration count.
    for _ in 0..50_000_000 {
        elapsed = pm_elapsed(start, pm_now(platform.pm_timer), platform.pm_timer.mask);
        if elapsed >= target {
            break;
        }
        core::hint::spin_loop();
    }
    let apic_elapsed = u32::MAX - read(0x390);
    write(0x380, 0);
    assert!(
        elapsed >= target && elapsed < PM_HZ as u32,
        "PM timer stuck or calibration stalled"
    );
    assert!(
        apic_elapsed > 0 && apic_elapsed < u32::MAX / 2,
        "LAPIC timer failed calibration"
    );
    let initial = u64::from(apic_elapsed) * PM_HZ / u64::from(elapsed) / 100;
    assert!(
        initial > 0 && initial <= u64::from(u32::MAX),
        "Invalid calibrated LAPIC interval"
    );
    TICKS.store(0, Ordering::Relaxed);
    write(0x320, (1 << 17) | 0x40); // Periodic, unmasked, vector 64.
    write(0x380, initial as u32);
    serial::write(format_args!(
        "[CPU:APIC] id={apic_id} periodic_vector=64 divider=16 initial={initial} calibration_pm={elapsed} calibration_apic={apic_elapsed} tick_ns={TICK_NS}\n"
    ));
}

pub fn timer_irq() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    write(0xb0, 0); // Exactly one EOI for each delivered LAPIC timer interrupt.
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// A coarse monotonic IRQ-derived time, not wall time or a lossless clock.
pub fn monotonic_ns() -> u64 {
    ticks().saturating_mul(TICK_NS)
}

pub fn verify_ticks(timer: PmTimer) {
    let initial = ticks();
    let start = pm_now(timer);
    let mut previous = monotonic_ns();
    loop {
        // Check while IF=0 so a tick arriving just before HLT cannot be lost.
        cpu::disable_interrupts();
        let current = monotonic_ns();
        assert!(current >= previous, "Monotonic timer moved backwards");
        previous = current;
        if ticks() - initial >= 8 {
            break;
        }
        cpu::wait_for_interrupt();
    }
    let pm = pm_elapsed(start, pm_now(timer), timer.mask);
    assert!(
        pm >= (PM_HZ / 50) as u32 && pm <= PM_HZ as u32,
        "Timer cadence outside the calibration tolerance"
    );
    serial::write(format_args!(
        "[CPU:TIMER] ticks={} monotonic_ns={} pm_elapsed={pm} eoi=verified IF=0\n",
        ticks(),
        monotonic_ns()
    ));
}
