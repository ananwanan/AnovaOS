// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Supervise a compositor and three admitted GUI clients alongside an independent COM1 shell.
//! Bound restarts, reap owned children and protect the files client's explicit close dialog.

mod checks;

use anova_gui::transport as t;
use anova_gui_runtime::{Error, GuiEvent, GuiRect, GuiWindowDesc, Window};
use anova_gui_user as gui;
use anova_rt::{self as rt, abi::*};

const APPLICATIONS: [&str; 3] = ["/bin/desktop-c", "/bin/desktop-cpp", "/bin/desktop-rust"];
const RESTARTS: usize = 3;

struct Session {
    server: u64,
    clients: [Option<u64>; 3],
    terminal_running: bool,
    files_running: bool,
}

impl Session {
    fn files(&mut self) -> Result<(), i64> {
        if self.files_running {
            return Err(BUSY);
        }
        if self.clients[1].is_some() {
            self.toggle(1)?;
        }
        let pid = rt::spawn("/bin/files", "")?;
        self.clients[1] = Some(pid);
        self.files_running = true;
        gui::log(format_args!("[DESKTOP:FILES] pid={pid}\n"));
        Ok(())
    }
    fn terminal(&mut self) -> Result<(), i64> {
        if self.clients[2].is_some() {
            self.toggle(2)?;
        }
        let pid = rt::spawn("/bin/terminal", "")?;
        self.clients[2] = Some(pid);
        self.terminal_running = true;
        gui::log(format_args!("[DESKTOP:TERMINAL] pid={pid}\n"));
        Ok(())
    }
    fn probe(&mut self, index: usize, mode: &str) -> Result<(), i64> {
        if self.clients[index].is_some() {
            self.toggle(index)?;
        }
        let pid = rt::spawn("gui-client", mode)?;
        self.clients[index] = Some(pid);
        gui::log(format_args!(
            "[DESKTOP:PROBE] app={index} mode={mode} pid={pid}\n"
        ));
        Ok(())
    }

    fn toggle(&mut self, index: usize) -> Result<(), i64> {
        // Only the file client can resolve its unsaved/failed-save dialog. Explicit
        // launcher stop/restart/quit must not bypass that cooperative close path.
        if index == 1 && self.files_running {
            return Err(BUSY);
        }
        if index == 2 {
            self.terminal_running = false;
        }
        if let Some(pid) = self.clients[index] {
            stop(pid)?;
            self.clients[index] = None;
        } else {
            let pid = rt::spawn(APPLICATIONS[index], "")?;
            self.clients[index] = Some(pid);
            gui::log(format_args!("[DESKTOP:LAUNCH] app={index} pid={pid}\n"));
        }
        Ok(())
    }

    fn observe(&mut self) -> Result<bool, i64> {
        for (index, slot) in self.clients.iter_mut().enumerate() {
            if let Some(pid) = *slot
                && let Some(status) = rt::process::poll(pid)?
            {
                gui::log(format_args!(
                    "[DESKTOP:EXIT] app={index} pid={pid} reason={} code={}\n",
                    status.status.reason, status.status.code
                ));
                *slot = None;
                if index == 2 {
                    self.terminal_running = false;
                }
                if index == 1 {
                    self.files_running = false;
                }
            }
        }
        if let Some(status) = rt::process::poll(self.server)? {
            gui::log(format_args!(
                "[DESKTOP:COMPOSITOR_EXIT] reason={} code={}\n",
                status.status.reason, status.status.code
            ));
            self.server = 0;
            return Ok(false);
        }
        Ok(true)
    }

    fn cleanup(&mut self) -> Result<(), i64> {
        for slot in &mut self.clients {
            if let Some(pid) = *slot {
                stop(pid)?;
                *slot = None;
            }
        }
        if self.server != 0 {
            stop(self.server)?;
            self.server = 0;
        }
        rt::write(b"[DESKTOP:CLEANUP] owned_children_reaped=true\n");
        Ok(())
    }
}

fn stop(pid: u64) -> Result<(), i64> {
    rt::process::stop(pid)?;
    let status = rt::wait(pid)?.status;
    gui::log(format_args!(
        "[DESKTOP:REAP] pid={pid} reason={} code={}\n",
        status.reason, status.code
    ));
    Ok(())
}

fn render(window: &mut Window, session: &Session, event: Option<&GuiEvent>) -> Result<u32, Error> {
    window.clear(0x182838)?;
    window.text(12, 24, "ANOVA DESKTOP", 0xffffff)?;
    window.text(12, 44, "1/2/3 launch or stop; r restart", 0xffffff)?;
    window.text(
        12,
        62,
        if session.files_running {
            "Close Files with F4 before stop/restart/exit"
        } else {
            "f files; t terminal; q exit"
        },
        0xffffff,
    )?;
    let mut action = 0;
    for (index, name) in [
        "C client",
        if session.files_running {
            "Files"
        } else {
            "C++ client"
        },
        if session.terminal_running {
            "Terminal"
        } else {
            "Rust client"
        },
    ]
    .iter()
    .enumerate()
    {
        let y = 88 + index as i32 * 30;
        window.text(12, y + 5, name, 0xffffff)?;
        if window.button(
            GuiRect {
                x: 160,
                y,
                width: 140,
                height: 24,
            },
            if session.clients[index].is_some() {
                "Stop"
            } else {
                "Launch"
            },
            event,
        )? {
            action = index as u32 + 1;
        }
    }
    match window.present() {
        Ok(()) | Err(Error::WouldBlock) => Ok(action),
        Err(error) => Err(error),
    }
}

fn gui_error(error: Error) -> i64 {
    match error {
        Error::Disconnected => BROKEN_PIPE,
        _ => IO_ERROR,
    }
}

fn run(test: bool) -> Result<(), i64> {
    for generation in 0..=RESTARTS {
        let mut session = Session {
            server: rt::spawn("compositor", if test { "desktop-test" } else { "desktop" })?,
            clients: [None; 3],
            terminal_running: false,
            files_running: false,
        };
        let outcome = (|| -> Result<bool, i64> {
            let identity = gui::connect()?;
            gui::scalar(t::CLOSE)?;
            // Drop the launcher window before replacing its compositor/session.
            let mut window =
                Window::create(GuiWindowDesc::new(20, 20, 320, 200)).map_err(gui_error)?;
            for index in 0..3 {
                session.toggle(index)?;
            }
            render(&mut window, &session, None).map_err(gui_error)?;
            gui::log(format_args!(
                "[DESKTOP:READY] generation={generation} session={identity} clients=3 quota=4\n"
            ));
            loop {
                if !session.observe()? {
                    return Ok(true);
                }
                let event = match window.poll() {
                    Ok(event) => event,
                    Err(Error::WouldBlock) => {
                        rt::sleep(1)?;
                        continue;
                    }
                    Err(Error::Disconnected) => return Ok(true),
                    Err(_) => return Err(IO_ERROR),
                };
                let mut action = render(&mut window, &session, Some(&event)).map_err(gui_error)?;
                if event.kind == 2 && event.state == 1 {
                    match event.codepoint {
                        49..=51 => action = event.codepoint - 48,
                        114 if !session.files_running => return Ok(true),
                        113 if !session.files_running => return Ok(false),
                        102 if !test => {
                            if let Err(error) = session.files() {
                                gui::log(format_args!("[DESKTOP:LAUNCH_ERROR] error={error}\n"));
                            }
                        }
                        116 => {
                            if let Err(error) = session.terminal() {
                                gui::log(format_args!("[DESKTOP:LAUNCH_ERROR] error={error}\n"));
                            }
                        }
                        102 if test => session.probe(2, "red")?,
                        98 if test => session.probe(1, "blue")?,
                        _ => {}
                    }
                }
                if (1..=3).contains(&action) {
                    if let Err(error) = session.toggle(action as usize - 1) {
                        gui::log(format_args!("[DESKTOP:LAUNCH_ERROR] error={error}\n"));
                    }
                    render(&mut window, &session, None).map_err(gui_error)?;
                }
            }
        })();
        session.cleanup()?;
        match outcome {
            Ok(false) => return Ok(()),
            Ok(true) | Err(BROKEN_PIPE) if generation < RESTARTS => {
                rt::write(b"[DESKTOP:RESTART] unsaved_buffers_discarded=true\n");
            }
            Ok(true) | Err(BROKEN_PIPE) => return Err(BUSY),
            Err(error) => return Err(error),
        }
    }
    Err(BUSY)
}

fn control_check() {
    assert_eq!(rt::process::stop(1), Err(NO_CHILD));
    assert_eq!(rt::process::stop(0), Err(INVALID_ARGUMENT));
    let child = rt::spawn("desktop", "park").unwrap();
    assert!(rt::process::poll(child).unwrap().is_none());
    rt::process::stop(child).unwrap();
    // SAFETY: The experimental POLL contract rejects this unmapped output and retains status.
    assert_eq!(
        unsafe { rt::raw(process::SYSCALL, process::command(process::POLL), child, 0) },
        BAD_ADDRESS
    );
    let result = rt::process::poll(child).unwrap().unwrap();
    assert_eq!(result.pid, child);
    assert_eq!(
        result.status,
        ExitStatus {
            reason: 3,
            code: 15
        }
    );
    assert_eq!(rt::process::stop(child), Err(NO_CHILD));
    let replacement = rt::spawn("desktop", "park").unwrap();
    assert!(replacement > child);
    assert_eq!(rt::process::stop(child), Err(NO_CHILD));
    assert!(rt::process::poll(replacement).unwrap().is_none());
    stop(replacement).unwrap();
    checks::boundaries();
    rt::write(b"[DESKTOP:CONTROL_PASS] copy_failure_retained=true stale_pid_rejected=true\n");
}

#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(argc: usize, argv: *const *const u8) -> u32 {
    // SAFETY: Native startup supplies immutable bounded arguments for this image lifetime.
    let argument = unsafe { rt::Args::from_raw(argc, argv) }
        .nth(1)
        .unwrap_or(b"");
    if argument == b"park" {
        loop {
            rt::sleep(100).unwrap();
        }
    }
    if argument == b"check" {
        control_check();
        return 0;
    }
    match run(argument == b"test") {
        Ok(()) => 0,
        Err(error) => {
            gui::log(format_args!(
                "[DESKTOP:STOPPED] error={error} serial_recovery=true\n"
            ));
            1
        }
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    rt::write(b"[DESKTOP:FAIL]\n");
    rt::exit(101)
}
