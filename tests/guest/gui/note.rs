// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Edit a bounded native desktop note and explicitly copy it through the process-local clipboard.
use anova_gui_runtime::{Error, GuiEvent, GuiRect, GuiWindowDesc, Window};

fn native(operation: u64, a: u64, b: u64, c: u64) -> i64 {
    let result: i64;
    // SAFETY: Callers pass the frozen native v1 arguments; borrowed slices outlive each call.
    unsafe {
        core::arch::asm!("syscall", inlateout("rax") operation => result, in("rdi") a,
            in("rsi") b, in("rdx") c, lateout("rcx") _, lateout("r11") _, options(nostack));
    }
    result
}

fn log(bytes: &[u8]) {
    let _ = native(0, 1, bytes.as_ptr() as u64, bytes.len() as u64);
}

fn render(
    window: &mut Window,
    note: &mut [u8],
    length: &mut u32,
    event: Option<&GuiEvent>,
) -> Result<bool, Error> {
    window.clear(0x182838)?;
    window.rect(
        GuiRect {
            x: 12,
            y: 44,
            width: 216,
            height: 12,
        },
        0x3070b0,
    )?;
    window.text(12, 24, "ANOVA NOTE", 0xffffff)?;
    window.text(12, 64, "Type a; Copy; q closes", 0xffffff)?;
    window.textfield(
        GuiRect {
            x: 12,
            y: 84,
            width: 216,
            height: 24,
        },
        note,
        length,
        event,
    )?;
    let copied = window.button(
        GuiRect {
            x: 12,
            y: 116,
            width: 100,
            height: 28,
        },
        "Copy",
        event,
    )?;
    if copied {
        let text = core::str::from_utf8(&note[..*length as usize]).map_err(|_| Error::Utf8)?;
        anova_gui_runtime::clipboard_set(text)?;
        let mut clipboard = [0u8; 64];
        let written = anova_gui_runtime::clipboard_get(&mut clipboard)?;
        if written != *length as usize || clipboard[..written] != note[..written] {
            return Err(Error::Buffer);
        }
    }
    match window.present() {
        Ok(()) | Err(Error::WouldBlock) => {}
        Err(error) => return Err(error),
    }
    Ok(copied)
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    let Ok(mut window) = Window::create(GuiWindowDesc::new(80, 80, 240, 160)) else {
        return 1;
    };
    let mut note = [0u8; 64];
    note[..4].copy_from_slice(b"note");
    let mut length = 4;
    let mut saw_key = false;
    let mut saw_copy = false;
    if render(&mut window, &mut note, &mut length, None).is_err() {
        return 2;
    }
    log(b"[GUI:NOTE:READY]\n");
    loop {
        let event = match window.poll() {
            Ok(event) => event,
            Err(Error::WouldBlock) => {
                let _ = native(4, 1, 0, 0);
                continue;
            }
            Err(_) => return 3,
        };
        if event.kind == 2
            && event.state == 1
            && event.codepoint == u32::from(b'q')
            && saw_key
            && saw_copy
        {
            break;
        }
        let Ok(copied) = render(&mut window, &mut note, &mut length, Some(&event)) else {
            return 4;
        };
        if !saw_key
            && event.kind == 2
            && event.state == 1
            && event.codepoint == u32::from(b'a')
            && length == 5
            && &note[..5] == b"notea"
        {
            saw_key = true;
            log(b"[GUI:NOTE:KEY] codepoint=97 text=notea\n");
        }
        if !saw_copy && copied && saw_key {
            saw_copy = true;
            log(b"[GUI:NOTE:POINTER] activated=true clipboard=notea\n");
        }
    }
    if window.close().is_err() {
        return 5;
    }
    log(b"[GUI:NOTE:DONE]\n");
    0
}
