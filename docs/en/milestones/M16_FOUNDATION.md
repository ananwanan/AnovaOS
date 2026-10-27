<!-- Plan measured resource growth, filesystem isolation and independently validated ext4 adoption. -->
# M16: resources, VFS and controlled ext4

**English** | [简体中文](../../zh-CN/milestones/M16_FOUNDATION.md)

Planned 2026-10-26 (Asia/Shanghai) against `dev` baseline `05a5fb1`.
**Updated 2026-10-27 against `616ff47`: backend/cache isolation and a controlled ext4
readonly profile are implemented and locally verified.** Dependency acquisition is repaired;
JBD2 recovery and fresh-disk migration/build responsiveness now have separate local
acceptance in [M16.5](../validation/M16.5.md)/[M16.6](../validation/M16.6.md).
The fresh-ext4 kernel capacity/recovery profile is locally accepted in
[M16 capacity](../validation/M16_CAPACITY.md); full remote CI is pending. See [M16 evidence](../validation/M16.md). The stage contracts below retain their full exits.
See the [roadmap](../ROADMAP.md), [current status](../guides/STATUS.md) and
[architecture](../ARCHITECTURE.md) for implemented boundaries.

M16 expands the development foundation before the [M17 developer desktop](M17_DESKTOP.md).
Full compiler self-hosting is a conditional M18 follow-up; it does not gate a
usable developer desktop. SMP, physical storage drivers, general Linux software
and a new AnovaOS disk format remain separately scoped. AnovaFS is not on this
critical path. Preserve the accepted ext2 development and recovery route.

## Baseline and sequencing

The current developer disk is 16 MiB with 512 inodes. Persistent files allow
4 MiB; tmpfs and readonly shared-file caches allow 1 MiB. VFS has 2,048 nodes,
128 shared open descriptions and 32 descriptors per process. The kernel has a
fixed 16 MiB heap and 32 total process/thread slots on one BSP; the kernel is
not preemptible and device I/O is polled. Raising QEMU RAM does not change these
software bounds. Linux/native ELF and startup limits remain separate.

The ext2 implementation accepts one 4 KiB-block group, at most 32,768 blocks
and 4,096 inodes, and direct/single-indirect regular files. These are limits of
our current implementation, not ext2 format maxima. The pre-M16 VFS directly owned `Ext2`, imported the complete persistent namespace
and reserved node capacity up front. It now owns a capability-described backend, loads
disk names on demand, grows fallibly from 32 cache slots and reclaims inactive regular
files within the unchanged 2,048-slot ceiling. Directories/cwds and active files/maps stay pinned. A larger disk alone cannot supply scalable namespace or cache behavior.
Historical [GCC](M15_GCC.md) and [Rust](M15_RUST.md) audits retain their original
1 MiB file profile; M15.5 raised current persistent files to 4 MiB.

Observed 2026-10-26 at baseline `05a5fb1`, the existing
[CI run](https://github.com/ananwanan/AnovaOS/actions/runs/40116492704)
has a successful `boot` job and a failed `guest-toolchain` job: downloading the
musl.cc native seed returned curl error 28 connecting to HTTPS port 443;
subsequent dependent steps were skipped. This is a seed-acquisition observation,
not evidence of a kernel failure or successful guest toolchain regression.

Execute M16.1 through M16.6 in order, retaining an independently usable ext2
profile. Resource and POSIX work may iterate as actual ext4/build results expose
new requirements. Every stage needs its own evidence and failure boundaries.

| Stage | Planned result | Status |
| --- | --- | --- |
| M16.1 | Recovered CI baseline and measured workload/resource budget | Fresh-ext4 kernel capacity/recovery locally accepted; complete remote CI pending |
| M16.2 | Filesystem backend boundary and bounded, scalable resource management | Backend/cache locally verified; quotas unchanged |
| M16.3 | Controlled ext4 readonly mount and data access | Readonly-v1 locally verified; see evidence |
| M16.4 | Controlled ext4 writes and declared POSIX semantics | Implemented opt-in write-v1; [profile/evidence](../validation/M16.4.md) |
| M16.5 | JBD2 recovery and verified durability | Opt-in checksum-v3/data-journal profile locally accepted; [evidence](../validation/M16.5.md) |
| M16.6 | Fresh-disk migration, real builds and responsive foreground use | Retained ext2 copy, guest bzip2 and UART budgets locally accepted; [evidence](../validation/M16.6.md); default unchanged |

## M16.1: CI baseline and measured workload

- Restore reproducible acquisition of the locked seed; preserve its source,
  license, inventory and hashes. Diagnose networking separately from build/runtime
  failures; do not replace failed tests with successful downloads or stale reports.
- Run the applicable existing CI jobs and retain actual run/commit/job identities,
  executed checks and skips. Local acceptance and remote CI stay separate.
- Declare a real source-build workload, exact source closure, tools, dependencies,
  build/host/target, ABI/sysroot and sequential recipe before sizing capacity.
- Measure file/inode/node counts, largest inputs/outputs, temporary storage,
  heap/pages, descriptors, task slots and wall time. Define explicit resource
  ceilings and foreground command/input latency budgets before the comparison.

Exit: a reviewable successful CI baseline for the executed scope, a reproducible
workload and a justified resource budget. Scheduler-boundary samples are not
exact peak RSS; clean-shutdown disk usage is not transient storage demand.
Do not promise arbitrary capacities from complete upstream archive inventories.

## M16.2: filesystem boundary and resource management

- Separate persistent filesystem operations from the concrete ext2 type, root
  inode number, file limit and timestamp precision. Represent mount/node identity,
  filesystem capabilities and explicit unsupported operations without changing
  frozen native v1 ABI or the immutable mounted executable-ABI marker.
- Introduce bounded on-demand lookup/directory iteration and reclaimable node/cache
  state rather than importing every disk entry. Preserve open/unlinked identity,
  shared offsets, mapping coherence and name/path checks across cache eviction.
- Expand heap, VFS/cache, task/fd and user mapping capacity only as justified by
  M16.1. Define growth/quotas, reclaim and fallible metadata paths; retain guard
  pages, W^X, ownership and complete allocation rollback. More slots do not imply SMP.
- Keep ext2 byte compatibility, accepted error behavior and independent old-image
  mounting/build/reboot gates. Verify low-memory, full-cache and exhaustion reuse.

Exit: the existing ext2 workload passes through the backend boundary, namespace
and metadata costs stay within declared budgets, and pressure cannot corrupt
other mounts/processes or prevent the retained recovery route.

## M16.3: controlled ext4 readonly access

- Pin an explicit ext4 image profile and e2fsprogs version/creation options;
  enumerate accepted compat/incompat/readonly-compatible features. Unknown or
  unsupported combinations fail clearly instead of silently enabling writes.
- Validate checked geometry, device bounds, multiple block groups, descriptors,
  inode layouts, extents, allocation references and required metadata checksums.
  Bound tree traversal, cycles, overlap and resource use for untrusted images.
- Implement actual readonly lookup/readdir/stat/read on that profile, including
  cross-group, fragmented, large-file and sparse fixtures with independent hashes.
- Reject dirty filesystems requiring journal replay until recovery is implemented;
  readonly mount must not pretend that unreplayed metadata is a consistent view.

Exit: guest bytes and declared metadata match independent host inspection;
malformed/unsupported/recovery-required images fail without disk mutation.
Readonly acceptance does not establish write, journal or crash recovery support.

## M16.4: controlled writes and POSIX semantics

The explicit `ext4-write` kernel profile implements this bounded non-journal stage.
See [M16.4](../validation/M16.4.md) for admission, capacities, semantics, commands and actual tests.
M16.1 budgets/remote CI and M16.5/M16.6 retain their separate exits.

- Add bounded allocation/free, create/read/write/resize, directories, rename,
  unlink/open lifetime, executable modes and timestamps for the declared profile.
  Specify replacement, zero-fill/sparse behavior, links and unsupported operations
  according to the real workload; never fake success to satisfy build probes.
- Preserve file mappings, descriptor/cwd lifetimes, mount identity and concurrent
  process behavior. Record capacity limits and rollback for every mutation path.
- Make the writable feature set and journal state explicit. Until M16.5, exclude
  journal-required profiles and do not bypass a replay requirement. Any temporary
  non-journal profile has a declared recovery limitation and independent fixtures.
- Keep the profile opt-in. Test exact persisted bytes and metadata, ENOSPC/OOM,
  invalid images, actual read/write/flush errors and clean reboot with host checks.

Exit: real file tools and the selected build subset work on the writable profile,
old outputs survive declared failures, and stopped images pass the specified
e2fsprogs checks. Clean reboot proves only its stated persistence scope; ext4
does not become the default work disk at this stage.

## M16.5: JBD2 recovery and durability

- Declare supported journal location/features, transaction boundaries, ordered
  data/metadata policy, checksums, commit/replay/revoke behavior and abort states.
  Implement the selected JBD2 profile with bounded parsing and resource ownership.
- Define fsync/sync, rename durability and flush/barrier assumptions on the actual
  block-device path. Report write/flush failures; never mark a failed commit clean.
- Inject interruption at each relevant write/commit/flush point, plus partial
  device writes, failed flushes and torn/corrupt journal records. Reboot and replay
  using independent disks, preserving the original failed image for inspection.
- Compare allowed pre/post-operation states, namespace and allocation invariants,
  file hashes and replay idempotence. Cross-check stopped copies with e2fsck;
  prohibit double allocation, leaked live ownership and exposure of uninitialized data.

Exit: the declared journal/durability contract survives the full fault matrix,
including rejected corruption and recoverable interruption. Normal shutdown or
two matching clean builds cannot substitute for crash/replay evidence.

## M16.6: migration and usable development

- Create a fresh ext4 work disk and copy files from the retained ext2 source;
  preserve declared paths, bytes, modes, timestamps and ABI policy. Keep inventories
  and hashes, verify reboot/recovery and leave the source disk recoverable.
  Do not perform irreversible in-place conversion or silently reformat user disks.
- Install and execute the actual POSIX build utilities and development inputs
  required by the selected workload. Rebuild real tools in the guest, run them,
  retain prior working versions after failure and compare source/output inventories.
- While building and stressing storage, verify foreground commands/input, process
  interruption, log retrieval, saving and clean shutdown against M16.1 latency
  budgets. Address measured blocking I/O/kernel paths rather than assuming SMP is required.
- Keep adoption opt-in until all dependent stages pass. Any later default-profile
  switch needs recorded acceptance, documentation and a retained ext2 boot/recovery
  choice. Do not infer guest-only image generation or compiler self-hosting.

Exit: an independently recoverable development disk supports the declared real
builds within resource and responsiveness budgets. This is the foundation for
M17's edit/build/run/diagnose desktop workflow, with self-hosting deferred to M18.

## Continuing acceptance and implementation references

Preserve the existing gates in [DEVELOPMENT](../guides/DEVELOPMENT.md), including
runtime/storage/persistence, C/C++/Rust/Cargo/network, SDK and M14 GUI/GPU checks.
Retain all five M15 `bootstrap-*-test`/`bootstrap-test` gates, source rejection
suites and the `bootstrap-metrics` feature check. Introduce an M16 command only
with its working implementation. `cargo xtask foundation-test` now runs the actual
readonly-v1 acceptance; it does not accept M16.4–M16.6. The separate `foundation-write-test` verifies only M16.4 write-v1.
Record exact inputs, feature profiles, commits/worktree hashes, QEMU configuration,
logs, artifacts and independent disk checks; keep translations and status current.

Format references: Linux kernel [ext4 overview](https://www.kernel.org/doc/html/latest/filesystems/ext4/overview.html),
[inode data/extents](https://www.kernel.org/doc/html/latest/filesystems/ext4/ifork.html),
[journal/JBD2](https://www.kernel.org/doc/html/latest/filesystems/ext4/journal.html),
and [e2fsprogs](https://e2fsprogs.sourceforge.net/). Pin the actual implementation
inputs and fixture tool versions; reference documentation is not compatibility evidence.

## Kernel capacity closeout

The additional `cargo xtask foundation-capacity-test` extends the existing guest
kernel recipe to a fresh JBD2 disk, with predeclared budgets, separate exhaustion/
construction sessions and successful-mutation disk high-water accounting. See
[M16 capacity](../validation/M16_CAPACITY.md) for the actual execution and CI result;
implementation does not itself close M16.1.
