// SPDX-License-Identifier: Apache-2.0
//! Adapt the bounded editor to the same native VFS namespace used by Linux PTY children.
//! File fsync and directory fsync currently flush the shared filesystem/device globally.

use anova_editor::Fs;
use anova_rt::{self as rt, abi::*};

pub struct NativeFs;

impl Fs for NativeFs {
    fn read(&mut self, path: &str, output: &mut [u8]) -> Result<usize, i64> {
        let fd = rt::open(path, OPEN_READ)?;
        let read = (|| {
            let mut length = 0;
            while length < output.len() {
                let end = (length + MAX_WRITE).min(output.len());
                let n = rt::read(fd, &mut output[length..end])?;
                if n == 0 {
                    return Ok(length);
                }
                length += n;
            }
            let mut extra = [0];
            if rt::read(fd, &mut extra)? != 0 {
                Err(TOO_LONG)
            } else {
                Ok(length)
            }
        })();
        let close = rt::close(fd);
        read.and_then(|length| close.map(|_| length))
    }

    fn write_new(&mut self, path: &str, bytes: &[u8]) -> Result<(), i64> {
        let fd = rt::open(path, OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE)?;
        let write = rt::write_all(fd, bytes).and_then(|_| rt::fsync(fd));
        let close = rt::close(fd);
        write.and(close)
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), i64> {
        rt::files::rename(from, to)
    }

    fn replace(&mut self, from: &str, to: &str) -> Result<(), i64> {
        rt::files::replace(from, to)
    }

    fn remove(&mut self, path: &str) -> Result<(), i64> {
        rt::unlink(path)
    }

    fn sync_parent(&mut self, path: &str) -> Result<(), i64> {
        let (parent, _) = path.rsplit_once('/').ok_or(INVALID_ARGUMENT)?;
        let fd = rt::open(parent, OPEN_READ | OPEN_DIRECTORY)?;
        let sync = rt::fsync(fd);
        let close = rt::close(fd);
        sync.and(close)
    }
}
