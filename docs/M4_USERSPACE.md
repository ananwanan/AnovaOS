# M4 isolated native applications

This document defines the implemented Ring 3 execution, ELF loader, native v1
ABI and Rust/C SDK boundaries. See [validation/M4.md](validation/M4.md) for
measured acceptance results and [M3_MEMORY.md](M3_MEMORY.md) for backing ownership.

## Execution and ownership

M4 runs one explicitly selected native application at a time on the single BSP.
Each invocation owns a private PML4, its lower-half table hierarchy, zeroed user
pages and a separate 64 KiB supervisor kernel stack with two absent guards. It
borrows only supervisor upper-half kernel table paths. The lower kernel root
entries are not copied. All user ancestors carry U/S; imported kernel ancestors
remain supervisor-only. Kernel mappings stay stable until the process is gone.

Application entry uses `IRETQ` with CS `0x33`, SS `0x2b`, IF=1 and IOPL=0. Every
GPR is initially cleared. TSS.RSP0 points to the invocation's kernel stack;
existing ISTs remain independent. User timer IRQs return to the same context.
There is no scheduler or simultaneous second application in M4.

On exit or a user #PF/#GP/#UD and other synchronous exception, the kernel records
an outcome, restores the original CR3 and suspended kernel stack, then releases
the application's tables, data and kernel stack. The return transfer crosses no
held lock or owned Rust local in the dispatch path. NMI, double fault and machine
check remain fatal kernel diagnostics, even if they interrupted user execution.
Every acceptance invocation checks that the physical free-frame count returns
to its baseline, including failed partial loads.

## ELF and startup layout

Only little-endian x86_64 ELF64 ET_EXEC is accepted. File size is bounded at
8 MiB, program headers at 32, PT_LOAD segments at 16 and rounded load pages at
4 MiB total. Load addresses must lie in `[0x10000, 0x40000000)`. The parser checks
header/version/size, checked file and virtual bounds, power-of-two/congruent
alignment, filesz <= memsz, ascending load addresses and page-rounded overlap.
It requires readable segments, rejects W+X and requires the entry in file-backed
executable bytes. Zeroed backing supplies BSS and partial-page padding.

PT_DYNAMIC, PT_INTERP, PT_TLS, unknown runtime headers and executable GNU_STACK
are rejected. NULL/NOTE/PHDR and non-executable GNU_STACK are metadata only.
There is no dynamic linking, relocation, TLS, PIE, libc or Linux personality.

The 64 KiB RW/NX user stack ends at `0x7fffffff0000` and has absent pages on both
sides. At the initial 16-byte-aligned RSP it contains:

| Word | Meaning |
| --- | --- |
| 0 | argc, from 1 through 16 |
| 1 through argc | Pointers to NUL-terminated UTF-8 argument bytes |
| argc + 1 | NULL argv terminator |
| argc + 2 | NULL envp; environment is currently empty |

Arguments are at most 128 bytes each, with no embedded NUL. Strings and the
pointer table fit in the top stack page. There is no Linux auxv. This startup
layout is native v1, not a claim of Linux process-startup compatibility.

## Native v1 syscall ABI

The image inventory and Limine module string explicitly select
`anova.native.v1:<name>`. ELF OSABI never selects a personality. `xtask apps`
validates the actual ELF files and writes `build/apps/manifest.toml` with ABI
identity and SHA-256. The ISO carries that inventory beside the original ELF
files; Limine loads them as retained modules. A filesystem is not required yet.

User code issues the actual x86_64 `SYSCALL` instruction: RAX is the syscall
number; RDI/RSI/RDX are arguments; RAX contains a signed result. RCX, R11 and
condition flags are clobbered, while other GPRs survive returning calls.

| Number | Call | Arguments | Result |
| --- | --- | --- | --- |
| 0 | write | fd (1 or 2), bytes, length <=4096 | Byte count, or error |
| 1 | exit | u32 status | Ends this invocation; status is retained |
| 2 | info | writable buffer, exact size 32 | 0, or error |
| Other | Unsupported | Ignored | -2 |

`write` is bounded, polling COM1 output, not a general fd/VFS service. It first
copies the complete validated range into a kernel buffer, so a rejected copy
does not print a partial user buffer. LF becomes CRLF at COM1. Empty writes do
not dereference their pointer. Info fields are serialized explicitly:

| Offset | Width | Field |
| --- | --- | --- |
| 0 | u32 | ABI version = 1 |
| 4 | u32 | Structure size = 32 |
| 8 | u64 | Page size = 4096 |
| 16 | u64 | Coarse delivered LAPIC tick count |
| 24 | u64 | Current invocation's process identifier |

All fields are little-endian; alignment is 8 and no padding is exposed.
Error -1 means bad address, -2 unsupported syscall, -3 invalid argument and -4
oversized write. These are native errors, not Linux errno assignments. Headers
and layout assertions are in `crates/abi` and `user/c/anova.h`.

## Entry, return and user copying

SYSCALL does not switch stacks. The entry stores user RSP only into a permanent
supervisor scratch slot and immediately loads the trusted process stack before
any push or call. STAR/LSTAR/EFER are configured explicitly. FMASK clears IF,
TF, DF, IOPL, NT and AC. The entry synthesizes the same saved frame as the trap
assembly and returns through `IRETQ`, not SYSRET. Before returning, the kernel
checks executable user RIP and writable user stack storage, fixes CS/SS and
keeps only arithmetic/DF flags plus forced IF/reserved bit. Invalid return state
ends the process on the kernel stack.

The scratch state is BSP-only with IF=0 on entry; FS/GS bases are zero and
FSGSBASE is disabled. No GS state or SWAPGS transition is used. This is not an
SMP or nested-task entry design.

`UserAddr` validates nonempty lower-canonical ranges and arithmetic overflow.
Copies walk every page and check effective U/S and read/write permission.
Untrusted pointers never become Rust references. The actual assembly copy has
one recoverable REP MOVSB site: a kernel #PF is fixed up only if CS, fault RIP,
access direction, CR2 range and the armed guard match, with reserved/user/fetch
errors rejected. Any other kernel fault remains fatal. A copy may have written
part of its destination if a real access-time fault occurs; callers must treat
an error accordingly.

IF stays clear through checking and copying, so no unmap can race on this BSP.
Before M5/SMP introduces concurrent revocation, replace this assumption with
address-space synchronization/pinning and the corresponding tests. No IRQ or
fault-recovery path acquires the memory manager or heap lock.

## CPU state and SDK

Before user entry, CPUID must report SYSCALL, FXSR, SSE and SSE2. CR0 enables MP
and NE and clears EM/TS; CR4 enables OSFXSR/OSXMMEXCPT and leaves XSAVE/AVX and
FSGSBASE disabled. An aligned zeroed FXSAVE image resets x87/MMX and all XMM
payloads for every invocation, with FCW `0x037f` and MXCSR `0x1f80`. Kernel code
continues to use the bare-metal soft-float/no-SIMD target. M5 must save and restore
enabled extended state before switching tasks; resetting it is not context switching.

`user/` is a separate Cargo workspace. Rust uses the pinned compiler's
`x86_64-unknown-linux-gnu` core solely for normal x86_64/SSE2 user code generation;
it is `no_std`, uses its own CRT/linker and links no host libc or Linux syscalls.
The linker discards unwind metadata/personality references because panics abort
via native exit; unwinding is not supported. The runtime supplies freestanding
memory intrinsics. C uses GCC with freestanding, static, no-PIE/no-stack-protector
flags and the same user linker script. No kernel target flags reach either app.

`hello-rust` verifies Ring 3, arguments, BSS, default FPU controls, x87 and SSE2,
then exits 0. `hello-c` verifies Ring 3, arguments, BSS and SSE2, then exits 7.
`probe` is an isolated acceptance program for negative cases and timer delivery.
Build with `cargo xtask apps`, lint Rust apps with `cargo xtask check-apps`, then
run `cargo xtask smoke` or the complete `cargo xtask verify` gate.

M4 does not provide scheduling, spawn/wait/fork, files, input, persistent
storage, networking, a general C library or compiler execution in the guest.

## Primary references

- [Intel SDM Volume 3A](https://cdrdv2-public.intel.com/874249/253668-090-sdm-vol-3a.pdf):
  long-mode privilege, paging, interrupt stack transitions and SYSCALL MSRs.
- [Intel instruction reference](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2b-manual.pdf):
  SYSCALL, IRETQ and FXSAVE/FXRSTOR instruction behavior.
- [System V gABI program loading](https://gabi.xinuos.com/elf/08-pheader.html):
  PT_LOAD sizes, alignment and zero initialization.
