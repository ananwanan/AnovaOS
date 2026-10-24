// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Independently compile a safe Rust GUI client; the separate link anchor owns startup and panic.
use anova_gui_runtime::{Error, GuiEvent, GuiRect, GuiWindowDesc, Window};

fn log(bytes: &[u8]) {
    // SAFETY: Native write borrows a valid immutable byte slice only for the synchronous call.
    unsafe {
        core::arch::asm!("syscall", inlateout("rax") 0u64 => _, in("rdi") 1u64,
            in("rsi") bytes.as_ptr(), in("rdx") bytes.len(), lateout("rcx") _,
            lateout("r11") _, options(nostack));
    }
}

fn sleep() {
    // SAFETY: Native sleep takes one scalar tick and no pointers or borrowed objects.
    unsafe {
        core::arch::asm!("syscall", inlateout("rax") 4u64 => _, in("rdi") 1u64,
            in("rsi") 0u64, in("rdx") 0u64, lateout("rcx") _, lateout("r11") _,
            options(nostack));
    }
}

fn contract() -> bool {
    use core::mem::{align_of, offset_of, size_of};
    if size_of::<anova_gui_runtime::GuiVersion>() != 32
        || size_of::<GuiWindowDesc>() != 32
        || size_of::<GuiRect>() != 16
        || size_of::<GuiEvent>() != 48
        || size_of::<anova_gui_runtime::GuiLayout>() != 28
        || align_of::<GuiEvent>() != 4
        || offset_of!(GuiEvent, x) != 16
        || offset_of!(GuiEvent, state) != 40
        || offset_of!(GuiWindowDesc, width) != 16
        || offset_of!(anova_gui_runtime::GuiLayout, cursor) != 16
    {
        return false;
    }
    if !matches!(
        anova_gui_runtime::query_version(0x10001),
        Err(Error::Version)
    ) {
        return false;
    }
    let Ok(version) = anova_gui_runtime::query_version(0x10000) else {
        return false;
    };
    if version.abi_version != 0x10000 || version.struct_size != 32 || version.event_size != 48 {
        return false;
    }
    let mut description = GuiWindowDesc::new(80, 80, 240, 160);
    description.flags = 0;
    let Ok(mut window) = Window::create(description) else {
        return false;
    };
    if !matches!(Window::create(description), Err(Error::Limit)) {
        return false;
    }
    if window.clear(0xff000000) != Err(Error::Invalid)
        || window.close().is_err()
        || window.close() != Err(Error::Stale)
    {
        return false;
    }
    if anova_gui_runtime::clipboard_set("clip\u{2603}").is_err() {
        return false;
    }
    let mut small = [0u8; 1];
    if anova_gui_runtime::clipboard_get(&mut small) != Err(Error::Buffer) {
        return false;
    }
    let mut copy = [0u8; 16];
    let Ok(length) = anova_gui_runtime::clipboard_get(&mut copy) else {
        return false;
    };
    length == 7 && &copy[..length] == "clip\u{2603}".as_bytes()
}

fn render(
    window: &mut Window,
    text: &mut [u8],
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
    window.text(12, 24, "ANOVA GUI", 0xffffff)?;
    window.text(12, 64, "C / C++ / Rust parity", 0xffffff)?;
    window.textfield(
        GuiRect {
            x: 12,
            y: 84,
            width: 216,
            height: 24,
        },
        text,
        length,
        event,
    )?;
    let activated = window.button(
        GuiRect {
            x: 12,
            y: 116,
            width: 100,
            height: 28,
        },
        "Click",
        event,
    )?;
    match window.present() {
        Ok(()) | Err(Error::WouldBlock) => {}
        Err(error) => return Err(error),
    }
    Ok(activated)
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    if !contract() {
        return 20;
    }
    log(b"[GUI:RUST:CONTRACT] PASS\n");
    let Ok(mut window) = Window::create(GuiWindowDesc::new(80, 80, 240, 160)) else {
        return 1;
    };
    let mut text = [0u8; 64];
    let mut length = 0;
    let mut saw_key = false;
    let mut saw_pointer = false;
    if render(&mut window, &mut text, &mut length, None).is_err() {
        return 2;
    }
    log(b"[GUI:RUST:READY]\n");
    loop {
        let event = match window.poll() {
            Ok(event) => event,
            Err(Error::WouldBlock) => {
                sleep();
                continue;
            }
            Err(_) => return 3,
        };
        if event.kind == 2
            && event.state == 1
            && event.codepoint == u32::from(b'q')
            && saw_key
            && saw_pointer
        {
            break;
        }
        let Ok(activated) = render(&mut window, &mut text, &mut length, Some(&event)) else {
            return 4;
        };
        if !saw_key
            && event.kind == 2
            && event.state == 1
            && event.codepoint == u32::from(b'a')
            && length == 1
            && text[0] == b'a'
        {
            saw_key = true;
            log(b"[GUI:RUST:KEY] codepoint=97 text=a\n");
        }
        if !saw_pointer && activated {
            saw_pointer = true;
            log(b"[GUI:RUST:POINTER] activated=true\n");
        }
    }
    if window.close().is_err() {
        return 5;
    }
    log(b"[GUI:RUST:DONE]\n");
    0
}
