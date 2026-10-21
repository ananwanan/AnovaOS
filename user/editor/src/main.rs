// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Edit bounded ASCII source files through canonical serial lines and explicit fsync on save.
//! Commands append, replace/delete one line, print, save or quit; no raw terminal or allocator.
use anova_rt::{self as rt, abi::*};
const LIMIT: usize = 16 * 1024;
fn line(bytes: &mut [u8; 256]) -> Result<usize, i64> {
    let mut n = 0;
    loop {
        let mut byte = [0];
        if rt::read(0, &mut byte)? == 0 {
            return Err(NOT_FOUND);
        }
        if byte[0] == b'\n' {
            return Ok(n);
        }
        if n == bytes.len() - 1 {
            return Err(TOO_LONG);
        }
        bytes[n] = byte[0];
        n += 1;
    }
}
fn bounds(text: &[u8], number: usize) -> Result<(usize, usize), i64> {
    if number == 0 {
        return Err(INVALID_ARGUMENT);
    }
    let mut start = 0;
    let mut current = 1;
    while start < text.len() {
        let end = text[start..]
            .iter()
            .position(|b| *b == b'\n')
            .map_or(text.len(), |n| start + n + 1);
        if current == number {
            return Ok((start, end));
        }
        current += 1;
        start = end;
    }
    Err(INVALID_ARGUMENT)
}
fn save(path: &str, text: &[u8]) -> Result<(), i64> {
    let fd = rt::open(path, OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE)?;
    let result = rt::write_all(fd, text).and_then(|_| rt::fsync(fd));
    let close = rt::close(fd);
    result.and(close)
}
fn edit(path: &str) -> Result<u32, i64> {
    if path.is_empty() {
        return Err(INVALID_ARGUMENT);
    }
    let mut text = [0; LIMIT];
    let mut length = 0;
    match rt::open(path, OPEN_READ) {
        Ok(fd) => {
            let result = (|| -> Result<(), i64> {
                loop {
                    if length == LIMIT {
                        let mut extra = [0];
                        if rt::read(fd, &mut extra)? != 0 {
                            return Err(TOO_LONG);
                        }
                        break;
                    }
                    let end = (length + 4096).min(LIMIT);
                    let n = rt::read(fd, &mut text[length..end])?;
                    if n == 0 {
                        break;
                    }
                    length += n;
                }
                if text[..length]
                    .iter()
                    .any(|b| !matches!(*b, b'\t' | b'\r' | b'\n' | 32..=126))
                {
                    return Err(INVALID_ARGUMENT);
                }
                Ok(())
            })();
            rt::close(fd)?;
            result?;
        }
        Err(NOT_FOUND) => {}
        Err(e) => return Err(e),
    }
    rt::write_all(
        1,
        b"AnovaOS line editor: p, a (. ends), d N, c N, w, q, q!\n",
    )?;
    let mut dirty = false;
    loop {
        rt::write_all(1, b"edit> ")?;
        let mut command = [0; 256];
        let n = line(&mut command)?;
        let command = core::str::from_utf8(&command[..n]).map_err(|_| INVALID_ARGUMENT)?;
        let result = (|| -> Result<bool, i64> {
            match command {
                "p" => rt::write_all(1, &text[..length])?,
                "a" => loop {
                    let mut bytes = [0; 256];
                    let n = line(&mut bytes)?;
                    if &bytes[..n] == b"." {
                        break;
                    }
                    if length + n + 1 > LIMIT {
                        return Err(NO_SPACE);
                    }
                    text[length..length + n].copy_from_slice(&bytes[..n]);
                    text[length + n] = b'\n';
                    length += n + 1;
                    dirty = true;
                },
                "w" => {
                    save(path, &text[..length])?;
                    dirty = false;
                    rt::write_all(1, b"[EDIT:SAVED] fsync=ok\n")?;
                }
                "q" if dirty => {
                    rt::write_all(2, b"Unsaved changes; use w or q!\n")?;
                }
                "q" | "q!" => return Ok(true),
                _ => {
                    let (op, number) = command.split_once(' ').ok_or(INVALID_ARGUMENT)?;
                    let number = number.parse::<usize>().map_err(|_| INVALID_ARGUMENT)?;
                    let (start, end) = bounds(&text[..length], number)?;
                    let mut replacement = [0; 256];
                    let count = match op {
                        "d" => 0,
                        "c" => {
                            let n = line(&mut replacement)?;
                            replacement[n] = b'\n';
                            n + 1
                        }
                        _ => return Err(INVALID_ARGUMENT),
                    };
                    let new_length = length - (end - start) + count;
                    if new_length > LIMIT {
                        return Err(NO_SPACE);
                    }
                    text.copy_within(end..length, start + count);
                    text[start..start + count].copy_from_slice(&replacement[..count]);
                    length = new_length;
                    dirty = true;
                }
            }
            Ok(false)
        })();
        match result {
            Ok(true) => return Ok(0),
            Ok(false) => {}
            Err(_) => rt::write_all(2, b"edit: command/save failed; buffer retained\n")?,
        }
    }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(argc: usize, argv: *const *const u8) -> u32 {
    // SAFETY: Loader supplies immutable, bounded argument strings for this process lifetime.
    let mut args = unsafe { rt::Args::from_raw(argc, argv) };
    let _ = args.next();
    let path = core::str::from_utf8(args.next().unwrap_or(b"")).unwrap_or("");
    match edit(path) {
        Ok(code) => code,
        Err(_) => {
            rt::write(b"edit: open/input failed\n");
            1
        }
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    rt::write(b"[EDIT:FAIL]\n");
    rt::exit(101)
}
