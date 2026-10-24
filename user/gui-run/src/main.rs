// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Run an independently built native GUI application alongside an isolated compositor.
//! Report actual wait statuses; physical F11 stops the compositor after the client exits.
use anova_gui::transport as t;
use anova_gui_user as gui;
use anova_rt as rt;

fn wait(role: &str, pid: u64, expected: rt::abi::ExitStatus) -> bool {
    let status = rt::wait(pid).unwrap().status;
    gui::log(format_args!(
        "[GUI-SDK:WAIT] role={role} pid={pid} reason={} code={}\n",
        status.reason, status.code
    ));
    status == expected
}

#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(argc: usize, argv: *const *const u8) -> u32 {
    // SAFETY: The native loader supplies immutable bounded startup strings.
    let mut args = unsafe { rt::Args::from_raw(argc, argv) };
    let path = args.nth(1).unwrap_or(b"");
    let Ok(path) = core::str::from_utf8(path) else {
        return 2;
    };
    if !path.starts_with("/persist/gui-") {
        rt::write(b"gui-run: expected /persist/gui-NAME\n");
        return 2;
    }
    let server = rt::spawn("compositor", "restart").unwrap();
    gui::log(format_args!(
        "[GUI-SDK:CHILD] role=compositor pid={server}\n"
    ));
    gui::connect().unwrap();
    gui::scalar(t::CLOSE).unwrap();
    rt::set_executable(path).unwrap();
    let client = rt::spawn(path, "").unwrap();
    gui::log(format_args!("[GUI-SDK:CHILD] role=client pid={client}\n"));
    let expected = if path == "/persist/gui-fault" {
        rt::abi::ExitStatus {
            reason: 1,
            code: 14,
        }
    } else {
        rt::abi::ExitStatus { reason: 0, code: 0 }
    };
    let client_ok = wait("client", client, expected);
    rt::write(b"[GUI-SDK:STOP] press_f11=true\n");
    let server_ok = wait(
        "compositor",
        server,
        rt::abi::ExitStatus { reason: 0, code: 0 },
    );
    if client_ok && server_ok {
        rt::write(b"[GUI-SDK:RUN] all_children_reaped=true\n");
        0
    } else {
        rt::write(b"[GUI-SDK:FAIL] child_status\n");
        1
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    rt::write(b"[GUI-SDK:FAIL] supervisor\n");
    rt::exit(101)
}
