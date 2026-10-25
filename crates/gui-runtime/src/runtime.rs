//! Own one bounded private window, authenticated copied RPC and a loss-reset event queue.
//! Blocking request calls yield for at most 500 native timer ticks; polling never waits.
//!
//! SPDX-License-Identifier: Apache-2.0

use crate::{
    Error, GuiEvent, GuiRect, GuiWindowDesc, MAX_HEIGHT, MAX_TEXT_BYTES, MAX_WIDTH,
    render::Canvas,
    wire::{self, Reply, Request, op, reply_kind},
};

pub(crate) trait Transport {
    fn connect(&mut self) -> Result<u64, Error>;
    fn send(&mut self, request: Request, pixels: &[u32]) -> Result<(), Error>;
    fn poll(&mut self) -> Result<Reply, Error>;
    fn close(&mut self);
    fn pause(&mut self);
}

pub(crate) struct Native;

impl Transport for Native {
    fn connect(&mut self) -> Result<u64, Error> {
        syscall(0, 0, 0, 1)
    }

    fn send(&mut self, request: Request, pixels: &[u32]) -> Result<(), Error> {
        let mut packet = [0_u8; 4096];
        packet[..64].copy_from_slice(&request.to_bytes());
        for (bytes, pixel) in packet[64..].chunks_exact_mut(4).zip(pixels) {
            bytes.copy_from_slice(&pixel.to_le_bytes());
        }
        syscall(packet.as_ptr() as u64, 64 + pixels.len() as u64 * 4, 0, 2).map(|_| ())
    }

    fn poll(&mut self) -> Result<Reply, Error> {
        let mut bytes = [0_u8; 64];
        syscall(bytes.as_mut_ptr() as u64, 64, 0, 5)?;
        Reply::from_bytes(&bytes).map_err(|_| Error::Transport)
    }

    fn close(&mut self) {
        let _ = syscall(0, 0, 0, 9);
    }

    fn pause(&mut self) {
        // SAFETY: Native sleep takes the scalar timer-tick count and no user pointers.
        unsafe { raw(4, 1, 0, 0) };
    }
}

fn syscall(pointer: u64, length: u64, _unused: u64, operation: u64) -> Result<u64, Error> {
    // SAFETY: Each transport caller supplies a live exact-size initialized copy buffer
    // for this synchronous operation, or a scalar operation with no address argument.
    let result = unsafe { raw(0x100, operation, pointer, length) };
    match result {
        -15 => Err(Error::WouldBlock),
        -7 | -16 => Err(Error::Disconnected),
        -18 => Err(Error::Limit),
        -24 => Err(Error::Busy),
        value if value < 0 => Err(Error::Transport),
        value => Ok(value as u64),
    }
}

unsafe fn raw(number: u64, a: u64, b: u64, c: u64) -> i64 {
    let value;
    // SAFETY: The caller owns any pointer ranges; native SYSCALL clobbers RCX/R11
    // and flags, retains no pointers, and preserves the user stack and other GPRs.
    unsafe {
        core::arch::asm!("syscall", inlateout("rax") number => value,
            in("rdi") a, in("rsi") b, in("rdx") c, lateout("rcx") _, lateout("r11") _,
            options(nostack));
    }
    value
}

pub(crate) struct Runtime<T> {
    pub transport: T,
    pub handle: u64,
    generation: u64,
    server_handle: u64,
    session: u64,
    sequence: u64,
    pub width: u32,
    pub height: u32,
    pixels: [u32; MAX_WIDTH as usize * MAX_HEIGHT as usize],
    events: [GuiEvent; 32],
    count: usize,
    pub clipboard: [u8; MAX_TEXT_BYTES as usize],
    pub clipboard_length: usize,
}

impl<T: Transport> Runtime<T> {
    pub const fn new(transport: T) -> Self {
        Self {
            transport,
            handle: 0,
            generation: 0,
            server_handle: 0,
            session: 0,
            sequence: 0,
            width: 0,
            height: 0,
            pixels: [0; MAX_WIDTH as usize * MAX_HEIGHT as usize],
            events: [GuiEvent::EMPTY; 32],
            count: 0,
            clipboard: [0; MAX_TEXT_BYTES as usize],
            clipboard_length: 0,
        }
    }

    pub fn check(&self, handle: u64) -> Result<(), Error> {
        if handle == 0 || handle != self.handle {
            Err(Error::Stale)
        } else {
            Ok(())
        }
    }

    pub fn create(&mut self, desc: GuiWindowDesc) -> Result<u64, Error> {
        desc.validate()?;
        if self.handle != 0 {
            return Err(Error::Limit);
        }
        self.generation = self.generation.checked_add(1).ok_or(Error::Limit)?;
        let mut connected = None;
        for _ in 0..500 {
            match self.transport.connect() {
                Err(Error::Disconnected | Error::WouldBlock | Error::Busy) => {
                    self.transport.pause()
                }
                result => {
                    connected = Some(result?);
                    break;
                }
            }
        }
        self.session = connected.ok_or(Error::Disconnected)?;
        self.sequence = 0;
        self.count = 0;
        self.server_handle = 0;
        let mut request = self.request(op::CREATE)?;
        request.x = desc.x;
        request.y = desc.y;
        request.width = desc.width;
        request.height = desc.height;
        request.flags = desc.flags;
        let reply = match self.rpc(request, &[]) {
            Ok(reply) => reply,
            Err(error) => {
                self.transport.close();
                return Err(error);
            }
        };
        if reply.window == 0 || reply.width != desc.width || reply.height != desc.height {
            self.transport.close();
            return Err(Error::Transport);
        }
        self.server_handle = reply.window;
        self.handle = self.generation;
        self.width = desc.width;
        self.height = desc.height;
        self.pixels.fill(0);
        Ok(self.handle)
    }

    pub fn destroy(&mut self, handle: u64) -> Result<(), Error> {
        self.check(handle)?;
        let request = self.request(op::DESTROY)?;
        let result = self.rpc(request, &[]).map(|_| ());
        self.transport.close();
        self.handle = 0;
        self.server_handle = 0;
        self.count = 0;
        self.width = 0;
        self.height = 0;
        result
    }

    pub fn show(&mut self, handle: u64, visible: bool) -> Result<(), Error> {
        self.check(handle)?;
        let request = self.request(if visible { op::SHOW } else { op::HIDE })?;
        self.rpc(request, &[]).map(|_| ())
    }

    pub fn canvas(&mut self, handle: u64) -> Result<Canvas<'_>, Error> {
        self.check(handle)?;
        Ok(Canvas {
            pixels: &mut self.pixels[..(self.width * self.height) as usize],
            width: self.width,
            height: self.height,
        })
    }

    pub fn present(&mut self, handle: u64) -> Result<(), Error> {
        self.check(handle)?;
        // Each upload is acknowledged before reusing its bounded stack packet. The server
        // retains private copies; PRESENT then commits damage with no vblank promise.
        let rows = 1008 / self.width;
        let mut y = 0;
        let mut chunk = [0; 1008];
        while y < self.height {
            let height = rows.min(self.height - y);
            let count = (self.width * height) as usize;
            let start = (y * self.width) as usize;
            chunk[..count].copy_from_slice(&self.pixels[start..start + count]);
            let mut request = self.request(op::UPLOAD)?;
            request.y = y as i32;
            request.width = self.width;
            request.height = height;
            request.stride = self.width;
            request.pixel_count = count as u32;
            self.rpc(request, &chunk[..count])?;
            if self.queued_resize() {
                // Applications drain the preserved geometry event and redraw before
                // retrying. A concurrent resize cannot commit an obsolete full frame.
                return Err(Error::WouldBlock);
            }
            y += height;
        }
        let request = self.request(op::PRESENT)?;
        self.rpc(request, &[]).map(|_| ())
    }

    pub fn next(&mut self, handle: u64) -> Result<GuiEvent, Error> {
        self.check(handle)?;
        let event = if self.count != 0 {
            let event = self.events[0];
            self.events.copy_within(1..self.count, 0);
            self.count -= 1;
            event
        } else {
            let reply = match self.transport.poll() {
                Ok(reply) => reply,
                Err(Error::WouldBlock) => return Err(Error::WouldBlock),
                Err(error) => return Err(self.retire(error)),
            };
            match self.event(reply) {
                Ok(event) => event,
                Err(error) => return Err(self.retire(error)),
            }
        };
        if event.kind == 3
            && event.width != 0
            && event.height != 0
            && (event.width != self.width || event.height != self.height)
        {
            // Geometry changes invalidate the private raster. The next event supplies
            // new dimensions; the application redraws before its next presentation.
            self.width = event.width;
            self.height = event.height;
            self.pixels.fill(0);
        }
        Ok(event)
    }

    fn request(&mut self, opcode: u32) -> Result<Request, Error> {
        self.sequence = self.sequence.checked_add(1).ok_or(Error::Limit)?;
        let mut request = Request::new(opcode, self.session);
        request.window = self.server_handle;
        request.sequence = self.sequence;
        Ok(request)
    }

    fn retire(&mut self, error: Error) -> Error {
        self.transport.close();
        self.handle = 0;
        self.server_handle = 0;
        self.count = 0;
        self.width = 0;
        self.height = 0;
        error
    }

    fn queued_resize(&self) -> bool {
        self.events[..self.count].iter().any(|event| {
            event.kind == 3
                && event.width != 0
                && (event.width != self.width || event.height != self.height)
        })
    }

    fn rpc(&mut self, request: Request, pixels: &[u32]) -> Result<Reply, Error> {
        let mut sent = false;
        for _ in 0..500 {
            match self.transport.send(request, pixels) {
                Err(Error::WouldBlock) => self.transport.pause(),
                result => {
                    if let Err(error) = result {
                        return Err(self.retire(error));
                    }
                    sent = true;
                    break;
                }
            }
        }
        if !sent {
            return Err(Error::WouldBlock);
        }
        for _ in 0..500 {
            match self.transport.poll() {
                Err(Error::WouldBlock) => self.transport.pause(),
                Err(error) => return Err(self.retire(error)),
                Ok(reply) if reply.kind == reply_kind::ACK => {
                    if reply.sequence != request.sequence || reply.session != self.session {
                        return Err(self.retire(Error::Transport));
                    }
                    return match reply.status {
                        0 => Ok(reply),
                        wire::status::QUOTA => Err(Error::Limit),
                        wire::status::STALE_HANDLE => Err(self.retire(Error::Stale)),
                        wire::status::SESSION => Err(self.retire(Error::Disconnected)),
                        wire::status::INVALID_SURFACE if self.queued_resize() => {
                            Err(Error::WouldBlock)
                        }
                        _ => Err(Error::Transport),
                    };
                }
                Ok(reply) => {
                    let event = match self.event(reply) {
                        Ok(event) => event,
                        Err(error) => return Err(self.retire(error)),
                    };
                    if self.count == self.events.len() {
                        // Input history is no longer authoritative, but the latest
                        // pointer geometry must survive without requiring another
                        // physical movement. Clear held buttons in the retained event
                        // so lost presses cannot accidentally activate a widget.
                        let geometry = core::iter::once(&event)
                            .chain(self.events[..self.count].iter().rev())
                            .find(|event| event.kind == 3 && event.width != 0)
                            .copied();
                        self.count = 1;
                        self.events[0] = GuiEvent {
                            kind: 1,
                            ..GuiEvent::EMPTY
                        };
                        if let Some(mut geometry) = geometry {
                            geometry.buttons = 0;
                            geometry.changed = 0;
                            self.events[1] = geometry;
                            self.count = 2;
                        }
                    } else {
                        self.events[self.count] = event;
                        self.count += 1;
                    }
                }
            }
        }
        // An unknown completion cannot be retried with a new request: terminate this
        // connection so a delayed ACK cannot be mistaken for another operation.
        Err(self.retire(Error::Disconnected))
    }

    fn event(&self, reply: Reply) -> Result<GuiEvent, Error> {
        if reply.session != self.session
            || reply.status != 0
            || reply.sequence != 0
            || reply.version != wire::VERSION
            || reply.reserved != 0
            || (reply.window != self.server_handle && reply.kind != reply_kind::RESET)
        {
            return Err(Error::Transport);
        }
        let mut event = GuiEvent::EMPTY;
        match reply.kind {
            reply_kind::RESET => event.kind = 1,
            reply_kind::KEY => {
                if reply.x != 0 || reply.y != 0 || reply.height != 0 {
                    return Err(Error::Transport);
                }
                event.kind = 2;
                event.key = reply.value;
                event.state = reply.width;
                event.modifiers = reply.flags;
                if reply.width == 1 || reply.width == 3 {
                    event.codepoint = key_text(reply.value, reply.flags);
                }
            }
            reply_kind::POINTER => {
                event.kind = 3;
                event.x = reply.x;
                event.y = reply.y;
                event.width = reply.width;
                event.height = reply.height;
                event.buttons = reply.value;
                event.changed = reply.flags;
            }
            _ => return Err(Error::Transport),
        }
        event.validate().map_err(|_| Error::Transport)?;
        if event.kind == 3 && (event.width > MAX_WIDTH || event.height > MAX_HEIGHT) {
            return Err(Error::Transport);
        }
        Ok(event)
    }
}

pub(crate) fn contains(rect: GuiRect, x: i32, y: i32) -> bool {
    i64::from(x) >= i64::from(rect.x)
        && i64::from(y) >= i64::from(rect.y)
        && i64::from(x) < i64::from(rect.x) + i64::from(rect.width)
        && i64::from(y) < i64::from(rect.y) + i64::from(rect.height)
}

pub(crate) fn key_text(key: u32, modifiers: u32) -> u32 {
    if modifiers & 6 != 0 {
        return 0;
    }
    let character = match key {
        2..=11 => b"1234567890"[(key - 2) as usize],
        12 => b'-',
        13 => b'=',
        14 => 8,
        15 => 9,
        16..=25 => b"qwertyuiop"[(key - 16) as usize],
        26 => b'[',
        27 => b']',
        28 => b'\n',
        30..=38 => b"asdfghjkl"[(key - 30) as usize],
        39 => b';',
        40 => b'\'',
        41 => b'`',
        43 => b'\\',
        44..=50 => b"zxcvbnm"[(key - 44) as usize],
        51 => b',',
        52 => b'.',
        53 => b'/',
        57 => b' ',
        _ => return 0,
    };
    if character.is_ascii_alphabetic() {
        return u32::from(if (modifiers & 1 != 0) ^ (modifiers & 8 != 0) {
            character.to_ascii_uppercase()
        } else {
            character
        });
    }
    if modifiers & 1 != 0 {
        let plain = b"1234567890-=[];'`\\,./";
        let shifted = b"!@#$%^&*()_+{}:\"~|<>?";
        if let Some(index) = plain.iter().position(|value| *value == character) {
            return u32::from(shifted[index]);
        }
    }
    u32::from(character)
}
