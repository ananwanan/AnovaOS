// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Execute bounded graphical shell commands through a pre-GUI native helper and real pipes.
use anova_gui_runtime::{Error, GuiEvent, GuiRect, GuiWindowDesc, Window};

const CAPACITY: usize = 128;

#[repr(C)]
struct Exec {
    name: u64,
    name_len: u64,
    argument: u64,
    argument_len: u64,
}

#[repr(C)]
#[derive(Default)]
struct Wait {
    pid: u64,
    reason: u32,
    code: u32,
}

struct Helper {
    pid: u64,
    request: u64,
    response: u64,
}

fn native(operation: u64, a: u64, b: u64, c: u64) -> i64 {
    let result: i64;
    // SAFETY: Callers provide frozen native v1 scalars and live, aligned, bounded buffers.
    unsafe {
        core::arch::asm!("syscall", inlateout("rax") operation => result, in("rdi") a,
            in("rsi") b, in("rdx") c, lateout("rcx") _, lateout("r11") _, options(nostack));
    }
    result
}

fn log(bytes: &[u8]) {
    let _ = write_all(1, bytes);
}
fn close(fd: u64) {
    let _ = native(9, fd, 0, 0);
}

fn exit(code: u64) -> ! {
    let _ = native(1, code, 0, 0);
    loop {
        core::hint::spin_loop();
    }
}

fn write_all(fd: u64, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        let count = native(0, fd, bytes.as_ptr() as u64, bytes.len() as u64);
        if count <= 0 || count as usize > bytes.len() {
            return false;
        }
        bytes = &bytes[count as usize..];
    }
    true
}

fn read_all(fd: u64, mut bytes: &mut [u8]) -> bool {
    while !bytes.is_empty() {
        let count = native(10, fd, bytes.as_mut_ptr() as u64, bytes.len() as u64);
        if count <= 0 || count as usize > bytes.len() {
            return false;
        }
        bytes = &mut bytes[count as usize..];
    }
    true
}

fn pipe() -> Option<[u64; 2]> {
    let mut pair = [0u64; 2];
    (native(14, pair.as_mut_ptr() as u64, 16, 0) == 0).then_some(pair)
}

fn wait(pid: u64) -> bool {
    let mut status = Wait::default();
    native(7, pid, &mut status as *mut Wait as u64, 16) == pid as i64
        && status.reason == 0
        && status.code == 0
}

fn execute(command: &[u8], output: &mut [u8; CAPACITY]) -> Result<usize, ()> {
    let (name, argument): (&[u8], &[u8]) = if command.starts_with(b"echo ") {
        (b"/bin/echo", &command[5..])
    } else if command == b"pwd" {
        (b"/bin/pwd", b"")
    } else {
        return Err(());
    };
    let pair = pipe().ok_or(())?;
    let child = native(8, 0, 0, 0);
    if child < 0 {
        close(pair[0]);
        close(pair[1]);
        return Err(());
    }
    if child == 0 {
        close(pair[0]);
        if native(13, pair[1], 1, 0) < 0 {
            exit(10);
        }
        if pair[1] != 1 {
            close(pair[1]);
        }
        let request = Exec {
            name: name.as_ptr() as u64,
            name_len: name.len() as u64,
            argument: argument.as_ptr() as u64,
            argument_len: argument.len() as u64,
        };
        let _ = native(6, &request as *const Exec as u64, 32, 0);
        exit(11);
    }
    close(pair[1]);
    let mut length = 0;
    let mut valid = true;
    while length < output.len() {
        let bytes = &mut output[length..];
        let count = native(10, pair[0], bytes.as_mut_ptr() as u64, bytes.len() as u64);
        if count < 0 || count as usize > bytes.len() {
            valid = false;
            break;
        }
        if count == 0 {
            break;
        }
        length += count as usize;
    }
    close(pair[0]);
    if !wait(child as u64) || !valid {
        return Err(());
    }
    if length > 0 && output[length - 1] == b'\n' {
        length -= 1;
    }
    Ok(length)
}

fn serve(request: u64, response: u64) -> ! {
    loop {
        let mut header = [0u8; 4];
        if !read_all(request, &mut header) {
            break;
        }
        let length = u32::from_le_bytes(header) as usize;
        if length > CAPACITY {
            exit(20);
        }
        let mut command = [0u8; CAPACITY];
        if !read_all(request, &mut command[..length]) {
            exit(21);
        }
        let mut output = [0u8; CAPACITY];
        let (status, written) = match execute(&command[..length], &mut output) {
            Ok(written) => (0u32, written),
            Err(()) => {
                output[..19].copy_from_slice(b"unsupported command");
                (1, 19)
            }
        };
        if !write_all(response, &status.to_le_bytes())
            || !write_all(response, &(written as u32).to_le_bytes())
            || !write_all(response, &output[..written])
        {
            exit(22);
        }
    }
    close(request);
    close(response);
    exit(0)
}

impl Helper {
    fn start() -> Option<Self> {
        let request = pipe()?;
        let Some(response) = pipe() else {
            close(request[0]);
            close(request[1]);
            return None;
        };
        let pid = native(8, 0, 0, 0);
        if pid < 0 {
            for fd in [request[0], request[1], response[0], response[1]] {
                close(fd);
            }
            return None;
        }
        if pid == 0 {
            close(request[1]);
            close(response[0]);
            serve(request[0], response[1]);
        }
        close(request[0]);
        close(response[1]);
        Some(Self {
            pid: pid as u64,
            request: request[1],
            response: response[0],
        })
    }

    fn command(&self, command: &[u8], output: &mut [u8; CAPACITY]) -> Option<usize> {
        if !write_all(self.request, &(command.len() as u32).to_le_bytes())
            || !write_all(self.request, command)
        {
            return None;
        }
        let mut status = [0u8; 4];
        let mut length = [0u8; 4];
        if !read_all(self.response, &mut status) || !read_all(self.response, &mut length) {
            return None;
        }
        let length = u32::from_le_bytes(length) as usize;
        if length > CAPACITY || !read_all(self.response, &mut output[..length]) {
            return None;
        }
        if u32::from_le_bytes(status) == 0 {
            let mut record = [0u8; 320];
            let mut written = 0;
            for part in [
                &b"[GUI:TERMINAL:COMMAND] command="[..],
                command,
                &b" output="[..],
                &output[..length],
                &b" exit=0\n"[..],
            ] {
                record[written..written + part.len()].copy_from_slice(part);
                written += part.len();
            }
            log(&record[..written]);
        }
        Some(length)
    }

    fn finish(self) -> bool {
        close(self.request);
        close(self.response);
        wait(self.pid)
    }
}

fn render(
    window: &mut Window,
    command: &mut [u8],
    length: &mut u32,
    last: &[u8],
    output: &[u8],
    event: Option<&GuiEvent>,
) -> Result<bool, Error> {
    let last = core::str::from_utf8(last).map_err(|_| Error::Utf8)?;
    let output = core::str::from_utf8(output).map_err(|_| Error::Utf8)?;
    window.clear(0x182838)?;
    window.text(12, 24, "ANOVA TERMINAL", 0xffffff)?;
    window.text(12, 52, "$", 0x70b0f0)?;
    window.text(24, 52, last, 0x70b0f0)?;
    window.text(12, 76, output, 0xffffff)?;
    window.text(12, 98, "echo TEXT / pwd; Enter", 0xffffff)?;
    window.textfield(
        GuiRect {
            x: 12,
            y: 116,
            width: 216,
            height: 28,
        },
        command,
        length,
        event,
    )?;
    match window.present() {
        Ok(()) => Ok(true),
        Err(Error::WouldBlock) => Ok(false),
        Err(error) => Err(error),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    // Only the helper forks command children; the UI never forks after creating GUI state.
    let Some(helper) = Helper::start() else {
        return 20;
    };
    let mut output = [0u8; CAPACITY];
    let Some(mut output_length) = helper.command(b"echo M14 native output", &mut output) else {
        return 21;
    };
    if &output[..output_length] != b"M14 native output" {
        return 22;
    }
    log(b"[GUI:TERMINAL:PIPE] command=echo output=M14 native output exit=0\n");
    let mut last = [0u8; CAPACITY];
    last[..22].copy_from_slice(b"echo M14 native output");
    let mut last_length = 22;
    let mut command = [0u8; CAPACITY];
    let mut length = 0;
    let Ok(mut window) = Window::create(GuiWindowDesc::new(80, 80, 240, 160)) else {
        return 1;
    };
    if render(
        &mut window,
        &mut command,
        &mut length,
        &last[..last_length],
        &output[..output_length],
        None,
    )
    .is_err()
    {
        return 2;
    }
    log(b"[GUI:TERMINAL:READY]\n");
    let mut pending_command = false;
    loop {
        let event = match window.poll() {
            Ok(event) => event,
            Err(Error::WouldBlock) => {
                let _ = native(4, 1, 0, 0);
                continue;
            }
            Err(_) => return 3,
        };
        if event.kind == 2 && event.state == 1 {
            if event.codepoint == u32::from(b'q') && length == 0 {
                break;
            }
            if matches!(event.codepoint, 10 | 13) && length > 0 {
                last_length = length as usize;
                last[..last_length].copy_from_slice(&command[..last_length]);
                let Some(written) = helper.command(&last[..last_length], &mut output) else {
                    return 4;
                };
                output_length = written;
                length = 0;
                pending_command = true;
            }
        }
        let presented = match render(
            &mut window,
            &mut command,
            &mut length,
            &last[..last_length],
            &output[..output_length],
            Some(&event),
        ) {
            Ok(presented) => presented,
            Err(_) => return 5,
        };
        if pending_command && presented {
            log(b"[GUI:TERMINAL:RENDERED]\n");
            pending_command = false;
        }
        if presented
            && event.kind == 2
            && matches!(event.state, 1 | 3)
            && (32..=126).contains(&event.codepoint)
        {
            let digits = [
                b'0' + (length / 100) as u8,
                b'0' + ((length / 10) % 10) as u8,
                b'0' + (length % 10) as u8,
            ];
            let start = if length >= 100 {
                0
            } else if length >= 10 {
                1
            } else {
                2
            };
            log(b"[GUI:TERMINAL:INPUT] length=");
            log(&digits[start..]);
            log(b"\n");
        }
    }
    if window.close().is_err() || !helper.finish() {
        return 6;
    }
    log(b"[GUI:TERMINAL:DONE]\n");
    0
}
