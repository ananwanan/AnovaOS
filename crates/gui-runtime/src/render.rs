//! Render clipped RGB primitives and original five-by-seven bitmap text into caller storage.
//! UTF-8 is validated before rendering; unsupported scalars use a visible replacement box.
//!
//! SPDX-License-Identifier: Apache-2.0

use crate::GuiRect;

/// Low-level CPU rasterizer for private, width*height-sized caller storage. This Rust
/// utility grants no window/transport authority and does not extend the frozen C ABI.
pub struct Canvas<'a> {
    pub pixels: &'a mut [u32],
    pub width: u32,
    pub height: u32,
}

impl Canvas<'_> {
    pub fn rect(&mut self, rect: GuiRect, color: u32) {
        let left = i64::from(rect.x).clamp(0, i64::from(self.width)) as usize;
        let top = i64::from(rect.y).clamp(0, i64::from(self.height)) as usize;
        let right =
            (i64::from(rect.x) + i64::from(rect.width)).clamp(0, i64::from(self.width)) as usize;
        let bottom =
            (i64::from(rect.y) + i64::from(rect.height)).clamp(0, i64::from(self.height)) as usize;
        for y in top..bottom {
            self.pixels[y * self.width as usize + left..y * self.width as usize + right]
                .fill(color);
        }
    }

    pub fn text(&mut self, x: i32, y: i32, text: &str, color: u32) {
        let mut column = i64::from(x);
        let mut row = i64::from(y);
        for character in text.chars() {
            if character == '\n' {
                column = i64::from(x);
                row += 9;
                continue;
            }
            let bitmap = glyph(character);
            for (dy, bits) in bitmap.iter().enumerate() {
                for dx in 0..5 {
                    let px = column + dx;
                    let py = row + dy as i64;
                    if bits & (16 >> dx) != 0
                        && px >= 0
                        && py >= 0
                        && px < i64::from(self.width)
                        && py < i64::from(self.height)
                    {
                        self.pixels[py as usize * self.width as usize + px as usize] = color;
                    }
                }
            }
            column += 6;
        }
    }
}

/// These small, independently authored bit patterns carry no third-party font data.
fn glyph(character: char) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 25, 21, 19, 19, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 27, 17],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        ' ' => [0; 7],
        '.' => [0, 0, 0, 0, 0, 6, 6],
        ',' => [0, 0, 0, 0, 6, 4, 8],
        ':' => [0, 6, 6, 0, 6, 6, 0],
        ';' => [0, 6, 6, 0, 6, 4, 8],
        '!' => [4, 4, 4, 4, 4, 0, 4],
        '?' => [14, 17, 1, 2, 4, 0, 4],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '=' => [0, 0, 31, 0, 31, 0, 0],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        '\\' => [16, 8, 8, 4, 2, 2, 1],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '[' => [14, 8, 8, 8, 8, 8, 14],
        ']' => [14, 2, 2, 2, 2, 2, 14],
        '{' => [2, 4, 4, 8, 4, 4, 2],
        '}' => [8, 4, 4, 2, 4, 4, 8],
        '<' => [1, 2, 4, 8, 4, 2, 1],
        '>' => [16, 8, 4, 2, 4, 8, 16],
        '|' => [4, 4, 4, 4, 4, 4, 4],
        '\'' => [4, 4, 2, 0, 0, 0, 0],
        '"' => [10, 10, 5, 0, 0, 0, 0],
        '*' => [0, 21, 14, 31, 14, 21, 0],
        '#' => [10, 10, 31, 10, 31, 10, 10],
        '%' => [25, 25, 2, 4, 8, 19, 19],
        '&' => [12, 18, 20, 8, 21, 18, 13],
        '@' => [14, 17, 23, 21, 23, 16, 14],
        '$' => [4, 15, 20, 14, 5, 30, 4],
        '^' => [4, 10, 17, 0, 0, 0, 0],
        '~' => [0, 0, 9, 22, 0, 0, 0],
        '`' => [8, 4, 2, 0, 0, 0, 0],
        _ => [31, 17, 21, 17, 21, 17, 31],
    }
}
