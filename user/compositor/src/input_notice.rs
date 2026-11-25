// SPDX-License-Identifier: Apache-2.0
//! Read a bounded boot snapshot once and show USB failures without requiring input or COM1.
//! Preserve the first USB identity and latest lines; this is not a live runtime log.
use anova_gui_runtime::render::Canvas;
use anova_rt::{abi::OPEN_READ, v1};

const ROWS: usize = 12;
const COLUMNS: usize = 126;
const HEIGHT: u32 = 160;

pub struct InputNotice {
    lines: [[u8; COLUMNS]; ROWS],
    lengths: [usize; ROWS],
    count: usize,
    unavailable: bool,
}

impl InputNotice {
    pub fn load() -> Self {
        let mut notice = Self {
            lines: [[0; COLUMNS]; ROWS],
            lengths: [0; ROWS],
            count: 0,
            unavailable: false,
        };
        let Ok(mut file) = v1::open("/boot.log", OPEN_READ) else {
            notice.unavailable = true;
            return notice;
        };
        let mut chunk = [0; 512];
        let mut line = [0; COLUMNS];
        let mut length = 0;
        let mut truncated = false;
        let mut remaining = 32 * 1024;
        while remaining != 0 {
            let limit = remaining.min(chunk.len());
            let count = match file.read(&mut chunk[..limit]) {
                Ok(0) => break,
                Ok(count) => count,
                Err(_) => {
                    notice.unavailable = true;
                    break;
                }
            };
            remaining -= count;
            for &byte in &chunk[..count] {
                if byte == b'\n' {
                    notice.push(&line[..length], truncated);
                    length = 0;
                    truncated = false;
                } else if byte != b'\r' {
                    if length < COLUMNS {
                        line[length] = if byte.is_ascii_graphic() || byte == b' ' {
                            byte
                        } else {
                            b'?'
                        };
                        length += 1;
                    } else {
                        truncated = true;
                    }
                }
            }
        }
        notice.push(&line[..length], truncated);
        if file.close().is_err() {
            notice.unavailable = true;
        }
        notice
    }

    fn push(&mut self, line: &[u8], truncated: bool) {
        if !line.starts_with(b"[USB:") {
            return;
        }
        if self.count == ROWS {
            self.lines.copy_within(2..ROWS, 1);
            self.lengths.copy_within(2..ROWS, 1);
            self.count -= 1;
        }
        self.lines[self.count][..line.len()].copy_from_slice(line);
        self.lengths[self.count] = line.len();
        if truncated {
            self.lines[self.count][COLUMNS - 3..].copy_from_slice(b"...");
        }
        self.count += 1;
    }

    pub fn draw(&self, frame: &mut [u32], width: u32) {
        let height = (frame.len() / width as usize) as u32;
        let rows = HEIGHT.min(height);
        let start = ((height - rows) * width) as usize;
        let pixels = &mut frame[start..];
        pixels.fill(0x102030);
        let mut canvas = Canvas {
            pixels,
            width,
            height: rows,
        };
        canvas.text(
            8,
            8,
            "USB BOOT DIAGNOSTICS V1 - BOOT SNAPSHOT, NOT LIVE",
            0xffffff,
        );
        canvas.text(
            8,
            20,
            "Connect keyboard and mouse before boot. Photograph this panel.",
            0xa8c8e8,
        );
        if self.count == 0 {
            canvas.text(
                8,
                34,
                if self.unavailable {
                    "Unable to read /boot.log. Record this screen and the machine model."
                } else {
                    "No USB boot messages. Check that the USB desktop image was used."
                },
                0xffffff,
            );
        }
        for index in 0..self.count {
            // Every stored byte is printable ASCII, including replacement/truncation markers.
            let text = core::str::from_utf8(&self.lines[index][..self.lengths[index]])
                .unwrap_or("USB boot line unavailable");
            canvas.text(8, 34 + index as i32 * 10, text, 0xffffff);
        }
    }
}
