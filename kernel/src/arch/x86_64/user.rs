// SPDX-License-Identifier: Apache-2.0
//! BSP Ring 3 transition and native SYSCALL configuration with IRETQ returns.
//! User code gets baseline x87/SSE2 only; no kernel SIMD or concurrent tasks.
use super::cpu;
use core::arch::{asm, global_asm, x86_64::__cpuid};

global_asm!(include_str!("user.S"));

unsafe extern "C" {
    pub fn anova_run_user(root: u64, entry: u64, stack: u64, kernel_stack: u64);
    pub fn anova_leave_user() -> !;
    fn anova_syscall_entry();
    pub fn anova_copy_user(destination: *mut u8, source: *const u8, length: usize) -> u64;
    pub fn anova_copy_user_fault();
    pub fn anova_copy_user_failed();
}

pub fn init() {
    // SAFETY: Long mode supports CPUID; check SYSCALL and the baseline FXSAVE/SSE2.
    let features = unsafe { __cpuid(1) };
    // SAFETY: x86_64 provides extended feature leaf 0x80000001.
    let extended = unsafe { __cpuid(0x8000_0001) };
    assert_eq!(
        features.edx & ((1 << 24) | (1 << 25) | (1 << 26)),
        (1 << 24) | (1 << 25) | (1 << 26),
        "FXSR/SSE2 required"
    );
    assert_ne!(extended.edx & (1 << 11), 0, "SYSCALL required");
    let (mut cr0, mut cr4): (u64, u64);
    // SAFETY: CPL0, IF=0, no user context or enabled extended state exists yet.
    // Kernel target remains soft-float. Disable unimplemented XSAVE/FSGSBASE.
    unsafe {
        asm!("mov {}, cr0",out(reg) cr0,options(nomem,nostack,preserves_flags));
        asm!("mov {}, cr4",out(reg) cr4,options(nomem,nostack,preserves_flags));
        cr0 = (cr0 & !((1 << 2) | (1 << 3))) | (1 << 1) | (1 << 5);
        cr4 = (cr4 & !((1 << 16) | (1 << 18))) | (1 << 9) | (1 << 10);
        asm!("mov cr0, {}",in(reg) cr0,options(nostack,preserves_flags));
        asm!("mov cr4, {}",in(reg) cr4,options(nostack,preserves_flags));
        asm!("xor eax,eax","mov fs,ax","mov gs,ax",out("eax") _,options(nostack));
    }
    for msr in [0xc000_0100, 0xc000_0101, 0xc000_0102] {
        cpu::write_msr(msr, 0);
    }
    cpu::write_msr(0xc000_0081, 8_u64 << 32); // Kernel CS=8, SS=16; SYSRET is unused.
    cpu::write_msr(0xc000_0082, anova_syscall_entry as *const () as u64);
    cpu::write_msr(0xc000_0084, 0x0004_7700); // IF, TF, DF, IOPL, NT, AC.
    cpu::write_msr(0xc000_0080, cpu::read_msr(0xc000_0080) | 1);
    crate::serial::write(format_args!(
        "[USER:CPU] syscall=true return=iretq x87=true SSE2=true AVX=false\n"
    ));
}

pub fn reset_fpu() {
    #[repr(C, align(16))]
    struct State([u8; 512]);
    let mut state = State([0; 512]);
    state.0[0..2].copy_from_slice(&0x037f_u16.to_le_bytes());
    state.0[24..28].copy_from_slice(&0x1f80_u32.to_le_bytes());
    // SAFETY: Aligned, fully initialized architectural FXSAVE image, all x87/MMX
    // and XMM payloads zero. Only this single user can use SIMD; kernel never does.
    unsafe {
        asm!("fxrstor64 [{}]",in(reg) state.0.as_ptr(),options(nostack));
    }
}
