// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Native Rust CRT, borrowed startup arguments and console/process syscall wrappers.
//! Built with the x86_64 user ABI and SSE2, without libc, allocation or Linux calls.
pub use anova_abi as abi;
mod memory;
use core::{
    arch::{asm, naked_asm},
    marker::PhantomData,
};

unsafe extern "C" {
    fn anova_main(argc: usize, argv: *const *const u8) -> u32;
}

/// Native ELF process entry, called by IRETQ with argc/argv on a 16-byte stack.
/// # Safety
/// The loader must supply the documented mapped startup block and an RX entry.
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start() -> ! {
    // SAFETY: The loader supplies bounded argc, argv and null-terminated strings.
    // Align before CALL; the native main return status crosses SYS_EXIT only.
    naked_asm!("xor ebp,ebp","mov rdi,[rsp]","lea rsi,[rsp+8]","and rsp,-16",
        "call {main}","mov edi,eax","mov eax,1","syscall","ud2",main=sym anova_main);
}

/// Make a native syscall with up to three arguments; RCX/R11 and flags are clobbered.
/// # Safety
/// Successful pointer-taking calls require live, appropriately owned buffers.
/// Deliberately invalid pointers may only be used when the ABI guarantees rejection.
pub unsafe fn raw(number: u64, a: u64, b: u64, c: u64) -> i64 {
    let result: i64;
    // SAFETY: Caller meets the buffer/aliasing requirements; no user stack is
    // accessed by SYSCALL itself and the kernel preserves all non-clobbered GPRs.
    unsafe {
        asm!("syscall",inlateout("rax") number=>result,in("rdi") a,in("rsi") b,
        in("rdx") c,lateout("rcx") _,lateout("r11") _,options(nostack));
    }
    result
}

pub fn write(bytes: &[u8]) -> i64 {
    // SAFETY: The borrowed slice remains readable and no syscall writes to it.
    unsafe { raw(abi::SYS_WRITE, 1, bytes.as_ptr() as u64, bytes.len() as u64) }
}
pub fn info() -> Result<abi::SystemInfo, i64> {
    let mut value = abi::SystemInfo::default();
    // SAFETY: The exclusive local is correctly sized, initialized and writable.
    let result = unsafe {
        raw(
            abi::SYS_INFO,
            &mut value as *mut _ as u64,
            size_of::<abi::SystemInfo>() as u64,
            0,
        )
    };
    if result == 0 { Ok(value) } else { Err(result) }
}
pub fn exit(status: u32) -> ! {
    // SAFETY: SYS_EXIT takes no pointers and cannot return on a supported ABI.
    unsafe {
        raw(abi::SYS_EXIT, u64::from(status), 0, 0);
        asm!("ud2", options(noreturn));
    }
}

fn result(value: i64) -> Result<u64, i64> {
    if value < 0 {
        Err(value)
    } else {
        Ok(value as u64)
    }
}
pub fn yield_now() {
    // SAFETY: No pointers; the kernel preserves the caller's live context.
    unsafe {
        raw(abi::SYS_YIELD, 0, 0, 0);
    }
}
/// Block for at least this many delivered 100 Hz timer ticks; zero yields.
pub fn sleep(ticks: u64) -> Result<(), i64> {
    // SAFETY: No pointers or borrowed resources cross into the kernel.
    result(unsafe { raw(abi::SYS_SLEEP, ticks, 0, 0) }).map(|_| ())
}
fn image_call(number: u64, name: &str, argument: &str) -> Result<u64, i64> {
    let request = abi::ExecRequest {
        name: name.as_ptr() as u64,
        name_len: name.len() as u64,
        argument: argument.as_ptr() as u64,
        argument_len: argument.len() as u64,
    };
    // SAFETY: The request and both strings remain readable until the call returns.
    result(unsafe {
        raw(
            number,
            &request as *const _ as u64,
            size_of::<abi::ExecRequest>() as u64,
            0,
        )
    })
}
pub fn spawn(name: &str, argument: &str) -> Result<u64, i64> {
    image_call(abi::SYS_SPAWN, name, argument)
}
/// A successful exec replaces this entire image and never returns.
pub fn exec(name: &str, argument: &str) -> Result<(), i64> {
    image_call(abi::SYS_EXEC, name, argument).map(|_| ())
}
/// Duplicate this single-threaded process; caller must account for execution in both copies.
/// # Safety
/// Copied external resources must tolerate two independent process owners. The
/// native SDK has no threads, shared allocator or host runtime requiring fork hooks.
pub unsafe fn fork() -> Result<u64, i64> {
    // SAFETY: The caller meets the documented independent-owner contract.
    result(unsafe { raw(abi::SYS_FORK, 0, 0, 0) })
}
/// pid=0 waits for any child; a failed copy leaves the child's status waitable.
pub fn wait(pid: u64) -> Result<abi::WaitStatus, i64> {
    let mut status = abi::WaitStatus::default();
    // SAFETY: A live exclusive 16-byte output remains mapped while the caller blocks.
    result(unsafe {
        raw(
            abi::SYS_WAIT,
            pid,
            &mut status as *mut _ as u64,
            size_of::<abi::WaitStatus>() as u64,
        )
    })?;
    Ok(status)
}
pub fn close(fd: u64) -> Result<(), i64> {
    // SAFETY: Console descriptors carry no user pointers.
    result(unsafe { raw(abi::SYS_CLOSE, fd, 0, 0) }).map(|_| ())
}

pub struct Args<'a> {
    argv: *const *const u8,
    count: usize,
    index: usize,
    lifetime: PhantomData<&'a [u8]>,
}
impl Args<'_> {
    /// Borrow startup strings whose storage remains unchanged for the borrow.
    /// # Safety
    /// argv has count live pointers, each to a NUL-terminated string <=128 bytes;
    /// count <=16 and the pointer array/strings remain valid and immutable.
    pub unsafe fn from_raw(count: usize, argv: *const *const u8) -> Self {
        assert!(count <= 16, "Invalid native argument count");
        Self {
            argv,
            count,
            index: 0,
            lifetime: PhantomData,
        }
    }
}
impl<'a> Iterator for Args<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.count {
            return None;
        }
        // SAFETY: from_raw requires a live pointer array and bounded immutable
        // strings for 'a; this iterator visits each in-range pointer only once.
        unsafe {
            let pointer = self.argv.add(self.index).read();
            self.index += 1;
            let mut length = 0;
            while length < 128 && pointer.add(length).read() != 0 {
                length += 1;
            }
            Some(core::slice::from_raw_parts(pointer, length))
        }
    }
}

pub fn check_cpu() -> bool {
    let (cs, flags, simd): (u16, u64, u64);
    let mut control = 0_u16;
    let mut mxcsr = 0_u32;
    let mut x87 = 0_u64;
    // SAFETY: These unprivileged user instructions use valid local outputs.
    // Explicit XMM clobbers prevent compiler assumptions about SIMD contents.
    unsafe {
        asm!("mov {0:x},cs",out(reg) cs,options(nomem,nostack,preserves_flags));
        asm!("pushfq","pop {}",out(reg) flags);
        asm!("fnstcw [{}]",in(reg) &mut control,options(nostack));
        asm!("stmxcsr [{}]",in(reg) &mut mxcsr,options(nostack));
        asm!("pxor xmm0,xmm0","pcmpeqd xmm1,xmm1","paddq xmm0,xmm1","movq {},xmm0",
            out(reg) simd,out("xmm0") _,out("xmm1") _,options(nomem,nostack));
        asm!("fld1","fld1","faddp","fstp qword ptr [{}]",in(reg) &mut x87,options(nostack));
    }
    cs & 3 == 3
        && flags & 0x3200 == 0x200
        && control == 0x37f
        && mxcsr == 0x1f80
        && simd == u64::MAX
        && x87 == 0x4000_0000_0000_0000
}
