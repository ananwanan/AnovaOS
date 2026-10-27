// SPDX-License-Identifier: Apache-2.0
//! Pump one acknowledged GUI upload at a time while the terminal keeps polling input/PTY I/O.
//! Raster snapshots stay immutable until PRESENT; geometry invalidates an in-flight frame.
//! This raw checked GUI protocol client leaves the shared C/C++/Rust runtime ABI unchanged.

use anova_gui::{Request, op, reply_kind, status, transport as t};
use anova_gui_runtime::{Error, GuiEvent, GuiRect, render::Canvas};
use anova_gui_user as gui;
use anova_rt::{self as rt, abi::*};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};

struct Pixels(UnsafeCell<[u32; 320 * 240]>);
// SAFETY: Only the single, never-forked terminal UI takes this process-private BSS once.
// No raster pointer escapes to another thread or survives a synchronous copied GUI send.
unsafe impl Sync for Pixels {}
static PIXELS: Pixels = Pixels(UnsafeCell::new([0; 320 * 240]));
static CLAIMED: AtomicBool = AtomicBool::new(false);

struct Pending {
    sequence: u64,
    opcode: u32,
    rows: u32,
}

pub(super) struct Window {
    session: u64,
    handle: u64,
    sequence: u64,
    width: u32,
    height: u32,
    pixels: &'static mut [u32],
    pending: Option<Pending>,
    active: bool,
    invalidated: bool,
    complete: bool,
    y: u32,
}

impl Window {
    pub fn create() -> Result<Self, Error> {
        assert!(!CLAIMED.swap(true, Ordering::AcqRel));
        let session = gui::connect().map_err(error)?;
        let mut request = Request::new(op::CREATE, session);
        request.sequence = 1;
        request.x = 450;
        request.y = 20;
        request.width = 312;
        request.height = 232;
        request.flags = 1;
        gui::send(&request, &[]).map_err(error)?;
        let reply = loop {
            match gui::poll() {
                Ok(reply) => break reply,
                Err(WOULD_BLOCK) => rt::sleep(1).map_err(error)?,
                Err(e) => return Err(error(e)),
            }
        };
        if reply.kind != reply_kind::ACK
            || reply.sequence != 1
            || reply.session != session
            || !reply.is_ok()
        {
            return Err(Error::Transport);
        }
        // SAFETY: CLAIMED establishes sole ownership for the lifetime of this window.
        // The process is single-threaded, never forks, and no other reference is formed.
        let pixels = unsafe { &mut *PIXELS.0.get() };
        Ok(Self {
            session,
            handle: reply.window,
            sequence: 1,
            width: 312,
            height: 232,
            pixels,
            pending: None,
            active: false,
            invalidated: false,
            complete: false,
            y: 0,
        })
    }

    pub fn clear(&mut self, color: u32) -> Result<(), Error> {
        self.pixels.fill(color);
        Ok(())
    }
    fn canvas(&mut self) -> Canvas<'_> {
        Canvas {
            pixels: &mut self.pixels[..(self.width * self.height) as usize],
            width: self.width,
            height: self.height,
        }
    }
    pub fn text(&mut self, x: i32, y: i32, text: &str, color: u32) -> Result<(), Error> {
        self.canvas().text(x, y, text, color);
        Ok(())
    }
    pub fn rect(&mut self, rect: GuiRect, color: u32) -> Result<(), Error> {
        self.canvas().rect(rect, color);
        Ok(())
    }
    pub fn idle(&self) -> bool {
        !self.active && self.pending.is_none()
    }
    pub fn begin(&mut self) {
        self.active = true;
        self.y = 0;
        self.invalidated = false;
    }

    pub fn advance(&mut self) -> Result<bool, Error> {
        let completed = core::mem::take(&mut self.complete);
        if self.pending.is_some() || !self.active {
            return Ok(completed);
        }
        let rows = (1008 / self.width).min(self.height - self.y);
        let opcode = if self.y == self.height {
            op::PRESENT
        } else {
            op::UPLOAD
        };
        let mut request = Request::new(opcode, self.session);
        request.sequence = self.sequence.checked_add(1).ok_or(Error::Limit)?;
        request.window = self.handle;
        let pixels = if opcode == op::UPLOAD {
            request.y = self.y as i32;
            request.width = self.width;
            request.height = rows;
            request.stride = self.width;
            request.pixel_count = self.width * rows;
            let start = (self.y * self.width) as usize;
            &self.pixels[start..start + request.pixel_count as usize]
        } else {
            &[]
        };
        match gui::send(&request, pixels) {
            Ok(()) => {
                self.sequence = request.sequence;
                self.pending = Some(Pending {
                    sequence: request.sequence,
                    opcode,
                    rows,
                });
            }
            Err(WOULD_BLOCK) => {}
            Err(e) => return Err(error(e)),
        }
        Ok(completed)
    }

    pub fn poll(&mut self) -> Result<GuiEvent, Error> {
        let reply = gui::poll().map_err(error)?;
        if reply.version != anova_gui::VERSION
            || reply.session != self.session
            || reply.reserved != 0
        {
            return Err(Error::Transport);
        }
        if reply.kind == reply_kind::ACK {
            let pending = self.pending.take().ok_or(Error::Transport)?;
            if reply.sequence != pending.sequence {
                return Err(Error::Transport);
            }
            if self.invalidated {
                if !matches!(reply.status, status::OK | status::INVALID_SURFACE) {
                    return Err(Error::Transport);
                }
                self.active = false;
                return Ok(GuiEvent::EMPTY);
            }
            if !reply.is_ok() {
                return Err(Error::Transport);
            }
            if pending.opcode == op::PRESENT {
                self.active = false;
                self.complete = true;
            } else {
                self.y += pending.rows;
            }
            return Ok(GuiEvent::EMPTY);
        }
        if reply.sequence != 0
            || !reply.is_ok()
            || (reply.window != self.handle && reply.kind != reply_kind::RESET)
        {
            return Err(Error::Transport);
        }
        let mut event = GuiEvent::EMPTY;
        match reply.kind {
            reply_kind::RESET => event.kind = 1,
            reply_kind::KEY => {
                event.kind = 2;
                event.key = reply.value;
                event.state = reply.width;
                event.modifiers = reply.flags;
                event.codepoint = codepoint(&event);
            }
            reply_kind::POINTER => {
                event.kind = 3;
                event.x = reply.x;
                event.y = reply.y;
                event.buttons = reply.value;
                event.changed = reply.flags;
                event.width = reply.width;
                event.height = reply.height;
                if !(32..=320).contains(&reply.width) || !(24..=240).contains(&reply.height) {
                    return Err(Error::Transport);
                }
                if self.width != reply.width || self.height != reply.height {
                    self.width = reply.width;
                    self.height = reply.height;
                    self.invalidated = true;
                    self.active = false;
                }
            }
            _ => return Err(Error::Transport),
        }
        Ok(event)
    }

    pub fn close(&mut self) -> Result<(), Error> {
        if self.handle != 0 {
            gui::scalar(t::CLOSE).map_err(error)?;
            self.handle = 0;
        }
        Ok(())
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn error(value: i64) -> Error {
    match value {
        WOULD_BLOCK => Error::WouldBlock,
        BROKEN_PIPE => Error::Disconnected,
        FILE_LIMIT | NO_SPACE => Error::Limit,
        _ => Error::Transport,
    }
}

fn codepoint(event: &GuiEvent) -> u32 {
    if !matches!(event.state, 1 | 3) || event.modifiers & 6 != 0 {
        return 0;
    }
    let byte = match event.key {
        2..=11 => b"1234567890"[(event.key - 2) as usize],
        12 => b'-',
        13 => b'=',
        14 => 8,
        15 => 9,
        16..=25 => b"qwertyuiop"[(event.key - 16) as usize],
        26 => b'[',
        27 => b']',
        28 => b'\n',
        30..=38 => b"asdfghjkl"[(event.key - 30) as usize],
        39 => b';',
        40 => b'\'',
        41 => b'`',
        43 => b'\\',
        44..=50 => b"zxcvbnm"[(event.key - 44) as usize],
        51 => b',',
        52 => b'.',
        53 => b'/',
        57 => b' ',
        _ => return 0,
    };
    if byte.is_ascii_alphabetic() {
        return u32::from(if (event.modifiers & 1 != 0) ^ (event.modifiers & 8 != 0) {
            byte.to_ascii_uppercase()
        } else {
            byte
        });
    }
    if event.modifiers & 1 != 0
        && let Some(index) = b"1234567890-=[];'`\\,./".iter().position(|b| *b == byte)
    {
        return u32::from(b"!@#$%^&*()_+{}:\"~|<>?"[index]);
    }
    u32::from(byte)
}
