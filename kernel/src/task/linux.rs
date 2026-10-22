// SPDX-License-Identifier: Apache-2.0
//! Implement the bounded Linux AMD64 personality on the BSP scheduler boundary.
//! Translate wire formats at this boundary; generic VFS, native ABI and scheduling remain independent.
mod io;
mod memory;
pub(crate) mod signals;
mod sockets;
mod terminal;
pub(crate) mod threads;
pub(super) use io::io_frame;

use super::*;
use crate::mm::uaccess;
use anova_abi::linux::{
    ENOSYS, errno, open_flags,
    strings::{self, StringVector},
};

pub fn random(bytes: &mut [u8]) -> Result<(), i64> {
    // SAFETY: CPUID is present in long mode; RDRAND is executed only after feature detection.
    if unsafe { core::arch::x86_64::__cpuid(1) }.ecx & (1 << 30) == 0 {
        return Err(ENOSYS);
    }
    for part in bytes.chunks_mut(8) {
        let mut value = 0;
        let mut available = false;
        for _ in 0..16 {
            // SAFETY: CPUID above advertised RDRAND; value is an aligned live u64.
            if unsafe { core::arch::x86_64::_rdrand64_step(&mut value) } == 1 {
                available = true;
                break;
            }
        }
        if !available {
            return Err(-5);
        }
        part.copy_from_slice(&value.to_le_bytes()[..part.len()]);
    }
    Ok(())
}
fn string(space: &mm::user::UserSpace, address: u64, bytes: &mut [u8; 129]) -> Result<usize, i64> {
    strings::read_c_string(address, bytes, &mut |at, output| {
        uaccess::from_user(space, UserAddr(at), output).map_err(errno)
    })
}

fn stat_bytes(meta: (u64, u32, usize, bool), permissions: u16, modified: u64) -> [u8; 144] {
    let (inode, kind, size, exec) = meta;
    let mut b = [0; 144];
    let mode: u32 = match kind {
        3 => 0o120777,
        2 => 0o040755,
        4 => 0o020666,
        5 => 0o010600,
        6 => 0o140600,
        _ => {
            if exec {
                0o100555
            } else {
                0o100644
            }
        }
    };
    b[8..16].copy_from_slice(&inode.to_le_bytes());
    b[16..24].copy_from_slice(&1u64.to_le_bytes());
    b[24..28].copy_from_slice(&((mode & !0o777) | u32::from(permissions)).to_le_bytes());
    b[88..96].copy_from_slice(&(modified / 1_000_000_000).to_le_bytes());
    b[96..104].copy_from_slice(&(modified % 1_000_000_000).to_le_bytes());
    b[104..112].copy_from_slice(&(modified / 1_000_000_000).to_le_bytes());
    b[112..120].copy_from_slice(&(modified % 1_000_000_000).to_le_bytes());
    b[48..56].copy_from_slice(&(size as u64).to_le_bytes());
    b[56..64].copy_from_slice(&4096u64.to_le_bytes());
    b[64..72].copy_from_slice(&(size as u64).div_ceil(512).to_le_bytes());
    b
}
pub(super) fn wait_bytes(status: ExitStatus) -> [u8; 4] {
    let value = match status.reason {
        0 => (status.code & 255) << 8,
        3 => status.code & 127,
        1 => match status.code {
            0 | 16 | 19 => 8,
            1 | 3 => 5,
            6 => 4,
            17 => 7,
            _ => 11,
        },
        _ => 11,
    };
    value.to_le_bytes()
}
impl Kernel {
    pub(super) fn linux_pending(&mut self, i: usize) -> Option<u32> {
        let image = self.process_mut(i).image.as_mut()?;
        if !image.linux {
            return None;
        }
        let mut ignored = 0;
        for (k, action) in image.actions.borrow().iter().enumerate() {
            if action.handler == 1 || (action.handler == 0 && matches!(k + 1, 17 | 18 | 23 | 28)) {
                ignored |= 1u64 << k;
            }
        }
        image.ignored = ignored;
        image.pending &= !ignored;
        let ready = image.pending & !image.sigmask;
        if ready == 0 {
            None
        } else {
            Some(ready.trailing_zeros() + 1)
        }
    }

    pub(super) fn executable_linux(&self, i: usize, name: &str) -> Result<bool, i64> {
        if name.contains('/') {
            return self.fs.linux_executable(self.process(i).cwd, name);
        }
        let mut bytes = [0; 128];
        if name.len() + 5 > bytes.len() {
            return Err(TOO_LONG);
        }
        bytes[..5].copy_from_slice(b"/bin/");
        bytes[5..5 + name.len()].copy_from_slice(name.as_bytes());
        self.fs.linux_executable(
            ROOT,
            core::str::from_utf8(&bytes[..5 + name.len()]).unwrap(),
        )
    }
    pub(super) fn linux_syscall(&mut self, i: usize) {
        let f = self.process(i).context.frame;
        if matches!(f.rax, 41..=55)
            || matches!(f.rax, 0 | 1)
                && self
                    .fs
                    .socket_id(&self.process(i).files, f.rdi)
                    .ok()
                    .flatten()
                    .is_some()
        {
            let result = self.socket_syscall(i, &f);
            if result != WOULD_BLOCK {
                self.process_mut(i).context.frame.rax = result as u64;
            }
            return;
        }
        if matches!(f.rax, 60 | 231) {
            if f.rax == 231 {
                self.linux_exit_group(i, Outcome::Exit((f.rdi & 255) as u32));
            } else {
                self.linux_thread_exit(i, (f.rdi & 255) as u32);
            }
            return;
        }
        if matches!(f.rax, 13 | 14 | 15 | 62 | 130 | 131 | 200 | 234) {
            let result = self.signal_syscall(i, &f);
            if f.rax != 15 && self.process(i).image.is_some() {
                self.process_mut(i).context.frame.rax = result as u64;
            }
            return;
        }
        if f.rax == 56 {
            let result = self.linux_clone(i, &f).map_or_else(|e| e, |n| n as i64);
            self.process_mut(i).context.frame.rax = result as u64;
            return;
        }
        if f.rax == 202 {
            let result = self.linux_futex(i, &f).unwrap_or_else(|e| e);
            self.process_mut(i).context.frame.rax = result as u64;
            return;
        }
        if matches!(f.rax, 7 | 271) {
            let result = self.linux_poll(i, &f).unwrap_or_else(|e| e);
            self.process_mut(i).context.frame.rax = result as u64;
            return;
        }
        if matches!(f.rax, 16 | 109 | 111 | 112 | 121 | 124) {
            let result = self.linux_terminal(i, &f).unwrap_or_else(|e| e);
            self.process_mut(i).context.frame.rax = result as u64;
            return;
        }
        if f.rax == 24 {
            self.policy.ready(i);
            self.process_mut(i).context.frame.rax = 0;
            return;
        }
        if f.rax == 95 {
            let old = self.process(i).thread.umask;
            let group = self.process(i).thread.tgid;
            for p in self
                .processes
                .iter_mut()
                .flatten()
                .filter(|p| p.thread.tgid == group)
            {
                p.thread.umask = f.rdi as u16 & 0o777;
            }
            self.process_mut(i).context.frame.rax = u64::from(old);
            return;
        }
        if matches!(f.rax, 57 | 58) {
            let vfork = f.rax == 58;
            let result = self.fork_space(i, vfork).map_or_else(errno, |pid| {
                if vfork {
                    let identity = self.process(i).image.as_ref().unwrap().space.identity();
                    self.process_mut(i).thread.vfork_child = Some((pid, identity));
                    self.policy.block(i);
                }
                pid as i64
            });
            self.process_mut(i).context.frame.rax = result as u64;
            return;
        }
        if f.rax == 59 {
            let result = self.linux_exec(i, &f);
            if let Err(error) = result {
                self.process_mut(i).context.frame.rax = error as u64;
            }
            return;
        }
        if f.rax == 35 {
            let space = &self.process(i).image.as_ref().unwrap().space;
            let result = threads::deadline(space, f.rdi, false);
            let result = match result {
                Ok(Some(deadline)) => {
                    if f.rsi != 0 && !space.accessible(UserAddr(f.rsi), 16, true, false) {
                        -14
                    } else {
                        self.process_mut(i).thread.blocked = Some(threads::Blocked::Sleep {
                            deadline,
                            remaining: f.rsi,
                        });
                        self.policy.block(i);
                        self.metrics.sleeps += 1;
                        0
                    }
                }
                Ok(None) => -14,
                Err(e) => e,
            };
            self.process_mut(i).context.frame.rax = result as u64;
            return;
        }
        if f.rax == 61 {
            let result = if f.rdi as i64 != -1 && (f.rdi == 0 || f.rdi > i64::MAX as u64)
                || f.rdx > 1
                || f.r10 != 0
            {
                Some(-22)
            } else {
                let space = &self.process(i).image.as_ref().unwrap().space;
                if f.rsi != 0 && !space.accessible(UserAddr(f.rsi), 4, true, false) {
                    Some(-14)
                } else {
                    let pid = if f.rdi == u64::MAX { 0 } else { f.rdi };
                    let parent = self.policy.entry(i).pid;
                    let eligible = (0..CAPACITY).filter(|&k| {
                        let e = self.policy.entry(k);
                        e.parent == parent && e.state != State::Empty && (pid == 0 || e.pid == pid)
                    });
                    let any = eligible.clone().next().is_some();
                    let zombie = eligible
                        .clone()
                        .any(|k| matches!(self.policy.entry(k).state, State::Zombie(_)));
                    if !any {
                        Some(-10)
                    } else if f.rdx == 1 && !zombie {
                        Some(0)
                    } else {
                        self.wait(i, pid, UserAddr(f.rsi)).map(errno)
                    }
                }
            };
            if let Some(result) = result {
                self.process_mut(i).context.frame.rax = result as u64;
            }
            return;
        }
        let result = self.linux_call(i, &f);
        if f.rax == 80 && result == 0 {
            let (group, cwd) = (self.process(i).thread.tgid, self.process(i).cwd);
            for p in self
                .processes
                .iter_mut()
                .flatten()
                .filter(|p| p.thread.tgid == group)
            {
                p.cwd = cwd;
            }
        }
        if result == -32 {
            self.queue_signal(i, 13);
        }
        if result == ENOSYS {
            serial::write(format_args!("[LINUX:ENOSYS] nr={}\n", f.rax));
        }
        if result == WOULD_BLOCK
            && self
                .fs
                .nonblocking(&self.process(i).files, f.rdi)
                .unwrap_or(false)
        {
            self.process_mut(i).context.frame.rax = (-11i64) as u64;
            return;
        }
        if result == WOULD_BLOCK && matches!(f.rax, 0 | 1 | 19 | 20) {
            self.process_mut(i).io = Some(f);
            self.policy.block(i);
            self.metrics.io_blocks += 1;
        } else {
            self.process_mut(i).context.frame.rax = result as u64;
        }
    }
    fn linux_exec(&mut self, i: usize, f: &TrapFrame) -> Result<(), i64> {
        let space = &self.process(i).image.as_ref().unwrap().space;
        let mut name = [0; 129];
        let mut arguments = StringVector::empty();
        let mut environment = StringVector::empty();
        // Keep large argument buffers in this frame; the active-root closure returns only a length.
        let n = space.with_active(|| -> Result<_, i64> {
            let mut copy = |at, output: &mut [u8]| {
                uaccess::from_user(space, UserAddr(at), output).map_err(errno)
            };
            let n = strings::read_c_string(f.rdi, &mut name, &mut copy)?;
            arguments.read(f.rsi, &mut copy)?;
            if f.rdx != 0 {
                environment.read(f.rdx, &mut copy)?;
            }
            if arguments.is_empty() {
                return Err(-22);
            }
            Ok(n)
        })?;
        let name = core::str::from_utf8(&name[..n]).unwrap();
        if !self.executable_linux(i, name).map_err(errno)? {
            return Err(-8);
        }
        let bytes = self.executable(i, name).map_err(errno)?;
        let args = arguments.strings();
        let env = environment.strings();
        let mut image = self
            .load_image(
                &bytes,
                anova_abi::linux::PERSONALITY,
                &args[..arguments.len()],
                &env[..environment.len()],
            )
            .map_err(|e| match e {
                loader::LoadError::Memory(_) => -12i64,
                loader::LoadError::Elf(anova_elf::Error::Arguments) => -7i64,
                loader::LoadError::Interpreter(error) => error,
                _ => -8i64,
            })?;
        let newfiles = self
            .fs
            .inherit(&self.processes[i].as_ref().unwrap().files)
            .map_err(errno)?;
        let group = self.process(i).thread.tgid;
        let leader = (0..CAPACITY)
            .find(|&k| self.policy.entry(k).pid == group && self.processes[k].is_some());
        let parent = leader.map_or(self.policy.entry(i).parent, |k| self.policy.entry(k).parent);
        for k in 0..CAPACITY {
            if k != i
                && self.processes[k]
                    .as_ref()
                    .is_some_and(|p| p.thread.tgid == group)
            {
                if self.process(k).image.is_some() {
                    self.linux_clear_tid(k);
                }
                self.fs
                    .clear(&mut self.processes[k].as_mut().unwrap().files);
                self.processes[k] = None;
                self.policy.remove(k);
            }
        }
        self.policy.exec_identity(i, group, parent);

        self.fs
            .clear(&mut self.processes[i].as_mut().unwrap().files);
        self.process_mut(i).files = newfiles;
        let p = self.process_mut(i);
        image.sigmask = p.image.as_ref().unwrap().sigmask;
        image.ignored = p.image.as_ref().unwrap().ignored;
        for (k, action) in p
            .image
            .as_ref()
            .unwrap()
            .actions
            .borrow()
            .iter()
            .enumerate()
        {
            if action.handler == 1 {
                image.actions.borrow_mut()[k] = *action;
            }
        }
        p.context = user::Context::new(image.entry, image.stack);
        p.image = Some(image);
        p.thread.blocked = None;
        p.thread.saved_mask = None;
        p.io = None;
        p.network_deadline = None;
        p.wait = None;
        self.fs
            .close_exec(&mut self.processes[i].as_mut().unwrap().files);
        self.metrics.execs += 1;
        Ok(())
    }
    fn linux_call(&mut self, i: usize, f: &TrapFrame) -> i64 {
        if f.rax == 218 {
            let pid = self.policy.entry(i).pid;
            self.process_mut(i).image.as_mut().unwrap().clear_tid = f.rdi;
            return pid as i64;
        }
        if matches!(f.rax, 9..=12) {
            return self.linux_memory(i, f).unwrap_or_else(|e| e);
        }
        if f.rax == 324 && f.rdi == 0 && f.rsi == 0 {
            return 0;
        }
        if f.rax == 158 {
            match f.rdi {
                0x1002 if f.rsi < (1u64 << 47) => {
                    self.process_mut(i).context.set_fs_base(f.rsi);
                    return 0;
                }
                0x1003 => {
                    let p = self.process(i);
                    let space = &p.image.as_ref().unwrap().space;
                    return space
                        .with_active(|| {
                            uaccess::to_user(
                                space,
                                UserAddr(f.rsi),
                                &p.context.fs_base().to_le_bytes(),
                            )
                        })
                        .map_or_else(errno, |()| 0);
                }
                _ => return -22,
            }
        }
        let pid = self.policy.entry(i).pid;
        let parent = self.policy.entry(i).parent;
        let p = self.processes[i].as_mut().unwrap();
        let image = p.image.as_ref().unwrap();
        let space = &image.space;
        space.with_active(|| {
            let mut run = || -> Result<i64, i64> {
                let mut native = *f;
                let translated = match f.rax {
                    0 => Some(SYS_READ),
                    1 => Some(SYS_WRITE),
                    3 => Some(SYS_CLOSE),
                    8 => Some(SYS_SEEK),
                    33 => Some(SYS_DUP2),
                    74 => Some(SYS_FSYNC),
                    _ => None,
                };
                if let Some(number) = translated {
                    native.rax = number;
                    if matches!(f.rax, 0 | 1) {
                        native.rdx = native.rdx.min(MAX_WRITE as u64);
                    }
                    let result = fs::syscall(
                        &native,
                        space,
                        &mut p.files,
                        &mut p.cwd,
                        &mut self.fs,
                        &mut self.terminal,
                    )
                    .unwrap();
                    return Ok(if result == WOULD_BLOCK && matches!(f.rax, 0 | 1) {
                        if self.fs.nonblocking(&p.files, f.rdi).map_err(errno)? {
                            -11
                        } else {
                            WOULD_BLOCK
                        }
                    } else {
                        errno(result)
                    });
                }
                Ok(match f.rax {
                    97 | 302 => {
                        let (resource, output) = if f.rax == 97 {
                            (f.rdi, f.rsi)
                        } else {
                            if f.rdi != 0 && f.rdi != p.thread.tgid {
                                return Err(-3);
                            }
                            if f.rdx != 0 {
                                return Err(-95);
                            }
                            (f.rsi, f.r10)
                        };
                        let bound: u64 = match resource {
                            0 => u64::MAX,
                            1 | 2 => u64::MAX,
                            3 => 1024 * 1024,
                            4 => 0,
                            6 => CAPACITY as u64,
                            7 => 32,
                            _ => return Err(-22),
                        };
                        if output != 0 {
                            let mut bytes = [0; 16];
                            bytes[..8].copy_from_slice(&bound.to_le_bytes());
                            bytes[8..].copy_from_slice(&bound.to_le_bytes());
                            uaccess::to_user(space, UserAddr(output), &bytes).map_err(errno)?;
                        } else if f.rax == 97 {
                            return Err(-14);
                        }
                        0
                    }
                    89 => {
                        if f.rdx == 0 {
                            return Err(-22);
                        }
                        let mut bytes = [0; 129];
                        let n = string(space, f.rdi, &mut bytes)?;
                        let target = self
                            .fs
                            .read_link(p.cwd, core::str::from_utf8(&bytes[..n]).unwrap())
                            .map_err(errno)?;
                        let count = target.len().min(f.rdx as usize);
                        uaccess::to_user(space, UserAddr(f.rsi), &target.as_bytes()[..count])
                            .map_err(errno)?;
                        count as i64
                    }
                    32 => {
                        let target = (0..32).find(|fd| !p.files.contains(*fd)).ok_or(-24)?;
                        self.fs.dup2(&mut p.files, f.rdi, target).map_err(errno)? as i64
                    }
                    2 | 257 => {
                        let (address, flags) = if f.rax == 2 {
                            (f.rdi, f.rsi)
                        } else {
                            if f.rdi as i64 != -100 {
                                return Err(-9);
                            }
                            (f.rsi, f.rdx)
                        };
                        let mut bytes = [0; 129];
                        let n = string(space, address, &mut bytes)?;
                        let flags = open_flags(flags & !(0x800 | 0x100))?;
                        let path = core::str::from_utf8(&bytes[..n]).unwrap();
                        let created = flags & OPEN_CREATE != 0
                            && self.fs.metadata(p.cwd, path) == Err(NOT_FOUND);
                        let mode = if f.rax == 2 { f.rdx } else { f.r10 };
                        if created && mode & !0o777 != 0 {
                            return Err(-95);
                        }
                        let fd = self
                            .fs
                            .open(
                                &mut p.files,
                                p.cwd,
                                core::str::from_utf8(&bytes[..n]).unwrap(),
                                flags,
                            )
                            .map_err(errno)?;
                        if created
                            && let Err(e) =
                                self.fs.fchmod(&p.files, fd, mode as u16 & !p.thread.umask)
                        {
                            self.fs.close(&mut p.files, fd).map_err(errno)?;
                            return Err(errno(e));
                        }
                        let linux_flags = if f.rax == 2 { f.rsi } else { f.rdx };
                        p.files
                            .set_close_on_exec(fd, linux_flags & 0x80000 != 0)
                            .map_err(errno)?;
                        let rawflags = if f.rax == 2 { f.rsi } else { f.rdx };
                        self.fs
                            .set_nonblocking(&p.files, fd, rawflags & 0x800 != 0)
                            .map_err(errno)?;
                        if let Ok(Some(anova_vfs::Device::PtyMaster(n))) =
                            self.fs.device(&p.files, fd, false)
                        {
                            self.terminal.reset_pty(n);
                        }
                        if let Ok(Some(d @ anova_vfs::Device::PtySlave(_))) =
                            self.fs.device(&p.files, fd, false)
                            && rawflags & 0x100 == 0
                            && p.thread.sid == pid
                            && self.terminal.session(d) == 0
                        {
                            self.terminal.attach(d, p.thread.sid, p.thread.pgid);
                        }
                        fd as i64
                    }
                    4 | 5 | 6 | 262 => {
                        let (meta, output) = if f.rax == 5 {
                            (
                                self.fs
                                    .descriptor_metadata(&p.files, f.rdi)
                                    .map_err(errno)?,
                                f.rsi,
                            )
                        } else {
                            let (address, output) = if matches!(f.rax, 4 | 6) {
                                (f.rdi, f.rsi)
                            } else {
                                if f.rdi as i64 != -100 || f.r10 != 0 {
                                    return Err(-22);
                                }
                                (f.rsi, f.rdx)
                            };
                            let mut bytes = [0; 129];
                            let n = string(space, address, &mut bytes)?;
                            (
                                if f.rax == 6 {
                                    self.fs
                                        .metadata_nofollow(
                                            p.cwd,
                                            core::str::from_utf8(&bytes[..n]).unwrap(),
                                        )
                                        .map_err(errno)?
                                } else {
                                    self.fs
                                        .metadata(p.cwd, core::str::from_utf8(&bytes[..n]).unwrap())
                                        .map_err(errno)?
                                },
                                output,
                            )
                        };
                        let (permissions, modified) = self.fs.attributes(meta.0);
                        uaccess::to_user(
                            space,
                            UserAddr(output),
                            &stat_bytes(meta, permissions, modified),
                        )
                        .map_err(errno)?;
                        0
                    }
                    16 => {
                        if p.files.contains(f.rdi) {
                            -25
                        } else {
                            -9
                        }
                    } // Supported terminal ioctls are dispatched by the outer terminal adapter.
                    19 => io::readv(
                        f,
                        space,
                        &mut p.files,
                        &mut p.cwd,
                        &mut self.fs,
                        &mut self.terminal,
                    ),
                    20 => io::writev(
                        f,
                        space,
                        &mut p.files,
                        &mut p.cwd,
                        &mut self.fs,
                        &mut self.terminal,
                    ),
                    21 => {
                        if f.rsi & !7 != 0 {
                            return Err(-22);
                        }
                        let mut bytes = [0; 129];
                        let n = string(space, f.rdi, &mut bytes)?;
                        let meta = self
                            .fs
                            .metadata(p.cwd, core::str::from_utf8(&bytes[..n]).unwrap())
                            .map_err(errno)?;
                        if f.rsi & 2 != 0
                            && !self
                                .fs
                                .writable(p.cwd, core::str::from_utf8(&bytes[..n]).unwrap())
                                .map_err(errno)?
                        {
                            return Err(-13);
                        }
                        if f.rsi & 1 != 0 && !meta.3 {
                            return Err(-13);
                        }
                        0
                    }
                    22 | 293 => {
                        if f.rax == 293 && f.rsi != 0 {
                            return Err(-22);
                        }
                        if !space.accessible(UserAddr(f.rdi), 8, true, false) {
                            return Err(-14);
                        }
                        let pair = self.fs.pipe(&mut p.files).map_err(errno)?;
                        let mut bytes = [0; 8];
                        bytes[..4].copy_from_slice(&(pair[0] as u32).to_le_bytes());
                        bytes[4..].copy_from_slice(&(pair[1] as u32).to_le_bytes());
                        if let Err(e) = uaccess::to_user(space, UserAddr(f.rdi), &bytes) {
                            self.fs.close(&mut p.files, pair[0]).unwrap();
                            self.fs.close(&mut p.files, pair[1]).unwrap();
                            return Err(errno(e));
                        }
                        0
                    }
                    39 => p.thread.tgid as i64,
                    186 => pid as i64,
                    110 => parent as i64,
                    102 | 104 | 107 | 108 => 0, // Single root identity, no credential transitions.
                    63 => {
                        let mut bytes = [0; 65 * 6];
                        for (k, text) in [
                            "AnovaOS",
                            "anova",
                            "0.1.0",
                            "M9 Linux runtime profile",
                            "x86_64",
                            "(none)",
                        ]
                        .iter()
                        .enumerate()
                        {
                            bytes[k * 65..k * 65 + text.len()].copy_from_slice(text.as_bytes());
                        }
                        uaccess::to_user(space, UserAddr(f.rdi), &bytes).map_err(errno)?;
                        0
                    }
                    72 => {
                        if !p.files.contains(f.rdi) {
                            return Err(-9);
                        }
                        match f.rsi {
                            0 | 1030 => {
                                let target =
                                    (f.rdx..32).find(|fd| !p.files.contains(*fd)).ok_or(-24)?;
                                let fd =
                                    self.fs.dup2(&mut p.files, f.rdi, target).map_err(errno)?;
                                p.files
                                    .set_close_on_exec(fd, f.rsi == 1030)
                                    .map_err(errno)?;
                                fd as i64
                            }
                            3 => {
                                let flags = self.fs.status_flags(&p.files, f.rdi).map_err(errno)?;
                                let access = match flags & (OPEN_READ | OPEN_WRITE) {
                                    OPEN_READ => 0,
                                    OPEN_WRITE => 1,
                                    _ => 2,
                                };
                                access
                                    | 0x8000
                                    | if flags & OPEN_APPEND != 0 { 0x400 } else { 0 }
                                    | if self.fs.nonblocking(&p.files, f.rdi).map_err(errno)? {
                                        0x800
                                    } else {
                                        0
                                    }
                            }
                            4 if f.rdx & !(0x800 | 0x400 | 0x8000 | 3) == 0 => {
                                self.fs
                                    .set_nonblocking(&p.files, f.rdi, f.rdx & 0x800 != 0)
                                    .map_err(errno)?;
                                self.fs
                                    .set_append(&p.files, f.rdi, f.rdx & 0x400 != 0)
                                    .map_err(errno)?;
                                0
                            }
                            1 => p.files.close_on_exec(f.rdi).map_err(errno)? as i64,
                            2 if f.rdx <= 1 => {
                                p.files
                                    .set_close_on_exec(f.rdi, f.rdx == 1)
                                    .map_err(errno)?;
                                0
                            }
                            _ => return Err(-22),
                        }
                    }
                    79 => {
                        let mut bytes = [0; 129];
                        let mut path = [0; 128];
                        let n = self.fs.canonical(p.cwd, &mut path).map_err(errno)?;
                        bytes[..n].copy_from_slice(&path[..n]);
                        if f.rsi < n as u64 + 1 {
                            return Err(-34);
                        }
                        uaccess::to_user(space, UserAddr(f.rdi), &bytes[..n + 1]).map_err(errno)?;
                        n as i64 + 1
                    }
                    90 | 91 => {
                        if f.rsi & !0o777 != 0 {
                            return Err(-95);
                        }
                        if f.rax == 91 {
                            self.fs
                                .fchmod(&p.files, f.rdi, f.rsi as u16)
                                .map_err(errno)?;
                        } else {
                            let mut bytes = [0; 129];
                            let n = string(space, f.rdi, &mut bytes)?;
                            self.fs
                                .chmod(
                                    p.cwd,
                                    core::str::from_utf8(&bytes[..n]).unwrap(),
                                    f.rsi as u16,
                                )
                                .map_err(errno)?;
                        }
                        0
                    }
                    80 | 83 | 87 => {
                        let mut bytes = [0; 129];
                        let n = string(space, f.rdi, &mut bytes)?;
                        let path = core::str::from_utf8(&bytes[..n]).unwrap();
                        match f.rax {
                            80 => p.cwd = self.fs.chdir(p.cwd, path).map_err(errno)?,
                            83 => {
                                if f.rsi & !0o022 != 0o755 {
                                    return Err(-95);
                                }
                                self.fs.mkdir(p.cwd, path).map_err(errno)?;
                            }
                            _ => self.fs.unlink(p.cwd, path).map_err(errno)?,
                        }
                        0
                    }
                    217 => {
                        if f.rdx < 24 {
                            return Err(-22);
                        }
                        match self.fs.peek_readdir(&p.files, f.rdi).map_err(errno)? {
                            None => 0,
                            Some(entry) => {
                                let n = (19 + entry.name_len as usize + 1 + 7) & !7;
                                if f.rdx < n as u64 {
                                    return Err(-22);
                                }
                                let mut bytes = [0; 88];
                                bytes[..8].copy_from_slice(&entry.inode.to_le_bytes());
                                bytes[8..16].copy_from_slice(&(entry.inode + 1).to_le_bytes());
                                bytes[16..18].copy_from_slice(&(n as u16).to_le_bytes());
                                bytes[18] = match entry.kind {
                                    2 => 4,
                                    3 => 10,
                                    4 => 2,
                                    _ => 8,
                                };
                                bytes[19..19 + entry.name_len as usize]
                                    .copy_from_slice(&entry.name[..entry.name_len as usize]);
                                uaccess::to_user(space, UserAddr(f.rsi), &bytes[..n])
                                    .map_err(errno)?;
                                self.fs.readdir(&p.files, f.rdi).map_err(errno)?;
                                n as i64
                            }
                        }
                    }
                    162 => {
                        self.fs.sync().map_err(errno)?;
                        0
                    }
                    228 if f.rdi <= 1 => {
                        let ticks = apic::ticks();
                        let mut bytes = [0; 16];
                        let (seconds, nanos) = if f.rdi == 0 {
                            crate::arch::x86_64::clock::realtime().ok_or(-5)?
                        } else {
                            (ticks / 100, (ticks % 100) * 10_000_000)
                        };
                        bytes[..8].copy_from_slice(&seconds.to_le_bytes());
                        bytes[8..].copy_from_slice(&nanos.to_le_bytes());
                        uaccess::to_user(space, UserAddr(f.rsi), &bytes).map_err(errno)?;
                        0
                    }
                    318 => {
                        if f.rdx & !1 != 0 {
                            return Err(-22);
                        }
                        let n = f.rsi.min(4096) as usize;
                        if n == 0 {
                            return Ok(0);
                        }
                        let mut bytes = [0; 4096];
                        if !space.accessible(UserAddr(f.rdi), n, true, false) {
                            return Err(-14);
                        }
                        random(&mut bytes[..n])?;
                        uaccess::to_user(space, UserAddr(f.rdi), &bytes[..n]).map_err(errno)?;
                        n as i64
                    }
                    _ => ENOSYS,
                })
            };
            run().unwrap_or_else(|e| e)
        })
    }
}
