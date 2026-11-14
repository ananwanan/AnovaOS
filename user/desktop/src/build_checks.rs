// SPDX-License-Identifier: Apache-2.0
//! Assert real controlled-job usercopy, ownership, identity and shared resource rollback.
//! Runs pre-GUI in a separate process against the explicitly admitted fixture compiler.

use super::{gui, rt, stop};
use rt::{
    abi::{build as b, *},
    build::Job,
};

fn raw(op: u32, id: u64, r: b::Request) -> i64 {
    let bytes = r.bytes();
    // SAFETY: The initialized wire record lives throughout the call; selected
    // transfer addresses are deliberately invalid under the checked copy contract.
    unsafe { rt::raw(b::SYSCALL, b::command(op), id, bytes.as_ptr() as u64) }
}
fn missing_staging() {
    assert_eq!(rt::open(b::STAGING, OPEN_READ), Err(NOT_FOUND));
}
fn finished(job: &Job) -> WaitStatus {
    for _ in 0..500 {
        if let Some(status) = job.status().unwrap() {
            return status;
        }
        rt::sleep(1).unwrap();
    }
    panic!("Compiler fixture status timeout");
}
pub fn run() {
    missing_staging();
    assert_eq!(raw(b::START, 1, b::Request::default()), INVALID_ARGUMENT);
    assert_eq!(
        raw(
            b::READ,
            1,
            b::Request {
                length: 1025,
                ..b::Request::default()
            }
        ),
        INVALID_ARGUMENT
    );
    // SAFETY: Unmapped request memory must be rejected before resource mutation.
    assert_eq!(
        unsafe { rt::raw(b::SYSCALL, b::command(b::START), 0, 0) },
        BAD_ADDRESS
    );
    assert_eq!(
        raw(
            b::START,
            0,
            b::Request {
                reserved: [1, 0],
                ..b::Request::default()
            }
        ),
        INVALID_ARGUMENT
    );
    let job = Job::start().unwrap();
    let id = job.id();
    assert_eq!(Job::start().err(), Some(BUSY));
    let request = pty::Request {
        length: 16,
        ..pty::Request::default()
    }
    .bytes();
    // SAFETY: Valid local PTY wire record with a build handle tests interface isolation.
    assert_eq!(
        unsafe {
            rt::raw(
                pty::SYSCALL,
                pty::command(pty::STATUS),
                id,
                request.as_ptr() as u64,
            )
        },
        NO_CHILD
    );
    // SAFETY: This test has no GUI ownership; the child only tests foreign scalar
    // handles and exits without returning through or dropping copied owners.
    let pid = unsafe { rt::fork() }.unwrap();
    if pid == 0 {
        assert_eq!(raw(b::CLOSE, id, b::Request::default()), NO_CHILD);
        rt::exit(0);
    }
    assert_eq!(rt::wait(pid).unwrap().status.code, 0);
    let status = finished(&job);
    assert_eq!(
        status.status,
        ExitStatus {
            reason: 0,
            code: 23
        }
    );
    assert_eq!(
        raw(
            b::STATUS,
            id,
            b::Request {
                length: 16,
                ..b::Request::default()
            }
        ),
        BAD_ADDRESS
    );
    assert_eq!(finished(&job).bytes(), status.bytes());
    assert_eq!(
        raw(
            b::READ,
            id,
            b::Request {
                length: 1024,
                ..b::Request::default()
            }
        ),
        BAD_ADDRESS
    );
    let mut bytes = [0; 1024];
    let n = job.read(&mut bytes).unwrap();
    assert!(bytes[..n].windows(12).any(|s| s == b"JOB-COPY-OK!"));
    drop(job);
    missing_staging();
    assert_eq!(
        raw(
            b::STATUS,
            id,
            b::Request {
                length: 16,
                ..b::Request::default()
            }
        ),
        NO_CHILD
    );
    let replacement = Job::start().unwrap();
    assert!(replacement.id() > id);
    assert_eq!(raw(b::CLOSE, id, b::Request::default()), NO_CHILD);
    drop(replacement);

    let mut sessions: [Option<rt::pty::Session>; 4] = core::array::from_fn(|_| None);
    for s in &mut sessions {
        *s = Some(rt::pty::Session::start(26, 50).unwrap());
    }
    assert_eq!(Job::start().err(), Some(FILE_LIMIT));
    missing_staging();
    drop(sessions);
    let mut children = [0; 32];
    let mut count = 0;
    loop {
        match rt::spawn("desktop", "park") {
            Ok(pid) => {
                children[count] = pid;
                count += 1;
            }
            Err(PROCESS_LIMIT) => break,
            Err(_) => panic!("Job task pressure failed"),
        }
    }
    assert_eq!(Job::start().err(), Some(PROCESS_LIMIT));
    missing_staging();
    for pid in &children[..count] {
        stop(*pid).unwrap();
    }

    let mut holders = [0; 5];
    let mut readers = [0; 5];
    let mut exhausted = false;
    for index in 0..5 {
        let pair = rt::pipe().unwrap();
        // SAFETY: No GUI or job is live. The child fills its own file table and
        // parks, then its exact parent terminates/reaps it; copied owners are absent.
        let pid = unsafe { rt::fork() }.unwrap();
        if pid == 0 {
            rt::close(pair[0]).unwrap();
            loop {
                match rt::open("/etc/motd", OPEN_READ) {
                    Ok(_) => {}
                    Err(FILE_LIMIT) => break,
                    Err(_) => panic!("Unexpected job descriptor pressure error"),
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
        match Job::start() {
            Err(FILE_LIMIT) => {
                for n in 0..=index {
                    stop(holders[n]).unwrap();
                    rt::close(readers[n]).unwrap();
                }
                exhausted = true;
                break;
            }
            Ok(job) => drop(job),
            Err(_) => panic!("Unexpected job open-pressure error"),
        }
    }
    assert!(exhausted);
    missing_staging();
    let fd = rt::open(b::STAGING, OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE).unwrap();
    rt::write_all(fd, b"RECOVERY").unwrap();
    rt::close(fd).unwrap();
    assert_eq!(Job::start().err(), Some(ALREADY_EXISTS));
    let fd = rt::open(b::STAGING, OPEN_READ).unwrap();
    assert_eq!(rt::read(fd, &mut bytes).unwrap(), 8);
    assert_eq!(&bytes[..8], b"RECOVERY");
    rt::close(fd).unwrap();
    rt::unlink(b::STAGING).unwrap();
    let pair = rt::pipe().unwrap();
    // SAFETY: No live GUI/job owner exists. This child intentionally exits with
    // an owned job; kernel creator-death cleanup must release its complete family.
    let owner = unsafe { rt::fork() }.unwrap();
    if owner == 0 {
        rt::close(pair[0]).unwrap();
        let job = Job::start().unwrap();
        rt::write_all(pair[1], &job.id().to_le_bytes()).unwrap();
        core::mem::forget(job);
        rt::exit(0);
    }
    rt::close(pair[1]).unwrap();
    let mut identity = [0; 8];
    assert_eq!(rt::read(pair[0], &mut identity).unwrap(), 8);
    rt::close(pair[0]).unwrap();
    assert_eq!(rt::wait(owner).unwrap().status.code, 0);
    missing_staging();
    let replacement = Job::start().unwrap();
    assert!(replacement.id() > u64::from_le_bytes(identity));
    replacement.close().unwrap();
    for _ in 0..4 {
        drop(Job::start().unwrap());
    }
    gui::log(format_args!(
        "[BUILD:CHECK_PASS] owner={} copy_retained=true foreign=true stale=true task_pressure={count} pty=4 open_pressure=true staging_preserved=true\n",
        rt::info().unwrap().process_id
    ));
}

pub fn workflow() {
    use rt::abi::workflow::{self as w, Recipe};

    let recipe = Recipe::C;
    let staging = recipe.staging();
    let temporary = recipe.temporary();
    let raw_workflow = |op, id| {
        let request = b::Request::default().bytes();
        // SAFETY: The initialized request remains live for the checked synchronous call.
        unsafe { rt::raw(w::SYSCALL, w::command(op), id, request.as_ptr() as u64) }
    };
    // SAFETY: This intentionally malformed request must fail before any mutation.
    assert_eq!(
        unsafe { rt::raw(w::SYSCALL, w::command(w::START), recipe as u64, 0) },
        BAD_ADDRESS
    );
    assert_eq!(raw_workflow(w::START, 0), INVALID_ARGUMENT);
    assert_eq!(raw_workflow(w::START, 4), INVALID_ARGUMENT);
    assert_eq!(raw_workflow(w::CANCEL, u64::MAX), NO_CHILD);
    let source = rt::open(b::SOURCE, OPEN_CREATE | OPEN_WRITE | OPEN_TRUNCATE).unwrap();
    rt::write_all(source, b"k").unwrap();
    rt::close(source).unwrap();
    let job = Job::start_recipe(recipe).unwrap();
    let id = job.id();
    assert_eq!(Job::start().err(), Some(BUSY));
    assert_eq!(raw(b::CLOSE, id, b::Request::default()), NO_CHILD);
    assert_eq!(job.publish(), Err(WOULD_BLOCK));
    // SAFETY: The peer only probes scalars, exits without dropping the copied
    // Job and cannot use the parent's PTY or job handle.
    let peer = unsafe { rt::fork() }.unwrap();
    if peer == 0 {
        assert_eq!(raw_workflow(w::CANCEL, id), NO_CHILD);
        assert_eq!(raw_workflow(w::PUBLISH, id), NO_CHILD);
        rt::exit(0);
    }
    assert_eq!(rt::wait(peer).unwrap().status.code, 0);
    let mut tasks = [rt::abi::monitor::Task::default(); rt::abi::monitor::MAX_TASKS];
    for _ in 0..100 {
        if rt::monitor::tasks(&mut tasks)
            .unwrap()
            .iter()
            .filter(|task| task.family == id)
            .count()
            > 1
        {
            break;
        }
        rt::sleep(1).unwrap();
    }
    assert!(
        rt::monitor::tasks(&mut tasks)
            .unwrap()
            .iter()
            .filter(|task| task.family == id)
            .count()
            > 1
    );
    job.cancel().unwrap();
    assert_eq!(
        finished(&job).status,
        ExitStatus {
            reason: 3,
            code: 15
        }
    );
    job.close().unwrap();
    assert!(
        rt::monitor::tasks(&mut tasks)
            .unwrap()
            .iter()
            .all(|task| task.family != id)
    );
    assert_eq!(rt::open(staging, OPEN_READ), Err(NOT_FOUND));
    assert_eq!(rt::open(temporary, OPEN_READ), Err(NOT_FOUND));
    assert_eq!(raw_workflow(w::PUBLISH, id), NO_CHILD);

    let fd = rt::open(temporary, OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE).unwrap();
    rt::write_all(fd, b"RECOVERY").unwrap();
    rt::close(fd).unwrap();
    assert_eq!(Job::start_recipe(recipe).err(), Some(ALREADY_EXISTS));
    assert_eq!(rt::open(staging, OPEN_READ), Err(NOT_FOUND));
    let fd = rt::open(temporary, OPEN_READ).unwrap();
    let mut preserved = [0; 8];
    assert_eq!(rt::read(fd, &mut preserved), Ok(8));
    assert_eq!(&preserved, b"RECOVERY");
    rt::close(fd).unwrap();
    rt::unlink(temporary).unwrap();

    let source = rt::open(b::SOURCE, OPEN_WRITE | OPEN_TRUNCATE).unwrap();
    rt::write_all(source, b"p").unwrap();
    rt::close(source).unwrap();
    rt::mkdir(recipe.output()).unwrap();
    let job = Job::start_recipe(recipe).unwrap();
    assert!(job.id() > id);
    let outcome = finished(&job);
    assert_eq!(outcome.status.code, 0);
    let invalid_status = b::Request {
        length: 16,
        ..b::Request::default()
    }
    .bytes();
    // SAFETY: The request itself is valid and specifies an intentionally null output.
    assert_eq!(
        unsafe {
            rt::raw(
                w::SYSCALL,
                w::command(w::STATUS),
                job.id(),
                invalid_status.as_ptr() as u64,
            )
        },
        BAD_ADDRESS
    );
    assert_eq!(finished(&job).bytes(), outcome.bytes());
    assert_eq!(job.read(&mut [0; 16]), Ok(0));
    assert_eq!(job.publish(), Err(WOULD_BLOCK));
    assert_eq!(job.publish(), Err(IS_DIRECTORY));
    job.close().unwrap();
    let fd = rt::open(recipe.output(), OPEN_READ | OPEN_DIRECTORY).unwrap();
    rt::close(fd).unwrap();
    assert_eq!(rt::open(staging, OPEN_READ), Err(NOT_FOUND));
    assert_eq!(rt::open(temporary, OPEN_READ), Err(NOT_FOUND));

    let job = Job::start_recipe(recipe).unwrap();
    assert_eq!(finished(&job).status.code, 0);
    assert_eq!(job.read(&mut [0; 16]), Ok(0));
    rt::unlink(staging).unwrap();
    let fd = rt::open(staging, OPEN_WRITE | OPEN_CREATE | OPEN_EXCLUSIVE).unwrap();
    rt::write_all(fd, b"REPLACEMENT").unwrap();
    rt::close(fd).unwrap();
    assert_eq!(job.publish(), Err(BUSY));
    assert_eq!(job.close(), Err(BUSY));
    let fd = rt::open(staging, OPEN_READ).unwrap();
    let mut replacement = [0; 11];
    assert_eq!(rt::read(fd, &mut replacement), Ok(replacement.len()));
    assert_eq!(&replacement, b"REPLACEMENT");
    rt::close(fd).unwrap();
    rt::unlink(staging).unwrap();
    assert_eq!(rt::open(temporary, OPEN_READ), Err(NOT_FOUND));
    rt::write(b"[WORKFLOW:CHECK_PASS] owner=true stale=true detached_cleanup=true publication_rejected=true recovery_path_preserved=true replacement_preserved=true\n");
}
