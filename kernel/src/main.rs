// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Boot CPU/memory/PCI ownership and run native/Linux workloads with persistent storage and a shell.

extern crate alloc;

mod abi;
mod arch;
mod boot;
mod drivers;
mod fs;
mod mm;
mod serial;
mod task;
use arch::x86_64::{acpi, apic, cpu, paging::EarlyMapper, traps};

use core::{
    arch::asm,
    panic::PanicInfo,
    sync::atomic::{AtomicU8, Ordering},
};

const _: () = assert!(
    cfg!(feature = "test-panic") as u8
        + cfg!(feature = "test-timeout") as u8
        + cfg!(feature = "test-invalid-opcode") as u8
        + cfg!(feature = "test-general-protection") as u8
        + cfg!(feature = "test-page-fault") as u8
        + cfg!(feature = "test-double-fault") as u8
        + cfg!(feature = "test-stack-guard") as u8
        + cfg!(feature = "test-read-only") as u8
        + cfg!(feature = "test-no-execute") as u8
        + cfg!(feature = "test-direct-no-execute") as u8
        + cfg!(feature = "test-unmapped") as u8
        + cfg!(feature = "test-oom") as u8
        <= 1,
    "Select only one fault injection feature"
);

static PHASE: AtomicU8 = AtomicU8::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // SAFETY: Limine enters the single BSP at CPL0 with interrupts disabled.
    // Keep them disabled until our IDT and interrupt controllers are ready.
    unsafe {
        asm!("cli", options(nomem, nostack));
    }
    serial::init();
    PHASE.store(1, Ordering::Relaxed);
    serial::write(format_args!(
        "[BOOT:ENTRY] AnovaOS {} arch=x86_64\n",
        env!("CARGO_PKG_VERSION")
    ));
    let info = boot::validate();
    PHASE.store(2, Ordering::Relaxed);
    info.report();
    let mut mapper = EarlyMapper::new(&info);
    traps::init(&mut mapper);
    PHASE.store(3, Ordering::Relaxed);
    traps::breakpoint_test();
    traps::inject_fault();

    if cfg!(feature = "test-panic") {
        panic!("Controlled M1 panic injection");
    }
    if cfg!(feature = "test-timeout") {
        serial::write(format_args!(
            "[TEST:HANG] Controlled M1 timeout injection\n"
        ));
        halt();
    }

    let platform = acpi::discover(&info, &mut mapper);
    apic::init(&platform, &mut mapper);
    drivers::prepare(&mut mapper, platform.pm_timer);
    PHASE.store(4, Ordering::Relaxed);
    let stack_top = mm::init(&info, mapper);
    // SAFETY: init returned the new owned, aligned stack. IF is still clear;
    // only copied timer values cross the boundary and no boot local is reused.
    unsafe {
        mm::enter_stack(
            stack_top,
            u64::from(platform.pm_timer.port),
            u64::from(platform.pm_timer.mask),
        )
    }
}

extern "C" fn kernel_main(timer_port: u64, timer_mask: u64) -> ! {
    PHASE.store(5, Ordering::Relaxed);
    mm::verify();
    apic::verify_ticks(anova_platform::acpi::PmTimer {
        port: timer_port as u16,
        mask: timer_mask as u32,
    });
    // Check that an exception also returns with IF restored when interrupted
    // code has IRQs enabled. The timer handler itself never logs or allocates.
    // SAFETY: GDT/TSS/IDT/APIC are installed and all external IRQs stay masked.
    unsafe {
        asm!("sti", options(nomem, nostack));
    }
    traps::breakpoint_test();
    assert!(cpu::flags() & (1 << 9) != 0, "IRET failed to restore IF");
    PHASE.store(6, Ordering::Relaxed);
    mm::inject_fault();
    task::start();
    serial::write(format_args!(
        "[BOOT:OK] M9 native/Linux runtime processes reclaimed; BSP idle, interrupts enabled\n"
    ));
    #[cfg(feature = "qemu-test")]
    {
        serial::write(format_args!("[TEST:PASS]\n"));
        serial::out8(0xf4, 0x10);
    }
    loop {
        cpu::wait_for_interrupt();
    }
}

pub(crate) fn halt() -> ! {
    loop {
        // SAFETY: CPL0; no interrupts, allocation, or boot memory reclamation
        // occurs. A test timeout is ended by the host watchdog.
        unsafe {
            asm!("cli", "hlt", options(nomem, nostack));
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    cpu::disable_interrupts();
    // The fatal path bypasses normal output and never waits for a logging lock.
    serial::emergency(format_args!(
        "\n[PANIC] phase={} {info}\n",
        PHASE.load(Ordering::Relaxed)
    ));
    #[cfg(feature = "qemu-test")]
    serial::out8(0xf4, 0x11);
    halt()
}
