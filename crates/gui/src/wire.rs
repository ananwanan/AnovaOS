//! Encode the fixed-width little-endian experimental GUI protocol without native pointers.
//!
//! All fields are integer bit patterns. Explicit byte conversion avoids padding, alignment
//! and host-endian assumptions; reserved fields and operation-specific bounds are validated
//! by the policy. The kernel supplies the authenticated client identity outside this packet.
//!
//! SPDX-License-Identifier: Apache-2.0

pub const VERSION: u32 = 1;
pub const MESSAGE_BYTES: usize = 64;
pub const CREATE_VISIBLE: u32 = 1;

pub mod op {
    pub const CREATE: u32 = 1;
    pub const DESTROY: u32 = 2;
    pub const MOVE: u32 = 3;
    pub const RESIZE: u32 = 4;
    pub const SHOW: u32 = 5;
    pub const HIDE: u32 = 6;
    pub const FOCUS: u32 = 7;
    pub const UPLOAD: u32 = 8;
    pub const PRESENT: u32 = 9;
}

pub mod reply_kind {
    pub const ACK: u32 = 1;
    pub const KEY: u32 = 2;
    pub const POINTER: u32 = 3;
    pub const RESET: u32 = 4;
}

pub mod status {
    pub const OK: i32 = 0;
    pub const VERSION: i32 = -1;
    pub const MALFORMED: i32 = -2;
    pub const SESSION: i32 = -3;
    pub const UNKNOWN_CLIENT: i32 = -4;
    pub const STALE_HANDLE: i32 = -5;
    pub const FOREIGN_HANDLE: i32 = -6;
    pub const QUOTA: i32 = -7;
    pub const INVALID_SURFACE: i32 = -8;
    pub const BUFFER_LENGTH: i32 = -9;
    pub const UNKNOWN_OPCODE: i32 = -10;
    pub const BUFFER_TOO_SMALL: i32 = -11;
    pub const ARITHMETIC: i32 = -12;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum Error {
    Version = status::VERSION,
    Malformed = status::MALFORMED,
    Session = status::SESSION,
    UnknownClient = status::UNKNOWN_CLIENT,
    StaleHandle = status::STALE_HANDLE,
    ForeignHandle = status::FOREIGN_HANDLE,
    Quota = status::QUOTA,
    InvalidSurface = status::INVALID_SURFACE,
    BufferLength = status::BUFFER_LENGTH,
    UnknownOpcode = status::UNKNOWN_OPCODE,
    BufferTooSmall = status::BUFFER_TOO_SMALL,
    Arithmetic = status::ARITHMETIC,
}

impl Error {
    pub const fn code(self) -> i32 {
        self as i32
    }
}

/// A request has no client ID or memory address. Upload pixels follow this 64-byte header.
/// `stride` counts u32 pixels, and `pixel_count` counts the complete supplied source slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Request {
    pub version: u32,
    pub opcode: u32,
    pub sequence: u64,
    pub session: u64,
    pub window: u64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub pixel_count: u32,
    pub flags: u32,
    pub reserved: u32,
}

impl Request {
    pub const fn new(opcode: u32, session: u64) -> Self {
        Self {
            version: VERSION,
            opcode,
            sequence: 0,
            session,
            window: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            stride: 0,
            pixel_count: 0,
            flags: 0,
            reserved: 0,
        }
    }

    pub fn to_bytes(self) -> [u8; MESSAGE_BYTES] {
        let mut bytes = [0; MESSAGE_BYTES];
        put32(&mut bytes, 0, self.version);
        put32(&mut bytes, 4, self.opcode);
        put64(&mut bytes, 8, self.sequence);
        put64(&mut bytes, 16, self.session);
        put64(&mut bytes, 24, self.window);
        put32(&mut bytes, 32, self.x as u32);
        put32(&mut bytes, 36, self.y as u32);
        put32(&mut bytes, 40, self.width);
        put32(&mut bytes, 44, self.height);
        put32(&mut bytes, 48, self.stride);
        put32(&mut bytes, 52, self.pixel_count);
        put32(&mut bytes, 56, self.flags);
        put32(&mut bytes, 60, self.reserved);
        bytes
    }

    /// Parse exactly one header; callers pass any trailing pixel bytes separately.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != MESSAGE_BYTES {
            return Err(Error::BufferLength);
        }
        Ok(Self {
            version: get32(bytes, 0),
            opcode: get32(bytes, 4),
            sequence: get64(bytes, 8),
            session: get64(bytes, 16),
            window: get64(bytes, 24),
            x: get32(bytes, 32) as i32,
            y: get32(bytes, 36) as i32,
            width: get32(bytes, 40),
            height: get32(bytes, 44),
            stride: get32(bytes, 48),
            pixel_count: get32(bytes, 52),
            flags: get32(bytes, 56),
            reserved: get32(bytes, 60),
        })
    }
}

/// ACK geometry describes the current window. KEY uses value=physical code,
/// flags=modifier bits and width=state (1 press, 2 release, 3 repeat). POINTER uses
/// x/y=window-local position, value=buttons, flags=changed buttons, width/height=size.
/// RESET clears every held key/button. A focus change requires a RESET to the old
/// target; callers can compare `focused_client()` before and after each operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Reply {
    pub version: u32,
    pub kind: u32,
    pub sequence: u64,
    pub window: u64,
    pub status: i32,
    pub reserved: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub value: u32,
    pub flags: u32,
    pub session: u64,
}

impl Reply {
    pub const fn success(sequence: u64, session: u64) -> Self {
        Self {
            version: VERSION,
            kind: reply_kind::ACK,
            sequence,
            window: 0,
            status: status::OK,
            reserved: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            value: 0,
            flags: 0,
            session,
        }
    }

    pub const fn failure(sequence: u64, session: u64, error: Error) -> Self {
        Self {
            status: error.code(),
            ..Self::success(sequence, session)
        }
    }

    pub const fn is_ok(self) -> bool {
        self.status == status::OK
    }

    pub fn to_bytes(self) -> [u8; MESSAGE_BYTES] {
        let mut bytes = [0; MESSAGE_BYTES];
        put32(&mut bytes, 0, self.version);
        put32(&mut bytes, 4, self.kind);
        put64(&mut bytes, 8, self.sequence);
        put64(&mut bytes, 16, self.window);
        put32(&mut bytes, 24, self.status as u32);
        put32(&mut bytes, 28, self.reserved);
        put32(&mut bytes, 32, self.x as u32);
        put32(&mut bytes, 36, self.y as u32);
        put32(&mut bytes, 40, self.width);
        put32(&mut bytes, 44, self.height);
        put32(&mut bytes, 48, self.value);
        put32(&mut bytes, 52, self.flags);
        put64(&mut bytes, 56, self.session);
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != MESSAGE_BYTES {
            return Err(Error::BufferLength);
        }
        let reply = Self {
            version: get32(bytes, 0),
            kind: get32(bytes, 4),
            sequence: get64(bytes, 8),
            window: get64(bytes, 16),
            status: get32(bytes, 24) as i32,
            reserved: get32(bytes, 28),
            x: get32(bytes, 32) as i32,
            y: get32(bytes, 36) as i32,
            width: get32(bytes, 40),
            height: get32(bytes, 44),
            value: get32(bytes, 48),
            flags: get32(bytes, 52),
            session: get64(bytes, 56),
        };
        if reply.version != VERSION {
            return Err(Error::Version);
        }
        if reply.reserved != 0 {
            return Err(Error::Malformed);
        }
        Ok(reply)
    }
}

const _: () = assert!(core::mem::size_of::<Request>() == MESSAGE_BYTES);
const _: () = assert!(core::mem::align_of::<Request>() == 8);
const _: () = assert!(core::mem::size_of::<Reply>() == MESSAGE_BYTES);
const _: () = assert!(core::mem::align_of::<Reply>() == 8);

fn put32(bytes: &mut [u8; MESSAGE_BYTES], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(bytes: &mut [u8; MESSAGE_BYTES], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn get32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap_or([0; 4]))
}

fn get64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap_or([0; 8]))
}
