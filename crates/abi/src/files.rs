// SPDX-License-Identifier: Apache-2.0
//! Experimental regular-file publication, separate from the frozen native v1 ABI.
//! Native and Linux processes use the same VFS; this extension does not change executable admission.

pub const SYSCALL: u64 = 0x103;
pub const VERSION: u32 = 1;
pub const RENAME: u32 = 0;
pub const REPLACE: u32 = 1;
pub const MAX_PATH: usize = 128;

/// RDI carries version:operation, RSI addresses this exact 40-byte record, RDX is 40.
/// Paths are nonempty UTF-8 without NUL, at most MAX_PATH bytes each. All inputs are
/// copied and checked before mutation. RENAME rejects an existing destination;
/// REPLACE explicitly permits replacing a regular file. Links/directories are rejected.
/// Neither operation implies a flush; callers must sync before declaring persistence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Request {
    pub from: u64,
    pub from_length: u64,
    pub to: u64,
    pub to_length: u64,
    pub reserved: u64,
}

impl Request {
    pub fn bytes(self) -> [u8; 40] {
        let mut bytes = [0; 40];
        for (output, value) in bytes.chunks_exact_mut(8).zip([
            self.from,
            self.from_length,
            self.to,
            self.to_length,
            self.reserved,
        ]) {
            output.copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    pub fn from_bytes(bytes: [u8; 40]) -> Self {
        Self {
            from: u64::from_le_bytes(bytes[..8].try_into().unwrap()),
            from_length: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            to: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            to_length: u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
            reserved: u64::from_le_bytes(bytes[32..].try_into().unwrap()),
        }
    }

    pub fn valid(self) -> bool {
        self.reserved == 0
            && (1..=MAX_PATH as u64).contains(&self.from_length)
            && (1..=MAX_PATH as u64).contains(&self.to_length)
    }
}

pub const fn command(operation: u32) -> u64 {
    ((VERSION as u64) << 32) | operation as u64
}

pub const fn decode(value: u64) -> Option<u32> {
    if value >> 32 == VERSION as u64 && value as u32 <= REPLACE {
        Some(value as u32)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_wire_checks_versions_reserved_and_both_path_bounds() {
        assert_eq!(size_of::<Request>(), 40);
        assert_eq!(align_of::<Request>(), 8);
        assert_eq!(decode(command(RENAME)), Some(RENAME));
        assert_eq!(decode(command(REPLACE)), Some(REPLACE));
        for value in [0, 1, command(2), 2_u64 << 32, u64::MAX] {
            assert_eq!(decode(value), None);
        }
        let request = Request {
            from: 0x1020304050607080,
            from_length: 128,
            to: 0x8877665544332211,
            to_length: 1,
            reserved: 0,
        };
        assert_eq!(Request::from_bytes(request.bytes()), request);
        assert!(request.valid());
        for request in [
            Request {
                reserved: 1,
                ..request
            },
            Request {
                from_length: 0,
                ..request
            },
            Request {
                to_length: 0,
                ..request
            },
            Request {
                from_length: 129,
                ..request
            },
            Request {
                to_length: 129,
                ..request
            },
        ] {
            assert!(!request.valid());
        }
    }
}
