// SPDX-License-Identifier: Apache-2.0
//! Implement the bounded Linux AMD64 personality on the BSP scheduler boundary.
//! Translate wire formats here; generic VFS, native ABI and scheduling remain independent.
use super::*;
use crate::mm::uaccess;
use anova_abi::linux::{ENOSYS, errno, open_flags};

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
pub fn io_frame(mut f: TrapFrame, linux: bool) -> TrapFrame {
    if linux {
        f.rax = if f.rax == 0 { SYS_READ } else { SYS_WRITE };
    }
    f
}
fn string(space: &mm::user::UserSpace, address: u64, bytes: &mut [u8; 129]) -> Result<usize, i64> {
    for i in 0..bytes.len() {
        let at = address.checked_add(i as u64).ok_or(-14)?;
        uaccess::from_user(space, UserAddr(at), &mut bytes[i..i + 1]).map_err(errno)?;
        if bytes[i] == 0 {
            core::str::from_utf8(&bytes[..i]).map_err(|_| -22)?;
            return Ok(i);
        }
    }
    Err(-36)
}
fn stat_bytes(meta: (u64, u32, usize, bool)) -> [u8; 144] {
    let (inode, kind, size, exec) = meta;
    let mut b = [0; 144];
    let mode: u32 = match kind {
        3 => 0o120777,
        2 => 0o040755,
        4 => 0o020666,
        5 => 0o010600,
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
    b[24..28].copy_from_slice(&mode.to_le_bytes());
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
        image.pending &= !image.ignored;
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
        if matches!(f.rax, 60 | 231) {
            self.finish(i, Outcome::Exit((f.rdi & 255) as u32));
            return;
        }
        if f.rax == 57 {
            let result = self.fork(i).map_or_else(errno, |pid| pid as i64);
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
        if f.rax == 62 {
            let target = (0..CAPACITY).find(|&k| {
                self.policy.entry(k).pid == f.rdi
                    && self.processes[k].is_some()
                    && !matches!(self.policy.entry(k).state, State::Empty | State::Zombie(_))
            });
            let result = if let Some(target) = target {
                if f.rsi == 0 {
                    0
                } else if matches!(f.rsi, 9 | 13 | 15)
                    && self.process(target).image.as_ref().unwrap().linux
                {
                    self.process_mut(target).image.as_mut().unwrap().pending |= 1 << (f.rsi - 1);
                    self.policy.ready(target);
                    0
                } else {
                    ENOSYS
                }
            } else {
                -3
            };
            self.process_mut(i).context.frame.rax = result as u64;
            return;
        }
        if f.rax == 35 {
            let space = &self.process(i).image.as_ref().unwrap().space;
            let mut bytes = [0; 16];
            let result = space
                .with_active(|| uaccess::from_user(space, UserAddr(f.rdi), &mut bytes))
                .map_err(errno);
            let sec = u64::from_le_bytes(bytes[..8].try_into().unwrap());
            let ns = u64::from_le_bytes(bytes[8..].try_into().unwrap());
            let delay = sec
                .checked_mul(100)
                .and_then(|n| n.checked_add(ns.div_ceil(10_000_000)));
            let result = match result {
                Err(e) => e,
                Ok(()) if sec > i64::MAX as u64 || ns >= 1_000_000_000 => -22,
                Ok(()) => match delay.and_then(|n| apic::ticks().checked_add(n)) {
                    Some(deadline) => {
                        self.policy.sleep(i, deadline);
                        self.metrics.sleeps += 1;
                        0
                    }
                    None => -22,
                },
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
        if result == -32 {
            let image = self.process(i).image.as_ref().unwrap();
            if (image.sigmask | image.ignored) & (1 << 12) == 0 {
                self.finish(i, Outcome::Signal(13));
                return;
            }
            if image.ignored & (1 << 12) == 0 {
                self.process_mut(i).image.as_mut().unwrap().pending |= 1 << 12;
            }
        }
        if result == ENOSYS {
            serial::write(format_args!("[LINUX:ENOSYS] nr={}\n", f.rax));
        }
        if result == WOULD_BLOCK && matches!(f.rax, 0 | 1 | 20) {
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
        let mut storage = [[0; 129]; 16];
        let mut lengths = [0; 16];
        let mut env_storage = [[0; 129]; 16];
        let mut env_lengths = [0; 16];
        let mut env_count = 0;
        let (n, count) = space.with_active(|| -> Result<(usize, usize), i64> {
            let n = string(space, f.rdi, &mut name)?;
            let mut count = 0;
            loop {
                let mut word = [0; 8];
                let at = f.rsi.checked_add(count as u64 * 8).ok_or(-14)?;
                uaccess::from_user(space, UserAddr(at), &mut word).map_err(errno)?;
                let address = u64::from_le_bytes(word);
                if address == 0 {
                    break;
                }
                if count == 16 {
                    return Err(-7);
                }
                lengths[count] = string(space, address, &mut storage[count])?;
                count += 1;
            }
            if f.rdx != 0 {
                loop {
                    let mut word = [0; 8];
                    let at = f.rdx.checked_add(env_count as u64 * 8).ok_or(-14)?;
                    uaccess::from_user(space, UserAddr(at), &mut word).map_err(errno)?;
                    let address = u64::from_le_bytes(word);
                    if address == 0 {
                        break;
                    }
                    if env_count == 16 {
                        return Err(-7);
                    }
                    env_lengths[env_count] = string(space, address, &mut env_storage[env_count])?;
                    env_count += 1;
                }
            }
            if count == 0 {
                return Err(-22);
            }
            Ok((n, count))
        })?;
        let name = core::str::from_utf8(&name[..n]).unwrap();
        if !self.executable_linux(i, name).map_err(errno)? {
            return Err(-8);
        }
        let bytes = self.executable(i, name).map_err(errno)?;
        let mut args = [""; 16];
        for k in 0..count {
            args[k] = core::str::from_utf8(&storage[k][..lengths[k]]).unwrap();
        }
        let mut env = [""; 16];
        for k in 0..env_count {
            env[k] = core::str::from_utf8(&env_storage[k][..env_lengths[k]]).unwrap();
        }
        let mut image = loader::load_env(
            &bytes,
            anova_abi::linux::PERSONALITY,
            &args[..count],
            &env[..env_count],
        )
        .map_err(|e| match e {
            loader::LoadError::Memory(_) => -12i64,
            _ => -8i64,
        })?;
        let p = self.process_mut(i);
        image.sigmask = p.image.as_ref().unwrap().sigmask;
        image.ignored = p.image.as_ref().unwrap().ignored;
        p.context = user::Context::new(image.entry, image.stack);
        p.image = Some(image);
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
        if matches!(f.rax, 13 | 14) {
            return self.linux_signal_state(i, f);
        }
        if matches!(f.rax, 9 | 11 | 12) {
            return self.linux_memory(i, f);
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
                        WOULD_BLOCK
                    } else {
                        errno(result)
                    });
                }
                Ok(match f.rax {
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
                        let flags = open_flags(flags)?;
                        let path = core::str::from_utf8(&bytes[..n]).unwrap();
                        if flags & OPEN_CREATE != 0
                            && self.fs.metadata(p.cwd, path) == Err(NOT_FOUND)
                        {
                            let mode = if f.rax == 2 { f.rdx } else { f.r10 };
                            if mode & !0o022 != 0o644 {
                                return Err(-95);
                            }
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
                        let linux_flags = if f.rax == 2 { f.rsi } else { f.rdx };
                        p.files
                            .set_close_on_exec(fd, linux_flags & 0x80000 != 0)
                            .map_err(errno)?;
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
                        uaccess::to_user(space, UserAddr(output), &stat_bytes(meta))
                            .map_err(errno)?;
                        0
                    }
                    16 => {
                        if p.files.contains(f.rdi) {
                            -25
                        } else {
                            -9
                        }
                    } // No termios ioctl is claimed by this profile.
                    20 => {
                        // writev: validate all descriptors and preserve partial completion.
                        if f.rdx > 16 {
                            return Err(-22);
                        }
                        let mut total = 0;
                        for k in 0..f.rdx {
                            let mut pair = [0; 16];
                            let at = f.rsi.checked_add(k * 16).ok_or(-14)?;
                            uaccess::from_user(space, UserAddr(at), &mut pair).map_err(errno)?;
                            native.rax = SYS_WRITE;
                            native.rsi = u64::from_le_bytes(pair[..8].try_into().unwrap());
                            let length = u64::from_le_bytes(pair[8..].try_into().unwrap());
                            native.rdx = length.min(MAX_WRITE as u64);
                            let result = fs::syscall(
                                &native,
                                space,
                                &mut p.files,
                                &mut p.cwd,
                                &mut self.fs,
                                &mut self.terminal,
                            )
                            .unwrap();
                            if result < 0 {
                                return Ok(if total != 0 {
                                    total
                                } else if result == WOULD_BLOCK {
                                    WOULD_BLOCK
                                } else {
                                    errno(result)
                                });
                            }
                            total += result;
                            if result as u64 != length {
                                break;
                            }
                        }
                        total
                    }
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
                        if f.rsi & 2 != 0 {
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
                    39 | 186 => pid as i64,
                    110 => parent as i64,
                    102 | 104 | 107 | 108 => 0, // Single root identity, no credential transitions.
                    63 => {
                        let mut bytes = [0; 65 * 6];
                        for (k, text) in [
                            "AnovaOS",
                            "anova",
                            "0.1.0",
                            "M8 static Linux ABI",
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
                    228 if f.rdi == 1 => {
                        let ticks = apic::ticks();
                        let mut bytes = [0; 16];
                        bytes[..8].copy_from_slice(&(ticks / 100).to_le_bytes());
                        bytes[8..].copy_from_slice(&((ticks % 100) * 10_000_000).to_le_bytes());
                        uaccess::to_user(space, UserAddr(f.rsi), &bytes).map_err(errno)?;
                        0
                    }
                    318 => {
                        if f.rdx & !1 != 0 {
                            return Err(-22);
                        }
                        let n = f.rsi.min(4096) as usize;
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
    fn linux_signal_state(&mut self, i: usize, f: &TrapFrame) -> i64 {
        if f.r10 != 8 {
            return -22;
        }
        let image = self.process(i).image.as_ref().unwrap();
        let mut input = [0; 32];
        let mut output = [0; 32];
        let len = if f.rax == 14 { 8 } else { 32 };
        if f.rax == 13 {
            if f.rdi == 0 || f.rdi > 64 || matches!(f.rdi, 9 | 19) {
                return -22;
            }
            output[..8].copy_from_slice(&((image.ignored >> (f.rdi - 1)) & 1).to_le_bytes());
        } else {
            if f.rdi > 2 {
                return -22;
            }
            output[..8].copy_from_slice(&image.sigmask.to_le_bytes());
        }
        let result = image.space.with_active(|| -> Result<(), i64> {
            if f.rsi != 0 {
                uaccess::from_user(&image.space, UserAddr(f.rsi), &mut input[..len])
                    .map_err(errno)?;
            }
            if f.rax == 13 && f.rsi != 0 {
                let handler = u64::from_le_bytes(input[..8].try_into().unwrap());
                if handler > 1 {
                    return Err(ENOSYS);
                }
                let flags = u64::from_le_bytes(input[8..16].try_into().unwrap());
                if flags & !0x0400_0000 != 0 {
                    return Err(-22);
                }
            }
            if f.rdx != 0 {
                uaccess::to_user(&image.space, UserAddr(f.rdx), &output[..len]).map_err(errno)?;
            }
            Ok(())
        });
        if let Err(e) = result {
            return e;
        }
        if f.rsi != 0 {
            let image = self.process_mut(i).image.as_mut().unwrap();
            let value = u64::from_le_bytes(input[..8].try_into().unwrap());
            if f.rax == 13 {
                let bit = 1u64 << (f.rdi - 1);
                image.ignored = (image.ignored & !bit) | if value == 1 { bit } else { 0 };
            } else {
                image.sigmask = match f.rdi {
                    0 => image.sigmask | value,
                    1 => image.sigmask & !value,
                    _ => value,
                } & !((1 << 8) | (1 << 18));
            }
        }
        0
    }
    fn linux_memory(&mut self, i: usize, f: &TrapFrame) -> i64 {
        let image = self.process_mut(i).image.as_mut().unwrap();
        match f.rax {
            12 => {
                let old = image.brk;
                if !(0x4000_0000..=0x4080_0000).contains(&f.rdi) {
                    return old as i64;
                }
                let start = (old + 4095) & !4095;
                let end = (f.rdi + 4095) & !4095;
                if end > start && image.space.map_anonymous(start, end).is_err() {
                    return old as i64;
                }
                for page in (end..start).step_by(4096) {
                    if image.space.unmap(page).is_err() {
                        return old as i64;
                    }
                }
                image.brk = f.rdi;
                f.rdi as i64
            }
            9 => {
                if f.r8 != u64::MAX || f.r9 != 0 || f.rsi == 0 {
                    return -22;
                }
                let Some(size) = f
                    .rsi
                    .checked_add(4095)
                    .map(|n| n & !4095)
                    .filter(|n| *n <= 8 * 1024 * 1024)
                else {
                    return -12;
                };
                // musl reserves a real inaccessible guard at the beginning of its brk arena.
                if f.rdx == 0 && f.r10 == 0x32 {
                    let Some(end) = f.rdi.checked_add(size) else {
                        return -22;
                    };
                    if f.rdi & 4095 != 0 || f.rdi < 0x4000_0000 || end > (image.brk + 4095) & !4095
                    {
                        return -22;
                    }
                    for page in (f.rdi..end).step_by(4096) {
                        if image.space.unmap(page).is_err() {
                            return -22;
                        }
                    }
                    return f.rdi as i64;
                }
                if f.rdx != 3 || f.r10 != 0x22 {
                    return -22;
                }
                let mut start = 0x5000_0000;
                let mut run = 0;
                let mut found = false;
                for page in (0x5000_0000..0x5080_0000).step_by(4096) {
                    if image.space.accessible(UserAddr(page), 1, false, false) {
                        run = 0;
                        start = page + 4096;
                    } else {
                        run += 4096;
                        if run == size {
                            found = true;
                            break;
                        }
                    }
                }
                if !found || image.space.map_anonymous(start, start + size).is_err() {
                    return -12;
                }
                image.mmap = start + size;
                start as i64
            }
            11 => {
                let Some(end) = f
                    .rdi
                    .checked_add(f.rsi)
                    .and_then(|n| n.checked_add(4095))
                    .map(|n| n & !4095)
                else {
                    return -22;
                };
                if f.rdi & 4095 != 0 || f.rsi == 0 || f.rdi < 0x5000_0000 || end > 0x5080_0000 {
                    return -22;
                }
                for page in (f.rdi..end).step_by(4096) {
                    if image.space.unmap(page).is_err() {
                        return -22;
                    }
                }
                0
            }
            _ => ENOSYS,
        }
    }
}
