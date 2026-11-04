// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Supervise a compositor and three admitted GUI clients alongside an independent COM1 shell.
//! Bound restarts, reap owned children and protect the files client's explicit close dialog.
//! Pump controlled build diagnostics and acknowledged GUI uploads in bounded turns.

mod build_checks;
mod checks;
mod diagnostics;
#[path = "../../terminal/src/window.rs"]
mod window;

use anova_gui_runtime::{Error, GuiRect};
use anova_gui_user as gui;
use anova_rt::{self as rt, abi::*};
use window::Window;

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

fn render(window: &mut Window, session: &Session) -> Result<(), Error> {
    window.clear(0x182838)?;
    window.text(12, 24, "ANOVA DESKTOP", 0xffffff)?;
    window.text(12, 44, "1/2/3 launch or stop; r restart", 0xffffff)?;
    window.text(
        12,
        62,
        if session.files_running {
            "Close Files with F4 before stop/restart/exit"
        } else {
            "f files; t terminal; b builds; q exit"
        },
        0xffffff,
    )?;
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
        window.rect(
            GuiRect {
                x: 160,
                y,
                width: 140,
                height: 24,
            },
            0x304860,
        )?;
        window.text(
            164,
            y + 5,
            if session.clients[index].is_some() {
                "Stop"
            } else {
                "Launch"
            },
            0xffffff,
        )?;
    }
    Ok(())
}

fn gui_error(error: Error) -> i64 {
    match error {
        Error::Disconnected => BROKEN_PIPE,
        _ => IO_ERROR,
    }
}

fn run(test: bool) -> Result<(), i64> {
    let mut diagnostics = diagnostics::Diagnostics::new();
    for generation in 0..=RESTARTS {
        let mut session = Session {
            server: rt::spawn("compositor", if test { "desktop-test" } else { "desktop" })?,
            clients: [None; 3],
            terminal_running: false,
            files_running: false,
        };
        let outcome = (|| -> Result<bool, i64> {
            // Drop the launcher window before replacing its compositor/session.
            let mut window = Window::create_at(20, 20, 320, 200).map_err(gui_error)?;
            let identity = window.session;
            for index in 0..3 {
                session.toggle(index)?;
            }
            let mut dirty = true;
            gui::log(format_args!(
                "[DESKTOP:READY] generation={generation} session={identity} clients=3 quota=4\n"
            ));
            loop {
                let old_clients = session.clients;
                if !session.observe()? {
                    return Ok(true);
                }
                dirty |= old_clients != session.clients;
                for _ in 0..16 {
                    let event = match window.poll() {
                        Ok(event) => event,
                        Err(Error::WouldBlock) => {
                            break;
                        }
                        Err(Error::Disconnected) => return Ok(true),
                        Err(_) => return Err(IO_ERROR),
                    };
                    dirty |= event.kind != 0;
                    if diagnostics.visible {
                        diagnostics.event(&event);
                        continue;
                    }
                    let mut action = 0;
                    if event.kind == 3
                        && event.changed & event.buttons & 1 != 0
                        && (160..300).contains(&event.x)
                    {
                        for index in 0..3 {
                            if (88 + index * 30..112 + index * 30).contains(&event.y) {
                                action = index as u32 + 1;
                            }
                        }
                    }
                    if event.kind == 2 && event.state == 1 {
                        match event.codepoint {
                            49..=51 => action = event.codepoint - 48,
                            114 if !session.files_running => return Ok(true),
                            113 if !session.files_running => return Ok(false),
                            102 if !test => {
                                if let Err(error) = session.files() {
                                    gui::log(format_args!(
                                        "[DESKTOP:LAUNCH_ERROR] error={error}\n"
                                    ));
                                }
                            }
                            116 => {
                                if let Err(error) = session.terminal() {
                                    gui::log(format_args!(
                                        "[DESKTOP:LAUNCH_ERROR] error={error}\n"
                                    ));
                                }
                            }
                            102 if test => session.probe(2, "red")?,
                            98 if test => session.probe(1, "blue")?,
                            98 => diagnostics.visible = true,
                            _ => {}
                        }
                    }
                    if (1..=3).contains(&action)
                        && let Err(error) = session.toggle(action as usize - 1)
                    {
                        gui::log(format_args!("[DESKTOP:LAUNCH_ERROR] error={error}\n"));
                    }
                }
                dirty |= diagnostics.turn();
                if dirty && window.idle() {
                    if diagnostics.visible {
                        diagnostics.render(&mut window)
                    } else {
                        render(&mut window, &session)
                    }
                    .map_err(gui_error)?;
                    window.begin();
                    dirty = false;
                }
                window.advance().map_err(gui_error)?;
                rt::sleep(1)?;
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
    if argument == b"build-check" {
        build_checks::run();
        return 0;
    }
    if argument == b"connect-full" {
        assert_eq!(gui::connect(), Err(NO_SPACE));
        rt::write(b"[DESKTOP:CONNECT_FULL_PASS] quota=4 error_retained=true\n");
        return 0;
    }
    if argument == b"connect-release" {
        gui::connect().unwrap();
        gui::scalar(anova_gui::transport::CLOSE).unwrap();
        gui::log(format_args!(
            "[DESKTOP:CONNECT_RELEASE_PASS] pid={}\n",
            rt::info().unwrap().process_id
        ));
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
