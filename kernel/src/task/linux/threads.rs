// SPDX-License-Identifier: Apache-2.0
//! Manage Linux threads, suspended vfork parents and atomic BSP futex wait/wake deadlines.
//! No borrowed user memory or kernel continuation is retained while a task sleeps.
use super::*;

#[derive(Clone, Copy)]
pub(in crate::task) enum Blocked {
    Futex {
        key: usize,
        address: u64,
        deadline: Option<u64>,
        bits: u32,
    },
    Poll {
        frame: TrapFrame,
        deadline: Option<u64>,
    },
    Sleep {
        deadline: u64,
        remaining: u64,
    },
    Signal,
    FileLock {
        frame: TrapFrame,
    },
}
#[derive(Clone, Copy)]
pub(in crate::task) struct SignalInfo {
    pub sender: u32,
    pub code: i32,
    pub address: u64,
    pub status: i32,
}
pub(crate) struct Thread {
    pub tgid: u64,
    pub pgid: u64,
    pub sid: u64,
    pub(in crate::task) blocked: Option<Blocked>,
    pub stopped: bool,
    pub leader_exit: Option<Outcome>,
    pub saved_mask: Option<u64>,
    // A vfork parent cannot run or deliver handlers while its child shares this root.
    pub vfork_child: Option<(u64, usize)>,
    pub umask: u16,
    pub altstack: super::signals::AltStack,
    pub info: [SignalInfo; 64],
}
impl Thread {
    pub fn new(pid: u64) -> Self {
        Self {
            tgid: pid,
            pgid: pid,
            sid: pid,
            blocked: None,
            stopped: false,
            leader_exit: None,
            saved_mask: None,
            vfork_child: None,
            umask: 0o022,
            altstack: super::signals::AltStack::default(),
            info: [SignalInfo {
                sender: 0,
                code: 128,
                address: 0,
                status: 0,
            }; 64],
        }
    }
}

pub(in crate::task) fn deadline(
    space: &mm::user::UserSpace,
    address: u64,
    absolute: bool,
) -> Result<Option<u64>, i64> {
    if address == 0 {
        return Ok(None);
    }
    let mut b = [0; 16];
    space
        .with_active(|| uaccess::from_user(space, UserAddr(address), &mut b))
        .map_err(errno)?;
    let sec = u64::from_le_bytes(b[..8].try_into().unwrap());
    let ns = u64::from_le_bytes(b[8..].try_into().unwrap());
    if sec > i64::MAX as u64 || ns >= 1_000_000_000 {
        return Err(-22);
    }
    let ticks = sec
        .checked_mul(100)
        .and_then(|n| n.checked_add(ns.div_ceil(10_000_000)))
        .ok_or(-22)?;
    Ok(Some(if absolute {
        ticks
    } else {
        apic::ticks().checked_add(ticks).ok_or(-22)?
    }))
}
impl Kernel {
    pub(in crate::task) fn linux_clone(&mut self, i: usize, f: &TrapFrame) -> Result<u64, i64> {
        if f.rdi == 0x4111 {
            // musl posix_spawn: a distinct child stack on a shared, suspended-parent root.
            if !self.process(i).image.as_ref().unwrap().space.accessible(
                UserAddr(f.rsi.saturating_sub(16)),
                16,
                true,
                false,
            ) {
                return Err(-14);
            }
            let pid = self.fork_space(i, true).map_err(errno)?;
            let child = (0..CAPACITY)
                .find(|&k| self.policy.entry(k).pid == pid)
                .unwrap();
            self.process_mut(child).context.frame.rsp = f.rsi;
            // Linux disables inherited alternate stacks for CLONE_VM without CLONE_VFORK;
            // this CLONE_VFORK variant keeps the parent's registration until exec resets it.
            let identity = self.process(i).image.as_ref().unwrap().space.identity();
            self.process_mut(i).thread.vfork_child = Some((pid, identity));
            self.policy.block(i);
            return Ok(pid);
        }
        if f.rdi == 17 && f.rsi == 0 {
            return self.fork(i).map_err(errno);
        }
        const REQUIRED: u64 = 0x100 | 0x200 | 0x400 | 0x800 | 0x10000;
        const ALLOWED: u64 =
            REQUIRED | 0x40000 | 0x80000 | 0x100000 | 0x200000 | 0x400000 | 0x1000000;
        if f.rdi & REQUIRED != REQUIRED || f.rdi & !ALLOWED != 0 {
            return Err(-22);
        }
        #[cfg(feature = "qemu-test")]
        serial::write(format_args!(
            "[M9:CLONE] tid={} flags={:#x}\n",
            self.policy.entry(i).pid,
            f.rdi
        ));
        let parent = self.process(i);
        let image = parent.image.as_ref().unwrap();
        if !image
            .space
            .accessible(UserAddr(f.rsi.saturating_sub(16)), 16, true, false)
            || f.r8 >= (1u64 << 47)
        {
            return Err(-14);
        }
        for (flag, address) in [(0x100000, f.rdx), (0x1000000, f.r10), (0x200000, f.r10)] {
            if f.rdi & flag != 0 && !image.space.accessible(UserAddr(address), 4, true, false) {
                return Err(-14);
            }
        }
        let files = self.process_mut(i).files.share().map_err(errno)?;
        let slot = self
            .policy
            .insert(self.policy.entry(i).parent)
            .map_err(errno)?;
        let stack = match mm::user::KernelStack::new(slot) {
            Ok(s) => s,
            Err(_) => {
                self.policy.remove(slot);
                return Err(-12);
            }
        };
        let parent = self.process(i);
        let old = parent.image.as_ref().unwrap();
        let image = loader::Loaded {
            linux: true,
            brk: old.brk,
            clear_tid: if f.rdi & 0x200000 != 0 { f.r10 } else { 0 },
            sigmask: old.sigmask,
            ignored: old.ignored,
            pending: 0,
            actions: old.actions.clone(),
            space: old.space.share(),
            entry: old.entry,
            stack: old.stack,
        };
        let mut context = parent.context;
        context.frame.rax = 0;
        context.frame.rsp = f.rsi;
        if f.rdi & 0x80000 != 0 {
            context.set_fs_base(f.r8);
        }
        let mut thread = Thread::new(parent.thread.tgid);
        thread.pgid = parent.thread.pgid;
        thread.sid = parent.thread.sid;
        thread.umask = parent.thread.umask;
        let process = Process {
            image: Some(image),
            stack: Some(stack),
            context,
            files,
            wait: None,
            cwd: parent.cwd,
            io: None,
            network_deadline: None,
            thread,
        };
        let tid = self.policy.entry(slot).pid;
        let space = &process.image.as_ref().unwrap().space;
        let copied = space.with_active(|| -> Result<(), i64> {
            if f.rdi & 0x100000 != 0 {
                uaccess::to_user(space, UserAddr(f.rdx), &(tid as u32).to_le_bytes())
                    .map_err(errno)?;
            }
            if f.rdi & 0x1000000 != 0 {
                uaccess::to_user(space, UserAddr(f.r10), &(tid as u32).to_le_bytes())
                    .map_err(errno)?;
            }
            Ok(())
        });
        if let Err(e) = copied {
            self.policy.remove(slot);
            return Err(e);
        }
        self.processes[slot] = Some(process);
        Ok(tid)
    }
    pub(in crate::task) fn futex_wake(
        &mut self,
        key: usize,
        address: u64,
        count: u64,
        bits: u32,
    ) -> i64 {
        let mut woken = 0;
        for k in 0..CAPACITY {
            if woken >= count {
                break;
            }
            let Some(p) = self.processes[k].as_mut() else {
                continue;
            };
            if p.image.is_none() {
                continue;
            }
            if matches!(p.thread.blocked,Some(Blocked::Futex {key:k,address:a,bits:b,..}) if k==key && a==address && b&bits!=0)
            {
                p.thread.blocked = None;
                p.context.frame.rax = 0;
                if !p.thread.stopped {
                    self.policy.ready(k);
                }
                woken += 1;
            }
        }
        woken as i64
    }
    pub(in crate::task) fn linux_futex(&mut self, i: usize, f: &TrapFrame) -> Result<i64, i64> {
        if f.rdi & 3 != 0 || f.rsi & !0x18f != 0 {
            return Err(-22);
        }
        if f.rsi & 0x100 != 0 {
            return Err(ENOSYS);
        }
        let image = self.process(i).image.as_ref().unwrap();
        let (key, address) = image.space.futex_key(f.rdi, f.rsi & 128 != 0).ok_or(-14)?;
        let op = f.rsi & 0x7f;
        #[cfg(feature = "qemu-test")]
        {
            static COUNT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
            if COUNT.fetch_add(1, Ordering::Relaxed) < 80 {
                serial::write(format_args!(
                    "[M9:FUTEX] tid={} op={} private={} address={:#x} value={}\n",
                    self.policy.entry(i).pid,
                    op,
                    f.rsi & 128 != 0,
                    f.rdi,
                    f.rdx
                ));
            }
        }
        let bits = if matches!(op, 9 | 10) {
            f.r9 as u32
        } else {
            u32::MAX
        };
        if bits == 0 {
            return Err(-22);
        }
        let mut b = [0; 4];
        image
            .space
            .with_active(|| uaccess::from_user(&image.space, UserAddr(f.rdi), &mut b))
            .map_err(errno)?;
        match op {
            0 | 9 => {
                if u32::from_le_bytes(b) != f.rdx as u32 {
                    return Err(-11);
                }
                let deadline = deadline(&image.space, f.r10, op == 9)?;
                if deadline.is_some_and(|d| d <= apic::ticks()) {
                    return Err(-110);
                }
                // Value comparison and queue publication both run at IF=0: wake cannot be lost.
                self.process_mut(i).thread.blocked = Some(Blocked::Futex {
                    key,
                    address,
                    deadline,
                    bits,
                });
                self.policy.block(i);
                Ok(0)
            }
            3 | 4 => {
                if f.rdx > i32::MAX as u64
                    || f.r10 > i32::MAX as u64
                    || f.r8 & 3 != 0
                    || f.r8 == f.rdi
                {
                    return Err(-22);
                }
                if op == 4 && u32::from_le_bytes(b) != f.r9 as u32 {
                    return Err(-11);
                }
                let (destination_key, destination_address) =
                    image.space.futex_key(f.r8, f.rsi & 128 != 0).ok_or(-14)?;
                let mut target = [0; 4];
                image
                    .space
                    .with_active(|| uaccess::from_user(&image.space, UserAddr(f.r8), &mut target))
                    .map_err(errno)?;
                let woken = self.futex_wake(key, address, f.rdx, u32::MAX);
                let mut moved = 0;
                for p in self.processes.iter_mut().flatten() {
                    if moved >= f.r10 {
                        break;
                    }
                    if let Some(Blocked::Futex {
                        key: k, address: a, ..
                    }) = p.thread.blocked.as_mut()
                        && *k == key
                        && *a == address
                    {
                        *k = destination_key;
                        *a = destination_address;
                        moved += 1;
                    }
                }
                Ok(woken + moved as i64)
            }
            1 | 10 if f.rdx <= i32::MAX as u64 => Ok(self.futex_wake(key, address, f.rdx, bits)),
            1 | 10 => Err(-22),
            _ => Err(ENOSYS),
        }
    }
    pub(in crate::task) fn linux_clear_tid(&mut self, i: usize) {
        let Some(image) = self.process(i).image.as_ref() else {
            return;
        };
        if !image.linux || image.clear_tid == 0 {
            return;
        }
        let (key, address) = image
            .space
            .futex_key(image.clear_tid, false)
            .unwrap_or((0, 0));
        let user_address = image.clear_tid;
        let cleared = image
            .space
            .with_active(|| uaccess::to_user(&image.space, UserAddr(user_address), &[0; 4]));
        if cleared.is_ok() {
            self.futex_wake(key, address, 1, u32::MAX);
        }
    }
    pub(in crate::task) fn linux_exit_group(&mut self, i: usize, outcome: Outcome) {
        let group = self.process(i).thread.tgid;
        for k in 0..CAPACITY {
            if k != i
                && self.processes[k]
                    .as_ref()
                    .is_some_and(|p| p.image.is_some() && p.thread.tgid == group)
            {
                self.finish(k, outcome);
            }
        }
        if self.process(i).image.is_some() {
            self.finish(i, outcome);
        }
    }
    pub(in crate::task) fn linux_thread_exit(&mut self, i: usize, code: u32) {
        let group = self.process(i).thread.tgid;
        if group == self.policy.entry(i).pid
            && (0..CAPACITY).any(|k| {
                k != i
                    && self.processes[k]
                        .as_ref()
                        .is_some_and(|p| p.image.is_some() && p.thread.tgid == group)
            })
        {
            self.process_mut(i).thread.leader_exit = Some(Outcome::Exit(code));
            self.policy.block(i);
        } else {
            self.finish(i, Outcome::Exit(code));
            let leader = (0..CAPACITY).find(|&k| {
                self.processes[k]
                    .as_ref()
                    .is_some_and(|p| p.thread.tgid == group && p.thread.leader_exit.is_some())
            });
            if let Some(leader) = leader
                && !(0..CAPACITY).any(|k| {
                    k != leader
                        && self.processes[k]
                            .as_ref()
                            .is_some_and(|p| p.image.is_some() && p.thread.tgid == group)
                })
            {
                let outcome = self.process_mut(leader).thread.leader_exit.take().unwrap();
                self.finish(leader, outcome);
            }
        }
    }
    pub(in crate::task) fn linux_wake(&mut self) {
        self.terminal.refresh(&self.fs);
        while let Some((group, event)) = self.terminal.take_signal() {
            let signal = match event {
                fs::TerminalSignal::Interrupt => 2,
                fs::TerminalSignal::Quit => 3,
                fs::TerminalSignal::Suspend => 20,
                fs::TerminalSignal::WindowChanged => 28,
                fs::TerminalSignal::Hangup => 1,
                fs::TerminalSignal::Continue => 18,
            };
            for i in 0..CAPACITY {
                if self.processes[i].as_ref().is_some_and(|p| {
                    p.image.as_ref().is_some_and(|x| x.linux) && p.thread.pgid == group
                }) {
                    self.queue_signal(i, signal);
                }
            }
        }
        for i in 0..CAPACITY {
            if self.processes[i].as_ref().is_none_or(|p| {
                p.image.is_none() || p.thread.stopped || p.thread.leader_exit.is_some()
            }) {
                continue;
            }
            if let Some((pid, identity)) = self.process(i).thread.vfork_child {
                let shared = self.processes.iter().enumerate().any(|(slot, child)| {
                    self.policy.entry(slot).pid == pid
                        && child
                            .as_ref()
                            .and_then(|p| p.image.as_ref())
                            .is_some_and(|image| image.space.identity() == identity)
                });
                if shared {
                    continue;
                }
                self.process_mut(i).thread.vfork_child = None;
                self.policy.ready(i);
            }
            if self.linux_pending(i).is_some() {
                self.policy.ready(i);
                continue;
            }
            if let Some((target, _)) = self.process(i).wait {
                let group = self.process(i).thread.tgid;
                let mut any = false;
                let mut ready = false;
                for k in 0..CAPACITY {
                    let entry = self.policy.entry(k);
                    if entry.parent == group
                        && entry.state != State::Empty
                        && (target == 0 || entry.pid == target)
                    {
                        any = true;
                        ready |= matches!(entry.state, State::Zombie(_));
                    }
                }
                if ready || !any {
                    self.policy.ready(i);
                }
            }
            let blocked = self.process(i).thread.blocked;
            let result = match blocked {
                Some(Blocked::FileLock { frame }) => {
                    let mode = if frame.rsi & 1 != 0 { 1 } else { 2 };
                    match self.fs.try_lock(
                        &self.processes[i].as_ref().unwrap().files,
                        frame.rdi,
                        mode,
                    ) {
                        Ok(()) => Some(0),
                        Err(WOULD_BLOCK) => None,
                        Err(e) => Some(errno(e)),
                    }
                }
                Some(Blocked::Futex {
                    deadline: Some(d), ..
                }) if apic::ticks() >= d => Some(-110),
                Some(Blocked::Sleep { deadline, .. }) if apic::ticks() >= deadline => Some(0),
                Some(Blocked::Poll { frame, deadline }) => {
                    let result = self.poll_descriptors(i, &frame);
                    if result != 0 || deadline.is_some_and(|d| apic::ticks() >= d) {
                        Some(result)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(result) = result {
                self.process_mut(i).thread.blocked = None;
                if matches!(blocked, Some(Blocked::Poll { .. })) {
                    self.restore_poll_mask(i);
                }
                self.process_mut(i).context.frame.rax = result as u64;
                self.policy.ready(i);
            }
        }
    }
}
