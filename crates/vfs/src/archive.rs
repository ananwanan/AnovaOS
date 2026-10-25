// SPDX-License-Identifier: Apache-2.0
//! Decode the project-owned ANOVAFS1 archive using checked byte ranges, never casts.
//! Entries are parent-first absolute UTF-8 paths; payloads remain borrowed from boot RAM.
use anova_abi::INVALID_ARGUMENT;

pub const MAGIC: &[u8; 8] = b"ANOVAFS1";
pub const FILE: u8 = 1;
pub const DIRECTORY: u8 = 2;
pub const SYMLINK: u8 = 3;
pub const EXECUTABLE: u8 = 4;
pub const LINUX_EXECUTABLE: u8 = 5;

pub struct Entry<'a> {
    pub kind: u8,
    pub path: &'a str,
    pub data: &'a [u8],
}
pub struct Archive<'a> {
    bytes: &'a [u8],
    cursor: usize,
    remaining: usize,
}
impl<'a> Archive<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, i64> {
        if bytes.len() < 12 || &bytes[..8] != MAGIC {
            return Err(INVALID_ARGUMENT);
        }
        let count = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        if count > 2048 {
            return Err(INVALID_ARGUMENT);
        }
        Ok(Self {
            bytes,
            cursor: 12,
            remaining: count,
        })
    }
    fn take(&mut self, len: usize) -> Result<&'a [u8], i64> {
        let end = self.cursor.checked_add(len).ok_or(INVALID_ARGUMENT)?;
        let bytes = self.bytes.get(self.cursor..end).ok_or(INVALID_ARGUMENT)?;
        self.cursor = end;
        Ok(bytes)
    }
    pub fn entry(&mut self) -> Result<Option<Entry<'a>>, i64> {
        if self.remaining == 0 {
            return if self.cursor == self.bytes.len() {
                Ok(None)
            } else {
                Err(INVALID_ARGUMENT)
            };
        }
        let header = self.take(12)?;
        let kind = header[0];
        let path_len = u16::from_le_bytes(header[2..4].try_into().unwrap()) as usize;
        let data_len = usize::try_from(u64::from_le_bytes(header[4..12].try_into().unwrap()))
            .map_err(|_| INVALID_ARGUMENT)?;
        if header[1] != 0
            || !(FILE..=LINUX_EXECUTABLE).contains(&kind)
            || path_len == 0
            || path_len > 128
            || (kind == DIRECTORY && data_len != 0)
            || data_len > 256 * 1024 * 1024
        {
            return Err(INVALID_ARGUMENT);
        }
        let path = core::str::from_utf8(self.take(path_len)?).map_err(|_| INVALID_ARGUMENT)?;
        if !path.starts_with('/')
            || path.ends_with('/')
            || path.contains('\0')
            || path[1..]
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == ".." || p.len() > 63)
        {
            return Err(INVALID_ARGUMENT);
        }
        let data = self.take(data_len)?;
        if kind == SYMLINK
            && (data.is_empty()
                || data.len() > 128
                || data.contains(&0)
                || core::str::from_utf8(data).is_err())
        {
            return Err(INVALID_ARGUMENT);
        }
        self.remaining -= 1;
        Ok(Some(Entry { kind, path, data }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;
    use alloc::vec::Vec;
    #[test]
    fn rejects_every_truncation_bad_lengths_paths_kinds_and_trailing_bytes() {
        let mut bytes = Vec::from(&b"ANOVAFS1\x01\0\0\0"[..]);
        bytes.extend_from_slice(&[FILE, 0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
        bytes.extend_from_slice(b"/fx");
        for n in 0..bytes.len() {
            let result = Archive::new(&bytes[..n]).and_then(|mut a| a.entry());
            assert!(result.is_err());
        }
        let mut a = Archive::new(&bytes).unwrap();
        assert_eq!(a.entry().unwrap().unwrap().data, b"x");
        assert!(a.entry().unwrap().is_none());
        for (offset, value) in [(12, 9), (13, 1), (14, 255), (16, 255), (24, b'.')] {
            let mut bad = bytes.clone();
            bad[offset] = value;
            assert!(Archive::new(&bad).unwrap().entry().is_err());
        }
        bytes.push(0);
        let mut a = Archive::new(&bytes).unwrap();
        a.entry().unwrap();
        assert!(a.entry().is_err());
    }
}
