// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Link guest-compiled C/C++/Rust client objects against the Rust GUI runtime and native CRT.
//! Supply compiler intrinsics and a terminating panic path without libc or a native std port.
extern crate anova_gui_runtime;

#[path = "rt/src/memory.rs"]
mod memory;

core::arch::global_asm!(include_str!("c/crt.S"), options(att_syntax));

// A Rust reference admits the runtime archive before resolving the C client's exported main.
#[used]
static RUNTIME: unsafe extern "C" fn(u32, *mut anova_gui_runtime::Version) -> i32 =
    anova_gui_runtime::anova_gui_query_version;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    // SAFETY: Native EXIT takes a scalar and terminates this process; no pointers cross it.
    unsafe {
        core::arch::asm!("syscall", in("rax") 1u64, in("rdi") 101u64,
            lateout("rcx") _, lateout("r11") _, options(nostack));
        core::arch::asm!("ud2", options(noreturn));
    }
}
