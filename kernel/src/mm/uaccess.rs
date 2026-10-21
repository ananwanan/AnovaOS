// SPDX-License-Identifier: Apache-2.0
//! Copy bounded user bytes after page-wise checks, recovering only at REP MOVSB.
//! IF=0 and scheduler-only map ownership pin the address space on the sole BSP.
//! No thread shares an address space and no copy spans a scheduling/blocking boundary.
use super::user::UserSpace;
use crate::arch::x86_64::{cpu, traps::TrapFrame, user};
use anova_abi::{BAD_ADDRESS, UserAddr};
use core::{
    arch::asm,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

static ARMED: AtomicBool = AtomicBool::new(false);
static START: AtomicU64 = AtomicU64::new(0);
static END: AtomicU64 = AtomicU64::new(0);
static WRITE: AtomicBool = AtomicBool::new(false);

fn raw(
    destination: *mut u8,
    source: *const u8,
    address: UserAddr,
    length: usize,
    write: bool,
) -> Result<(), i64> {
    assert_eq!(cpu::flags() & (1 << 9), 0, "Usercopy requires IF=0");
    if length == 0 {
        return Ok(());
    }
    assert!(!ARMED.load(Ordering::Relaxed), "Recursive usercopy");
    START.store(address.0, Ordering::Relaxed);
    END.store(
        address.checked_end(length).ok_or(BAD_ADDRESS)?,
        Ordering::Relaxed,
    );
    WRITE.store(write, Ordering::Relaxed);
    ARMED.store(true, Ordering::Release);
    // SAFETY: Caller owns the kernel slice, prevalidates each user page, and
    // keeps mappings stable with IF=0. The dedicated assembly fault site has
    // an exact #PF fixup; no Rust reference to user storage is constructed.
    let result = unsafe { user::anova_copy_user(destination, source, length) };
    ARMED.store(false, Ordering::Release);
    if result == 1 {
        Ok(())
    } else {
        Err(BAD_ADDRESS)
    }
}

pub fn from_user(space: &UserSpace, address: UserAddr, output: &mut [u8]) -> Result<(), i64> {
    if !space.accessible(address, output.len(), false, false) {
        return Err(BAD_ADDRESS);
    }
    raw(
        output.as_mut_ptr(),
        address.0 as *const u8,
        address,
        output.len(),
        false,
    )
}

pub fn to_user(space: &UserSpace, address: UserAddr, input: &[u8]) -> Result<(), i64> {
    if !space.accessible(address, input.len(), true, false) {
        return Err(BAD_ADDRESS);
    }
    raw(
        address.0 as *mut u8,
        input.as_ptr(),
        address,
        input.len(),
        true,
    )
}

pub fn recover(frame: &mut TrapFrame) -> bool {
    if frame.cs & 3 != 0
        || !ARMED.load(Ordering::Acquire)
        || frame.rip != user::anova_copy_user_fault as *const () as u64
        || frame.error & ((1 << 2) | (1 << 3) | (1 << 4)) != 0
        || (frame.error & 2 != 0) != WRITE.load(Ordering::Relaxed)
    {
        return false;
    }
    let address: u64;
    // SAFETY: CPL0 #PF entry captures CR2 before any diagnostic memory access.
    unsafe {
        asm!("mov {},cr2",out(reg) address,options(nomem,nostack,preserves_flags));
    }
    if !(START.load(Ordering::Relaxed)..END.load(Ordering::Relaxed)).contains(&address) {
        return false;
    }
    frame.rip = user::anova_copy_user_failed as *const () as u64;
    crate::serial::emergency(format_args!(
        "[USER:COPYFAULT] write={} address={address:#x} recovered=true\n",
        WRITE.load(Ordering::Relaxed)
    ));
    true
}

#[cfg(feature = "qemu-test")]
pub fn verify_recovery() {
    let mut byte = 0_u8;
    // Exercise the access-time fixup separately from preflight rejection. This
    // dedicated unmapped lower page models a revocation after a successful walk.
    assert_eq!(
        raw(&mut byte, 0x10000 as *const u8, UserAddr(0x10000), 1, false),
        Err(BAD_ADDRESS)
    );
    assert_eq!(
        raw(0x10000 as *mut u8, &byte, UserAddr(0x10000), 1, true),
        Err(BAD_ADDRESS)
    );
}
