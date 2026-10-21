// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Independent Rust ELF showing argc/argv, BSS and CPU state through native syscalls.
use anova_rt::{self as rt, Args};
use core::{
    panic::PanicInfo,
    sync::atomic::{AtomicU64, Ordering},
};
static BSS: AtomicU64 = AtomicU64::new(0);

#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(argc: usize, argv: *const *const u8) -> u32 {
    assert!(rt::check_cpu(), "User CPU baseline failed");
    assert_eq!(BSS.load(Ordering::Relaxed), 0, "BSS not zeroed");
    BSS.store(42, Ordering::Relaxed);
    assert_eq!(rt::info().unwrap().abi_version, rt::abi::VERSION);
    rt::write(b"[APP:RUST] hello from Ring 3; args:");
    // SAFETY: The native CRT passes the loader's bounded immutable startup block.
    for arg in unsafe { Args::from_raw(argc, argv) } {
        rt::write(b" ");
        rt::write(arg);
    }
    rt::write(b"; BSS=zero x87/SSE2=ok\n");
    0
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    rt::write(b"[APP:FAIL] Rust hello\n");
    rt::exit(101)
}
