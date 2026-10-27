// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Start the native shell, wait for child status and restart interactive sessions after exit.
//! Test mode runs the filesystem suite; desktop mode keeps COM1 and GUI children independent.
use anova_rt as rt;
#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(argc: usize, argv: *const *const u8) -> u32 {
    // SAFETY: The native loader provides bounded immutable startup strings.
    let mode = unsafe { rt::Args::from_raw(argc, argv) }
        .nth(1)
        .unwrap_or(b"");
    let test = mode == b"test";
    assert_eq!(rt::info().unwrap().process_id, 1);
    rt::write(b"[INIT:START] pid=1 native-v1 tmpfs=volatile\n");
    if test {
        let child = rt::spawn("/bin/fs-test", "suite").unwrap();
        assert_eq!(
            rt::wait(child).unwrap().status,
            rt::abi::ExitStatus { reason: 0, code: 0 }
        );
    }
    if mode == b"desktop" {
        let desktop = rt::spawn("/bin/desktop", "").unwrap();
        let mut shell = rt::spawn("/bin/shell", "").unwrap();
        loop {
            let child = rt::wait(0).unwrap();
            if child.pid == shell {
                rt::write(b"[INIT] COM1 shell exited; restarting\n");
                shell = rt::spawn("/bin/shell", "").unwrap();
            } else if child.pid == desktop {
                rt::write(b"[INIT] desktop exited; COM1 shell remains available\n");
            }
        }
    }
    loop {
        let child = rt::spawn("/bin/shell", if test { "/etc/smoke.sh" } else { "" }).unwrap();
        let status = rt::wait(child).unwrap().status;
        if test {
            assert_eq!(status, rt::abi::ExitStatus { reason: 0, code: 0 });
            rt::write(b"[INIT:TEST] shell_status=0 children_reaped=true\n");
            return 0;
        }
        rt::write(b"[INIT] shell exited; starting a new session\n");
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    let _ = rt::write_all(2, b"[INIT:FAIL]\n");
    rt::exit(101)
}
