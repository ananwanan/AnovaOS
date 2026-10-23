// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Checked copied-message transport and serial formatting for experimental native GUI programs.
//! This is internal M14.3 plumbing, not the future public C/C++/Rust GUI SDK.

use anova_gui::{Reply, Request, transport as t};
use anova_rt as rt;
use core::fmt::{self, Write};

/// Invoke the experimental kernel boundary with raw user addresses.
/// # Safety
/// The selected operation's output range must be exclusively writable and input ranges
/// readable for the complete synchronous call; output must never alias a shared reference.
/// Deliberately invalid addresses are allowed only where the kernel guarantees rejection.
pub unsafe fn call(operation: u64, pointer: u64, length: usize) -> Result<u64, i64> {
    // SAFETY: The caller supplies the selected operation's address ownership contract.
    let value = unsafe { rt::raw(t::SYS_GUI, operation, pointer, length as u64) };
    if value < 0 {
        Err(value)
    } else {
        Ok(value as u64)
    }
}

pub fn scalar(operation: u64) -> Result<u64, i64> {
    // SAFETY: Scalar operations carry no address; pointer operations reject zero length/null.
    unsafe { call(operation, 0, 0) }
}

/// Pass an exclusively owned initialized integer-only wire layout to the kernel.
/// # Safety
/// T must be the exact transport layout selected by operation, permit every bit pattern,
/// and have all bytes initialized (including padding). Every output range selected by T
/// must be exclusively writable for the call; returned bytes must remain valid for T.
/// The kernel copies before returning and retains no caller pointer.
pub unsafe fn output<T>(operation: u64, value: &mut T) -> Result<(), i64> {
    // SAFETY: The caller guarantees T's layout and exclusive writable lifetime for operation.
    unsafe { call(operation, value as *mut T as u64, size_of::<T>()) }.map(|_| ())
}

/// Pass an initialized wire layout that the kernel only borrows for this syscall.
/// # Safety
/// The selected operation must only read T and every address nested in it; it must never
/// write through this shared reference or any nested shared reference. T must be its exact
/// wire layout with every byte initialized; nested ranges remain readable for the entire
/// synchronous call. The kernel retains no caller pointer.
pub unsafe fn input<T>(operation: u64, value: &T) -> Result<(), i64> {
    // SAFETY: The caller guarantees operation only reads the initialized live T layout.
    unsafe { call(operation, value as *const T as u64, size_of::<T>()) }.map(|_| ())
}

pub fn receive(envelope: &mut t::Envelope) -> Result<(), i64> {
    // SAFETY: Envelope has only initialized fixed-width integers and byte arrays, no padding.
    unsafe { output(t::RECEIVE, envelope) }
}

pub fn poll() -> Result<Reply, i64> {
    let mut bytes = [0; 64];
    // SAFETY: The reply layout is exactly 64 writable initialized bytes.
    unsafe { output(t::POLL, &mut bytes) }?;
    Reply::from_bytes(&bytes).map_err(|_| rt::abi::INVALID_ARGUMENT)
}

pub fn send(request: &Request, pixels: &[u32]) -> Result<(), i64> {
    let length = 64_usize
        .checked_add(pixels.len().checked_mul(4).ok_or(rt::abi::TOO_LONG)?)
        .ok_or(rt::abi::TOO_LONG)?;
    if length > t::PACKET_BYTES {
        return Err(rt::abi::TOO_LONG);
    }
    let mut bytes = [0; t::PACKET_BYTES];
    bytes[..64].copy_from_slice(&request.to_bytes());
    for (target, pixel) in bytes[64..length].chunks_exact_mut(4).zip(pixels) {
        target.copy_from_slice(&pixel.to_le_bytes());
    }
    // SAFETY: SEND reads only the live initialized prefix and retains no caller pointer.
    unsafe { call(t::SEND, bytes.as_ptr() as u64, length) }.map(|_| ())
}

pub fn deliver(client: u64, reply: &Reply) -> Result<(), i64> {
    let delivery = t::Delivery {
        client,
        payload: reply.to_bytes(),
    };
    // SAFETY: Delivery is the initialized 72-byte kernel reply layout with no padding.
    unsafe { input(t::REPLY, &delivery) }
}

pub fn connect() -> Result<u64, i64> {
    for _ in 0..100 {
        match scalar(t::CONNECT) {
            Err(rt::abi::NOT_FOUND | rt::abi::WOULD_BLOCK) => rt::sleep(1).unwrap(),
            result => return result,
        }
    }
    Err(rt::abi::NOT_FOUND)
}

struct Serial;
impl Write for Serial {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        rt::write_all(1, text.as_bytes()).map_err(|_| fmt::Error)
    }
}

pub fn log(arguments: fmt::Arguments<'_>) {
    let _ = Serial.write_fmt(arguments);
}
