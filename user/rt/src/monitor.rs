// SPDX-License-Identifier: Apache-2.0
//! Read bounded resource and owned task snapshots through the native monitor ABI.
//! A stop request leaves the exact child's wait record with its parent.

use crate::{
    abi::{INVALID_ARGUMENT, monitor::*},
    raw, result,
};

pub fn resources() -> Result<Resources, i64> {
    let mut output = Resources::default();
    // SAFETY: The initialized, writable record lives through the synchronous checked copy.
    result(unsafe {
        raw(
            SYSCALL,
            command(RESOURCES),
            core::ptr::from_mut(&mut output) as u64,
            RESOURCE_BYTES as u64,
        )
    })?;
    Ok(output)
}

pub fn tasks(output: &mut [Task; MAX_TASKS]) -> Result<&[Task], i64> {
    // SAFETY: The writable array lives through the synchronous checked copy.
    let count = result(unsafe {
        raw(
            SYSCALL,
            command(TASKS),
            output.as_mut_ptr() as u64,
            (MAX_TASKS * TASK_BYTES) as u64,
        )
    })?;
    output.get(..count as usize).ok_or(INVALID_ARGUMENT)
}

pub fn stop(pid: u64) -> Result<bool, i64> {
    // SAFETY: STOP uses scalar parameters. The kernel checks the exact PID and owner.
    result(unsafe { raw(SYSCALL, command(STOP), pid, 0) }).map(|exited| exited == 0)
}
