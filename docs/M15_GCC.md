<!-- Define the locked GCC feasibility assessment, real guest blockers and prerequisites for compiler reconstruction. -->
# M15.3: GCC source-build feasibility

This profile implements a reproducible assessment of the existing GCC 11.2.0
route. It audits locked source archives, runs the inherited C++ compiler in the
guest, and diagnoses missing build tools, development headers and capacity.
A successful assessment can conclude that construction is blocked. It does
not produce a new GCC compiler stage. Actual local execution and acceptance
are recorded separately in [validation/M15.3](validation/M15.3.md).

## Run the assessment

Prepare the host utilities and verified M11 compiler/sysroot as described in
[M11_TOOLCHAIN](M11_TOOLCHAIN.md), then run:

```sh
cargo xtask fetch
cargo xtask doctor
cargo xtask toolchain-build
python3 ports/bootstrap/test_gcc_probe.py
cargo xtask bootstrap-gcc-test
```

The source-only assessment is independently repeatable:

```sh
python3 ports/bootstrap/gcc_probe.py
python3 ports/bootstrap/gcc_probe.py --verify
```

It requires the seven source archives already cached by `toolchain-build` in
`build/toolchain/`. It does not download missing inputs or trust previously
extracted trees. The exact archive URLs and SHA-256 values remain locked in
`ports/gcc/inputs.json`; the inherited `bootstrap.tgz` binary package is excluded
from this source-tree inventory. Every archive hash is checked before reading
members. The parser rejects ambiguous/duplicate paths, traversal, links and
special files. Only selected regular files at or below the guest's writable
file limit are published.

The script writes `build/bootstrap-gcc/source-audit.json` and a bounded,
source-only `build/bootstrap-gcc/source/`. Existing changed outputs or redirected
paths are rejected; retain intentional changes elsewhere before removing a
changed generated cache. `--verify` checks existing outputs without creating or
rewriting them. Each selected file records its originating archive, member,
length and SHA-256. `INPUTS.json` and `SOURCE.sha256` accompany the selection.
The original GCC configure startup scripts, release-specific prerequisites
and their notices are preserved without source patches. The selection is not
a complete GCC source tree or a configured compiler build.

## Machine roles and inherited tools

The assessed build, compiler host and application target use AnovaOS x86_64
through its explicit Linux-musl compatibility personality. This is separate
from the bare-metal kernel target and native application SDK. The candidate
compiler route would build C/C++ for `x86_64-linux-musl` with the existing
`/tc` sysroot and controlled static executable profile.

The seed is M11 GCC/G++ 11.2.0, binutils 2.37, make 4.4.1 and musl 1.2.4,
prepared on the host and verified against the complete installed inventory.
GMP 6.2.1, MPFR 4.1.0 and MPC 1.2.1 are locked compiler dependencies. GCC
11.2.0's own `INSTALL/prerequisites.html`, retained in the probe workspace,
requires a C++11 compiler, GNU make 3.80 or later, GMP 4.3.2 or later, MPFR
3.1.0 or later and MPC 1.0.1 or later, among other build facilities. The
selected versions meet these version requirements; availability and runtime
semantics still require actual guest testing.

The host audits/selects source files, packages inherited tools, compiles two
first-party diagnostics, creates the work disk, starts QEMU and inspects the
result. It does not rebuild GCC, its arithmetic dependencies or the seed-test
application. `/bin/m15-gcc-probe` and `/bin/m15-capacity` are diagnostics,
not newly constructed compiler components. The guest alone compiles the
C++11 test application.

## Guest probes and recovery contract

`bootstrap-gcc-test` creates a fresh 16 MiB Linux-personality work disk with
512 inodes, 4 KiB blocks and 128-byte inodes. It imports the selected sources,
notices and first-party fixtures into `/persist/gcc-probe`. The two QEMU
q35/TCG boots use one CPU, 1 GiB RAM and no NIC. Existing work disks are not
reformatted, and the ordinary image and ABI remain unchanged.

The first boot must satisfy all of the following:

- Guest `g++` compiles `seed.cpp` with `-std=c++11 -O1`; the resulting static
  executable runs a lambda, initializer-list/vector and unique-pointer test
  and reports value 42. This tests the inherited seed on one small fixture.
- Separate actual compiler invocations including `gmp.h`, `mpfr.h` and `mpc.h`
  fail with status 1 and the corresponding missing-header diagnostic. A
  previous successful seed executable remains usable.
- The diagnostic attempts `execvp` for `sed`, `awk`, `grep`, `expr`, `tr`,
  `sort`, `cmp`, `mv` and `ln` using `/bin:/tc/bin:/linux`. Each child must
  return an actual `ENOENT` error through a checked pipe and exit 127.
- It launches the unchanged GCC configure entry point with `/linux/sh`, an
  explicit `--srcdir=.`, matching build/host/target triples,
  `--disable-bootstrap`, `--enable-languages=c,c++` and `--disable-multilib`.
  Captured output must show the missing `sed` diagnostic, and configure must
  fail. A missing-source-directory rejection does not satisfy this check.
- The existing capacity diagnostic proves `EFBIG`, `EMFILE` and `EAGAIN`
  rejection with subsequent file, descriptor and fork/wait recovery. The
  seed executable must still run after these checks.

This is a configure **startup** subset. Keeping its selected entry scripts
allows diagnosis of the initial tool blocker; later configuration, compiler
sources, generated Makefiles and dependency tests are not exercised. Fixing
`sed` alone does not establish that the rest of configure can complete. The
diagnostic bounds captured output and the host session has a timeout; overflow,
a hang, an unexpected exit or absent error evidence fails the assessment.

After real clean shutdown the host independently inspects the disk, checks all
imported bytes, parses the guest-created seed ELF, verifies its executable mode,
and runs read-only `e2fsck -f -n`. The second boot executes that retained seed,
shuts down cleanly, repeats filesystem inspection/checking and requires every
persisted input and output hash to match. No new compiler stage is installed or
promoted; recovery concerns the existing seed and this diagnostic application.

## Audited capacity and interpretation

These deterministic counts come from the checksum-locked **complete release
archives**, including tests, documentation, unrelated targets and languages.
They are not the minimum dependency closure of a C/C++ compiler build.

| Release | Regular files | Logical file bytes | Files above 1 MiB |
| --- | ---: | ---: | ---: |
| GCC 11.2.0 | 108,258 | 620,324,686 | 65 |
| binutils 2.37 | 25,646 | 259,643,675 | 26 |
| make 4.4.1 | 395 | 8,780,526 | 0 |
| musl 1.2.4 | 2,622 | 3,407,989 | 0 |
| GMP 6.2.1 | 2,094 | 15,594,737 | 1 |
| MPFR 4.1.0 | 539 | 11,381,943 | 1 |
| MPC 1.2.1 | 340 | 3,693,749 | 0 |
| Total | 139,894 | 922,827,305 | 93 |

The combined full-tree namespace requires at least 145,684 nodes, including
implicit parent directories. Rounding only regular-file data to 4 KiB blocks
already requires 1,358,241,792 bytes, before directory blocks, inode tables,
compiler outputs or temporary files. These lower bounds exceed the existing
16 MiB/512-inode work disk and 2,048-node VFS. Even GCC alone exceeds them.
Importing the full archives is therefore unavailable in the current profile.
Reusing seed binutils/make/musl or pruning optional source trees changes the
input closure and requires its own audited inventory.

The 1 MiB writable-file limit is a separate obstacle. GCC's release
`Makefile.in` is 2,145,896 bytes, so an ordinary unmodified top-level build
input cannot be persisted even after tests and manuals are removed. Raising
QEMU RAM does not change this limit. A future route must support its actual
large inputs, generated files, archives and compiler ELF, with safe rejection,
rollback and old-image compatibility. This assessment does not infer a
sufficient new file or disk limit from one source file.

The first-boot resource summary includes intentional exhaustion probes;
it is not ordinary GCC build demand. Scheduler-boundary physical-frame and
heap samples omit transient syscall allocations and do not establish exact
RSS. Host wall time includes firmware, boot, UART and shutdown. Shutdown disk
inspection is not a transient storage peak. No full compiler build ran, so its
memory, task, descriptor, temporary-storage and elapsed-time requirements
remain unmeasured.

## Evidence and next implementation conditions

Only complete success writes `build/logs/bootstrap-gcc.result.json`. It embeds
`build/bootstrap-gcc/prepared.json`, including the source-audit contents,
source commit/worktree state, exact imported hashes, inherited tool inventories,
diagnostic hashes, ISO/initramfs identities and QEMU configuration. Per-boot
serial, QEMU and result files use the `bootstrap-gcc-probe` and
`bootstrap-gcc-reboot` prefixes in `build/logs/`. Actual acceptance and retained
artifact identity belong in [validation/M15.3](validation/M15.3.md).

Continuing to a first GCC stage requires a declared source closure and measured
work profile, functional POSIX shell/build utilities, available arithmetic
headers and compatible link libraries or guest builds of those dependencies,
and sufficient regular-file/disk/inode/VFS capacity. A static compiler's
embedded GMP/MPFR/MPC implementation does not supply reusable development
headers. Additional shell semantics and syscall gaps may appear after these
first blockers are removed and must be diagnosed instead of bypassed with
fabricated configure answers.

Start any later compiler reconstruction sequentially with an explicit
single-stage configuration and preserved prior working tools. Verify the new
compiler by compiling and running applications before considering stage2/stage3.
A multistage claim additionally needs recorded stage relationships and real
comparison evidence. Current single-stage reconstruction, multistage GCC
bootstrap, Rust/LLVM source construction and guest kernel/image construction
remain unverified. M15.4 and M15.5 keep their independent prerequisites.

## Capacity profile after M15.5

This assessment intentionally retains its accepted 1 MiB file-capacity profile
and source audit for comparison with its original validation record.
[M15.5 kernel reconstruction](M15_KERNEL.md) raises current persistent ext2
files to 4 MiB; tmpfs and readonly shared-file caches remain at 1 MiB. The
unpruned compiler source trees also exceed unchanged disk/inode/VFS bounds,
and their missing build prerequisites remain independent. The earlier audit
is historical capacity evidence, not a claim that the current persistent limit
is still 1 MiB or a new assessment of a pruned build closure.
