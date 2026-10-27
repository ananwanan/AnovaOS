<!-- Describe locked reconstruction recipes, compiler feasibility inputs and host/guest responsibilities. -->
# Bounded bzip2 reconstruction

**English** | [简体中文](README.zh-CN.md)

This recipe selects the unmodified C sources, headers, notices, upstream
Makefile/manual and three sample/reference pairs from official bzip2 1.0.8.
`inputs.json` pins the SHA-256 plus the SHA-512 published by
[Sourceware](https://sourceware.org/pub/bzip2/sha512.sum). The upstream
`LICENSE` remains intact under its bzip2 license. No upstream source patches
are applied. The custom `Makefile.anova` is Apache-2.0 first-party code.

Run `python3 ports/bootstrap/prepare.py`, then `--verify` to check the full
prepared source inventory. Preparation downloads to `build/bootstrap`, verifies
both checksums before reading archive members and accepts only regular files
inside the exact archive root. It never runs a compiler. Selected file hashes,
machine roles and seed tools are saved in `inputs.json`, `source/INPUTS.json`
and `source/SOURCE.sha256`. A changed cache fails instead of silently replacing
its contents. The large optional PostScript manual and unrelated utilities are
excluded; the HTML manual, license and all compiled units are retained.

On the independently prepared Linux-personality work disk, in the guest:

```sh
cd /persist/bootstrap
make -f Makefile.anova -j1
./bzip2 -1 -c sample1.ref > sample1.guest.bz2
./bzip2 -dc sample1.guest.bz2 > sample1.guest.ref
./bzip2 -t sample1.guest.bz2
./bzip2 -t invalid.bz2
make -f Makefile.anova failure
```

The last two commands must fail. `failure` invokes the real guest compiler on
`rejected.c`, whose intentional `#error` must leave the prior compressor
untouched. The normal target directly links eight guest-compiled objects;
it does not rebuild GCC, libc or a separate libbz2 archive. GCC wrappers provide
static non-PIE linking and `/tc` as sysroot. `clean` removes only declared object
files and compressor outputs. The acceptance harness must independently compare
the reference bytes, compressed sample bytes and retained tool/source hashes
after clean shutdown and reboot. This controlled recipe does not establish
arbitrary upstream configure/Makefile compatibility or compiler bootstrap.

## Compiler source assessments

`gcc_probe.py` audits the seven M11 compiler dependency archives and publishes
bounded unchanged inputs for the [GCC feasibility gate](../../docs/en/milestones/M15_GCC.md).
`rust_probe.py` independently audits the locked official Rust 1.90.0 source
distribution, including the LLVM fork, Cargo and vendor contents. Its bounded
selection supports the [Rust/LLVM/Cargo feasibility gate](../../docs/en/milestones/M15_RUST.md).
Each command supports `--verify` for read-only verification of its generated
source cache, rejects changed or redirected outputs, and retains upstream
notices. These source-only preparations do not execute upstream build scripts
or reconstruct compiler components. The corresponding `test_gcc_probe.py` and
`test_rust_probe.py` check integrity, archive safety and publication rejection.

The first-party `rust-probe/` fixtures exercise inherited guest Rust/C++ seeds,
real missing-tool exec errors and an isolated offline dependency failure.
Successful seed applications are preserved and executed again after reboot;
they are distinct from source-built LLVM, rustc or Cargo. Exact artifact
identities and host participation remain in each gate's generated result.

## Guest kernel reconstruction

`kernel_prepare.py` snapshots exact first-party kernel/local-crate sources,
isolates Cargo metadata, authenticates the locked offline registry graph and
prepares inherited official bare-metal target libraries. `--verify` is read-only
and rejects stale or modified publications. `test_kernel_prepare.py` checks
archive safety, lock drift, source integrity and publication boundaries.

`kernel-build.c` is a host-built recipe/diagnostic that runs actual guest Cargo,
rustc and rust-lld, publishes only a checked kernel ELF and captures real syntax
failure while retaining the prior kernel. `cargo xtask bootstrap-kernel-test`
packages the retained ELF without rebuilding it on the host, compares extracted
ISO bytes and verifies actual boot identity plus recovery. See
[M15 kernel](../../docs/en/milestones/M15_KERNEL.md) for exact seed/source roles and bounds.

`cargo xtask foundation-capacity-test` reuses this kernel recipe on a fresh JBD2
ext4 work disk, separately measures deliberate exhaustion and clean construction,
and preserves exact kernel boot/failure recovery. See
[M16 capacity](../../docs/en/validation/M16_CAPACITY.md) for budgets and actual results.
