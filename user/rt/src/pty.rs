// SPDX-License-Identifier: Apache-2.0
//! Own a nonblocking native PTY session and its explicitly admitted Linux shell descendants.

use crate::{
    abi::{self, pty::*},
    raw, result,
};

pub struct Session(u64);

impl Session {
    pub fn start(rows: u32, columns: u32) -> Result<Self, i64> {
        call(
            START,
            0,
            Request {
                rows,
                columns,
                ..Request::default()
            },
        )
        .map(Self)
    }

    pub fn id(&self) -> u64 {
        self.0
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

    pub fn write(&self, bytes: &[u8]) -> Result<usize, i64> {
        call(
            WRITE,
            self.0,
            Request {
                buffer: bytes.as_ptr() as u64,
                length: bytes.len() as u64,
                ..Request::default()
            },
        )
        .map(|n| n as usize)
    }

    pub fn resize(&self, rows: u32, columns: u32) -> Result<(), i64> {
        call(
            RESIZE,
            self.0,
            Request {
                rows,
                columns,
                ..Request::default()
            },
        )
        .map(|_| ())
    }

    pub fn status(&self) -> Result<Option<abi::WaitStatus>, i64> {
        let mut bytes = [0; 16];
        match call(
            STATUS,
            self.0,
            Request {
                buffer: bytes.as_mut_ptr() as u64,
                length: 16,
                ..Request::default()
            },
        ) {
            Err(abi::WOULD_BLOCK) => Ok(None),
            Err(error) => Err(error),
            Ok(_) => Ok(Some(abi::WaitStatus {
                pid: u64::from_le_bytes(bytes[..8].try_into().unwrap()),
                status: abi::ExitStatus {
                    reason: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
                    code: u32::from_le_bytes(bytes[12..].try_into().unwrap()),
                },
            })),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = call(CLOSE, self.0, Request::default());
    }
}

fn call(operation: u32, id: u64, request: Request) -> Result<u64, i64> {
    let bytes = request.bytes();
    // SAFETY: The exact initialized wire record and its borrowed transfer storage remain
    // live for this synchronous, nonblocking call. The kernel validates ownership and spans.
    result(unsafe { raw(SYSCALL, command(operation), id, bytes.as_ptr() as u64) })
}
