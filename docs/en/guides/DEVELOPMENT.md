<!-- Describe reproducible host setup, implemented commands, target boundaries and validation workflows. -->
# Development and validation

**English** | [简体中文](../../zh-CN/guides/DEVELOPMENT.md)

M0/M1 provide the Cargo workspace, host xtask, bare-metal kernel and UEFI ISO.
The M2 BSP baseline adds CPU tables, guarded exception stacks, ACPI discovery
and a calibrated periodic LAPIC IRQ. M3 adds checked frames, a fully owned table
hierarchy, a guarded normal stack and a reclaiming `alloc` heap. Start from dev
or its task branch; read [STATUS.md](STATUS.md), [M3_MEMORY.md](../milestones/M3_MEMORY.md)
and [AGENTS.md](../../../AGENTS.md). M4 adds isolated native Rust/C applications; read
[M4_USERSPACE.md](../milestones/M4_USERSPACE.md) for the ABI/SDK and lifetime boundaries.
M5 adds saved CPU/FPU contexts, user preemption, blocking and process lifecycle;
read [M5_PROCESSES.md](../milestones/M5_PROCESSES.md). M6 adds VFS, init, an interactive shell,
blocking pipes and polled canonical serial input; read [M6_FILESYSTEM.md](../milestones/M6_FILESYSTEM.md).
M7 adds PCI/modern virtio-blk, a supported ext2 data image, a serial editor and
verified shutdown/reboot persistence; read [M7_STORAGE.md](../milestones/M7_STORAGE.md).
External device IRQ delivery remains pending; block completion is polled.

## Host setup

The tested host is x86_64 Ubuntu 24.04.3. Ubuntu 24.04 under WSL2 can use TCG;
M13 additionally validates Ubuntu 24.04 under WSL2 with QEMU 8.2.2 TCG;
see [M13 evidence](../validation/M13.md). Install rustup
from its official distribution, then install host utilities:

```sh
sudo apt-get update
sudo apt-get install --no-install-recommends qemu-system-x86 xorriso curl dpkg-dev gcc g++ binutils make musl-tools qemu-user e2fsprogs openssl python3 binutils
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
python3 scripts/check_docs.py
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
injection. See [M2_CPU.md](../milestones/M2_CPU.md) for IRQ, mapping and time limitations.

Complete firmware/bootloader transcripts remain in *.serial.log, process
messages in *.qemu.log, and case/exit/timeout/classification in *.result.txt.
A banner alone never passes.

## CI and evidence

[boot.yml](../../../.github/workflows/boot.yml) runs the above checks on Ubuntu 24.04,
including all 13 QEMU cases, the two-boot terminal scenario and persistence/error scenarios, and uploads logs even on failure. Actions are
pinned to commits. Configured CI and remote CI execution are separate facts.
Current local evidence through M15.6 is listed in the
[documentation index](../index.md#validation).
[validation/M7.md](../validation/M7.md),
[validation/M6.md](../validation/M6.md),
[validation/M5.md](../validation/M5.md),
[validation/M4.md](../validation/M4.md),
[validation/M3.md](../validation/M3.md),
[validation/M2.md](../validation/M2.md) and
[validation/M0-M1.md](../validation/M0-M1.md) record the historical gates.

Record source baseline, host/tool versions, input hashes, commands, results and
artifact hashes for boot changes. Debug paths and ISO timestamps still affect
artifact bytes. Synchronize both READMEs and current status.

## M8 static Linux tools

Read [M8_LINUX.md](../milestones/M8_LINUX.md) before extending the compatibility boundary.
`cargo xtask apps` now also requires musl-gcc and GNU make and builds a pinned
BusyBox source revision. Initial source acquisition requires Git/network access;
build logs/config/source bundle stay in build/. `cargo xtask trace-compat` uses
qemu-x86_64 -strace on Linux and records actual host calls. `cargo xtask test-compat`
runs the full boot regression with required Linux evidence. The ordinary UART
test also launches musl and ash. Other BusyBox applets remain unverified; the separately accepted M9 runtime
adds the bounded signal/terminal behavior described below.

## M9 runtime verification

[M9_RUNTIME.md](../milestones/M9_RUNTIME.md) defines the pinned musl 1.2.4 runtime hash,
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
See [validation/M9.md](../validation/M9.md) for measured results and hashes.

Multi-image commands build user space once, retain its SHA-256 snapshot and
rehash all generated app/runtime/archive outputs before every image reuse.
Changed/missing outputs fail the command. Separate invocations rebuild normally;
this avoids repeated BusyBox builds without accepting stale user artifacts.

## M10 controlled peers and package verification

OpenSSL, Python 3 (standard library only) and readelf are required in addition to
the existing host tools. `cargo xtask apps` fetches the exact Mbed TLS 2.28.10
commit and builds guest clients; Cargo.lock pins smoltcp. Static/shared musl linker
inputs must both be installed. Missing shared libc now fails explicitly before
packaging incorrect TLS DSOs. See [M9 runtime](../milestones/M9_RUNTIME.md).

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
Read [M10_NETWORK_PACKAGES](../milestones/M10_NETWORK_PACKAGES.md) before using online features.

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
plain `run` keeps the smaller base image. See [M11_TOOLCHAIN](../milestones/M11_TOOLCHAIN.md)
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
hashes with e2fsck. Keep all M9/M10/M11 gates. See [M12_RUST](../milestones/M12_RUST.md) and
[validation/M12](../validation/M12.md) for provenance, exact bounds and exclusions.

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
See [M12_NETWORK](../milestones/M12_NETWORK.md) for transport limits and configuration, and
[validation/M12](../validation/M12.md) for actual acceptance status. CI retains the
deterministic online gate; the public gate depends on external availability.

## M13 application workflow and diagnostics

Follow [M13_SDK](../milestones/M13_SDK.md) for runnable create/build/pack/boot/install/upgrade
steps, separate native/Linux disk policy, stable errors and crash symbols.
Commands `sdk`, `app-new`, `app-build`, `app-pack`, `app-image`, `app-run`,
`symbolize`, `sdk-test`, `app-test`, `package-v2-test` and `diagnostics-test`
are implemented. GCC/G++ and binutils (`readelf`, `strip`, `addr2line`) are
required for the three native templates and host symbolization. M13 source
SDK templates retain debug symbols; compact package payloads have separate
hashes. Kernel resource logs describe global counters, not process accounting.
Preserve all M0–M12 gates alongside the four new SDK/workflow/package/diagnostic
gates. Remote CI results remain separate from local evidence.

## M14.1–M14.2 software display and input

```sh
cargo xtask graphics-image
cargo xtask graphics-test
cargo xtask input-test
cargo xtask run --graphics
```

The optional `graphics` kernel feature draws a CPU test pattern; ordinary serial
and compiler images retain their existing profile. `graphics-image` produces
`build/images/anova-graphics.iso`. Interactive `run --graphics` needs a working
GTK display and retains the COM1 shell on standard input/output. Graphics are
currently limited to the normal recovery image, independently of compiler images.

`graphics-test` runs headlessly with QEMU standard VGA, captures an actual QMP
screendump and compares every visible pixel with the CPU reference. A separate
boot without VGA must retain serial recovery. The shell remains the text
diagnostic fallback; graphical text/runtime and terminal support belong to the
separately accepted M14.4 profile, not this M14.1 test image.
See [M14 GUI](../milestones/M14_GUI.md) for format, memory, cache and stage boundaries and
[M14.1 validation](../validation/M14.1.md) for actual local results.

The same opt-in graphics profile also initializes the QEMU i8042 keyboard in
native scan-code set 1 and its standard ID-0 three-button mouse. Input delivery
is temporary scheduler polling, bounded to 64 bytes and a 64-event queue per
pass. External device IRQs remain masked. Physical input goes to one diagnostic
consumer and never becomes shell text; keep COM1 available for all commands.
The cursor saves/restores a fixed 7×9 footprint and clips at all four edges;
input/cursor storage allocates no heap. It first appears on a pointer event,
leaving the original boot screenshot intact for `graphics-test`.

`input-test` builds `build/images/anova-input.iso` with `graphics,input-test`.
Its three 60-second-watchdog scenarios use standard VGA, `-vga none` and
`-machine i8042=off`. QMP injects ordered key/repeat/extended/modifier, pointer
and three-button sequences. The display scenario saves center/four-corner PPMs
and compares all 480,000 pixels, including restored background. The test image
also checks bounded queue overflow, malformed input, diagnostic consumer detach/
reconnect, independent PM-timer partial-sequence expiry and real controller-port
disable/reinitialization before readiness.
Every boot must retain COM1 command recovery and clean ACPI shutdown.

Artifacts are under `build/input/`, including per-scenario serial,
QMP/QEMU logs, result files, PPM images and SHA-256 evidence. The local
three-boot gate passed; see [M14.2 validation](../validation/M14.2.md).
CI retains `build/input/`; remote execution remains separate from local evidence.
These cases establish neither physical hotplug nor USB, external IRQ, real
hardware or user-space GUI routing. Preserve `graphics-test`, `input-test`,
their kernel feature checks and all earlier gates.

## M14.3 native window composition

```sh
cargo xtask gui-image
cargo xtask gui-test
cargo xtask run --gui
```

`gui-image` builds `build/images/anova-gui.iso` with `graphics,gui`.
The COM1 shell remains the entry point: type `gui-test` to spawn the independent
user compositor and two native clients. Click for focus, title-drag for movement,
and bottom-right drag for resize. The supervised demonstration uses X in the
red window to fault that client, F12 to fault/restart the compositor and F11
in the restarted session to close it and return to the shell. Interactive GTK
availability is separate from the accepted headless gate.

`gui-test` builds `build/images/anova-gui-test.iso` with
`graphics,gui,gui-test`. Three boots cover standard VGA/i8042, no VGA, and
no i8042. The kernel test profile exhausts the actual heap and checks connection
rollback/reuse; ordinary interactive images omit that pressure injection.
Actual QMP input drives window focus, movement, resize and both isolated user
faults. Eight screenshots compare all pixels against a separate host reference;
every boot requires COM1 recovery and clean ACPI shutdown. Results, hashes and
serial/QMP/scanout artifacts stay in `build/gui/`. The CI gate retains that
folder; local results do not claim a remote CI run.

Preserve all previous gates alongside `gui-test`. The internal syscall transport
is separate from native v1 and the M14.4 GUI C ABI. Each client owns copied
private pixels, one bounded window and a bounded mailbox; no device mappings
are exported. See [M14_COMPOSITOR](../milestones/M14_COMPOSITOR.md) for exact ownership,
backpressure and restart semantics and [M14.3 validation](../validation/M14.3.md)
for actual verification and exclusions. M14.4 GUI runtime/SDKs and the bounded M14.5 GPU
profile have their own local acceptance, described below.

## M14.4 GUI runtime and independent language SDKs

The GUI source SDK has its own `anova.gui.v1` C library ABI. The Rust runtime
implements text, layout, buttons, text fields, event polling and private
clipboard storage. Generated C declarations, move-only C++ RAII and safe Rust
wrappers share those semantics. The compositor and kernel isolation from
M14.3 remain in use; no Linux GUI framework or native `std` port is added.
See [runtime contract](../milestones/M14_RUNTIME.md) and [GUI SDK](../../../sdk/gui/README.md).

```sh
cargo xtask gui-sdk
cargo xtask gui-sdk-image
cargo xtask gui-sdk-test
cargo xtask gui-sdk-replay # Recheck retained outputs after a successful build gate
cargo xtask root-image build/disks/gui-dev.ext2
cargo xtask run --gui-sdk --disk build/disks/gui-dev.ext2
```

`gui-sdk` exports a checksummed source distribution to
`build/sdk/anova-gui-sdk-1.0.0`. Copy it before independently building its
`crates/gui-runtime/Cargo.toml`, so generated Cargo outputs do not enter the
immutable release. The GUI image requires the prepared M11/M12 compiler
profiles and uses 3 GiB guest memory, one BSP, standard VGA and polled PS/2.
The interactive command opens QEMU GTK; headless acceptance uses QMP.
Use a native data disk, whose immutable marker is `anova.native.v1`.
Compiler executables remain explicitly Linux-personality initramfs entries.

Inside that GUI development image:

```sh
cd /persist
cp /gui/templates/main.c main.c
gui-build c
gui-run /persist/gui-c
```

Type physical `a`, click the button, then type `q` to close the demo. Press F11
to stop its compositor and return to the serial shell. For C++ or Rust, copy
`main.cpp` or `main.rs` and select `gui-build cpp` or `gui-build rust`; run the
corresponding `/persist/gui-cpp` or `/persist/gui-rust`. `gui-build note` and
`gui-build terminal` accept `note.rs` and `terminal.rs`. The Rust graphical
terminal executes `echo TEXT` and `pwd` through a pre-GUI helper and real pipes;
Enter submits the field and `q` in an empty field closes/reaps it. This bounded
terminal does not implement a PTY or ANSI escape sequences.

GCC/G++ compile independent freestanding objects; guest rustc compiles the
runtime rlib and links the objects with native startup/intrinsics/panic code.
The generated application ABI follows the native disk policy. No runtime libc,
C++ exceptions, dynamic Rust standard library or host-prebuilt GUI client is
used. A failed compilation retains the preceding runnable executable.

`gui-sdk-replay` rechecks the successful build gate's exact retained ISO, source
and native executable hashes, then runs another physical-input/scanout boot
without recompilation. It requires successful build evidence and records a
separate replay result. It cannot replace the original compilation gate.

The gate saves each source through the actual serial editor, compiles it in
guest, verifies physical input and literal scanout samples, checks a real client
page fault and both child wait statuses, then compares exact source/ELF hashes
across clean shutdown/reboot. Run image-building tests sequentially: their
verified `build/apps` snapshots are intentionally invalidated by concurrent
writers. [M14.4 validation](../validation/M14.4.md) records actual results and the
boundary between host checks, QEMU verification and untested physical hardware.

## M14.5 bounded hardware clear/readback

```sh
cargo xtask gpu-test
cargo xtask gpu-fallback-test
cargo clippy --locked -p anova-kernel --target x86_64-unknown-none --features gpu -- -D warnings
cargo clippy --locked -p anova-kernel --target x86_64-unknown-none --features gpu-test -- -D warnings
```

Both commands build `build/images/anova-gpu.iso` with `graphics,gui,gpu-test`.
The full gate requires a working GTK/OpenGL environment and a recognized actual
hardware renderer. It keeps standard VGA for recovery and attaches a secondary
`virtio-gpu-gl-pci`. The compositor is launched from the real COM1 shell to
render and compare offscreen pixels, inject a lost doorbell and demonstrate
CPU fallback and subsequent GPU reuse. `gpu-fallback-test` requires no GUI/GL
environment: it covers a 2D-only GPU and no GPU with the same user interface.

On this WSLg host the full gate selects Gallium `d3d12` and a Wayland GTK backend
for the QEMU child when the corresponding environment is available. Explicit
`GALLIUM_DRIVER`/`GDK_BACKEND` settings are preserved. `ANOVA_GPU_DISPLAY` may
select another QEMU GL display, for example `egl-headless` on a Linux host with
a suitable DRM render node. The full gate reads the exact device's capset
renderer and rejects software/unknown renderers; a successful external GL probe
or mere presence of `/dev/dxg` does not establish hardware acceptance.

The result directory `build/gpu/` contains per-boot results, exact commands,
source ELF/ISO hashes, host graphics package versions, serial/QMP logs and full
800×600 PPM comparisons. The full gate's successful output remains separate
from the headless fallback result. Run image-building gates sequentially.
Only headless fallback and kernel feature checks are added to ordinary CI;
the hardware gate must be run explicitly on a supported host. See
[M14 GPU](../milestones/M14_GPU.md) and [M14.5 validation](../validation/M14.5.md) for ownership,
latency measurements and the boundary between GPU drawing and CPU presentation.

## Controlled source reconstruction

After preparing the M11 compiler/sysroot, run `cargo xtask bootstrap-prepare`
to create a new source-only bzip2 1.0.8 workspace disk and instrumented ISO.
`cargo xtask bootstrap-test` creates its own fresh disk and verifies resource
limits, actual guest compilation and reboot/compiler-rejection recovery.
Keep the printed work disk and shut down cleanly. Inputs, seed identities,
source/output hashes and sampled resource observations are retained in
`build/bootstrap/prepared.json` and `build/logs/bootstrap.result.json`; complete
per-boot serial/QEMU evidence uses the `bootstrap-` log prefix. See
[M15 bootstrap](../milestones/M15_BOOTSTRAP.md) for prerequisites and measured-profile limits,
and [local acceptance](../validation/M15.1-M15.2.md) for exact executed scope.


## GCC reconstruction feasibility

After `cargo xtask toolchain-build`, run `cargo xtask bootstrap-gcc-test`.
The command authenticates seven cached source archives, prepares a separate
work disk and verifies the inherited C++11 seed, actual missing dependencies,
selected upstream configure startup rejection, limit recovery and reboot.
`python3 ports/bootstrap/test_gcc_probe.py` covers source/cache rejection;
`python3 ports/bootstrap/gcc_probe.py --verify` independently checks the prepared
source inventory without writes. The successful assessment reports current
blockers; it does not produce a rebuilt GCC. See [M15 GCC](../milestones/M15_GCC.md) and
[local validation](../validation/M15.3.md). The final report is
`build/logs/bootstrap-gcc.result.json`, with per-boot `bootstrap-gcc-` logs and
embedded source/seed/image identities.

## Rust, LLVM and Cargo reconstruction feasibility

After `cargo xtask toolchain-build` and `cargo xtask rust-build`, run
`cargo xtask bootstrap-rust-test`. The command audits the locked official Rust
source distribution and its separate LLVM/rustc/Cargo components, then uses two
q35/TCG boots to verify Rust std/C++17 seed applications, actual missing-tool
exec failures, an isolated offline dependency rejection and retained application
hashes after reboot. It does not reconstruct a compiler or Cargo.
`python3 ports/bootstrap/test_rust_probe.py` checks archive/cache rejection;
`python3 ports/bootstrap/rust_probe.py --verify` checks existing publication
without rewriting it. The final report is `build/logs/bootstrap-rust.result.json`,
with embedded `build/bootstrap-rust/prepared.json`, source audit, exact seed/image
identities and per-boot `bootstrap-rust-` logs. See [M15 Rust](../milestones/M15_RUST.md) and
[local validation](../validation/M15.4.md) for the independent component conclusions
and the difference between the inherited M12 seed and upstream's stage0.

## Guest kernel reconstruction

After preparing the locked M12 Rust runtime, run:

```sh
python3 ports/bootstrap/test_kernel_prepare.py
cargo xtask bootstrap-kernel-test
```

The gate prepares exact kernel sources, locked offline registry packages and
inherited bare-metal target libraries, then runs guest Cargo/rustc/rust-lld.
It retains the resulting ELF, packages it without rebuilding a host kernel,
extracts and compares the ISO's kernel bytes, then verifies actual boot,
compiler-failure retention and another recovery boot. The final report is
`build/logs/bootstrap-kernel.result.json`, with per-session `bootstrap-kernel-`
logs and inventories in `build/bootstrap-kernel/inputs.json` and `prepared.json`.
Run image-building gates sequentially. See [M15 kernel](../milestones/M15_KERNEL.md) for
source/seed provenance, 4 MiB persistent versus 1 MiB tmpfs/cache bounds,
measurement limits and the separate guest-only image-generation scope.

## Repeated clean reconstruction

`cargo xtask bootstrap-repeat-test` runs both bzip2 and kernel gates twice
sequentially on fresh work disks. It checks empty/source-only initial workspaces,
retains each round under a unique `build/bootstrap-repeat/` run directory, and
compares exact input/seed/recipe identities and full output bytes. Existing
functional, compiler-rejection, reboot and filesystem checks run in each round.
The successful aggregate is `build/logs/bootstrap-repeat.result.json`; mismatches
retain `comparison.json` and fail. Run image-building gates exclusively. See
[M15 repeat](../milestones/M15_REPEAT.md) and [validation/M15.6](../validation/M15.6.md) for
retained artifacts, metadata differences and bootstrap limitations.

## Documentation maintenance

Update both READMEs and matching `docs/en/`/`docs/zh-CN/` pages in the same
development batch as changes to interfaces, commands, limits, status or next steps.
Keep historical validation dates and scope; record local, remote CI and hardware
results separately. Run `python3 scripts/check_docs.py`, `cargo xtask check-files`
and `git diff --check`. The Python check validates page parity, language links,
local files and heading fragments; reviewers still verify translation meaning.

## M16 filesystem foundation

`cargo xtask foundation-test` builds a normal native image and creates fresh controlled ext4
fixtures below `build/foundation/`. Four real TCG boots check readonly access on writable and
readonly devices, 10,001 directory entries, 20 MiB initialized bytes, a 200 MiB sparse file,
native disk execution, clean reboot, unknown-feature/replay rejection and serial recovery.
Each boot has a 120-second deadline (negative boots use the existing 60-second gate).
The gate retains the exact ISO/initramfs, original and rejected disks, hashes/result JSON
and `build/logs/foundation-*.serial.log`. Stopped valid disks must pass `e2fsck -fn`.
`test-host` includes independent ext4 fixtures and VFS cache-pressure tests; CI also runs
`python3 ports/gcc/test_fetch.py`. See [M16 evidence](../validation/M16.md).

Compiler inputs now fall back from musl.cc to the locked official more.musl.cc archive.
Both require the original digest. `ANOVA_SOURCE_CACHE=/absolute/directory` optionally
supplies archives by their locked local names (including `bootstrap.tgz`); cache corruption
or symlinks fail instead of bypassing verification. CI caches only checked source archives,
not generated compiler outputs. Existing ext2 images remain writable/default. Ext4 writes,
JBD2 recovery and migration/build responsiveness have no acceptance in this readonly gate.


## M16.4 opt-in writable ext4

```sh
cargo xtask ext4-image --linux build/disks/work.ext4
cargo xtask run --ext4-write --toolchain --disk build/disks/work.ext4
cargo xtask foundation-write-test
```

The new 8 MiB image uses the pinned non-journal extent profile. Omit `--linux` and
`--toolchain` for native applications. A writable run requires an explicit disk;
ordinary runs still mount ext4 readonly and retain the default ext2 recovery path.
The separate six-boot write gate verifies file/directory semantics, real guest
GCC/make, native OOM, ENOSPC, reboot, host e2fsck/hashes and actual virtio read/write/
flush failures. Its complete contract and retained evidence are in
[M16.4](../validation/M16.4.md). This historical gate covers non-journal write-v1; the separate commands below accept JBD2 and opt-in migration.

## M16.5/M16.6 journal and new work disks

```sh
cargo xtask ext4-journal-image --linux build/disks/new-work.ext4
cargo xtask run --ext4-write --toolchain --disk build/disks/new-work.ext4
cargo xtask ext4-migrate build/disks/old.ext2 build/disks/migrated.ext4
cargo xtask foundation-journal-test
cargo xtask foundation-migrate-test
```

The new 64 MiB/8 MiB-journal profile remains opt-in; both image commands require
new destinations. Migration reads a stopped/clean ext2 source readonly and saves
an exact inventory alongside the new image. JBD2 uses one synchronous checksum-v3
data/metadata transaction, audited replay and persistent orphan recovery. The new
gates retain write/flush/tear faults, independent e2fsck replay, real guest bzip2,
interruption/reboot retention and host-measured foreground UART budgets. See
[M16.5](../validation/M16.5.md) and [M16.6](../validation/M16.6.md). Default ext2,
existing software quotas, broader M16/remote CI and M17/M18 remain separate.

## M16 kernel capacity closeout

`cargo xtask foundation-capacity-test` uses a fresh 64 MiB JBD2 disk with inherited Rust seeds,
separates real heap/VFS/fd/task/disk exhaustion from clean guest kernel construction,
and verifies exact retained-ELF boot/rejection/recovery. Budget, sampling and evidence
contracts are in [M16 capacity](../validation/M16_CAPACITY.md). Actions temporarily run only on pushes to `main`; PR, manual and other-branch
triggers are disabled by owner request.

The local `e2db30f` execution passed all four sessions. Its measured normal build
used 9,494,480 heap bytes, 1,564 VFS slots, 14 tasks, 16 FDs/process and 6,072 disk
blocks at peak; the earlier pressure session separately reached real limits and
verified reuse. Read the capacity record for frame sampling limits, exact input
closure and complete retained logs before applying these figures to another recipe.
