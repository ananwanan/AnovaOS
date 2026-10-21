// SPDX-License-Identifier: Apache-2.0
//! Adapt native ABI v1 syscalls to console, process lifecycle and system queries.
//! Linux personality and filesystem descriptors are intentionally not implemented.
use crate::{
    arch::x86_64::{apic, traps::TrapFrame},
    mm::{uaccess, user::UserSpace},
    serial,
};
use anova_abi::*;

pub fn syscall(frame: &TrapFrame, space: &UserSpace, pid: u64) -> i64 {
    match frame.rax {
        SYS_EXIT => INVALID_ARGUMENT,
        SYS_WRITE => {
            if frame.rdi != 1 && frame.rdi != 2 {
                return INVALID_ARGUMENT;
            }
            let Ok(length) = usize::try_from(frame.rdx) else {
                return TOO_LONG;
            };
            if length > MAX_WRITE {
                return TOO_LONG;
            }
            let mut bytes = [0; MAX_WRITE];
            if let Err(error) = uaccess::from_user(space, UserAddr(frame.rsi), &mut bytes[..length])
            {
                return error;
            }
            serial::bytes(&bytes[..length]);
            length as i64
        }
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
            match uaccess::to_user(space, UserAddr(frame.rdi), &bytes) {
                Ok(()) => 0,
                Err(e) => e,
            }
        }
        _ => UNSUPPORTED,
    }
}
