// SPDX-License-Identifier: Apache-2.0
//! Bridge checked user buffers to memory/disk VFS, durability calls and the polled terminal.
//! Calls run on the BSP at IF=0; blocking retains only a copied syscall frame, never a lock.
use crate::{
    arch::x86_64::traps::TrapFrame,
    mm::{uaccess, user::UserSpace},
};
use anova_abi::*;
use anova_vfs::{Descriptors, Vfs};

mod tty;
pub use tty::{Terminal, TerminalSignal};
fn path(
    space: &UserSpace,
    address: u64,
    length: u64,
    output: &mut [u8; 128],
) -> Result<usize, i64> {
    let n = usize::try_from(length).map_err(|_| TOO_LONG)?;
    if n == 0 || n > output.len() {
        return Err(INVALID_ARGUMENT);
    }
    uaccess::from_user(space, UserAddr(address), &mut output[..n])?;
    if output[..n].contains(&0) || core::str::from_utf8(&output[..n]).is_err() {
        return Err(INVALID_ARGUMENT);
    }
    Ok(n)
}
pub fn ready(frame: &TrapFrame, files: &Descriptors, vfs: &Vfs<'_>, terminal: &Terminal) -> bool {
    let writing = frame.rax == SYS_WRITE;
    if let Ok(Some(device)) = vfs.device(files, frame.rdi, writing)
        && terminal.is_terminal(device)
    {
        return terminal.endpoint_ready(device, writing, vfs);
    }
    vfs.ready(files, frame.rdi, writing)
}
pub fn syscall(
    frame: &TrapFrame,
    space: &UserSpace,
    files: &mut Descriptors,
    cwd: &mut usize,
    vfs: &mut Vfs<'_>,
    terminal: &mut Terminal,
) -> Option<i64> {
    let value = (|| -> Result<i64, i64> {
        Ok(match frame.rax {
            SYS_WRITE | SYS_READ => {
                let writing = frame.rax == SYS_WRITE;
                let device = vfs.device(files, frame.rdi, writing)?;
                let len = usize::try_from(frame.rdx).map_err(|_| TOO_LONG)?;
                if len > MAX_WRITE {
                    return Err(TOO_LONG);
                }
                if !space.accessible(UserAddr(frame.rsi), len, !writing, false) && len != 0 {
                    return Err(BAD_ADDRESS);
                }
                let mut bytes = [0; MAX_WRITE];
                if writing {
                    uaccess::from_user(space, UserAddr(frame.rsi), &mut bytes[..len])?;
                    if device.is_some_and(|d| terminal.is_terminal(d)) {
                        terminal.write(device.unwrap(), &bytes[..len], vfs)? as i64
                    } else {
                        vfs.write(files, frame.rdi, &bytes[..len])? as i64
                    }
                } else {
                    let n = if device.is_some_and(|d| terminal.is_terminal(d)) {
                        terminal.read(device.unwrap(), &mut bytes[..len], vfs)?
                    } else {
                        vfs.read(files, frame.rdi, &mut bytes[..len])?
                    };
                    uaccess::to_user(space, UserAddr(frame.rsi), &bytes[..n])?;
                    n as i64
                }
            }
            SYS_OPEN | SYS_MKDIR | SYS_CHDIR | SYS_UNLINK | SYS_SET_EXECUTABLE => {
                let mut bytes = [0; 128];
                let n = path(space, frame.rdi, frame.rsi, &mut bytes)?;
                let name = core::str::from_utf8(&bytes[..n]).unwrap();
                match frame.rax {
                    SYS_OPEN => vfs.open(files, *cwd, name, frame.rdx)? as i64,
                    SYS_MKDIR => {
                        vfs.mkdir(*cwd, name)?;
                        0
                    }
                    SYS_CHDIR => {
                        *cwd = vfs.chdir(*cwd, name)?;
                        0
                    }
                    SYS_SET_EXECUTABLE => {
                        vfs.set_executable(*cwd, name)?;
                        0
                    }
                    _ => {
                        vfs.unlink(*cwd, name)?;
                        0
                    }
                }
            }
            SYS_GETCWD => {
                let mut bytes = [0; 128];
                let n = vfs.canonical(*cwd, &mut bytes)?;
                if frame.rsi < (n as u64) {
                    return Err(TOO_LONG);
                }
                uaccess::to_user(space, UserAddr(frame.rdi), &bytes[..n])?;
                n as i64
            }
            SYS_CLOSE => {
                vfs.close(files, frame.rdi)?;
                0
            }
            SYS_SYNC if frame.rdi == 0 && frame.rsi == 0 && frame.rdx == 0 => {
                vfs.sync()?;
                0
            }
            SYS_FSYNC => {
                vfs.fsync(files, frame.rdi)?;
                0
            }
            SYS_DUP2 => vfs.dup2(files, frame.rdi, frame.rsi)? as i64,
            SYS_SEEK => vfs.seek(files, frame.rdi, frame.rsi as i64, frame.rdx)? as i64,
            SYS_PIPE => {
                if frame.rsi != 16 {
                    return Err(INVALID_ARGUMENT);
                }
                if !space.accessible(UserAddr(frame.rdi), 16, true, false) {
                    return Err(BAD_ADDRESS);
                }
                let fds = vfs.pipe(files)?;
                let mut bytes = [0; 16];
                bytes[..8].copy_from_slice(&fds[0].to_le_bytes());
                bytes[8..].copy_from_slice(&fds[1].to_le_bytes());
                if let Err(e) = uaccess::to_user(space, UserAddr(frame.rdi), &bytes) {
                    vfs.close(files, fds[0])?;
                    vfs.close(files, fds[1])?;
                    return Err(e);
                }
                0
            }
            SYS_READDIR => {
                if frame.rdx != size_of::<DirEntry>() as u64 {
                    return Err(INVALID_ARGUMENT);
                }
                if !space.accessible(UserAddr(frame.rsi), size_of::<DirEntry>(), true, false) {
                    return Err(BAD_ADDRESS);
                }
                match vfs.readdir(files, frame.rdi)? {
                    Some(entry) => {
                        uaccess::to_user(space, UserAddr(frame.rsi), &entry.bytes())?;
                        1
                    }
                    None => 0,
                }
            }
            _ => return Err(UNSUPPORTED),
        })
    })();
    if matches!(
        frame.rax,
        SYS_WRITE
            | SYS_READ
            | SYS_OPEN
            | SYS_MKDIR
            | SYS_CHDIR
            | SYS_UNLINK
            | SYS_GETCWD
            | SYS_CLOSE
            | SYS_DUP2
            | SYS_SEEK
            | SYS_PIPE
            | SYS_READDIR
            | SYS_SYNC
            | SYS_FSYNC
            | SYS_SET_EXECUTABLE
    ) {
        Some(value.unwrap_or_else(|e| e))
    } else {
        None
    }
}
