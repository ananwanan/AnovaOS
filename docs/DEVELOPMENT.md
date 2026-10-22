<!-- Describe reproducible host setup, implemented build commands and actual validation workflows. -->
# Development and validation

M0/M1 provide the Cargo workspace, host xtask, bare-metal kernel and UEFI ISO.
The M2 BSP baseline adds CPU tables, guarded exception stacks, ACPI discovery
and a calibrated periodic LAPIC IRQ. M3 adds checked frames, a fully owned table
hierarchy, a guarded normal stack and a reclaiming `alloc` heap. Start from dev
or its task branch; read [STATUS.md](STATUS.md), [M3_MEMORY.md](M3_MEMORY.md)
and [AGENTS.md](../AGENTS.md). M4 adds isolated native Rust/C applications; read
[M4_USERSPACE.md](M4_USERSPACE.md) for the ABI/SDK and lifetime boundaries.
M5 adds saved CPU/FPU contexts, user preemption, blocking and process lifecycle;
read [M5_PROCESSES.md](M5_PROCESSES.md). M6 adds VFS, init, an interactive shell,
blocking pipes and polled canonical serial input; read [M6_FILESYSTEM.md](M6_FILESYSTEM.md).
M7 adds PCI/modern virtio-blk, a supported ext2 data image, a serial editor and
verified shutdown/reboot persistence; read [M7_STORAGE.md](M7_STORAGE.md).
External device IRQ delivery remains pending; block completion is polled.

## Host setup

The tested host is x86_64 Ubuntu 24.04.3. Ubuntu 24.04 under WSL2 can use TCG;
WSL2 itself was not exercised in this validation environment. Install rustup
from its official distribution, then install host utilities:

```sh
sudo apt-get update
sudo apt-get install --no-install-recommends qemu-system-x86 xorriso curl dpkg-dev gcc make musl-tools qemu-user e2fsprogs openssl python3 binutils
rustup show active-toolchain
cargo xtask fetch
cargo xtask doctor
```

Rustup loads rust-toolchain.toml: stable Rust 1.90.0, rustfmt, Clippy and the
precompiled x86_64-unknown-none target. QEMU 8.2.2 is checked; Ubuntu package
suffixes may differ. dpkg-deb comes from dpkg (also installed with dpkg-dev).
fetch extracts a fixed OVMF package locally, so system OVMF is unnecessary.
No xtask command uses sudo or runs package installation scripts.

Cargo.lock pins registry packages and checksums. boot/dependencies.toml pins
Limine 9.5.0 binary commit, protocol reference, OVMF package and boot-file hashes.
Downloads use HTTPS and are verified before extraction/use. A corrupt cached
file causes an error; inspect and remove that named cache file before retrying.

Firmware defaults to build/firmware/usr/share/OVMF/. ANOVA_OVMF_CODE and
ANOVA_OVMF_VARS can select existing regular files; their contents must still
match the locked CODE/VARS hashes. Explicit paths do not disable verification.

## Implemented commands

Run from the repository root:

| Command | Output/behavior | Failure behavior |
| --- | --- | --- |
| `cargo xtask fetch` | Verified build/cache/ and build/firmware/ | Network, extraction or checksum error returns 1 |
| `cargo xtask doctor` | Checks tools, versions, target and boot inputs | Reports all discovered problems; returns 1 |
| `cargo xtask build` | target/x86_64-unknown-none/debug/anova-kernel and ELF checks | Build/link/ELF failure returns 1 |
| `cargo xtask apps` | build/apps/: eight Rust/C ELFs, checked initramfs and ABI/SHA-256 manifest | Compiler/linker/ELF failure returns 1 |
| `cargo xtask check-apps` | Rust user workspace Clippy with actual user target/linker flags | Warnings are errors |
| `cargo xtask check-files` | Header and per-file inventory coverage required by DEV-001 | Missing explanation/inventory entry returns 1 |
| `cargo xtask image` | build/images/anova-normal.iso | Missing/corrupt inputs stop image generation |
| `cargo xtask run` | UEFI boot workloads, native PID 1 and interactive serial shell | Process errors propagate; stop QEMU with Ctrl+C |
| `cargo xtask smoke` | Memory/CPU and 11 isolation cases, then CPU/FPU/process lifecycle plus VFS/pipe/shell/tmpfs-OOM suite | Missing isolation/reclamation evidence, fatal kernel fault, wrong exit or deadline returns 1 |
| `cargo xtask smoke --case panic` | Controlled panic and QEMU 35 | Intentionally returns 1 |
| `cargo xtask smoke --case timeout --timeout-secs 8` | Controlled hang, killed by watchdog | Intentionally returns 1 |
| `cargo xtask smoke --case page-fault` | Real guard write faults on IST2, QEMU 37 | Intentionally returns 1 |
| `cargo xtask smoke --case double-fault` | Real stack failure reaches #DF on IST1, QEMU 37 | Intentionally returns 1 |
| `cargo xtask smoke --case read-only` | Real supervisor write to RO RAM, QEMU 37 | Intentionally returns 1 |
| `cargo xtask smoke --case no-execute` | Real instruction fetch from NX RAM, QEMU 37 | Intentionally returns 1 |
| `cargo xtask smoke --case direct-no-execute` | Real NX fetch through the HHDM alias, QEMU 37 | Intentionally returns 1 |
| `cargo xtask smoke --case stack-guard` | Write to the owned normal-stack guard, QEMU 37 | Intentionally returns 1 |
| `cargo xtask smoke --case unmapped` | Access after unmap and TLB flush, QEMU 37 | Intentionally returns 1 |
| `cargo xtask smoke --case oom` | Infallible allocation panic, QEMU 35 | Intentionally returns 1 |
| `cargo xtask verify` | Boot passes; twelve reached failure injections are rejected | Wrong vector/error/IST/address, unexpected outcome or unreached injection fails |
| `cargo xtask terminal-test` | Two normal boots: UART editing/CRLF/EOF, dynamic musl, Ctrl+C/EINTR, shell restart and volatile file loss | Missing actual output, guest failure or 30-second/session watchdog returns 1 |
| `cargo xtask root-image build/disks/NAME.ext2` | New 16 MiB native-v1 ext2 data image | Existing files and unsafe paths are refused |
| `cargo xtask run --disk build/disks/NAME.ext2` | Reuse an explicitly selected image; guest shutdown cleanly powers off | Does not format missing/existing images |
| `cargo xtask persistence-test` | Editor/source/binary checks across shutdown/reboot, e2fsck and device error scenarios | Missing bytes/hash/mount/error/actual exit or 60-second/session watchdog fails |
| `cargo xtask test-host` | Frame/buddy/table, VFS/pipe/FD, scheduler, archive/parser and transcript tests | Test failures propagate |

smoke accepts --case boot/panic/timeout/invalid-opcode/general-protection/page-fault/
double-fault/stack-guard/read-only/no-execute/direct-no-execute/unmapped/oom
and --timeout-secs 1..300; its default is 30 seconds. verify uses
30 seconds per case, including the deliberately hung image. Larger M9 runtime
images must reach their actual injection point before the watchdog classification. Deadlines bound the QEMU process;
compilation and downloads have their own failure behavior.

## Target and image boundaries

The alias runs host xtask. No global build.target is configured; default-members
excludes the kernel from accidental host builds. Kernel build.rs rejects the
wrong target and selects its own linker script. The bare-metal target defaults
to soft-float/no SIMD, kernel code model and no red zone; no host libc, system
allocator or std is linked. M3 supplies its own global buddy allocator. M4's
separate user workspace uses freestanding x86_64-unknown-linux-gnu core/SSE2,
its own startup/linker and no libc/Linux syscalls; kernel flags do not apply.

The linker retains writable Limine requests between delimiters and separates
RX/R/RW segments at 0xffffffff80000000. build checks ELF class/endian/machine,
ET_EXEC, program header bounds, segment ranges/permissions and executable entry;
PT_INTERP/PT_DYNAMIC and RWX are rejected.

The image is a UEFI-only El Torito ISO. There is no BIOS installation or physical
block-device access. Fixed output paths reject symlinks and device files. QEMU
uses q35/TCG, qemu64, 256 MiB, one vCPU, no network and a modern virtio data disk. CODE is read-only;
VARS is copied into a unique build/tmp/ directory and removed on normal process
completion. Default runs create a disposable ext2 data image in that same
temporary directory; explicit images and persistence-test use build/disks/.
The watchdog kills and reaps its guest. See M7_STORAGE for clean shutdown and
non-journaled ext2 recovery limits.

## Validation

```sh
cargo fmt --all -- --check
cargo fmt --manifest-path user/Cargo.toml --all -- --check
cargo xtask check-files
cargo clippy --locked -p xtask -p anova-platform -p anova-memory -p anova-abi -p anova-elf -p anova-sched -p anova-vfs -p anova-storage --all-targets -- -D warnings
cargo xtask check-apps
cargo xtask apps
cargo clippy --locked -p anova-kernel --target x86_64-unknown-none --features qemu-test -- -D warnings
cargo clippy --locked -p anova-kernel --target x86_64-unknown-none --features test-double-fault -- -D warnings
cargo clippy --locked -p anova-kernel --target x86_64-unknown-none --features test-no-execute -- -D warnings
cargo xtask test-host
cargo xtask build
cargo xtask fetch
cargo xtask doctor
cargo xtask verify
cargo xtask terminal-test
cargo xtask persistence-test
```

Do not use cargo test --workspace for the hardware kernel or enable multiple fault
injection features together. The kernel has no host test harness.

Success requires ordered boot/memory/CPU/timer markers, 8+ IRQs with consistent
nanoseconds, both returning breakpoint IF states, TEST:PASS and QEMU 33.
isa-debug-exit maps guest values as (value << 1) | 1: success 0x10 becomes 33,
panic 0x11 becomes 35, and exception 0x12 becomes 37. Exit 0 is rejected.
Only test features access this device; normal startup stays in the interactive
shell, with blocked input yielding to interruptible scheduler STI/HLT. Panic reports phase, source location and message through independent
bounded output. Exceptions report registers, CR2, errors and actual IST.
The gate treats exceptions as failures and separately validates each expected
injection. See [M2_CPU.md](M2_CPU.md) for IRQ, mapping and time limitations.

Complete firmware/bootloader transcripts remain in *.serial.log, process
messages in *.qemu.log, and case/exit/timeout/classification in *.result.txt.
A banner alone never passes.

## CI and evidence

[boot.yml](../.github/workflows/boot.yml) runs the above checks on Ubuntu 24.04,
including all 13 QEMU cases, the two-boot terminal scenario and persistence/error scenarios, and uploads logs even on failure. Actions are
pinned to commits. Configured CI and remote CI execution are separate facts.
Current local evidence is in [validation/M7.md](validation/M7.md);
[validation/M6.md](validation/M6.md),
[validation/M5.md](validation/M5.md),
[validation/M4.md](validation/M4.md),
[validation/M3.md](validation/M3.md),
[validation/M2.md](validation/M2.md) and
[validation/M0-M1.md](validation/M0-M1.md) record the historical gates.

Record source baseline, host/tool versions, input hashes, commands, results and
artifact hashes for boot changes. Debug paths and ISO timestamps still affect
artifact bytes. Synchronize both READMEs and current status.

## M8 static Linux tools

Read [M8_LINUX.md](M8_LINUX.md) before extending the compatibility boundary.
`cargo xtask apps` now also requires musl-gcc and GNU make and builds a pinned
BusyBox source revision. Initial source acquisition requires Git/network access;
build logs/config/source bundle stay in build/. `cargo xtask trace-compat` uses
qemu-x86_64 -strace on Linux and records actual host calls. `cargo xtask test-compat`
runs the full boot regression with required Linux evidence. The ordinary UART
test also launches musl and ash. Other applets and full signal/terminal behavior
remain unverified.

## M9 runtime verification

[M9_RUNTIME.md](M9_RUNTIME.md) defines the pinned musl 1.2.4 runtime hash,
ANOVA_MUSL_LIBC override, resource bounds and supported flags. apps now builds
static/PIE/dynamic thread/signal/TTY probes, a deliberately missing-interpreter
ELF, two TLS shared objects and the verified actual interpreter. Initramfs packs
these under /bin and /lib. File purposes cover every new source and fixture.

trace-compat records all three M9 profiles with QEMU-user on Linux, alongside
M8 traces. Same-architecture exec children can leave QEMU-user; the transcript
states this boundary. M9_HOST identifies Linux's permissive W+X policy in this
comparison. QEMU-user 8.2.2 aborts on the deliberately malformed sigreturn case;
only that host-emulator case is marked unavailable. Native Linux and system-QEMU
exercise the case independently; the guest gate always requires its signal result.

verify/test-compat now require three independent runtime successes and exact
frame/heap/task/descriptor cleanup, real heap-exhaustion rollback, missing-library
and missing-interpreter errors. terminal-test launches dynamic musl over UART and
sends an actual Ctrl+C byte to an interruptible read. Persistence tests retain
the original controlled save/shutdown/reboot and real disk error requirements.
See [validation/M9.md](validation/M9.md) for measured results and hashes.

Multi-image commands build user space once, retain its SHA-256 snapshot and
rehash all generated app/runtime/archive outputs before every image reuse.
Changed/missing outputs fail the command. Separate invocations rebuild normally;
this avoids repeated BusyBox builds without accepting stale user artifacts.

## M10 controlled peers and package verification

OpenSSL, Python 3 (standard library only) and readelf are required in addition to
the existing host tools. `cargo xtask apps` fetches the exact Mbed TLS 2.28.10
commit and builds guest clients; Cargo.lock pins smoltcp. Static/shared musl linker
inputs must both be installed. Missing shared libc now fails explicitly before
packaging incorrect TLS DSOs. See [M9 runtime](M9_RUNTIME.md).

```sh
cargo xtask network-test
cargo xtask package-test
cargo xtask run --network --disk build/disks/work.ext2
```

network-test binds loopback ports 18080–18082. package-test binds 18083 and 18085
for standard-library Python HTTPS peers, with 18084/18086 reserved as negative
unbound peers. A collision fails clearly; no host network settings are changed.
Guest transcript watchdogs, syscall results, actual X.509 error flags and clean
e2fsck checks determine acceptance. Logs use network.* and packages-*.serial.log
under build/logs. Test CA private keys remain only in ignored build/compat.
Read [M10_NETWORK_PACKAGES](M10_NETWORK_PACKAGES.md) before using online features.

## Offline guest C/C++ development (M11)

`cargo xtask toolchain-build` constructs locked GCC/G++, binutils, make and the
matching musl/CRT/C++ sysroot from source. It needs Python 3.12+, GCC/G++, make,
m4, bison, flex and texinfo in addition to the base prerequisites.
`ANOVA_BUILD_JOBS=1..8` controls build parallelism (default 6).
`cargo xtask toolchain-image` packs the separately verified development ISO;
`cargo xtask toolchain-test` runs the real UART edit/build/run/reboot acceptance.

```sh
cargo xtask root-image --linux build/disks/devel.ext2
cargo xtask run --toolchain --disk build/disks/devel.ext2
```

Use a clean guest shutdown to retain sources and binaries. Development runs
use 1 GiB RAM, one CPU, a read-only `/tc` and a Linux-personality data disk;
plain `run` keeps the smaller base image. See [M11_TOOLCHAIN](M11_TOOLCHAIN.md)
for editor commands, tested features, input hashes and unsupported operations.


## Offline guest Rust/Cargo development (M12)

`cargo xtask rust-build` verifies M11 construction and prepares the checksum-locked
upstream musl Rust 1.90.0 binary distributions, matching std/rust-lld and itoa
vendor. `cargo xtask rust-image` independently verifies and packs
`build/images/anova-rust.iso`; `cargo xtask run --rust` selects it with 3 GiB RAM.
`--rust` and `--toolchain` are mutually exclusive. Use an explicit Linux-profile
data image for retained sources and guest `shutdown` for a clean filesystem:

```sh
cargo xtask root-image --linux build/disks/rust-devel.ext2
cargo xtask run --rust --disk build/disks/rust-devel.ext2
cargo xtask rust-cross-test
cargo xtask rust-runtime-test
cargo xtask rust-test
```

The three gates separate preliminary host cross compilation, actual compiler
startup and UART edit/guest compile/run/clean reboot. The full gate checks
std, a procedural macro shared object, frozen offline Cargo/vendor, build.rs,
C FFI, freshness/source edits, a real parser failure and persisted source/ELF
hashes with e2fsck. Keep all M9/M10/M11 gates. See [M12_RUST](M12_RUST.md) and
[validation/M12](validation/M12.md) for provenance, exact bounds and exclusions.

## Opt-in online Cargo development (M12)

The ordinary `cargo` launcher keeps its offline default. `cargo-online` uses the
same official Cargo binary with online transport and a persistent Cargo home.
Start the development image with an explicit Linux disk and virtio-net:

```sh
cargo xtask rust-image
cargo xtask root-image --linux build/disks/rust-online.ext2
cargo xtask run --rust --network --disk build/disks/rust-online.ext2
```

In a guest project under `/persist`, use `cargo-online build` followed by
`cargo-online build --locked`. An independent boot without a NIC can use
`cargo-online build --offline --frozen --target-dir rebuilt` to recompile from
the saved cache. Guest `shutdown` is required for retained data.

`cargo xtask rust-online-test` serves a deterministic HTTPS sparse registry and
Git smart-HTTP repository, tests strict certificate checks and transfer failures,
and checks guest compilation, filesystem integrity and a fresh offline rebuild.
It requires Python 3, OpenSSL, Git and the existing compiler/QEMU utilities.
`cargo xtask rust-public-test` separately downloads pinned crates.io/GitHub
sources without source replacement. Its optional `ANOVA_CARGO_TEST_PROXY` and
`ANOVA_CARGO_TEST_CA` inputs describe the test host's reachable HTTP proxy and
public PEM trust roots; credentials and private keys must never be packed.
See [M12_NETWORK](M12_NETWORK.md) for transport limits and configuration, and
[validation/M12](validation/M12.md) for actual acceptance status. CI retains the
deterministic online gate; the public gate depends on external availability.
