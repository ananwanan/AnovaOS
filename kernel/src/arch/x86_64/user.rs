// SPDX-License-Identifier: Apache-2.0
//! BSP saved GPR/segment/x87/SSE2 contexts, native SYSCALL and IRETQ transitions.
//! Each user trap returns to the trusted scheduler root/stack; kernel code remains SIMD-free.
use super::cpu;
use core::arch::{asm, global_asm, x86_64::__cpuid};

global_asm!(include_str!("user.S"));

unsafe extern "C" {
    fn anova_run_context(
        root: u64,
        frame: *const super::traps::TrapFrame,
        fpu: *const u8,
        kernel_stack: u64,
    );
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
    // Disable AMD fast-FXSAVE omission so CPL0 saves/restores all XMM lanes.
    cpu::write_msr(0xc000_0080, (cpu::read_msr(0xc000_0080) & !(1 << 14)) | 1);
    crate::serial::write(format_args!(
        "[USER:CPU] syscall=true return=iretq x87=true SSE2=true AVX=false\n"
    ));
}

#[derive(Clone, Copy)]
#[repr(C, align(16))]
struct FpuState([u8; 512]);

#[derive(Clone, Copy)]
pub struct Context {
    pub frame: super::traps::TrapFrame,
    fpu: FpuState,
    selectors: [u16; 4],
    bases: [u64; 2],
}

impl Context {
    pub fn new(entry: u64, stack: u64) -> Self {
        let mut fpu = FpuState([0; 512]);
        fpu.0[..2].copy_from_slice(&0x037f_u16.to_le_bytes());
        fpu.0[24..28].copy_from_slice(&0x1f80_u32.to_le_bytes());
        let mut frame = super::traps::TrapFrame::default();
        frame.rip = entry;
        frame.rsp = stack;
        frame.cs = 0x33;
        frame.ss = 0x2b;
        frame.rflags = 0x202;
        Self {
            frame,
            fpu,
            selectors: [0x2b, 0x2b, 0, 0],
            bases: [0; 2],
        }
    }

    pub fn capture(&mut self, frame: &super::traps::TrapFrame) {
        self.frame = *frame;
        // SAFETY: This context is live and exclusively owned by the active BSP.
        // Its 512-byte area is 16-byte aligned; kernel Rust never touches SIMD.
        unsafe {
            asm!("fxsave64 [{}]", in(reg) self.fpu.0.as_mut_ptr(), options(nostack));
            asm!("mov {0:x},ds", "mov {1:x},es", "mov {2:x},fs", "mov {3:x},gs",
                out(reg) self.selectors[0], out(reg) self.selectors[1],
                out(reg) self.selectors[2], out(reg) self.selectors[3], options(nomem,nostack,preserves_flags));
        }
        self.bases = [cpu::read_msr(0xc000_0100), cpu::read_msr(0xc000_0101)];
    }

    pub fn run(&self, root: u64, kernel_stack: u64) {
        // SAFETY: Only previously CPU-validated selectors or initial flat selectors
        // are restored. The GDT is permanent, IF=0 and kernel has no TLS/per-GS data.
        unsafe {
            asm!("mov ds,{0:x}", "mov es,{1:x}", "mov fs,{2:x}", "mov gs,{3:x}",
                in(reg) self.selectors[0], in(reg) self.selectors[1],
                in(reg) self.selectors[2], in(reg) self.selectors[3], options(nostack,preserves_flags));
        }
        cpu::write_msr(0xc000_0100, self.bases[0]);
        cpu::write_msr(0xc000_0101, self.bases[1]);
        // SAFETY: Scheduler validated the user return frame; image, context and
        // stack remain pinned until a trap restores this exact kernel continuation.
        unsafe {
            anova_run_context(root, &self.frame, self.fpu.0.as_ptr(), kernel_stack);
        }
        cpu::write_msr(0xc000_0100, 0);
        cpu::write_msr(0xc000_0101, 0);
    }
}
