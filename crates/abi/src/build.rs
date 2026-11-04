// SPDX-License-Identifier: Apache-2.0
//! Experimental controlled GCC jobs, independent of native v1, GUI and interactive PTY v1.
//! One globally serialized job owns the reserved staging path until CLOSE/creator death.

pub const SYSCALL: u64 = 0x104;
pub const VERSION: u32 = 1;
pub const START: u32 = 0;
pub const READ: u32 = 1;
pub const STATUS: u32 = 2;
pub const CLOSE: u32 = 3;
pub const SOURCE: &str = "/persist/build.c";
pub const STAGING: &str = "/persist/build-app.next";
pub const OUTPUT: &str = "/persist/build-app";
pub const COMMAND: &str =
    "/bin/gcc -fdiagnostics-color=never /persist/build.c -o /persist/build-app.next";

/// RDI is version:operation, RSI is the exact job PID (zero for START), RDX points
/// to this fully copied 32-byte little-endian record. STATUS copies WaitStatus
/// without consuming it; READ retains bytes after failed copy. CLOSE invalidates
/// identity, closes the private master and reaps the entire tagged family.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Request {
    pub buffer: u64,
    pub length: u64,
    pub reserved: [u64; 2],
}

impl Request {
    pub fn bytes(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        for (chunk, value) in bytes.chunks_exact_mut(8).zip([
            self.buffer,
            self.length,
            self.reserved[0],
            self.reserved[1],
        ]) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        let mut values = [0; 4];
        for (value, chunk) in values.iter_mut().zip(bytes.chunks_exact(8)) {
            *value = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        Self {
            buffer: values[0],
            length: values[1],
            reserved: [values[2], values[3]],
        }
    }
    pub fn valid(self, operation: u32) -> bool {
        self.reserved == [0; 2]
            && match operation {
                START | CLOSE => self.buffer == 0 && self.length == 0,
                READ => self.length <= super::pty::MAX_TRANSFER as u64,
                STATUS => self.length == 16,
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
    fn layout_version_and_all_operation_bounds() {
        assert_eq!(size_of::<Request>(), 32);
        assert_eq!(align_of::<Request>(), 8);
        assert_eq!(core::mem::offset_of!(Request, reserved), 16);
        for op in START..=CLOSE {
            assert_eq!(decode(command(op)), Some(op));
            let r = Request {
                buffer: if op == READ || op == STATUS {
                    0x1234
                } else {
                    0
                },
                length: if op == STATUS { 16 } else { 0 },
                reserved: [0; 2],
            };
            assert!(Request::from_bytes(r.bytes()).valid(op));
            assert!(
                !Request {
                    reserved: [0, 1],
                    ..r
                }
                .valid(op)
            );
        }
        assert_eq!(decode(command(CLOSE + 1)), None);
        assert_eq!(decode(START as u64), None);
        assert_eq!(decode(2 << 32), None);
        assert!(
            !Request {
                length: 1025,
                ..Request::default()
            }
            .valid(READ)
        );
        assert!(
            !Request {
                length: 15,
                ..Request::default()
            }
            .valid(STATUS)
        );
        assert!(
            !Request {
                buffer: 1,
                ..Request::default()
            }
            .valid(START)
        );
    }
}
