// SPDX-License-Identifier: Apache-2.0
//! Adapt blocked Linux I/O frames and preserve completed readv/writev bytes across later faults.
//! User copies run only with the caller's address space pinned by the BSP scheduler.
use crate::{
    arch::x86_64::traps::TrapFrame,
    fs::{self, Terminal},
    mm::{uaccess, user::UserSpace},
};
use anova_abi::{
    MAX_WRITE, SYS_READ, SYS_WRITE, USER_LIMIT, UserAddr, WOULD_BLOCK,
    linux::{EFAULT, EINVAL, errno},
};
use anova_vfs::{Descriptors, Vfs};

const IOV_LIMIT: usize = 16;
const IOV_BYTES: usize = 16;

pub fn io_frame(mut frame: TrapFrame, linux: bool) -> TrapFrame {
    if linux {
        frame.rax = if matches!(frame.rax, 0 | 19) {
            SYS_READ
        } else {
            SYS_WRITE
        };
    }
    frame
}

fn vector_io(
    frame: &TrapFrame,
    space: &UserSpace,
    files: &mut Descriptors,
    cwd: &mut usize,
    vfs: &mut Vfs<'_>,
    terminal: &mut Terminal,
    writing: bool,
) -> i64 {
    if vfs
        .socket_id(files, frame.rdi)
        .map_err(errno)
        .ok()
        .flatten()
        .is_some()
        || vfs.is_packet(files, frame.rdi)
    {
        return -95;
    }
    if frame.rdx > IOV_LIMIT as u64 {
        return EINVAL;
    }
    let mut total = 0;
    let result = (|| -> Result<(), i64> {
        vfs.device(files, frame.rdi, writing).map_err(errno)?;
        let mut vectors = [0; IOV_LIMIT * IOV_BYTES];
        let vectors = &mut vectors[..frame.rdx as usize * IOV_BYTES];
        if !vectors.is_empty() {
            uaccess::from_user(space, UserAddr(frame.rsi), vectors).map_err(errno)?;
        }
        // Import every descriptor before publishing any I/O, as required for an invalid vector.
        for pair in vectors.chunks_exact(IOV_BYTES) {
            let address = u64::from_le_bytes(pair[..8].try_into().unwrap());
            let length = u64::from_le_bytes(pair[8..].try_into().unwrap());
            if length > i64::MAX as u64 {
                return Err(EINVAL);
            }
            if address
                .checked_add(length)
                .is_none_or(|end| end > USER_LIMIT)
            {
                return Err(EFAULT);
            }
        }
        let mut native = *frame;
        native.rax = if writing { SYS_WRITE } else { SYS_READ };
        for pair in vectors.chunks_exact(IOV_BYTES) {
            native.rsi = u64::from_le_bytes(pair[..8].try_into().unwrap());
            let length = u64::from_le_bytes(pair[8..].try_into().unwrap());
            if length == 0 {
                continue;
            }
            native.rdx = length.min(MAX_WRITE as u64);
            let count = fs::syscall(&native, space, files, cwd, vfs, terminal)
                .expect("Native vectored I/O is implemented");
            if count < 0 {
                return Err(if count == WOULD_BLOCK {
                    WOULD_BLOCK
                } else {
                    errno(count)
                });
            }
            total += count;
            if count as u64 != length {
                break;
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => total,
        Err(_) if total > 0 => total,
        Err(error) => error,
    }
}

pub(in crate::task) fn readv(
    frame: &TrapFrame,
    space: &UserSpace,
    files: &mut Descriptors,
    cwd: &mut usize,
    vfs: &mut Vfs<'_>,
    terminal: &mut Terminal,
) -> i64 {
    vector_io(frame, space, files, cwd, vfs, terminal, false)
}

pub(in crate::task) fn writev(
    frame: &TrapFrame,
    space: &UserSpace,
    files: &mut Descriptors,
    cwd: &mut usize,
    vfs: &mut Vfs<'_>,
    terminal: &mut Terminal,
) -> i64 {
    vector_io(frame, space, files, cwd, vfs, terminal, true)
}
