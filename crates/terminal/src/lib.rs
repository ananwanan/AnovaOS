// SPDX-License-Identifier: Apache-2.0
//! Parse a bounded ANSI subset into an ASCII cell grid and a fixed 256-line history.
//! Chunk boundaries never affect parsing. Resize clips/pads without reflow, unsupported
//! control strings are consumed without allocation, and non-ASCII text uses '?' cells.

#![no_std]

pub const COLUMNS: usize = 52;
pub const ROWS: usize = 26;
pub const HISTORY: usize = 256;
pub const PALETTE: [u32; 16] = [
    0x101820, 0xcc5555, 0x66bb77, 0xccbb66, 0x6699cc, 0xbb77bb, 0x66bbbb, 0xdde5ed, 0x667788,
    0xff7777, 0x88ee99, 0xffee88, 0x88bbff, 0xee99ee, 0x88eeee, 0xffffff,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cell {
    pub byte: u8,
    pub foreground: u8,
    pub background: u8,
}
impl Cell {
    const EMPTY: Self = Self {
        byte: b' ',
        foreground: 7,
        background: 0,
    };
}
type Line = [Cell; COLUMNS];

#[derive(Clone, Copy)]
enum Parser {
    Ground,
    Escape,
    Csi,
    String,
    StringEscape,
}

pub struct Screen {
    screen: [Line; ROWS],
    history: [Line; HISTORY],
    head: usize,
    saved_lines: usize,
    dropped: u64,
    view: usize,
    columns: usize,
    rows: usize,
    x: usize,
    y: usize,
    saved: (usize, usize),
    wrap: bool,
    foreground: u8,
    background: u8,
    bold: bool,
    inverse: bool,
    cursor: bool,
    parser: Parser,
    parameters: [u16; 8],
    parameter: usize,
    private: bool,
    invalid: bool,
    continuation: u8,
    pub received: u64,
}

impl Default for Screen {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen {
    pub const fn new() -> Self {
        Self {
            screen: [[Cell::EMPTY; COLUMNS]; ROWS],
            history: [[Cell::EMPTY; COLUMNS]; HISTORY],
            head: 0,
            saved_lines: 0,
            dropped: 0,
            view: 0,
            columns: COLUMNS,
            rows: ROWS,
            x: 0,
            y: 0,
            saved: (0, 0),
            wrap: false,
            foreground: 7,
            background: 0,
            bold: false,
            inverse: false,
            cursor: true,
            parser: Parser::Ground,
            parameters: [0; 8],
            parameter: 0,
            private: false,
            invalid: false,
            continuation: 0,
            received: 0,
        }
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.rows, self.columns)
    }
    pub fn history_len(&self) -> usize {
        self.saved_lines
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    pub fn view_offset(&self) -> usize {
        self.view
    }
    pub fn scroll_view(&mut self, lines: i32) {
        self.view = self
            .view
            .saturating_add_signed(lines as isize)
            .min(self.saved_lines);
    }
    pub fn live_view(&mut self) {
        self.view = 0;
    }

    /// Start a fresh producer without letting an unterminated escape/control string
    /// or old SGR attributes hide the replacement shell. Retain visible/history cells.
    pub fn new_stream(&mut self) {
        self.parser = Parser::Ground;
        self.continuation = 0;
        self.foreground = 7;
        self.background = 0;
        self.bold = false;
        self.inverse = false;
        self.cursor = true;
        self.wrap = false;
        self.view = 0;
    }

    pub fn cell(&self, row: usize, column: usize) -> Cell {
        if row >= self.rows || column >= self.columns {
            return Cell::EMPTY;
        }
        let line = self.saved_lines + row - self.view;
        if line < self.saved_lines {
            self.history[(self.head + line) % HISTORY][column]
        } else {
            self.screen[line - self.saved_lines][column]
        }
    }

    pub fn cursor(&self) -> Option<(usize, usize)> {
        (self.cursor && self.view == 0).then_some((self.y, self.x))
    }

    pub fn resize(&mut self, rows: usize, columns: usize) {
        let rows = rows.clamp(1, ROWS);
        let columns = columns.clamp(1, COLUMNS);
        // No reflow: preserve the top-left intersection, erase clipped/new cells so
        // enlarging a window cannot reveal content previously removed by shrinking it.
        let blank = self.blank();
        for row in 0..ROWS {
            for column in 0..COLUMNS {
                if row >= rows.min(self.rows) || column >= columns.min(self.columns) {
                    self.screen[row][column] = blank;
                }
            }
        }
        self.rows = rows;
        self.columns = columns;
        self.x = self.x.min(columns - 1);
        self.y = self.y.min(rows - 1);
        self.wrap = false;
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.received = self.received.saturating_add(bytes.len() as u64);
        for &byte in bytes {
            self.byte(byte);
        }
    }

    fn blank(&self) -> Cell {
        Cell {
            byte: b' ',
            foreground: self.foreground,
            background: self.background,
        }
    }

    fn newline(&mut self) {
        self.wrap = false;
        if self.y + 1 < self.rows {
            self.y += 1;
            return;
        }
        let slot = (self.head + self.saved_lines) % HISTORY;
        self.history[slot] = self.screen[0];
        if self.saved_lines == HISTORY {
            self.head = (self.head + 1) % HISTORY;
            self.dropped = self.dropped.saturating_add(1);
        } else {
            self.saved_lines += 1;
        }
        if self.view != 0 {
            self.view = (self.view + 1).min(self.saved_lines);
        }
        self.screen.copy_within(1..self.rows, 0);
        self.screen[self.rows - 1] = [self.blank(); COLUMNS];
    }

    fn put(&mut self, byte: u8) {
        if self.wrap {
            self.x = 0;
            self.newline();
        }
        let fg = if self.bold && self.foreground < 8 {
            self.foreground + 8
        } else {
            self.foreground
        };
        let (fg, bg) = if self.inverse {
            (self.background, fg)
        } else {
            (fg, self.background)
        };
        self.screen[self.y][self.x] = Cell {
            byte,
            foreground: fg,
            background: bg,
        };
        if self.x + 1 == self.columns {
            self.wrap = true;
        } else {
            self.x += 1;
        }
    }

    fn byte(&mut self, byte: u8) {
        match self.parser {
            Parser::String => match byte {
                7 => self.parser = Parser::Ground,
                27 => self.parser = Parser::StringEscape,
                _ => {}
            },
            Parser::StringEscape => {
                self.parser = match byte {
                    b'\\' | 7 => Parser::Ground,
                    27 => Parser::StringEscape,
                    _ => Parser::String,
                }
            }
            Parser::Escape => {
                self.parser = Parser::Ground;
                match byte {
                    b'[' => {
                        self.parser = Parser::Csi;
                        self.parameters = [0; 8];
                        self.parameter = 0;
                        self.private = false;
                        self.invalid = false;
                    }
                    b']' | b'P' | b'^' | b'_' => self.parser = Parser::String,
                    b'7' => self.saved = (self.y, self.x),
                    b'8' => self.restore(),
                    27 => self.parser = Parser::Escape,
                    _ => {}
                }
            }
            Parser::Csi => match byte {
                b'0'..=b'9' => {
                    let value = self.parameters[self.parameter] as u32 * 10 + (byte - b'0') as u32;
                    if value > 9999 {
                        self.invalid = true;
                    }
                    self.parameters[self.parameter] = value.min(9999) as u16;
                }
                b';' => {
                    if self.parameter == 7 {
                        self.invalid = true;
                    } else {
                        self.parameter += 1;
                    }
                }
                b'?' if self.parameter == 0 && self.parameters[0] == 0 => self.private = true,
                0x40..=0x7e => {
                    if !self.invalid {
                        self.csi(byte);
                    }
                    self.parser = Parser::Ground;
                }
                27 => self.parser = Parser::Escape,
                24 | 26 => self.parser = Parser::Ground,
                _ => self.invalid = true,
            },
            Parser::Ground => {
                if self.continuation != 0 {
                    if byte & 0xc0 == 0x80 {
                        self.continuation -= 1;
                        return;
                    }
                    self.continuation = 0;
                }
                match byte {
                    27 => self.parser = Parser::Escape,
                    b'\r' => {
                        self.x = 0;
                        self.wrap = false;
                    }
                    b'\n' => self.newline(),
                    8 => {
                        self.x = self.x.saturating_sub(1);
                        self.wrap = false;
                    }
                    9 => {
                        self.x = ((self.x / 8 + 1) * 8).min(self.columns - 1);
                        self.wrap = false;
                    }
                    32..=126 => self.put(byte),
                    0xc2..=0xdf => {
                        self.put(b'?');
                        self.continuation = 1;
                    }
                    0xe0..=0xef => {
                        self.put(b'?');
                        self.continuation = 2;
                    }
                    0xf0..=0xf4 => {
                        self.put(b'?');
                        self.continuation = 3;
                    }
                    0x80..=0xff => self.put(b'?'),
                    _ => {}
                }
            }
        }
    }

    fn restore(&mut self) {
        self.y = self.saved.0.min(self.rows - 1);
        self.x = self.saved.1.min(self.columns - 1);
        self.wrap = false;
    }

    fn csi(&mut self, final_byte: u8) {
        let p = self.parameters;
        let n = usize::from(p[0].max(1));
        if self.private {
            if p[0] == 25 && matches!(final_byte, b'h' | b'l') {
                self.cursor = final_byte == b'h';
            }
            return;
        }
        match final_byte {
            b'A' => self.y = self.y.saturating_sub(n),
            b'B' => self.y = (self.y + n).min(self.rows - 1),
            b'C' => self.x = (self.x + n).min(self.columns - 1),
            b'D' => self.x = self.x.saturating_sub(n),
            b'G' => self.x = (n - 1).min(self.columns - 1),
            b'H' | b'f' => {
                self.y = (n - 1).min(self.rows - 1);
                self.x = usize::from(p[1].max(1) - 1).min(self.columns - 1);
            }
            b'J' if p[0] <= 2 => {
                let cursor = self.y * self.columns + self.x;
                let blank = self.blank();
                for row in 0..self.rows {
                    for column in 0..self.columns {
                        let i = row * self.columns + column;
                        if p[0] == 2 || (p[0] == 0 && i >= cursor) || (p[0] == 1 && i <= cursor) {
                            self.screen[row][column] = blank;
                        }
                    }
                }
            }
            b'K' if p[0] <= 2 => {
                let blank = self.blank();
                for column in 0..self.columns {
                    if p[0] == 2
                        || (p[0] == 0 && column >= self.x)
                        || (p[0] == 1 && column <= self.x)
                    {
                        self.screen[self.y][column] = blank;
                    }
                }
            }
            b's' => self.saved = (self.y, self.x),
            b'u' => self.restore(),
            b'm' => {
                if p[..=self.parameter].iter().any(|p| {
                    !matches!(p,
                    0 | 1 | 7 | 22 | 27 | 30..=37 | 39 | 40..=47 | 49 | 90..=97 | 100..=107)
                }) {
                    return;
                }
                for &p in &p[..=self.parameter] {
                    match p {
                        0 => {
                            self.foreground = 7;
                            self.background = 0;
                            self.bold = false;
                            self.inverse = false;
                        }
                        1 => self.bold = true,
                        22 => self.bold = false,
                        7 => self.inverse = true,
                        27 => self.inverse = false,
                        30..=37 => self.foreground = (p - 30) as u8,
                        40..=47 => self.background = (p - 40) as u8,
                        90..=97 => self.foreground = (p - 90 + 8) as u8,
                        100..=107 => self.background = (p - 100 + 8) as u8,
                        39 => self.foreground = 7,
                        49 => self.background = 0,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if final_byte != b'm' {
            self.wrap = false;
        }
    }
}

#[cfg(test)]
mod tests;
