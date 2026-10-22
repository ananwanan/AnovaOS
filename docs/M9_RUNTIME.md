<!-- Explain bounded Linux threading, signal, terminal and dynamic-musl contracts and their ownership rules. -->
# M9 Linux runtime profile

M9 extends the explicit Linux personality from M8. The native v1 ABI remains
separate. The Linux image identity is now `linux.x86_64.v1`; this is explicit
initramfs policy, not a guess from ELF OSABI. Validation is recorded in
[validation/M9.md](validation/M9.md).

## Ownership and scheduling

There are 32 scheduler slots in total, shared by processes, threads and zombies.
Each live task has its own 64 KiB guarded trap stack, GPR/x87/SSE2 context,
FS base, signal mask and pending signals. The BSP scheduler uses a separate
128 KiB guarded stack: debug ELF/exec paths retain more stack than the original
64 KiB M3 continuation. Terminal buffers are fallibly allocated outside that
stack. Kernel Rust still avoids SIMD, AVX remains disabled, and there is no SMP
or kernel preemption.

The measured musl clone profile requires CLONE_VM, CLONE_FS, CLONE_FILES,
CLONE_SIGHAND and CLONE_THREAD. SETTLS, PARENT_SETTID, CHILD_SETTID and
CHILD_CLEARTID are checked before publication. SYSVSEM shares the currently empty
semaphore namespace; DETACHED has Linux's ignored meaning. Unsupported clone
profiles fail. getpid returns the thread-group ID and gettid returns the real
scheduler TID. A plain clone with SIGCHLD and no supplied stack uses eager fork.

Threads share a reference-counted root/page owner, descriptor table, cwd and
signal dispositions. Fallible Shared control blocks report OOM without an allocation panic. Empty
descriptor placeholders allocate nothing; table inheritance allocates before
publishing open references. RefCell enforces exclusive mutation; scheduling and usercopy
run at IF=0, and no reference survives a switch. Fork copies private pages and
retains shared-anonymous backing. Closing a shared descriptor is visible to all
threads; leaving one thread releases the table only after its last owner exits.
Exec first loads the replacement and interpreter, then removes siblings,
transfers the leader identity, resets caught dispositions and closes CLOEXEC
slots on a detached table. Failed loading leaves the caller alive.

Futex WAIT/WAKE, WAIT_BITSET/WAKE_BITSET and REQUEUE/CMP_REQUEUE use actual
32-bit values. Compare and queue publication are atomic on the BSP. Private keys
use root identity; shared keys use never-reused backing-allocation identities and offsets. This permits
cross-process futexes on MAP_SHARED anonymous pages without conflating forked
private pages. Wait mismatches return EAGAIN, bad alignment EINVAL, bad pointers
EFAULT and elapsed deadlines ETIMEDOUT. Relative and absolute monotonic time
round upward to the 100 Hz timer. Requeue preserves waiter deadlines and masks.
PI futexes, robust lists, wall-clock futex deadlines and priority inheritance
remain unsupported.

Thread exit clears its checked clear-child-tid address and wakes a waiter before
releasing the shared root. exit_group terminates the whole group. The leader's
pthread_exit waits for remaining members; its parent receives final exit status
only after the group ends. Shared physical frames are released after the last
mapping owner and after lower tables are destroyed.

## Signals

The Linux AMD64 rt_sigaction/rt_sigprocmask, kill/tkill/tgkill, rt_sigsuspend and
rt_sigreturn boundaries use checked user copies. Dispositions are shared and
masks are task-local. KILL and STOP cannot be blocked or caught. Default stop and
continue retain task state; process-group addressing supports the terminal
foreground group. Default CHLD/URG/WINCH/CONT dispositions do not interrupt I/O.

Caught signals receive an actual user rt_sigframe with siginfo, ucontext,
restorer address, saved mask, GPRs and aligned FXSAVE state. Handler return enters
rt_sigreturn. The return path imports user edits, validates selectors and MXCSR,
checks the final instruction/stack mappings and sanitizes RFLAGS before IRETQ.
Nested handlers use the user stack with its red zone preserved. SIGINFO,
RESTORER, RESTART, NODEFER and RESETHAND are supported in the declared profile.
Alternate stacks, queued realtime signals, timers and general pthread
cancellation are not part of this profile.

Interruptible pipe/terminal I/O and wait return EINTR after a handler, or restart
the original syscall for SA_RESTART. Completed writev bytes remain completed.
Futex, poll/ppoll and nanosleep return EINTR; interrupted nanosleep can copy a
remaining monotonic duration. ppoll/sigsuspend restore the original mask through
the signal frame. Synchronous faults use Linux signal numbers and default group
termination while native faults retain their native diagnostic/status contract.

## Readiness and terminals

poll and ppoll inspect real descriptors, readiness, pipe EOF/HUP, errors and
invalid descriptors. Empty finite waits block and expire; negative poll fds are
ignored. O_NONBLOCK and F_GETFL/F_SETFL operate on the shared open description.
A zero-progress unavailable operation returns EAGAIN; partial writev returns its
completed byte count. Poll accepts at most 32 descriptors.

The existing polled COM1 terminal and four PTY pairs share a bounded line
discipline. Backend-neutral terminal events are translated to Linux signal
numbers only in the Linux adapter. /dev/ptmx allocates a master; TIOCGPTN identifies /dev/pts/0 through
/dev/pts/3 and TIOCSPTLCK implements the slave lock. Open descriptions determine
peer lifetime. Master/slave input and output are distinct queues. Canonical
input, echo, erase, EOF, a VMIN=1/VTIME=0 raw profile, TCGETS/TCSETS, window-size
queries/updates and foreground group queries/updates are provided. Signal
characters generate foreground INT/QUIT/TSTP, and master hangup generates HUP
and CONT. Session leaders can acquire a slave on open without O_NOCTTY.

This is a job-control foundation, not complete interactive ash job control.
WUNTRACED/WCONTINUED, PTY packet mode, arbitrary line disciplines, timed raw
reads, background TTOU/TTIN policy and full terminal session administration are
not claimed. UART and device completion are still polled with external IRQs
masked. PTY queues and canonical lines remain bounded.

## Dynamic musl and memory

The parser validates ET_EXEC and ET_DYN, segment lengths/overlaps, TLS and a
bounded absolute PT_INTERP path. The kernel loads the executable at its fixed
PIE bias and the real musl interpreter at a separate bias, then enters the
interpreter with the executable's AT_PHDR/AT_PHNUM/AT_ENTRY and correct AT_BASE.
The user-space musl loader performs symbol resolution and relocations; the
kernel does not substitute pre-relocated images.

Private file mappings read actual immutable initramfs backing without moving
fd offsets. Anonymous mappings are zeroed; MAP_SHARED anonymous backing survives
fork with reference-counted ownership. mprotect changes real PTE permissions,
PROT_NONE retains backing while denying CPL3 access, and W^X rejects simultaneous
write/execute. The mmap arena is 32 MiB, each request at most 8 MiB; ELF memory is
bounded separately. MAP_FIXED is confined to the mmap arena, refuses replacement of shared backing,
and has the measured brk-guard exception. New-page allocation rolls back before replacing existing
backing. There is no ASLR, COW or persistent-file mmap/writeback.

The fixture builds musl 1.2.4 static ET_EXEC, static PIE and PT_INTERP programs,
plus two real TLS shared objects. A dependency is linked at startup; another
TLS module is dlopened after worker threads already exist. A missing dlopen
library must produce a real error naming the missing file. Missing interpreters
fail exec transactionally with a path/error diagnostic. No glibc support or
unmeasured dynamic software is implied. No synthetic /proc data is installed:
these measured programs do not need it.

## Build and verification

Use the M8 host prerequisites, including musl-tools, make and qemu-user. The
runtime defaults to /usr/lib/x86_64-linux-musl/libc.so; ANOVA_MUSL_LIBC can select
a regular file from a locally extracted toolchain. The runtime must match the
pinned Ubuntu musl 1.2.4-2 SHA-256
`6e8ff8ad02f4a3a0b70029577e2eb9df8f6526aea302a60a8d649874d6fa45ea`. Exact
ELF/runtime hashes are recorded in build/apps/manifest.toml. Host traces and guest
logs are distinct evidence.

```sh
cargo xtask apps
cargo xtask trace-compat
cargo xtask test-compat
cargo xtask verify
cargo xtask terminal-test
cargo xtask persistence-test
```

Normal native shell entry points are linux-threads, linux-pie and linux-dynamic.
All probes remain independent user ELF programs with real syscall boundaries.
EOF, Ctrl+C and dynamic startup also require ordinary UART tests; boot markers
alone do not establish acceptance. Test-only negative/OOM workloads remain out
of ordinary startup.

Specification inputs include the [Linux AMD64 signal context](https://github.com/torvalds/linux/blob/master/arch/x86/include/uapi/asm/sigcontext.h),
[Linux futex UAPI](https://github.com/torvalds/linux/blob/master/include/uapi/linux/futex.h)
and [musl 1.2.4 source](https://git.musl-libc.org/cgit/musl/tree/?h=v1.2.4).
No Linux or musl implementation code is copied into the kernel.

A relocated musl toolchain must retain the shared libc linker input as well as
libc.a and headers. M10's build verifies that both TLS DSOs declare a real libc.so
DT_NEEDED dependency. Otherwise -shared can silently pull the static TLS accessor
from libc.a and fail dynamic TLS at runtime; this environment is now rejected.
