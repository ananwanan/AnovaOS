// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Start the native shell, wait for child status and restart interactive sessions after exit.
//! Test mode first runs the independent filesystem suite and then a shell script.
use anova_rt as rt;
#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(argc: usize, argv: *const *const u8) -> u32 {
    // SAFETY: The native loader provides bounded immutable startup strings.
    let test = unsafe { rt::Args::from_raw(argc, argv) }.nth(1) == Some(&b"test"[..]);
    assert_eq!(rt::info().unwrap().process_id, 1);
    rt::write(b"[INIT:START] pid=1 native-v1 tmpfs=volatile\n");
    if test {
        let child = rt::spawn("/bin/fs-test", "suite").unwrap();
        assert_eq!(
            rt::wait(child).unwrap().status,
            rt::abi::ExitStatus { reason: 0, code: 0 }
        );
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
