// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Keep bounded editor text in userspace, with checked line insertion and deletion.
//! This pure module is testable on the host without terminal or filesystem services.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditError {
    Full,
    NoLine,
}

pub struct Text<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> Default for Text<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Text<N> {
    pub const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    pub fn append(&mut self, bytes: &[u8]) -> Result<(), EditError> {
        let end = self
            .len
            .checked_add(bytes.len())
            .filter(|end| *end <= N)
            .ok_or(EditError::Full)?;
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
        Ok(())
    }

    pub fn append_line(&mut self, text: &[u8]) -> Result<(), EditError> {
        if self
            .len
            .checked_add(text.len())
            .and_then(|n| n.checked_add(1))
            .is_none_or(|n| n > N)
        {
            return Err(EditError::Full);
        }
        self.append(text)?;
        self.append(b"\n")
    }

    /// Return the byte range of a 1-based line, including its newline when present.
    fn line(&self, number: usize) -> Result<(usize, usize), EditError> {
        if number == 0 {
            return Err(EditError::NoLine);
        }
        let mut first = 0;
        for index in 1..=number {
            if first >= self.len {
                return Err(EditError::NoLine);
            }
            let end = self.bytes[first..self.len]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(self.len, |i| first + i + 1);
            if index == number {
                return Ok((first, end));
            }
            first = end;
        }
        Err(EditError::NoLine)
    }

    pub fn delete(&mut self, number: usize) -> Result<(), EditError> {
        let (start, end) = self.line(number)?;
        self.bytes.copy_within(end..self.len, start);
        self.len -= end - start;
        Ok(())
    }

    pub fn replace(&mut self, number: usize, text: &[u8]) -> Result<(), EditError> {
        let (start, end) = self.line(number)?;
        let old = end - start;
        let new = text.len().checked_add(1).ok_or(EditError::Full)?;
        let size = self
            .len
            .checked_sub(old)
            .and_then(|n| n.checked_add(new))
            .filter(|size| *size <= N)
            .ok_or(EditError::Full)?;
        if new > old {
            self.bytes.copy_within(end..self.len, end + new - old);
        } else {
            self.bytes.copy_within(end..self.len, start + new);
        }
        self.bytes[start..start + text.len()].copy_from_slice(text);
        self.bytes[start + text.len()] = b'\n';
        self.len = size;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_lines_and_capacity_rollback() {
        let mut text = Text::<12>::new();
        text.append_line(b"abc").unwrap();
        text.append_line(b"de").unwrap();
        assert_eq!(text.bytes(), b"abc\nde\n");
        assert_eq!(text.append_line(b"123456"), Err(EditError::Full));
        assert_eq!(text.bytes(), b"abc\nde\n");
        text.replace(1, b"x").unwrap();
        assert_eq!(text.bytes(), b"x\nde\n");
        text.delete(2).unwrap();
        assert_eq!(text.bytes(), b"x\n");
        assert_eq!(text.delete(2), Err(EditError::NoLine));
        assert_eq!(text.replace(0, b""), Err(EditError::NoLine));
    }

    #[test]
    fn replace_shorter_longer_and_nonterminated_file() {
        let mut text = Text::<16>::new();
        text.append(b"a\nb").unwrap();
        text.replace(2, b"more").unwrap();
        assert_eq!(text.bytes(), b"a\nmore\n");
        text.replace(1, b"").unwrap();
        assert_eq!(text.bytes(), b"\nmore\n");
        assert_eq!(text.replace(2, b"0123456789abcdef"), Err(EditError::Full));
        assert_eq!(text.bytes(), b"\nmore\n");
    }
}
