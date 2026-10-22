<!-- Define the offline guest GCC profile, build inputs, ownership rules and acceptance workflow. -->
# M11: application development inside AnovaOS

The optional development image runs real GCC/G++ 11.2.0, cc1/cc1plus, collect2,
GNU binutils 2.37 and GNU make 4.4.1. The target sysroot contains source-built
musl 1.2.4, CRT, libgcc and libstdc++ 11.2.0. Application sources are entered
through the guest editor; compilation, assembly and linking execute in guest
processes. The host never supplies the acceptance application binaries.

## Build and run

On the normal Linux/WSL host, install the dependencies from DEVELOPMENT plus
Python 3.12+, GNU make, GCC/G++, m4, bison, flex and texinfo. Allow several GiB
of host disk and memory for compiler construction; `ANOVA_BUILD_JOBS` selects
1–8 build jobs, default 6. All generated files remain under `build/toolchain`.

```sh
cargo xtask toolchain-build
cargo xtask toolchain-image
cargo xtask root-image --linux build/disks/devel.ext2
cargo xtask run --toolchain --disk build/disks/devel.ext2
```

Inside the guest:

```sh
cd /persist
edit main.c
gcc main.c -o hello-c
./hello-c
edit main.cpp
g++ -std=c++17 -pthread main.cpp -o hello-cpp
./hello-cpp
make
shutdown
```

The editor accepts tabs for make recipes. `p`, `a` (terminate with `.`), `c N`,
`d N`, `w`, and `q` are its commands. Save with `w` before leaving. Ordinary
`run` stays the base image; `run --toolchain` without an explicit disk creates
a disposable Linux-profile disk. Keep an explicit disk and shut down cleanly
for persistence. `toolchain-image` writes `build/images/anova-devel.iso` and
the development runner uses one CPU and 1 GiB RAM.

`ports/gcc/inputs.json` locks every archive before extraction, including the
musl.cc GCC 11.2.1 static bootstrap, GMP 6.2.1, MPFR 4.1.0 and MPC 1.2.1.
`build.py` builds guest tools and target libraries from upstream sources and
records the installed file inventory. Packaging verifies that inventory,
checks supported executable ELFs, records per-file/archive SHA-256, and builds
a separate read-only `/tc` sysroot. No host compiler is installed into guest
`/bin`, and no toolchain files are added to the base image. See
[third-party boundaries](third-party/m11.md).

The small `/bin/gcc`, `/bin/g++` and `/bin/make` launcher sets the controlled
PATH, TMPDIR, locale, static non-PIE application linkage and an explicit GCC
GC configuration (32 MiB minimum heap, 100% expansion). GNU make uses the real
BusyBox ash at `/linux/sh`; its source build selects fork instead of
posix_spawn. Compiler tools may themselves use accepted static PIE.

## Kernel and filesystem changes

- Linux ELF files may occupy 64 MiB and map up to 128 MiB; native ELF limits
  remain bounded separately. Boot module retention accepts a checked 128 MiB
  archive; every byte must still lie inside retained Limine module RAM.
- Linux startup permits 64 arguments and 64 environment strings, each at most
  1,023 bytes, with 8 KiB total payload per vector and a checked 16 KiB startup
  block. Paths remain at most 128 bytes. Linux user stacks are 1 MiB; native
  user stacks remain 64 KiB. Scheduler continuation is 256 KiB with guards.
- The bounded kernel heap is 16 MiB at a separate aligned address, disjoint
  from stacks and fault-test mappings. tmpfs files may reach 1 MiB; VFS has
  2,048 nodes. Descriptor/open-description/task bounds remain 32/128/32.
- brk has a 128 MiB window; mmap has a separate 512 MiB window with 128 MiB per
  call. All pages are allocated eagerly. W^X, user bounds, PROT_NONE and OOM
  rollback remain enforced. The owned lower-root invariant permits bounded
  page-table membership queries instead of scanning page metadata for each
  permission update. Host and kernel development builds use optimization
  level 1 with debug information.
- Read-only initramfs mappings borrow immutable bytes. Private mappings of
  tmpfs/ext2 snapshot file bytes independently of the descriptor offset;
  later file changes and mapping writes stay independent. No shared persistent
  mmap/writeback or file page cache is provided.
- `vfork` shares the live user root and suspends the parent until successful
  child exec or exit/fault. Pending parent signals are deferred until that
  root is released by the child. Normal fork retains its eager private copy.
  The bounded clone/posix_spawn process variants are not added by M11.
- Single-root umask, regular-file chmod/fchmod, retained permission bits,
  readlink and supported getrlimit/prlimit queries serve actual compiler calls.
  Unsupported resource changes return an error. File modification timestamps
  use checked UTC plus the scheduler clock; ext2 persists whole seconds for
  make dependency checks. There is no additional multi-user permission model.
- A separate disk explicitly declares `linux.x86_64.v1` in `.anova-abi`.
  Imported and newly created descendants inherit that image policy. Native
  disks continue to declare `anova.native.v1`; ELF headers never select ABI.

## Acceptance

```sh
cargo xtask toolchain-test
```

The watchdog-driven UART test edits C, C++, Makefile and negative fixtures,
then requires stdio/malloc/argv/files and multi-source C, C++ containers,
unwinding/RAII/exceptions and a real std::thread. It checks make's no-op build
and selective rebuild after changing a source, syntax/link errors with nonzero
status, temporary-file cleanup, vfork and permission/snapshot negatives.
After clean shutdown the host independently verifies exact source bytes and
ELF hashes/modes. A second boot executes saved applications and compares hashes;
e2fsck must remain clean. See [validation/M11](validation/M11.md).

This is a small offline static application profile. LTO, OpenMP, sanitizers,
general configure projects, compiler self-hosting/bootstrap, Rust/Cargo,
SMP, public toolchain package publication and general Linux/glibc compatibility
are not claimed. GCC's optional sysinfo/getrusage and madvise probes may return
ENOSYS; the tested build path uses explicit GC settings and does not depend
on those optimizations or profiling interfaces. ext2 still has a 1 MiB file
limit, no journal, rename transaction or power-loss recovery guarantee.
