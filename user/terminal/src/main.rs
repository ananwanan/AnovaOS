// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Run a real Linux PTY shell through bounded I/O turns and an independently polled GUI.
//! Fixed cell/history/input storage prevents output-driven allocation. F5 replaces the shell;
//! F4 closes the terminal. COM1 carries diagnostics independently of the merged PTY stream.

mod checks;
mod window;

use anova_gui_runtime::{Error, GuiEvent, GuiRect};
use anova_gui_user as gui;
use anova_rt::{self as rt, abi::*, pty::Session};
use anova_terminal::{PALETTE, Screen};
use core::{
    cell::UnsafeCell,
    fmt::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
};
use window::Window;

struct Storage(UnsafeCell<Screen>);
// SAFETY: This native executable is single-threaded and takes STORAGE once. It never
// forks after entering run; no callback or foreign process receives its BSS pointer.
unsafe impl Sync for Storage {}
static CLAIMED: AtomicBool = AtomicBool::new(false);
static STORAGE: Storage = Storage(UnsafeCell::new(Screen::new()));

struct Input {
    bytes: [u8; 256],
    length: usize,
}

struct Label {
    bytes: [u8; 64],
    length: usize,
}
impl fmt::Write for Label {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.length.checked_add(value.len()).ok_or(fmt::Error)?;
        self.bytes
            .get_mut(self.length..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(value.as_bytes());
        self.length = end;
        Ok(())
    }
}
impl Input {
    fn push(&mut self, bytes: &[u8]) -> bool {
        if self.length + bytes.len() > self.bytes.len() {
            return false;
        }
        self.bytes[self.length..self.length + bytes.len()].copy_from_slice(bytes);
        self.length += bytes.len();
        true
    }
    fn flush(&mut self, session: &Session) -> Result<(), i64> {
        if self.length != 0 {
            match session.write(&self.bytes[..self.length]) {
                Ok(n) => {
                    self.bytes.copy_within(n..self.length, 0);
                    self.length -= n;
                }
                Err(WOULD_BLOCK) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

fn key(event: &GuiEvent, input: &mut Input) -> bool {
    if event.modifiers & 2 != 0 {
        let byte = match event.key {
            46 => 3,
            32 => 4,
            44 => 26,
            22 => 21,
            _ => return true,
        };
        // Cancellation must not sit behind a full local input queue. The line discipline
        // flushes unsubmitted input on Ctrl+C; drop queued bytes before submitting it.
        if byte == 3 {
            input.length = 0;
        }
        return input.push(&[byte]);
    }
    let bytes: &[u8] = match event.key {
        1 => b"\x1b",
        14 => b"\x7f",
        0x148 => b"\x1b[A",
        0x150 => b"\x1b[B",
        0x14d => b"\x1b[C",
        0x14b => b"\x1b[D",
        0x147 => b"\x1b[H",
        0x14f => b"\x1b[F",
        0x153 => b"\x1b[3~",
        _ if event.codepoint > 0 && event.codepoint < 128 => {
            return input.push(&[event.codepoint as u8]);
        }
        _ => return true,
    };
    input.push(bytes)
}

fn render(window: &mut Window, screen: &Screen, notice: &str) -> Result<(), Error> {
    window.clear(PALETTE[0])?;
    window.text(4, 16, notice, 0x88bbff)?;
    let (rows, columns) = screen.dimensions();
    for row in 0..rows {
        for column in 0..columns {
            let cell = screen.cell(row, column);
            let x = 4 + column as i32 * 6;
            let y = 24 + row as i32 * 8;
            if cell.background != 0 {
                window.rect(
                    GuiRect {
                        x,
                        y,
                        width: 6,
                        height: 8,
                    },
                    PALETTE[cell.background as usize],
                )?;
            }
            let byte = [cell.byte];
            window.text(
                x,
                y,
                core::str::from_utf8(&byte).unwrap(),
                PALETTE[cell.foreground as usize],
            )?;
        }
    }
    if let Some((row, column)) = screen.cursor() {
        window.rect(
            GuiRect {
                x: 4 + column as i32 * 6,
                y: 30 + row as i32 * 8,
                width: 5,
                height: 1,
            },
            PALETTE[7],
        )?;
    }
    window.begin();
    Ok(())
}

fn start(screen: &mut Screen) -> Option<Session> {
    let (rows, columns) = screen.dimensions();
    match Session::start(rows as u32, columns as u32) {
        Ok(session) => {
            gui::log(format_args!(
                "[TERMINAL:SHELL] session={} rows={rows} columns={columns}\n",
                session.id()
            ));
            Some(session)
        }
        Err(error) => {
            screen.feed(b"\r\n[shell start failed; F5 retries]\r\n");
            gui::log(format_args!(
                "[TERMINAL:START_ERROR] error={error} retry=F5\n"
            ));
            None
        }
    }
}

fn run() -> Result<(), Error> {
    assert!(!CLAIMED.swap(true, Ordering::AcqRel));
    // SAFETY: The single UI owner acquires this static once, never forks, and does
    // not expose aliases. The 44 KiB cell store remains outside the small user stack.
    let screen = unsafe { &mut *STORAGE.0.get() };
    screen.resize(26, 50);
    let mut window = Window::create()?;
    let mut session = start(screen);
    let mut input = Input {
        bytes: [0; 256],
        length: 0,
    };
    let mut dirty = true;
    let mut exited = false;
    let mut last_status = ExitStatus::default();
    let mut input_full = false;
    let mut eof = false;
    let mut overflow_reported = false;
    let mut output = [0; 1024];
    let mut turns = 0_u64;
    let mut history_dropped = 0;
    gui::log(format_args!(
        "[TERMINAL:READY] pid={} history=256 input=256 read_turn=4096 events_turn=16\n",
        rt::info().unwrap().process_id
    ));
    loop {
        turns += 1;
        // Drain a bounded number of events before every bounded PTY transfer batch.
        for _ in 0..16 {
            let event = match window.poll() {
                Ok(event) => event,
                Err(Error::WouldBlock) => break,
                Err(e) => return Err(e),
            };
            if event.kind == 1 {
                input.length = 0;
                input_full = false;
            }
            if event.kind == 3 && event.width != 0 && event.height != 0 {
                let rows = event.height.saturating_sub(24) as usize / 8;
                let columns = event.width.saturating_sub(8) as usize / 6;
                if screen.dimensions() != (rows.clamp(1, 26), columns.clamp(1, 52)) {
                    screen.resize(rows, columns);
                    let (rows, columns) = screen.dimensions();
                    if let Some(session) = &session {
                        session
                            .resize(rows as u32, columns as u32)
                            .map_err(|_| Error::Transport)?;
                    }
                    gui::log(format_args!(
                        "[TERMINAL:RESIZE] rows={rows} columns={columns} received={}\n",
                        screen.received
                    ));
                    dirty = true;
                }
            }
            if event.kind != 2 || !matches!(event.state, 1 | 3) {
                continue;
            }
            if event.key == 62 {
                window.close()?;
                return Ok(());
            }
            if event.key == 63 {
                drop(session.take());
                input.length = 0;
                input_full = false;
                exited = false;
                eof = false;
                screen.new_stream();
                screen.feed(b"\r\n[terminal: new shell]\r\n");
                session = start(screen);
                dirty = true;
                continue;
            }
            if event.modifiers & 1 != 0 && matches!(event.key, 0x149 | 0x151) {
                screen.scroll_view(if event.key == 0x149 { 10 } else { -10 });
                gui::log(format_args!(
                    "[TERMINAL:HISTORY] offset={} dropped={}\n",
                    screen.view_offset(),
                    screen.dropped()
                ));
                dirty = true;
                continue;
            }
            if !exited && session.is_some() {
                screen.live_view();
                input_full = !key(&event, &mut input);
                if input_full {
                    gui::log(format_args!("[TERMINAL:INPUT_FULL] capacity=256\n"));
                    screen.feed(b"\x07");
                }
                dirty = true;
            }
        }
        if let Some(session) = &session {
            if !exited {
                input.flush(session).map_err(|_| Error::Transport)?;
            }
            if !eof {
                for _ in 0..4 {
                    match session.read(&mut output) {
                        Ok(0) => {
                            eof = true;
                            dirty = true;
                            gui::log(format_args!("[TERMINAL:EOF] session={}\n", session.id()));
                            break;
                        }
                        Ok(n) => {
                            screen.feed(&output[..n]);
                            dirty = true;
                        }
                        Err(WOULD_BLOCK) => break,
                        Err(_) => return Err(Error::Transport),
                    }
                }
            }
            if !exited && let Some(status) = session.status().map_err(|_| Error::Transport)? {
                exited = true;
                last_status = status.status;
                input.length = 0;
                dirty = true;
                gui::log(format_args!(
                    "[TERMINAL:EXIT] session={} reason={} code={}\n",
                    session.id(),
                    status.status.reason,
                    status.status.code
                ));
            }
        }
        if screen.dropped() > history_dropped {
            history_dropped = screen.dropped();
            if !overflow_reported {
                overflow_reported = true;
                gui::log(format_args!(
                    "[TERMINAL:SCROLLBACK_FULL] lines=256 dropped={history_dropped}\n"
                ));
            }
        }
        if dirty && window.idle() {
            let mut label = Label {
                bytes: [0; 64],
                length: 0,
            };
            if exited {
                write!(
                    &mut label,
                    "Shell {} {}; F5 restart; F4 close",
                    match last_status.reason {
                        0 => "exit",
                        3 => "signal",
                        _ => "fault",
                    },
                    last_status.code
                )
                .map_err(|_| Error::Buffer)?;
            }
            let notice = if session.is_none() {
                "Shell unavailable; F5 retries; F4 closes"
            } else if exited {
                core::str::from_utf8(&label.bytes[..label.length]).map_err(|_| Error::Utf8)?
            } else if input_full {
                "Input full; Ctrl+C cancels; F4 closes"
            } else if screen.view_offset() != 0 {
                "Scrollback; Shift-PgDn returns; F4 closes"
            } else {
                "PTY: F5 shell; F4 close; Shift-PgUp history"
            };
            render(&mut window, screen, notice)?;
            dirty = false;
        }
        if window.advance()? {
            gui::log(format_args!(
                "[TERMINAL:PRESENT] turns={turns} received={} history={} dropped={}\n",
                screen.received,
                screen.history_len(),
                screen.dropped()
            ));
        }
        rt::sleep(1).map_err(|_| Error::Transport)?;
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(argc: usize, argv: *const *const u8) -> u32 {
    // SAFETY: Native startup supplies bounded immutable argument pointers for this image.
    let mode = unsafe { rt::Args::from_raw(argc, argv) }
        .nth(1)
        .unwrap_or(b"");
    if mode == b"check" {
        checks::run();
        return 0;
    }
    match run() {
        Ok(()) => {
            rt::write(b"[TERMINAL:DONE]\n");
            0
        }
        Err(e) => {
            gui::log(format_args!(
                "[TERMINAL:STOPPED] error={e:?} serial_recovery=true\n"
            ));
            1
        }
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    rt::write(b"[TERMINAL:FAIL]\n");
    rt::exit(101)
}
