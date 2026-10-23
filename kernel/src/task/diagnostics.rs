// SPDX-License-Identifier: Apache-2.0
//! Report captured user faults and walk bounded frame pointers with checked, recoverable usercopy.
//! The scheduler owns live mappings with IF=0; diagnostics never dereference user pointers in Rust.
use crate::{arch::x86_64::traps::TrapFrame, mm, serial};
use anova_abi::UserAddr;

const MAX_FRAMES: usize = 16;
const MAX_STACK_SPAN: u64 = 1024 * 1024;
const MAX_FRAME_STEP: u64 = 64 * 1024;

/// Capture addresses before the process image is released or Linux delivers a signal handler.
/// Frame zero is the faulting instruction; callers use return-address minus one for symbol lookup.
pub(super) fn fault(pid: u64, space: &mm::user::UserSpace, frame: &TrapFrame) {
    serial::emergency(format_args!(
        "[USER:REGISTERS] pid={pid} vector={} error={:#x} rip={:#x} rsp={:#x} rbp={:#x} rax={:#x} rbx={:#x} rcx={:#x} rdx={:#x} rsi={:#x} rdi={:#x}\n",
        frame.vector,
        frame.error,
        frame.rip,
        frame.rsp,
        frame.rbp,
        frame.rax,
        frame.rbx,
        frame.rcx,
        frame.rdx,
        frame.rsi,
        frame.rdi
    ));
    serial::emergency(format_args!(
        "[USER:BACKTRACE] pid={pid} index=0 pc={:#x} lookup={:#x}\n",
        frame.rip, frame.rip
    ));
    let mut count = 1;
    let mut pointer = frame.rbp;
    let end = frame.rsp.saturating_add(MAX_STACK_SPAN);
    let reason = space.with_active(|| {
        while count < MAX_FRAMES {
            if pointer == 0 {
                return "end";
            }
            if pointer & 7 != 0 || pointer < frame.rsp || pointer > end.saturating_sub(16) {
                return "invalid-frame";
            }
            let mut record = [0_u8; 16];
            if mm::uaccess::from_user(space, UserAddr(pointer), &mut record).is_err() {
                return "unreadable-frame";
            }
            let next = u64::from_le_bytes(record[..8].try_into().unwrap());
            let pc = u64::from_le_bytes(record[8..].try_into().unwrap());
            let Some(lookup) = pc.checked_sub(1) else {
                return "invalid-return";
            };
            if !space.accessible(UserAddr(lookup), 1, false, true) {
                return "non-executable-return";
            }
            serial::emergency(format_args!(
                "[USER:BACKTRACE] pid={pid} index={count} pc={pc:#x} lookup={lookup:#x}\n"
            ));
            count += 1;
            if next != 0 && (next <= pointer || next - pointer > MAX_FRAME_STEP) {
                return "invalid-chain";
            }
            pointer = next;
        }
        "frame-limit"
    });
    serial::emergency(format_args!(
        "[USER:BACKTRACE-END] pid={pid} frames={count} reason={reason} max_frames={MAX_FRAMES}\n"
    ));
    // This runs on the scheduler root after trap capture, outside the emergency
    // exception path. No allocator/MM lock survives user execution or reaches this call.
    serial::emergency(format_args!(
        "[USER:RESOURCES] pid={pid} free_frames={} heap_used_bytes={} task_capacity={}\n",
        mm::user::free_frames(),
        mm::heap_used(),
        anova_sched::CAPACITY
    ));
}
