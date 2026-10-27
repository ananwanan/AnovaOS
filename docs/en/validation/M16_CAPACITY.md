<!-- Record the ext4 kernel workload, declared budgets, exhaustion recovery and actual CI evidence. -->
# M16 kernel capacity and CI closeout

**English** | [简体中文](../../zh-CN/validation/M16_CAPACITY.md)

Recorded 2026-10-27 (Asia/Shanghai) on `feat/m16-foundation`, implementation base
`91d2b32`. The new gate is implemented; actual acceptance results are recorded
below after execution. Existing [M16.5](M16.5.md)/[M16.6](M16.6.md) evidence keeps
its original bzip2 and recovery scope. Default storage remains ext2.

## Reproducible workload and budgets

```sh
cargo xtask fetch
cargo xtask toolchain-build
cargo xtask rust-build
cargo fetch --locked
cargo xtask foundation-capacity-test
```

The gate uses the existing [M15 kernel recipe](../milestones/M15_KERNEL.md): exact
current kernel/local-crate/linker sources, authenticated offline Cargo crates and
inherited official Rust/Cargo/rust-lld 1.90.0 plus matching precompiled bare-metal
libraries. Build/host is AnovaOS's x86_64 Linux-musl personality, target is
`x86_64-unknown-none`, one job/codegen unit, opt-level z, stripped, panic abort.
The selected features are `ext4-write,bootstrap-metrics`. Source inputs remain
readonly in the initramfs; all build output and temporary files use the new disk.
This reconstructs the kernel, not the inherited compiler or a complete boot image.

Before comparison, `budgets.json` records:

| Resource | Ceiling |
| --- | --- |
| Machine | q35/TCG/OVMF, 1 BSP, 3 GiB RAM, no network |
| Kernel heap | 16 MiB |
| Sampled frame delta | 524,288 pages of 4 KiB (2 GiB) |
| VFS slots | 2,048 |
| Process/thread slots; descriptors/process | 32; 32 |
| Fresh JBD2 work disk | 64 MiB, 4,096 inodes, 8 MiB internal journal |
| Each session watchdog | 3,600 seconds |

Memory samples occur at scheduler boundaries and omit temporary syscall peaks;
frame deltas include kernel allocations and are not exact peak RSS. Ext4 tracks
allocation counters after every successful mutation, retaining per-mount block/
inode peaks even when temporary files are later removed. These counts include
filesystem metadata/journal, exclude rejected or failed transactions and reset
on remount. Final current counts must match the independently read stopped
superblock and the image must pass `e2fsck -fn`.

## Acceptance sequence and evidence

1. Create a fresh JBD2 disk with an explicit Linux ABI marker; require no previous
   kernel workspace. Boot and reach real heap OOM, EFBIG/EMFILE/EAGAIN rejection,
   all 2,048 VFS slots twice, and disk ENOSPC. Check offsets/bytes, removal and
   resource reuse; execute a native app and cleanly shut down.
2. On that same recovered disk, build the actual kernel from a clean workspace.
   Record build-only memory/VFS/fd/task samples and transient disk peaks separately
   from deliberate pressure. Independently inspect the retained guest ELF and all
   persisted workspace bytes.
3. Package the exact retained ELF without a host kernel rebuild, extract/compare
   ISO bytes and boot the new kernel. Run actual rustc syntax rejection, retain the
   old working ELF, execute the native app and shut down cleanly.
4. Boot the identical new kernel again; require the exact embedded identity and
   unchanged workspace inventory. Every stopped disk must pass e2fsck.

Evidence stays under `build/foundation/capacity-TIMESTAMP/`: budgets, full input/
implementation hashes, drivers, initramfs, seed/new-kernel ISOs, work disk, guest
ELF, per-session UART/QEMU/result files and result JSON. CI uploads this directory
alongside existing foundation evidence on success or failure. Failed sessions
retain their diagnostics and never publish a successful result.

## Remote CI execution

The workflow supports manual dispatch and pushes to `dev`/`feat/m16-foundation`,
plus PRs targeting `dev`. Both complete jobs and all previous gates remain.
The guest-toolchain watchdog is 180 minutes to include the additional independent
ext4 kernel build. No failure is ignored. An installed GitHub connector supports
reading runs and publishing commits but exposes no workflow-dispatch operation.
The browser login was attempted; authentication did not produce a verified signed-in
page and subsequent browser operations timed out. A branch push therefore supplies
an independent executable trigger; its actual event must be recorded as `push`,
not `workflow_dispatch`. No manual-dispatch success is claimed without a run.

Actual run/commit/job/artifact identities and final results are pending execution.
