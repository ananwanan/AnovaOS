<!-- Define native VFS, descriptor, pipe, terminal and shell contracts inherited from M6. -->
# M6 filesystems, init and native shell

This document defines the implemented VFS, descriptor, pipe, console and shell
contracts. It records bounds and ownership needed before M7 disk backends.
Measured results are in [validation/M6.md](validation/M6.md); the native CPU,
loader and scheduler contracts remain in [M5_PROCESSES.md](M5_PROCESSES.md).

M7 adds the ext2 mount, executable disk files, durability calls and editor;
[M7_STORAGE.md](M7_STORAGE.md) supersedes the no-disk/archived-only executable
boundaries below. The M6 validation record remains historical evidence.
M11 increases VFS/tmpfs capacities and adds explicit Linux development disks;
the current bounds are in [M11_TOOLCHAIN](M11_TOOLCHAIN.md).

## Boot and filesystem model

`xtask apps` builds seven independent ELF applications and packs an
`initramfs.anovafs` archive. The ISO passes this retained Limine module with the
explicit `anova.initramfs.v1` identity. The archive manifest contains its SHA-256;
the host validates all archived native executable entries before image creation.
The kernel checks module reservations and archive structure. It trusts the
host-built boot image; this is not a signature or Secure Boot chain.

`anova-vfs` is a hardware-independent `no_std` crate using fallible `alloc`.
Its sole owner is the BSP scheduler. The initial namespace contains these mounts:

| Mount | Backend | Implemented behavior |
| --- | --- | --- |
| `/` | initramfs, read-only | Borrowed regular/executable files, directories, archive symlinks |
| `/tmp` | tmpfs, read/write | Create, read, write, sparse seek, append, truncate, mkdir and file unlink |
| `/dev` | devfs | Canonical `/dev/console` and EOF/discard `/dev/null` |

Mount attachment is boot-only and requires an existing empty directory. The
mount root retains its namespace parent, so `/tmp/..` resolves to `/`. There is
no user mount/unmount interface, rename, hard link, directory removal, permission
identity, executable tmpfs file, or disk backend. A later backend can replace
the node data operations while retaining paths and open-description ownership.
Directories are never removed, so cwd inode identities remain live.

Resolution walks components in order, checking directory type before `.`/`..`
and following relative or absolute links with a depth limit of eight. It never
lexically cancels `file/..`. Absolute paths start at root, relative paths at the
process cwd; attempts to walk above root stay there. Paths are UTF-8, length
1–128, without embedded NUL; names are at most 63 bytes. Archive paths are
absolute, canonical, parent-first and cannot traverse a previously declared
symlink. Unsupported archive input and duplicate names are rejected.

`ANOVAFS1` is a project-owned format, not tar/cpio: eight magic bytes, a u32
little-endian entry count, then entries with u8 kind, zero u8 reserved, u16 path
length and u64 payload length, followed by exactly those path and payload bytes.
Kinds 1/2/3/4 mean regular/directory/symlink/native executable. Directories have
empty payloads. Truncation, overflow, bad kinds/paths, malformed link targets and
trailing bytes fail. Payloads are bounded to 8 MiB, the retained archive module
to 16 MiB, and entry count to 96. No Rust/C structure is cast onto archive bytes.

## Descriptor and process ownership

Each process has 32 descriptor slots, initially console stdin=0 (read), stdout=1
and stderr=2 (write). A slot refers to one of 128 global open descriptions with
its own flags, offset, target and reference count. A separate `open` creates an
independent offset. `dup2`, fork and spawn increment the same description's
reference count; closing any inherited slot leaves other owners intact. `exec`
retains the descriptor table and cwd. M6 extends M5 spawn to inherit both from
its parent. Descriptors cannot implement `Copy`/`Clone` outside VFS ownership.

Unlink removes a tmpfs name while existing descriptions continue to access its
data; the final independent open close frees it. Reusing an unlinked node waits
for every description to disappear. Append selects current EOF for each write
inside the single-owner critical section. Sparse writes zero the gap. Failed
writes caused by size or heap exhaustion preserve the offset and previous data.
Create/pipe check descriptor/object capacity before publishing new resources.

The VFS supports 96 nodes, 128 open descriptions, 16 pipes and 64 KiB per tmpfs
file. Total storage also depends on the fixed 2 MiB kernel heap; allocation
failure returns native `NO_MEMORY`, not a kernel panic. Files can be unlinked
to release storage. Limits return explicit errors; no fake success is used.

## Pipes, blocking and console

Pipes are 1,024-byte bounded byte streams. Reads return available bytes; empty
reads block while a writer exists and return EOF after the final writer closes.
Full writes block while readers exist; writes with no readers return
`BROKEN_PIPE` (signals are not implemented). Writes may return a partial count;
the runtime's `write_all` retries the remainder. Pipe offsets are not seekable.
A zero-length valid read/write returns zero without waiting.

A process that would block stores only its captured syscall frame and enters
`Blocked`; no VFS/memory/heap lock, active user root, borrowed user slice or
kernel-stack continuation survives the wait. The scheduler checks at most 32
blocked processes for readiness after every event. Data transfer or final close
changes readiness, wakes the process and retries the pinned single-threaded
user copy. With no ready process it executes STI/HLT. This is bounded BSP event
scanning, not user polling, an SMP wait queue or a nonblocking I/O API.

COM1 RX is polled on scheduler iterations, with the existing 100 Hz LAPIC IRQ
waking idle input. UART IRQs and all external IOAPIC lines remain disabled.
The kernel consumes at most 64 UART bytes per iteration and performs no IRQ
allocation/logging. Canonical input has a 255-byte editable payload, 1,024-byte
ready queue, printable ASCII echo, backspace/DEL, CR/LF normalization and Ctrl+D.
A Ctrl+D on an empty line produces one EOF; with pending text it commits those
bytes. Queue pressure stops RX consumption. This is a minimal terminal boundary,
without termios, PTYs, signals, job control or USB keyboard support.

File syscall buffers are at most 4,096 bytes. Page permissions and lengths are
checked before transfer, and the narrow M4 fault fixup remains in use. Only the
BSP mutates mappings, each process is single-threaded, and IF=0 pins the address
space throughout copying; no copy spans a block or context switch.

## Additive native v1 file calls

Registers remain RAX=number, RDI/RSI/RDX=arguments, RAX=result; RCX/R11 are
clobbered. These are native numbers and errors, not Linux/POSIX encodings.

| Number | Call | Arguments; successful result |
| --- | --- | --- |
| 0 | write | fd, buffer, length; bytes written |
| 9 | close | fd; 0 |
| 10 | read | fd, writable buffer, length; bytes read, 0=EOF |
| 11 | open | path pointer, path length, flags; fd |
| 12 | seek | fd, signed i64 offset, whence 0/1/2; new offset |
| 13 | dup2 | source fd, target fd; target fd |
| 14 | pipe | writable u64[2] pointer, size=16; 0 |
| 15 | readdir | fd, record pointer, size=80; 1=entry, 0=end |
| 16 | mkdir | path pointer, path length; 0 |
| 17 | chdir | path pointer, path length; 0 |
| 18 | getcwd | output pointer, capacity; bytes without NUL |
| 19 | unlink | path pointer, path length; 0 |

Open bits are READ=1, WRITE=2, CREATE=4, TRUNCATE=8, APPEND=16 and DIRECTORY=32;
unknown bits are rejected. Readdir serializes inode:u64, kind:u32, name_len:u32
and a fully initialized 64-byte name, for 80 bytes total. Record kinds are
regular=1, directory=2, link=3, device=4. There are no implicit `.`/`..` entries.
M6 write now accepts any writable descriptor; invalid/closed descriptors return
`BAD_DESCRIPTOR`, including the old fd=9 negative probe.

Calls 5/6 now resolve native executable archive entries by path; a bare name
uses `/bin`. The native v1 ExecRequest remains name plus **one** argument string
(each at most 128 bytes), not a POSIX argv/envp vector. The Rust SDK wraps file
calls. The C SDK retains `anova_syscall` for direct use of this documented ABI.
Linux personality selection remains unimplemented.

## Init, shell and commands

After historical startup/validation workloads the final session starts
`/bin/init` as PID 1. It starts `/bin/shell`, waits for it and restarts an
interactive shell after EOF/exit. Existing M5 kernel adoption still handles
orphans; this init is not a POSIX service manager or general orphan reaper.
Test init first waits for `fs-test`, then for the shell acceptance script.

The native shell has a 512-byte input limit, four pipeline stages and 32 parser
tokens, with single/double quotes, escapes and `<`, `>`, `>>`, `|`. Commands
receive joined argument text under the native v1 contract. Parent builtins are
`cd`, `help`, `status`, `exit`; they reject pipelines/redirection. Native
multicall ELF commands are `ls`, `cat`, `echo`, `pwd`, `mkdir`, `rm`, `true`,
`false`. `ls`/`cat` take one optional path. These are project-owned commands,
not BusyBox, Bash, or Linux binaries. `status` prints the previous command's
code; pipelines use the final stage's status. Expansion, globbing, environment
variables, semicolons, scripts as executable files and job control are absent.

Each external stage forks, duplicates the selected pipe ends to stdin/stdout,
closes all original pipe descriptors, applies file redirection and execs the
selected native ELF. The parent closes every endpoint before waiting for all
children. This allows pipelines larger than the buffer and EOF without leaked
writers. A failed stage or an early reader exit releases endpoints and wakes
blocked peers. Partial fork/pipe failures also close endpoints and reap children.

Example session after `cargo xtask run`:

```sh
ls /
mkdir /tmp/work
cd /tmp/work
echo hello > message
cat < message | cat
hello-c demo
status
```

`/tmp` data survives shell restarts within this boot, but is lost after reboot.
M7 must add virtio-blk, a supported ext2 subset and explicit persistence tests.
