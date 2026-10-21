# M5 native processes and scheduling

This document defines implemented BSP scheduling, CPU/FPU ownership, native
process syscalls and their limits. [validation/M5.md](validation/M5.md) records
measured evidence. It supersedes the sequential lifecycle in
[M4_USERSPACE.md](M4_USERSPACE.md); the M4 ELF/entry/copy ABI remains applicable.

## Process and execution model

The kernel supports 32 process slots, including unreaped zombies, with one
thread per process. Each owns a private user PML4/lower tables, eagerly backed
user pages, a unique guarded 64 KiB kernel stack, saved user context and console
descriptor table. A monotonic positive PID identifies a process during a
scheduler session; slot reuse never reuses its PID. The boot runner starts
separate sessions for historical isolation probes, then a multiprocess session.

`anova-sched` owns allocation-free state/parent policy; `kernel/task` owns
resources and activation. States are Empty, Ready, Running, Sleeping(deadline),
Waiting(child-or-any) and Zombie(status). No thread creation or shared user
address space exists. There is no SMP or kernel-thread API.

The trusted scheduler retains its permanent root and BSP stack. For each resume:

1. Validate executable RIP, writable user stack, selectors and permitted flags.
2. Set TSS.RSP0 and the SYSCALL stack pointer to this process's kernel stack.
3. Restore segments/bases, the aligned 512-byte FPU image, private CR3 and GPRs.
4. IRETQ into Ring 3. Syscall, synchronous user fault or timer IRQ captures a
   value snapshot into a separate trusted buffer, then restores the scheduler
   continuation/root. No process/context borrow is overwritten during capture.
5. Handle syscalls, blocking, allocation or teardown on the scheduler stack.

The trap tail transfer abandons only a bounded entry stack with no owned Rust
locals or held locks. Kernel stacks are never copied by fork and never freed
while active. Fatal kernel faults, NMI, machine check and double fault retain
independent IST/diagnostic behavior. Kernel code remains SIMD-free.

## CPU state and preemption

Saved state includes the 15 GPRs, user RIP/RSP/RFLAGS, selectors, DS/ES/FS/GS,
FS/GS bases and the enabled x87/MMX/SSE2 environment, all 16 XMM registers and
MXCSR. Fresh spawn/exec images start with cleared GPR/FPU payloads, FCW 0x037f
and MXCSR 0x1f80; fork inherits the captured context with child RAX=0.
FXSAVE64/FXRSTOR64 use a 16-byte aligned 512-byte area. EFER.FFXSR is cleared
so CPL0 cannot omit XMM registers. XSAVE/AVX and FSGSBASE remain disabled;
additional architectural state must be implemented before it is enabled.

The existing calibrated LAPIC timer delivers nominal 100 Hz ticks. A user-mode
tick sends exactly one EOI, captures context without allocation/locks/logging,
and rotates the ready queue. The quantum is one delivered tick. Explicit yield
also rotates; a normal nonblocking syscall continues the same process until a
timer interrupt or blocking operation. Host policy tests bound round-robin
starvation for continuously ready peers.

Kernel processing is non-preemptible with IF=0. The time API counts delivered
IRQs, so long kernel sections can delay/coalesce ticks; it is neither wall time
nor a real-time guarantee. `sleep(n)` uses a checked absolute deadline in these
same ticks; zero yields. Sleeping and waiting processes do not poll user code.
When no process is ready, the scheduler uses STI/HLT and disables IF again before
examining state. The STI shadow prevents a lost interrupt between test and halt.

## Address-space and lock contract

Only the BSP scheduler can mutate user mappings. Each single-threaded process
exclusively owns its root; no syscall unmaps a running sibling's memory. User
copies execute inside an IF=0 address-space activation critical section and
restore the permanent root before allocation/exec/fork/teardown. Copy checking
and REP MOVSB cannot span a context switch; the exact M4 #PF fixup is retained.
Thus the active root and pages are pinned for the complete copy by exclusive
ownership and disabled local scheduling, rather than a shared reference count.
Shared address spaces, concurrent revocation, preemptible kernel paths or SMP
must add a corresponding explicit pin/lock protocol before use.

Upper kernel mappings are supervisor-only and borrowed. Stack bank addresses
are `0xffffd00001000000 + slot * (64 KiB + 8 KiB)` with absent lower/upper guards;
the 64 KiB payload starts one page later. All slots share the already retained
managed upper hierarchy. Kernel map edits happen only on the permanent root at
IF=0; no other CPU or active process can access an edited path. CR3 reloads
invalidate translations before private pages or empty tables are reused.

There is no scheduler spinlock: only the BSP owns its state while IF=0. Existing
memory-manager-before-heap ordering remains. IRQ capture takes neither lock.
Allocation occurs only after returning to the scheduler, and no lock is held
while waiting or running a user process.

## Native v1 extensions

These are additive native ABI calls, not Linux syscall numbers. Numbers 0–2 and
the original error meanings remain unchanged. Syscall inputs use RAX and
RDI/RSI/RDX; RCX/R11 are clobbered, results use RAX with negative native errors.

| Number | Call | Inputs | Result |
| --- | --- | --- | --- |
| 3 | yield | none | 0 after a scheduling opportunity |
| 4 | sleep | ticks | 0 after deadline; overflow returns INVALID_ARGUMENT |
| 5 | spawn | request pointer, size=32 | Child PID, or error |
| 6 | exec | request pointer, size=32 | Success replaces image and never returns |
| 7 | wait | child PID (0=any), output pointer, size=16 | Reaped PID, or error |
| 8 | fork | none | Parent: child PID; child: 0 |
| 9 | close | descriptor | 0, or BAD_DESCRIPTOR |

`ExecRequest` contains four little-endian u64 fields: name pointer/length,
argument pointer/length. Name is 1..128 UTF-8 bytes; argument is 0..128 UTF-8
bytes. Neither may contain NUL. On success argc=2, argv=[name, argument], with
an empty environment. Names select explicit native modules `hello-rust`,
`hello-c`, `probe`, `process-test`; they are not filesystem paths. Unknown names
return NOT_FOUND. Requests, nested pointers, sizes and ranges are checked and
copied before image loading. The native module personality remains explicit.

`WaitStatus` is 16 bytes: u64 PID, u32 reason, u32 code. Reason 0 carries the exit
status; reason 1 carries the CPU exception vector; reason 2 indicates rejected
user return state. Fault address/error diagnostics remain on serial. A caller
may wait only for its children; no eligible child returns NO_CHILD. Invalid
output returns BAD_ADDRESS without consuming a zombie. A sleeping parent is
readied on its matching child's exit and completes the copy on its next turn.

New errors are NO_CHILD=-5, NO_MEMORY=-6, NOT_FOUND=-7, PROCESS_LIMIT=-8 and
BAD_DESCRIPTOR=-9. Legacy out-of-domain console write fd values still return
INVALID_ARGUMENT; a closed valid console slot returns BAD_DESCRIPTOR.

## Lifecycle and descriptors

Spawn fully loads a child before publishing resources. Eager fork copies every
user page and its effective W/X permissions into independent frames; it copies
saved CPU/FPU and descriptor state. Partial allocation failure rolls back the
new root, leaves, tables, stack and slot; the parent continues. COW is pending.

Exec builds an inactive replacement first. Failed validation/allocation retains
old memory, context, PID, parent and descriptors. Success resets context/BSS/
arguments, replaces private memory and preserves PID, parent, kernel stack and
descriptor table. There is no close-on-exec flag yet.

The descriptor table currently owns only stdout=1 and stderr=2 console sinks;
stdin is unavailable. Spawn starts with those defaults, fork copies the open
slots, exec retains them, close clears one process's slot, and exit clears all.
The console has no file offset or heap-backed open-file object. VFS, open/dup,
shared offsets, pipes, input and general descriptor reference counting are M6.

Exit and contained user faults immediately reclaim inactive image pages/tables,
kernel stack and descriptor ownership on the scheduler root. Only status and
small bookkeeping remain in a zombie. Parent wait consumes it once. When a
parent dies its children are adopted by kernel PID 0; orphan zombies are reaped
automatically, including children still sleeping/running at adoption. A user
init process and durable system process service are M6 work.

## Verification and specification basis

`cargo xtask smoke` runs the M4 suite and actual independent process-test ELF.
The process suite covers all XMM lanes/GPR sentinels with no syscalls in the busy
loops, inherited fork FP controls, private memory/descriptor state, exec reset,
wait-any/target/error, child #UD, sleep/HLT, orphan reaping, repeated create/exit,
32-slot exhaustion/reuse and three partial-allocation OOM rollbacks. Frame and
heap usage must return exactly to their pre-run values. The host classifier
requires the counters and cases, not merely a banner. Full `verify` retains all
13 CPU/memory/failure images. See [M5 evidence](validation/M5.md).

CPU behavior follows the [Intel SDM](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)
and AMD64 Architecture Programmer's Manual Volume 2, publication 24593 (media/
x87 state and EFER.FFXSR), from [AMD's official documentation](https://docs.amd.com/).
No third-party scheduler implementation was imported and no dependency was added
outside the workspace.
