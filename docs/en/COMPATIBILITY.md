<!-- Record verified native/Linux software profiles, package admission and source-reconstruction compatibility limits. -->
# Existing-software compatibility

**English** | [简体中文](../zh-CN/COMPATIBILITY.md)

Locally accepted profiles cover native applications, persistence, selected BusyBox,
static/PIE/dynamic musl, controlled networking/packages, guest C/C++/Rust/Cargo,
native SDKs and M14 GUI. M15 separately verifies controlled source reconstruction.
These are versioned bounded profiles, not general Linux/glibc compatibility.
See [M9 runtime](milestones/M9_RUNTIME.md), [M12 Rust](milestones/M12_RUST.md)
and [STATUS](guides/STATUS.md) for current bounds.

## 1. Compatibility layers

| Layer | Meaning | Project policy |
| --- | --- | --- |
| Source | Recompile existing software for AnovaOS | Portable C/C++/Rust and bounded POSIX APIs first |
| Linux binary | Run specific Linux ELF without source changes/recompilation | Tested subset of Linux x86_64 user ABI |
| Windows | Run PE/Win32 programs | Future Wine/port exploration, outside initial scope |
| Virtual machine | Run software inside a different complete guest OS | Separate future work; not AnovaOS ABI compatibility |
| Kernel drivers | Load another kernel's drivers | Unsupported; drivers must be implemented or adapted for this kernel |

ELF is a file format, System V AMD64 specifies part of calling/runtime behavior,
POSIX describes APIs/behavior, and the Linux syscall ABI describes kernel interfaces.
None substitutes for the others.

## 2. Implementation sequence

1. Native `no_std` applications verify isolation and SDK behavior.
2. Pinned small Linux x86_64 static ET_EXEC applications.
3. Static musl hello/file tools and selected BusyBox applets.
4. Dynamic musl, TLS, signals, threads and build tools.
5. GCC/G++ and rustc/Cargo, driven by actual guest development workflows.
6. Explicitly selected glibc combinations, graphics libraries and desktop protocols.

Static linkage reduces interpreter/library deployment needs, not kernel dependencies.
BusyBox applets differ substantially: running echo does not establish shell/network/mount support.

## 3. ABI implementation checks

### 3.1 Loading and startup

Check ELF64 headers, PT_LOAD, BSS, address/length/alignment overflow, permissions and admitted types;
required 16-byte alignment, argc/argv/envp/auxv and entry state; explicit personality metadata
because Linux commonly uses System V OSABI; controlled ET_EXEC followed by independent
static/dynamic PIE acceptance; PT_TLS, FS base and per-thread FPU/SSE state across switches.

### 3.2 Syscalls

Linux x86_64 uses RAX for the number, RDI/RSI/RDX/R10/R8/R9 for arguments and RAX for results.
SYSCALL affects RCX/R11. Return Linux negative errno, not libc's separate `-1 + errno` wrapper.
The following is an investigation starting point, not a complete static-program requirement list.

| Capability | Candidate interfaces/semantics | Verification focus |
| --- | --- | --- |
| Console/files | read/write/writev, openat/close, fstat/newfstatat, lseek | Partial I/O, errno, fd lifetime |
| Memory | brk, mmap/munmap/mprotect, permissions | Alignment, MAP flags, failures |
| Startup/exit | execve, exit/exit_group, wait4, getpid | argv/env, status, cleanup |
| Paths/directories | getdents64, readlink, cwd, rename/unlink | Layouts, dirfd, open offsets |
| Time/random | clock_gettime, nanosleep, getrandom | Monotonicity, blocking/errors, actual entropy |
| TLS/threads | arch_prctl, clone, set_tid_address, futex | FS base, flags, join, deadlines, cleanup |
| Signals | rt_sigaction, rt_sigprocmask, rt_sigreturn | AMD64 frames, masks, restart |
| Shell/TTY | pipe2, dup2/dup3, ioctl, termios, groups | EOF, Ctrl+C, redirection, jobs |
| I/O waiting | poll/ppoll, possible later epoll | Wake races, deadlines, close, signals |
| Capability probes | uname, required `/proc`, limits | Real capability and precise unsupported errors |

Requirements depend on software/compiler/libc versions and build options. Add support using traces,
source and behavioral tests. Return specified errors when libc can legitimately fall back.
Implement real semantics before claiming support: futex cannot always return zero; fsync cannot
silently discard dirty data; fixed byte patterns are not secure randomness.

### 3.3 Dynamic linking and glibc

The kernel loads PT_INTERP and supplies mmap/mprotect/files/startup information; the user dynamic
linker performs symbol resolution and ordinary relocation. musl and glibc need matching interpreters
and sysroots. glibc additionally depends on version-specific symbol versions, threads, signals, TLS,
vDSO fallback, NSS and locale. Arbitrary distribution binary trees are outside initial scope.
Actual ioctl and `/proc` requirements matter; syscall-number coverage does not establish Linux compatibility.

## 4. Software matrix

Native acceptance is in [M4](validation/M4.md); static CLI and musl runtime evidence are in
[M8](validation/M8.md) and [M9](validation/M9.md). Exact ELF hashes remain in those records and image manifests.

| Software/build | Stage | First tested behavior | Current status |
| --- | --- | --- | --- |
| Native Rust `no_std` | M4 | write/exit/info, argv, BSS, x87/SSE2, bad-pointer isolation | Locally accepted q35/TCG |
| Native freestanding C | M4 | write/exit/info, argv, BSS, SSE2, exit 7 | Locally accepted q35/TCG |
| First-party Linux syscall ELF | M8 | errno, auxv, TLS, fork/exec/env/wait, CLOEXEC, SIGPIPE, anonymous memory | Locally accepted q35/TCG |
| Static musl 1.2.4 hello/file | M8 | TLS, errno, malloc, file I/O | Locally accepted q35/TCG |
| Bounded static BusyBox 1.36.1 | M8 | echo/ls/cat, selected sh pipes/redirection/status | Locally accepted; other applets unverified |
| Dynamic musl 1.2.4 | M9 | PT_INTERP, startup libraries, dlopen, dynamic TLS | Locally accepted bounded profile |
| Static/static-PIE/dynamic musl pthread fixtures | M9 | Threads, join, futex, signal return, poll, PTY | Locally accepted within M9_RUNTIME |
| GNU make 4.4.1 / binutils 2.37 | M11 | Children, timestamps, no-op/selective incremental builds | Locally accepted offline |
| Controlled GCC/G++ 11.2.0 | M11 | Guest C/C++ compilation/linking, exceptions, threads | Locally accepted within M11_TOOLCHAIN |
| Rust 1.90.0 std applications | M12 | Vec/String, argv, files, threads and worker-child wait | Locally accepted musl target |
| Controlled rustc/Cargo 1.90.0 binaries | M12 | Guest std, offline vendor, build.rs/C FFI, macros | Locally accepted within M12_RUST |
| Portable tools such as SQLite CLI | After M13 as needed | Writes, locks, persistence | Planned |
| Selected glibc CLI software | Independent follow-up | Version-specific behavior | Planned |
| SDL/graphics software | Independent follow-up | Software rendering, windows/input | Planned |
| Browser/Electron/Wine/commercial applications | Unscheduled | Separate dependency/cost analysis | Outside initial scope |

## 5. Per-software verification records

Record upstream URL/version/commit/license and source/binary SHA-256; compiler/target/libc/sysroot,
static/dynamic/PIE and features; interpreter/libraries/data/environment; host trace versus guest
commands/inputs/output/status; supported/unsupported/unverified behavior and missing interfaces;
and tested kernel commit, machine, RAM and vCPU count.
Use planned, building, starts, partially passing, verified or regression failure.
Unexecuted checks remain unverified; preparing a binary is not acceptance.

## 6. Comparison and regression

Observe the same binary/inputs under controlled Linux, then compare AnovaOS outputs, exit codes,
files and relevant syscall semantics. Host strace discovers requirements; it does not verify guest behavior.
Negative cases include bad addresses, partial I/O, EINTR, invalid fds, EOF, concurrent close,
timeout, exhaustion, truncation and missing interpreters/libraries. Extend coverage by actual need.
[M8 design](milestones/M8_LINUX.md) and [evidence](validation/M8.md) are historical. M9 added selected
handlers, UART Ctrl+C, PTY foreground signals and dynamic musl; BusyBox tests still do not establish
complete interactive jobs or arbitrary scripts.

## M9 measured runtime additions

| Program/input | Result | Boundaries |
| --- | --- | --- |
| First-party musl 1.2.4 static ET_EXEC | Verified | pthread/cond/mutex/join, TLS, futex, signals, poll, PTYs, lifecycle negatives |
| Same source as static PIE | Verified | Musl rcrt1 self-relocation; same runtime cases |
| Same source as PT_INTERP dynamic ELF | Verified | Actual musl interpreter, startup TLS .so, dlopen after thread creation and dynamic TLS |
| Missing interpreter / missing dlopen library | Verified rejection | ENOENT leaves exec caller intact; loader names missing library |
| UART dynamic launch and Ctrl+C | Verified within declared terminal profile | No complete ash jobs, realtime signals or glibc claim |

Exact build flags, runtime checksum, guest evidence, host trace differences and
remaining limits are in [M9 validation](validation/M9.md).

## M10 measured additions

The [controlled M10 profile](milestones/M10_NETWORK_PACKAGES.md) adds AF_INET client
TCP/UDP, bind/connect/read/write/sendto/recvfrom, names/shutdown, SO_ERROR and I/O
timeouts, poll/nonblocking behavior and signal interruption. Actual guest DNS A,
TLS 1.2 and package clients are tested. File readv, dup and restricted chmod(0755)
complete their measured runtime needs. Existing musl static/PIE/dynamic hashes
remain the M9 baseline. This does not establish curl, wget, musl getaddrinfo,
TCP servers, general socket vectored/message APIs, filesystem AF_UNIX, IPv6 or arbitrary Linux
network application support. Unsupported interfaces return real errors.

## Verified M11 offline compiler profile

| Software | Verified behavior | Remaining limits |
| --- | --- | --- |
| GCC/G++ 11.2.0 | Actual guest C/C++ compile/assemble/link; C argv/stdio/malloc/files/multiple sources; C++17 string/vector/RAII/exceptions/std::thread | Small static applications; no general project, LTO/OpenMP/sanitizer or self-bootstrap claim |
| GNU binutils 2.37 | Guest assembler, linker and collect2 pipeline; static ELF output and unresolved-symbol failure | Other utilities/targets and dynamic application toolchains unverified |
| GNU make 4.4.1 | Real shell/child compilation, no-op and selective incremental build | Serial fork configuration; arbitrary build scripts unverified |
| musl 1.2.4 and libgcc/libstdc++ 11.2.0 | Matching source-built CRT/headers/archives in `/tc`; generated C/C++ runtime tests | No glibc compatibility claim |

Linux disk personality is explicitly inherited from its `.anova-abi` manifest.
M11 adds real vfork parent suspension, private file snapshots, mode/umask and
resource-query support; optional unsupported compiler probes return ENOSYS.
Exact bounds and exclusions: [M11_TOOLCHAIN](milestones/M11_TOOLCHAIN.md).
Actual logs and file hashes: [M11 acceptance](validation/M11.md).


## Verified M12 offline Rust profile

Official checksum-locked Rust 1.90.0 `x86_64-unknown-linux-musl` rustc/Cargo/std
and rust-lld run on the AnovaOS kernel. A host cross test is preliminary;
acceptance binaries compile from UART-edited sources inside the guest. Separate
tests cover std/threads/child output, compiler startup, a standalone procedural macro,
locked offline itoa, build.rs generated input/environment and guest GCC/ar C FFI.
The fixed Cargo macro graph includes an ordinary host/build dependency, generated
macro input, transitive rebuilds from 42 to 84, invalid-input status 101 and an
unchanged runnable prior application, followed by a successful source repair.
Cargo freshness/source edits, a real parser failure, disk hashes, e2fsck and
reboot execution are checked. Runtime tests include alternate signal stacks,
detached exit, invalid-return faults, positioned/resized files and OFD locks.

This establishes the bounded profile in [M12_RUST](milestones/M12_RUST.md), not general
Cargo/ecosystem compatibility. Arbitrary macro graphs, native AnovaOS std
and Rust/LLVM source construction/bootstrap remain unverified.
M12 also adds bounded anonymous AF_UNIX SOCK_SEQPACKET pairs; SOCK_STREAM
pairs remain rejected with ESOCKTNOSUPPORT. Linux/VFS names and paths use
255/512-byte bounds without changing native v1 directory wire records.
Actual logs and hashes: [M12 acceptance](validation/M12.md).


## Verified M12 online Cargo profile

The same official Cargo 1.90.0 downloads a checksum-pinned crate from an HTTPS
sparse registry and a full-revision Git dependency over HTTPS smart HTTP.
`cargo-online` selects this opt-in transport and `/persist/cargo-home`; ordinary
Cargo remains offline by default. Independent controlled-peer tests reject
untrusted/expired/wrong-name certificates, corrupt/truncated archives and an
application timeout. Guest compilation, no-op builds and fresh-target rebuilds
after a no-NIC reboot preserve inspected source/cache/lockfile/ELF hashes.

The separate public gate downloads crates.io itoa 1.0.15 and GitHub dtolnay/itoa
revision `e2766b868e4ac1ae2bf5bea1ac43d4c0da23b899` without source replacement.
Local public acceptance uses a configured host HTTP proxy and public PEM roots;
direct public DNS/transport is not established. It does not imply arbitrary
Cargo projects, private registry credentials, SSH, Cargo publish or IPv6.

Runtime coverage adds select/readiness/deadline/EINTR, actual file timestamps,
readonly shared-file coherence and inode lifetime across fork/close/unlink/name
replacement, an actual readonly write fault, and real heap OOM/reuse. Writable
shared-file mappings and persistent mmap writeback remain rejected. Exact bounds
and configuration: [M12_NETWORK](milestones/M12_NETWORK.md). Actual command results:
[M12 acceptance](validation/M12.md).

## Persistent-file capacity in the M15.5 implementation

Persistent ext2 regular files now admit 4 MiB within the existing direct and
single-indirect format. Existing images retain their format; no reformatting is
required. tmpfs files and readonly shared-file caches remain limited to 1 MiB.
An active shared mapping prevents growth past the cache limit with `EBUSY`;
ordinary file I/O and private mappings can access larger persistent files.
This capacity change does not establish additional compiler/bootstrap or
arbitrary Cargo-project compatibility. The kernel reconstruction profile and
its acceptance status are recorded separately in [M15 kernel](milestones/M15_KERNEL.md).

## M13 application package admission

SDK 1.0.0 keeps native v1 numbers, errors and layouts frozen. ANOVAPK2 adds
explicit target/ABI/entry/dependency metadata and executable consistency checks.
Native persistent disks admit native packages; Linux disks admit bounded static
ET_EXEC Linux packages. Installation cannot change the VFS personality, and the
mount's ABI marker cannot be rewritten or replaced. Dynamic/PIE package
admission is deferred without changing the independently tested M9 ELF runtime.
This package profile does not establish general Linux package compatibility,
glibc, a dependency solver, signature authentication or per-process sandboxes.
See [M13 design](milestones/M13_SDK.md) and [validation](validation/M13.md).


## M15.5 guest-built kernel profile

The local `bootstrap-kernel-test` gate rebuilds the actual bare-metal kernel
with inherited Rust/Cargo/rust-lld running in AnovaOS Linux-musl processes.
The host packages the exact retained guest ELF; independent extraction and
actual boot identity verify the selected kernel. Three sessions cover build,
boot, syntax rejection with old-kernel retention and recovery. See
[M15 kernel](milestones/M15_KERNEL.md) and [local evidence](validation/M15.5.md).
The accepted configuration is stripped opt-z with bootstrap metrics and no GUI;
target libraries remain inherited seeds. Compiler reconstruction and guest-only image generation remain separate. Independent clean
repeat builds are locally accepted under [M15.6](validation/M15.6.md), with exact final artifacts
and retained intermediate differences.
