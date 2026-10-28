// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Browse the shared work volume and edit bounded ASCII sources with explicit save/leave checks.
//! One acknowledged raster chunk per turn keeps physical keyboard input independently polled.

mod fs;
#[path = "../../terminal/src/window.rs"]
mod window;

use anova_editor::{Editor, Error as EditError, MAX_PATH};
use anova_gui_runtime::{Error, GuiEvent, GuiRect};
use anova_gui_user as gui;
use anova_rt::{self as rt, abi::*};
use core::{
    cell::UnsafeCell,
    fmt::Write,
    sync::atomic::{AtomicBool, Ordering},
};
use fs::NativeFs;
use window::Window;

const TEXT_Y: i32 = 56;
const VISIBLE_ROWS: usize = 16;

struct Storage(UnsafeCell<Editor>);
// SAFETY: The single native UI claims its process-private editor once and never forks.
unsafe impl Sync for Storage {}
static EDITOR: Storage = Storage(UnsafeCell::new(Editor::new()));
static CLAIMED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct Text {
    bytes: [u8; 128],
    len: usize,
}
impl Text {
    const fn new() -> Self {
        Self {
            bytes: [0; 128],
            len: 0,
        }
    }
    fn set(&mut self, text: &str) {
        self.len = text.len().min(128);
        self.bytes[..self.len].copy_from_slice(&text.as_bytes()[..self.len]);
    }
    fn text(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("?")
    }
}
impl Write for Text {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        let end = self.len.checked_add(text.len()).ok_or(core::fmt::Error)?;
        self.bytes
            .get_mut(self.len..end)
            .ok_or(core::fmt::Error)?
            .copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Action {
    Open(Text),
    New(Text),
    Rename(Text),
    Delete,
    Close,
}
#[derive(Clone, Copy, PartialEq)]
enum Prompt {
    None,
    Open,
    New,
    Rename,
    Mkdir,
}
#[derive(Clone, Copy, PartialEq)]
enum Dialog {
    None,
    Unsaved,
    Overwrite,
    Delete,
}

struct Ui {
    cwd: Text,
    entries: [DirEntry; 64],
    count: usize,
    selected: usize,
    browser: bool,
    prompt: Prompt,
    input: Text,
    notice: Text,
    dialog: Dialog,
    pending: Option<Action>,
}
impl Ui {
    fn new() -> Self {
        let mut value = Self {
            cwd: Text::new(),
            entries: [DirEntry::default(); 64],
            count: 0,
            selected: 0,
            browser: true,
            prompt: Prompt::None,
            input: Text::new(),
            notice: Text::new(),
            dialog: Dialog::None,
            pending: None,
        };
        value.cwd.set("/persist");
        value
    }
    fn error(&mut self, error: EditError, editor: &Editor) {
        self.notice = Text::new();
        let reason = match error {
            EditError::Io(NO_SPACE) => "Disk/inode space exhausted",
            EditError::Io(FILE_LIMIT) => "Descriptor quota exhausted",
            EditError::Io(READ_ONLY) => "Read-only storage",
            EditError::Io(IO_ERROR | DEVICE_TIMEOUT) => "Storage write/flush failed",
            EditError::Io(ALREADY_EXISTS) => "Target/recovery file exists",
            EditError::TooLong => "File/buffer limit: 16384 bytes",
            EditError::InvalidText => "Unsupported source bytes",
            _ => "Operation failed",
        };
        let _ = write!(&mut self.notice, "{reason}: {error:?}");
        gui::log(format_args!(
            "[FILES:ERROR] error={error:?} dirty={}\n",
            editor.dirty()
        ));
    }
    fn refresh(&mut self) -> Result<(), EditError> {
        let fd = rt::open(self.cwd.text(), OPEN_READ | OPEN_DIRECTORY).map_err(EditError::Io)?;
        self.count = 0;
        let result = (|| {
            while let Some(entry) = rt::readdir(fd).map_err(EditError::Io)? {
                if self.count == self.entries.len() {
                    self.notice
                        .set("Directory limited to 64; F1 opens exact paths");
                    break;
                }
                self.entries[self.count] = entry;
                self.count += 1;
            }
            Ok(())
        })();
        let close = rt::close(fd).map_err(EditError::Io);
        self.selected = self.selected.min(self.count.saturating_sub(1));
        result.and(close)?;
        gui::log(format_args!(
            "[FILES:BROWSE] cwd={} entries={}\n",
            self.cwd.text(),
            self.count
        ));
        Ok(())
    }
    fn path(&self, input: &str) -> Result<Text, EditError> {
        let mut path = Text::new();
        if !input.starts_with('/') {
            write!(&mut path, "{}/", self.cwd.text()).map_err(|_| EditError::InvalidPath)?;
        }
        path.write_str(input).map_err(|_| EditError::InvalidPath)?;
        let text = path.text();
        if text.len() > MAX_PATH
            || !(text == "/persist" || text.starts_with("/persist/"))
            || text[1..]
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == ".." || part.len() > 64)
            || !text.bytes().all(|b| (32..=126).contains(&b))
        {
            return Err(EditError::InvalidPath);
        }
        Ok(path)
    }
    fn perform(
        &mut self,
        action: Action,
        editor: &mut Editor,
        discard: bool,
    ) -> Result<bool, EditError> {
        let result = match action {
            Action::Open(path) => {
                if let Ok(fd) = rt::open(path.text(), OPEN_READ | OPEN_DIRECTORY) {
                    rt::close(fd).map_err(EditError::Io)?;
                    self.cwd = path;
                    self.browser = true;
                    self.refresh()?;
                    gui::log(format_args!("[FILES:DIRECTORY] path={}\n", self.cwd.text()));
                    return Ok(false);
                }
                editor.open(&mut NativeFs, path.text(), discard).map(|_| {
                    self.browser = false;
                    gui::log(format_args!(
                        "[FILES:OPEN] path={} bytes={}\n",
                        editor.path(),
                        editor.bytes().len()
                    ));
                })
            }
            Action::New(path) => editor.create(path.text(), discard).map(|_| {
                self.browser = false;
                gui::log(format_args!("[FILES:NEW] path={}\n", editor.path()));
            }),
            Action::Rename(path) => editor.rename(&mut NativeFs, path.text(), discard).map(|_| {
                gui::log(format_args!("[FILES:RENAMED] path={}\n", editor.path()));
            }),
            Action::Delete => {
                let mut path = Text::new();
                path.set(editor.path());
                editor.delete_file(&mut NativeFs, discard).map(|_| {
                    self.browser = true;
                    gui::log(format_args!("[FILES:DELETED] path={}\n", path.text()));
                })
            }
            Action::Close => editor.close(discard),
        };
        match result {
            Err(EditError::Unsaved) => {
                self.pending = Some(action);
                self.dialog = Dialog::Unsaved;
                gui::log(format_args!("[FILES:UNSAVED] path={}\n", editor.path()));
                Ok(false)
            }
            Err(error) => Err(error),
            Ok(()) => {
                self.notice.set("Ready");
                if !matches!(action, Action::Close) {
                    self.refresh()?;
                }
                Ok(matches!(action, Action::Close))
            }
        }
    }
    fn save(&mut self, editor: &mut Editor, overwrite: bool) -> Result<bool, EditError> {
        match editor.save(&mut NativeFs, overwrite) {
            Ok(outcome) => {
                self.dialog = Dialog::None;
                self.notice.set(if outcome.backup_retained {
                    "Saved; backup retained: cleanup failed"
                } else {
                    "Saved: file sync + publication + volume sync"
                });
                gui::log(format_args!(
                    "[FILES:SAVED] path={} bytes={} dirty={}\n",
                    editor.path(),
                    editor.bytes().len(),
                    editor.dirty()
                ));
                if let Some(action) = self.pending.take() {
                    return self.perform(action, editor, true);
                }
                self.refresh()?;
                Ok(false)
            }
            Err(EditError::Conflict) => {
                self.dialog = Dialog::Overwrite;
                gui::log(format_args!("[FILES:CONFLICT] path={}\n", editor.path()));
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }
    fn event(&mut self, event: &GuiEvent, editor: &mut Editor) -> Result<bool, EditError> {
        if event.kind != 2 || !matches!(event.state, 1 | 3) {
            return Ok(false);
        }
        if event.key == 1 {
            self.prompt = Prompt::None;
            self.dialog = Dialog::None;
            self.pending = None;
            return Ok(false);
        }
        if self.dialog != Dialog::None {
            return match (self.dialog, event.codepoint) {
                (Dialog::Unsaved, 115) => self.save(editor, false),
                (Dialog::Unsaved, 100) => {
                    self.dialog = Dialog::None;
                    let action = self.pending.take().ok_or(EditError::NoFile)?;
                    self.perform(action, editor, true)
                }
                (Dialog::Overwrite, 121) => self.save(editor, true),
                (Dialog::Delete, 121) => {
                    self.dialog = Dialog::None;
                    self.perform(Action::Delete, editor, false)
                }
                _ => Ok(false),
            };
        }
        if self.prompt != Prompt::None {
            match event.codepoint {
                8 => self.input.len = self.input.len.saturating_sub(1),
                10 => {
                    let path = self.path(self.input.text())?;
                    let prompt = core::mem::replace(&mut self.prompt, Prompt::None);
                    let action = match prompt {
                        Prompt::Open => Action::Open(path),
                        Prompt::New => Action::New(path),
                        Prompt::Rename => Action::Rename(path),
                        Prompt::Mkdir => {
                            rt::mkdir(path.text()).map_err(EditError::Io)?;
                            rt::sync().map_err(EditError::Io)?;
                            self.refresh()?;
                            gui::log(format_args!("[FILES:MKDIR] path={}\n", path.text()));
                            return Ok(false);
                        }
                        Prompt::None => return Ok(false),
                    };
                    return self.perform(action, editor, false);
                }
                32..=126 if self.input.len < MAX_PATH => {
                    self.input.bytes[self.input.len] = event.codepoint as u8;
                    self.input.len += 1;
                }
                32..=126 => return Err(EditError::InvalidPath),
                _ => {}
            }
            return Ok(false);
        }
        match event.key {
            61 if self.browser => self.notice.set("Open the file before renaming it"),
            59 | 60 | 61 | 66 => {
                self.prompt = match event.key {
                    59 => Prompt::Open,
                    60 => Prompt::New,
                    61 => Prompt::Rename,
                    _ => Prompt::Mkdir,
                };
                self.input = Text::new();
            }
            62 => return self.perform(Action::Close, editor, false),
            63 => {
                self.pending = None;
                return self.save(editor, false);
            }
            64 if editor.is_open() && !self.browser => self.dialog = Dialog::Delete,
            64 => self.notice.set("Open the file before deleting it"),
            65 => {
                self.browser = true;
                self.refresh()?;
            }
            31 if event.modifiers & 2 != 0 => {
                self.pending = None;
                return self.save(editor, false);
            }
            _ if self.browser => match event.key {
                0x148 => self.selected = self.selected.saturating_sub(1),
                0x150 if self.count > 0 => self.selected = (self.selected + 1).min(self.count - 1),
                28 if self.count > 0 => {
                    let entry = self.entries[self.selected];
                    let name = core::str::from_utf8(&entry.name[..entry.name_len as usize])
                        .map_err(|_| EditError::InvalidPath)?;
                    return self.perform(Action::Open(self.path(name)?), editor, false);
                }
                14 if self.cwd.text() != "/persist" => {
                    let end = self.cwd.text().rfind('/').unwrap_or(8);
                    self.cwd.len = end.max(8);
                    self.refresh()?;
                }
                _ => {}
            },
            _ if editor.is_open() => match event.key {
                0x14b => editor.move_left(),
                0x14d => editor.move_right(),
                0x148 => editor.move_up(),
                0x150 => editor.move_down(),
                0x147 => editor.move_home(),
                0x14f => editor.move_end(),
                0x153 => {
                    editor.delete();
                }
                14 => {
                    editor.backspace();
                }
                _ if matches!(event.codepoint, 9 | 10 | 32..=126) => {
                    editor.insert(&[event.codepoint as u8])?
                }
                _ => {}
            },
            _ => {}
        }
        Ok(false)
    }
    fn render(&self, window: &mut Window, editor: &Editor) -> Result<(), Error> {
        window.clear(0x17232f)?;
        window.text(4, 18, "F1 open F2 new F3 rename F4 close F5 save", 0xffffff)?;
        window.text(4, 28, "F6 delete F7 files F8 mkdir; Esc cancel", 0xffffff)?;
        let mut title = Text::new();
        let _ = write!(
            &mut title,
            "{} {}",
            if editor.dirty() { "*" } else { " " },
            if self.browser {
                self.cwd.text()
            } else {
                editor.path()
            }
        );
        window.text(4, 40, title.text(), 0x60d0ff)?;
        if self.browser {
            let start = self.selected.saturating_sub(VISIBLE_ROWS - 1);
            for (row, entry) in self.entries[start..self.count]
                .iter()
                .take(VISIBLE_ROWS)
                .enumerate()
            {
                let mut label = Text::new();
                let name =
                    core::str::from_utf8(&entry.name[..entry.name_len as usize]).unwrap_or("?");
                let _ = write!(
                    &mut label,
                    "{} {}{}",
                    if start + row == self.selected {
                        ">"
                    } else {
                        " "
                    },
                    name,
                    if entry.kind == 2 { "/" } else { "" }
                );
                window.text(4, TEXT_Y + row as i32 * 8, label.text(), 0xffffff)?;
            }
        } else {
            let bytes = editor.bytes();
            let cursor = editor.cursor();
            let line = bytes[..cursor].iter().filter(|b| **b == b'\n').count();
            let line_start = bytes[..cursor]
                .iter()
                .rposition(|b| *b == b'\n')
                .map_or(0, |p| p + 1);
            let column = cursor - line_start;
            let first = line.saturating_sub(VISIBLE_ROWS - 1);
            let left = column.saturating_sub(49);
            let mut row = 0;
            let mut col = 0;
            for &byte in bytes {
                if row >= first
                    && row < first + VISIBLE_ROWS
                    && col >= left
                    && col < left + 50
                    && byte != b'\n'
                    && byte != b'\r'
                {
                    let glyph = [if byte == b'\t' { b' ' } else { byte }];
                    window.text(
                        4 + (col - left) as i32 * 6,
                        TEXT_Y + (row - first) as i32 * 8,
                        core::str::from_utf8(&glyph).unwrap(),
                        0xffffff,
                    )?;
                }
                if byte == b'\n' {
                    row += 1;
                    col = 0;
                } else {
                    col += 1;
                }
            }
            window.rect(
                GuiRect {
                    x: 4 + (column - left) as i32 * 6,
                    y: TEXT_Y + 7 + (line - first) as i32 * 8,
                    width: 5,
                    height: 1,
                },
                0x60d0ff,
            )?;
        }
        window.text(4, 188, self.notice.text(), 0xffc060)?;
        if self.prompt != Prompt::None {
            let label = match self.prompt {
                Prompt::Open => "Open file/directory:",
                Prompt::New => "New file:",
                Prompt::Rename => "Rename to:",
                _ => "New directory:",
            };
            window.text(4, 202, label, 0x60ff90)?;
            let begin = self.input.len.saturating_sub(50);
            window.text(
                4,
                214,
                core::str::from_utf8(&self.input.bytes[begin..self.input.len]).unwrap(),
                0xffffff,
            )?;
        } else {
            let label = match self.dialog {
                Dialog::Unsaved => "Unsaved: s save / d discard / Esc cancel",
                Dialog::Overwrite => "File changed/exists: y overwrite / Esc cancel",
                Dialog::Delete => "Delete file? y confirms / Esc cancels",
                Dialog::None => "Arrows navigate/edit; Enter opens; * unsaved",
            };
            window.text(4, 210, label, 0x60ff90)?;
        }
        window.begin();
        Ok(())
    }
}

fn run() -> Result<(), Error> {
    assert!(!CLAIMED.swap(true, Ordering::AcqRel));
    // SAFETY: CLAIMED grants unique access to the process-private editor for this UI lifetime.
    let editor = unsafe { &mut *EDITOR.0.get() };
    let mut ui = Ui::new();
    if let Err(error) = ui.refresh() {
        ui.error(error, editor);
    }
    let mut window = Window::create_at(270, 250, 318, 240)?;
    gui::log(format_args!(
        "[FILES:READY] cwd=/persist limit=16384 entries=64\n"
    ));
    let mut dirty = true;
    loop {
        for _ in 0..16 {
            let event = match window.poll() {
                Ok(event) => event,
                Err(Error::WouldBlock) => break,
                Err(error) => return Err(error),
            };
            if event.kind == 2 && matches!(event.state, 1 | 3) {
                match ui.event(&event, editor) {
                    Ok(true) => {
                        window.close()?;
                        return Ok(());
                    }
                    Ok(false) => {}
                    Err(error) => ui.error(error, editor),
                }
                dirty = true;
            }
            if event.kind == 3 {
                dirty = true;
            }
        }
        if dirty && window.idle() {
            ui.render(&mut window, editor)?;
            dirty = false;
        }
        if window.advance()? {
            gui::log(format_args!(
                "[FILES:PRESENT] mode={} dirty={}\n",
                if ui.browser { "browser" } else { "editor" },
                editor.dirty()
            ));
        }
        rt::sleep(1).map_err(|_| Error::Transport)?;
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(_: usize, _: *const *const u8) -> u32 {
    match run() {
        Ok(()) => {
            rt::write(b"[FILES:DONE]\n");
            0
        }
        Err(error) => {
            gui::log(format_args!(
                "[FILES:STOPPED] error={error:?} serial_recovery=true\n"
            ));
            1
        }
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    rt::write(b"[FILES:FAIL]\n");
    rt::exit(101)
}
