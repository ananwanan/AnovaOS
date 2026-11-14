// SPDX-License-Identifier: Apache-2.0
//! Explicit regular-file no-clobber rename and replacement through experimental files v1.
//! Data is shared with Linux processes; publication does not imply storage synchronization.

use crate::{abi::files::*, raw, result};

/// Rename a regular file only when the destination does not exist.
pub fn rename(from: &str, to: &str) -> Result<(), i64> {
    call(RENAME, from, to)
}

/// Explicitly allow replacement of an existing regular file; sync separately for persistence.
pub fn replace(from: &str, to: &str) -> Result<(), i64> {
    call(REPLACE, from, to)
}

fn call(operation: u32, from: &str, to: &str) -> Result<(), i64> {
    let bytes = Request {
        from: from.as_ptr() as u64,
        from_length: from.len() as u64,
        to: to.as_ptr() as u64,
        to_length: to.len() as u64,
        reserved: 0,
    }
    .bytes();
    // SAFETY: The complete initialized request and both immutable path slices stay
    // readable until the call returns; the kernel copies every input before mutation.
    result(unsafe {
        raw(
            SYSCALL,
            command(operation),
            bytes.as_ptr() as u64,
            bytes.len() as u64,
        )
    })
    .map(|_| ())
}
