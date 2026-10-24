//! Decode bounded set-1 keyboard and three-byte PS/2 mouse input without hardware access.
//!
//! This crate allocates nothing and has no interrupt or synchronization assumptions. A caller
//! serializes access and supplies device bytes after removing command responses and transport
//! errors. The controller must supply translated or native scan-code set 1 and a standard ID-0
//! mouse. Physical key events carry no text, keyboard layout, focus or application routing policy.
//! A diagnostic consumer may inspect events; a future user-space server owns those policies.
//!
//! Mouse bit 3 identifies a packet header only while searching for a header: data bytes may also
//! have that bit set. Lost bytes cannot always be detected from the wire format. The hardware
//! owner must call `reset(DeviceError)` after transport errors or an incomplete-packet timeout.
//! Every reset invalidates all held keys/buttons, discards queued history and preserves position.
//!
//! Protocol references: [QEMU 8.2.2 PS/2 device model](https://github.com/qemu/qemu/blob/v8.2.2/hw/input/ps2.c)
//! and [Microsoft's standard PS/2 packet format](https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/keyboard-and-mouse-hid-client-drivers).
//!
//! SPDX-License-Identifier: Apache-2.0

#![no_std]

/// A physical set-1 key; `extended` distinguishes an E0 key from the base keypad key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalKey {
    pub code: u8,
    pub extended: bool,
}

impl PhysicalKey {
    /// Pause has an E1 sequence, no break code and no persistent pressed state.
    pub const PAUSE: Self = Self {
        code: 0x45,
        extended: true,
    };

    /// Stable diagnostic number, not a public GUI ABI or a character value.
    pub const fn number(self) -> u16 {
        self.code as u16 + if self.extended { 256 } else { 0 }
    }

    fn index(self) -> usize {
        self.code as usize + if self.extended { 128 } else { 0 }
    }
}

/// Repeated make codes are explicit; releases never produce text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyState {
    Pressed,
    Released,
    Repeat,
}

/// Current modifier state after the associated physical key transition.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn shift(self) -> bool {
        self.0 & 1 != 0
    }

    pub const fn control(self) -> bool {
        self.0 & 2 != 0
    }

    pub const fn alt(self) -> bool {
        self.0 & 4 != 0
    }

    pub const fn caps_lock(self) -> bool {
        self.0 & 8 != 0
    }
}

/// One physical transition; interpreting this as text requires a separate layout policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyEvent {
    pub key: PhysicalKey,
    pub state: KeyState,
    pub modifiers: Modifiers,
}

/// Unsupported scan code, broken special sequence or invalid mouse header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    InvalidKeyboardCode,
    InvalidPauseSequence,
    InvalidMouseHeader,
}

/// State of a set-1 decoder. No key-map or Unicode translation is performed.
#[derive(Clone, Copy)]
pub struct Keyboard {
    held: [bool; 256],
    extended: bool,
    pause_index: usize,
    caps_lock: bool,
}

impl Keyboard {
    pub const fn new() -> Self {
        Self {
            held: [false; 256],
            extended: false,
            pause_index: 0,
            caps_lock: false,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn is_pressed(&self, key: PhysicalKey) -> bool {
        key.code < 128 && self.held[key.index()]
    }

    pub const fn has_partial_sequence(&self) -> bool {
        self.extended || self.pause_index != 0
    }

    pub fn modifiers(&self) -> Modifiers {
        let shift = self.held[0x2a] || self.held[0x36];
        let control = self.held[0x1d] || self.held[128 + 0x1d];
        let alt = self.held[0x38] || self.held[128 + 0x38];
        Modifiers(
            shift as u8 | (control as u8) << 1 | (alt as u8) << 2 | (self.caps_lock as u8) << 3,
        )
    }

    /// Accepts the 101/102-key base range and documented E0 navigation/modifier/GUI keys.
    /// Canonical Pause emits one `Pressed` event but never stays held: the device has no break
    /// sequence. PrintScreen's E0 fake Shift bytes are ignored, including navigation fake shifts.
    /// On error the standalone decoder drops its prefix; its owner must invalidate held state.
    pub fn feed(&mut self, byte: u8) -> Result<Option<KeyEvent>, DecodeError> {
        const PAUSE: [u8; 5] = [0x1d, 0x45, 0xe1, 0x9d, 0xc5];
        if self.pause_index != 0 {
            let index = self.pause_index - 1;
            if byte != PAUSE[index] {
                self.pause_index = 0;
                self.extended = false;
                return Err(DecodeError::InvalidPauseSequence);
            }
            self.pause_index += 1;
            if self.pause_index <= PAUSE.len() {
                return Ok(None);
            }
            self.pause_index = 0;
            return Ok(Some(KeyEvent {
                key: PhysicalKey::PAUSE,
                state: KeyState::Pressed,
                modifiers: self.modifiers(),
            }));
        }
        if byte == 0xe1 && !self.extended {
            self.pause_index = 1;
            return Ok(None);
        }
        if byte == 0xe0 && !self.extended {
            self.extended = true;
            return Ok(None);
        }
        let extended = core::mem::take(&mut self.extended);
        let code = byte & 0x7f;
        if extended && matches!(code, 0x2a | 0x36) {
            return Ok(None);
        }
        let valid = if extended {
            matches!(code, 0x1c | 0x1d | 0x35 | 0x37 | 0x38 | 0x46..=0x49 | 0x4b | 0x4d
                | 0x4f..=0x53 | 0x5b..=0x5f | 0x63)
        } else {
            matches!(code, 0x01..=0x54 | 0x56..=0x58)
        };
        if !valid {
            return Err(DecodeError::InvalidKeyboardCode);
        }
        // Alt+PrintScreen is reported as base SysRq, but names the same physical key.
        let key = if !extended && code == 0x54 {
            PhysicalKey {
                code: 0x37,
                extended: true,
            }
        } else {
            PhysicalKey { code, extended }
        };
        let held = &mut self.held[key.index()];
        let state = if byte & 0x80 != 0 {
            if !*held {
                return Ok(None);
            }
            *held = false;
            KeyState::Released
        } else if *held {
            KeyState::Repeat
        } else {
            *held = true;
            KeyState::Pressed
        };
        if !extended && code == 0x3a && state == KeyState::Pressed {
            self.caps_lock = !self.caps_lock;
        }
        Ok(Some(KeyEvent {
            key,
            state,
            modifiers: self.modifiers(),
        }))
    }
}

impl Default for Keyboard {
    fn default() -> Self {
        Self::new()
    }
}

/// Three standard buttons, with PS/2 bit positions retained for diagnostics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Buttons(u8);

impl Buttons {
    pub const LEFT: u8 = 1;
    pub const RIGHT: u8 = 2;
    pub const MIDDLE: u8 = 4;

    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// A complete wire packet. Positive Y points upward; the dispatcher converts to screen Y.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MousePacket {
    pub dx: i16,
    pub dy: i16,
    pub buttons: Buttons,
    pub changed: Buttons,
    /// Saturated device motion is discarded on both axes while preserving button transitions.
    pub overflow: bool,
}

/// Standard ID-0 PS/2 parser; wheel and extended-button modes are deliberately unsupported.
#[derive(Clone, Copy)]
pub struct Mouse {
    bytes: [u8; 3],
    length: usize,
    buttons: Buttons,
}

impl Mouse {
    pub const fn new() -> Self {
        Self {
            bytes: [0; 3],
            length: 0,
            buttons: Buttons(0),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub const fn has_partial_packet(&self) -> bool {
        self.length != 0
    }

    pub fn feed(&mut self, byte: u8) -> Result<Option<MousePacket>, DecodeError> {
        if self.length == 0 && byte & 8 == 0 {
            return Err(DecodeError::InvalidMouseHeader);
        }
        self.bytes[self.length] = byte;
        self.length += 1;
        if self.length != 3 {
            return Ok(None);
        }
        self.length = 0;
        let header = self.bytes[0];
        let overflow = header & 0xc0 != 0;
        let buttons = Buttons(header & 7);
        let changed = Buttons(buttons.0 ^ self.buttons.0);
        self.buttons = buttons;
        // The header supplies bit 8. Casting only the data byte to i8 loses +/-256 semantics.
        let axis = |value: u8, negative: bool| value as i16 - if negative { 256 } else { 0 };
        Ok(Some(MousePacket {
            dx: if overflow {
                0
            } else {
                axis(self.bytes[1], header & 0x10 != 0)
            },
            dy: if overflow {
                0
            } else {
                axis(self.bytes[2], header & 0x20 != 0)
            },
            buttons,
            changed,
            overflow,
        }))
    }
}

impl Default for Mouse {
    fn default() -> Self {
        Self::new()
    }
}

/// Clipped screen-space state. Values returned by a dispatcher are independent snapshots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pointer {
    pub x: i32,
    pub y: i32,
    pub buttons: Buttons,
    width: i32,
    height: i32,
}

impl Pointer {
    pub const fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return None;
        }
        Some(Self {
            x: (width / 2) as i32,
            y: (height / 2) as i32,
            buttons: Buttons(0),
            width: width as i32,
            height: height as i32,
        })
    }

    pub fn move_by(&mut self, dx: i16, dy: i16) {
        self.x = (self.x as i64 + dx as i64).clamp(0, self.width as i64 - 1) as i32;
        self.y = (self.y as i64 + dy as i64).clamp(0, self.height as i64 - 1) as i32;
    }
}

/// Screen-space movement and button state; no focus or target window is assigned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointerEvent {
    pub x: i32,
    pub y: i32,
    pub dx: i16,
    pub dy: i16,
    pub buttons: Buttons,
    pub changed: Buttons,
    pub overflow: bool,
}

/// Consumers must release every held key/button after any reset event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResetReason {
    QueueOverflow,
    MalformedInput,
    DeviceError,
    ClientDisconnected,
    ClientConnected,
    Explicit,
}

/// Internal diagnostic delivery contract; not a kernel syscall or frozen GUI ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    Key(KeyEvent),
    Pointer(PointerEvent),
    Reset(ResetReason),
}

/// Serial bounded dispatcher. The caller drains it frequently and owns hardware lifecycle.
///
/// A full queue discards all buffered events plus the triggering event, resets device state and
/// leaves one `Reset(QueueOverflow)`. Later events follow that reset in order. Pointer position is
/// preserved so consumers can query it to recover, and buttons are released. Disconnect applies
/// the same invalidation, then ignores incoming bytes until connect. Capacity zero is rejected.
pub struct Dispatcher<const N: usize> {
    keyboard: Keyboard,
    mouse: Mouse,
    pointer: Pointer,
    events: [Option<Event>; N],
    head: usize,
    length: usize,
    connected: bool,
}

impl<const N: usize> Dispatcher<N> {
    pub const fn new(width: u32, height: u32) -> Option<Self> {
        if N == 0 {
            return None;
        }
        let Some(pointer) = Pointer::new(width, height) else {
            return None;
        };
        Some(Self {
            keyboard: Keyboard::new(),
            mouse: Mouse::new(),
            pointer,
            events: [None; N],
            head: 0,
            length: 0,
            connected: true,
        })
    }

    pub const fn pointer(&self) -> Pointer {
        self.pointer
    }

    pub const fn is_connected(&self) -> bool {
        self.connected
    }

    pub const fn mouse_has_partial_packet(&self) -> bool {
        self.mouse.has_partial_packet()
    }

    pub const fn keyboard_has_partial_sequence(&self) -> bool {
        self.keyboard.has_partial_sequence()
    }

    pub const fn has_partial(&self) -> bool {
        self.keyboard.has_partial_sequence() || self.mouse.has_partial_packet()
    }

    pub fn feed_keyboard(&mut self, byte: u8) {
        if !self.connected {
            return;
        }
        match self.keyboard.feed(byte) {
            Ok(Some(event)) => self.push(Event::Key(event)),
            Ok(None) => {}
            Err(_) => self.reset(ResetReason::MalformedInput),
        }
    }

    pub fn feed_mouse(&mut self, byte: u8) {
        if !self.connected {
            return;
        }
        match self.mouse.feed(byte) {
            Ok(Some(packet)) => {
                let dy = -packet.dy;
                self.pointer.move_by(packet.dx, dy);
                self.pointer.buttons = packet.buttons;
                self.push(Event::Pointer(PointerEvent {
                    x: self.pointer.x,
                    y: self.pointer.y,
                    dx: packet.dx,
                    dy,
                    buttons: packet.buttons,
                    changed: packet.changed,
                    overflow: packet.overflow,
                }));
            }
            Ok(None) => {}
            Err(_) => self.reset(ResetReason::MalformedInput),
        }
    }

    pub fn pop_event(&mut self) -> Option<Event> {
        if self.length == 0 {
            return None;
        }
        let event = self.events[self.head].take();
        self.head = (self.head + 1) % N;
        self.length -= 1;
        event
    }

    pub fn reset(&mut self, reason: ResetReason) {
        self.keyboard.reset();
        self.mouse.reset();
        self.pointer.buttons = Buttons(0);
        self.events = [None; N];
        self.events[0] = Some(Event::Reset(reason));
        self.head = 0;
        self.length = 1;
    }

    pub fn disconnect(&mut self) {
        self.connected = false;
        self.reset(ResetReason::ClientDisconnected);
    }

    pub fn connect(&mut self) {
        self.connected = true;
        self.reset(ResetReason::ClientConnected);
    }

    fn push(&mut self, event: Event) {
        if self.length == N {
            self.reset(ResetReason::QueueOverflow);
            return;
        }
        self.events[(self.head + self.length) % N] = Some(event);
        self.length += 1;
    }
}

#[cfg(test)]
mod tests;
