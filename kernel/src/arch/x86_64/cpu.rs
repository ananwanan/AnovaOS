// SPDX-License-Identifier: Apache-2.0
//! Privileged operations for the single BSP. No FPU/SIMD is enabled here.
use core::arch::asm;

pub fn flags() -> u64 {
    let flags;
    // SAFETY: Reading RFLAGS uses the current valid kernel stack.
    unsafe {
        asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags));
    }
    flags
}

pub fn disable_interrupts() {
    // SAFETY: This kernel runs at CPL0; callers preserve any required IF state.
    unsafe {
        asm!("cli", options(nomem, nostack));
    }
}

pub fn without_interrupts<T>(action: impl FnOnce() -> T) -> T {
    let previous = flags();
    disable_interrupts();
    let result = action();
    if previous & (1 << 9) != 0 {
        // SAFETY: Restore only the IF bit that was set on entry, after the action.
        unsafe {
            asm!("sti", options(nomem, nostack));
        }
    }
    result
}

pub fn wait_for_interrupt() {
    // SAFETY: Used only after GDT/TSS/IDT/APIC setup at CPL0. The STI shadow
    // prevents losing a wakeup between enabling interrupts and HLT.
    unsafe {
        asm!("sti", "hlt", options(nomem, nostack));
    }
}

pub fn in32(port: u16) -> u32 {
    let value;
    // SAFETY: CPL0; callers use validated ACPI PM timer or serialized PCI configuration ports.
    unsafe {
        asm!("in eax, dx", in("dx") port, out("eax") value, options(nomem, nostack, preserves_flags));
    }
    value
}

pub fn read_msr(index: u32) -> u64 {
    let (low, high): (u32, u32);
    // SAFETY: Callers check CPU features and use architectural APIC/EFER MSRs.
    unsafe {
        asm!("rdmsr", in("ecx") index, out("eax") low, out("edx") high, options(nomem, nostack, preserves_flags));
    }
    u64::from(low) | (u64::from(high) << 32)
}

pub fn write_msr(index: u32, value: u64) {
    // SAFETY: CPL0; callers preserve reserved bits and validate feature support.
    unsafe {
        asm!("wrmsr", in("ecx") index, in("eax") value as u32, in("edx") (value >> 32) as u32, options(nostack, preserves_flags));
    }
}
