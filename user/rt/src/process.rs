// SPDX-License-Identifier: Apache-2.0
//! Poll and stop exact immediate native children through the separate versioned supervisor API.

use crate::{abi, raw, result};

pub fn poll(pid: u64) -> Result<Option<abi::WaitStatus>, i64> {
    let mut status = abi::WaitStatus::default();
    // SAFETY: The exclusive initialized 16-byte output lives throughout the synchronous copy.
    let value = unsafe {
        raw(
            abi::process::SYSCALL,
            abi::process::command(abi::process::POLL),
            pid,
            &mut status as *mut _ as u64,
        )
    };
    if value == abi::WOULD_BLOCK {
        return Ok(None);
    }
    result(value)?;
    Ok(Some(status))
}

pub fn stop(pid: u64) -> Result<(), i64> {
    // SAFETY: All arguments are scalar; kernel checks exact direct-child ownership.
    result(unsafe {
        raw(
            abi::process::SYSCALL,
            abi::process::command(abi::process::STOP),
            pid,
            0,
        )
    })
    .map(|_| ())
}
