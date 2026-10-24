<!-- Define independent clean guest builds, retained evidence and bounded comparison/recovery acceptance. -->
# M15.6: repeated guest reconstruction

This profile repeats the completed [bzip2](M15_BOOTSTRAP.md) and
[kernel](M15_KERNEL.md) reconstruction recipes on independent clean work
disks. It compares their declared inputs and outputs while retaining the
existing functional, rejection and reboot checks. The implementation contract
is described here; actual execution and acceptance belong in the
[M15.6 validation record](validation/M15.6.md) and [current status](STATUS.md).

## Prepare and run

Prepare the locked boot inputs, the M11 GCC/sysroot and the M12 Rust runtime
using the commands documented in [DEVELOPMENT](DEVELOPMENT.md). Ensure the
locked registry archives required by kernel preparation are available:

```sh
cargo fetch --locked
cargo xtask bootstrap-repeat-test
```

Run the command exclusively. The repeated gate and the existing image-building
gates share generated application, image and preparation paths; concurrent
execution can replace inputs while another session uses them.

Each of two rounds runs the complete `bootstrap-test` and
`bootstrap-kernel-test` flows sequentially. Each component creates a new work
disk rather than cleaning or reusing the preceding build directory. The
bzip2 disk receives sources without prebuilt objects or an executable. The
kernel disk starts without a Cargo target directory, compiled objects or a
retained kernel. The second round receives the same declared sources, recipes
and inherited seeds; it does not receive first-round guest build outputs.

The bzip2 profile uses one q35/TCG CPU, 1 GiB RAM and no NIC. The kernel profile
uses one q35/TCG CPU, 3 GiB RAM and no NIC. Guest paths and compile flags remain
those of the component profiles. This is two independent clean build
workspaces under one host environment, not two independently provisioned hosts.

## Retained evidence

Each invocation receives its own directory below `build/bootstrap-repeat/`.
The run also retains `source/`, `source-files.json` and `environment.json`
for the complete repository snapshot and measured host tool identities. Its
`round-1/` and `round-2/` evidence is preserved before another component
invocation can replace shared outputs. Retain preparation manifests, source
and seed inventories, component result reports, complete serial/compiler logs,
guest output hashes and extracted artifacts. Each `round-N/bzip2/` and
`round-N/kernel/` retains `result.json`, `prepared.json`, `inputs.json`,
`invocation.log`, per-session logs, seed inventories, `initramfs.anovafs`,
`seed.iso` and a stopped `work.ext2` copy. Kernel evidence additionally retains
`guest.iso`, `kernel.elf` and `kernel.readelf.txt`. Each component has a complete
`inventory.json`. Original work disks also retain their distinct paths.

The manifests identify source revision and worktree state, exact source file
hashes, inherited seed tools and target libraries, guest environment, target,
flags, resource bounds and host participation. A commit identifier alone does
not identify an uncommitted source snapshot. The kernel's embedded build
identity covers its declared inputs and recipe and is checked again during
actual boot. The independent ELF hash identifies all output bytes.

`build/logs/bootstrap-repeat.result.json` is published with `result=Ok` only
after both rounds and comparison validation succeed. A failed or interrupted
invocation must not leave a stale successful aggregate result. The run's
comparison report retains observed differences even when they prevent
acceptance. The original component reports remain separately scoped evidence. Failed-child
canonical logs and manifests are copied into an explicitly `unvalidated/`
diagnostic directory; those files can be partial or stale and never establish
acceptance.

## Comparison contract

Require identical declared source, seed, recipe and environment inputs before
interpreting output equality. A changed input, missing artifact or reused work
disk invalidates the comparison. Compare all eight bzip2 objects, its linked
executable, the compressed golden sample and decompressed reference bytes.
Each component gate already checks executable admission, real execution and
independent host decoding against the official sample.

Compare complete kernel ELF bytes and hashes, retaining independent `readelf`
header, section, segment and note listings for both outputs. Any byte mismatch
fails the aggregate; the retained listings support diagnosis. Preserve differences
rather than removing timestamps, paths, build identities or metadata to make
the outputs match. Whole-file equality and functional agreement are separate
observations. A difference needs its recorded location and explanation before
it can support a qualified acceptance claim.

Fresh host disk names, filesystem metadata, host wall times, sampled resource
observations and compiler diagnostic timing can differ between rounds. ISO
creation timestamps can also change image hashes despite identical packaged
kernel bytes. Report those environmental and packaging differences separately
from declared final artifact comparison. Kernel intermediate libraries, generated
configuration and Cargo caches can differ; their raw inventory differences are
retained and do not support a claim that the entire build directory reproduces.
The kernel profile independently extracts
`/boot/kernel.elf` from each generated ISO and requires exact equality with
that round's retained guest ELF.

## Failure and reboot recovery

Each bzip2 flow builds and executes the tool, then reboots and invokes the
deliberate C compiler rejection. The previously accepted executable must still
run, and the entire persisted source/output inventory must remain unchanged.
Each kernel flow constructs and publishes the ELF, packages its exact bytes
without a host kernel rebuild, boots that ELF and checks its embedded build
identity. A real Rust syntax rejection must preserve the prior kernel, and a
further boot must retain the recovery workspace inventory.

The two component flows use three QEMU sessions each, for twelve sessions
across two rounds. Every normal shutdown must flush storage and acknowledge
DMA reset; every stopped work disk must pass read-only `e2fsck -f -n`.
The rejection fixtures are bounded C and Rust compile failures. They do not
prove transactional recovery from every interrupted link, complete failed
kernel Cargo build or power-loss scenario on the non-journaled ext2 backend.

## Scope

The host still prepares locked sources and inherited seeds, compiles the
initial kernel, applications and diagnostics, creates work disks, packages
ISOs, extracts retained artifacts and drives verification. Guest processes
compile the selected bzip2 target and actual kernel. Inherited GCC, Rust,
Cargo, rust-lld and precompiled bare-metal libraries remain seeds.

The [GCC](M15_GCC.md) and [Rust/LLVM/Cargo](M15_RUST.md) stages remain feasibility
assessments with their documented blockers. This gate does not reconstruct
those compiler components, generate a complete boot image inside the guest,
rebuild a GUI-enabled kernel or establish native AnovaOS `std`. Two matching
builds establish the recorded comparison under these inputs and conditions;
they do not establish general reproducibility, independence from inherited
compilers or bootstrap from source alone.
