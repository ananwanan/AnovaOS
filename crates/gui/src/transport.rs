// SPDX-License-Identifier: Apache-2.0
//! Experimental M14.3 kernel transport, separate from frozen native v1 and the future GUI C ABI.
//! Packets are copied synchronously; identities are assigned by the kernel, never by clients.

pub const SYS_GUI: u64 = 0x100;
pub const BIND: u64 = 0;
pub const CONNECT: u64 = 1;
pub const SEND: u64 = 2;
pub const RECEIVE: u64 = 3;
pub const REPLY: u64 = 4;
pub const POLL: u64 = 5;
pub const INPUT: u64 = 6;
pub const INFO: u64 = 7;
pub const PRESENT: u64 = 8;
pub const CLOSE: u64 = 9;
pub const UNBIND: u64 = 10;
/// Server-only eviction of a stalled connection; pointer argument is a scalar client identity.
pub const DROP_CLIENT: u64 = 11;
pub const PACKET_BYTES: usize = 4096;
pub const CONNECTED: u32 = 1;
pub const REQUEST: u32 = 2;
pub const DISCONNECTED: u32 = 3;
pub const RESET: u32 = 1;
pub const KEY: u32 = 2;
pub const POINTER: u32 = 3;

#[repr(C)]
pub struct Envelope {
    pub session: u64,
    pub client: u64,
    pub kind: u32,
    pub length: u32,
    pub payload: [u8; PACKET_BYTES],
}
impl Default for Envelope {
    fn default() -> Self {
        Self {
            session: 0,
            client: 0,
            kind: 0,
            length: 0,
            payload: [0; PACKET_BYTES],
        }
    }
}
#[repr(C)]
pub struct Delivery {
    pub client: u64,
    pub payload: [u8; 64],
}
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct HardwareEvent {
    pub kind: u32,
    pub value: u32,
    pub x: i32,
    pub y: i32,
    pub flags: u32,
    pub reserved: u32,
}
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
    pub session: u64,
}
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Scanout {
    pub pixels: u64,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub reserved: u32,
}
const _: () = assert!(size_of::<Envelope>() == 4120 && size_of::<Delivery>() == 72);
const _: () = assert!(size_of::<HardwareEvent>() == 24 && size_of::<DisplayInfo>() == 16);
const _: () = assert!(size_of::<Scanout>() == 32);
