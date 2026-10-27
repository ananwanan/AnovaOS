// SPDX-License-Identifier: Apache-2.0
//! Experimental versioned direct-child supervision, separate from frozen native v1 records.
//! PIDs are never reused. Only a native caller's immediate children are eligible.
//! Live Linux processes are unsupported; exited children retain native wait status semantics.

pub const SYSCALL: u64 = 0x101;
pub const VERSION: u32 = 1;
pub const POLL: u32 = 0;
pub const STOP: u32 = 1;

/// RDI carries version:operation, RSI an exact nonzero PID, RDX the POLL status address.
/// POLL copies the existing 16-byte WaitStatus and reaps only after successful copying.
/// A live child returns WOULD_BLOCK without changing the caller's scheduling state.
/// STOP requires RDX=0, terminates with signal 15 and leaves a waitable status.
pub const fn command(operation: u32) -> u64 {
    ((VERSION as u64) << 32) | operation as u64
}

pub const fn decode(value: u64) -> Option<u32> {
    if value >> 32 == VERSION as u64 && value as u32 <= STOP {
        Some(value as u32)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_and_operation_are_checked_independently() {
        assert_eq!(decode(command(POLL)), Some(POLL));
        assert_eq!(decode(command(STOP)), Some(STOP));
        for value in [0, 1, 2_u64 << 32, command(2), u64::MAX] {
            assert_eq!(decode(value), None);
        }
    }
}
