// SPDX-License-Identifier: Apache-2.0
//! Experimental native PTY sessions, separate from frozen v1 and Linux syscall numbers.
//! Each monotonically identified session belongs to its exact native creator. Its master
//! is private kernel state; forked processes receive neither the handle nor its descriptors.

pub const SYSCALL: u64 = 0x102;
pub const VERSION: u32 = 1;
pub const START: u32 = 0;
pub const READ: u32 = 1;
pub const WRITE: u32 = 2;
pub const RESIZE: u32 = 3;
pub const STATUS: u32 = 4;
pub const CLOSE: u32 = 5;
pub const MAX_TRANSFER: usize = 1024;

/// RDI is version:operation; RSI is the session (zero for START); RDX points to exactly
/// 32 bytes below. Every operation validates the full record before changing any state.
/// START returns a shell PID/session, READ/WRITE return partial byte counts or WOULD_BLOCK,
/// STATUS copies an unreaped native WaitStatus (WOULD_BLOCK while live), CLOSE kills and
/// reaps every tagged descendant and closes the master. Creator death performs CLOSE.
/// READ drains buffered output before EOF (zero). stdout/stderr share the same slave.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Request {
    pub buffer: u64,
    pub length: u64,
    pub rows: u32,
    pub columns: u32,
    pub reserved: u64,
}

impl Request {
    pub fn bytes(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        bytes[..8].copy_from_slice(&self.buffer.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.length.to_le_bytes());
        bytes[16..20].copy_from_slice(&self.rows.to_le_bytes());
        bytes[20..24].copy_from_slice(&self.columns.to_le_bytes());
        bytes[24..].copy_from_slice(&self.reserved.to_le_bytes());
        bytes
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self {
            buffer: u64::from_le_bytes(bytes[..8].try_into().unwrap()),
            length: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            rows: u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
            columns: u32::from_le_bytes(bytes[20..24].try_into().unwrap()),
            reserved: u64::from_le_bytes(bytes[24..].try_into().unwrap()),
        }
    }

    pub fn valid(self, operation: u32) -> bool {
        if self.reserved != 0 {
            return false;
        }
        match operation {
            START | RESIZE => {
                self.buffer == 0
                    && self.length == 0
                    && (1..=128).contains(&self.rows)
                    && (1..=256).contains(&self.columns)
            }
            READ | WRITE => {
                self.rows == 0 && self.columns == 0 && self.length <= MAX_TRANSFER as u64
            }
            STATUS => self.rows == 0 && self.columns == 0 && self.length == 16,
            CLOSE => self.buffer == 0 && self.length == 0 && self.rows == 0 && self.columns == 0,
            _ => false,
        }
    }
}

pub const fn command(operation: u32) -> u64 {
    ((VERSION as u64) << 32) | operation as u64
}

pub const fn decode(value: u64) -> Option<u32> {
    if value >> 32 == VERSION as u64 && value as u32 <= CLOSE {
        Some(value as u32)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_fields_and_operation_specific_bounds_are_checked() {
        assert_eq!(size_of::<Request>(), 32);
        let r = Request {
            rows: 26,
            columns: 52,
            ..Request::default()
        };
        assert!(Request::from_bytes(r.bytes()).valid(START));
        assert!(!r.valid(READ));
        for r in [
            Request { rows: 0, ..r },
            Request { columns: 257, ..r },
            Request { reserved: 1, ..r },
            Request { buffer: 1, ..r },
        ] {
            assert!(!r.valid(START));
        }
        assert!(
            Request {
                length: 1024,
                ..Request::default()
            }
            .valid(READ)
        );
        assert!(
            !Request {
                length: 1025,
                ..Request::default()
            }
            .valid(READ)
        );
        assert_eq!(decode(command(CLOSE)), Some(CLOSE));
        assert_eq!(decode(command(CLOSE + 1)), None);
        assert_eq!(decode(0), None);
    }
}
