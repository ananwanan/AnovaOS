// SPDX-License-Identifier: Apache-2.0
//! Bridge exact native creators to private PTY masters and admitted Linux shells/compiler jobs.
//! Transfers never block. Session tags survive Linux fork/exec/setsid; creator death closes
//! the master and kills/reaps tagged descendants without granting authority over peer tasks.

use super::*;
use anova_abi::pty::{self, Request};
use anova_vfs::Device;

pub(super) struct Session {
    owner: u64,
    id: u64,
    build: bool,
    files: Descriptors,
    device: Device,
    pending: [u8; pty::MAX_TRANSFER],
    length: usize,
    #[cfg(feature = "bootstrap-metrics")]
    baseline: (usize, usize, (usize, usize)),
}

impl Kernel {
    pub(super) fn pty_control(&mut self, caller: usize, frame: &TrapFrame) -> Result<i64, i64> {
        let operation = pty::decode(frame.rdi).ok_or(INVALID_ARGUMENT)?;
        let space = self.process(caller).image.as_ref().unwrap().space.share();
        let mut bytes = [0; 32];
        space.with_active(|| mm::uaccess::from_user(&space, UserAddr(frame.rdx), &mut bytes))?;
        let request = Request::from_bytes(bytes);
        if !request.valid(operation) {
            return Err(INVALID_ARGUMENT);
        }
        self.pty_request(caller, frame, operation, request, false)
    }

    pub(super) fn build_control(&mut self, caller: usize, frame: &TrapFrame) -> Result<i64, i64> {
        use anova_abi::build as b;
        let operation = b::decode(frame.rdi).ok_or(INVALID_ARGUMENT)?;
        let space = self.process(caller).image.as_ref().unwrap().space.share();
        let mut bytes = [0; 32];
        space.with_active(|| mm::uaccess::from_user(&space, UserAddr(frame.rdx), &mut bytes))?;
        let request = b::Request::from_bytes(bytes);
        if !request.valid(operation) {
            return Err(INVALID_ARGUMENT);
        }
        let op = match operation {
            b::START => pty::START,
            b::READ => pty::READ,
            b::STATUS => pty::STATUS,
            b::CLOSE => pty::CLOSE,
            _ => return Err(INVALID_ARGUMENT),
        };
        self.pty_request(
            caller,
            frame,
            op,
            Request {
                buffer: request.buffer,
                length: request.length,
                rows: if op == pty::START { 26 } else { 0 },
                columns: if op == pty::START { 50 } else { 0 },
                reserved: 0,
            },
            true,
        )
    }

    fn pty_request(
        &mut self,
        caller: usize,
        frame: &TrapFrame,
        operation: u32,
        request: Request,
        build: bool,
    ) -> Result<i64, i64> {
        let space = self.process(caller).image.as_ref().unwrap().space.share();
        let owner = self.policy.entry(caller).pid;
        if operation == pty::START {
            if frame.rsi != 0 {
                return Err(INVALID_ARGUMENT);
            }
            return self.pty_start(caller, request, build).map(|id| id as i64);
        }
        let slot = self
            .ptys
            .iter()
            .position(|s| {
                s.as_ref()
                    .is_some_and(|s| s.owner == owner && s.id == frame.rsi && s.build == build)
            })
            .ok_or(NO_CHILD)?;
        match operation {
            pty::CLOSE => {
                self.pty_close(slot)?;
                Ok(0)
            }
            pty::RESIZE => {
                self.terminal
                    .set_window(self.ptys[slot].as_ref().unwrap().device, window(request));
                Ok(0)
            }
            pty::STATUS => {
                let child = self.policy.child(caller, frame.rsi)?;
                let State::Zombie(status) = self.policy.entry(child).state else {
                    return Err(WOULD_BLOCK);
                };
                let bytes = WaitStatus {
                    pid: frame.rsi,
                    status,
                }
                .bytes();
                space.with_active(|| {
                    mm::uaccess::to_user(&space, UserAddr(request.buffer), &bytes)
                })?;
                Ok(0)
            }
            pty::READ | pty::WRITE => {
                let count = request.length as usize;
                if count == 0 {
                    return Ok(0);
                }
                let session = self.ptys[slot].as_mut().unwrap();
                if operation == pty::WRITE {
                    let mut bytes = [0; pty::MAX_TRANSFER];
                    space.with_active(|| {
                        mm::uaccess::from_user(
                            &space,
                            UserAddr(request.buffer),
                            &mut bytes[..count],
                        )
                    })?;
                    return self
                        .terminal
                        .write(session.device, &bytes[..count], &self.fs)
                        .map(|n| n as i64);
                }
                // Retain bytes in kernel-owned storage if copying fails; a retry may request
                // fewer bytes. Never consume an inaccessible caller's output irretrievably.
                if session.length == 0 {
                    session.length =
                        match self
                            .terminal
                            .read(session.device, &mut session.pending, &self.fs)
                        {
                            Ok(n) => n,
                            Err(IO_ERROR) => return Ok(0),
                            Err(e) => return Err(e),
                        };
                }
                let n = count.min(session.length);
                space.with_active(|| {
                    mm::uaccess::to_user(&space, UserAddr(request.buffer), &session.pending[..n])
                })?;
                session.pending.copy_within(n..session.length, 0);
                session.length -= n;
                Ok(n as i64)
            }
            _ => Err(INVALID_ARGUMENT),
        }
    }

    fn pty_start(&mut self, caller: usize, request: Request, build: bool) -> Result<u64, i64> {
        if build && self.ptys.iter().flatten().any(|s| s.build) {
            return Err(BUSY);
        }
        #[cfg(feature = "bootstrap-metrics")]
        let baseline = (
            mm::heap_used(),
            mm::user::free_frames(),
            self.fs.live_objects(),
        );
        let slot = self
            .ptys
            .iter()
            .position(Option::is_none)
            .ok_or(FILE_LIMIT)?;
        // This is executable admission, not a privilege inferred from an ELF or pathname.
        let executable = if build { "/bin/gcc" } else { "/linux/sh" };
        if !self.fs.linux_executable(ROOT, executable)? {
            return Err(PERMISSION_DENIED);
        }
        let bytes = self.fs.load_executable(ROOT, executable)?;
        let owner = self.policy.entry(caller).pid;
        let mut files = Descriptors::default();
        let mut child = None;
        let mut staging = false;
        let result = (|| -> Result<(u64, Device), i64> {
            if build {
                // Reserve only a new regular staging file. Never truncate a prior output
                // or silently adopt a recovery file from an earlier creator/system boot.
                self.fs.open(
                    &mut files,
                    ROOT,
                    anova_abi::build::STAGING,
                    OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE,
                )?;
                staging = true;
                self.fs.close(&mut files, 0)?;
            }
            self.fs
                .open(&mut files, ROOT, "/dev/ptmx", OPEN_READ | OPEN_WRITE)?;
            let Some(device @ Device::PtyMaster(n)) = self.fs.device(&files, 0, false)? else {
                return Err(IO_ERROR);
            };
            self.terminal.reset_pty(n);
            self.fs.pty_lock(n, false);
            let process = self.add_environment(
                &bytes,
                if build {
                    &[
                        "/bin/gcc",
                        "-fdiagnostics-color=never",
                        anova_abi::build::SOURCE,
                        "-o",
                        anova_abi::build::STAGING,
                    ]
                } else {
                    &["/linux/sh", "-i"]
                },
                owner,
                anova_abi::linux::PERSONALITY,
                &[
                    "PATH=/tc/bin:/linux",
                    "TERM=ansi",
                    "PS1=pty$ ",
                    "HOME=/persist",
                ],
            )?;
            child = Some(process);
            let pid = self.policy.entry(process).pid;
            let p = self.processes[process].as_mut().unwrap();
            // Inherited COM1, files and pipes must not leak into the shell or its children.
            self.fs.clear(&mut p.files);
            let mut path = *b"/dev/pts/0";
            path[9] += n as u8;
            self.fs.open(
                &mut p.files,
                ROOT,
                core::str::from_utf8(&path).unwrap(),
                OPEN_READ | OPEN_WRITE,
            )?;
            self.fs.dup2(&mut p.files, 0, 1)?;
            self.fs.dup2(&mut p.files, 0, 2)?;
            p.pty_member = Some(pid);
            self.terminal.attach(device, pid, pid);
            self.terminal.set_window(device, window(request));
            Ok((pid, device))
        })();
        match result {
            Ok((id, device)) => {
                self.ptys[slot] = Some(Session {
                    owner,
                    id,
                    build,
                    files,
                    device,
                    pending: [0; pty::MAX_TRANSFER],
                    length: 0,
                    #[cfg(feature = "bootstrap-metrics")]
                    baseline,
                });
                Ok(id)
            }
            Err(error) => {
                if let Some(child) = child {
                    self.finish(child, Outcome::Exit(125));
                    self.reap(child);
                }
                self.fs.clear(&mut files);
                if staging {
                    let cleanup = self.fs.unlink(ROOT, anova_abi::build::STAGING);
                    serial::write(format_args!(
                        "[BUILD:ROLLBACK] staging={cleanup:?} start_error={error}\n"
                    ));
                }
                Err(error)
            }
        }
    }

    pub(super) fn pty_close_owner(&mut self, owner: u64) {
        for slot in 0..self.ptys.len() {
            if self.ptys[slot].as_ref().is_some_and(|s| s.owner == owner) {
                let _ = self.pty_close(slot);
            }
        }
    }

    fn pty_close(&mut self, slot: usize) -> Result<(), i64> {
        let Some(mut session) = self.ptys[slot].take() else {
            return Ok(());
        };
        self.fs.clear(&mut session.files);
        // Terminate the complete tagged family before reaping, so scheduler reparenting
        // cannot let an orphan escape. The tag is independent of mutable SID/PGID values.
        for i in 0..CAPACITY {
            if self.processes[i]
                .as_ref()
                .is_some_and(|p| p.pty_member == Some(session.id) && p.image.is_some())
            {
                self.finish(i, Outcome::Signal(15));
            }
        }
        for i in 0..CAPACITY {
            if self.processes[i]
                .as_ref()
                .is_some_and(|p| p.pty_member == Some(session.id))
            {
                self.reap(i);
            }
        }
        let cleanup = if session.build {
            // Successful publication already renamed this path; otherwise remove this
            // job's reserved partial output. Persistent I/O faults may require recovery.
            let result = self.fs.unlink(ROOT, anova_abi::build::STAGING);
            serial::write(format_args!(
                "[BUILD:CLEANUP] id={} staging={result:?}\n",
                session.id
            ));
            match result {
                Ok(()) | Err(NOT_FOUND) => Ok(()),
                Err(error) => Err(error),
            }
        } else {
            Ok(())
        };
        let (owner, id) = (session.owner, session.id);
        #[cfg(feature = "bootstrap-metrics")]
        let baseline = session.baseline;
        drop(session);
        serial::write(format_args!(
            "[PTY:CLOSED] owner={} session={} descendants_reaped=true\n",
            owner, id
        ));
        #[cfg(feature = "bootstrap-metrics")]
        {
            let current = (
                mm::heap_used(),
                mm::user::free_frames(),
                self.fs.live_objects(),
            );
            serial::write(format_args!(
                "[PTY:RECLAIM] owner={owner} session={id} heap_before={} heap_after={} frames_before={} frames_after={} opens_before={} opens_after={} pipes_before={} pipes_after={} exact={}\n",
                baseline.0,
                current.0,
                baseline.1,
                current.1,
                baseline.2.0,
                current.2.0,
                baseline.2.1,
                current.2.1,
                baseline == current
            ));
        }
        cleanup
    }
}

fn window(request: Request) -> [u8; 8] {
    let mut bytes = [0; 8];
    bytes[..2].copy_from_slice(&(request.rows as u16).to_le_bytes());
    bytes[2..4].copy_from_slice(&(request.columns as u16).to_le_bytes());
    bytes
}
