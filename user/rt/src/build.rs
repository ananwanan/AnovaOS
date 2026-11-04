// SPDX-License-Identifier: Apache-2.0
//! Own one controlled compiler job; status is the compiler leader's actual wait result.
//! Drop releases its private PTY, staging file and tagged descendants, never retained UI logs.

use crate::{
    abi::{self, build::*},
    raw, result,
};

pub struct Job(u64);
impl Job {
    pub fn start() -> Result<Self, i64> {
        call(START, 0, Request::default()).map(Self)
    }
    pub fn id(&self) -> u64 {
        self.0
    }
    /// Always invalidates the handle; an error reports staging cleanup after reaping.
    pub fn close(mut self) -> Result<(), i64> {
        let id = self.0;
        self.0 = 0;
        call(CLOSE, id, Request::default()).map(|_| ())
    }
    pub fn read(&self, bytes: &mut [u8]) -> Result<usize, i64> {
        call(
            READ,
            self.0,
            Request {
                buffer: bytes.as_mut_ptr() as u64,
                length: bytes.len() as u64,
                ..Request::default()
            },
        )
        .map(|n| n as usize)
    }
    pub fn status(&self) -> Result<Option<abi::WaitStatus>, i64> {
        let mut status = abi::WaitStatus::default();
        match call(
            STATUS,
            self.0,
            Request {
                buffer: &mut status as *mut _ as u64,
                length: 16,
                ..Request::default()
            },
        ) {
            Err(abi::WOULD_BLOCK) => Ok(None),
            Err(e) => Err(e),
            Ok(_) => Ok(Some(status)),
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        if self.0 != 0 {
            let _ = call(CLOSE, self.0, Request::default());
        }
    }
}
fn call(operation: u32, id: u64, request: Request) -> Result<u64, i64> {
    let bytes = request.bytes();
    // SAFETY: The exact initialized request and any borrowed transfer storage remain
    // live for this synchronous call; the kernel checks ownership, lengths and mappings.
    result(unsafe { raw(SYSCALL, command(operation), id, bytes.as_ptr() as u64) })
}
