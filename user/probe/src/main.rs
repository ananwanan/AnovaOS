// SPDX-License-Identifier: Apache-2.0
#![no_std]
#![no_main]
//! Trigger real Ring 3 syscall errors, protection faults and unsafe-return attempts.
//! The kernel acceptance runner supplies one named case and verifies its outcome.
use anova_rt::{self as rt, Args, abi::*};
use core::{arch::asm, panic::PanicInfo, sync::atomic::AtomicU64};
static DATA: AtomicU64 = AtomicU64::new(0);

fn syscalls() {
    #[repr(align(4096))]
    struct Buffer([u8; 8192]);
    let mut buffer = Buffer([b'A'; 8192]);
    let output = buffer.0.as_mut_ptr();
    // SAFETY: Successful queries use exclusively owned bytes. All deliberate
    // invalid addresses/lengths are cases that native v1 must reject before copy.
    unsafe {
        assert_eq!(rt::raw(9999, 0, 0, 0), UNSUPPORTED);
        assert_eq!(rt::raw(SYS_WRITE, 1, 0, 0), 0);
        assert_eq!(rt::raw(SYS_WRITE, 9, 0, 0), INVALID_ARGUMENT);
        for address in [
            0,
            0xffff_ffff_8000_0000,
            USER_LIMIT,
            u64::MAX,
            STACK_TOP - 16,
        ] {
            assert_eq!(rt::raw(SYS_WRITE, 1, address, 32), BAD_ADDRESS);
            assert_eq!(rt::raw(SYS_INFO, address, 32, 0), BAD_ADDRESS);
        }
        assert_eq!(rt::raw(SYS_WRITE, 1, output as u64, u64::MAX), TOO_LONG);
        assert_eq!(rt::raw(SYS_INFO, output as u64, 31, 0), INVALID_ARGUMENT);
        assert_eq!(
            rt::raw(SYS_INFO, anova_main as *const () as u64, 32, 0),
            BAD_ADDRESS
        );
        assert_eq!(rt::raw(SYS_INFO, output.add(4080) as u64, 32, 0), 0);
        let info = output.add(4080).cast::<SystemInfo>().read_unaligned();
        assert_eq!(info.abi_version, VERSION);
        assert_eq!(info.page_size, 4096);
        assert_eq!(rt::raw(SYS_WRITE, 1, output.add(100) as u64, 4), 4);
        // DF is masked on kernel entry, restored only as a user flag. Copying
        // must still advance forwards, then CLD restores the compiler's ABI.
        let result: i64;
        asm!("std","syscall","cld",inlateout("rax") SYS_INFO=>result,
            in("rdi") output as u64,in("rsi") 32_u64,in("rdx") 0_u64,
            lateout("rcx") _,lateout("r11") _,options(nostack));
        assert_eq!(result, 0);
    }
    rt::write(b"\n[APP:PROBE] pointers/errors/cross-page/DF passed\n");
}

#[unsafe(no_mangle)]
unsafe extern "C" fn anova_main(argc: usize, argv: *const *const u8) -> u32 {
    // SAFETY: CRT arguments obey the loader's lifetime and bound requirements.
    let case = unsafe { Args::from_raw(argc, argv) }.nth(1).unwrap();
    match case {
        b"syscalls" => {
            syscalls();
            return 0;
        }
        b"timer" => {
            let start = rt::info().unwrap().ticks;
            while rt::info().unwrap().ticks == start {
                core::hint::spin_loop();
            }
            return 0;
        }
        // SAFETY: Intentional invalid operations occur only in this isolated
        // user ELF. Assembly avoids creating invalid Rust references/pointers
        // with dereference contracts; the kernel must contain the CPU fault.
        _ => unsafe {
            match case {
                b"page-fault" => {
                    asm!("mov al,byte ptr [rax]",inout("rax") 0_u64=>_,options(nostack))
                }
                b"kernel-read" => {
                    asm!("mov al,byte ptr [rax]",inout("rax") 0xffff_ffff_8000_0000_u64=>_,options(nostack))
                }
                b"kernel-write" => {
                    asm!("mov byte ptr [rax],0",in("rax") 0xffff_ffff_8000_0000_u64,options(nostack))
                }
                b"stack-guard" => {
                    asm!("mov al,byte ptr [rax]",inout("rax") (STACK_TOP-STACK_BYTES-1)=>_,options(nostack))
                }
                b"invalid-opcode" => asm!("ud2", options(noreturn)),
                b"privileged" => asm!("cli", options(nostack)),
                b"read-only" => {
                    asm!("mov byte ptr [rax],0",in("rax") anova_main as *const (),options(nostack))
                }
                b"no-execute" => asm!("call rax",in("rax") &DATA),
                b"bad-rsp" => {
                    asm!("mov rsp,-1","syscall","ud2",in("rax") 9999_u64,options(noreturn))
                }
                _ => {}
            }
        },
    }
    99
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    rt::write(b"[APP:FAIL] native probe\n");
    rt::exit(101)
}
