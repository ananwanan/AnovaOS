// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Exercise real VFS/ext2 syscalls, inherited offsets, exec/cwd, user buffers, reboot data and pipes.
//! Child modes validate resources after fork/exec; shell-check reads files produced by its script.
use anova_rt::{self as rt, abi::*};
mod disk;
fn ok(pid: u64) {
    assert_eq!(
        rt::wait(pid).unwrap().status,
        ExitStatus { reason: 0, code: 0 }
    );
}
fn content(path: &str, expected: &[u8]) {
    let fd = rt::open(path, OPEN_READ).unwrap();
    let mut bytes = [0; 256];
    let mut offset = 0;
    loop {
        let n = rt::read(fd, &mut bytes).unwrap();
        if n == 0 {
            break;
        }
        assert_eq!(&bytes[..n], &expected[offset..offset + n]);
        offset += n;
    }
    assert_eq!(offset, expected.len());
    rt::close(fd).unwrap();
}
fn files() {
    assert_eq!(rt::open("/etc/motd", OPEN_WRITE), Err(READ_ONLY));
    assert_eq!(rt::open("/absent", OPEN_READ), Err(NOT_FOUND));
    assert_eq!(rt::open("/etc/motd/../motd", OPEN_READ), Err(NOT_DIRECTORY));
    assert_eq!(rt::open("/tmp", OPEN_WRITE), Err(IS_DIRECTORY));
    assert_eq!(rt::mkdir("/etc/new"), Err(READ_ONLY));
    assert_eq!(rt::read(99, &mut [0]), Err(BAD_DESCRIPTOR));
    let fd = rt::open(
        "/tmp/shared",
        OPEN_READ | OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE,
    )
    .unwrap();
    rt::write_all(fd, b"abc").unwrap();
    rt::seek(fd, 0, 0).unwrap();
    rt::dup2(fd, 8).unwrap();
    let mut b = [0];
    assert_eq!(rt::read(8, &mut b), Ok(1));
    assert_eq!(b, [b'a']);
    // SAFETY: The native single-threaded SDK has no allocator; child shares only kernel file descriptions.
    let pid = unsafe { rt::fork() }.unwrap();
    if pid == 0 {
        assert_eq!(rt::read(fd, &mut b), Ok(1));
        assert_eq!(b, [b'b']);
        rt::exit(0);
    }
    ok(pid);
    assert_eq!(rt::read(fd, &mut b), Ok(1));
    assert_eq!(b, [b'c']);
    rt::seek(fd, 0, 0).unwrap();
    // SAFETY: Deliberately invalid output must be rejected before advancing the file offset.
    assert_eq!(unsafe { rt::raw(SYS_READ, fd, 0, 1) }, BAD_ADDRESS);
    assert_eq!(rt::seek(fd, 0, 1), Ok(0));
    rt::dup2(fd, 7).unwrap();
    ok(rt::spawn("/bin/fs-test", "exec-file").unwrap());
    assert_eq!(rt::seek(fd, 0, 1), Ok(3));
    rt::unlink("/tmp/shared").unwrap();
    assert_eq!(rt::open("/tmp/shared", OPEN_READ), Err(NOT_FOUND));
    rt::seek(fd, 0, 0).unwrap();
    assert_eq!(rt::read(fd, &mut b), Ok(1));
    assert_eq!(b, [b'a']);
    for f in [fd, 7, 8] {
        rt::close(f).unwrap();
    }
    rt::mkdir("/tmp/work").unwrap();
    rt::chdir("/tmp/work").unwrap();
    ok(rt::spawn("/bin/fs-test", "cwd").unwrap());
    let mut path = [0; 128];
    let n = rt::getcwd(&mut path).unwrap();
    assert_eq!(&path[..n], b"/tmp/work");
    rt::chdir("/").unwrap();
    let d = rt::open("/dev", OPEN_READ | OPEN_DIRECTORY).unwrap();
    // SAFETY: Bad directory output and pipe arrays are rejected without resource or cursor mutation.
    unsafe {
        assert_eq!(rt::raw(SYS_READDIR, d, 0, 80), BAD_ADDRESS);
        assert_eq!(rt::raw(SYS_PIPE, 0, 16, 0), BAD_ADDRESS);
        assert_eq!(rt::raw(SYS_PIPE, 0, 15, 0), INVALID_ARGUMENT);
        assert_eq!(rt::raw(SYS_OPEN, u64::MAX, 128, OPEN_READ), BAD_ADDRESS);
    }
    assert_eq!(&rt::readdir(d).unwrap().unwrap().name[..7], b"console");
    assert!(rt::readdir(d).unwrap().is_some());
    assert!(rt::readdir(d).unwrap().is_none());
    rt::close(d).unwrap();
    rt::write(b"[FS:CASE] paths-copy-offset-fork-exec-cwd passed=true\n");
}
fn pipes() {
    let [r, w] = rt::pipe().unwrap();
    // SAFETY: This bounded single-threaded fork deliberately splits pipe endpoint ownership.
    let pid = unsafe { rt::fork() }.unwrap();
    if pid == 0 {
        rt::close(r).unwrap();
        rt::write_all(w, &[b'P'; 4096]).unwrap();
        rt::close(w).unwrap();
        rt::exit(0);
    }
    rt::close(w).unwrap();
    rt::sleep(3).unwrap();
    let mut total = 0;
    let mut bytes = [0; 127];
    loop {
        let n = rt::read(r, &mut bytes).unwrap();
        if n == 0 {
            break;
        }
        assert!(bytes[..n].iter().all(|b| *b == b'P'));
        total += n;
    }
    assert_eq!(total, 4096);
    rt::close(r).unwrap();
    ok(pid);
    let [r, w] = rt::pipe().unwrap();
    rt::dup2(w, 9).unwrap();
    rt::close(w).unwrap();
    // SAFETY: The child waits with a retained write endpoint until its short sleep completes.
    let pid = unsafe { rt::fork() }.unwrap();
    if pid == 0 {
        rt::close(r).unwrap();
        rt::sleep(2).unwrap();
        rt::write_all(9, b"tail").unwrap();
        rt::exit(0);
    }
    rt::close(9).unwrap();
    let mut out = [0; 4];
    assert_eq!(rt::read(r, &mut out), Ok(4));
    assert_eq!(&out, b"tail");
    assert_eq!(rt::read(r, &mut out), Ok(0));
    rt::close(r).unwrap();
    ok(pid);
    let [r, w] = rt::pipe().unwrap();
    rt::close(r).unwrap();
    assert_eq!(rt::write_fd(w, b"x"), Err(BROKEN_PIPE));
    assert_eq!(rt::seek(w, 0, 0), Err(NOT_SEEKABLE));
    rt::close(w).unwrap();
    rt::write(b"[FS:CASE] pipe-block-drain-eof-broken passed=true\n");
}
fn limits() {
    let mut fds = [0; 32];
    let mut n = 0;
    loop {
        match rt::open("/dev/null", OPEN_WRITE) {
            Ok(fd) => {
                fds[n] = fd;
                n += 1;
            }
            Err(FILE_LIMIT) => break,
            _ => panic!("Unexpected fd capacity result"),
        }
    }
    assert_eq!(n, 29);
    assert_eq!(rt::pipe(), Err(FILE_LIMIT));
    assert_eq!(
        rt::open("/tmp/not-created", OPEN_WRITE | OPEN_CREATE),
        Err(FILE_LIMIT)
    );
    for fd in &fds[..n] {
        rt::close(*fd).unwrap();
    }
    assert_eq!(rt::open("/tmp/not-created", OPEN_READ), Err(NOT_FOUND));
    for _ in 0..128 {
        let fd = rt::open("/tmp/reuse", OPEN_WRITE | OPEN_CREATE).unwrap();
        rt::write_all(fd, b"reused").unwrap();
        rt::close(fd).unwrap();
        rt::unlink("/tmp/reuse").unwrap();
    }
    let fd = rt::open("/tmp/big", OPEN_WRITE | OPEN_CREATE).unwrap();
    for _ in 0..16 {
        rt::write_all(fd, &[b'L'; 4096]).unwrap();
    }
    assert_eq!(rt::write_fd(fd, b"x"), Err(NO_SPACE));
    rt::close(fd).unwrap();
    rt::unlink("/tmp/big").unwrap();
    rt::write(b"[FS:CASE] limits-rollback-reuse passed=true\n");
}
fn heap_pressure() {
    let mut count = 0;
    let mut reached_oom = false;
    for i in 0..48 {
        let mut path = *b"/tmp/pressure-aa";
        path[15] = b'a' + (i % 26) as u8;
        path[14] = b'a' + (i / 26) as u8;
        let fd = rt::open(
            core::str::from_utf8(&path).unwrap(),
            OPEN_WRITE | OPEN_CREATE,
        )
        .unwrap();
        count += 1;
        let mut offset = 0;
        for _ in 0..16 {
            match rt::write_fd(fd, &[b'O'; 4096]) {
                Ok(n) => {
                    assert_eq!(n, 4096);
                    offset += n;
                }
                Err(NO_MEMORY) => {
                    assert_eq!(rt::seek(fd, 0, 1), Ok(offset as u64));
                    reached_oom = true;
                    break;
                }
                other => panic!("Unexpected tmpfs pressure result: {other:?}"),
            }
        }
        rt::close(fd).unwrap();
        if reached_oom {
            break;
        }
    }
    assert!(reached_oom);
    for i in 0..count {
        let mut path = *b"/tmp/pressure-aa";
        path[15] = b'a' + (i % 26) as u8;
        path[14] = b'a' + (i / 26) as u8;
        rt::unlink(core::str::from_utf8(&path).unwrap()).unwrap();
    }
    let fd = rt::open("/tmp/recovered", OPEN_WRITE | OPEN_CREATE).unwrap();
    rt::write_all(fd, &[b'R'; 4096]).unwrap();
    rt::close(fd).unwrap();
    rt::unlink("/tmp/recovered").unwrap();
    rt::write(b"[FS:CASE] tmpfs-oom-rollback passed=true\n");
}
fn shell_check() {
    content("/tmp/message", b"M6-shell-ok\nagain\n");
    content("/tmp/pipe", b"pipeline-data\n");
    content("/tmp/work/note", b"relative\n");
    content("/tmp/large", &[b'Q'; 5000]);
    rt::write(b"[SHELL:TEST] files=verified pipeline_bytes=5000 redirection=true cwd=true\n");
}
#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(argc: usize, argv: *const *const u8) -> u32 {
    // SAFETY: The native CRT provides bounded immutable argument storage.
    let mode = unsafe { rt::Args::from_raw(argc, argv) }
        .nth(1)
        .unwrap_or(b"");
    match mode {
        b"exec-file" => {
            let mut out = [0; 3];
            assert_eq!(rt::read(7, &mut out), Ok(3));
            assert_eq!(&out, b"abc");
        }
        b"cwd" => {
            let mut path = [0; 128];
            let n = rt::getcwd(&mut path).unwrap();
            assert_eq!(&path[..n], b"/tmp/work");
            rt::chdir("/").unwrap();
        }
        b"shell-check" => shell_check(),
        b"disk-suite" => disk::suite(),
        b"disk-reboot" => disk::reboot(),
        b"disk-error" => disk::io_error(),
        b"suite" => {
            files();
            pipes();
            limits();
            heap_pressure();
            disk::suite();
            rt::write(b"[FS:TEST] passed=true\n");
        }
        _ => return 2,
    }
    0
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    struct Diagnostic;
    impl core::fmt::Write for Diagnostic {
        fn write_str(&mut self, text: &str) -> core::fmt::Result {
            rt::write_all(2, text.as_bytes()).map_err(|_| core::fmt::Error)
        }
    }
    use core::fmt::Write;
    let _ = writeln!(Diagnostic, "{info}");
    let _ = rt::write_all(2, b"[FS:FAIL]\n");
    rt::exit(101)
}
