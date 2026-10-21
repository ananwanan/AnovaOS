// SPDX-License-Identifier: Apache-2.0
//! Run one isolated native process at a time and return to a trusted continuation.
//! This owns lifecycle/teardown, not a scheduler, fork, signals or Linux processes.
mod loader;
#[cfg(feature = "qemu-test")]
mod validation;
use crate::{
    abi,
    arch::x86_64::{
        cpu,
        traps::{self, TrapFrame},
        user,
    },
    boot, mm, serial,
};
use anova_abi::{PERSONALITY, SYS_EXIT, UserAddr};
use core::{
    arch::asm,
    ptr,
    sync::atomic::{AtomicPtr, AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Exit(u32),
    Fault {
        vector: u64,
        error: u64,
        address: u64,
    },
    BadReturn,
}
struct Process {
    image: loader::Loaded,
    pid: u64,
    outcome: Option<Outcome>,
}
static CURRENT: AtomicPtr<Process> = AtomicPtr::new(ptr::null_mut());
static USER_IRQS: AtomicU64 = AtomicU64::new(0);

pub fn note_timer() {
    USER_IRQS.fetch_add(1, Ordering::Relaxed);
}

fn finish(process: &mut Process, outcome: Outcome) -> ! {
    process.outcome = Some(outcome);
    // SAFETY: Only the active BSP process can reach this path. The assembly
    // restores its saved kernel CR3/stack and callee-saved registers. This tail
    // transfer crosses no Rust destructor or held lock; process owns resources
    // on the suspended kernel stack and run() drops them after normal return.
    unsafe { user::anova_leave_user() }
}

pub fn dispatch(frame: &mut TrapFrame) {
    assert_eq!(cpu::flags() & (1 << 9), 0, "User dispatch requires IF=0");
    let pointer = CURRENT.load(Ordering::Acquire);
    assert!(!pointer.is_null(), "User trap without an active process");
    // SAFETY: run publishes a pinned stack local for one BSP only. IRQs cannot
    // reenter syscall dispatch, and ordinary timer IRQs never borrow this state.
    let process = unsafe { &mut *pointer };
    if frame.vector != 0x80 {
        let address: u64;
        // SAFETY: CPL0 captures CR2 for the user fault report; it is meaningful for #PF.
        unsafe {
            asm!("mov {},cr2",out(reg) address,options(nomem,nostack,preserves_flags));
        }
        let address = if frame.vector == 14 { address } else { 0 };
        serial::emergency(format_args!(
            "[USER:FAULT] pid={} vector={} error={:#x} address={address:#x} rip={:#x}\n",
            process.pid, frame.vector, frame.error, frame.rip
        ));
        finish(
            process,
            Outcome::Fault {
                vector: frame.vector,
                error: frame.error,
                address,
            },
        );
    }
    if frame.rax == SYS_EXIT && frame.rdi <= u32::MAX as u64 {
        serial::write(format_args!(
            "[USER:EXIT] pid={} status={}\n",
            process.pid, frame.rdi
        ));
        finish(process, Outcome::Exit(frame.rdi as u32));
    }
    frame.rax = abi::syscall(frame, &process.image.space, process.pid) as u64;
    let valid = frame.cs == 0x33
        && frame.ss == 0x2b
        && process
            .image
            .space
            .accessible(UserAddr(frame.rip), 1, false, true)
        && frame.rsp.checked_sub(8).is_some_and(|address| {
            process
                .image
                .space
                .accessible(UserAddr(address), 8, true, false)
        });
    if !valid {
        serial::emergency(format_args!(
            "[USER:RETURNREJECT] pid={} rip={:#x} rsp={:#x}\n",
            process.pid, frame.rip, frame.rsp
        ));
        finish(process, Outcome::BadReturn);
    }
    // Preserve arithmetic/DF flags only. Force IF and the reserved-one bit;
    // clear IOPL/TF/NT/RF/VM/AC so user state cannot change kernel privilege.
    frame.rflags = (frame.rflags & 0xcd5) | 0x202;
}

fn run(bytes: &[u8], args: &[&str], pid: u64) -> Result<Outcome, loader::LoadError> {
    let image = loader::load(bytes, PERSONALITY, args)?;
    Ok(cpu::without_interrupts(|| {
        let mut process = Process {
            image,
            pid,
            outcome: None,
        };
        assert!(
            CURRENT.load(Ordering::Acquire).is_null(),
            "Concurrent processes require M5"
        );
        traps::set_kernel_stack(mm::user::KERNEL_TOP);
        user::reset_fpu();
        serial::write(format_args!(
            "[USER:ENTER] pid={pid} abi={PERSONALITY} cr3={:#x} entry={:#x} rsp={:#x} cs=0x33\n",
            process.image.space.root().0,
            process.image.entry,
            process.image.stack
        ));
        CURRENT.store(&mut process, Ordering::Release);
        // SAFETY: Loader validated RX entry, RW guarded stack and private root.
        // The kernel stack and all entry code stay supervisor-mapped. The only
        // return is finish's trusted continuation after restoring the kernel CR3.
        unsafe {
            user::anova_run_user(
                process.image.space.root().0,
                process.image.entry,
                process.image.stack,
                mm::user::KERNEL_TOP,
            );
        }
        CURRENT.store(ptr::null_mut(), Ordering::Release);
        mm::user::restore_kernel_stack();
        let outcome = process
            .outcome
            .expect("User context returned without exit or fault");
        drop(process);
        outcome
    }))
}

fn checked_run(bytes: &[u8], args: &[&str], pid: u64, expected: Outcome) {
    let before = mm::user::free_frames();
    assert_eq!(
        run(bytes, args, pid).unwrap_or_else(|e| panic!("App load failed: {e}")),
        expected,
        "Unexpected app result"
    );
    assert_eq!(
        mm::user::free_frames(),
        before,
        "Process leaked physical pages"
    );
    serial::write(format_args!("[USER:REAP] pid={pid} frames_restored=true\n"));
}

pub fn start() {
    cpu::without_interrupts(user::init);
    let rust = boot::native_module("hello-rust");
    let c = boot::native_module("hello-c");
    checked_run(
        rust,
        &["hello-rust", "AnovaOS", "ring3"],
        1,
        Outcome::Exit(0),
    );
    checked_run(c, &["hello-c", "native-v1"], 2, Outcome::Exit(7));
    #[cfg(feature = "qemu-test")]
    cpu::without_interrupts(|| validation::run(rust));
    serial::write(format_args!(
        "[USER:READY] Rust/C apps exited; kernel resumed\n"
    ));
}
