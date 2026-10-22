<!-- Define the bounded guest Rust/Cargo toolchain, OS interfaces and independent acceptance gates. -->
# M12: Rust development inside AnovaOS

This profile prepares real Rust 1.90.0 musl host tools, matching `std`, embedded
LLVM and `rust-lld` at `/rs`, alongside the source-built M11 C/C++ tools at `/tc`.
Application compilation runs in guest processes. Host cross-compilation is a
separate preliminary test and never supplies the persistent acceptance binaries.
Actual acceptance and remaining exclusions are recorded in [validation/M12](validation/M12.md).

## Construction and operation

Use the Linux/WSL dependencies from [M11_TOOLCHAIN](M11_TOOLCHAIN.md), plus the
host Rust `x86_64-unknown-linux-musl` target for the preliminary cross test:

```sh
rustup target add x86_64-unknown-linux-musl
cargo xtask toolchain-build
cargo xtask rust-build
cargo xtask rust-image
cargo xtask root-image --linux build/disks/rust-devel.ext2
cargo xtask run --rust --disk build/disks/rust-devel.ext2
```

`rust-image` writes `build/images/anova-rust.iso`. Its QEMU profile uses one CPU,
3 GiB RAM and TCG. Ordinary images and the kernel target stay separate. Plain
`run --rust` creates a disposable Linux-profile disk; select an explicit disk
and use guest `shutdown` for retained files. The editor is the M7 line editor.

```sh
cd /persist
edit main.rs
rustc --edition=2024 main.rs -o hello-rust
./hello-rust
cargo build --offline --frozen
shutdown
```

The launcher configures `/rs`, the actual LLVM linker, one codegen/link/build
job, bounded temporary paths and offline Cargo. It does not invoke rustup,
contact a host compilation service or apply the bare-metal kernel linker script
to ordinary applications. Default Rust outputs use static musl linkage.

Linux development disks remain 16 MiB, one-group ext2 with 128-byte inodes and
the existing 1 MiB file limit, but now allocate 512 inodes. Cargo's macro and
build-script artifacts exhaust the earlier 128-inode profile. Ordinary native
disks keep 128 inodes; existing disks are never reformatted automatically.

## Provenance and offline dependency

[inputs.json](../ports/rust/inputs.json) pins the official 2025-09-18 Rust 1.90.0
musl rustc/Cargo/std archives and itoa 1.0.15. These are **upstream binary
distributions**, not a claim that this repository rebuilt Rust or LLVM from
source. M11's locked musl bootstrap supplies the compiler's dynamic unwinder.
Rust source construction and compiler self-bootstrap remain separate work.

The preparation script verifies whole-archive SHA-256 before extraction,
validates archive paths, selects only needed components and retains licenses.
The compiler driver and rust-lld have debug/unneeded symbols stripped; installed
hashes distinguish the staged files from their original distribution archives.
It serializes preparation, builds a distinct candidate and publishes a complete
installation before retiring the previous one. Image construction rechecks the
entire installed regular-file inventory. The final archive has its own SHA-256.

The registry dependency is a real checksum-locked vendored itoa crate. Its
archive and per-file checksums, generated Cargo.lock and source replacement are
independent checks. All crate bytes are available before boot; networking is
unnecessary. See [third-party/m12](third-party/m12.md) for upstream notices.

## Kernel and ownership changes

- Linux startup accepts 256 argv and 256 env entries, bounded by 8 KiB payload
  per vector, 1,023 bytes per string and a checked 32 KiB startup block. Native
  startup keeps its 64-entry limit. Linux/VFS paths accept 512 bytes and
  components 255 bytes, covering hashed Cargo objects. Native v1 path buffers
  stay 128 bytes; native 80-byte directory records remain unchanged and return
  TOO_LONG without advancing when a name exceeds their 63-byte representation.
  Linux getdents uses an independent bounded record.
- Linux ELF files and total loaded segments are bounded at 256 MiB. Initramfs
  entries are at most 256 MiB and retained modules at most 1 GiB; every byte
  still needs validated retained boot RAM. The Linux mmap window is
  `[0x50000000, 0xc0000000)`, with 256 MiB per call. brk, native ELF, 16 MiB
  kernel heap, guarded kernel stacks and 1 MiB writable-file bounds remain.
- mmap keeps a per-root search hint and wraps over the complete window if the
  tail has no gap. Occupied and PROT_NONE pages cannot be reused. Hints commit
  only after successful allocation/protection; the existing OOM rollback stays.
- `MAP_STACK` is a Linux hint. Actual guarded mappings, `sigaltstack`,
  `SA_ONSTACK`, signal frames and validated return use real per-thread state.
  The 32-bit flags field never includes the ABI's unspecified padding.
- Ordinary fork inherits the alternate-stack registration; VM-sharing threads
  start disabled. Exec clears it. Unsupported autodisarm remains rejected.
- musl posix_spawn's exact `CLONE_VM | CLONE_VFORK | SIGCHLD` process variant
  has a distinct child stack and suspended parent; existing vfork ownership,
  failed-exec wakeups, separate fd tables and signal rules remain in force.
- Linux wait operates on children of the thread group, including when a worker
  thread launches and waits for a linker. Only the calling waiter blocks.
- Linux threads may unmap their own stack immediately before SYS_exit. Return
  checks require a canonical user RSP without predicting a later stack access;
  actual accesses still fault through user page permissions. Native return
  checks retain their original mapped-stack requirement.
- Regular-file pread/pwrite preserve shared open offsets, including failures.
  Pipe FIONREAD/FIONBIO expose real queued bytes and shared status flags.
- Bounded ftruncate preserves offsets, retains the prefix on shrink and
  zero-fills growth. ext2 preflights space and uses its failed-device mutation
  rules. rust-lld uses `--no-mmap-output-file`; writable shared-file mmap/writeback
  remains outside this profile. The online continuation adds bounded readonly
  shared-file mappings with coherent writes and retained inode lifetime.
- AF_UNIX SOCK_SEQPACKET pairs use two queues from the 16-entry pipe pool.
  Packets are at most 1,022 bytes with preserved boundaries, atomic sends,
  truncating receives, real duplex traffic, backpressure, EOF, nonblocking/
  CLOEXEC flags and last-open-description lifetime. Filesystem Unix sockets,
  ancillary data, other socketpair types and vectored packet I/O are unsupported.
- Empty-directory removal checks children, retains an open removed inode and
  releases its blocks/inode only after final close. Mounts, live cwd references
  and parents of removed open child directories return EBUSY in this profile.
  Checked dirfd-relative open/stat/unlink and O_NOFOLLOW support real rustc
  temporary-archive cleanup; file-type bits in mode_t are ignored while
  unsupported special permission bits remain rejected.
- Advisory flock state belongs to the open description: dup/fork share it,
  final close releases it, contention can block and pending signals interrupt.
- Regular-file rename preserves open replaced inodes on tmpfs/ext2. Directory
  rename supports a new name within the same parent, preserving dot-dot and
  parent link counts; cross-parent moves and directory replacement are rejected. ext2 keeps
  its ordered-write/failed-device behavior and normal-shutdown persistence;
  this adds no journal or power-loss transaction guarantee.

## Separate acceptance gates

```sh
cargo xtask rust-cross-test
cargo xtask rust-runtime-test
cargo xtask rust-test
```

The cross gate exercises real std allocation, argv, files, threads, child output,
detached-thread self-unmap/exit, alternate signals, long names, packet pairs,
directory cleanup, positioned I/O and locks.
The runtime gate starts the actual matching rustc and Cargo and shuts down.

The full gate enters sources through UART, runs guest rustc, builds and loads a
standalone procedural-macro shared object, builds a dependency-free Cargo project
and builds a locked vendor Cargo project with a path-dependent procedural macro.
Its build.rs generates OUT_DIR input, executes guest GCC/ar and links a real C
function into Rust. A second Cargo build checks freshness; a source edit forces
rebuild. The Cargo macro has an ordinary host dependency and a build script
using the same support crate to generate OUT_DIR source and environment. Editing
that support crate changes expansion from 42 to 84 and must recompile the support
crate, macro and application while retaining the vendor dependency. Invalid macro
input must return Cargo status 101 and leave the earlier application runnable;
restoring valid source must build successfully again. A separate parser error
must return nonzero without replacing an earlier working executable.
Host inspection checks exact source bytes, ELF/mode and SHA-256 for 23 files,
then a clean second boot runs the saved applications. e2fsck and
unchanged hashes are required. No banner or host-produced application passes.

Procedural macros use this exact compiler's host ABI. The launcher selects
dynamic musl CRT/PIC options for a single `proc-macro` crate type. Cargo `lib`
and `rlib` dependencies also use PIC so address-bearing data can be linked into
the macro shared object; application linkage remains static by default. General
Cargo workspaces and arbitrary proc-macro graphs are not implied by this bounded
offline profile. The independent online registry/Git/TLS continuation is defined
in [M12_NETWORK](M12_NETWORK.md), with actual results in the validation record.
A native AnovaOS std port and Rust/LLVM self-bootstrap remain separate work.

Cargo may emit a nonfatal last-use/global-cache warning because POSIX fcntl
record locks are unsupported. The accepted frozen vendor build does not rely
on registry cache tracking. Advisory flock is implemented independently; the
warning is retained rather than reporting invented record-lock success.
