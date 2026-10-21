// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Run the native serial line editor with bounded input, open/save and explicit dirty state.
//! This is not a full-screen or POSIX terminal editor; /tmp remains volatile until M7 disk mount.
use anova_rt::{self as rt, abi::*};
use editor::Text;

fn message(bytes: &[u8]) {
    let _ = rt::write_all(1, bytes);
}

fn read_line(buffer: &mut [u8; 256]) -> Result<Option<usize>, i64> {
    let mut n = 0;
    loop {
        let mut byte = [0];
        if rt::read(0, &mut byte)? == 0 {
            return Ok(None);
        }
        if byte[0] == b'\n' {
            return Ok(Some(n));
        }
        if n == buffer.len() {
            // Drain the oversized command instead of accidentally running its prefix.
            while rt::read(0, &mut byte)? != 0 {
                if byte[0] == b'\n' {
                    break;
                }
            }
            return Err(TOO_LONG);
        }
        buffer[n] = byte[0];
        n += 1;
    }
}

fn load(path: &str, text: &mut Text<8192>) -> Result<(), i64> {
    let fd = match rt::open(path, OPEN_READ) {
        Ok(fd) => fd,
        Err(NOT_FOUND) => return Ok(()),
        Err(error) => return Err(error),
    };
    let result = (|| {
        let mut chunk = [0; 512];
        loop {
            let n = rt::read(fd, &mut chunk)?;
            if n == 0 {
                return Ok(());
            }
            text.append(&chunk[..n]).map_err(|_| NO_SPACE)?;
        }
    })();
    let close = rt::close(fd);
    result?;
    close
}

fn save(path: &str, text: &Text<8192>) -> Result<(), i64> {
    let fd = rt::open(path, OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE)?;
    let result = rt::write_all(fd, text.bytes());
    let close = rt::close(fd);
    result?;
    close
}

fn edit(path: &str) -> u32 {
    let mut text = Text::<8192>::new();
    if load(path, &mut text).is_err() {
        message(b"ed: failed to read file (or file exceeds 8192 bytes)\n");
        return 1;
    }
    message(b"AnovaOS ed: p=print, a TEXT=append, r N TEXT=replace, d N=delete, w=save, q=quit, Q=discard, h=help\n");
    let mut dirty = false;
    let mut buffer = [0; 256];
    loop {
        message(b"ed> ");
        let n = match read_line(&mut buffer) {
            Ok(Some(n)) => n,
            Ok(None) => return if dirty { 1 } else { 0 },
            Err(TOO_LONG) => {
                message(b"ed: command exceeds 256 bytes\n");
                continue;
            }
            Err(_) => return 1,
        };
        let Ok(command) = core::str::from_utf8(&buffer[..n]) else {
            message(b"ed: invalid UTF-8\n");
            continue;
        };
        let changed = if command == "p" {
            message(text.bytes());
            if !text.bytes().ends_with(b"\n") {
                message(b"\n");
            }
            continue;
        } else if command == "w" {
            match save(path, &text) {
                Ok(()) => {
                    dirty = false;
                    message(b"ed: saved\n");
                }
                Err(_) => message(b"ed: save failed\n"),
            }
            continue;
        } else if command == "q" {
            if dirty {
                message(b"ed: unsaved changes; w to save or Q to discard\n");
                continue;
            }
            return 0;
        } else if command == "Q" {
            return 0;
        } else if command == "h" {
            message(b"ed PATH opens a file; append with a TEXT, replace with r LINE TEXT, delete with d LINE, w saves, q exits\n");
            continue;
        } else if let Some(line) = command.strip_prefix("a ") {
            text.append_line(line.as_bytes())
        } else if let Some(rest) = command.strip_prefix("d ") {
            match rest.parse::<usize>() {
                Ok(number) => text.delete(number),
                Err(_) => Err(editor::EditError::NoLine),
            }
        } else if let Some(rest) = command.strip_prefix("r ") {
            match rest.split_once(' ') {
                Some((number, line)) => match number.parse::<usize>() {
                    Ok(number) => text.replace(number, line.as_bytes()),
                    Err(_) => Err(editor::EditError::NoLine),
                },
                None => Err(editor::EditError::NoLine),
            }
        } else {
            message(b"ed: unknown command\n");
            continue;
        };
        match changed {
            Ok(()) => dirty = true,
            Err(_) => message(b"ed: invalid line or document full\n"),
        }
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(argc: usize, argv: *const *const u8) -> u32 {
    // SAFETY: The native ELF loader provides initialized, bounded argument strings.
    let mut args = unsafe { rt::Args::from_raw(argc, argv) };
    let _name = args.next();
    let path = args.next().and_then(|value| core::str::from_utf8(value).ok());
    match path {
        Some(path) if !path.is_empty() => edit(path),
        _ => {
            message(b"usage: ed PATH\n");
            2
        }
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    let _ = rt::write_all(2, b"[EDITOR:FAIL]\n");
    rt::exit(101)
}
