<!-- Define the bounded M15 build environment, guest bzip2 reconstruction and measurement contract. -->
# M15: controlled source reconstruction

This profile implements the first M15 build environment and a small C tool
reconstruction path. It selects official bzip2 1.0.8 as the capacity target,
reuses the [M11 guest tools](M11_TOOLCHAIN.md), and creates an independent
persistent work disk. Implementation and intended checks are described here;
acceptance requires the actual command results recorded in
[current status](STATUS.md). M15.3 has a separate [GCC feasibility
assessment](M15_GCC.md), and M15.4 independently assesses
[Rust/LLVM/Cargo](M15_RUST.md). Compiler construction and M15.5/M15.6 remain
outside this bzip2 profile.

## Prepare and verify

Install the host prerequisites from [DEVELOPMENT](DEVELOPMENT.md), fetch the
locked boot inputs, and prepare the source-built M11 compiler/sysroot first:

```sh
cargo xtask fetch
cargo xtask doctor
cargo xtask toolchain-build
cargo xtask bootstrap-prepare
cargo xtask bootstrap-test
```

`bootstrap-prepare` verifies the official source archive and selects the
required unmodified source files, notices and samples into
`build/bootstrap/source/`. It packages the existing M11 compiler and a separate
capacity diagnostic into `build/images/anova-bootstrap.iso`, then creates a
new `build/disks/bootstrap-TIMESTAMP.ext2` containing `/persist/bootstrap`.
The exact image and disk paths, input hashes, source commit/worktree state,
seed-tool inventories and configuration are saved in
`build/bootstrap/prepared.json`. An existing work disk is never reformatted.

`bootstrap-test` prepares its own fresh work disk and runs three QEMU boots.
It writes `build/logs/bootstrap.result.json` only after every scenario,
independent disk inspection and filesystem check succeeds. The per-boot serial,
QEMU and result logs are `build/logs/bootstrap-capacity.*`, `bootstrap-build.*`
and `bootstrap-reboot.*`; persisted source/output hashes are retained alongside
the build and reboot logs. A missing successful result is not acceptance. The final result embeds the
preparation manifest so a later preparation cannot replace the accepted
run's recorded source, image and seed identities.

The bzip2 source-only step can be checked independently without preparing or
booting the image:

```sh
python3 ports/bootstrap/prepare.py
python3 ports/bootstrap/prepare.py --verify
python3 ports/bootstrap/test_prepare.py
```

Input verification fails if the cached archive or selected source tree changes.
Retain intentional local changes elsewhere before removing a changed generated
source cache and preparing it again. The script does not silently overwrite it.

## Build roles and provenance

The declared build and host are AnovaOS on x86_64 using the explicit
`linux.x86_64.v1` compatibility personality. The target is a static,
non-PIE `x86_64-linux-musl` user executable. It is separate from the native
AnovaOS SDK and from the bare-metal kernel target.

GCC 11.2.0, binutils 2.37, make 4.4.1, musl 1.2.4 and `/tc` provide the
already prepared M11 seed environment. The host supplies these seed binaries,
downloads/checks/selects sources, imports them into the work disk, packages the
boot ISO and inspects the resulting disk. It does not compile bzip2.
The separately host-compiled `/bin/m15-capacity` is a diagnostic fixture, not
the reconstructed tool or compiler-bootstrap evidence.

`ports/bootstrap/inputs.json` locks the official
[source archive](https://sourceware.org/pub/bzip2/bzip2-1.0.8.tar.gz), its SHA-256
and the SHA-512 published in [Sourceware's checksum list](https://sourceware.org/pub/bzip2/sha512.sum).
Both are checked before archive members are read. Only canonical regular-file
members under the exact release root are accepted; links, duplicate paths,
traversal and selected inputs above the writable-file limit are rejected.
The generated `INPUTS.json` records machine roles, seed tools and per-file
hashes; `SOURCE.sha256` records the selected source/fixture bytes.

The selected upstream input includes eight C compilation units, two headers,
the original Makefile, LICENSE, README, CHANGES, HTML manual and all three
official compressed/reference sample pairs. No upstream source patches are
applied. The optional PostScript manual is 1,766,625 bytes and is excluded
because it exceeds the 1 MiB writable-file bound; no compiled source unit is
excluded for that reason. The custom recipe and negative fixtures are clearly
separate first-party additions. See [third-party boundaries](third-party/m15.md).

## Capacity and measurements

The selected environment uses one QEMU q35/TCG CPU, 1 GiB RAM and no NIC. The
work disk retains the existing Linux development profile: 16 MiB, 512 inodes,
4 KiB blocks and 128-byte inodes. Writable files remain limited to 1 MiB,
VFS node slots to 2,048, descriptors per process to 32, shared open descriptions
to 128, task/thread slots to 32 and the kernel heap to 16 MiB. This target
uses these existing bounds; the profile does not enlarge them or change the
format or ABI marker of older disks.

The first boot runs real capacity-rejection probes. An oversized persistent
file resize must return `EFBIG` while retaining the prior length and bytes.
Opening descriptors to the per-process limit must return `EMFILE`, followed by
successful close/reopen recovery. Forked children wait for pipe EOF until the
task bound returns `EAGAIN`; all children must then exit and be reaped, and a
new fork/wait must succeed. These are controlled exhaustion tests, not resource
needs attributed to the ordinary bzip2 build.

The optional kernel `bootstrap-metrics` feature samples at scheduler boundaries
without adding a syscall ABI. Its shutdown summary reports the initial free
physical-frame count, observed minimum, frame-count delta, observed heap use,
live tasks, occupied task slots, descriptors per process, VFS node slots and
sample count. Each scheduler instance starts fresh. The capacity, build and
reboot boots retain separate measurements so exhaustion probes do not inflate
the build's reported task/descriptor demand.

These sampled values omit temporary allocations inside a syscall. Physical
frame deltas include kernel allocations and are not process RSS; VFS node-slot
observations include retained reusable slots. The IRQ-derived elapsed clock
does not charge all kernel execution and is not a compiler CPU-time benchmark.
The report separately records host wall time including firmware, boot, UART
interaction and shutdown. Disk blocks/inodes, logical file bytes, largest file
and exact file hashes are inspected after the guest shuts down. End-of-boot
disk usage is not an exact transient disk-usage peak.

## Guest reconstruction and recovery

The second boot executes the following in the guest's native serial shell:

```sh
cd /persist/bootstrap
make -f Makefile.anova -j1
./bzip2 -1 -kc sample1.ref > sample1.out.bz2
./bzip2 -dc sample1.out.bz2 > roundtrip
./bzip2 -t sample1.out.bz2
./bzip2 -t README
shutdown
```

The custom recipe invokes guest GCC sequentially for all eight objects and
links `/persist/bootstrap/bzip2`. It uses `-O1 -g0` and the existing wrapper's
static non-PIE flags and `/tc` sysroot. It does not invoke upstream configure,
rebuild a compiler or build a separate libbz2 archive. Every command status is
checked; treating README as compressed input must produce status 2.

After clean shutdown the host parses the persisted executable, checks its
executable mode and input hashes, compares the guest round-trip bytes with the
official sample and independently decodes the guest-produced compressed stream
with Python's bzip2 library. Block size 1 also requires exact compressed-byte
equality with the official `sample1.bz2` golden fixture.

The third boot executes the retained compressor, then invokes
`make -f Makefile.anova fail`. That target compiles `rejected.c`, which must
reach its intentional compiler `#error` and fail with status 2. The prior
compressor must still test the saved compressed stream successfully. After
another clean shutdown every persisted source and output file must match the
prior hash inventory. This proves recovery for the declared compiler-rejection
path; it does not establish transactional replacement after an interrupted
link or arbitrary build failure. Each boot also requires clean shutdown and
a clean read-only `e2fsck -f -n` result.

## Remaining M15 scope

This is a controlled C tool source-reconstruction profile. It does not establish
general configure-project compatibility, GCC single-stage or multistage
bootstrap, LLVM/rustc/Cargo source construction, a guest-built kernel, guest
boot-image generation or two independent clean builds. The separate pinned
[GCC assessment](M15_GCC.md) and [Rust/LLVM/Cargo assessment](M15_RUST.md)
supply M15.3/M15.4 source auditing and guest prerequisite/blocker probes
without constructing compiler components. M15.5 needs preserved
guest kernel ELF and actual boot evidence; M15.6 needs repeated clean builds,
artifact comparison and recovery within its declared completed scope.
