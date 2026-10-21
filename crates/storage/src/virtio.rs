// SPDX-License-Identifier: Apache-2.0
//! Define modern virtio-blk feature gates and split-virtqueue wire layouts.
//! The hardware transport must retain pinned DMA buffers until a matching used descriptor
//! is observed; none of these structures alone performs a PCI or block I/O transaction.
use crate::Error;

pub const VERSION_1: u64 = 1 << 32;
pub const FLUSH: u64 = 1 << 9;
pub const RING_INDIRECT: u64 = 1 << 28;
pub const RING_EVENT_IDX: u64 = 1 << 29;
pub const BLK_READ: u32 = 0;
pub const BLK_WRITE: u32 = 1;
pub const BLK_FLUSH: u32 = 4;
pub const STATUS_OK: u8 = 0;
pub const STATUS_IO_ERROR: u8 = 1;
pub const STATUS_UNSUPPORTED: u8 = 2;

pub const ACKNOWLEDGE: u8 = 1;
pub const DRIVER: u8 = 2;
pub const DRIVER_OK: u8 = 4;
pub const FEATURES_OK: u8 = 8;
pub const FAILED: u8 = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Negotiated {
    pub features: u64,
    pub flush: bool,
}

pub fn negotiate(offered: u64) -> Result<Negotiated, Error> {
    if offered & VERSION_1 == 0 || offered & FLUSH == 0 {
        return Err(Error::Unsupported);
    }
    // No EVENT_IDX, packed queue, indirect descriptors or discard/write-zeroes
    // until their distinct semantics are implemented and tested.
    Ok(Negotiated {
        features: VERSION_1 | FLUSH,
        flush: true,
    })
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Descriptor {
    pub address: u64,
    pub length: u32,
    pub flags: u16,
    pub next: u16,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct UsedElement {
    pub id: u32,
    pub length: u32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Header {
    pub kind: u32,
    pub reserved: u32,
    pub sector: u64,
}

pub const DESCRIPTOR_NEXT: u16 = 1;
pub const DESCRIPTOR_WRITE: u16 = 2;

/// Only one in-flight request may own a three-element descriptor chain until
/// the device advances used.idx; a completion with any other id is invalid.
pub fn validate_completion(
    submitted_head: u16,
    used_id: u32,
    used_length: u32,
    expected_max: u32,
    status: u8,
) -> Result<(), Error> {
    if used_id != u32::from(submitted_head) || used_length > expected_max + 1 {
        return Err(Error::Corrupt);
    }
    match status {
        STATUS_OK => Ok(()),
        STATUS_IO_ERROR => Err(Error::Io),
        _ => Err(Error::Unsupported),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicitly_supported_features_are_accepted() {
        assert_eq!(negotiate(VERSION_1), Err(Error::Unsupported));
        assert_eq!(negotiate(FLUSH), Err(Error::Unsupported));
        let result = negotiate(VERSION_1 | FLUSH | RING_INDIRECT).unwrap();
        assert_eq!(result.features, VERSION_1 | FLUSH);
    }
    #[test]
    fn validate_used_ring_completion_and_status() {
        assert_eq!(core::mem::size_of::<Descriptor>(), 16);
        assert_eq!(core::mem::size_of::<Header>(), 16);
        assert_eq!(validate_completion(0, 0, 513, 512, STATUS_OK), Ok(()));
        assert_eq!(
            validate_completion(0, 1, 0, 512, STATUS_OK),
            Err(Error::Corrupt)
        );
        assert_eq!(
            validate_completion(0, 0, 514, 512, STATUS_OK),
            Err(Error::Corrupt)
        );
        assert_eq!(
            validate_completion(0, 0, 2, 512, STATUS_IO_ERROR),
            Err(Error::Io)
        );
    }
}
