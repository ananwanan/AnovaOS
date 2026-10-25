<!-- Define independent Rust, LLVM and Cargo source-build assessments and their seed/provenance boundaries. -->
# M15.4: Rust, LLVM and Cargo feasibility

**English** | [简体中文](../../zh-CN/milestones/M15_RUST.md)

This profile assesses three source-build routes separately using the official
Rust 1.90.0 source distribution. It audits complete archive contents and tests
selected prerequisites in the actual AnovaOS guest. The resulting report can
successfully establish that the current profile is blocked. It does not rebuild
LLVM, rustc, Cargo, their libraries or the kernel. Executed local results belong
in [validation/M15.4](../validation/M15.4.md).

## Run the assessment

Prepare the host utilities, M11 GCC/sysroot and M12 official musl tools first:

```sh
cargo xtask fetch
cargo xtask doctor
cargo xtask toolchain-build
cargo xtask rust-build
python3 ports/bootstrap/test_rust_probe.py
cargo xtask bootstrap-rust-test
```

The source-only preparation and read-only verification are also available:

```sh
python3 ports/bootstrap/rust_probe.py
python3 ports/bootstrap/rust_probe.py --verify
```

[`ports/bootstrap/rust-inputs.json`](../../../ports/bootstrap/rust-inputs.json)
identifies the dated official source archive, published checksum URL and SHA-256.
Preparation verifies the archive before reading members, records complete-tree
and separate component inventories, and publishes only bounded unchanged
upstream files under `build/bootstrap-rust/source/`. `INPUTS.json` and
`SOURCE.sha256` record each selected file's archive member, size and digest.
The complete source distribution is not imported into the guest. Changed caches,
unsafe archive paths and redirected output paths are rejected. `--verify`
checks the existing publication without rewriting it.

## Component boundaries and prerequisites

The proposed build/host/target is `x86_64-unknown-linux-musl` through AnovaOS's
explicit Linux compatibility personality. This is separate from native SDK
applications and the `x86_64-unknown-none` kernel target. Existing M12 tools are
inherited official Rust 1.90.0 binaries; existing M11 tools are inherited
GCC/G++ 11.2.0, binutils 2.37, make 4.4.1 and musl 1.2.4. Their ability to
compile applications establishes seed functionality within those tests only.

| Component | Source route | Current prerequisites and interpretation |
| --- | --- | --- |
| LLVM | Rust's included LLVM 20.1.8 fork, independently inventoried | Requires a working C++17 compiler, CMake 3.20.0 or later and a build generator. Ninja is optional when a supported make generator is used. A small C++17 test does not establish full LLVM compatibility. |
| rustc | Rust 1.90.0 compiler/bootstrap sources | The pinned `src/stage0` specifies Rust 1.89.0 dated 2025-08-07. Upstream bootstrap starts through Python, builds its Rust bootstrap program, then builds std/compiler stages with Cargo. The M12 1.90.0 seed does not establish that prescribed stage0 is installed or functional. |
| Cargo | Cargo package 0.91.0 included in the Rust 1.90.0 distribution | Its package manifest requires rustc 1.88 or later; the workspace default is 1.86. Requires Rust/Cargo seeds, its own locked dependency graph and C build dependencies. Optional system-library discovery and OpenSSL/vendored-library choices must be resolved for the selected musl configuration. M12's small application vendor cache is not Cargo's dependency closure. |

The retained upstream build files and manuals are the authoritative
release-specific requirements. The following official references explain the
route; moving branches and general guides do not override the audited release:

- [Rust 1.90.0 installation instructions](https://github.com/rust-lang/rust/blob/1.90.0/INSTALL.md).
- [Rust 1.90.0 stage0 lock](https://github.com/rust-lang/rust/blob/1.90.0/src/stage0).
- [Rust 1.90.0 bootstrap implementation guide](https://github.com/rust-lang/rust/blob/1.90.0/src/bootstrap/README.md).
- [Rust 1.90.0 component declarations](https://github.com/rust-lang/rust/blob/1.90.0/.gitmodules).

Cargo's source-package version above is distinct from the distributed command's
`cargo 1.90.0` version. The Rust source identity is
`1159e78c4747b02ef996e55082b704c09b970588` in the release archive; it does not
depend on a mutable LLVM/Cargo branch name.

No source-built component is installed or promoted by this assessment. A future
route that reuses prebuilt LLVM must identify it as inherited, rather than
claiming LLVM construction. Alternate stage0 choices require separate explicit
configuration and actual validation; they cannot silently replace upstream's
release-specific seed requirement.

## Actual guest checks and reboot contract

`bootstrap-rust-test` creates a fresh 16 MiB Linux-personality work disk with
512 inodes. It uses two q35/TCG boots, one CPU, 3 GiB RAM and no NIC, preserving
the ordinary ABI, runtime and filesystem limits. The larger RAM setting hosts
the existing M12 tool image; it does not raise writable-file or namespace limits.

The first boot must compile and execute a Rust std application with result 42
and a separate C++17 application with result 42 using the inherited compilers.
The diagnostic attempts actual execution of `python3`, `cmake`, `ninja`, `git`,
`perl` and `pkg-config`, checks each child's reported `ENOENT` and exit 127,
and captures a genuine Cargo offline dependency-resolution failure. The Cargo
fixture selects a dependency from the pinned upstream manifest; it is an
isolated dependency probe, not an attempted complete Cargo build. Missing Ninja,
Git, Perl or pkg-config alone does not prove they are mandatory for every
possible source/build configuration.

The diagnostic also attempts `python3 rust-x.py --help` against the retained
unchanged entry file. Its actual exec failure establishes that the selected
bootstrap entry cannot start in this profile; it does not execute Python or
establish a later upstream bootstrap failure.

That dependency is `curl = "=0.4.48"`, matching the audited Cargo manifest and
lockfile. The fixture's `.cargo/config.toml` selects the inherited `/rs/vendor`
directory. Cargo must return 101 with the actual missing `curl` diagnostic;
that establishes the selected offline vendor profile lacks this dependency.
It does not test the complete release vendor tree or all Cargo prerequisites.
The `src/stage0` requirement is audited statically and the inherited seed's
version is recorded separately; no 1.89.0 runtime stage0 test is performed.

Every expected compiler/tool failure must return the declared nonzero status;
absent diagnostics, unexpected success, output overflow or timeout fails the
assessment. The retained successful applications must still run after negative
probes. The second independent boot executes those persisted applications and
requires all workspace bytes to match. After both clean shutdowns the host
parses the guest-built ELF files, verifies executable modes and imported input
bytes, and requires read-only `e2fsck -f -n` success.

An intentional Rust compile error also returns nonzero while preserving the
previous application. The Rust seed exercises std vectors, a worker thread and
persistent file bytes; the C++17 seed exercises a fold expression and
`std::optional`. Neither fixture includes compiler component source code.

The host audits/selects upstream sources, builds the first-party diagnostic,
packages inherited seeds, creates the disk, drives QEMU and inspects outputs.
The guest compiles only the declared first-party test applications. No host
compiler output substitutes for those guest-built applications, and those
applications do not count as reconstructed compiler components.

## Capacity and evidence

The audit records file counts, logical bytes, namespace nodes, 4 KiB-rounded
regular-file data and files exceeding the 1 MiB writable-file limit for the full
distribution and each component. The full release includes tests, documentation,
unrelated targets and vendored dependencies. These counts are not a minimal
build closure or measurements of a real LLVM/rustc/Cargo build. Any pruned
selection needs its own declared dependency closure and generated-output budget.

The authenticated distribution currently has this inventory. Component rows
partition the archive; the remaining Rust row includes compiler/std/bootstrap,
tests, documentation and other tools rather than only rustc's build closure.

| Archive section | Regular files | Logical bytes | Files above 1 MiB |
| --- | ---: | ---: | ---: |
| LLVM project | 72,514 | 830,210,925 | 48 |
| Cargo source | 2,665 | 16,316,850 | 1 |
| Vendored dependencies | 92,988 | 1,736,948,141 | 118 |
| Remaining Rust and other files | 74,244 | 760,681,217 | 75 |
| Full release | 242,411 | 3,344,157,133 | 242 |

The whole namespace needs at least 279,266 nodes. Regular-file data alone,
rounded to 4 KiB blocks, needs at least 3,994,664,960 bytes before filesystem
metadata or build outputs. The archive's 142 in-root symbolic links are
inventoried and validated without extracting or following them. Source
selection publishes regular files only. Cargo's lockfile names 537 packages,
including 512 registry packages; these totals also include dependencies outside
a particular target's selected runtime/build closure.

Current independent constraints include the 16 MiB/512-inode work disk, 2,048
VFS nodes, 1 MiB writable files, 16 MiB kernel heap and 32 task slots. Additional
QEMU RAM does not remove them. A full source build's temporary storage, memory,
descriptor/task demand and elapsed time remain unmeasured. Scheduler-boundary
resource observations are not exact RSS, and post-shutdown disk allocation is
not transient storage demand.

Only complete success publishes `build/logs/bootstrap-rust.result.json`, with
the source audit and preparation identity embedded. Preparation records source
branch/commit/worktree, implementation hashes, imported bytes, inherited seed
inventories, host diagnostic identity, ISO/initramfs identity and machine
configuration in `build/bootstrap-rust/prepared.json`. Per-boot logs use the
`bootstrap-rust-` prefix. Keep these generated artifacts together for review.

Before reconstructing a component, choose its source closure, install and test
its actual build tools and native dependencies, provide the selected stage0
where required, and expand capacity with overflow/rollback/old-image tests.
Start with a sequential measured build and verify the resulting tool's actual
function. Full Rust bootstrap, new LLVM/Cargo tools, native AnovaOS std and complete
guest-only image generation remain independent unverified capabilities. M15.5
separately accepts guest kernel construction using the existing verified rustc
and inherited target libraries, with host-assisted image packaging.

## Capacity profile after M15.5

This assessment intentionally retains its accepted 1 MiB file-capacity profile
and source audit for comparison with its original validation record.
[M15.5 kernel reconstruction](M15_KERNEL.md) raises current persistent ext2
files to 4 MiB; tmpfs and readonly shared-file caches remain at 1 MiB. The
unpruned compiler source trees also exceed unchanged disk/inode/VFS bounds,
and their missing build prerequisites remain independent. The earlier audit
is historical capacity evidence, not a claim that the current persistent limit
is still 1 MiB or a new assessment of a pruned build closure.
