// SPDX-License-Identifier: Apache-2.0
//! Own one controlled compiler job through the legacy or fixed-recipe interface.
//! Status is the compiler leader's actual wait result.
//! Drop releases its private PTY, staging file and tagged descendants, never retained UI logs.

use crate::{
    abi::{self, build::*, workflow},
    raw, result,
};

pub struct Job {
    id: u64,
    syscall: u64,
}
impl Job {
    pub fn start() -> Result<Self, i64> {
        Ok(Self {
            id: call(SYSCALL, START, 0, Request::default())?,
            syscall: SYSCALL,
        })
    }
    pub fn start_recipe(recipe: workflow::Recipe) -> Result<Self, i64> {
        Ok(Self {
            id: call(workflow::SYSCALL, START, recipe as u64, Request::default())?,
            syscall: workflow::SYSCALL,
        })
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    /// Always invalidates the handle; an error reports staging cleanup after reaping.
    pub fn close(mut self) -> Result<(), i64> {
        let id = core::mem::take(&mut self.id);
        call(self.syscall, CLOSE, id, Request::default()).map(|_| ())
    }
    pub fn cancel(&self) -> Result<(), i64> {
        call(self.syscall, workflow::CANCEL, self.id, Request::default()).map(|_| ())
    }
    pub fn publish(&self) -> Result<(), i64> {
        call(self.syscall, workflow::PUBLISH, self.id, Request::default()).map(|_| ())
    }
    pub fn read(&self, bytes: &mut [u8]) -> Result<usize, i64> {
        call(
            self.syscall,
            READ,
            self.id,
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
            self.syscall,
            STATUS,
            self.id,
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
        if self.id != 0 {
            let _ = call(self.syscall, CLOSE, self.id, Request::default());
        }
    }
}
fn call(syscall: u64, operation: u32, id: u64, request: Request) -> Result<u64, i64> {
    let bytes = request.bytes();
    let command = if syscall == SYSCALL {
        command(operation)
    } else {
        workflow::command(operation)
    };
    // SAFETY: The exact initialized request and any borrowed transfer storage remain
    // live for this synchronous call; the kernel checks ownership, lengths and mappings.
    result(unsafe { raw(syscall, command, id, bytes.as_ptr() as u64) })
}
