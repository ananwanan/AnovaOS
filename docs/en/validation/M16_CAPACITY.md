<!-- Record the ext4 kernel workload, declared budgets, exhaustion recovery and actual CI evidence. -->
# M16 kernel capacity and CI closeout

**English** | [简体中文](../../zh-CN/validation/M16_CAPACITY.md)

Recorded 2026-10-27 on `feat/m16-foundation`. Local capacity acceptance passed
in a clean isolated checkout of `e2db30f9752d3e29cb6c6734498cda418071036c`.
The later `1b63f42` change adds an identical-hash seed transport and CI evidence;
it does not change kernel/runtime code. Run #57 finished with core success and an
ext4 fixture admission failure; complete remote acceptance remains open. The owner
then restricted Actions to main pushes, so feature-branch reruns are paused. Historical [M16.5](M16.5.md) and
[M16.6](M16.6.md) retain their original recovery/bzip2 scopes. Default storage is ext2.

## Reproduce the declared workload

```sh
cargo xtask fetch
cargo xtask toolchain-build
cargo xtask rust-build
cargo fetch --locked
cargo xtask foundation-capacity-test
```

This uses the accepted [M15 kernel recipe](../milestones/M15_KERNEL.md): exact
kernel/local-crate/linker sources, locked offline Cargo dependencies and inherited
official Rust/Cargo/rust-lld 1.90.0 with matching precompiled core/alloc/compiler_builtins.
Build/host is AnovaOS's x86_64 Linux-musl personality; target is
`x86_64-unknown-none`, features `ext4-write,bootstrap-metrics`, one job/codegen unit,
opt-level z, stripped, panic abort. This reconstructs the real kernel ELF.

All build output and temporary files use a **fresh 64 MiB JBD2 work disk**:
4 KiB blocks, 4,096 inodes and an 8 MiB internal journal. Inputs/tools stay readonly
in the separate initramfs; 64 MiB is not a claim that the compiler and sources fit
on that work disk. The local initramfs is 717,902,503 bytes. The recorded additional
kernel source/target-library closure has 957 files and 82,073,888 bytes.

Budgets are written before execution: q35/TCG/OVMF, 1 BSP, 3 GiB guest RAM, no network,
16 MiB kernel heap, 2,048 VFS slots, 32 total process/thread slots, 32 FDs/process,
524,288 additional 4 KiB frames (2 GiB), and 3,600 seconds per session. Host tools were
Rust 1.90.0, QEMU 8.2.2 and e2fsprogs 1.47.0.

## Actual local measurements

| Resource | Declared ceiling | Deliberate pressure | Clean kernel construction |
| --- | ---: | ---: | ---: |
| Sampled heap bytes | 16,777,216 | 16,774,240; actual allocation failure | 9,494,480 |
| Sampled additional 4 KiB frames | 524,288 | 8,764 | 142,825 |
| VFS slots | 2,048 | 2,048 | 1,564 |
| Live / occupied task slots | 32 | 32 / 32 | 14 / 14 |
| FDs per process | 32 | 32 | 16 |
| Allocated 4 KiB disk blocks, peak | 16,384 | 16,384; actual ENOSPC | 6,072 |
| Allocated inodes, peak | 4,096 | 13 | 241 |
| Host wall milliseconds including boot/shutdown | 3,600,000/session | 558,133 | 971,675 |

The construction frame delta is 585,011,200 bytes; allocated disk peak is
24,870,912 bytes **including filesystem metadata and journal**. The resulting
workspace contains 179 regular files / 14,618,936 bytes; its largest file is
2,202,508 bytes. These are observed requirements for this recipe, not minimum
capacities or proof for broader compiler/desktop workloads.

Memory is sampled at scheduler boundaries, not exact peak RSS; frame deltas include
kernel allocations and omit transient syscall peaks. Ext4 allocation peaks are
updated after every successful mutation, including temporary files later removed.
Rejected/failed transactions do not publish usage; counters reset on remount.
Each final current count matched an independent stopped-superblock read and
`e2fsck -f -n` passed.

## Exhaustion and recovery actually executed

1. Real tmpfs heap allocation failed with prior bytes and file offset retained.
   The guest freed resources and continued.
2. EFBIG preserved the existing file/offset; EMFILE reached 32 FDs (29 new plus
   inherited descriptors); EAGAIN reached 32 task slots (29 children plus existing
   tasks). The probe closed/reaped resources and reused them.
3. Two VFS directory cycles each allocated 712 additional pinned directories and
   reached all 2,048 global slots. Rejected creation left no inode; removal and
   subsequent file creation succeeded with no loss of slots in the second cycle.
4. A real write filled all 16,384 disk blocks, preserved prior data/offset on ENOSPC,
   then truncated/unlinked and reused blocks. After cleanup the disk returned to
   2,318 blocks / 12 inodes. A native app ran and the system shut down cleanly.
5. On that **same recovered disk**, the clean actual kernel build succeeded.
   Cargo reported 10m 57s in the guest. The host inspected the persisted ELF and
   packaged those exact bytes without rebuilding the kernel on the host.
6. The new kernel booted with the exact embedded identity. A real Rust syntax error
   returned compiler status 1 and retained the bootable ELF. A native app still ran.
   The final recovery boot preserved every workspace file digest. Both new-kernel
   sessions shut down cleanly and passed independent filesystem checks.

The rejection session took 36,725 ms and peaked at 91,549 frames / 6,301,088 heap
bytes / 7 tasks / 8 FDs / 1,341 VFS slots. Recovery took 18,509 ms, peaking at
140 frames / 1,274,144 heap bytes / 3 tasks / 3 FDs / 1,335 VFS slots.
Both ended at 6,074 blocks / 243 inodes (the two retained diagnostic files account
for the increase after construction).

## Retained local evidence

The complete [local result and raw logs](../../assets/validation/M16_CAPACITY_LOCAL.json)
include predeclared budgets, all input/implementation hashes, four session results,
the complete host driver and UART logs, and per-log lengths/SHA-256 values.
Each log was decoded and re-encoded from the committed JSON and independently matched
its original SHA-256. The implementation digest remained unchanged throughout the gate.

| Identity | Value |
| --- | --- |
| Build identity | `c6b411cd182e1d9277e091b49c1234db8544e054b45bab476d9af264b890b573` |
| Guest kernel | 411,008 bytes |
| Guest kernel SHA-256 | `dfe9b864571f0c681b64633a661339c5a262c0755e00de88647c4ce46b30f44b` |
| Packaged ISO SHA-256 | `90a29e80b9b7937ed7a901ddb35e5d4e35d42d7aef2a84307252f012aa0fb67d` |
| Initramfs SHA-256 | `f7b1e19d1ca5099923ffed457b7e4e92c4e45433f4c06db8d40702d0d2f4bff7` |
| Implementation inventory SHA-256 | `baba52da3d37545c6c1de7adb5cc3a2d54dbd04ec206302f7fddaf6829b36147` |

The local raw image/ELF/ISO bundle remains in
`build/foundation/capacity-1793082851991418782/` of the isolated acceptance checkout.
Every CI execution uploads its own `build/foundation/` and logs on success or failure.

## Remote CI execution and repairs

The complete workflow retains both `boot` and dependent `guest-toolchain` and every
previous gate. **Current owner policy: only pushes to `main` trigger Actions.**
PR and manual-dispatch triggers are temporarily removed; pushes to `dev` and feature
branches do not trigger this workflow. The runs below used the earlier branch policy.
The guest-job watchdog remains 180 minutes. Within an executed baseline no gate is
allowed to fail; downstream skips after failure are recorded as skips.

| Run / attempt | Tested commit | Core | Guest toolchain | Actual observation |
| --- | --- | --- | --- | --- |
| [#54 / 1](https://github.com/ananwanan/AnovaOS/actions/runs/40457781527) | `92b8a38` | Passed | Failed | Locked OVMF primary HTTP 503 before guest gates |
| [#55 / 1](https://github.com/ananwanan/AnovaOS/actions/runs/40458309803/attempts/1) | `8eee351` | Failed | Skipped | Same OVMF transport failure |
| [#55 / 2](https://github.com/ananwanan/AnovaOS/actions/runs/40458309803/attempts/2) | `8eee351` | Failed | Skipped | Manually reran the root job and dependent job; same HTTP 503 |
| [#56 / 1](https://github.com/ananwanan/AnovaOS/actions/runs/40460883750) | `e2db30f` | Passed | Failed | Firmware fallback exercised; both musl seed origins timed out |
| [#57 / 1](https://github.com/ananwanan/AnovaOS/actions/runs/40463666492) | `1b63f42` | Passed | Failed | Source build/C++ passed; copied UID/GID 1001 caused ext4 mount rejection |

The manual retry is a GitHub job rerun of the complete dependency chain; its
original event remains `push`, not `workflow_dispatch`. Actual run/job/step boundaries and failures are
retained in [M16 CI evidence](../../assets/validation/M16_CI.json).

The official Launchpad OVMF fallback was downloaded and matched the existing package
SHA-256 `a094c13d06f2740691ff57d108dff32aa087179363ddb0de42d463b4f7f9bc13`.
Run #56 actually used it after primary HTTP 503. Firmware/version/extracted digests
did not change. Only transport errors allow fallback; wrong digests, corrupt or
redirected caches are fatal. Unique partials are cleaned; publication checks races.
Five added acquisition tests cover these boundaries; #56 passed 230 host tests.

Run #56 then failed before compiler construction because musl.cc and more.musl.cc
each timed out twice connecting to HTTPS. The additional community mirror
`https://joeyparrish.github.io/musl-cc-mirror/x86_64-linux-musl-native.tgz`
was independently downloaded: 89,080,066 bytes, SHA-256
`eb1db6f0f3c2bdbdbfb993d7ef7e2eeef82ac1259f6a6e1757c33a97dbcef3ad`.
It matches the existing seed lock; version/license/digest remain unchanged.

The #54 core ZIP (artifact `12712834879`, 16,179,053 bytes) was downloaded and
matched SHA-256 `0f876894f490bfd21784d9b6f6711569cd831fdd10a6af518f31957c9c8af44a`.
Its 45 [exact result records](../../assets/validation/M16_CORE_CI54.json) are retained.
This is core-only evidence; failed/skipped guest gates are not accepted.

## Corrections and remaining scope

The first VFS probe used mkdir mode 0700 outside the admitted Linux profile;
the corrected probe uses 0755 without extending permissions. An exploratory
construction boot reached the separate host 8 GiB memory limit during module
loading; the driver now drops packaged byte buffers before QEMU. Those interrupted
sessions are not accepted; the successful isolated run above uses the repaired code.

The declared fresh ext4 kernel workload fits the unchanged software quotas after
real exhaustion/recovery. Default storage remains ext2; arbitrary ext4 formats,
compiler source reconstruction, guest-only image generation, larger GUI workloads,
SMP and physical hardware require separate acceptance. The next desktop work is
[M17](../milestones/M17_DESKTOP.md) after the complete remote baseline is accepted.

The successful #57 core job is also retained as 45 [current-code result records](../../assets/validation/M16_CORE_CI57.json).
Artifact `12715692453` (16,179,359 bytes) was independently downloaded and matched
SHA-256 `76d51dbf684ce5a86dbed3ef07b8281cefecb7a06355fd12f3778cb3076c58d9`.
The dependent guest job subsequently failed the fixture ownership check; this ZIP accepts the core scope only.

## Final remote observation and main-only policy

Run #57 successfully built the locked source toolchain and passed the guest C/C++
gate, then the ext4 fixture was rejected before writing. `mke2fs -d` copied runner
UID/GID 1001 into seed files, outside the existing root-owned writable profile.
The guest artifact (ID `12717126551`, 47,318,051 bytes) was downloaded and matched
SHA-256 `8c0d62cede96b215d74c38dfb4791bc26240fcff341d582629fea4240379e6aa`.
The exact failed UART and completed/skipped steps are retained in the CI JSON.

Fresh write/journal fixture constructors now explicitly set root ownership, normalize
only their declared seed paths with debugfs and independently audit the final image.
Kernel ownership admission remains unchanged. The added regression injects UID/GID
1001 into both formats, confirms rejection, repairs ownership and verifies marker
bytes and e2fsck. All 54 xtask tests and strict xtask Clippy pass.

The owner then requested main-only builds. No further feature-branch workflow is
started. Full remote revalidation of this fix and the downstream gates remains
pending on `main`; local kernel capacity success is not full remote CI success.
