// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Native Rust application using the typed v1 SDK and a locked local offline dependency.
use anova_rt::{self as rt, v1};

#[unsafe(no_mangle)]
extern "C" fn anova_main(_argc: usize, _argv: *const *const u8) -> u32 {
    if v1::info().is_err() {
        return 1;
    }
    if v1::write_all(1, offline_message::message()).is_err() {
        return 2;
    }
    match v1::open("/m13-missing", rt::abi::OPEN_READ) {
        Err(error) if error == v1::Error::NOT_FOUND => {
            let _ = v1::write_all(1, error.name().as_bytes());
            let _ = v1::write_all(1, b"\n");
            0
        }
        _ => 3,
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    let _ = v1::write_all(2, b"[M13:RUST] panic\n");
    rt::exit(101)
}
