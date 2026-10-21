# Development and validation

M0/M1 provide the Cargo workspace, host xtask, bare-metal kernel and UEFI ISO.
The M2 BSP baseline adds CPU tables, guarded exception stacks, ACPI discovery
and a calibrated periodic LAPIC IRQ. M3 adds checked frames, a fully owned table
hierarchy, a guarded normal stack and a reclaiming `alloc` heap. Start from dev
or its task branch; read [STATUS.md](STATUS.md), [M3_MEMORY.md](M3_MEMORY.md)
and [AGENTS.md](../AGENTS.md). M4 adds isolated native Rust/C applications; read
[M4_USERSPACE.md](M4_USERSPACE.md) for the ABI/SDK and lifetime boundaries.
M5 adds saved CPU/FPU contexts, user preemption, blocking and process lifecycle;
read [M5_PROCESSES.md](M5_PROCESSES.md). External device IRQ delivery remains pending.

## Host setup

The tested host is x86_64 Ubuntu 24.04.3. Ubuntu 24.04 under WSL2 can use TCG;
WSL2 itself was not exercised in this validation environment. Install rustup
from its official distribution, then install host utilities:

```sh
sudo apt-get update
sudo apt-get install --no-install-recommends qemu-system-x86 xorriso curl dpkg-dev gcc
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
| `cargo xtask apps` | build/apps/: Rust/C ELF files and explicit ABI/SHA-256 manifest | Compiler/linker/ELF failure returns 1 |
| `cargo xtask check-apps` | Rust user workspace Clippy with actual user target/linker flags | Warnings are errors |
| `cargo xtask check-files` | Header and per-file inventory coverage required by DEV-001 | Missing explanation/inventory entry returns 1 |
| `cargo xtask image` | build/images/anova-normal.iso | Missing/corrupt inputs stop image generation |
| `cargo xtask run` | UEFI boot, native Rust/C apps on serial, then idle | Process errors propagate; stop QEMU with Ctrl+C |
| `cargo xtask smoke` | Memory/CPU and 11 isolation cases, then CPU/FPU/process lifecycle/OOM/capacity suite | Missing isolation/reclamation evidence, fatal kernel fault, wrong exit or deadline returns 1 |
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
| `cargo xtask test-host` | Frame/buddy/table boundaries and rollback; transcript/ACPI/PM/ELF/CLI tests | Test failures propagate |

smoke accepts --case boot/panic/timeout/invalid-opcode/general-protection/page-fault/
double-fault/stack-guard/read-only/no-execute/direct-no-execute/unmapped/oom
and --timeout-secs 1..300; its default is 30 seconds. verify uses
30 seconds per case, except 8 seconds for the deliberately hung image. Deadlines bound the QEMU process;
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
uses q35/TCG, qemu64, 256 MiB, one vCPU, no network or data disk. CODE is read-only;
VARS is copied into a unique build/tmp/ directory and removed on normal process
completion. The watchdog kills and reaps its guest. This does not test disk
persistence.

## Validation

```sh
cargo fmt --all -- --check
cargo fmt --manifest-path user/Cargo.toml --all -- --check
cargo xtask check-files
cargo clippy --locked -p xtask -p anova-platform -p anova-memory -p anova-abi -p anova-elf -p anova-sched --all-targets -- -D warnings
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
```

Do not use cargo test --workspace for the hardware kernel or enable multiple fault
injection features together. The kernel has no host test harness.

Success requires ordered boot/memory/CPU/timer markers, 8+ IRQs with consistent
nanoseconds, both returning breakpoint IF states, TEST:PASS and QEMU 33.
isa-debug-exit maps guest values as (value << 1) | 1: success 0x10 becomes 33,
panic 0x11 becomes 35, and exception 0x12 becomes 37. Exit 0 is rejected.
Only test features access this device; normal startup stays interruptible in
sti/hlt. Panic reports phase, source location and message through independent
bounded output. Exceptions report registers, CR2, errors and actual IST.
The gate treats exceptions as failures and separately validates each expected
injection. See [M2_CPU.md](M2_CPU.md) for IRQ, mapping and time limitations.

Complete firmware/bootloader transcripts remain in *.serial.log, process
messages in *.qemu.log, and case/exit/timeout/classification in *.result.txt.
A banner alone never passes.

## CI and evidence

[boot.yml](../.github/workflows/boot.yml) runs the above checks on Ubuntu 24.04,
including all 13 QEMU cases, and uploads logs even on failure. Actions are
pinned to commits. Configured CI and remote CI execution are separate facts.
Current local evidence is in [validation/M5.md](validation/M5.md);
[validation/M4.md](validation/M4.md),
[validation/M3.md](validation/M3.md),
[validation/M2.md](validation/M2.md) and
[validation/M0-M1.md](validation/M0-M1.md) record the historical gates.

Record source baseline, host/tool versions, input hashes, commands, results and
artifact hashes for boot changes. Debug paths and ISO timestamps still affect
artifact bytes. Synchronize both READMEs and current status.
