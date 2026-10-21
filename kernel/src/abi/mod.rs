// SPDX-License-Identifier: Apache-2.0
//! Adapt native system queries; task owns process lifecycle and fs owns descriptor calls.
use crate::{
    arch::x86_64::{apic, traps::TrapFrame},
    mm::{uaccess, user::UserSpace},
};
use anova_abi::*;
pub fn syscall(frame: &TrapFrame, space: &UserSpace, pid: u64) -> i64 {
    match frame.rax {
        SYS_EXIT => INVALID_ARGUMENT,
        SYS_INFO => {
            if frame.rsi != size_of::<SystemInfo>() as u64 {
                return INVALID_ARGUMENT;
            }
            let bytes = SystemInfo {
                abi_version: VERSION,
                size: 32,
                page_size: 4096,
                ticks: apic::ticks(),
                process_id: pid,
            }
            .bytes();
            uaccess::to_user(space, UserAddr(frame.rdi), &bytes).map_or_else(|e| e, |()| 0)
        }
        _ => UNSUPPORTED,
    }
}
