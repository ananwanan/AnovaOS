// SPDX-License-Identifier: Apache-2.0
//! Exercise actual native child/task/descriptor boundaries without replacing GUI acceptance.
//! The explicitly admitted short Linux fixture tests personality rejection and exits itself.

use super::{gui, rt, stop};
use rt::abi::*;

pub(super) fn boundaries() {
    let parent = 1;
    assert_eq!(rt::process::poll(parent).err(), Some(NO_CHILD));
    let linux = rt::spawn("/linux/desktop-park", "").unwrap();
    assert_eq!(rt::process::poll(linux).err(), Some(UNSUPPORTED));
    assert_eq!(rt::process::stop(linux), Err(UNSUPPORTED));
    assert_eq!(
        rt::wait(linux).unwrap().status,
        ExitStatus { reason: 0, code: 0 }
    );
    assert_eq!(rt::process::poll(linux).err(), Some(NO_CHILD));

    let mut children = [0; 32];
    let mut length = 0;
    loop {
        match rt::spawn("desktop", "park") {
            Ok(pid) => {
                assert!(length < children.len());
                children[length] = pid;
                length += 1;
            }
            Err(PROCESS_LIMIT) => break,
            Err(_) => panic!("Unexpected task pressure error"),
        }
    }
    assert!(length > 0);
    for pid in &children[..length] {
        stop(*pid).unwrap();
        assert_eq!(rt::process::poll(*pid).err(), Some(NO_CHILD));
    }
    let replacement = rt::spawn("desktop", "park").unwrap();
    stop(replacement).unwrap();

    let mut descriptors = [0; 32];
    let mut opened = 0;
    loop {
        match rt::open("/etc/motd", OPEN_READ) {
            Ok(fd) => {
                assert!(opened < descriptors.len());
                descriptors[opened] = fd;
                opened += 1;
            }
            Err(FILE_LIMIT) => break,
            Err(_) => panic!("Unexpected descriptor pressure error"),
        }
    }
    assert!(opened > 0);
    for fd in &descriptors[..opened] {
        rt::close(*fd).unwrap();
    }
    let fd = rt::open("/etc/motd", OPEN_READ).unwrap();
    rt::close(fd).unwrap();
    gui::log(format_args!(
        "[DESKTOP:BOUNDARIES] nonchild=true live_linux=true task_pressure={length} fd_pressure={opened} reused=true\n"
    ));
}
