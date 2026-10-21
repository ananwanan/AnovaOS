// SPDX-License-Identifier: Apache-2.0
//! Supply freestanding compiler memory intrinsics without linking host libc.
use core::arch::asm;

/// Fill a live writable range and return its original pointer.
/// # Safety
/// destination is valid for length writable bytes and obeys C aliasing rules.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(destination: *mut u8, value: i32, length: usize) -> *mut u8 {
    // SAFETY: The caller owns the complete range; the user ABI requires DF=0.
    unsafe {
        asm!("rep stosb",inout("rdi") destination=>_,inout("rcx") length=>_,in("al") value as u8,options(nostack));
    }
    destination
}
/// Copy non-overlapping byte ranges and return the destination.
/// # Safety
/// Both ranges are live for length bytes, destination writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(destination: *mut u8, source: *const u8, length: usize) -> *mut u8 {
    // SAFETY: Caller provides valid, non-overlapping ranges; DF=0 at C ABI calls.
    unsafe {
        asm!("rep movsb",inout("rdi") destination=>_,inout("rsi") source=>_,inout("rcx") length=>_,options(nostack));
    }
    destination
}
/// Copy possibly overlapping byte ranges and return the destination.
/// # Safety
/// Both ranges are live for length bytes, and destination is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(
    destination: *mut u8,
    source: *const u8,
    length: usize,
) -> *mut u8 {
    if length == 0 {
        return destination;
    }
    // SAFETY: Caller provides the ranges; copy backwards when destination is
    // above source, then restore DF=0 before returning to compiler-generated code.
    unsafe {
        if destination as usize > source as usize {
            asm!("std","rep movsb","cld",inout("rdi") destination.add(length-1)=>_,
                inout("rsi") source.add(length-1)=>_,inout("rcx") length=>_,options(nostack));
        } else {
            memcpy(destination, source, length);
        }
    }
    destination
}

/// Compare live ranges and return the signed difference at the first mismatch.
/// # Safety
/// Both inputs are readable for length bytes; overlapping immutable ranges are allowed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(left: *const u8, right: *const u8, length: usize) -> i32 {
    for i in 0..length {
        // SAFETY: The caller supplies both ranges. Volatile byte loads keep the
        // compiler from replacing this freestanding primitive with a recursive libc call.
        let (a, b) = unsafe { (left.add(i).read_volatile(), right.add(i).read_volatile()) };
        if a != b {
            return i32::from(a) - i32::from(b);
        }
    }
    0
}
/// Equality comparison emitted by LLVM for slice/string comparisons.
/// # Safety
/// Both inputs are readable for length bytes, as required by memcmp.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bcmp(left: *const u8, right: *const u8, length: usize) -> i32 {
    // SAFETY: Identical readable-range contract; only zero/nonzero is significant.
    unsafe { memcmp(left, right, length) }
}
