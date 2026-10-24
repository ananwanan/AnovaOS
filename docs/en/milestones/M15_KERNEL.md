<!-- Define guest kernel reconstruction, exact retained-ELF packaging and reboot acceptance boundaries. -->
# M15.5: guest kernel reconstruction

**English** | [简体中文](../../zh-CN/milestones/M15_KERNEL.md)

This profile reconstructs the actual AnovaOS kernel with the inherited M12
Rust/Cargo tools running in AnovaOS Linux-musl processes. The host prepares
locked inputs and packages the retained guest ELF into a bootable image. These
are separate capabilities from rebuilding Rust, rebuilding its target libraries,
or generating an entire boot image inside the guest. Executed acceptance belongs
in [current status](../guides/STATUS.md) and [local evidence](../validation/M15.5.md).

## Commands and prerequisites

Use the pinned host Rust toolchain and host utilities from
[DEVELOPMENT](../guides/DEVELOPMENT.md), including `musl-gcc`, `xorriso`, QEMU, OVMF and
`e2fsck`. Prepare the inherited M12 Rust tools if they are not already available:

```sh
cargo xtask fetch
cargo xtask rust-build
python3 ports/bootstrap/test_kernel_prepare.py
cargo xtask bootstrap-kernel-test
```

The gate creates a fresh work disk under `build/disks/`; it does not reformat
an existing disk. Run it exclusively with other image-building guest gates,
which share host-prepared application and runtime outputs. QEMU uses the normal
single-BSP q35/TCG profile, 3 GiB RAM and no network device.

Source preparation can also be inspected separately:

```sh
python3 ports/bootstrap/kernel_prepare.py
python3 ports/bootstrap/kernel_prepare.py --verify
```

Preparation downloads only the missing locked bare-metal target component,
requires the repository's locked crate archives in the host Cargo cache, and
never compiles a kernel. Missing registry archives require
`cargo fetch --locked` before preparation. A changed cached archive, redirected
output or altered published source tree fails explicitly. Re-running preparation
can refresh an intact snapshot after legitimate first-party source changes;
`--verify` only reads and rejects a stale snapshot.

## Inputs and machine roles

The host publishes these independently inventoried inputs:

| Host output | Guest location | Role |
| --- | --- | --- |
| `build/bootstrap-kernel/source` | `/ks/source` | Read-only exact kernel, local crate and linker sources, isolated workspace metadata, complete locked registry sources |
| `build/bootstrap-kernel/seeds/rs` | `/rs` overlay | Official inherited bare-metal target libraries and unchanged license notices |
| Existing M12 Rust runtime | `/rs` | Official inherited Linux-musl Rust/Cargo 1.90.0, rust-lld, host standard libraries and runtime dependencies |
| Host-compiled `kernel-build.elf` | `/bin/m15-kernel-build` | Build recipe, bounded log capture, ELF admission and successful publication |
| Fresh ext2 work disk | `/persist/kernel` | Cargo outputs, captured logs, retained ELF and failure fixture |

`ports/bootstrap/kernel-inputs.json` locks the official Rust 1.90.0
`x86_64-unknown-none` target archive by SHA-256. Its precompiled `core`, `alloc`
and `compiler_builtins` libraries are inherited seeds. They are supplied through
read-only initramfs storage, independently from writable-file capacity.
The [official target documentation](https://doc.rust-lang.org/rustc/platform-support/x86_64-unknown-none.html)
describes the freestanding target and its precompiled components; this target
does not provide a native AnovaOS `std`.

First-party kernel, crate and linker files retain their exact current repository
bytes. The prepared workspace excludes the host-only `xtask` package and changes
its default member to `kernel`; Cargo generates an isolated lockfile whose
package versions, registry identities and checksums must all match entries in
the repository lock. The original kernel and local crate manifests remain
unchanged. The copied Cargo configuration retains kernel target flags and adds
an offline directory-source replacement. Complete upstream crate archive files
and licenses are preserved; generated `.cargo-checksum.json` records authenticate
their prepared contents.

`build/bootstrap-kernel/inputs.json` separates original repository file hashes,
generated metadata, registry-package locks and inherited-library hashes. It
also records the source revision and worktree state. An uncommitted source
snapshot is identified by those exact file hashes; its base commit alone does
not identify the modified source. The complete source/seed namespace is checked
against archive path/component limits and a 1,500-entry preparation budget.
The kernel image omits `/tc` to retain room within the existing 2,048 VFS nodes.

## Guest compilation and publication

The driver changes to `/ks/source` so Cargo loads the isolated offline vendor
configuration. The actual guest compilation command is:

```sh
cargo build --offline --locked --release \
  --target=x86_64-unknown-none --features=bootstrap-metrics \
  --manifest-path=/ks/source/Cargo.toml -p anova-kernel \
  --target-dir=/persist/kernel/target
```

The driver supplies one build job, no incremental cache, `opt-level=z`, one
codegen unit, no debug information, stripped symbols and aborting panics. Target
flags select static relocation, the kernel code model and no red zone. Cargo's
explicit target keeps these kernel flags separate from Linux-musl build-script
processes. The inherited Rust launcher selects `/rs` as sysroot and rust-lld as
linker, with one linker thread and file-write output rather than writable shared
mapping. No host target object, rlib or kernel ELF is substituted into this build.

The new build identity covers the prepared source and target-library inventory,
the driver/launcher/orchestration sources and the inherited Rust runtime inventory. The driver passes
that identity through `ANOVA_KERNEL_BUILD_ID`; the kernel embeds it and reports
`[BOOT:BUILD] id=...` after serial initialization. This identity ties the accepted
boot to its declared inputs and recipe. The independently recorded ELF SHA-256
identifies the exact output bytes.

The launcher preserves the driver's explicit `TMPDIR=/persist/kernel/tmp`,
so intermediate metadata can exceed the unchanged tmpfs file limit. It accepts
up to 80 arguments while reserving space for its injected options; the locked
smoltcp invocation requires 71 arguments.

Cargo output is captured to a bounded persistent log while also appearing in
the serial transcript. Only successful compilation proceeds to publication.
The driver checks a static ELF64 x86_64 image, checked program-header bounds,
high-half load addresses, executable entry coverage and absence of writable
executable segments. It copies the completed file to `kernel.candidate`, syncs
and validates it, then renames it to `kernel.elf`. A compilation failure leaves
the prior accepted kernel intact.

The persistent ext2 file ceiling is 4 MiB, within its existing direct and single
indirect block layout. tmpfs remains limited to 1 MiB per writable file; the
disk format, 16 MiB work disk, 512 disk inodes, 16 MiB kernel heap, 32 task slots
and 2,048 VFS nodes remain bounded. The larger persistent ceiling is needed for
the selected smoltcp intermediate library. Each storage write keeps a private
indirect-table snapshot to bound repeated device reads during small compiler
writes; data/pointer/length publication ordering remains unchanged. This does
not remove the independent
compiler-source bootstrap prerequisites assessed in [M15_GCC](M15_GCC.md) and
[M15_RUST](M15_RUST.md).

## Retained-ELF packaging and actual reboot

The gate runs three separate QEMU sessions:

1. Boot the host-built seed kernel, compile the kernel inside guest processes,
   publish it and shut down normally. The host remounts the work disk, validates
   the persisted ELF and saves its exact bytes as
   `build/bootstrap-kernel/guest-kernel.elf`.
2. Package that retained ELF with the existing locked bootloader and application
   inputs, then boot the resulting guest-kernel image. This packaging path never
   invokes a host kernel build. The host independently extracts
   `/boot/kernel.elf` from the ISO and compares every byte before booting it.
   The running kernel must report the expected identity and pass boot/runtime
   commands. An actual rejected Rust compilation must preserve the accepted
   kernel, followed by normal shutdown. This probe compiles an intentionally
   invalid small Rust source to the unpublished candidate path; it is not a
   complete failed kernel Cargo rebuild.
3. Boot the same image again, verify its identity and application execution,
   shut down normally and compare all retained workspace file hashes with the
   preceding recovery state. Each stopped disk is independently checked with
   `e2fsck -f -n`.

The gate records source/seed inventories, initial host kernel identity, the
host-compiled driver, initramfs and ISO hashes, guest ELF bytes/hash, serial and
compiler logs, shutdown results and resource measurements. Scheduler-boundary
sampling is not exact peak RSS; its elapsed IRQ clock also omits some kernel
execution. Only a fully successful gate publishes
`build/logs/bootstrap-kernel.result.json` with `result=Ok`.

Host assistance still creates the work disk, supplies inherited compilers and
target libraries, constructs the first boot image, extracts the guest output,
packages the second image and drives verification. Guest-only image generation,
new compiler construction, GUI-enabled kernel reconstruction and general
reproducibility require their own evidence. The separate [M15.6 repeat
profile](M15_REPEAT.md) now has [local acceptance](../validation/M15.6.md) for
two clean kernel ELFs and bzip2 outputs; raw kernel intermediate differences
are retained and are not claimed byte-identical. Normal shutdown/reboot recovery does not establish power-loss
recovery for the non-journaled ext2 implementation.
