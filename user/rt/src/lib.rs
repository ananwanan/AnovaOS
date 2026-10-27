// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Native Rust CRT, startup arguments and checked process/file/durability syscall wrappers.
//! Built with the x86_64 user ABI and SSE2, without libc, allocation or Linux calls.
pub use anova_abi as abi;
mod memory;
pub mod process;
pub mod pty;
pub mod v1;
#[cfg(not(test))]
use core::arch::naked_asm;
use core::{arch::asm, marker::PhantomData};

#[cfg(not(test))]
unsafe extern "C" {
    fn anova_main(argc: usize, argv: *const *const u8) -> u32;
}

/// Native ELF process entry, called by IRETQ with argc/argv on a 16-byte stack.
/// # Safety
/// The loader must supply the documented mapped startup block and an RX entry.
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[cfg(not(test))]
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
pub fn sync() -> Result<(), i64> {
    // SAFETY: No pointer arguments; flush reports actual device completion or failure.
    result(unsafe { raw(abi::SYS_SYNC, 0, 0, 0) }).map(|_| ())
}
pub fn fsync(fd: u64) -> Result<(), i64> {
    // SAFETY: Descriptor is a scalar; the kernel checks ownership and target kind.
    result(unsafe { raw(abi::SYS_FSYNC, fd, 0, 0) }).map(|_| ())
}
pub fn shutdown() -> Result<(), i64> {
    // SAFETY: No pointers. Success stops writers, syncs storage and does not return.
    result(unsafe { raw(abi::SYS_SHUTDOWN, 0, 0, 0) }).map(|_| ())
}
pub fn set_executable(path: &str) -> Result<(), i64> {
    // SAFETY: The string stays readable throughout the bounded path copy.
    result(unsafe {
        raw(
            abi::SYS_SET_EXECUTABLE,
            path.as_ptr() as u64,
            path.len() as u64,
            0,
        )
    })
    .map(|_| ())
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
    // SAFETY: Descriptor close takes no user pointers.
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
    /// For count <=64, argv has count live pointers. Each string is readable through
    /// its first NUL or for at least 1024 bytes, and the array/strings remain valid and
    /// immutable for the borrow. Larger counts panic before dereferencing argv;
    /// strings without a NUL within the native 1023-byte limit panic during iteration.
    pub unsafe fn from_raw(count: usize, argv: *const *const u8) -> Self {
        assert!(count <= abi::MAX_ARGS, "Invalid native argument count");
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
            while length <= abi::MAX_ARGUMENT_BYTES && pointer.add(length).read() != 0 {
                length += 1;
            }
            assert!(
                length <= abi::MAX_ARGUMENT_BYTES,
                "Invalid native argument length"
            );
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

/// Read into exclusively owned memory. A zero result is EOF, including the final pipe close.
pub fn read(fd: u64, bytes: &mut [u8]) -> Result<usize, i64> {
    // SAFETY: Exclusive writable slice remains live while the process blocks.
    result(unsafe {
        raw(
            abi::SYS_READ,
            fd,
            bytes.as_mut_ptr() as u64,
            bytes.len() as u64,
        )
    })
    .map(|n| n as usize)
}
pub fn write_fd(fd: u64, bytes: &[u8]) -> Result<usize, i64> {
    // SAFETY: Immutable slice remains readable until the blocking write completes.
    result(unsafe {
        raw(
            abi::SYS_WRITE,
            fd,
            bytes.as_ptr() as u64,
            bytes.len() as u64,
        )
    })
    .map(|n| n as usize)
}
pub fn write_all(fd: u64, mut bytes: &[u8]) -> Result<(), i64> {
    while !bytes.is_empty() {
        let n = write_fd(fd, &bytes[..bytes.len().min(abi::MAX_WRITE)])?;
        if n == 0 {
            return Err(abi::INVALID_ARGUMENT);
        }
        bytes = &bytes[n..];
    }
    Ok(())
}
pub fn open(path: &str, flags: u64) -> Result<u64, i64> {
    // SAFETY: The length-delimited path stays live and the kernel only reads it.
    result(unsafe {
        raw(
            abi::SYS_OPEN,
            path.as_ptr() as u64,
            path.len() as u64,
            flags,
        )
    })
}
pub fn seek(fd: u64, offset: i64, whence: u64) -> Result<u64, i64> {
    // SAFETY: This call takes only scalar descriptor and offset values.
    result(unsafe { raw(abi::SYS_SEEK, fd, offset as u64, whence) })
}
pub fn dup2(source: u64, target: u64) -> Result<u64, i64> {
    // SAFETY: Descriptors are scalar process-local handles, never pointers.
    result(unsafe { raw(abi::SYS_DUP2, source, target, 0) })
}
pub fn pipe() -> Result<[u64; 2], i64> {
    let mut fds = [0; 2];
    // SAFETY: The initialized local is exactly the writable 16-byte result layout.
    result(unsafe { raw(abi::SYS_PIPE, fds.as_mut_ptr() as u64, 16, 0) })?;
    Ok(fds)
}
pub fn readdir(fd: u64) -> Result<Option<abi::DirEntry>, i64> {
    let mut entry = abi::DirEntry::default();
    // SAFETY: The exclusive initialized output has the ABI's exact size/alignment.
    let n = result(unsafe {
        raw(
            abi::SYS_READDIR,
            fd,
            &mut entry as *mut _ as u64,
            size_of::<abi::DirEntry>() as u64,
        )
    })?;
    Ok((n != 0).then_some(entry))
}
fn path_call(number: u64, path: &str) -> Result<(), i64> {
    // SAFETY: The immutable path remains readable throughout the call.
    result(unsafe { raw(number, path.as_ptr() as u64, path.len() as u64, 0) }).map(|_| ())
}
pub fn mkdir(path: &str) -> Result<(), i64> {
    path_call(abi::SYS_MKDIR, path)
}
pub fn chdir(path: &str) -> Result<(), i64> {
    path_call(abi::SYS_CHDIR, path)
}
pub fn unlink(path: &str) -> Result<(), i64> {
    path_call(abi::SYS_UNLINK, path)
}
pub fn getcwd(output: &mut [u8]) -> Result<usize, i64> {
    // SAFETY: The borrowed output is exclusively writable and remains live.
    result(unsafe {
        raw(
            abi::SYS_GETCWD,
            output.as_mut_ptr() as u64,
            output.len() as u64,
            0,
        )
    })
    .map(|n| n as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_arguments_accept_all_current_loader_limits_without_truncation() {
        let mut text = [b'x'; abi::MAX_ARGUMENT_BYTES + 1];
        text[abi::MAX_ARGUMENT_BYTES] = 0;
        let pointers = [text.as_ptr(); abi::MAX_ARGS];
        // SAFETY: All 64 immutable pointers reference the same live terminated 1023-byte string.
        let mut arguments = unsafe { Args::from_raw(pointers.len(), pointers.as_ptr()) };
        for _ in 0..abi::MAX_ARGS {
            assert_eq!(arguments.next().unwrap(), &text[..abi::MAX_ARGUMENT_BYTES]);
        }
        assert_eq!(arguments.next(), None);
        // SAFETY: Zero arguments access neither the null array nor any string pointer.
        assert_eq!(unsafe { Args::from_raw(0, core::ptr::null()) }.next(), None);
    }

    #[test]
    #[should_panic(expected = "Invalid native argument count")]
    fn native_arguments_reject_one_more_than_the_loader_count_limit() {
        // SAFETY: An excessive count is rejected before inspecting the deliberately null array.
        let _ = unsafe { Args::from_raw(abi::MAX_ARGS + 1, core::ptr::null()) };
    }

    #[test]
    #[should_panic(expected = "Invalid native argument length")]
    fn native_arguments_reject_one_more_than_the_loader_string_limit() {
        let mut text = [b'x'; abi::MAX_ARGUMENT_BYTES + 2];
        text[abi::MAX_ARGUMENT_BYTES + 1] = 0;
        let pointers = [text.as_ptr()];
        // SAFETY: The string remains immutable and has the required 1024 readable bytes.
        let _ = unsafe { Args::from_raw(1, pointers.as_ptr()) }.next();
    }
}
