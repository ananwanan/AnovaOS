// SPDX-License-Identifier: Apache-2.0
//! Versioned, allocation-free native SDK facade with stable typed errors and owned descriptors.
//! The older scalar-error wrappers remain available for existing v1 applications.
pub use crate::abi::{Error, SDK_VERSION, decode};
pub type Result<T> = core::result::Result<T, Error>;

fn error(raw: i64) -> Error {
    Error::from_raw(raw).expect("Native wrapper returned a nonnegative error")
}

/// Check the exact wire version and response size before using this SDK's layouts.
pub fn info() -> Result<crate::abi::SystemInfo> {
    let info = crate::info().map_err(error)?;
    if info.abi_version != crate::abi::VERSION || info.size != 32 {
        return Err(Error::UNSUPPORTED);
    }
    Ok(info)
}

pub fn write_all(fd: u64, bytes: &[u8]) -> Result<()> {
    crate::write_all(fd, bytes).map_err(error)
}

pub fn read(fd: u64, bytes: &mut [u8]) -> Result<usize> {
    crate::read(fd, bytes).map_err(error)
}

pub fn open(path: &str, flags: u64) -> Result<File> {
    crate::open(path, flags)
        .map(|descriptor| File {
            descriptor: Some(descriptor),
        })
        .map_err(error)
}

pub fn spawn(path: &str, argument: &str) -> Result<u64> {
    crate::spawn(path, argument).map_err(error)
}

pub fn wait(pid: u64) -> Result<crate::abi::WaitStatus> {
    crate::wait(pid).map_err(error)
}

pub fn sync() -> Result<()> {
    crate::sync().map_err(error)
}

/// An exclusively owned descriptor; dropping it closes the descriptor once.
/// Forking with a live File requires the raw fork API's independent-owner contract.
pub struct File {
    descriptor: Option<u64>,
}

impl File {
    pub fn descriptor(&self) -> u64 {
        self.descriptor.expect("File already closed")
    }

    pub fn read(&mut self, bytes: &mut [u8]) -> Result<usize> {
        read(self.descriptor(), bytes)
    }

    pub fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        write_all(self.descriptor(), bytes)
    }

    pub fn sync(&self) -> Result<()> {
        crate::fsync(self.descriptor()).map_err(error)
    }

    /// Report close errors explicitly; Drop cannot report errors to the caller.
    pub fn close(mut self) -> Result<()> {
        crate::close(self.descriptor.take().expect("File already closed")).map_err(error)
    }
}

impl Drop for File {
    fn drop(&mut self) {
        if let Some(descriptor) = self.descriptor.take() {
            let _ = crate::close(descriptor);
        }
    }
}
