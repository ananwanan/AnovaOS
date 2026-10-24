// SPDX-License-Identifier: Apache-2.0
//! Check ext2 syscalls, 4 MiB file bounds, shared offsets, indirection, sparse zeros and unlink.
//! Reboot verification also checks editor-created source and a binary copied by the interactive shell.
use super::*;
pub fn io_error() {
    assert_eq!(
        rt::open("/persist/io-error", OPEN_WRITE | OPEN_CREATE),
        Err(IO_ERROR)
    );
    assert_eq!(rt::sync(), Err(IO_ERROR));
    assert_eq!(
        rt::open("/persist/retry", OPEN_WRITE | OPEN_CREATE),
        Err(IO_ERROR)
    );
    assert_eq!(rt::shutdown(), Err(IO_ERROR));
    rt::write(b"[DISK:IOERROR] create=error sync=error retry=error kernel=alive\n");
}
pub fn suite() {
    let fd = rt::open(
        "/persist/probe",
        OPEN_READ | OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE,
    )
    .unwrap();
    assert_eq!(
        rt::open("/persist/probe", OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE),
        Err(ALREADY_EXISTS)
    );
    rt::write_all(fd, b"abc").unwrap();
    rt::seek(fd, 0, 0).unwrap();
    rt::dup2(fd, 7).unwrap();
    // SAFETY: Single-threaded native fork shares only the open description, with no user allocator.
    let child = unsafe { rt::fork() }.unwrap();
    if child == 0 {
        let mut b = [0];
        assert_eq!(rt::read(7, &mut b), Ok(1));
        assert_eq!(b, [b'a']);
        rt::exit(0);
    }
    ok(child);
    let mut bytes = [0; 256];
    assert_eq!(rt::read(fd, &mut bytes[..2]), Ok(2));
    assert_eq!(&bytes[..2], b"bc");
    // Cross the twelfth direct block and verify new sparse space contains zeros.
    rt::seek(fd, 12 * 4096 + 13, 0).unwrap();
    rt::write_all(fd, b"indirect-tail").unwrap();
    rt::seek(fd, 4096, 0).unwrap();
    assert_eq!(rt::read(fd, &mut bytes), Ok(256));
    assert_eq!(bytes, [0; 256]);
    rt::seek(fd, 12 * 4096 + 13, 0).unwrap();
    assert_eq!(rt::read(fd, &mut bytes), Ok(13));
    assert_eq!(&bytes[..13], b"indirect-tail");
    rt::seek(fd, 1024 * 1024, 0).unwrap();
    rt::write_all(fd, b"extended").unwrap();
    rt::seek(fd, 1024 * 1024, 0).unwrap();
    assert_eq!(rt::read(fd, &mut bytes), Ok(8));
    assert_eq!(&bytes[..8], b"extended");
    rt::seek(fd, 4 * 1024 * 1024, 0).unwrap();
    assert_eq!(rt::write_all(fd, b"overflow"), Err(NO_SPACE));
    assert_eq!(rt::seek(fd, 0, 1), Ok(4 * 1024 * 1024));
    // SAFETY: A bad user source must be rejected before disk mutation or offset advancement.
    assert_eq!(unsafe { rt::raw(SYS_WRITE, fd, 0, 1) }, BAD_ADDRESS);
    rt::fsync(fd).unwrap();
    assert_eq!(rt::fsync(99), Err(BAD_DESCRIPTOR));
    assert_eq!(rt::fsync(1), Err(INVALID_ARGUMENT));
    rt::unlink("/persist/probe").unwrap();
    rt::seek(7, 0, 0).unwrap();
    assert_eq!(rt::read(7, &mut bytes[..3]), Ok(3));
    assert_eq!(&bytes[..3], b"abc");
    rt::close(fd).unwrap();
    rt::close(7).unwrap();
    // A released inode and its blocks must be reusable in the same session.
    let fd = rt::open("/persist/probe", OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE).unwrap();
    rt::write_all(fd, b"reused").unwrap();
    rt::close(fd).unwrap();
    rt::unlink("/persist/probe").unwrap();
    rt::sync().unwrap();
    rt::write(b"[DISK:TEST] offsets=true fork=true indirect=true sparse=true unlink=true reuse=true negatives=true\n");
}
pub fn reboot() {
    content("/persist/src/demo.c", b"int main(void) { return 42; }\n");
    content("/persist/note", b"durable\nappend\n");
    assert_eq!(rt::open("/tmp/volatile", OPEN_READ), Err(NOT_FOUND));
    let source = rt::open("/bin/hello-c", OPEN_READ).unwrap();
    let saved = rt::open("/persist/saved-app", OPEN_READ).unwrap();
    let mut a = [0; 256];
    let mut b = [0; 256];
    let mut total = 0;
    loop {
        let n = rt::read(source, &mut a).unwrap();
        assert_eq!(rt::read(saved, &mut b), Ok(n));
        if n == 0 {
            break;
        }
        assert_eq!(&a[..n], &b[..n]);
        total += n;
    }
    assert!(total > 4096);
    rt::close(source).unwrap();
    rt::close(saved).unwrap();
    rt::write(b"[DISK:REBOOT] source=exact binary=exact note=exact tmpfs=lost\n");
}
