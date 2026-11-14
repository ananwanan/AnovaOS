// SPDX-License-Identifier: Apache-2.0
//! Exercise real session ownership, failed-copy retention and PTY/task/open-description recovery.
//! Run before creating a window; deliberate native forks never duplicate live GUI ownership.
use super::{Session, gui, rt};
use rt::abi::{
    pty::{self, Request},
    *,
};

fn raw(operation: u32, id: u64, request: Request) -> i64 {
    let bytes = request.bytes();
    // SAFETY: The exact initialized record lives for this call. Some tests deliberately
    // supply inaccessible transfer pointers, which the kernel must reject without loss.
    unsafe {
        rt::raw(
            pty::SYSCALL,
            pty::command(operation),
            id,
            bytes.as_ptr() as u64,
        )
    }
}

fn send(session: &Session, mut bytes: &[u8]) {
    for _ in 0..500 {
        if bytes.is_empty() {
            return;
        }
        match session.write(bytes) {
            Ok(n) => bytes = &bytes[n..],
            Err(WOULD_BLOCK) => rt::sleep(1).unwrap(),
            Err(_) => panic!("PTY check write failed"),
        }
    }
    panic!("PTY check write timeout");
}

fn drain(session: &Session) {
    let mut bytes = [0; 1024];
    for _ in 0..500 {
        match session.read(&mut bytes) {
            Ok(0) | Err(WOULD_BLOCK) => return,
            Ok(_) => {}
            Err(_) => panic!("PTY check read failed"),
        }
    }
    panic!("PTY check drain timeout");
}

fn stop(pid: u64) {
    rt::process::stop(pid).unwrap();
    rt::wait(pid).unwrap();
}

pub(super) fn run() {
    assert_eq!(raw(pty::START, 0, Request::default()), INVALID_ARGUMENT);
    // SAFETY: An unmapped request must fail without allocating a PTY or creating a child.
    assert_eq!(
        unsafe { rt::raw(pty::SYSCALL, pty::command(pty::START), 0, 0) },
        BAD_ADDRESS
    );
    let mut sessions: [Option<Session>; 4] = core::array::from_fn(|_| None);
    for s in &mut sessions {
        *s = Some(Session::start(26, 50).unwrap());
    }
    assert_eq!(Session::start(26, 50).err(), Some(FILE_LIMIT));
    let id = sessions[0].as_ref().unwrap().id();
    assert_eq!(
        raw(
            pty::READ,
            id,
            Request {
                length: 1025,
                ..Request::default()
            }
        ),
        INVALID_ARGUMENT
    );
    // SAFETY: No GUI or process-local ownership has been created. This child only tries
    // a foreign scalar session ID and exits; it does not drop or use copied wrappers.
    let child = unsafe { rt::fork() }.unwrap();
    if child == 0 {
        assert_eq!(raw(pty::CLOSE, id, Request::default()), NO_CHILD);
        rt::exit(0);
    }
    assert_eq!(rt::wait(child).unwrap().status.code, 0);
    for s in &mut sessions {
        drop(s.take());
    }
    assert_eq!(raw(pty::CLOSE, id, Request::default()), NO_CHILD);

    let session = Session::start(26, 50).unwrap();
    assert!(session.id() > id);
    rt::sleep(20).unwrap();
    drain(&session);
    send(&session, b"/linux/echo COPY-RETAINED\r");
    rt::sleep(20).unwrap();
    assert_eq!(
        raw(
            pty::READ,
            session.id(),
            Request {
                buffer: 0,
                length: 1024,
                ..Request::default()
            }
        ),
        BAD_ADDRESS
    );
    let mut output = [0; 1024];
    let n = session.read(&mut output).unwrap();
    assert!(output[..n].windows(13).any(|s| s == b"COPY-RETAINED"));
    send(&session, b"exit 17\r");
    let mut done = false;
    for _ in 0..500 {
        drain(&session);
        if let Some(status) = session.status().unwrap() {
            assert_eq!(
                status.status,
                ExitStatus {
                    reason: 0,
                    code: 17
                }
            );
            done = true;
            break;
        }
        rt::sleep(1).unwrap();
    }
    assert!(done, "PTY shell exit timeout");
    assert_eq!(
        raw(
            pty::STATUS,
            session.id(),
            Request {
                length: 16,
                ..Request::default()
            }
        ),
        BAD_ADDRESS
    );
    assert_eq!(session.status().unwrap().unwrap().status.code, 17);
    drop(session);

    let mut children = [0; 32];
    let mut count = 0;
    loop {
        match rt::spawn("desktop", "park") {
            Ok(pid) => {
                children[count] = pid;
                count += 1;
            }
            Err(PROCESS_LIMIT) => break,
            Err(_) => panic!("PTY task pressure failed"),
        }
    }
    assert_eq!(Session::start(26, 50).err(), Some(PROCESS_LIMIT));
    for &pid in &children[..count] {
        stop(pid);
    }
    drop(Session::start(26, 50).unwrap());

    // Each child fills its actual 32-slot table; independently opened files consume the
    // global 128-description pool. Existing pipes can still report/release that pressure.
    let mut holders = [0; 5];
    let mut readers = [0; 5];
    for index in 0..5 {
        let pair = rt::pipe().unwrap();
        // SAFETY: This pre-GUI check owns no live session here. The child never returns
        // through copied owners and parks until its exact parent stops/reaps it.
        let pid = unsafe { rt::fork() }.unwrap();
        if pid == 0 {
            rt::close(pair[0]).unwrap();
            loop {
                match rt::open("/etc/motd", OPEN_READ) {
                    Ok(_) => {}
                    Err(FILE_LIMIT) => break,
                    Err(_) => panic!("PTY descriptor pressure failed"),
                }
            }
            rt::write_all(pair[1], b"R").unwrap();
            loop {
                rt::sleep(100).unwrap();
            }
        }
        rt::close(pair[1]).unwrap();
        holders[index] = pid;
        readers[index] = pair[0];
        let mut ready = [0];
        assert_eq!(rt::read(pair[0], &mut ready).unwrap(), 1);
        if Session::start(26, 50).err() == Some(FILE_LIMIT) {
            for n in 0..=index {
                stop(holders[n]);
                rt::close(readers[n]).unwrap();
            }
            for _ in 0..4 {
                drop(Session::start(26, 50).unwrap());
            }
            gui::log(format_args!(
                "[TERMINAL:BOUNDARIES] owner={} pty=4 ownership=true stale=true copy_retained=true status=17 task_pressure={count} open_pressure=true recovered=true\n",
                rt::info().unwrap().process_id
            ));
            return;
        }
    }
    panic!("PTY description pool was not exhausted");
}
