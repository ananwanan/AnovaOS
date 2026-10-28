// SPDX-License-Identifier: Apache-2.0
//! Keep bounded ASCII source buffers, exact-byte conflict checks and conservative saves.
//! Save success requires exclusive staging, file flushes and a parent flush after publication.
//! Failed publication retains a dirty buffer and any recovery files; no power-loss guarantee
//! or exclusion against concurrent writers between individual filesystem calls is implied.

#![no_std]

use anova_abi::{NOT_FOUND, TOO_LONG};

pub const LIMIT: usize = 16 * 1024;
/// Reserve eight bytes within the native 128-byte path for recovery suffixes.
pub const MAX_PATH: usize = 120;
pub const COMPONENT_LIMIT: usize = 63;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Unsaved,
    Conflict,
    TooLong,
    InvalidText,
    InvalidPath,
    NoFile,
    Io(i64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SaveOutcome {
    /// The data and publication flushes succeeded; only recovery-file cleanup failed.
    pub backup_retained: bool,
}

/// Each operation reports actual errors. `read` rejects files larger than the output;
/// `write_new` uses exclusive creation, writes all bytes, fsyncs and checks close.
/// `rename` refuses replacement, while `replace` explicitly permits regular-file replacement.
pub trait Fs {
    fn read(&mut self, path: &str, output: &mut [u8]) -> Result<usize, i64>;
    fn write_new(&mut self, path: &str, bytes: &[u8]) -> Result<(), i64>;
    fn rename(&mut self, from: &str, to: &str) -> Result<(), i64>;
    fn replace(&mut self, from: &str, to: &str) -> Result<(), i64>;
    fn remove(&mut self, path: &str) -> Result<(), i64>;
    fn sync_parent(&mut self, path: &str) -> Result<(), i64>;
}

fn io(error: i64) -> Error {
    if error == TOO_LONG {
        Error::TooLong
    } else {
        Error::Io(error)
    }
}

pub fn valid_text(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|byte| matches!(byte, b'\t' | b'\r' | b'\n' | 32..=126))
}

/// Work files use canonical absolute paths; recovery siblings remain explicitly inspectable.
pub fn valid_path(path: &str) -> bool {
    path.len() <= MAX_PATH
        && path.strip_prefix("/persist/").is_some_and(|relative| {
            !relative.is_empty()
                && relative.split('/').all(|component| {
                    !component.is_empty()
                        && component != "."
                        && component != ".."
                        && component.len() <= COMPONENT_LIMIT
                        && component
                            .bytes()
                            .all(|b| matches!(b, 32..=126) && b != b'\\')
                })
        })
}

fn editable_path(path: &str) -> bool {
    valid_path(path)
        && path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.len() <= COMPONENT_LIMIT - 8)
        && !path.ends_with(".m17-tmp")
        && !path.ends_with(".m17-bak")
}

pub struct Editor {
    text: [u8; LIMIT],
    saved: [u8; LIMIT],
    scratch: [u8; LIMIT],
    length: usize,
    saved_length: usize,
    cursor: usize,
    path: [u8; MAX_PATH],
    path_length: usize,
    existed: bool,
    dirty: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    pub const fn new() -> Self {
        Self {
            text: [0; LIMIT],
            saved: [0; LIMIT],
            scratch: [0; LIMIT],
            length: 0,
            saved_length: 0,
            cursor: 0,
            path: [0; MAX_PATH],
            path_length: 0,
            existed: false,
            dirty: false,
        }
    }

    pub fn path(&self) -> &str {
        // Paths enter only through the checked UTF-8 string APIs below.
        core::str::from_utf8(&self.path[..self.path_length]).unwrap()
    }
    pub fn bytes(&self) -> &[u8] {
        &self.text[..self.length]
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn dirty(&self) -> bool {
        self.dirty
    }
    pub fn is_open(&self) -> bool {
        self.path_length != 0
    }

    fn set_path(&mut self, path: &str) {
        self.path[..path.len()].copy_from_slice(path.as_bytes());
        self.path_length = path.len();
    }
    fn check_leave(&self, discard: bool) -> Result<(), Error> {
        if self.dirty && !discard {
            Err(Error::Unsaved)
        } else {
            Ok(())
        }
    }
    fn check_open(&self) -> Result<(), Error> {
        if self.is_open() {
            Ok(())
        } else {
            Err(Error::NoFile)
        }
    }
    fn refresh_dirty(&mut self) {
        self.dirty = !self.existed || self.bytes() != &self.saved[..self.saved_length];
    }

    pub fn open(&mut self, fs: &mut impl Fs, path: &str, discard: bool) -> Result<(), Error> {
        self.check_leave(discard)?;
        if !editable_path(path) {
            return Err(Error::InvalidPath);
        }
        let length = fs.read(path, &mut self.scratch).map_err(io)?;
        if length > LIMIT {
            return Err(Error::TooLong);
        }
        if !valid_text(&self.scratch[..length]) {
            return Err(Error::InvalidText);
        }
        self.text[..length].copy_from_slice(&self.scratch[..length]);
        self.saved[..length].copy_from_slice(&self.scratch[..length]);
        self.length = length;
        self.saved_length = length;
        self.cursor = 0;
        self.existed = true;
        self.dirty = false;
        self.set_path(path);
        Ok(())
    }

    pub fn create(&mut self, path: &str, discard: bool) -> Result<(), Error> {
        self.check_leave(discard)?;
        if !editable_path(path) {
            return Err(Error::InvalidPath);
        }
        self.set_path(path);
        self.length = 0;
        self.saved_length = 0;
        self.cursor = 0;
        self.existed = false;
        self.dirty = true;
        Ok(())
    }

    pub fn close(&mut self, discard: bool) -> Result<(), Error> {
        self.check_leave(discard)?;
        self.path_length = 0;
        self.length = 0;
        self.cursor = 0;
        self.dirty = false;
        Ok(())
    }

    pub fn insert(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.check_open()?;
        if !valid_text(bytes) {
            return Err(Error::InvalidText);
        }
        let length = self
            .length
            .checked_add(bytes.len())
            .filter(|n| *n <= LIMIT)
            .ok_or(Error::TooLong)?;
        self.text
            .copy_within(self.cursor..self.length, self.cursor + bytes.len());
        self.text[self.cursor..self.cursor + bytes.len()].copy_from_slice(bytes);
        self.cursor += bytes.len();
        self.length = length;
        self.refresh_dirty();
        Ok(())
    }
    pub fn backspace(&mut self) {
        if self.cursor != 0 {
            self.text
                .copy_within(self.cursor..self.length, self.cursor - 1);
            self.cursor -= 1;
            self.length -= 1;
            self.refresh_dirty();
        }
    }
    pub fn delete(&mut self) {
        if self.cursor < self.length {
            self.text
                .copy_within(self.cursor + 1..self.length, self.cursor);
            self.length -= 1;
            self.refresh_dirty();
        }
    }
    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }
    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.length);
    }
    pub fn move_home(&mut self) {
        self.cursor = self.text[..self.cursor]
            .iter()
            .rposition(|b| *b == b'\n')
            .map_or(0, |n| n + 1);
    }
    pub fn move_end(&mut self) {
        self.cursor += self.text[self.cursor..self.length]
            .iter()
            .position(|b| *b == b'\n')
            .unwrap_or(self.length - self.cursor);
    }
    pub fn move_up(&mut self) {
        let end = self.cursor;
        self.move_home();
        let start = self.cursor;
        if start != 0 {
            self.cursor -= 1;
            self.move_home();
            self.cursor += (end - start).min(start - 1 - self.cursor);
        } else {
            self.cursor = end;
        }
    }
    pub fn move_down(&mut self) {
        let old = self.cursor;
        self.move_home();
        let column = old - self.cursor;
        self.cursor = old;
        self.move_end();
        if self.cursor < self.length {
            self.cursor += 1;
            let start = self.cursor;
            self.move_end();
            self.cursor = start + column.min(self.cursor - start);
        } else {
            self.cursor = old;
        }
    }

    /// Save uses optimistic exact-byte conflict detection, not a cross-process write lock.
    /// A failed post-replacement flush may leave the new target visible; its prior bytes
    /// remain in `.m17-bak`. Recovery siblings are never silently overwritten.
    pub fn save(&mut self, fs: &mut impl Fs, overwrite: bool) -> Result<SaveOutcome, Error> {
        self.check_open()?;
        let mut path_bytes = [0; MAX_PATH];
        path_bytes[..self.path_length].copy_from_slice(&self.path[..self.path_length]);
        let path = core::str::from_utf8(&path_bytes[..self.path_length]).unwrap();
        let prior = match fs.read(path, &mut self.scratch) {
            Ok(length) if length <= LIMIT => Some(length),
            Ok(_) => return Err(Error::TooLong),
            Err(NOT_FOUND) => None,
            Err(error) => return Err(io(error)),
        };
        let unchanged = match prior {
            Some(length) => {
                self.existed && self.scratch[..length] == self.saved[..self.saved_length]
            }
            None => !self.existed,
        };
        if !unchanged && !overwrite {
            return Err(Error::Conflict);
        }
        let mut temporary = [0; MAX_PATH + 8];
        let mut backup = [0; MAX_PATH + 8];
        temporary[..path.len()].copy_from_slice(path.as_bytes());
        temporary[path.len()..path.len() + 8].copy_from_slice(b".m17-tmp");
        backup[..path.len()].copy_from_slice(path.as_bytes());
        backup[path.len()..path.len() + 8].copy_from_slice(b".m17-bak");
        let temporary = core::str::from_utf8(&temporary[..path.len() + 8]).unwrap();
        let backup = core::str::from_utf8(&backup[..path.len() + 8]).unwrap();
        // Once a save is attempted, no error may falsely turn this buffer clean.
        self.dirty = true;
        fs.write_new(temporary, self.bytes()).map_err(io)?;
        if let Some(length) = prior {
            fs.write_new(backup, &self.scratch[..length]).map_err(io)?;
            fs.sync_parent(path).map_err(io)?;
        }
        if prior.is_some() {
            fs.replace(temporary, path).map_err(io)?;
        } else {
            fs.rename(temporary, path).map_err(io)?;
        }
        fs.sync_parent(path).map_err(io)?;
        self.saved[..self.length].copy_from_slice(&self.text[..self.length]);
        self.saved_length = self.length;
        self.existed = true;
        self.dirty = false;
        let backup_retained = prior.is_some() && fs.remove(backup).is_err();
        Ok(SaveOutcome { backup_retained })
    }

    /// Rename never overwrites a destination and never implicitly discards dirty work.
    pub fn rename(&mut self, fs: &mut impl Fs, to: &str, discard: bool) -> Result<(), Error> {
        self.check_open()?;
        self.check_leave(discard)?;
        if !editable_path(to) {
            return Err(Error::InvalidPath);
        }
        if !self.existed {
            return Err(Error::NoFile);
        }
        fs.rename(self.path(), to).map_err(io)?;
        self.set_path(to);
        // Publication happened; an error here is explicit and the new path remains selected.
        fs.sync_parent(to).map_err(io)?;
        if discard {
            self.text[..self.saved_length].copy_from_slice(&self.saved[..self.saved_length]);
            self.length = self.saved_length;
            self.cursor = self.cursor.min(self.length);
            self.dirty = false;
        }
        Ok(())
    }

    pub fn delete_file(&mut self, fs: &mut impl Fs, discard: bool) -> Result<(), Error> {
        self.check_open()?;
        self.check_leave(discard)?;
        fs.remove(self.path()).map_err(io)?;
        self.existed = false;
        self.dirty = true;
        fs.sync_parent(self.path()).map_err(io)?;
        self.close(true)
    }
}

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests;
