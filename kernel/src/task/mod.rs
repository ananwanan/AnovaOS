// SPDX-License-Identifier: Apache-2.0
//! Schedule bounded native/Linux processes and shared-root threads with private stacks on one BSP.
//! User traps capture state; lifecycle, polled devices, copying and reclamation run on the kernel root.
//! Optional bootstrap diagnostics sample resource use within each independent scheduler instance.
#[cfg(feature = "bootstrap-metrics")]
mod bootstrap;
mod diagnostics;
mod linux;
pub(crate) fn network_seed(bytes: &mut [u8]) -> Result<(), i64> {
    linux::random(bytes)
}
mod loader;
mod process_control;
mod pty;
#[cfg(feature = "qemu-test")]
mod validation;
use crate::{
    abi,
    arch::x86_64::{
        apic, cpu,
        traps::{self, TrapFrame},
        user,
    },
    boot, fs, mm, serial,
};
use alloc::{borrow::Cow, vec::Vec};
use anova_abi::*;
use anova_sched::{CAPACITY, Scheduler, State};
use anova_vfs::{Descriptors, ROOT, Vfs};
use core::{
    arch::asm,
    ptr,
    sync::atomic::{AtomicPtr, AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Exit(u32),
    Signal(u32),
    Fault {
        vector: u64,
        error: u64,
        address: u64,
    },
    BadReturn,
}
impl Outcome {
    fn status(self) -> ExitStatus {
        match self {
            Self::Exit(code) => ExitStatus { reason: 0, code },
            Self::Signal(code) => ExitStatus { reason: 3, code },
            Self::Fault { vector, .. } => ExitStatus {
                reason: 1,
                code: vector as u32,
            },
            Self::BadReturn => ExitStatus { reason: 2, code: 0 },
        }
    }
}
struct Process {
    image: Option<loader::Loaded>,
    stack: Option<mm::user::KernelStack>,
    context: user::Context,
    files: Descriptors,
    wait: Option<(u64, UserAddr)>,
    cwd: usize,
    io: Option<TrapFrame>,
    network_deadline: Option<u64>,
    thread: linux::threads::Thread,
    pty_member: Option<u64>,
}
static CAPTURE: AtomicPtr<user::Context> = AtomicPtr::new(ptr::null_mut());
static USER_IRQS: AtomicU64 = AtomicU64::new(0);
static FAULT_ADDRESS: AtomicU64 = AtomicU64::new(0);

pub fn note_timer() {
    USER_IRQS.fetch_add(1, Ordering::Relaxed);
}

/// No allocation, lock, owned local, or live process reference crosses this tail transfer.
pub fn dispatch(frame: &mut TrapFrame) {
    let pointer = CAPTURE.load(Ordering::Acquire);
    assert!(
        !pointer.is_null(),
        "User trap without an active capture buffer"
    );
    let address: u64;
    // SAFETY: CPL0 captures CR2 before returning to the scheduler; only #PF uses it.
    unsafe {
        asm!("mov {},cr2",out(reg) address,options(nomem,nostack,preserves_flags));
    }
    FAULT_ADDRESS.store(
        if frame.vector == 14 { address } else { 0 },
        Ordering::Relaxed,
    );
    // SAFETY: enter() publishes a separate live capture local for the sole BSP.
    // It has no borrowed process/context alias and IRQ gates have disabled IF.
    unsafe {
        (*pointer).capture(frame);
    }
    // SAFETY: Assembly restores the suspended kernel root/stack and callee-saved
    // registers. This function and the trap caller have no destructors or locks.
    unsafe { user::anova_leave_user() }
}

#[derive(Default)]
struct Metrics {
    preemptions: u64,
    preempt_switches: u64,
    switches: u64,
    sleeps: u64,
    waits: u64,
    idle: u64,
    forks: u64,
    execs: u64,
    spawns: u64,
    reaps: u64,
    io_blocks: u64,
    io_wakes: u64,
}
struct Kernel {
    policy: Scheduler,
    processes: Vec<Option<Process>>,
    metrics: Metrics,
    fs: Vfs<'static>,
    network: Option<crate::net::Stack>,
    terminal: fs::Terminal,
    ptys: [Option<pty::Session>; 4],
    #[cfg(feature = "gui")]
    gui: crate::gui::Gui,
    #[cfg(feature = "bootstrap-metrics")]
    resources: bootstrap::Samples,
}
impl Kernel {
    fn new(first_pid: u64) -> Result<Self, loader::LoadError> {
        let mut processes = Vec::new();
        processes
            .try_reserve_exact(CAPACITY)
            .map_err(|_| anova_memory::Error::OutOfMemory)?;
        processes.resize_with(CAPACITY, || None);
        Ok(Self {
            policy: Scheduler::new(first_pid),
            processes,
            metrics: Metrics::default(),
            fs: Vfs::new(boot::initramfs()).map_err(|_| anova_memory::Error::OutOfMemory)?,
            network: None,
            terminal: fs::Terminal::new().map_err(|_| anova_memory::Error::OutOfMemory)?,
            ptys: core::array::from_fn(|_| None),
            #[cfg(feature = "gui")]
            gui: crate::gui::Gui::default(),
            #[cfg(feature = "bootstrap-metrics")]
            resources: bootstrap::Samples::new(),
        })
    }
    fn load_image(
        &mut self,
        bytes: &[u8],
        personality: &str,
        args: &[&str],
        env: &[&str],
    ) -> Result<loader::Loaded, loader::LoadError> {
        let data = if personality == anova_abi::linux::PERSONALITY {
            match anova_elf::interpreter(bytes)? {
                Some(path) => Some(self.fs.load_executable(ROOT, path).map_err(|error| {
                    serial::write(format_args!(
                        "[LINUX:INTERPRETER] path={path} errno={}\n",
                        anova_abi::linux::errno(error)
                    ));
                    loader::LoadError::Interpreter(anova_abi::linux::errno(error))
                })?),
                None => None,
            }
        } else {
            None
        };
        loader::load_interpreter(bytes, personality, args, env, data.as_deref())
    }
    fn process(&self, i: usize) -> &Process {
        self.processes[i].as_ref().expect("Missing process")
    }
    fn process_mut(&mut self, i: usize) -> &mut Process {
        self.processes[i].as_mut().expect("Missing process")
    }
    #[cfg(feature = "qemu-test")]
    fn add(&mut self, bytes: &[u8], args: &[&str], parent: u64) -> Result<usize, i64> {
        self.add_personality(bytes, args, parent, PERSONALITY)
    }
    fn add_personality(
        &mut self,
        bytes: &[u8],
        args: &[&str],
        parent: u64,
        personality: &str,
    ) -> Result<usize, i64> {
        self.add_environment(bytes, args, parent, personality, &[])
    }
    fn add_environment(
        &mut self,
        bytes: &[u8],
        args: &[&str],
        parent: u64,
        personality: &str,
        environment: &[&str],
    ) -> Result<usize, i64> {
        let i = self.policy.insert(parent)?;
        let result = (|| {
            let stack = mm::user::KernelStack::new(i).map_err(|_| NO_MEMORY)?;
            let image = self
                .load_image(bytes, personality, args, environment)
                .map_err(|e| match e {
                    loader::LoadError::Memory(_) => NO_MEMORY,
                    _ => INVALID_ARGUMENT,
                })?;
            let context = user::Context::new(image.entry, image.stack);
            let (files, cwd) = if parent == 0 {
                (self.fs.standard()?, ROOT)
            } else {
                let parent = self
                    .processes
                    .iter()
                    .enumerate()
                    .find(|(slot, p)| p.is_some() && self.policy.entry(*slot).pid == parent)
                    .map(|(_, p)| p.as_ref().unwrap())
                    .ok_or(NO_CHILD)?;
                (self.fs.inherit(&parent.files)?, parent.cwd)
            };
            Ok(Process {
                image: Some(image),
                stack: Some(stack),
                context,
                files,
                wait: None,
                cwd,
                io: None,
                network_deadline: None,
                thread: linux::threads::Thread::new(self.policy.entry(i).pid),
                pty_member: None,
            })
        })();
        match result {
            Ok(process) => {
                self.processes[i] = Some(process);
                #[cfg(feature = "gui")]
                self.gui
                    .set_image(self.policy.entry(i).pid, bytes, personality == PERSONALITY);
                self.metrics.spawns += 1;
                Ok(i)
            }
            Err(error) => {
                self.policy.remove(i);
                Err(error)
            }
        }
    }
    fn executable(&mut self, i: usize, name: &str) -> Result<Cow<'static, [u8]>, i64> {
        if name.contains('/') {
            return self.fs.load_executable(self.process(i).cwd, name);
        }
        let mut path = [0; 128];
        if name.len() + 5 > path.len() {
            return Err(TOO_LONG);
        }
        path[..5].copy_from_slice(b"/bin/");
        path[5..5 + name.len()].copy_from_slice(name.as_bytes());
        self.fs
            .load_executable(ROOT, core::str::from_utf8(&path[..5 + name.len()]).unwrap())
    }
    fn fork(&mut self, parent: usize) -> Result<u64, i64> {
        self.fork_space(parent, false)
    }
    fn fork_space(&mut self, parent: usize, share: bool) -> Result<u64, i64> {
        let i = self.policy.insert(self.process(parent).thread.tgid)?;
        let result = (|| {
            let stack = mm::user::KernelStack::new(i).map_err(|_| NO_MEMORY)?;
            let old = self.process(parent);
            let image = old.image.as_ref().expect("Missing parent image");
            let image = loader::Loaded {
                linux: image.linux,
                brk: image.brk,
                clear_tid: 0,
                sigmask: image.sigmask,
                ignored: image.ignored,
                pending: 0,
                actions: anova_memory::shared::Shared::try_new(core::cell::RefCell::new(
                    *image.actions.borrow(),
                ))
                .map_err(|_| NO_MEMORY)?,
                space: if share {
                    image.space.share()
                } else {
                    image.space.clone_eager().map_err(|_| NO_MEMORY)?
                },
                entry: image.entry,
                stack: image.stack,
            };
            let mut context = old.context;
            let cwd = old.cwd;
            context.frame.rax = 0;
            Ok(Process {
                image: Some(image),
                stack: Some(stack),
                context,
                files: self
                    .fs
                    .inherit(&self.processes[parent].as_ref().unwrap().files)?,
                wait: None,
                cwd,
                io: None,
                network_deadline: None,
                thread: linux::threads::Thread::new(self.policy.entry(i).pid),
                pty_member: self.process(parent).pty_member,
            })
        })();
        match result {
            Ok(mut process) => {
                process.thread.pgid = self.process(parent).thread.pgid;
                process.thread.sid = self.process(parent).thread.sid;
                process.thread.umask = self.process(parent).thread.umask;
                process.thread.altstack = self.process(parent).thread.altstack;
                self.processes[i] = Some(process);
                self.metrics.forks += 1;
                Ok(self.policy.entry(i).pid)
            }
            Err(error) => {
                self.policy.remove(i);
                Err(error)
            }
        }
    }
    #[cfg(feature = "qemu-test")]
    fn exec(&mut self, i: usize, bytes: &[u8], args: &[&str]) -> Result<(), i64> {
        self.exec_personality(i, bytes, args, PERSONALITY)
    }
    fn exec_personality(
        &mut self,
        i: usize,
        bytes: &[u8],
        args: &[&str],
        personality: &str,
    ) -> Result<(), i64> {
        let image = self
            .load_image(bytes, personality, args, &[])
            .map_err(|e| match e {
                loader::LoadError::Memory(_) => NO_MEMORY,
                _ => INVALID_ARGUMENT,
            })?;
        let p = self.process_mut(i);
        p.context = user::Context::new(image.entry, image.stack);
        let linux = image.linux;
        p.image = Some(image);
        #[cfg(feature = "gui")]
        self.gui
            .set_image(self.policy.entry(i).pid, bytes, personality == PERSONALITY);
        if linux {
            serial::write(format_args!(
                "[LINUX:EXEC] image={} dynamic={}\n",
                args.first().copied().unwrap_or(""),
                anova_elf::interpreter(bytes).ok().flatten().is_some()
            ));
            let group = self.process(i).thread.pgid;
            self.terminal.set_foreground(group);
        }
        self.metrics.execs += 1;
        Ok(())
    }
    fn finish(&mut self, i: usize, outcome: Outcome) {
        let owner = self.policy.entry(i).pid;
        self.pty_close_owner(owner);
        self.linux_clear_tid(i);
        let pid = self.policy.entry(i).pid;
        #[cfg(feature = "gui")]
        self.gui.close_process(pid);
        match outcome {
            Outcome::Signal(signal) => {
                serial::write(format_args!("[USER:SIGNAL] pid={pid} signal={signal}\n"))
            }
            Outcome::Exit(status) => {
                serial::write(format_args!("[USER:EXIT] pid={pid} status={status}\n"))
            }
            Outcome::Fault {
                vector,
                error,
                address,
            } => serial::emergency(format_args!(
                "[USER:FAULT] pid={pid} vector={vector} error={error:#x} address={address:#x} rip={:#x}\n",
                self.process(i).context.frame.rip
            )),
            Outcome::BadReturn => {
                serial::emergency(format_args!("[USER:RETURNREJECT] pid={pid}\n"))
            }
        }
        let p = self.processes[i].as_mut().unwrap();
        drop(p.image.take());
        drop(p.stack.take());
        self.fs.clear(&mut p.files);
        p.io = None;
        p.wait = None;
        p.thread.blocked = None;
        p.thread.saved_mask = None;
        self.policy.exit(i, outcome.status());
        self.linux_notify_exit(i, outcome);
    }
    fn reap(&mut self, i: usize) {
        self.processes[i] = None;
        self.policy.remove(i);
        self.metrics.reaps += 1;
    }
    fn wait(&mut self, i: usize, pid: u64, output: UserAddr) -> Option<i64> {
        if self.process(i).image.as_ref().unwrap().linux
            && self.process(i).context.frame.rdx & 2 != 0
            && let Some(result) = self.linux_wait_stopped(i, pid, output)
        {
            return Some(result);
        }
        let process = self.process(i);
        let result = if process.image.as_ref().unwrap().linux
            && process.thread.tgid != self.policy.entry(i).pid
        {
            let group = process.thread.tgid;
            let mut found = false;
            let child = (0..CAPACITY).find(|&k| {
                let entry = self.policy.entry(k);
                let eligible = entry.parent == group
                    && entry.state != State::Empty
                    && (pid == 0 || entry.pid == pid);
                found |= eligible;
                eligible && matches!(entry.state, State::Zombie(_))
            });
            if child.is_some() {
                Ok(child)
            } else if found {
                self.policy.block(i);
                Ok(None)
            } else {
                Err(NO_CHILD)
            }
        } else {
            self.policy.wait(i, pid)
        };
        let child = match result {
            Ok(Some(child)) => child,
            Ok(None) => {
                self.process_mut(i).wait = Some((pid, output));
                self.metrics.waits += 1;
                return None;
            }
            Err(error) => return Some(error),
        };
        let entry = self.policy.entry(child);
        let State::Zombie(status) = entry.state else {
            unreachable!()
        };
        let bytes = WaitStatus {
            pid: entry.pid,
            status,
        }
        .bytes();
        let space = &self.process(i).image.as_ref().expect("Missing image").space;
        let linux = self.process(i).image.as_ref().unwrap().linux;
        let linux_bytes = linux::wait_bytes(status);
        let bytes = if linux { &linux_bytes[..] } else { &bytes[..] };
        if !(linux && output.0 == 0)
            && let Err(error) = space.with_active(|| mm::uaccess::to_user(space, output, bytes))
        {
            return Some(error);
        }
        self.reap(child);
        Some(entry.pid as i64)
    }
    fn valid_return(&mut self, i: usize) -> bool {
        let p = self.process_mut(i);
        let frame = &mut p.context.frame;
        let space = &p.image.as_ref().expect("Missing image").space;
        let valid = frame.cs == 0x33
            && frame.ss == 0x2b
            && space.accessible(UserAddr(frame.rip), 1, false, true)
            && if p.image.as_ref().unwrap().linux {
                // musl detached threads munmap their own stack immediately before
                // SYS_exit. IRETQ and intervening IRQs use the trusted kernel stack;
                // CPL3 stack access is faulted when actually attempted, not predicted.
                UserAddr(frame.rsp).checked_end(0).is_some()
            } else {
                frame
                    .rsp
                    .checked_sub(8)
                    .is_some_and(|a| space.accessible(UserAddr(a), 8, true, false))
            };
        frame.rflags = (frame.rflags & 0xcd5) | 0x202;
        valid
    }
    fn syscall(&mut self, i: usize) {
        if let Some((seconds, nanos)) = crate::arch::x86_64::clock::realtime() {
            self.fs.set_clock(seconds * 1_000_000_000 + nanos);
        }
        if self.process(i).image.as_ref().unwrap().linux {
            self.linux_syscall(i);
            return;
        }
        let f = self.process(i).context.frame;
        let result = match f.rax {
            anova_abi::process::SYSCALL => self.process_control(i, &f),
            anova_abi::pty::SYSCALL => self.pty_control(i, &f).unwrap_or_else(|e| e),
            #[cfg(feature = "gui")]
            anova_gui::transport::SYS_GUI => {
                let pid = self.policy.entry(i).pid;
                let space = &self.processes[i]
                    .as_ref()
                    .unwrap()
                    .image
                    .as_ref()
                    .unwrap()
                    .space;
                self.gui.syscall(pid, space, f.rdi, f.rsi, f.rdx)
            }
            SYS_EXIT if f.rdi <= u32::MAX as u64 => {
                self.finish(i, Outcome::Exit(f.rdi as u32));
                return;
            }
            SYS_YIELD => {
                self.policy.ready(i);
                0
            }
            SYS_SLEEP => match apic::ticks().checked_add(f.rdi) {
                Some(deadline) => {
                    self.metrics.sleeps += 1;
                    self.policy.sleep(i, deadline);
                    0
                }
                None => INVALID_ARGUMENT,
            },
            SYS_FORK => self.fork(i).map_or_else(|e| e, |pid| pid as i64),
            SYS_SHUTDOWN if f.rdi == 0 && f.rsi == 0 && f.rdx == 0 => {
                if !crate::arch::x86_64::acpi::can_poweroff() {
                    self.process_mut(i).context.frame.rax = UNSUPPORTED as u64;
                    return;
                }
                // Reject an already failed disk while console and caller descriptors still exist.
                if let Err(error) = self.fs.sync() {
                    self.process_mut(i).context.frame.rax = error as u64;
                    return;
                }
                #[cfg(feature = "bootstrap-metrics")]
                self.sample_resources();
                // Stop all writers and close orphaned files before recording a clean unmount.
                for process in self.processes.iter_mut().flatten() {
                    self.fs.clear(&mut process.files);
                }
                match self.fs.shutdown() {
                    Ok(()) => {
                        // Final counters include orphan releases performed while stopping writers.
                        #[cfg(feature = "bootstrap-metrics")]
                        self.sample_resources();
                        // Drop confirms virtio reset before returning its DMA frame to the allocator.
                        if let Err(error) = self.fs.detach_disk() {
                            serial::emergency(format_args!(
                                "[POWER:FAIL] DMA reset error={error}\n"
                            ));
                            crate::halt();
                        }
                        if let Some(net) = self.network.take()
                            && let Err(error) = net.stop()
                        {
                            serial::emergency(format_args!(
                                "[POWER:FAIL] network reset error={error}\n"
                            ));
                            crate::halt();
                        }
                        #[cfg(feature = "bootstrap-metrics")]
                        self.resources.report();
                        crate::arch::x86_64::acpi::poweroff();
                    }
                    Err(error) => {
                        // Writers have been stopped and their descriptors closed; do not resume
                        // a session that can no longer report or safely retry the final unmount.
                        serial::emergency(format_args!(
                            "[POWER:FAIL] final unmount error={error}; halted dirty\n"
                        ));
                        crate::halt();
                    }
                }
            }
            SYS_WAIT => {
                if f.rdx != size_of::<WaitStatus>() as u64 {
                    INVALID_ARGUMENT
                } else {
                    let space = &self.process(i).image.as_ref().expect("Missing image").space;
                    if !space.accessible(UserAddr(f.rsi), size_of::<WaitStatus>(), true, false) {
                        BAD_ADDRESS
                    } else {
                        match self.wait(i, f.rdi, UserAddr(f.rsi)) {
                            Some(result) => result,
                            None => return,
                        }
                    }
                }
            }
            SYS_SPAWN | SYS_EXEC => {
                let space = &self.process(i).image.as_ref().expect("Missing image").space;
                let request = space.with_active(|| Request::read(space, &f));
                match request {
                    Err(error) => error,
                    Ok(request) => match self.executable(i, request.name()) {
                        Err(error) => error,
                        Ok(bytes) => {
                            let linux = self.executable_linux(i, request.name()).unwrap_or(false);
                            let personality = if linux {
                                anova_abi::linux::PERSONALITY
                            } else {
                                PERSONALITY
                            };
                            // Native v1 carries one opaque argument; Linux argv needs separate words.
                            // Rich quoting remains the Linux shell's responsibility.
                            let native_args = [request.name(), request.argument()];
                            let mut linux_args = [""; 16];
                            linux_args[0] = request.name();
                            let words = request.argument().split_ascii_whitespace();
                            let count = words.clone().count();
                            if linux && count > 15 {
                                self.process_mut(i).context.frame.rax = INVALID_ARGUMENT as u64;
                                return;
                            }
                            for (slot, word) in linux_args[1..].iter_mut().zip(words) {
                                *slot = word;
                            }
                            let args = if linux {
                                &linux_args[..count + 1]
                            } else {
                                &native_args[..]
                            };
                            if f.rax == SYS_SPAWN {
                                self.add_personality(
                                    &bytes,
                                    args,
                                    self.policy.entry(i).pid,
                                    personality,
                                )
                                .map_or_else(|e| e, |child| self.policy.entry(child).pid as i64)
                            } else {
                                match self.exec_personality(i, &bytes, args, personality) {
                                    Ok(()) => return,
                                    Err(error) => error,
                                }
                            }
                        }
                    },
                }
            }
            _ => {
                let pid = self.policy.entry(i).pid;
                let p = self.processes[i].as_mut().unwrap();
                let space = &p.image.as_ref().expect("Missing image").space;
                space.with_active(|| {
                    fs::syscall(
                        &f,
                        space,
                        &mut p.files,
                        &mut p.cwd,
                        &mut self.fs,
                        &mut self.terminal,
                    )
                    .unwrap_or_else(|| abi::syscall(&f, space, pid))
                })
            }
        };
        if result == WOULD_BLOCK && matches!(f.rax, SYS_READ | SYS_WRITE) {
            self.process_mut(i).io = Some(f);
            self.policy.block(i);
            self.metrics.io_blocks += 1;
        } else {
            self.process_mut(i).context.frame.rax = result as u64;
        }
    }
    fn drive(&mut self, root_pid: u64) -> Outcome {
        let root: u64;
        // SAFETY: CPL0 on the scheduler stack, checking the reclamation root invariant.
        unsafe {
            asm!("mov {},cr3",out(reg) root,options(nomem,nostack,preserves_flags));
        }
        assert_eq!(
            root,
            mm::user::kernel_root(),
            "Scheduler requires permanent kernel root"
        );
        let mut root_outcome = None;
        let mut active = None;
        let mut previous = None;
        let mut preempted = false;
        loop {
            #[cfg(feature = "bootstrap-metrics")]
            self.sample_resources();
            if let Err(error) = self.fs.collect_mappings() {
                serial::write(format_args!(
                    "[FS:FAIL] mapped inode cleanup error={error}\n"
                ));
            }
            if let Some(net) = self.network.as_mut() {
                net.collect(|id| self.fs.socket_live(id));
                net.poll();
            }
            #[cfg(feature = "graphics")]
            crate::drivers::input::poll();
            self.terminal.poll();
            self.linux_wake();
            for i in 0..CAPACITY {
                if self.policy.entry(i).state == State::Blocked {
                    let p = self.process(i);
                    if p.io.as_ref().is_some_and(|f| {
                        if self.fs.socket_id(&p.files, f.rdi).ok().flatten().is_some()
                            || self.fs.is_packet(&p.files, f.rdi)
                        {
                            return self.socket_ready(i, f);
                        }
                        fs::ready(
                            &linux::io_frame(*f, p.image.as_ref().unwrap().linux),
                            &p.files,
                            &self.fs,
                            &self.terminal,
                        )
                    }) {
                        self.policy.ready(i);
                        self.metrics.io_wakes += 1;
                    }
                }
            }
            for i in 0..CAPACITY {
                let e = self.policy.entry(i);
                if (e.parent == 0
                    || self.processes[i]
                        .as_ref()
                        .is_some_and(|p| p.thread.tgid != e.pid))
                    && matches!(e.state, State::Zombie(_))
                {
                    if e.pid == root_pid
                        && root_outcome.is_none()
                        && let State::Zombie(status) = e.state
                    {
                        root_outcome = Some(if status.reason == 3 {
                            Outcome::Signal(status.code)
                        } else {
                            Outcome::Exit(status.code)
                        });
                    }
                    self.reap(i);
                }
            }
            if !self.policy.live() {
                break;
            }
            let index = active.take().or_else(|| self.policy.select(apic::ticks()));
            let Some(i) = index else {
                self.metrics.idle += 1;
                cpu::wait_for_interrupt();
                cpu::disable_interrupts();
                continue;
            };
            if previous.is_some_and(|old| old != i) {
                self.metrics.switches += 1;
                if preempted {
                    self.metrics.preempt_switches += 1;
                }
            }
            previous = Some(i);
            preempted = false;
            if self.process(i).thread.vfork_child.is_some() {
                self.policy.block(i);
                continue;
            }
            if let Some(signal) = self.linux_pending(i) {
                match self.deliver_signal(i, signal) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(signal) => {
                        self.linux_exit_group(i, Outcome::Signal(signal));
                        if self.policy.entry(i).pid == root_pid {
                            root_outcome = Some(Outcome::Signal(signal));
                        }
                        continue;
                    }
                }
            }
            if let Some((pid, output)) = self.process_mut(i).wait.take() {
                match self.wait(i, pid, output) {
                    Some(result) => {
                        let result = if self.process(i).image.as_ref().unwrap().linux {
                            anova_abi::linux::errno(result)
                        } else {
                            result
                        };
                        self.process_mut(i).context.frame.rax = result as u64;
                    }
                    None => continue,
                }
            }
            if let Some(frame) = self.process_mut(i).io.take() {
                self.process_mut(i).context.frame = frame;
                self.syscall(i);
                if self.policy.entry(i).state == State::Blocked {
                    continue;
                }
            }
            if self.process(i).thread.blocked.is_some()
                || self.process(i).thread.stopped
                || self.process(i).thread.leader_exit.is_some()
            {
                self.policy.block(i);
                continue;
            }
            if !self.valid_return(i) {
                if self.process(i).image.as_ref().unwrap().linux {
                    self.linux_bad_return(i);
                    continue;
                }
                self.finish(i, Outcome::BadReturn);
                if self.policy.entry(i).pid == root_pid {
                    root_outcome = Some(Outcome::BadReturn);
                }
                continue;
            }
            #[cfg(feature = "bootstrap-metrics")]
            self.sample_resources();
            let p = self.process(i);
            let mut capture = user::Context::new(0, 0);
            CAPTURE.store(&mut capture, Ordering::Release);
            let top = p.stack.as_ref().expect("Missing kernel stack").top();
            traps::set_kernel_stack(top);
            p.image
                .as_ref()
                .expect("Missing image")
                .space
                .refresh_file_pages();
            p.context
                .run(p.image.as_ref().expect("Missing image").space.root().0, top);
            CAPTURE.store(ptr::null_mut(), Ordering::Release);
            mm::user::restore_kernel_stack();
            self.process_mut(i).context = capture;
            let f = capture.frame;
            if f.vector == 0x40 {
                self.metrics.preemptions += 1;
                preempted = true;
                self.policy.ready(i);
            } else if f.vector == 0x80 {
                let linux = self.process(i).image.as_ref().unwrap().linux;
                let exiting = if linux {
                    matches!(f.rax, 60 | 231)
                } else {
                    f.rax == SYS_EXIT && f.rdi <= u32::MAX as u64
                };
                self.syscall(i);
                if let State::Zombie(status) = self.policy.entry(i).state
                    && status.reason == 3
                    && self.policy.entry(i).pid == root_pid
                {
                    root_outcome = Some(Outcome::Signal(status.code));
                }
                if exiting && self.policy.entry(i).pid == root_pid {
                    root_outcome = Some(Outcome::Exit(if linux {
                        (f.rdi & 255) as u32
                    } else {
                        f.rdi as u32
                    }));
                }
                if self.policy.entry(i).state == State::Running {
                    active = Some(i);
                }
            } else {
                diagnostics::fault(
                    self.policy.entry(i).pid,
                    &self.process(i).image.as_ref().unwrap().space,
                    &f,
                );
                let outcome = Outcome::Fault {
                    vector: f.vector,
                    error: f.error,
                    address: FAULT_ADDRESS.load(Ordering::Relaxed),
                };
                if self.process(i).image.as_ref().unwrap().linux {
                    self.linux_exception(i, &f, FAULT_ADDRESS.load(Ordering::Relaxed));
                    continue;
                }
                self.finish(i, outcome);
                if self.policy.entry(i).pid == root_pid {
                    root_outcome = Some(outcome);
                }
            }
        }
        assert!(
            self.processes.iter().all(Option::is_none),
            "Unreaped process resources"
        );
        assert_eq!(
            self.fs.live_objects(),
            (0, 0),
            "Open descriptions or pipes leaked"
        );
        root_outcome.expect("Root process did not terminate")
    }
}

struct Request {
    name: [u8; MAX_NAME],
    name_len: usize,
    argument: [u8; MAX_NAME],
    argument_len: usize,
}
impl Request {
    fn read(space: &mm::user::UserSpace, f: &TrapFrame) -> Result<Self, i64> {
        if f.rsi != size_of::<ExecRequest>() as u64 {
            return Err(INVALID_ARGUMENT);
        }
        let mut wire = [0; 32];
        mm::uaccess::from_user(space, UserAddr(f.rdi), &mut wire)?;
        let word =
            |offset| u64::from_le_bytes(wire[offset..offset + 8].try_into().expect("Wire field"));
        let name_len = usize::try_from(word(8)).map_err(|_| TOO_LONG)?;
        let argument_len = usize::try_from(word(24)).map_err(|_| TOO_LONG)?;
        if name_len == 0 || name_len > MAX_NAME || argument_len > MAX_NAME {
            return Err(INVALID_ARGUMENT);
        }
        let mut request = Self {
            name: [0; MAX_NAME],
            name_len,
            argument: [0; MAX_NAME],
            argument_len,
        };
        mm::uaccess::from_user(space, UserAddr(word(0)), &mut request.name[..name_len])?;
        mm::uaccess::from_user(
            space,
            UserAddr(word(16)),
            &mut request.argument[..argument_len],
        )?;
        for bytes in [&request.name[..name_len], &request.argument[..argument_len]] {
            if bytes.contains(&0) || core::str::from_utf8(bytes).is_err() {
                return Err(INVALID_ARGUMENT);
            }
        }
        Ok(request)
    }
    fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).expect("Validated UTF-8")
    }
    fn argument(&self) -> &str {
        core::str::from_utf8(&self.argument[..self.argument_len]).expect("Validated UTF-8")
    }
}

fn run(bytes: &[u8], args: &[&str], pid: u64) -> Result<Outcome, loader::LoadError> {
    cpu::without_interrupts(|| {
        let mut kernel = Kernel::new(pid)?;
        if args.first() == Some(&"/bin/init")
            && let Err(error) = crate::drivers::attach(&mut kernel.fs)
        {
            serial::write(format_args!(
                "[DISK:REJECT] mount error={error}; /persist unavailable\n"
            ));
        }
        if args.first() == Some(&"/bin/init") {
            kernel.network =
                crate::net::Stack::attach().map_err(|_| anova_memory::Error::OutOfMemory)?;
        }
        let linux = args
            .first()
            .is_some_and(|name| kernel.fs.linux_executable(ROOT, name).unwrap_or(false));
        let personality = if linux {
            anova_abi::linux::PERSONALITY
        } else {
            PERSONALITY
        };
        let index = kernel
            .add_personality(bytes, args, 0, personality)
            .map_err(|_| loader::LoadError::Memory(anova_memory::Error::OutOfMemory))?;
        let pid = kernel.policy.entry(index).pid;
        let outcome = kernel.drive(pid);
        #[cfg(feature = "qemu-test")]
        if args.first() == Some(&"/linux/busybox") && args.get(1) == Some(&"sh") {
            let script = args.get(3).copied().unwrap_or("");
            let (path, length, byte) = if script.contains("/tmp/large") {
                ("/tmp/large", 5000, b'Q')
            } else if script.contains("/tmp/bb") {
                ("/tmp/bb", 12, 0)
            } else {
                ("", 0, 0)
            };
            if length != 0 {
                let mut files = Descriptors::default();
                let fd = kernel.fs.open(&mut files, ROOT, path, OPEN_READ).unwrap();
                let mut bytes = [0; 4096];
                let mut total = 0;
                loop {
                    let n = kernel.fs.read(&files, fd, &mut bytes).unwrap();
                    if n == 0 {
                        break;
                    }
                    if byte != 0 {
                        assert!(bytes[..n].iter().all(|b| *b == byte));
                    } else {
                        assert_eq!(&bytes[..n], b"M8-pipeline\n");
                    }
                    total += n;
                }
                kernel.fs.clear(&mut files);
                assert_eq!(total, length);
                serial::write(format_args!(
                    "[LINUX:SHELL] bytes={length} redirection=true verified=true\n"
                ));
            }
        }
        if args.first() == Some(&"process-test") {
            let m = &kernel.metrics;
            #[cfg(feature = "qemu-test")]
            {
                assert!(
                    m.preempt_switches >= 2 && m.idle > 0 && m.waits > 0 && m.sleeps > 0,
                    "Scheduling paths not exercised"
                );
                assert!(
                    m.forks >= 2 && m.execs >= 1 && m.spawns >= 16,
                    "Lifecycle paths not exercised"
                );
            }
            serial::write(format_args!(
                "[SCHED:TEST] preemptions={} preempt_switches={} switches={} sleeps={} waits={} idle_halts={} forks={} execs={} spawns={} reaps={}\n",
                m.preemptions,
                m.preempt_switches,
                m.switches,
                m.sleeps,
                m.waits,
                m.idle,
                m.forks,
                m.execs,
                m.spawns,
                m.reaps
            ));
        }
        if args.first() == Some(&"/bin/init") && cfg!(feature = "qemu-test") {
            assert!(
                kernel.metrics.io_blocks > 0 && kernel.metrics.io_wakes > 0,
                "I/O blocking was not exercised"
            );
            serial::write(format_args!(
                "[VFS:IO] blocks={} wakes={} open_descriptions=0 pipes=0\n",
                kernel.metrics.io_blocks, kernel.metrics.io_wakes
            ));
            kernel
                .fs
                .shutdown()
                .map_err(|_| loader::LoadError::Memory(anova_memory::Error::InvalidRange))?;
        }
        Ok(outcome)
    })
}
fn checked_run(bytes: &[u8], args: &[&str], pid: u64, expected: Outcome) {
    let before = mm::user::free_frames();
    let heap = mm::heap_used();
    assert_eq!(
        run(bytes, args, pid).unwrap_or_else(|e| panic!("App load failed: {e}")),
        expected,
        "Unexpected app result"
    );
    assert_eq!(
        mm::user::free_frames(),
        before,
        "Process leaked physical pages"
    );
    assert_eq!(mm::heap_used(), heap, "Process leaked heap storage");
    serial::write(format_args!("[USER:REAP] pid={pid} frames_restored=true\n"));
    if let Some(name) = args.first().filter(|name| {
        matches!(
            **name,
            "/bin/linux-threads" | "/bin/linux-dynamic" | "/bin/linux-pie"
        )
    }) {
        serial::write(format_args!(
            "[M9:RECLAIM] image={name} frames_restored=true heap_restored=true tasks=0 descriptors=0\n"
        ));
    }
}
pub fn start() {
    cpu::without_interrupts(user::init);
    let rust = boot::native_module("hello-rust");
    checked_run(
        rust,
        &["hello-rust", "AnovaOS", "ring3"],
        1,
        Outcome::Exit(0),
    );
    checked_run(
        boot::native_module("hello-c"),
        &["hello-c", "native-v1"],
        2,
        Outcome::Exit(7),
    );
    #[cfg(feature = "qemu-test")]
    cpu::without_interrupts(|| validation::run(rust));
    checked_run(
        boot::native_module("process-test"),
        &[
            "process-test",
            if cfg!(feature = "qemu-test") {
                "suite"
            } else {
                "demo"
            },
        ],
        22,
        Outcome::Exit(0),
    );
    serial::write(format_args!(
        "[SCHED:READY] processes_reclaimed=true descriptors=0 frames_restored=true heap_restored=true\n"
    ));
    serial::write(format_args!(
        "[USER:READY] Rust/C apps and concurrent processes exited; kernel resumed\n"
    ));
    serial::write(format_args!(
        "[VFS:MOUNTS] root=initramfs:ro /tmp=tmpfs:rw /dev=devfs stdin=0 stdout=1 stderr=2\n"
    ));
    #[cfg(feature = "qemu-test")]
    for (name, args) in [
        ("/bin/linux-probe", &["/bin/linux-probe"][..]),
        ("/bin/linux-probe", &["/bin/linux-probe", "exit-high"][..]),
        ("/bin/linux-musl", &["/bin/linux-musl", "test"][..]),
        ("/bin/linux-threads", &["/bin/linux-threads"][..]),
        ("/bin/linux-dynamic", &["/bin/linux-dynamic"][..]),
        ("/bin/linux-pie", &["/bin/linux-pie"][..]),
    ] {
        let bytes = Vfs::new(boot::initramfs())
            .unwrap()
            .executable(ROOT, name)
            .unwrap();
        checked_run(bytes, args, 100, Outcome::Exit(0));
    }
    #[cfg(feature = "qemu-test")]
    for args in [
        &["/linux/busybox", "echo", "M8-busybox-echo"][..],
        &["/linux/busybox", "ls", "/etc"][..],
        &["/linux/busybox", "cat", "/etc/motd"][..],
        &[
            "/linux/busybox",
            "sh",
            "-c",
            "/linux/echo M8-pipeline | /linux/cat > /tmp/bb; /linux/cat /tmp/bb",
        ][..],
        &[
            "/linux/busybox",
            "sh",
            "-c",
            "/linux/cat /etc/large | /linux/cat > /tmp/large; /linux/cat /tmp/large > /dev/null",
        ][..],
        &[
            "/linux/busybox",
            "sh",
            "-c",
            "/linux/false; /linux/echo M8-status-$?",
        ][..],
    ] {
        let bytes = Vfs::new(boot::initramfs())
            .unwrap()
            .executable(ROOT, "/linux/busybox")
            .unwrap();
        checked_run(bytes, args, 100, Outcome::Exit(0));
        serial::write(format_args!(
            "[LINUX:BUSYBOX] applet={} passed=true\n",
            args[1]
        ));
    }
    let init = {
        let mut root = Vfs::new(boot::initramfs()).expect("Invalid initramfs");
        root.executable(ROOT, "/bin/init").expect("Missing init")
    };
    checked_run(
        init,
        &[
            "/bin/init",
            if cfg!(feature = "qemu-test") {
                "test"
            } else if cfg!(feature = "desktop-test") {
                "desktop-test"
            } else if cfg!(feature = "desktop") {
                "desktop"
            } else {
                ""
            },
        ],
        1,
        Outcome::Exit(0),
    );
    serial::write(format_args!(
        "[VFS:READY] frames_restored=true heap_restored=true tmpfs=volatile\n"
    ));
}
