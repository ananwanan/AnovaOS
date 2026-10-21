// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Exercise real scheduling, blocking, fork/exec, parent waits and descriptor lifetimes.
//! Distinct CPU sentinels survive syscall-free loops; intentional child faults stay isolated.
use anova_rt::{self as rt, Args, abi::*};
use core::{
    arch::{asm, global_asm},
    panic::PanicInfo,
    sync::atomic::{AtomicU64, Ordering},
};
global_asm!(include_str!("context.S"));
static PRIVATE: AtomicU64 = AtomicU64::new(0);
unsafe extern "C" {
    fn anova_context_burn(seed: u64, iterations: u64, fork: u64) -> i64;
}

fn burn(fork: bool) -> u64 {
    let seed = rt::info().unwrap().process_id;
    // SAFETY: Assembly follows SysV, preserves callee-saved GPRs and restores
    // default FP controls. There are no external resources requiring fork hooks.
    let result = unsafe { anova_context_burn(seed, 30_000_000, u64::from(fork)) };
    assert!(result >= 0, "CPU context corruption or fork failure");
    result as u64
}
fn wait(pid: u64, reason: u32, code: u32) {
    let result = rt::wait(pid).unwrap();
    assert!(pid == 0 || result.pid == pid);
    assert_eq!(result.status, ExitStatus { reason, code });
}
fn spawn(argument: &str) -> u64 {
    rt::spawn("process-test", argument).unwrap()
}

fn suite() {
    assert_eq!(rt::wait(0).err(), Some(NO_CHILD));
    assert_eq!(rt::spawn("missing-image", "").err(), Some(NOT_FOUND));
    assert_eq!(rt::exec("missing-image", "").err(), Some(NOT_FOUND));
    assert_eq!(rt::sleep(u64::MAX), Err(INVALID_ARGUMENT));
    // SAFETY: Invalid pointers and layouts are explicitly rejected before access.
    unsafe {
        assert_eq!(rt::raw(SYS_SPAWN, 0, 32, 0), BAD_ADDRESS);
        assert_eq!(rt::raw(SYS_EXEC, 0, 31, 0), INVALID_ARGUMENT);
        assert_eq!(rt::raw(SYS_WAIT, 0, 0, 16), BAD_ADDRESS);
    }
    let before = rt::info().unwrap().ticks;
    rt::sleep(3).unwrap();
    assert!(rt::info().unwrap().ticks >= before + 3);
    let a = spawn("worker-a");
    let b = spawn("worker-b");
    let crash = spawn("crash");
    // A failed wait copy must not consume the eventual zombie.
    // SAFETY: This invalid output is guaranteed to return BAD_ADDRESS.
    unsafe {
        assert_eq!(rt::raw(SYS_WAIT, crash, 0, 16), BAD_ADDRESS);
    }
    wait(crash, 1, 6);
    wait(a, 0, 0);
    wait(b, 0, 0);
    assert_eq!(rt::wait(a).err(), Some(NO_CHILD));
    rt::write(b"[PROC:CASE] preemption-fpu-fault passed=true\n");

    PRIVATE.store(17, Ordering::Relaxed);
    // SAFETY: This single-threaded native process has only console descriptors.
    let child = unsafe { rt::fork() }.unwrap();
    if child == 0 {
        assert_eq!(PRIVATE.load(Ordering::Relaxed), 17);
        PRIVATE.store(99, Ordering::Relaxed);
        rt::close(1).unwrap();
        assert_eq!(rt::write(b""), BAD_DESCRIPTOR);
        rt::sleep(2).unwrap();
        rt::exit(23);
    }
    wait(child, 0, 23);
    assert_eq!(PRIVATE.load(Ordering::Relaxed), 17);
    assert!(rt::write(b"[PROC:CASE] fork-private-fd passed=true\n") > 0);
    let child = burn(true);
    if child == 0 {
        rt::exit(29);
    }
    wait(child, 0, 29);
    rt::write(b"[PROC:CASE] fork-cpu-state passed=true\n");
    let child = spawn("exec");
    wait(child, 0, 31);
    rt::write(b"[PROC:CASE] exec-reset-retain-fd passed=true\n");
    let orphan = spawn("orphan");
    wait(orphan, 0, 19);
    for _ in 0..16 {
        spawn("quick");
        wait(0, 0, 17);
    }
    rt::write(b"[PROC:CASE] wait-any-orphan-repeat passed=true\n");
    for _ in 0..31 {
        spawn("quick");
    }
    assert_eq!(
        rt::spawn("process-test", "quick").err(),
        Some(PROCESS_LIMIT)
    );
    for _ in 0..31 {
        wait(0, 0, 17);
    }
    let reused = spawn("quick");
    wait(reused, 0, 17);
    rt::write(b"[PROC:CASE] capacity-reuse passed=true\n");
}

#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(argc: usize, argv: *const *const u8) -> u32 {
    // SAFETY: The CRT supplies the loader's bounded immutable argument block.
    let mode = unsafe { Args::from_raw(argc, argv) }.nth(1).unwrap();
    match mode {
        b"worker-a" | b"worker-b" => {
            assert!(rt::check_cpu());
            burn(false);
            rt::yield_now();
            burn(false);
            rt::sleep(2).unwrap();
            rt::write(b"[PROC:WORKER] GPR/XMM0-15/x87/MXCSR preserved\n");
            0
        }
        b"crash" => {
            // SAFETY: Deliberate child-only #UD must terminate this process.
            unsafe {
                asm!("ud2", options(noreturn));
            }
        }
        b"quick" => 17,
        b"orphan" => {
            spawn("quick");
            19
        }
        b"exec" => {
            PRIVATE.store(99, Ordering::Relaxed);
            rt::close(1).unwrap();
            rt::exec("process-test", "exec-target").unwrap();
            panic!("Successful exec returned");
        }
        b"exec-target" => {
            assert_eq!(PRIVATE.load(Ordering::Relaxed), 0);
            assert!(rt::check_cpu());
            assert_eq!(rt::write(b""), BAD_DESCRIPTOR);
            assert_eq!(rt::close(1), Err(BAD_DESCRIPTOR));
            31
        }
        b"suite" => {
            suite();
            0
        }
        b"demo" => {
            let a = spawn("worker-a");
            let b = spawn("worker-b");
            wait(a, 0, 0);
            wait(b, 0, 0);
            0
        }
        _ => 101,
    }
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    // stderr stays open even in descriptor-lifetime tests.
    // SAFETY: A static readable error string is a valid console write buffer.
    unsafe {
        rt::raw(
            SYS_WRITE,
            2,
            b"[APP:FAIL] process suite\n".as_ptr() as u64,
            25,
        );
    }
    rt::exit(101)
}
