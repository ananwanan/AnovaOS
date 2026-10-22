<!-- Define the implemented static Linux personality, exact compatibility scope and validation entry points. -->
# M8 static Linux CLI

This is the historical M8 profile. [M9_RUNTIME](M9_RUNTIME.md) supersedes its
single-thread, custom-signal, terminal and dynamic-loading limitations. Current
Linux identity is linux.x86_64.v1; the native ABI remains independent.
M11 adds offline guest C/C++ development and supersedes startup, ELF and
memory capacities; see [M11_TOOLCHAIN](M11_TOOLCHAIN.md) for current bounds.

M8 runs a deliberately bounded Linux AMD64 personality alongside native v1.
Acceptance evidence is in [validation/M8.md](validation/M8.md). This is not a
Linux kernel, a distribution ABI, general POSIX certification or guest compilation.

## Image identity and startup

Archive kind 5 explicitly means `linux.x86_64.static.v1`; kind 4 remains native.
The VFS follows symlinks before looking up this policy. ELF OSABI, a pathname
suffix or absence of PT_INTERP never chooses the personality. The persistent
volume's existing `.anova-abi` still selects native v1: copying a Linux executable
onto that volume does not convert its policy or make it executable as Linux.

Both loaders require checked x86_64 ET_EXEC, bounded PT_LOAD segments, private
zeroed pages, an executable file-backed entry and W^X. Linux additionally permits
validated static PT_TLS and GNU_RELRO metadata. Program headers must be inside
a loadable segment. musl initializes its own static TLS using AT_PHDR; this is
not dynamic TLS support. GNU_RELRO is accepted as metadata but not write-protected.
PT_INTERP, PT_DYNAMIC, ET_DYN/static PIE, executable stacks and unknown program
header types remain rejected.

Linux RSP is 16-byte aligned with argc, argv, NULL, envp, NULL and terminated
auxv. Aux entries include PHDR/PHENT/PHNUM, PAGESZ, ENTRY, BASE=0, FLAGS=0,
root UID/EUID/GID/EGID, SECURE=0 and 16 bytes for AT_RANDOM. argv/envp each allow
16 strings of at most 128 UTF-8 bytes and share the final 4 KiB stack page;
requests that do not fit fail. exec copies nested pointers before loading a
replacement; bad paths, vectors, ELF and allocation leave the caller intact.

RAX selects Linux calls; arguments use RDI, RSI, RDX, R10, R8, R9. A distinct
scheduler dispatch prevents Linux numbers 0/1 from entering native write/exit.
Linux errors are negative x86_64 errno values. Unknown numbers return ENOSYS.
Only `kernel/task/linux.rs` and `anova-abi::linux` translate Linux details.
VFS, physical memory and scheduler policy retain backend-neutral interfaces.

## Implemented calls and bounds

| Calls | Supported contract |
| --- | --- |
| read/write/writev, close/lseek | Checked buffers, partial transfers, VFS offsets, EOF and blocking pipes; individual transfers up to 4 KiB, writev up to 16 vectors |
| open/openat | Read/write/create/exclusive/truncate/append/directory/largefile/CLOEXEC; openat currently requires AT_FDCWD |
| stat/fstat/lstat/newfstatat | Explicit 144-byte AMD64 serialization; stable virtual inode, type, size, fixed backend permissions, zero timestamps; newfstatat requires AT_FDCWD and flags=0 |
| getdents64 | One real entry per call, aligned variable-length record; too-small or bad user buffers do not consume it; virtual directories omit dot entries |
| getcwd/chdir/mkdir/unlink/access | Existing bounded UTF-8 paths and backend semantics; access conservatively rejects write checks |
| dup2/pipe/pipe2/fcntl | Shared descriptions; pipe2 flags=0; fcntl DUPFD, GETFD, SETFD and DUPFD_CLOEXEC; descriptor-local CLOEXEC, fork inheritance and real close on successful exec |
| fsync/sync | Existing actual disk durability paths; failures are propagated |
| brk | Eager private data pages within an 8 MiB arena at 0x40000000; failed growth returns the old break; shrink releases leaves |
| mmap/munmap | Private anonymous RW pages in an 8 MiB arena at 0x50000000; freed gaps can be reused; partial allocation rolls back; no shared/file mapping or executable memory |
| mmap guard | musl's anonymous PROT_NONE/MAP_FIXED request only inside its allocated brk arena; it actually removes access to those pages |
| arch_prctl | SET_FS and GET_FS; lower canonical base, copied output; FS survives syscall, preemption and fork, then resets on exec |
| fork/execve/exit/exit_group/wait4 | Eager fork and inherited fd/cwd; bounded argv/envp; Linux 4-byte status; targeted/any-child wait, WNOHANG and NULL status; rusage/process-group waits unsupported |
| getpid/getppid/gettid and identity queries | One thread per process and a single root identity; no credential changes |
| set_tid_address | Register clear-child-tid, return real PID, best-effort checked clear on exit; no thread/futex wake is claimed |
| clock_gettime/nanosleep | CLOCK_MONOTONIC in delivered 100 Hz IRQ time; checked relative sleep rounds upward to ticks; no wall clock or signal-handler restart support |
| getrandom | Up to 4 KiB using CPUID-checked RDRAND and bounded retries; unsupported flags/faults/hardware failure return errors; no deterministic fallback |
| uname | Names AnovaOS and its actual architecture, without claiming to be a Linux kernel |

A native parent waiting on a Linux child can now receive native ExitStatus reason
3 with the terminating signal number; the native wait layout is unchanged.

The namespace has no general permission/credential model. New Linux files are
accepted only when the requested mode after fixed 022 masking is 0644; mkdir
requires the corresponding 0755 profile. Other creation modes return EOPNOTSUPP.
Existing-file open ignores the unused creation mode. This does not implement
chmod, umask, access-control enforcement, file timestamps or a real-time clock.
Character-device truncation leaves the device intact. ioctls report ENOTTY
(or EBADF for an invalid fd); termios/PTY interfaces are not implemented.

The QEMU baseline is now `qemu64,+rdrand` under TCG. Linux loading requires
AT_RANDOM from the advertised hardware source. A machine without that source
cannot load these images; it is not silently seeded with a counter or address.

## Signals and terminal limitations

M8 implements signal masks, default/ignore disposition queries/updates, pending
blocked SIGPIPE, default SIGPIPE termination and signal-coded wait status.
`kill` accepts positive live PIDs and signal 0 or the limited default/ignore
signals KILL/PIPE/TERM. KILL cannot be masked or ignored. Ignored pending signals
are discarded; unmasked fatal pending signals terminate before the next user
instruction, including a sleeping or I/O-blocked target.

Custom handler registration returns ENOSYS. There is no user signal frame,
rt_sigreturn, SA_RESTART, sigsuspend, stop/continue or process group. BusyBox ash's
attempts to register custom SIGCHLD/INT handlers therefore visibly fail, while
the specifically tested synchronous wait and noninteractive scripts work.
No claim is made for ash `trap`, Ctrl+C, job control or arbitrary shell scripts.
These are M9 work, not fake-success registrations.

The string decoder copies at most the remaining bytes of one user page at a
time and stops after finding NUL; it does not touch the following guard page.
argv/envp share one bounded decoder and are filled in place while the address
space is pinned. Native and Linux startup blocks now use the same checked
placement helper without constructing an intermediate native header.

writev imports its complete descriptor array before publishing writes. A bad
array, signed-negative length or invalid address range fails before I/O; a
later buffer access fault returns the number of bytes already written. Empty
getrandom requests return zero without touching the pointer or entropy source.
See [optimization validation](validation/M8_OPTIMIZATION.md).

Linux reads use the existing canonical COM1 terminal: CR/LF normalization,
backspace/DEL, echo and Ctrl+D. Device IRQs stay masked and the BSP polls input.
The minimal ash build has no interactive prompt/job control; it can consume
serial command lines after launch and `exit` returns to the native shell.

## Build, use and tests

Install GNU make, GCC, `musl-tools` and optionally `qemu-user` in addition to the
existing host prerequisites. The measured libc is Ubuntu musl 1.2.4-2; GCC is
13.3.0. `cargo xtask apps` builds the probe and musl application with separate
Linux linker defaults, then builds unmodified BusyBox 1.36.1 at commit
`1a64f6a20aaf6ea4dbba68bbfa8cc1ab7e5c57c4` with the checked-in applet profile.
First use fetches that exact Git revision; subsequent builds reject source
modifications and reuse its build directory. SHA-256 of ELF/config/archive
content is recorded in `build/apps/manifest.toml`. The manifest identifies the
actual local build, not every possible host compiler's output.

```sh
cargo xtask apps
cargo xtask trace-compat
cargo xtask test-compat
cargo xtask verify
cargo xtask terminal-test
cargo xtask persistence-test
```

`trace-compat` uses QEMU user-mode syscall tracing against Linux for these exact
ELFs. It works when ptrace is prohibited. On exec, same-architecture children
may run directly on the Linux host; separate echo/cat/ls traces cover those
applets. This is explicitly not a complete ptrace trace across every child.
Output/trace files are under `build/logs/compat-host-*`.

`test-compat` runs the complete boot acceptance image, including historical
native isolation/process/storage cases. The classifier requires real Linux
probe/musl/app/root-reclamation evidence, a 5,000-byte shell pipeline and actual
exit-device success. `terminal-test` also launches musl and ash through UART in
an ordinary non-test image; persistence tests retain the M7 save/reboot/error
contract. The Linux stress workloads run only in qemu-test images.

In the normal native shell:

```sh
linux-musl test
/linux/echo hello
/linux/ls /etc
/linux/cat /etc/motd
/linux/sh
```

Inside ash use explicit applet paths, for example:

```sh
/linux/echo hello | /linux/cat > /tmp/linux-note
/linux/cat /tmp/linux-note
exit
```

`/bin` still contains native tools; ash cannot exec them as Linux. Native v1's
single argument string is retained; launch ash without an argument, then let
ash parse its own input. The independent native and Linux SDK contracts have
not been conflated.

BusyBox remains GPL-2.0-only, musl keeps its MIT notices, and neither is linked
into the kernel. The ISO includes their notices. An accompanying exact BusyBox
source/config/rebuild bundle is generated at
`build/images/busybox-1.36.1-source.tar.gz`; retain it when redistributing a
built ISO. See [LICENSING.md](LICENSING.md).

## Specification references

- [Linux x86_64 syscall table](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/arch/x86/entry/syscalls/syscall_64.tbl)
- [Linux x86_64 stat UAPI](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/arch/x86/include/uapi/asm/stat.h)
- [Linux FS/GS documentation](https://www.kernel.org/doc/html/latest/arch/x86/x86_64/fsgs.html)
- [musl startup/TLS source](https://git.musl-libc.org/cgit/musl/tree/src/env/__init_tls.c?h=v1.2.4)
- [BusyBox 1.36.1 upstream source](https://git.busybox.net/busybox/tree/?h=1_36_1)
