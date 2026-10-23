// SPDX-License-Identifier: Apache-2.0
//! Own bounded authenticated GUI mailboxes and exclusive checked display/input access.
//! Called only by the BSP scheduler at IF=0; window policy and all rendering remain in userspace.
use crate::{
    boot, graphics,
    mm::{self, uaccess, user::UserSpace},
    serial,
};
use alloc::{collections::VecDeque, vec::Vec};
use anova_abi::*;
use anova_gui::transport::*;
use core::slice;

const CLIENTS: usize = 4;
const REPLIES: usize = 16;
struct Client {
    pid: u64,
    connected: bool,
    announced: bool,
    pending: Vec<u8>,
    replies: VecDeque<[u8; 64]>,
}
#[derive(Default)]
pub struct Gui {
    server: Option<u64>,
    session: u64,
    clients: Vec<Client>,
    next_client: usize,
    admitted: [u64; 32],
}
impl Gui {
    pub fn set_image(&mut self, pid: u64, bytes: &[u8], native: bool) {
        self.close_process(pid);
        if native
            && bytes == boot::native_module("compositor")
            && let Some(slot) = self.admitted.iter_mut().find(|slot| **slot == 0)
        {
            *slot = pid;
        }
    }
    pub fn close_process(&mut self, pid: u64) {
        for slot in &mut self.admitted {
            if *slot == pid {
                *slot = 0;
            }
        }
        self.disconnect(pid);
    }
    fn disconnect(&mut self, pid: u64) {
        if self.server == Some(pid) {
            self.server = None;
            self.clients.clear();
            self.clients.shrink_to_fit();
            crate::drivers::input::gui_owner(false);
            serial::write(format_args!(
                "[GUI:SERVER-RELEASE] pid={pid} session={} clients=0\n",
                self.session
            ));
        } else if let Some(client) = self.clients.iter_mut().find(|c| c.pid == pid) {
            client.connected = false;
            client.pending.clear();
            client.replies.clear();
        }
    }
    fn server(&self, pid: u64) -> Result<(), i64> {
        if self.server == Some(pid) {
            Ok(())
        } else {
            Err(PERMISSION_DENIED)
        }
    }
    fn client(&mut self, pid: u64) -> Result<&mut Client, i64> {
        self.clients
            .iter_mut()
            .find(|c| c.pid == pid && c.connected)
            .ok_or(BROKEN_PIPE)
    }
    fn connect(&mut self, pid: u64) -> Result<i64, i64> {
        if self.server.is_none() {
            return Err(NOT_FOUND);
        }
        if self.server == Some(pid) {
            return Err(PERMISSION_DENIED);
        }
        if self.clients.iter().any(|c| c.pid == pid) {
            return Err(BUSY);
        }
        if self.clients.len() == CLIENTS {
            return Err(NO_SPACE);
        }
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(PACKET_BYTES)
            .map_err(|_| NO_MEMORY)?;
        let mut replies = VecDeque::new();
        replies.try_reserve_exact(REPLIES).map_err(|_| NO_MEMORY)?;
        self.clients.try_reserve(1).map_err(|_| NO_MEMORY)?;
        self.clients.push(Client {
            pid,
            connected: true,
            announced: false,
            pending,
            replies,
        });
        Ok(self.session as i64)
    }
    pub fn syscall(
        &mut self,
        pid: u64,
        space: &UserSpace,
        op: u64,
        pointer: u64,
        length: u64,
    ) -> i64 {
        self.call(
            pid,
            self.admitted.contains(&pid),
            space,
            op,
            pointer,
            length,
        )
        .unwrap_or_else(|error| error)
    }
    fn call(
        &mut self,
        pid: u64,
        authorized: bool,
        space: &UserSpace,
        op: u64,
        pointer: u64,
        length: u64,
    ) -> Result<i64, i64> {
        match op {
            BIND => {
                scalar(pointer, length)?;
                if !authorized {
                    return Err(PERMISSION_DENIED);
                }
                if self.server.is_some() {
                    return Err(BUSY);
                }
                let display = boot::display().map_err(|_| UNSUPPORTED)?;
                mm::check_display(&display).map_err(|_| UNSUPPORTED)?;
                if display.layout.width() > 800 || display.layout.height() > 600 {
                    return Err(UNSUPPORTED);
                }
                self.session = self
                    .session
                    .checked_add(1)
                    .filter(|v| *v <= i64::MAX as u64)
                    .ok_or(NO_SPACE)?;
                self.server = Some(pid);
                self.clients.clear();
                crate::drivers::input::gui_owner(true);
                serial::write(format_args!(
                    "[GUI:SERVER-ACQUIRE] pid={pid} session={}\n",
                    self.session
                ));
                Ok(self.session as i64)
            }
            CONNECT => {
                scalar(pointer, length)?;
                self.connect(pid)
            }
            SEND => {
                let length = usize::try_from(length).map_err(|_| INVALID_ARGUMENT)?;
                if length == 0 || length > PACKET_BYTES {
                    return Err(INVALID_ARGUMENT);
                }
                let c = self.client(pid)?;
                if !c.pending.is_empty() {
                    return Err(WOULD_BLOCK);
                }
                let mut bytes = [0; PACKET_BYTES];
                copy_in(space, pointer, &mut bytes[..length])?;
                c.pending.extend_from_slice(&bytes[..length]);
                Ok(0)
            }
            RECEIVE => {
                self.server(pid)?;
                exact(length, size_of::<Envelope>())?;
                let count = self.clients.len();
                for offset in 0..count {
                    let index = (self.next_client + offset) % count;
                    let c = &self.clients[index];
                    let kind = if !c.connected {
                        DISCONNECTED
                    } else if !c.announced {
                        CONNECTED
                    } else if !c.pending.is_empty() {
                        REQUEST
                    } else {
                        continue;
                    };
                    let mut bytes = [0; size_of::<Envelope>()];
                    bytes[..8].copy_from_slice(&self.session.to_le_bytes());
                    bytes[8..16].copy_from_slice(&c.pid.to_le_bytes());
                    bytes[16..20].copy_from_slice(&kind.to_le_bytes());
                    if kind == REQUEST {
                        bytes[20..24].copy_from_slice(&(c.pending.len() as u32).to_le_bytes());
                        bytes[24..24 + c.pending.len()].copy_from_slice(&c.pending);
                    }
                    copy_out(space, pointer, &bytes)?;
                    if kind == DISCONNECTED {
                        self.clients.remove(index);
                    } else if kind == CONNECTED {
                        self.clients[index].announced = true;
                    } else {
                        self.clients[index].pending.clear();
                    }
                    self.next_client = index + 1;
                    return Ok(0);
                }
                Err(WOULD_BLOCK)
            }
            REPLY => {
                self.server(pid)?;
                exact(length, size_of::<Delivery>())?;
                let mut bytes = [0; size_of::<Delivery>()];
                copy_in(space, pointer, &mut bytes)?;
                let client = u64::from_le_bytes(bytes[..8].try_into().unwrap());
                let c = self.client(client)?;
                if c.replies.len() == REPLIES {
                    return Err(WOULD_BLOCK);
                }
                c.replies.push_back(bytes[8..].try_into().unwrap());
                Ok(0)
            }
            POLL => {
                exact(length, 64)?;
                let c = self.client(pid)?;
                let bytes = c.replies.front().ok_or(WOULD_BLOCK)?;
                copy_out(space, pointer, bytes)?;
                c.replies.pop_front();
                Ok(0)
            }
            INPUT => {
                self.server(pid)?;
                exact(length, size_of::<HardwareEvent>())?;
                // Preflight before consuming; all fields are copied out as initialized bytes.
                if !space.accessible(UserAddr(pointer), size_of::<HardwareEvent>(), true, false) {
                    return Err(BAD_ADDRESS);
                }
                let event = crate::drivers::input::gui_event()?;
                let mut bytes = [0; size_of::<HardwareEvent>()];
                bytes[..4].copy_from_slice(&event.kind.to_le_bytes());
                bytes[4..8].copy_from_slice(&event.value.to_le_bytes());
                bytes[8..12].copy_from_slice(&event.x.to_le_bytes());
                bytes[12..16].copy_from_slice(&event.y.to_le_bytes());
                bytes[16..20].copy_from_slice(&event.flags.to_le_bytes());
                copy_out(space, pointer, &bytes)?;
                Ok(0)
            }
            INFO => {
                self.server(pid)?;
                exact(length, size_of::<DisplayInfo>())?;
                let display = boot::display().map_err(|_| UNSUPPORTED)?;
                let mut bytes = [0; size_of::<DisplayInfo>()];
                bytes[..4].copy_from_slice(&display.layout.width().to_le_bytes());
                bytes[4..8].copy_from_slice(&display.layout.height().to_le_bytes());
                bytes[8..].copy_from_slice(&self.session.to_le_bytes());
                copy_out(space, pointer, &bytes)?;
                Ok(0)
            }
            PRESENT => {
                self.server(pid)?;
                exact(length, size_of::<Scanout>())?;
                let mut bytes = [0; size_of::<Scanout>()];
                copy_in(space, pointer, &mut bytes)?;
                let pixels = u64::from_le_bytes(bytes[..8].try_into().unwrap());
                let field =
                    |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                let (x, y, width, height, stride) =
                    (field(8), field(12), field(16), field(20), field(24));
                let display = boot::display().map_err(|_| UNSUPPORTED)?;
                if field(28) != 0
                    || width == 0
                    || height == 0
                    || stride < width
                    || stride > 800
                    || x.checked_add(width)
                        .is_none_or(|right| right > display.layout.width())
                    || y.checked_add(height)
                        .is_none_or(|bottom| bottom > display.layout.height())
                {
                    return Err(INVALID_ARGUMENT);
                }
                let last = u64::from(height - 1)
                    .checked_mul(u64::from(stride))
                    .and_then(|n| n.checked_add(u64::from(width)))
                    .and_then(|n| n.checked_mul(4))
                    .ok_or(INVALID_ARGUMENT)?;
                if !space.accessible(UserAddr(pixels), last as usize, false, false) {
                    return Err(BAD_ADDRESS);
                }
                let count = (width as usize)
                    .checked_mul(height as usize)
                    .ok_or(INVALID_ARGUMENT)?;
                let mut copied = Vec::<u32>::new();
                copied.try_reserve_exact(count).map_err(|_| NO_MEMORY)?;
                copied.resize(count, 0);
                for row in 0..height as usize {
                    let source = pixels
                        .checked_add(row as u64 * u64::from(stride) * 4)
                        .ok_or(INVALID_ARGUMENT)?;
                    let row_pixels = &mut copied[row * width as usize..(row + 1) * width as usize];
                    // SAFETY: u32 storage is exclusive, initialized and has no padding;
                    // the byte borrow lasts only during the checked synchronous copy.
                    let row_bytes = unsafe {
                        slice::from_raw_parts_mut(
                            row_pixels.as_mut_ptr().cast(),
                            row_pixels.len() * 4,
                        )
                    };
                    copy_in(space, source, row_bytes)?;
                }
                graphics::present_region(&display, x, y, width, height, &copied);
                Ok(0)
            }
            CLOSE => {
                scalar(pointer, length)?;
                self.client(pid)?;
                self.disconnect(pid);
                Ok(0)
            }
            UNBIND => {
                scalar(pointer, length)?;
                self.server(pid)?;
                self.disconnect(pid);
                Ok(0)
            }
            DROP_CLIENT => {
                self.server(pid)?;
                exact(length, 0)?;
                self.client(pointer)?;
                self.disconnect(pointer);
                Ok(0)
            }
            _ => Err(UNSUPPORTED),
        }
    }
}
fn exact(length: u64, expected: usize) -> Result<(), i64> {
    if length == expected as u64 {
        Ok(())
    } else {
        Err(INVALID_ARGUMENT)
    }
}
fn scalar(pointer: u64, length: u64) -> Result<(), i64> {
    if pointer == 0 && length == 0 {
        Ok(())
    } else {
        Err(INVALID_ARGUMENT)
    }
}
fn copy_in(space: &UserSpace, pointer: u64, bytes: &mut [u8]) -> Result<(), i64> {
    space.with_active(|| uaccess::from_user(space, UserAddr(pointer), bytes))
}
fn copy_out(space: &UserSpace, pointer: u64, bytes: &[u8]) -> Result<(), i64> {
    space.with_active(|| uaccess::to_user(space, UserAddr(pointer), bytes))
}

/// Test real fallible mailbox allocation without suspending any live client or GUI session.
#[cfg(feature = "gui-test")]
pub fn verify_transport() {
    let before = mm::heap_used();
    {
        let mut test = Gui {
            server: Some(1),
            session: 1,
            ..Gui::default()
        };
        let mut blocks = Vec::new();
        blocks.try_reserve_exact(8192).unwrap();
        let allocated = mm::heap_used();
        for size in [4096, 1024, 256, 64] {
            let layout = core::alloc::Layout::from_size_align(size, 16).unwrap();
            loop {
                // SAFETY: Test retains the exact pointer/layout until the matching free;
                // every allocation is disjoint and no reference is created from it.
                let pointer = unsafe { alloc::alloc::alloc(layout) };
                if pointer.is_null() {
                    break;
                }
                assert!(
                    blocks.len() < blocks.capacity(),
                    "GUI pressure storage exhausted"
                );
                blocks.push((pointer, layout));
            }
        }
        let exhausted = mm::heap_used();
        assert_eq!(test.connect(2), Err(NO_MEMORY));
        assert!(test.clients.is_empty());
        assert_eq!(
            mm::heap_used(),
            exhausted,
            "GUI OOM did not roll back allocations"
        );
        for (pointer, layout) in blocks.drain(..) {
            // SAFETY: Each pointer is a distinct live allocation with its original layout.
            unsafe {
                alloc::alloc::dealloc(pointer, layout);
            }
        }
        assert_eq!(mm::heap_used(), allocated);
        for pid in 2..6 {
            assert_eq!(test.connect(pid), Ok(1));
        }
        assert_eq!(test.connect(6), Err(NO_SPACE));
        assert_eq!(test.connect(2), Err(BUSY));
        test.close_process(1);
        assert!(test.clients.is_empty());
    }
    assert_eq!(
        mm::heap_used(),
        before,
        "GUI transport leaked after pressure/reconnect"
    );
    serial::write(format_args!(
        "[GUI:TRANSPORT-TEST] oom=true rollback=true quotas=true\n"
    ));
}
