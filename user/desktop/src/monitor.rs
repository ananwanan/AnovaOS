// SPDX-License-Identifier: Apache-2.0
//! Display bounded live samples, select exact task identities and retain recent wait status.
//! Show launch and service errors while protecting the Files buffer and replacement budget.

use super::{Session, Window, gui, rt};
use anova_gui_runtime::{Error, GuiEvent};
use core::fmt::{self, Write};
use rt::abi::{monitor as m, *};

const SAMPLE_TICKS: u64 = 50;
const ROWS: usize = 10;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Resources,
    Tasks,
    Services,
}

pub enum Action {
    None,
    Compositor,
    Session,
    Quit,
    Poweroff,
}

pub struct Panel {
    pub page: Option<Page>,
    resources: Result<m::Resources, i64>,
    tasks: [m::Task; m::MAX_TASKS],
    count: usize,
    selected: u64,
    next_sample: u64,
    task_error: Option<i64>,
    action: Option<(&'static str, i64)>,
    exit: Option<(&'static str, WaitStatus)>,
    replacement: &'static str,
}

impl Panel {
    pub fn new() -> Self {
        Self {
            page: None,
            resources: Ok(m::Resources::default()),
            tasks: [m::Task::default(); m::MAX_TASKS],
            count: 0,
            selected: 0,
            next_sample: 0,
            task_error: None,
            action: None,
            exit: None,
            replacement: "Initial session",
        }
    }

    pub fn open(&mut self, page: Page) {
        self.page = Some(page);
    }

    pub fn sample(&mut self) -> bool {
        let Some(page) = self.page else {
            return false;
        };
        let Ok(info) = rt::info() else {
            return false;
        };
        if info.ticks < self.next_sample {
            return false;
        }
        self.next_sample = info.ticks.saturating_add(SAMPLE_TICKS);
        self.resources = rt::monitor::resources();
        if page == Page::Tasks {
            match rt::monitor::tasks(&mut self.tasks) {
                Ok(tasks) => {
                    self.count = tasks.len();
                    self.tasks[..self.count].sort_unstable_by_key(|task| task.pid);
                    self.task_error = None;
                }
                Err(error) => {
                    self.count = 0;
                    self.task_error = Some(error);
                }
            }
        }
        true
    }

    pub fn remember(&mut self, name: &'static str, status: WaitStatus) {
        self.exit = Some((name, status));
    }

    pub fn report(&mut self, action: &'static str, status: Result<(), i64>) {
        let code = status.err().unwrap_or(0);
        self.action = Some((action, code));
        gui::log(format_args!(
            "[DESKTOP:SERVICE] action={action} result={code}\n"
        ));
    }

    pub fn application_result(&mut self, action: &'static str, result: Result<(), i64>) {
        self.action = result.err().map(|error| {
            gui::log(format_args!("[DESKTOP:LAUNCH_ERROR] error={error}\n"));
            (action, error)
        });
    }

    pub fn replaced(&mut self, reason: &'static str) {
        self.replacement = reason;
        self.next_sample = 0;
    }

    pub fn event(&mut self, session: &Session, event: &GuiEvent) -> Action {
        if event.kind != 2 || event.state != 1 {
            return Action::None;
        }
        match event.key {
            1 => self.page = None,
            2 => self.open(Page::Resources),
            3 => self.open(Page::Tasks),
            4 => self.open(Page::Services),
            _ => {
                if self.page == Some(Page::Tasks) {
                    self.task_event(session, event);
                } else if self.page == Some(Page::Services) {
                    return match event.codepoint as u8 {
                        b'c' => Action::Compositor,
                        b'r' => Action::Session,
                        b'q' => Action::Quit,
                        b'p' => Action::Poweroff,
                        _ => Action::None,
                    };
                }
            }
        }
        Action::None
    }

    fn task_event(&mut self, session: &Session, event: &GuiEvent) {
        if matches!(event.key, 0x148 | 0x150) {
            let next = if event.key == 0x148 {
                self.tasks[..self.count]
                    .iter()
                    .rfind(|task| task.pid < self.selected)
            } else {
                self.tasks[..self.count]
                    .iter()
                    .find(|task| task.pid > self.selected)
            };
            if let Some(task) = next {
                self.selected = task.pid;
            }
        } else if event.codepoint == u32::from(b's') && self.selected != 0 {
            let pid = self.selected;
            let result = if session.cooperative(pid) {
                Err(BUSY)
            } else if pid == session.server {
                Err(PERMISSION_DENIED)
            } else {
                rt::monitor::stop(pid)
            };
            self.action = Some(match result {
                Ok(true) => ("Task stopped", 0),
                Ok(false) => ("Task already exited", 0),
                Err(BUSY) => ("Close app with F4", BUSY),
                Err(error) => ("Task stop denied", error),
            });
            gui::log(format_args!(
                "[DESKTOP:TASK_STOP] pid={pid} result={result:?}\n"
            ));
        }
    }

    pub fn render(
        &self,
        window: &mut Window,
        session: &Session,
        generation: usize,
    ) -> Result<(), Error> {
        window.clear(0x182838)?;
        line(
            window,
            16,
            format_args!("1 Resources   2 Tasks   3 Services   Esc"),
        )?;
        match self.page {
            Some(Page::Resources) => self.render_resources(window)?,
            Some(Page::Tasks) => self.render_tasks(window, session)?,
            Some(Page::Services) => {
                line(
                    window,
                    36,
                    format_args!("Compositor PID {}  generation {generation}", session.server),
                )?;
                line(
                    window,
                    48,
                    format_args!(
                        "{}; {} replacements left",
                        self.replacement,
                        super::RESTARTS - generation
                    ),
                )?;
                line(
                    window,
                    68,
                    format_args!("c replace compositor   r restart session"),
                )?;
                line(
                    window,
                    80,
                    format_args!("q close desktop   p shut down system"),
                )?;
                line(
                    window,
                    104,
                    format_args!("F4 in Files/Terminal to close the app"),
                )?;
                line(
                    window,
                    116,
                    format_args!("COM1 shell available for recovery"),
                )?;
            }
            None => {}
        }
        if let Some((name, status)) = self.exit {
            line(
                window,
                164,
                format_args!(
                    "{name} {} exit {}/{}",
                    status.pid, status.status.reason, status.status.code
                ),
            )?;
        }
        self.notice(window)
    }

    pub fn notice(&self, window: &mut Window) -> Result<(), Error> {
        if let Some((action, code)) = self.action {
            line(window, 180, format_args!("{action}  {}", ErrorCode(code)))?;
        }
        Ok(())
    }

    fn render_resources(&self, w: &mut Window) -> Result<(), Error> {
        let r = match &self.resources {
            Ok(r) => r,
            Err(error) => {
                return line(
                    w,
                    36,
                    format_args!("Resource sample failed {}", ErrorCode(*error)),
                );
            }
        };
        let used = r
            .frames_total
            .saturating_sub(r.frames_free)
            .saturating_sub(r.frames_reserved);
        let tasks: u32 = r.tasks.iter().sum();
        line(
            w,
            32,
            format_args!("Current sample @ {} ticks (10 ms/tick)", r.ticks),
        )?;
        line(
            w,
            44,
            format_args!(
                "{}B pages  used {used}  free {}",
                r.page_bytes, r.frames_free
            ),
        )?;
        line(
            w,
            56,
            format_args!(
                "Pages total {}  reserved {}",
                r.frames_total, r.frames_reserved
            ),
        )?;
        line(
            w,
            68,
            format_args!("Heap {} / {} B", r.heap_used, r.heap_bytes),
        )?;
        line(
            w,
            80,
            format_args!(
                "Tasks {tasks}/{}  free {}",
                r.tasks_total,
                r.tasks_total.saturating_sub(tasks)
            ),
        )?;
        let [ready, run, sleep, wait, block, stop, zombie] = r.tasks;
        line(
            w,
            92,
            format_args!(
                "Ready {ready} Run {run} Sleep {sleep} Wait {wait} Block {block} Stop {stop} Z {zombie}"
            ),
        )?;
        line(
            w,
            104,
            format_args!(
                "VFS {}/{} ({} slots)  Open {}/{}",
                r.nodes_used, r.nodes_total, r.node_slots, r.opens_used, r.opens_total
            ),
        )?;
        line(
            w,
            116,
            format_args!(
                "PTY {}/{} ({} owned)  GUI {}/{} ({})",
                r.ptys_used, r.ptys_total, r.ptys_managed, r.gui_used, r.gui_total, r.gui_connected
            ),
        )?;
        if r.disk_format == 0 {
            line(
                w,
                128,
                format_args!("/persist N/A  GUI session {}", r.gui_session),
            )?;
        } else if r.disk_flags & m::DISK_FAILED != 0 {
            line(
                w,
                128,
                format_args!("ext{} failed; disk free N/A", r.disk_format),
            )?;
        } else {
            let mode = if r.disk_flags & m::DISK_READ_ONLY != 0 {
                "ro"
            } else if r.disk_flags & m::DISK_JOURNALED != 0 {
                "JBD2"
            } else {
                "rw"
            };
            line(
                w,
                128,
                format_args!(
                    "ext{} {mode} free {}/{} x {}B",
                    r.disk_format, r.blocks_free, r.blocks_total, r.block_bytes
                ),
            )?;
            line(
                w,
                140,
                format_args!(
                    "Inodes free {}/{}  GUI session {}",
                    r.inodes_free, r.inodes_total, r.gui_session
                ),
            )?;
        }
        line(
            w,
            152,
            format_args!("FD cap {}   RSS / CPU time / peaks N/A", r.fds_per_process),
        )
    }

    fn render_tasks(&self, w: &mut Window, session: &Session) -> Result<(), Error> {
        if let Some(error) = self.task_error {
            return line(
                w,
                32,
                format_args!("Task sample failed {}", ErrorCode(error)),
            );
        }
        let tasks = &self.tasks[..self.count];
        let selected = tasks.iter().position(|task| task.pid == self.selected);
        line(
            w,
            32,
            format_args!("Owned tasks   Up/Down selects   s stops"),
        )?;
        let start = selected.unwrap_or(0).saturating_sub(ROWS - 1);
        for (row, task) in tasks.iter().skip(start).take(ROWS).enumerate() {
            let marker = if task.pid == self.selected { '>' } else { ' ' };
            let kind = match task.kind {
                m::NATIVE => "Native",
                m::LINUX_PROCESS => "Linux",
                m::LINUX_THREAD => "Thread",
                _ => "?",
            };
            let state = [
                "Ready", "Running", "Sleeping", "Waiting", "Blocked", "Stopped", "Zombie",
            ]
            .get(task.state as usize)
            .copied()
            .unwrap_or("?");
            let stop = if task.flags & m::CAN_STOP != 0
                && !session.cooperative(task.pid)
                && task.pid != session.server
            {
                "s"
            } else {
                " "
            };
            line(
                w,
                44 + row as i32 * 9,
                format_args!(
                    "{marker} {} {kind} {state} parent {} group {} {stop}",
                    task.pid, task.parent, task.group
                ),
            )?;
        }
        if let Some(index) = selected {
            let task = tasks[index];
            if task.state == m::ZOMBIE {
                line(
                    w,
                    144,
                    format_args!(
                        "Task {} exit {}/{}",
                        task.pid, task.exit.reason, task.exit.code
                    ),
                )?;
            } else {
                line(
                    w,
                    144,
                    format_args!(
                        "Start tick {}  PTY family {}",
                        task.started_tick, task.family
                    ),
                )?;
            }
        } else if self.selected != 0 {
            line(
                w,
                144,
                format_args!("Selected task {} no longer present", self.selected),
            )?;
        }
        Ok(())
    }
}

struct ErrorCode(i64);
impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            f.write_str("OK")
        } else if let Some(error) = rt::abi::Error::from_raw(self.0) {
            f.write_str(error.name())
        } else {
            self.0.fmt(f)
        }
    }
}

fn line(window: &mut Window, y: i32, args: fmt::Arguments<'_>) -> Result<(), Error> {
    let mut label = Label([0; 52], 0);
    let _ = label.write_fmt(args);
    window.text(
        4,
        y,
        core::str::from_utf8(&label.0[..label.1]).unwrap(),
        0xffffff,
    )
}

struct Label([u8; 52], usize);
impl Write for Label {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let n = (self.0.len() - self.1).min(value.len());
        let bytes = &mut self.0[self.1..self.1 + n];
        for (dest, byte) in bytes.iter_mut().zip(value.bytes()) {
            *dest = if byte.is_ascii() { byte } else { b'?' };
        }
        self.1 += n;
        Ok(())
    }
}
