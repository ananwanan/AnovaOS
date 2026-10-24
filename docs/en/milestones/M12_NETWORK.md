<!-- Define opt-in online Cargo, source/cache ownership, strict TLS and bounded network acceptance. -->
# M12 online Cargo

**English** | [简体中文](../../zh-CN/milestones/M12_NETWORK.md)

The bounded controlled and public gates are locally accepted on q35/TCG.
Public crates.io/GitHub acceptance used the documented host proxy and public
PEM roots; direct public DNS/transport and remote CI remain separate.
Actual evidence is recorded in [M12 validation](../validation/M12.md).

This continuation adds an explicit online mode to the M12 guest toolchain.
The compiler and Cargo remain the checksum-locked, unmodified official Rust
1.90.0 musl binaries. Source acquisition, extraction, dependency resolution,
compilation and linking execute in AnovaOS user processes. Host test peers
serve source data only. The kernel target remains `x86_64-unknown-none`.

## Commands and configuration

Prepare the tools with `cargo xtask rust-build`, create a Linux development
disk, and start the Rust image with networking:

```sh
cargo xtask rust-image
cargo xtask root-image --linux build/disks/rust-online.ext2
cargo xtask run --rust --network --disk build/disks/rust-online.ext2
```

Inside the guest, enter a project under `/persist` and run:

```sh
cargo-online build
cargo-online build --locked
cargo-online build --offline --frozen
shutdown
```

`cargo` retains the original offline default. `cargo-online` selects online
transport and `/persist/cargo-home`; explicitly supplied `CARGO_HOME` and
`CARGO_NET_OFFLINE` values are preserved. The actual compiler launcher preserves
the chosen environment when Cargo invokes rustc and build scripts. One build
job, one codegen/link job and the existing compiler/linker bounds still apply.
Use `--offline --frozen` explicitly when testing a saved online project without
a NIC. Select an explicit disk and shut down cleanly to retain its cache.

The launcher supplies the pinned Mozilla CA store through `SSL_CERT_FILE`.
Cargo's normal project `http.cainfo`, `http.proxy`, registry and source settings
remain available. TLS verification is enabled: the CA chain, certificate date
and server identity are checked by the upstream user-space libraries. No
certificate-verification bypass is added. The default trust store and its
MPL-2.0 notice are locked in [inputs.json](../../../ports/rust/inputs.json).

## Runtime additions

- Linux `select` imports at most 32 descriptor bits per set, matching the
  descriptor-table limit. Real VFS/socket readiness, EOF, invalid descriptors,
  checked writable outputs, monotonic deadlines, remaining timeval and EINTR
  are implemented. Original masks are retained while blocked. Neither select
  nor poll is restarted as a read syscall when SA_RESTART is present.
- TCP establishment retains its three-second bound. An established connection
  has a 30-second stack inactivity bound, allowing shorter application transfer
  deadlines to report their own timeout instead of a premature broken pipe.
- Readonly file-backed MAP_SHARED supports bounded regular files up to 1 MiB,
  including tmpfs and ext2. A fallible 1 MiB versioned cache per mapped inode
  retains identity independently of descriptors and observes successful file
  writes/growth. Changed pages refresh before user execution. Fork shares
  backing and mapping ownership; munmap, exec and exit release it.
- Replaced/unlinked mapped inodes survive their last descriptor and are
  reclaimed after their last mapping. Writable shared-file mappings and
  mprotect-to-write are rejected. Shrinking/truncating a live mapped file is
  rejected with EBUSY. Growth beyond 1 MiB is also rejected with EBUSY while a
  shared mapping is live; once the last mapping is released, an idle cache can
  be discarded before larger persistent-file growth. Mapping beyond the current
  final file page, fixed shared placement and files larger than the 1 MiB shared
  cache profile are rejected. This is not a
  writable mmap/msync or page-cache writeback implementation.
- `utimensat` and `futimesat` update actual access/modify timestamps, support
  descriptor or checked dirfd-relative paths and UTIME_NOW/UTIME_OMIT, and
  reject bad pointers/ranges/flags. ext2 stores whole seconds in 128-byte
  inodes; tmpfs preserves nanoseconds. Explicit timestamps survive clean
  reboot. Negative epochs and ext2 timestamps outside u32 seconds are rejected.
- Ext2 import uses a fallible iterative directory traversal bounded by the
  existing VFS node limit. Nested Git cache directories no longer fail at the
  former eight-level recursive import bound; syscall paths retain their checked
  512-byte limit.

The 16 MiB kernel heap, 1 MiB tmpfs/shared-cache limits, 16 MiB development disk,
512 inodes, 2,048 VFS nodes, 32 task slots, 32 descriptors, eight Internet sockets,
single BSP and polled virtio networking remain in force. M15.5 raises persistent
ext2 regular files to 4 MiB without enlarging the shared-file cache or changing
the disk format; its build profile and acceptance status are recorded in
[M15 kernel](M15_KERNEL.md). Large crates, Git packs,
dependency graphs or cache indexes may exceed these bounds and fail. POSIX
fcntl record locks remain unsupported: Cargo can emit its existing nonfatal
global-cache housekeeping warning. Unsupported operations do not report success.

## Independent acceptance gates

```sh
cargo xtask rust-cross-test
cargo xtask rust-online-test
cargo xtask rust-public-test
```

The cross gate additionally checks select/poll signal interruption, EOF, pointer
and timeout negatives, readonly shared-file coherence/fork/inode lifetime,
actual kernel-heap exhaustion and reuse, and timestamp semantics.

`rust-online-test` starts a real HTTPS sparse registry and Git smart-HTTP peer
on host loopback ports 18443–18445. Its archive has an independent SHA-256 and
its Git dependency a full revision. Neither dependency is preloaded in the
guest. Tests require the actual CA/name/date failure, rejected corrupted and
truncated archives, an application timeout, a repaired checksum failure,
guest compilation, a no-op build and an independent boot without a peer or NIC.
Persisted sources, lockfile, cache and executable hashes are inspected directly;
e2fsck must pass. The second boot rebuilds both dependencies into a fresh target
directory before executing its new binary. Generated TLS private keys remain
under host `build/`.

`rust-public-test` independently uses crates.io itoa 1.0.15 and the public
GitHub itoa repository at revision
`e2766b868e4ac1ae2bf5bea1ac43d4c0da23b899`, followed by guest compilation and
offline reboot/cache checks and a fresh-target rebuild. It has no vendor source
replacement. Public availability is distinct from the deterministic local gate. If a test host
requires a proxy, `ANOVA_CARGO_TEST_PROXY` supplies a guest-reachable HTTP proxy
without embedded credentials; `ANOVA_CARGO_TEST_CA` optionally supplies public
PEM roots for that test image. These inputs do not change the standard image.
The proxy transports HTTPS; compilation remains inside the guest.

Keep the original three M12 Rust gates and every earlier OS/toolchain gate.
Evidence and actual command results belong in [M12 validation](../validation/M12.md).
An AnovaOS-native libc/std port and compiler self-bootstrap remain independent
deliverables. Sparse HTTPS and HTTPS Git do not imply SSH, authenticated private
registries, Cargo publish, arbitrary workspaces, IPv6 or general Linux support.
