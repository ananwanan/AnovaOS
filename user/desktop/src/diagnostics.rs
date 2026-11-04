// SPDX-License-Identifier: Apache-2.0
//! Keep bounded compiler attempts in the launcher across compositor replacements.
//! Completion comes only from the job wait record; raw merged output never drives status.

use super::{Window, gui, rt};
use anova_gui_runtime::{Error, GuiEvent};
use anova_terminal::build_log::{History, Phase};
use core::{cell::UnsafeCell, fmt::Write};
use rt::abi::{build as b, *};

struct Storage(UnsafeCell<History>);
// SAFETY: Only run's single UI owner borrows this BSS, once, without forking.
unsafe impl Sync for Storage {}
static HISTORY: Storage = Storage(UnsafeCell::new(History::new()));
const MAX_DRAIN_TURNS: u8 = 3;

pub struct Diagnostics {
    history: &'static mut History,
    job: Option<rt::build::Job>,
    active: usize,
    selected: usize,
    scroll: usize,
    drain_turns: u8,
    pub visible: bool,
}
struct Label {
    bytes: [u8; 128],
    len: usize,
}
impl Label {
    fn new() -> Self {
        Self {
            bytes: [0; 128],
            len: 0,
        }
    }
    fn text(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap()
    }
}
impl Write for Label {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let end = self.len.checked_add(s.len()).ok_or(core::fmt::Error)?;
        self.bytes
            .get_mut(self.len..end)
            .ok_or(core::fmt::Error)?
            .copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}
impl Diagnostics {
    /// Caller enters once per launcher process, after all fork-based checks.
    pub fn new() -> Self {
        // SAFETY: run is called once by main; the borrow stays in its sole Diagnostics.
        let history = unsafe { &mut *HISTORY.0.get() };
        Self {
            history,
            job: None,
            active: 0,
            selected: 0,
            scroll: 0,
            drain_turns: 0,
            visible: false,
        }
    }
    fn start(&mut self) {
        if self.job.is_some() {
            return;
        }
        let Some(index) = self.history.begin() else {
            return;
        };
        self.active = index;
        self.selected = index;
        self.scroll = 0;
        self.drain_turns = 0;
        let r = &mut self.history.records[index];
        match rt::build::Job::start() {
            Ok(job) => {
                r.id = job.id();
                r.phase = Phase::Running;
                gui::log(format_args!(
                    "[BUILD:START] attempt={} id={} command={} merged=true\n",
                    r.attempt,
                    r.id,
                    b::COMMAND
                ));
                self.job = Some(job);
            }
            Err(error) => {
                r.phase = Phase::StartFailed;
                r.error = error;
                gui::log(format_args!(
                    "[BUILD:START_ERROR] attempt={} error={error}\n",
                    r.attempt
                ));
            }
        }
    }
    pub fn event(&mut self, event: &GuiEvent) {
        if event.kind != 2 || event.state != 1 {
            return;
        }
        match event.key {
            1 => self.visible = false,
            26 | 27 => {
                self.selected = self.history.neighbor(self.selected, event.key == 27);
                self.scroll = 0;
            }
            0x149 => {
                self.scroll = (self.scroll + 10).min(
                    self.history.records[self.selected]
                        .lines()
                        .count()
                        .saturating_sub(12),
                )
            }
            0x151 => self.scroll = self.scroll.saturating_sub(10),
            _ if event.codepoint == 103 => self.start(),
            _ => {}
        }
    }
    pub fn turn(&mut self) -> bool {
        let Some(job) = &self.job else {
            return false;
        };
        let r = &mut self.history.records[self.active];
        let mut changed = false;
        let mut quiet = false;
        let mut eof = false;
        let mut output = [0; 1024];
        for _ in 0..4 {
            match job.read(&mut output) {
                Ok(0) => {
                    eof = true;
                    quiet = true;
                    break;
                }
                Ok(n) => {
                    r.append(&output[..n]);
                    changed = true;
                }
                Err(WOULD_BLOCK) => {
                    quiet = true;
                    break;
                }
                Err(error) => {
                    changed |= r.error != error || r.phase != Phase::CaptureFailed;
                    r.error = error;
                    r.incomplete = true;
                    r.phase = Phase::CaptureFailed;
                    break;
                }
            }
        }
        match job.status() {
            Ok(Some(status)) => {
                assert_eq!(status.pid, r.id);
                changed |= r.status != Some(status.status);
                r.status = Some(status.status);
                self.drain_turns = self.drain_turns.saturating_add(1);
                if r.phase != Phase::CaptureFailed {
                    r.phase = Phase::Draining;
                }
                // A detached continuous writer must not hold a completed leader's
                // task/PTY forever. Preserve bounded tail work, then explicitly cut.
                if quiet || r.phase == Phase::CaptureFailed || self.drain_turns >= MAX_DRAIN_TURNS {
                    r.incomplete |= !eof;
                    r.phase = Phase::Finished;
                    if status.status.reason == 0
                        && status.status.code == 0
                        && r.error == 0
                        && !r.incomplete
                    {
                        match rt::files::replace(b::STAGING, b::OUTPUT) {
                            Ok(()) => r.published = true,
                            Err(error) => r.error = error,
                        }
                    }
                    if let Some(job) = self.job.take()
                        && let Err(error) = job.close()
                    {
                        r.error = error;
                    }
                    gui::log(format_args!(
                        "[BUILD:END] attempt={} id={} reason={} code={} bytes={} dropped={} published={} error={} incomplete={} drain_turns={}\n",
                        r.attempt,
                        r.id,
                        status.status.reason,
                        status.status.code,
                        r.len(),
                        r.dropped,
                        r.published,
                        r.error,
                        r.incomplete,
                        self.drain_turns
                    ));
                    changed = true;
                }
            }
            Ok(None) => {}
            Err(error) => {
                // Keep the owner handle and retry; a failed status delivery cannot
                // consume the result or turn an unknown outcome into a success.
                changed |= r.error != error;
                r.error = error;
            }
        }
        changed
    }
    pub fn render(&self, w: &mut Window) -> Result<(), Error> {
        let r = &self.history.records[self.selected];
        w.clear(0x182838)?;
        w.text(4, 16, "BUILD DIAGNOSTICS - MERGED STDOUT/STDERR", 0xffffff)?;
        w.text(4, 32, "/bin/gcc -fdiagnostics-color=never", 0xffffff)?;
        w.text(
            4,
            40,
            "/persist/build.c -o /persist/build-app.next",
            0xffffff,
        )?;
        let mut label = Label::new();
        let _ = write!(label, "Attempt {}  task {}  {:?}", r.attempt, r.id, r.phase);
        w.text(4, 56, label.text(), 0xffffff)?;
        let mut label = Label::new();
        if let Some(status) = r.status {
            let kind = match status.reason {
                0 => "Exit",
                1 => "Fault",
                2 => "Rejected",
                3 => "Signal",
                _ => "Unknown",
            };
            let _ = write!(
                label,
                "{kind} reason={} code={} error={}",
                status.reason, status.code, r.error
            );
        } else {
            let _ = write!(label, "Exit pending/unknown  error={}", r.error);
        }
        w.text(4, 64, label.text(), 0xffffff)?;
        w.text(
            4,
            72,
            if r.published {
                "Published /persist/build-app (sync separately)"
            } else if r.incomplete {
                "Capture cut; prior build-app retained"
            } else if r.error != 0 && r.status.is_some_and(|s| s.reason == 0 && s.code == 0) {
                "Publication/capture error: inspect storage"
            } else {
                "Prior build-app retained; no new publication"
            },
            0xffffff,
        )?;
        if r.phase == Phase::StartFailed {
            w.text(
                4,
                80,
                match r.error {
                    FILE_LIMIT | PROCESS_LIMIT | NO_MEMORY | NO_SPACE => {
                        "Resource limit; release capacity, then g retries"
                    }
                    ALREADY_EXISTS => "Inspect/remove build-app.next before retry",
                    NOT_FOUND | PERMISSION_DENIED => {
                        "Compiler missing/unadmitted; select toolchain"
                    }
                    _ => "Start failed; inspect task/storage before retry",
                },
                0xffcc66,
            )?;
        } else if r.phase == Phase::Finished && r.error != 0 {
            w.text(4, 80, "Task I/O/cleanup error: inspect storage", 0xffcc66)?;
        }
        w.text(
            4,
            88,
            "g build; [/] records; PgUp/Dn logs; Esc back",
            0xffffff,
        )?;
        let mut label = Label::new();
        let _ = write!(
            label,
            "Dropped {}B; evicted {}; cut={}",
            r.dropped, self.history.evicted, r.incomplete
        );
        w.text(4, 96, label.text(), 0xffcc66)?;
        let lines = r.lines().count();
        let start = lines.saturating_sub(12 + self.scroll);
        for (row, (bytes, n)) in r.lines().skip(start).take(12).enumerate() {
            w.text(
                4,
                104 + row as i32 * 8,
                core::str::from_utf8(&bytes[..n]).unwrap(),
                0xffffff,
            )?;
        }
        Ok(())
    }
}
