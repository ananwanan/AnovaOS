// SPDX-License-Identifier: Apache-2.0
//! Deliver AMD64 Linux signal frames and import checked sigreturn state with x87/SSE preservation.
//! Dispositions belong to a shared thread group; masks and pending signals belong to each task.
use super::*;

#[derive(Clone, Copy, Default)]
pub(crate) struct Action {
    pub handler: u64,
    pub flags: u64,
    pub restorer: u64,
    pub mask: u64,
}
const RESTART: u64 = 0x1000_0000;
const UNBLOCKABLE: u64 = (1 << 8) | (1 << 18);
fn word(bytes: &[u8], index: usize) -> u64 {
    u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
}
fn put(bytes: &mut [u8], index: usize, value: u64) {
    bytes[index * 8..index * 8 + 8].copy_from_slice(&value.to_le_bytes());
}

impl Kernel {
    pub(in crate::task) fn linux_notify_exit(&mut self, i: usize, outcome: Outcome) {
        let pid = self.policy.entry(i).pid;
        let parent = self.policy.entry(i).parent;
        if self.process(i).thread.tgid == pid
            && let Some(parent) = (0..CAPACITY).find(|&k| {
                self.policy.entry(k).pid == parent
                    && self.processes[k]
                        .as_ref()
                        .is_some_and(|p| p.image.as_ref().is_some_and(|x| x.linux))
            })
        {
            self.queue_signal_info(
                parent,
                17,
                pid as u32,
                if outcome.status().reason == 0 { 1 } else { 2 },
                0,
                outcome.status().code as i32,
            );
        }
    }
    pub(in crate::task) fn linux_bad_return(&mut self, i: usize) {
        let f = self.process(i).context.frame;
        serial::write(format_args!(
            "[LINUX:BAD_RETURN] tid={} rip={:#x} rsp={:#x}\n",
            self.policy.entry(i).pid,
            f.rip,
            f.rsp
        ));
        self.queue_signal(i, 11);
        self.policy.ready(i);
    }
    pub(in crate::task) fn linux_exception(&mut self, i: usize, f: &TrapFrame, address: u64) {
        serial::write(format_args!(
            "[LINUX:FAULT] tid={} vector={} error={:#x} rip={:#x} rsp={:#x} address={:#x}\n",
            self.policy.entry(i).pid,
            f.vector,
            f.error,
            f.rip,
            f.rsp,
            address
        ));
        let signal = match f.vector {
            0 | 16 | 19 => 8,
            1 | 3 => 5,
            6 => 4,
            17 => 7,
            _ => 11,
        };
        let code = match f.vector {
            14 => {
                if f.error & 1 != 0 {
                    2
                } else {
                    1
                }
            }
            6 => 2,
            0 => 1,
            _ => 128,
        };
        self.queue_signal_info(
            i,
            signal,
            0,
            code,
            if f.vector == 14 { address } else { f.rip },
            0,
        );
        self.policy.ready(i);
    }
    pub(in crate::task) fn queue_signal(&mut self, i: usize, signal: u32) {
        self.queue_signal_info(i, signal, 0, 128, 0, 0);
    }
    pub(in crate::task) fn queue_signal_info(
        &mut self,
        i: usize,
        signal: u32,
        sender: u32,
        code: i32,
        address: u64,
        status: i32,
    ) {
        if signal == 18 || signal == 9 {
            self.process_mut(i).thread.stopped = false;
            self.policy.ready(i);
        }
        let p = self.process_mut(i);
        let Some(image) = p.image.as_mut() else {
            return;
        };
        if image.pending & (1u64 << (signal - 1)) == 0 {
            p.thread.info[signal as usize - 1] = threads::SignalInfo {
                sender,
                code,
                address,
                status,
            };
        }
        image.pending |= 1u64 << (signal - 1);
        if signal == 18 {
            image.pending &= !((1 << 18) | (1 << 19) | (1 << 20) | (1 << 21));
        }
    }
    pub(in crate::task) fn signal_syscall(&mut self, i: usize, f: &TrapFrame) -> i64 {
        if f.rax == 15 {
            if self.signal_return(i).is_err() {
                self.linux_exit_group(i, Outcome::Signal(11));
            }
            return 0;
        }
        if f.rax == 62 || f.rax == 200 || f.rax == 234 {
            let (target, sig) = if f.rax == 234 {
                (f.rsi as i64, f.rdx)
            } else {
                (f.rdi as i64, f.rsi)
            };
            if sig > 64 {
                return -22;
            }
            let group = self.process(i).thread.pgid;
            let sender = self.process(i).thread.tgid as u32;
            let mut found = false;
            for k in 0..CAPACITY {
                let Some(p) = self.processes[k].as_ref() else {
                    continue;
                };
                if p.image.as_ref().is_none_or(|x| !x.linux) {
                    continue;
                }
                let eligible = if f.rax == 62 {
                    match target {
                        0 => p.thread.pgid == group && p.thread.tgid == self.policy.entry(k).pid,
                        -1 => p.thread.tgid == self.policy.entry(k).pid,
                        n if n < 0 => {
                            p.thread.pgid == n.unsigned_abs()
                                && p.thread.tgid == self.policy.entry(k).pid
                        }
                        n => p.thread.tgid == n as u64 && p.thread.tgid == self.policy.entry(k).pid,
                    }
                } else {
                    target > 0
                        && self.policy.entry(k).pid == target as u64
                        && (f.rax != 234 || p.thread.tgid == f.rdi)
                };
                if eligible {
                    found = true;
                    if sig != 0 {
                        self.queue_signal_info(
                            k,
                            sig as u32,
                            sender,
                            if f.rax == 62 { 0 } else { -6 },
                            0,
                            0,
                        );
                    }
                }
            }
            return if found { 0 } else { -3 };
        }
        if f.rax == 130 {
            if f.rsi != 8 {
                return -22;
            }
            let image = self.process(i).image.as_ref().unwrap();
            let mut b = [0; 8];
            if image
                .space
                .with_active(|| uaccess::from_user(&image.space, UserAddr(f.rdi), &mut b))
                .is_err()
            {
                return -14;
            }
            let old = image.sigmask;
            let p = self.process_mut(i);
            p.thread.saved_mask = Some(old);
            p.image.as_mut().unwrap().sigmask = u64::from_le_bytes(b) & !UNBLOCKABLE;
            p.thread.blocked = Some(super::threads::Blocked::Signal);
            self.policy.block(i);
            return -4;
        }
        if f.rax == 131 {
            // Alternate stacks are explicitly unsupported; querying the disabled state is truthful.
            if f.rdi != 0 {
                return ENOSYS;
            }
            if f.rsi == 0 {
                return 0;
            }
            let image = self.process(i).image.as_ref().unwrap();
            let mut b = [0; 24];
            b[8..12].copy_from_slice(&2u32.to_le_bytes());
            return image
                .space
                .with_active(|| uaccess::to_user(&image.space, UserAddr(f.rsi), &b))
                .map_or_else(errno, |()| 0);
        }
        if f.r10 != 8 {
            return -22;
        }
        let image = self.process(i).image.as_ref().unwrap();
        let mut input = [0; 32];
        let mut output = [0; 32];
        let len = if f.rax == 14 { 8 } else { 32 };
        if f.rax == 13 {
            if !(1..=64).contains(&f.rdi) || matches!(f.rdi, 9 | 19) {
                return -22;
            }
            let action = image.actions.borrow()[f.rdi as usize - 1];
            for (k, v) in [action.handler, action.flags, action.restorer, action.mask]
                .iter()
                .enumerate()
            {
                put(&mut output, k, *v);
            }
        } else {
            if f.rdi > 2 {
                return -22;
            }
            put(&mut output, 0, image.sigmask);
        }
        let copied = image.space.with_active(|| -> Result<(), i64> {
            if f.rsi != 0 {
                uaccess::from_user(&image.space, UserAddr(f.rsi), &mut input[..len])
                    .map_err(errno)?;
            }
            if f.rax == 13 && f.rsi != 0 {
                let (handler, flags, restorer) =
                    (word(&input, 0), word(&input, 1), word(&input, 2));
                if flags & !(0x0400_0000 | RESTART | 4 | 1 | 0x4000_0000 | 0x8000_0000) != 0 {
                    return Err(-22);
                }
                if handler > 1
                    && (flags & 0x0400_0000 == 0
                        || !image.space.accessible(UserAddr(handler), 1, false, true)
                        || !image.space.accessible(UserAddr(restorer), 1, false, true))
                {
                    return Err(-14);
                }
            }
            if f.rdx != 0 {
                uaccess::to_user(&image.space, UserAddr(f.rdx), &output[..len]).map_err(errno)?;
            }
            Ok(())
        });
        if let Err(e) = copied {
            return e;
        }
        if f.rsi != 0 {
            let image = self.process_mut(i).image.as_mut().unwrap();
            let value = word(&input, 0);
            if f.rax == 13 {
                image.actions.borrow_mut()[f.rdi as usize - 1] = Action {
                    handler: value,
                    flags: word(&input, 1),
                    restorer: word(&input, 2),
                    mask: word(&input, 3) & !UNBLOCKABLE,
                };
                let bit = 1u64 << (f.rdi - 1);
                image.ignored = (image.ignored & !bit) | if value == 1 { bit } else { 0 };
                if value == 1 {
                    image.pending &= !bit;
                }
            } else {
                image.sigmask = match f.rdi {
                    0 => image.sigmask | value,
                    1 => image.sigmask & !value,
                    _ => value,
                } & !UNBLOCKABLE;
            }
        }
        0
    }
    pub(in crate::task) fn deliver_signal(&mut self, i: usize, signal: u32) -> Result<bool, u32> {
        let image = self.process(i).image.as_ref().unwrap();
        let action = image.actions.borrow()[signal as usize - 1];
        if action.handler == 0 && matches!(signal, 17 | 18 | 23 | 28) {
            self.process_mut(i).image.as_mut().unwrap().pending &= !(1u64 << (signal - 1));
            return Ok(true);
        }
        if action.handler == 0 && matches!(signal, 19..=22) {
            self.process_mut(i).image.as_mut().unwrap().pending &= !(1u64 << (signal - 1));
            self.process_mut(i).thread.stopped = true;
            self.policy.block(i);
            return Ok(false);
        }
        if action.handler == 0 || signal == 9 {
            return Err(signal);
        }
        let p = self.process_mut(i);
        let interrupted = p.io.is_some() || p.wait.is_some() || p.thread.blocked.is_some();
        p.network_deadline = None;
        let original = p.io.take().unwrap_or(p.context.frame);
        if interrupted {
            p.wait = None;
            if let Some(super::threads::Blocked::Sleep {
                deadline,
                remaining,
            }) = p.thread.blocked
                && remaining != 0
            {
                let image = p.image.as_ref().unwrap();
                let left = deadline.saturating_sub(apic::ticks());
                let mut b = [0; 16];
                put(&mut b, 0, left / 100);
                put(&mut b, 1, (left % 100) * 10_000_000);
                let _ = image
                    .space
                    .with_active(|| uaccess::to_user(&image.space, UserAddr(remaining), &b));
            }
            p.thread.blocked = None;
            p.context.frame = original;
            if action.flags & RESTART != 0 && matches!(original.rax, 0 | 1 | 19 | 20 | 44 | 45 | 61)
            {
                p.context.frame.rip = p.context.frame.rip.checked_sub(2).ok_or(11u32)?;
            } else {
                p.context.frame.rax = (-4i64) as u64;
            }
        }
        let frame = p.context.frame;
        let base = frame.rsp.checked_sub(128 + 1024).ok_or(11u32)? & !63;
        let base = base.checked_sub(8).ok_or(11u32)?;
        let mut bytes = [0; 1024];
        put(&mut bytes, 0, action.restorer);
        bytes[32..36].copy_from_slice(&2u32.to_le_bytes()); // Disabled alternate stack.
        let registers = [
            frame.r8,
            frame.r9,
            frame.r10,
            frame.r11,
            frame.r12,
            frame.r13,
            frame.r14,
            frame.r15,
            frame.rdi,
            frame.rsi,
            frame.rbp,
            frame.rbx,
            frame.rdx,
            frame.rax,
            frame.rcx,
            frame.rsp,
            frame.rip,
            frame.rflags,
            0x002b_0000_0000_0033,
            frame.error,
            frame.vector,
            0,
            0,
            base + 456,
        ];
        for (k, v) in registers.iter().enumerate() {
            put(&mut bytes[48..], k, *v);
        }
        let oldmask = p
            .thread
            .saved_mask
            .take()
            .unwrap_or(p.image.as_ref().unwrap().sigmask);
        put(&mut bytes, 38, oldmask);
        bytes[312..316].copy_from_slice(&signal.to_le_bytes());
        let info = p.thread.info[signal as usize - 1];
        bytes[320..324].copy_from_slice(&info.code.to_le_bytes());
        if matches!(signal, 4 | 7 | 8 | 11) && info.code > 0 {
            bytes[328..336].copy_from_slice(&info.address.to_le_bytes());
        } else {
            bytes[328..332].copy_from_slice(&info.sender.to_le_bytes());
            if signal == 17 {
                bytes[336..340].copy_from_slice(&info.status.to_le_bytes());
            }
        }
        bytes[456..968].copy_from_slice(p.context.fpu_bytes());
        let image = p.image.as_mut().unwrap();
        image
            .space
            .with_active(|| uaccess::to_user(&image.space, UserAddr(base), &bytes))
            .map_err(|_| 11u32)?;
        image.pending &= !(1u64 << (signal - 1));
        image.sigmask |= action.mask;
        if action.flags & 0x4000_0000 == 0 {
            image.sigmask |= 1u64 << (signal - 1);
        }
        if action.flags & 0x8000_0000 != 0 {
            image.actions.borrow_mut()[signal as usize - 1] = Action::default();
        }
        p.context.frame.rip = action.handler;
        p.context.frame.rsp = base;
        p.context.frame.rdi = signal as u64;
        p.context.frame.rsi = base + 312;
        p.context.frame.rdx = base + 8;
        p.context.frame.rax = 0;
        Ok(true)
    }
    fn signal_return(&mut self, i: usize) -> Result<(), i64> {
        let p = self.process_mut(i);
        let image = p.image.as_mut().unwrap();
        let base = p.context.frame.rsp.checked_sub(8).ok_or(-14)?;
        let mut bytes = [0; 312];
        let mut fpu = [0; 512];
        image.space.with_active(|| -> Result<(), i64> {
            uaccess::from_user(&image.space, UserAddr(base), &mut bytes).map_err(errno)?;
            let fp = word(&bytes[48..], 23);
            uaccess::from_user(&image.space, UserAddr(fp), &mut fpu).map_err(errno)?;
            Ok(())
        })?;
        if word(&bytes[48..], 18) != 0x002b_0000_0000_0033 || !p.context.restore_fpu(&fpu) {
            return Err(-22);
        }
        let r = |k| word(&bytes[48..], k);
        let frame = &mut p.context.frame;
        frame.r8 = r(0);
        frame.r9 = r(1);
        frame.r10 = r(2);
        frame.r11 = r(3);
        frame.r12 = r(4);
        frame.r13 = r(5);
        frame.r14 = r(6);
        frame.r15 = r(7);
        frame.rdi = r(8);
        frame.rsi = r(9);
        frame.rbp = r(10);
        frame.rbx = r(11);
        frame.rdx = r(12);
        frame.rax = r(13);
        frame.rcx = r(14);
        frame.rsp = r(15);
        frame.rip = r(16);
        frame.rflags = r(17);
        image.sigmask = word(&bytes, 38) & !UNBLOCKABLE;
        Ok(())
    }
}
