// SPDX-License-Identifier: Apache-2.0
//! Provide bounded canonical/raw line disciplines for COM1 and four PTY pairs.
//! Buffers allocate fallibly at initialization; UART polling and foreground events never allocate.
use crate::serial;
use anova_abi::{IO_ERROR, WOULD_BLOCK};
use anova_vfs::{Device, Vfs};
#[derive(Clone, Copy)]
pub enum TerminalSignal {
    Interrupt,
    Quit,
    Suspend,
    WindowChanged,
    Hangup,
    Continue,
}
impl TerminalSignal {
    fn bit(self) -> u64 {
        1u64 << (self as u32)
    }
    fn from_index(index: u32) -> Self {
        match index {
            0 => Self::Interrupt,
            1 => Self::Quit,
            2 => Self::Suspend,
            3 => Self::WindowChanged,
            4 => Self::Hangup,
            5 => Self::Continue,
            _ => unreachable!("Invalid terminal event"),
        }
    }
}
struct Queue {
    bytes: [u8; 1024],
    head: usize,
    count: usize,
}
impl Queue {
    const fn new() -> Self {
        Self {
            bytes: [0; 1024],
            head: 0,
            count: 0,
        }
    }
    fn push(&mut self, b: u8) -> bool {
        if self.count == 1024 {
            return false;
        }
        self.bytes[(self.head + self.count) % 1024] = b;
        self.count += 1;
        true
    }
    fn read(&mut self, output: &mut [u8]) -> usize {
        let n = output.len().min(self.count);
        for b in &mut output[..n] {
            *b = self.bytes[self.head];
            self.head = (self.head + 1) % 1024;
            self.count -= 1;
        }
        n
    }
}
struct Tty {
    line: [u8; 256],
    length: usize,
    input: Queue,
    output: Queue,
    eof: bool,
    last_cr: bool,
    termios: [u8; 36],
    foreground: u64,
    session: u64,
    window: [u8; 8],
}
impl Tty {
    const fn new() -> Self {
        let mut settings = [0; 36];
        settings[0] = 0;
        settings[1] = 5;
        settings[4] = 5;
        settings[8] = 0xbf;
        settings[12] = 0x3b;
        settings[13] = 0x8a;
        settings[17] = 3;
        settings[18] = 28;
        settings[19] = 127;
        settings[20] = 21;
        settings[21] = 4;
        settings[23] = 1;
        settings[27] = 26;
        Self {
            line: [0; 256],
            length: 0,
            input: Queue::new(),
            output: Queue::new(),
            eof: false,
            last_cr: false,
            termios: settings,
            foreground: 0,
            session: 0,
            window: [24, 0, 80, 0, 0, 0, 0, 0],
        }
    }
    fn echo(&mut self, console: bool, bytes: &[u8]) {
        if console {
            serial::bytes(bytes);
        } else {
            for b in bytes {
                if *b == b'\n' && self.termios[4] & 5 == 5 {
                    self.output.push(b'\r');
                }
                self.output.push(*b);
            }
        }
    }
    fn feed(&mut self, b: u8, console: bool) -> Option<TerminalSignal> {
        let canonical = self.termios[12] & 2 != 0;
        let echo = self.termios[12] & 8 != 0;
        if self.termios[12] & 1 != 0 {
            let signal = if b == self.termios[17] {
                Some(TerminalSignal::Interrupt)
            } else if b == self.termios[18] {
                Some(TerminalSignal::Quit)
            } else if b == self.termios[27] {
                Some(TerminalSignal::Suspend)
            } else {
                None
            };
            if let Some(signal) = signal {
                self.length = 0;
                self.input = Queue::new();
                if echo {
                    self.echo(console, b"^C\n");
                }
                return Some(signal);
            }
        }
        if !canonical {
            self.input.push(b);
            if echo {
                self.echo(console, &[b]);
            }
            return None;
        }
        if b == b'\n' && self.last_cr {
            self.last_cr = false;
            return None;
        }
        self.last_cr = b == b'\r';
        match b {
            8 | 127 if self.length > 0 => {
                self.length -= 1;
                if echo {
                    self.echo(console, b"\x08 \x08");
                }
            }
            b'\r' | b'\n' | 4 => {
                if b == 4 && self.length == 0 {
                    self.eof = true;
                    return None;
                }
                if b != 4 {
                    self.line[self.length] = b'\n';
                    self.length += 1;
                    if echo {
                        self.echo(console, b"\n");
                    }
                }
                for n in 0..self.length {
                    self.input.push(self.line[n]);
                }
                self.length = 0;
            }
            b'\t' | 32..=126 if self.length < 255 => {
                self.line[self.length] = b;
                self.length += 1;
                if echo {
                    self.echo(console, &[b]);
                }
            }
            _ => {}
        }
        None
    }
}
pub struct Terminal {
    ttys: alloc::vec::Vec<Tty>,
    signals: [u64; 5],
    masters: [bool; 4],
}
impl Terminal {
    pub fn new() -> Result<Self, i64> {
        let mut ttys = alloc::vec::Vec::new();
        ttys.try_reserve_exact(5)
            .map_err(|_| anova_abi::NO_MEMORY)?;
        ttys.resize_with(5, Tty::new);
        Ok(Self {
            ttys,
            signals: [0; 5],
            masters: [false; 4],
        })
    }
    pub fn is_terminal(&self, d: Device) -> bool {
        matches!(
            d,
            Device::Console | Device::PtyMaster(_) | Device::PtySlave(_)
        )
    }
    fn index(d: Device) -> usize {
        match d {
            Device::PtyMaster(n) | Device::PtySlave(n) => n + 1,
            _ => 0,
        }
    }
    pub fn reset_pty(&mut self, n: usize) {
        self.ttys[n + 1] = Tty::new();
        self.signals[n + 1] = 0;
        self.masters[n] = false;
    }
    pub fn session(&self, d: Device) -> u64 {
        self.ttys[Self::index(d)].session
    }
    pub fn controlling_pty(&self, session: u64) -> Option<usize> {
        self.ttys[1..]
            .iter()
            .position(|tty| session != 0 && tty.session == session)
    }
    pub fn attach(&mut self, d: Device, session: u64, group: u64) {
        let tty = &mut self.ttys[Self::index(d)];
        tty.session = session;
        tty.foreground = group;
    }
    pub fn set_foreground(&mut self, group: u64) {
        self.ttys[0].foreground = group;
    }
    pub fn foreground(&self, d: Device) -> u64 {
        self.ttys[Self::index(d)].foreground
    }
    pub fn set_group(&mut self, d: Device, group: u64) {
        self.ttys[Self::index(d)].foreground = group;
    }
    pub fn settings(&self, d: Device) -> [u8; 36] {
        self.ttys[Self::index(d)].termios
    }
    pub fn set_settings(&mut self, d: Device, settings: [u8; 36], flush: bool) {
        let tty = &mut self.ttys[Self::index(d)];
        tty.termios = settings;
        if flush {
            tty.length = 0;
            tty.input = Queue::new();
            tty.eof = false;
        }
    }
    pub fn window(&self, d: Device) -> [u8; 8] {
        self.ttys[Self::index(d)].window
    }
    pub fn set_window(&mut self, d: Device, window: [u8; 8]) {
        let tty = &mut self.ttys[Self::index(d)];
        if tty.window != window {
            tty.window = window;
            self.signals[Self::index(d)] |= TerminalSignal::WindowChanged.bit();
        }
    }
    pub fn refresh(&mut self, vfs: &Vfs<'_>) {
        for n in 0..4 {
            let now = vfs.pty_present(n, true);
            if self.masters[n] && !now {
                self.signals[n + 1] |=
                    TerminalSignal::Hangup.bit() | TerminalSignal::Continue.bit();
            }
            self.masters[n] = now;
        }
    }
    pub fn take_signal(&mut self) -> Option<(u64, TerminalSignal)> {
        for (n, bits) in self.signals.iter_mut().enumerate() {
            if *bits != 0 {
                let signal = TerminalSignal::from_index(bits.trailing_zeros());
                *bits &= !signal.bit();
                return Some((self.ttys[n].foreground, signal));
            }
        }
        None
    }
    pub fn poll(&mut self) {
        let tty = &mut self.ttys[0];
        if tty.input.count > 768 || tty.eof {
            return;
        }
        for _ in 0..64 {
            let Some(b) = serial::try_read() else {
                break;
            };
            if let Some(signal) = tty.feed(b, true) {
                self.signals[0] |= signal.bit();
            }
            if tty.input.count > 768 || tty.eof {
                break;
            }
        }
    }
    pub fn endpoint_ready(&self, d: Device, writing: bool, vfs: &Vfs<'_>) -> bool {
        let tty = &self.ttys[Self::index(d)];
        match d {
            Device::PtyMaster(n) if !writing => tty.output.count > 0 || !vfs.pty_present(n, false),
            Device::PtyMaster(_) => tty.input.count < 768,
            Device::PtySlave(n) if writing => tty.output.count < 1024 || !vfs.pty_present(n, true),
            _ => {
                writing
                    || tty.input.count > 0
                    || tty.eof
                    || matches!(d,Device::PtySlave(n) if !vfs.pty_present(n,true))
            }
        }
    }
    pub fn read(&mut self, d: Device, output: &mut [u8], vfs: &Vfs<'_>) -> Result<usize, i64> {
        if output.is_empty() {
            return Ok(0);
        }
        let tty = &mut self.ttys[Self::index(d)];
        if let Device::PtyMaster(n) = d {
            if tty.output.count > 0 {
                return Ok(tty.output.read(output));
            }
            return Err(if vfs.pty_present(n, false) {
                WOULD_BLOCK
            } else {
                IO_ERROR
            });
        }
        if tty.input.count > 0 {
            return Ok(tty.input.read(output));
        }
        if tty.eof {
            tty.eof = false;
            return Ok(0);
        }
        if matches!(d,Device::PtySlave(n) if !vfs.pty_present(n,true)) {
            return Ok(0);
        }
        Err(WOULD_BLOCK)
    }
    pub fn write(&mut self, d: Device, bytes: &[u8], vfs: &Vfs<'_>) -> Result<usize, i64> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if d == Device::Console {
            serial::bytes(bytes);
            return Ok(bytes.len());
        }
        let tty = &mut self.ttys[Self::index(d)];
        let mut count = 0;
        if matches!(d, Device::PtyMaster(_)) {
            for b in bytes {
                if tty.input.count > 768
                    || tty.eof
                    || (tty.termios[12] & 8 != 0 && tty.output.count > 1016)
                {
                    break;
                }
                if let Some(signal) = tty.feed(*b, false) {
                    self.signals[Self::index(d)] |= signal.bit();
                }
                count += 1;
            }
        } else {
            if matches!(d,Device::PtySlave(n) if !vfs.pty_present(n,true)) {
                return Err(IO_ERROR);
            }
            for b in bytes {
                if tty.output.count > 1022 {
                    break;
                }
                if *b == b'\n' && tty.termios[4] & 5 == 5 {
                    tty.output.push(b'\r');
                }
                tty.output.push(*b);
                count += 1;
            }
        }
        if count == 0 {
            Err(WOULD_BLOCK)
        } else {
            Ok(count)
        }
    }
}
