// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]

mod boot;
mod serial;

use core::{
    arch::asm,
    panic::PanicInfo,
    sync::atomic::{AtomicU8, Ordering},
};

#[cfg(all(feature = "test-panic", feature = "test-timeout"))]
compile_error!("Select only one fault injection feature");

static PHASE: AtomicU8 = AtomicU8::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // SAFETY: Limine enters the single BSP at CPL0 with interrupts disabled.
    // Keep them disabled until M2 installs an IDT and interrupt controllers.
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

    if cfg!(feature = "test-panic") {
        panic!("Controlled M1 panic injection");
    }
    if cfg!(feature = "test-timeout") {
        serial::write(format_args!(
            "[TEST:HANG] Controlled M1 timeout injection\n"
        ));
        halt();
    }

    PHASE.store(3, Ordering::Relaxed);
    serial::write(format_args!(
        "[BOOT:OK] M1 complete; BSP idle, interrupts disabled\n"
    ));
    #[cfg(feature = "qemu-test")]
    {
        serial::write(format_args!("[TEST:PASS]\n"));
        serial::out8(0xf4, 0x10);
    }
    halt()
}

fn halt() -> ! {
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
    // This path creates its own lock-free writer and never waits for another
    // logger. M1 has one BSP and no interrupt or AP logging.
    serial::write(format_args!(
        "\n[PANIC] phase={} {info}\n",
        PHASE.load(Ordering::Relaxed)
    ));
    #[cfg(feature = "qemu-test")]
    serial::out8(0xf4, 0x11);
    halt()
}
