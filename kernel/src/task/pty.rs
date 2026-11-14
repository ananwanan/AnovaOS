// SPDX-License-Identifier: Apache-2.0
//! Bridge exact native creators to private PTY masters and admitted Linux shells/desktop jobs.
//! Transfers never block. Session tags survive Linux fork/exec/setsid; creator death closes
//! the master and kills/reaps tagged descendants without granting authority over peer tasks.
//! Workflow descriptors pin the reserved output nodes across bounded publication calls.

use super::*;
use anova_abi::{
    pty::{self, Request},
    workflow::{self, Recipe},
};
use anova_vfs::Device;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Shell,
    Legacy,
    Workflow(Recipe),
}

impl Kind {
    fn recipe(self) -> Option<Recipe> {
        match self {
            Self::Shell => None,
            Self::Legacy => Some(Recipe::C),
            Self::Workflow(recipe) => Some(recipe),
        }
    }
    fn same_interface(self, other: Self) -> bool {
        matches!((self, other), (Self::Workflow(_), Self::Workflow(_))) || self == other
    }
}

pub(super) struct Session {
    owner: u64,
    id: u64,
    kind: Kind,
    files: Descriptors,
    device: Device,
    pending: [u8; pty::MAX_TRANSFER],
    length: usize,
    eof: bool,
    stage_pending: bool,
    workflow: Option<Publication>,
    #[cfg(feature = "bootstrap-metrics")]
    baseline: (usize, usize, (usize, usize)),
}

struct Publication {
    files: Descriptors,
    staging: u64,
    temporary: u64,
    copied: usize,
    cancelled: bool,
}

impl Kernel {
    pub(super) fn is_workflow_member(&self, index: usize) -> bool {
        self.process(index).pty_member.is_some_and(|id| {
            self.ptys
                .iter()
                .flatten()
                .any(|session| session.id == id && matches!(session.kind, Kind::Workflow(_)))
        })
    }

    pub(super) fn owns_pty_family(&self, owner: u64, id: u64) -> bool {
        self.ptys
            .iter()
            .flatten()
            .any(|session| session.owner == owner && session.id == id)
    }

    pub(super) fn pty_control(&mut self, caller: usize, frame: &TrapFrame) -> Result<i64, i64> {
        let operation = pty::decode(frame.rdi).ok_or(INVALID_ARGUMENT)?;
        let space = self.process(caller).image.as_ref().unwrap().space.share();
        let mut bytes = [0; 32];
        space.with_active(|| mm::uaccess::from_user(&space, UserAddr(frame.rdx), &mut bytes))?;
        let request = Request::from_bytes(bytes);
        if !request.valid(operation) {
            return Err(INVALID_ARGUMENT);
        }
        self.pty_request(caller, frame, operation, request, Kind::Shell)
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
            Kind::Legacy,
        )
    }

    pub(super) fn workflow_control(&mut self, caller: usize, f: &TrapFrame) -> Result<i64, i64> {
        let op = workflow::decode(f.rdi).ok_or(INVALID_ARGUMENT)?;
        let space = &self.process(caller).image.as_ref().unwrap().space;
        let mut bytes = [0; 32];
        space.with_active(|| mm::uaccess::from_user(space, UserAddr(f.rdx), &mut bytes))?;
        let request = anova_abi::build::Request::from_bytes(bytes);
        if !workflow::valid(request, op) {
            return Err(INVALID_ARGUMENT);
        }
        if op == workflow::START {
            let recipe = Recipe::decode(f.rsi).ok_or(INVALID_ARGUMENT)?;
            let args = recipe.args();
            if !self.fs.linux_executable(ROOT, args[0])? {
                return Err(PERMISSION_DENIED);
            }
            // The legacy ABI still allows the old start behavior. New recipes require
            // a Linux-admitting persistent work disk before creating a staging file.
            self.fs.check_linux_work_disk()?;
            let terminal = Request {
                rows: 26,
                columns: 50,
                buffer: 0,
                length: 0,
                reserved: 0,
            };
            return self
                .pty_start(caller, terminal, Kind::Workflow(recipe))
                .map(|id| id as i64);
        }
        let kind = Kind::Workflow(Recipe::C);
        let id = f.rsi;
        let owner = self.policy.entry(caller).pid;
        match op {
            workflow::CANCEL => {
                let slot = self.pty_slot(owner, id, kind)?;
                self.policy.child(caller, id)?;
                let session = self.ptys[slot].as_mut().unwrap();
                let publication = session.workflow.as_mut().unwrap();
                if !session.stage_pending || publication.cancelled {
                    return Err(BUSY);
                }
                publication.cancelled = true;
                self.terminate_family(id, Outcome::Signal(15));
                Ok(0)
            }
            workflow::PUBLISH => self.publish_workflow(caller, owner, id, kind),
            _ => self.pty_request(
                caller,
                f,
                match op {
                    workflow::READ => pty::READ,
                    workflow::STATUS => pty::STATUS,
                    workflow::CLOSE => pty::CLOSE,
                    _ => return Err(INVALID_ARGUMENT),
                },
                Request {
                    buffer: request.buffer,
                    length: request.length,
                    rows: 0,
                    columns: 0,
                    reserved: 0,
                },
                kind,
            ),
        }
    }

    fn publish_workflow(
        &mut self,
        caller: usize,
        owner: u64,
        id: u64,
        kind: Kind,
    ) -> Result<i64, i64> {
        let slot = self.pty_slot(owner, id, kind)?;
        let session = self.ptys[slot].as_ref().unwrap();
        let recipe = session.kind.recipe().unwrap();
        let publication = session.workflow.as_ref().unwrap();
        let leader = self.policy.child(caller, id)?;
        match self.policy.entry(leader).state {
            State::Zombie(ExitStatus { reason: 0, code: 0 }) => {}
            State::Zombie(_) => return Err(PERMISSION_DENIED),
            _ => return Err(WOULD_BLOCK),
        }
        if !session.eof
            || self
                .processes
                .iter()
                .flatten()
                .any(|p| p.pty_member == Some(id) && p.image.is_some())
        {
            return Err(BUSY);
        }
        if !session.stage_pending {
            return Err(NOT_FOUND);
        }
        if publication.cancelled {
            return Err(PERMISSION_DENIED);
        }
        let length = Self::check_reserved(
            &mut self.fs,
            &publication.files,
            publication.temporary,
            recipe.temporary(),
        )?;
        let staged = Self::check_reserved(
            &mut self.fs,
            &publication.files,
            publication.staging,
            recipe.staging(),
        )?;
        if length == 0 || length > anova_vfs::FILE_BYTES {
            return Err(INVALID_ARGUMENT);
        }
        let copied = publication.copied;
        if staged != copied || copied > length {
            return Err(IO_ERROR);
        }
        if copied < length {
            // A Linux linker can produce many synchronous small writes. Collect them
            // in existing tmpfs, then commit at most one bounded disk chunk per turn.
            let mut buffer = [0; workflow::COPY_BYTES];
            let count = (length - copied).min(buffer.len());
            let n = self.fs.read_at(
                &publication.files,
                publication.temporary,
                copied as u64,
                &mut buffer[..count],
            )?;
            if n != count
                || self.fs.write_at(
                    &publication.files,
                    publication.staging,
                    copied as u64,
                    &buffer[..count],
                )? != count
            {
                return Err(IO_ERROR);
            }
            self.ptys[slot]
                .as_mut()
                .unwrap()
                .workflow
                .as_mut()
                .unwrap()
                .copied += count;
            return Err(WOULD_BLOCK);
        }
        self.fs.set_executable(ROOT, recipe.staging())?;
        // The backend handles rename and target replacement in the existing transaction.
        self.fs
            .rename_file(ROOT, recipe.staging(), recipe.output(), true)?;
        self.ptys[slot].as_mut().unwrap().stage_pending = false;
        Ok(0)
    }

    fn check_reserved(
        fs: &mut Vfs<'_>,
        files: &Descriptors,
        fd: u64,
        path: &str,
    ) -> Result<usize, i64> {
        let (identity, _, _, _) = fs.descriptor_metadata(files, fd)?;
        let (named, kind, length, _) = fs.metadata_nofollow(ROOT, path)?;
        if named != identity || kind != 1 {
            return Err(BUSY);
        }
        Ok(length)
    }

    fn remove_reserved(
        fs: &mut Vfs<'_>,
        files: &Descriptors,
        fd: u64,
        path: &str,
    ) -> Result<(), i64> {
        match Self::check_reserved(fs, files, fd, path) {
            Ok(_) => fs.unlink(ROOT, path),
            Err(NOT_FOUND) => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn pty_slot(&self, owner: u64, id: u64, kind: Kind) -> Result<usize, i64> {
        self.ptys
            .iter()
            .position(|s| {
                s.as_ref()
                    .is_some_and(|s| s.owner == owner && s.id == id && s.kind.same_interface(kind))
            })
            .ok_or(NO_CHILD)
    }

    fn pty_request(
        &mut self,
        caller: usize,
        frame: &TrapFrame,
        operation: u32,
        request: Request,
        kind: Kind,
    ) -> Result<i64, i64> {
        let space = self.process(caller).image.as_ref().unwrap().space.share();
        let owner = self.policy.entry(caller).pid;
        if operation == pty::START {
            if frame.rsi != 0 {
                return Err(INVALID_ARGUMENT);
            }
            return self.pty_start(caller, request, kind).map(|id| id as i64);
        }
        let slot = self.pty_slot(owner, frame.rsi, kind)?;
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
                            Err(IO_ERROR) => {
                                session.eof = true;
                                return Ok(0);
                            }
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

    fn pty_start(&mut self, caller: usize, request: Request, kind: Kind) -> Result<u64, i64> {
        let recipe = kind.recipe();
        if recipe.is_some()
            && self
                .ptys
                .iter()
                .flatten()
                .any(|s| s.kind.recipe().is_some())
        {
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
        let args = match kind {
            Kind::Shell => &["/linux/sh", "-i"][..],
            Kind::Legacy => &[
                "/bin/gcc",
                "-fdiagnostics-color=never",
                anova_abi::build::SOURCE,
                "-o",
                anova_abi::build::STAGING,
            ][..],
            Kind::Workflow(recipe) => recipe.args(),
        };
        let executable = args[0];
        if !self.fs.linux_executable(ROOT, executable)? {
            return Err(PERMISSION_DENIED);
        }
        let bytes = self.fs.load_executable(ROOT, executable)?;
        let owner = self.policy.entry(caller).pid;
        let mut files = Descriptors::default();
        let mut child = None;
        let mut stage_created = false;
        let mut build_files = Descriptors::default();
        let mut staged_fd = None;
        let mut temporary_fd = None;
        let result = (|| -> Result<(u64, Device), i64> {
            if let Some(recipe) = recipe {
                // Reserve only a new regular staging file. Never truncate a prior output
                // or silently adopt a recovery file from an earlier creator/system boot.
                let fd = self.fs.open(
                    &mut build_files,
                    ROOT,
                    recipe.staging(),
                    OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE,
                )?;
                stage_created = true;
                if matches!(kind, Kind::Workflow(_)) {
                    staged_fd = Some(fd);
                    temporary_fd = Some(self.fs.open(
                        &mut build_files,
                        ROOT,
                        recipe.temporary(),
                        OPEN_READ | OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE,
                    )?);
                } else {
                    self.fs.close(&mut build_files, fd)?;
                }
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
                args,
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
                    kind,
                    files,
                    device,
                    pending: [0; pty::MAX_TRANSFER],
                    length: 0,
                    eof: false,
                    stage_pending: stage_created,
                    workflow: staged_fd
                        .zip(temporary_fd)
                        .map(|(staging, temporary)| Publication {
                            files: build_files,
                            staging,
                            temporary,
                            copied: 0,
                            cancelled: false,
                        }),
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
                if let Some(recipe) = recipe {
                    if temporary_fd.is_some() {
                        let _ = self.fs.unlink(ROOT, recipe.temporary());
                    }
                    // START has not returned, so no caller can replace these names
                    // between reservation, descriptor cleanup and rollback.
                    self.fs.clear(&mut build_files);
                    if stage_created {
                        let cleanup = self.fs.unlink(ROOT, recipe.staging());
                        serial::write(format_args!(
                            "[BUILD:ROLLBACK] staging={cleanup:?} start_error={error}\n"
                        ));
                    }
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

    fn terminate_family(&mut self, id: u64, outcome: Outcome) {
        // The tag survives reparenting and changes to SID/PGID. Cancel leaves the
        // leader waitable until its owner copies status and closes the handle.
        for i in 0..CAPACITY {
            if self.processes[i]
                .as_ref()
                .is_some_and(|p| p.pty_member == Some(id) && p.image.is_some())
            {
                self.finish(i, outcome);
            }
        }
    }

    fn pty_close(&mut self, slot: usize) -> Result<(), i64> {
        let Some(mut session) = self.ptys[slot].take() else {
            return Ok(());
        };
        self.fs.clear(&mut session.files);
        self.terminate_family(session.id, Outcome::Signal(15));
        for i in 0..CAPACITY {
            if self.processes[i]
                .as_ref()
                .is_some_and(|p| p.pty_member == Some(session.id))
            {
                self.reap(i);
            }
        }
        let cleanup = if session.stage_pending
            && let Some(recipe) = session.kind.recipe()
        {
            // Successful publication already renamed this path; otherwise remove this
            // job's reserved partial output. Persistent I/O faults may require recovery.
            let result = if let Some(publication) = &session.workflow {
                Self::remove_reserved(
                    &mut self.fs,
                    &publication.files,
                    publication.staging,
                    recipe.staging(),
                )
            } else {
                self.fs.unlink(ROOT, recipe.staging())
            };
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
        let temporary_cleanup = if let Some(publication) = &mut session.workflow {
            let result = Self::remove_reserved(
                &mut self.fs,
                &publication.files,
                publication.temporary,
                session.kind.recipe().unwrap().temporary(),
            );
            let stage_close = self.fs.close(&mut publication.files, publication.staging);
            let tmp_close = self.fs.close(&mut publication.files, publication.temporary);
            result.and(stage_close).and(tmp_close)
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
        cleanup.and(temporary_cleanup)
    }
}

fn window(request: Request) -> [u8; 8] {
    let mut bytes = [0; 8];
    bytes[..2].copy_from_slice(&(request.rows as u16).to_le_bytes());
    bytes[2..4].copy_from_slice(&(request.columns as u16).to_le_bytes());
    bytes
}
