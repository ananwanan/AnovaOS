// SPDX-License-Identifier: Apache-2.0
//! Translate Linux poll/ppoll, terminal ioctls and process-group/session requests.
//! Readiness uses actual VFS/TTY state, including pipe EOF, errors and finite tick deadlines.
use super::*;
use anova_vfs::Device;
impl Kernel {
    pub(in crate::task) fn poll_descriptors(&mut self, i: usize, f: &TrapFrame) -> i64 {
        if f.rsi > 32 {
            return -22;
        }
        let image = self.process(i).image.as_ref().unwrap();
        let space = &image.space;
        let p = self.process(i);
        let mut bytes = [0; 256];
        let n = f.rsi as usize * 8;
        space
            .with_active(|| -> Result<i64, i64> {
                uaccess::from_user(space, UserAddr(f.rdi), &mut bytes[..n]).map_err(errno)?;
                let mut count = 0;
                for entry in bytes[..n].chunks_exact_mut(8) {
                    let fd = i32::from_le_bytes(entry[..4].try_into().unwrap());
                    let events = u16::from_le_bytes(entry[4..6].try_into().unwrap());
                    let mut result = 0u16;
                    if fd >= 0 {
                        match self.fs.readiness(&p.files, fd as u64) {
                            Err(_) => result = 32,
                            Ok((mut read, mut write, hup, error)) => {
                                let device = self
                                    .fs
                                    .device(&p.files, fd as u64, false)
                                    .or_else(|_| self.fs.device(&p.files, fd as u64, true))
                                    .ok()
                                    .flatten();
                                if let Some(device) =
                                    device.filter(|d| self.terminal.is_terminal(*d))
                                {
                                    read = self.terminal.endpoint_ready(device, false, &self.fs);
                                    write = self.terminal.endpoint_ready(device, true, &self.fs);
                                }
                                if read {
                                    result |= events & 1;
                                }
                                if write {
                                    result |= events & 4;
                                }
                                if hup {
                                    result |= 16;
                                }
                                if error {
                                    result |= 8;
                                }
                            }
                        }
                    }
                    entry[6..8].copy_from_slice(&result.to_le_bytes());
                    if result != 0 {
                        count += 1;
                    }
                }
                uaccess::to_user(space, UserAddr(f.rdi), &bytes[..n]).map_err(errno)?;
                Ok(count)
            })
            .unwrap_or_else(|e| e)
    }
    pub(in crate::task) fn linux_poll(&mut self, i: usize, f: &TrapFrame) -> Result<i64, i64> {
        let deadline = if f.rax == 7 {
            let ms = f.rdx as i32;
            if ms < 0 {
                None
            } else {
                Some(
                    apic::ticks()
                        .checked_add((ms as u64).div_ceil(10))
                        .ok_or(-22)?,
                )
            }
        } else {
            let image = self.process(i).image.as_ref().unwrap();
            let deadline = threads::deadline(&image.space, f.rdx, false)?;
            if f.r10 != 0 {
                if f.r8 != 8 {
                    return Err(-22);
                }
                let mut b = [0; 8];
                image
                    .space
                    .with_active(|| uaccess::from_user(&image.space, UserAddr(f.r10), &mut b))
                    .map_err(errno)?;
                let old = image.sigmask;
                self.process_mut(i).thread.saved_mask = Some(old);
                self.process_mut(i).image.as_mut().unwrap().sigmask =
                    u64::from_le_bytes(b) & !((1 << 8) | (1 << 18));
            }
            deadline
        };
        let result = self.poll_descriptors(i, f);
        if result != 0 || deadline.is_some_and(|d| d <= apic::ticks()) {
            self.restore_poll_mask(i);
            return Ok(result);
        }
        self.process_mut(i).thread.blocked = Some(threads::Blocked::Poll {
            frame: *f,
            deadline,
        });
        self.policy.block(i);
        Ok(0)
    }
    pub(in crate::task) fn restore_poll_mask(&mut self, i: usize) {
        if let Some(mask) = self.process_mut(i).thread.saved_mask.take() {
            self.process_mut(i).image.as_mut().unwrap().sigmask = mask;
        }
    }
    pub(in crate::task) fn linux_terminal(&mut self, i: usize, f: &TrapFrame) -> Result<i64, i64> {
        if f.rax != 16 {
            let pid = self.policy.entry(i).pid;
            match f.rax {
                109 => {
                    let target = if f.rdi == 0 { pid } else { f.rdi };
                    let slot = (0..CAPACITY)
                        .find(|&k| {
                            self.processes[k]
                                .as_ref()
                                .is_some_and(|p| p.image.is_some())
                                && self.policy.entry(k).pid == target
                        })
                        .ok_or(-3)?;
                    if target != pid
                        && self.policy.entry(slot).parent != self.process(i).thread.tgid
                    {
                        return Err(-3);
                    }
                    let group = if f.rsi == 0 { target } else { f.rsi };
                    let session = self.process(i).thread.sid;
                    if self.process(slot).thread.sid != session
                        || self.process(slot).thread.sid == target
                    {
                        return Err(-1);
                    }
                    if group != target
                        && !self
                            .processes
                            .iter()
                            .flatten()
                            .any(|p| p.thread.pgid == group && p.thread.sid == session)
                    {
                        return Err(-1);
                    }
                    let tgid = self.process(slot).thread.tgid;
                    for p in self
                        .processes
                        .iter_mut()
                        .flatten()
                        .filter(|p| p.thread.tgid == tgid)
                    {
                        p.thread.pgid = group;
                    }
                    Ok(0)
                }
                111 => Ok(self.process(i).thread.pgid as i64),
                112 => {
                    if self
                        .processes
                        .iter()
                        .flatten()
                        .any(|p| p.thread.pgid == pid)
                    {
                        return Err(-1);
                    }
                    let group = self.process(i).thread.tgid;
                    for p in self
                        .processes
                        .iter_mut()
                        .flatten()
                        .filter(|p| p.thread.tgid == group)
                    {
                        p.thread.sid = pid;
                        p.thread.pgid = pid;
                    }
                    Ok(pid as i64)
                }
                121 | 124 => {
                    let pid = if f.rdi == 0 { pid } else { f.rdi };
                    let p = (0..CAPACITY)
                        .find(|&k| {
                            self.processes[k]
                                .as_ref()
                                .is_some_and(|p| p.image.is_some())
                                && self.policy.entry(k).pid == pid
                        })
                        .ok_or(-3)?;
                    Ok(if f.rax == 121 {
                        self.process(p).thread.pgid
                    } else {
                        self.process(p).thread.sid
                    } as i64)
                }
                _ => Err(ENOSYS),
            }
        } else {
            let p = self.process(i);
            let d = self
                .fs
                .device(&p.files, f.rdi, false)
                .or_else(|_| self.fs.device(&p.files, f.rdi, true))
                .map_err(errno)?
                .ok_or(-25)?;
            if !self.terminal.is_terminal(d) {
                return Err(-25);
            }
            let space = p.image.as_ref().unwrap().space.share();
            let mut b = [0; 36];
            let output = match f.rsi {
                0x5401 => {
                    b = self.terminal.settings(d);
                    36
                }
                0x540f => {
                    b[..4].copy_from_slice(&(self.terminal.foreground(d) as u32).to_le_bytes());
                    4
                }
                0x5413 => {
                    b[..8].copy_from_slice(&self.terminal.window(d));
                    8
                }
                0x8004_5430 => {
                    let Device::PtyMaster(n) = d else {
                        return Err(-25);
                    };
                    b[..4].copy_from_slice(&(n as u32).to_le_bytes());
                    4
                }
                0x5403 | 0x5404 => return Err(ENOSYS),
                0x5402 | 0x5410 | 0x5414 | 0x4004_5431 => 0,
                _ => return Err(-25),
            };
            if output != 0 {
                space
                    .with_active(|| uaccess::to_user(&space, UserAddr(f.rdx), &b[..output]))
                    .map_err(errno)?;
                return Ok(0);
            }
            let len = if matches!(f.rsi, 0x5402..=0x5404) {
                36
            } else if f.rsi == 0x5414 {
                8
            } else {
                4
            };
            space
                .with_active(|| uaccess::from_user(&space, UserAddr(f.rdx), &mut b[..len]))
                .map_err(errno)?;
            match f.rsi {
                0x5402..=0x5404 => {
                    // Support the measured default canonical profile and VMIN=1/VTIME=0 raw profile.
                    if b[12] & 2 == 0 && (b[22] != 0 || b[23] != 1) {
                        return Err(-95);
                    }
                    self.terminal.set_settings(d, b, f.rsi == 0x5404);
                }
                0x5410 => {
                    let group = u32::from_le_bytes(b[..4].try_into().unwrap()) as u64;
                    let sid = self.process(i).thread.sid;
                    if self.terminal.session(d) != sid {
                        return Err(-25);
                    }
                    if group == 0
                        || !self.processes.iter().flatten().any(|p| {
                            p.image.is_some() && p.thread.pgid == group && p.thread.sid == sid
                        })
                    {
                        return Err(-1);
                    }
                    self.terminal.set_group(d, group);
                }
                0x5414 => self.terminal.set_window(d, b[..8].try_into().unwrap()),
                _ => {
                    let Device::PtyMaster(n) = d else {
                        return Err(-25);
                    };
                    let locked = u32::from_le_bytes(b[..4].try_into().unwrap());
                    if locked > 1 {
                        return Err(-22);
                    }
                    self.fs.pty_lock(n, locked != 0);
                }
            }
            Ok(0)
        }
    }
}
