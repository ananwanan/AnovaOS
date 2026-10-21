// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Run the native shell and small multicall tools with fork/exec/wait, pipes and redirection.
//! Keep all parsing, command policy and line assembly in user space, without a libc or allocator.
use anova_rt::{self as rt, abi::*};
use core::fmt::{self, Write};
use shell::{Command, Pipeline};
struct Output(u64);
impl Write for Output {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        rt::write_all(self.0, text.as_bytes()).map_err(|_| fmt::Error)
    }
}
fn status(code: u32) {
    let _ = writeln!(Output(1), "[SHELL:STATUS] code={code}");
}
fn copy(input: u64) -> Result<(), i64> {
    let mut bytes = [0; 256];
    loop {
        let n = rt::read(input, &mut bytes)?;
        if n == 0 {
            return Ok(());
        }
        rt::write_all(1, &bytes[..n])?;
    }
}
fn tool(name: &str, argument: &str) -> Result<u32, i64> {
    Ok(match name {
        "echo" => {
            rt::write_all(1, argument.as_bytes())?;
            rt::write_all(1, b"\n")?;
            0
        }
        "cat" => {
            if argument.is_empty() {
                copy(0)?;
            } else {
                let fd = rt::open(argument, OPEN_READ)?;
                let result = copy(fd);
                rt::close(fd)?;
                result?;
            }
            0
        }
        "ls" => {
            let fd = rt::open(
                if argument.is_empty() { "." } else { argument },
                OPEN_READ | OPEN_DIRECTORY,
            )?;
            let result = (|| -> Result<(), i64> {
                while let Some(entry) = rt::readdir(fd)? {
                    rt::write_all(1, &entry.name[..entry.name_len as usize])?;
                    if entry.kind == 2 {
                        rt::write_all(1, b"/")?;
                    }
                    rt::write_all(1, b"\n")?;
                }
                Ok(())
            })();
            rt::close(fd)?;
            result?;
            0
        }
        "pwd" => {
            let mut bytes = [0; 128];
            let n = rt::getcwd(&mut bytes)?;
            rt::write_all(1, &bytes[..n])?;
            rt::write_all(1, b"\n")?;
            0
        }
        "mkdir" => {
            rt::mkdir(argument)?;
            0
        }
        "rm" => {
            rt::unlink(argument)?;
            0
        }
        "true" => 0,
        "false" => 1,
        _ => return Err(NOT_FOUND),
    })
}
fn redirect(command: &Command) -> Result<(), i64> {
    if !command.input.empty() {
        let fd = rt::open(command.input.text(), OPEN_READ)?;
        rt::dup2(fd, 0)?;
        if fd != 0 {
            rt::close(fd)?;
        }
    }
    if !command.output.empty() {
        let flags = OPEN_WRITE
            | OPEN_CREATE
            | if command.append {
                OPEN_APPEND
            } else {
                OPEN_TRUNCATE
            };
        let fd = rt::open(command.output.text(), flags)?;
        rt::dup2(fd, 1)?;
        if fd != 1 {
            rt::close(fd)?;
        }
    }
    Ok(())
}
fn child(command: &Command, pipes: &[[u64; 2]], stage: usize) -> ! {
    let result = (|| -> Result<(), i64> {
        if stage > 0 {
            rt::dup2(pipes[stage - 1][0], 0)?;
        }
        if stage < pipes.len() {
            rt::dup2(pipes[stage][1], 1)?;
        }
        for pipe in pipes {
            rt::close(pipe[0])?;
            rt::close(pipe[1])?;
        }
        redirect(command)?;
        rt::exec(command.name.text(), command.argument.text())?;
        Err(UNSUPPORTED)
    })();
    let error = result.unwrap_err();
    let _ = writeln!(Output(2), "shell: {} failed ({error})", command.name.text());
    rt::exit(if error == NOT_FOUND { 127 } else { 1 });
}
fn execute(pipeline: &Pipeline) -> u32 {
    let mut pipes = [[0; 2]; 3];
    let mut made = 0;
    for pipe in &mut pipes[..pipeline.count - 1] {
        match rt::pipe() {
            Ok(p) => {
                *pipe = p;
                made += 1;
            }
            Err(_) => {
                for p in &pipes[..made] {
                    let _ = rt::close(p[0]);
                    let _ = rt::close(p[1]);
                }
                return 1;
            }
        }
    }
    let pipes = &pipes[..made];
    let mut children = [0; 4];
    let mut started = 0;
    for (i, command) in pipeline.commands[..pipeline.count].iter().enumerate() {
        // SAFETY: The SDK is single-threaded with no allocator; inherited descriptors
        // are shared kernel references and each child closes all unused pipe endpoints.
        match unsafe { rt::fork() } {
            Ok(0) => child(command, pipes, i),
            Ok(pid) => {
                children[i] = pid;
                started += 1;
            }
            Err(_) => break,
        }
    }
    for p in pipes {
        let _ = rt::close(p[0]);
        let _ = rt::close(p[1]);
    }
    let mut code = 1;
    for pid in &children[..started] {
        match rt::wait(*pid) {
            Ok(s) => {
                code = if s.status.reason == 0 {
                    s.status.code
                } else {
                    128 + s.status.code
                }
            }
            Err(_) => code = 1,
        }
    }
    if started == pipeline.count { code } else { 1 }
}
fn run(input: u64, interactive: bool) -> u32 {
    let mut line = [0; 512];
    let mut len = 0;
    let mut overflow = false;
    let mut previous = 0;
    if interactive {
        rt::write(b"AnovaOS native shell. /tmp is lost on reboot. Type help.\nanova> ");
    }
    loop {
        let mut byte = [0];
        match rt::read(input, &mut byte) {
            Ok(0) => return previous,
            Err(_) => return 1,
            _ => {}
        }
        if byte[0] != b'\n' {
            if len < line.len() {
                line[len] = byte[0];
                len += 1;
            } else {
                overflow = true;
            }
            continue;
        }
        let parsed = if overflow {
            Err(shell::ParseError)
        } else {
            shell::parse(&line[..len])
        };
        len = 0;
        overflow = false;
        match parsed {
            Err(_) => {
                rt::write(b"shell: syntax or line limit error\n");
                previous = 2;
            }
            Ok(p) if p.count == 0 => {}
            Ok(p) => {
                let c = &p.commands[0];
                let name = c.name.text();
                if matches!(name, "cd" | "exit" | "status" | "help") {
                    if p.count != 1 || !c.input.empty() || !c.output.empty() {
                        previous = 2;
                    } else {
                        match name {
                            "cd" => {
                                previous = if rt::chdir(if c.argument.empty() {
                                    "/"
                                } else {
                                    c.argument.text()
                                })
                                .is_ok()
                                {
                                    0
                                } else {
                                    1
                                }
                            }
                            "exit" => {
                                return if c.argument.empty() {
                                    previous
                                } else {
                                    c.argument.text().parse().unwrap_or(2)
                                };
                            }
                            "status" => {
                                status(previous);
                                previous = 0;
                            }
                            _ => {
                                rt::write(b"cd pwd ls cat echo mkdir rm true false status exit; | < > >>; /bin apps\n");
                                previous = 0;
                            }
                        }
                    }
                } else {
                    previous = execute(&p);
                }
            }
        }
        if interactive {
            rt::write(b"anova> ");
        }
    }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(argc: usize, argv: *const *const u8) -> u32 {
    // SAFETY: The loader supplies bounded immutable strings for the process lifetime.
    let mut args = unsafe { rt::Args::from_raw(argc, argv) };
    let name = core::str::from_utf8(args.next().unwrap())
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap();
    let argument = core::str::from_utf8(args.next().unwrap_or(b"")).unwrap();
    if name != "shell" {
        return match tool(name, argument) {
            Ok(code) => code,
            Err(error) => {
                let _ = writeln!(Output(2), "{name}: failed ({error})");
                1
            }
        };
    }
    if argument.is_empty() {
        run(0, true)
    } else {
        match rt::open(argument, OPEN_READ) {
            Ok(fd) => {
                let code = run(fd, false);
                let _ = rt::close(fd);
                code
            }
            Err(_) => 1,
        }
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    let _ = rt::write_all(2, b"[SHELL:FAIL]\n");
    rt::exit(101)
}
