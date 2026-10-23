// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Supervise independent GUI processes and prove bounded transport admission and recovery.
//! QMP drives physical input; the supervisor only waits for actual child exit statuses.

use anova_gui::transport as t;
use anova_gui_user as gui;
use anova_rt::{self as rt, abi::*};

fn spawn(name: &str, mode: &str) -> u64 {
    let pid = rt::spawn(name, mode).unwrap();
    gui::log(format_args!(
        "[GUI:CLIENT] name={mode} pid={pid} image={name}\n"
    ));
    pid
}

fn wait(pid: u64, reason: u32, code: u32) {
    assert_eq!(rt::wait(pid).unwrap().status, ExitStatus { reason, code });
}

fn exists(path: &str) -> bool {
    match rt::open(path, OPEN_READ) {
        Ok(fd) => {
            rt::close(fd).unwrap();
            true
        }
        Err(NOT_FOUND) => false,
        Err(_) => panic!("GUI rendezvous open failed"),
    }
}

fn ready(path: &str) {
    for _ in 0..1000 {
        if exists(path) {
            return;
        }
        rt::sleep(1).unwrap();
    }
    panic!("GUI client readiness timeout");
}

fn remove(path: &str) {
    match rt::unlink(path) {
        Ok(()) | Err(NOT_FOUND) => {}
        Err(_) => panic!("GUI rendezvous cleanup failed"),
    }
}

fn transport_boundaries() {
    for operation in [t::BIND, t::RECEIVE, t::INPUT, t::PRESENT] {
        // SAFETY: An unauthorized caller is rejected before any null address is accessed.
        assert_eq!(
            unsafe { gui::call(operation, 0, 0) },
            Err(PERMISSION_DENIED)
        );
    }
    // SAFETY: SEND rejects these empty/oversize lengths before inspecting the null pointer.
    unsafe {
        assert_eq!(gui::call(t::SEND, 0, 0), Err(INVALID_ARGUMENT));
        assert_eq!(gui::call(t::SEND, 0, 4097), Err(INVALID_ARGUMENT));
    }
    rt::write(b"[GUI:TRANSPORT_BOUNDARIES] unauthorized=true bad_length=true\n");
}

fn pressure() {
    for path in [
        "/tmp/gui-hold0",
        "/tmp/gui-hold1",
        "/tmp/gui-hold2",
        "/tmp/gui-release",
    ] {
        remove(path);
    }
    // The supervisor retains one connection while three independent children fill the quota.
    let a = spawn("gui-client", "hold0");
    ready("/tmp/gui-hold0");
    let b = spawn("gui-client", "hold1");
    ready("/tmp/gui-hold1");
    let c = spawn("gui-client", "hold2");
    ready("/tmp/gui-hold2");
    let rejected = spawn("gui-client", "reject");
    wait(rejected, 0, 0);
    gui::scalar(t::CLOSE).unwrap();
    let fd = rt::open("/tmp/gui-release", OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE).unwrap();
    rt::write_all(fd, b"release").unwrap();
    rt::close(fd).unwrap();
    wait(a, 0, 0);
    wait(b, 0, 0);
    wait(c, 0, 0);
    // A new connection proves released kernel/client slots can be reused.
    rt::sleep(2).unwrap();
    gui::connect().unwrap();
    gui::scalar(t::CLOSE).unwrap();
    rt::write(b"[GUI:PRESSURE] connection_quota=true reclaimed=true heap_exhaustion=false\n");
}

#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(_: usize, _: *const *const u8) -> u32 {
    transport_boundaries();
    for path in [
        "/tmp/gui-red-ready",
        "/tmp/gui-blue-ready",
        "/tmp/gui-red-window",
    ] {
        remove(path);
    }
    let server = spawn("compositor", "test");
    match gui::connect() {
        Err(NOT_FOUND) => {
            wait(server, 0, 2);
            rt::write(b"[GUI:NO_DISPLAY]\n");
            return 0;
        }
        Ok(_) => {}
        Err(_) => panic!("GUI supervisor connection failed"),
    }
    // SAFETY: SEND guarantees BAD_ADDRESS for the deliberately unmapped null input.
    assert_eq!(unsafe { gui::call(t::SEND, 0, 64) }, Err(BAD_ADDRESS));
    rt::write(b"[GUI:POINTER_BOUNDARY] bad_pointer=true\n");
    pressure();
    let mailbox = spawn("gui-client", "mailbox");
    wait(mailbox, 0, 0);
    let red = spawn("gui-client", "red");
    ready("/tmp/gui-red-ready");
    let blue = spawn("gui-client", "blue");
    ready("/tmp/gui-blue-ready");
    rt::write(b"[GUI:READY] clients=2 isolated=true\n");
    let red_status = rt::wait(red).unwrap().status;
    let server_status = rt::wait(server).unwrap().status;
    wait(blue, 0, 0);
    if server_status
        == (ExitStatus {
            reason: 0,
            code: 74,
        })
    {
        assert_eq!(red_status, ExitStatus { reason: 0, code: 0 });
        rt::write(b"[GUI:NO_INPUT_PASS] clients_reaped=true\n");
        return 0;
    }
    assert_eq!(red_status, ExitStatus { reason: 1, code: 6 });
    assert_eq!(server_status, ExitStatus { reason: 1, code: 6 });
    rt::write(b"[GUI:FAULT_REAPED] reason=1 code=6 other_client_survived=true\n");
    rt::write(b"[GUI:SERVER_FAULT_REAPED] reason=1 code=6\n");
    let restarted = spawn("compositor", "restart");
    gui::connect().unwrap();
    gui::scalar(t::CLOSE).unwrap();
    let client = spawn("gui-client", "restart");
    wait(restarted, 0, 0);
    wait(client, 0, 0);
    rt::write(b"[GUI:TEST_PASS] restart=true all_children_reaped=true\n");
    0
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    rt::write(b"[GUI:FAIL] supervisor\n");
    rt::exit(101)
}
