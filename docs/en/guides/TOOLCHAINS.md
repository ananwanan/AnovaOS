<!-- Explain build/host/target roles, accepted guest toolchains and separately scoped compiler and kernel reconstruction. -->
# From cross compilation to development inside AnovaOS

**English** | [简体中文](../../zh-CN/guides/TOOLCHAINS.md)

## 1. Build, host and target are different roles

| Stage | build | host | target | What it establishes |
| --- | --- | --- | --- | --- |
| Cross-build kernel/app | Linux developer machine | Linux developer machine | AnovaOS kernel/user ABI | Host can generate code for the OS |
| Prepare guest-hosted compiler | Linux developer machine | Tested AnovaOS user ABI | Tested AnovaOS user ABI | Compiler preparation; guest execution still required |
| Build applications in guest | AnovaOS | AnovaOS | AnovaOS | Native-machine application development |
| Compiler bootstrap | AnovaOS | AnovaOS | AnovaOS | Declared compiler components reconstructed on the OS |

The tested guest user ABI can be Linux x86_64 musl. A host-built compiler can run in AnovaOS
before the guest can rebuild complete GCC/LLVM. Do not confuse compiler execution with bootstrap.

## 2. Two user-space routes

### Route A: reuse tools through Linux compatibility

Keep the independent Rust kernel and use a controlled Linux x86_64 musl C/C++ sysroot;
Rust can use `x86_64-unknown-linux-musl`. Programs invoke Linux ABI semantics that AnovaOS
must actually implement. Begin with static or controlled-dependency compiler packages plus
editor/shell/libc/linker/development inputs. A custom OS does not automatically satisfy the
official Linux target's runtime assumptions. This route avoids an initial custom compiler
platform port, but implementing compatible kernel semantics remains substantial work.

### Route B: native AnovaOS target, independently scoped

Define native syscalls, CRT, stacks, dynamic-loading/libc contracts; add compiler configuration,
startup/link rules and C/C++ runtimes. newlib may be assessed but does not imply complete POSIX.
Start Rust with custom `core/alloc`, then port std's platform/libc/thread/file/time/process layers.
`x86_64-unknown-anovaos` is a candidate local name, not an existing upstream Rust/GCC target.
A target JSON supplies some code-generation settings, not the runtime or standard library.
Pin local patches and differences before pursuing upstream host/target support.
Report Route A development and Route B native std separately.

## 3. Kernel and first applications

`x86_64-unknown-none` supplies bare-metal ELF core, not std; use alloc only after allocator setup.
Kernel linker/no-red-zone flags must not contaminate user programs or host xtask. Prefer stable;
if nightly is actually necessary, pin the tested date. Experimental `-Z build-std` does not port std.

M4 supplies `_start`, native runtime, user linker and write/exit wrappers for Ring 3 no_std apps.
Exit through the syscall; do not return to an invalid address or call kernel functions directly.
Define user code model, red zone, SSE/FPU and linkage independently of kernel defaults.
For initial std applications, cross-build with locked Linux-musl tools, then test guest
allocation/argv/files/errors. Inspect ELF to establish static/PIE behavior rather than infer it
from a target string.

## 4. GCC/G++ pipeline

### Prerequisites

The kernel needs tested exec/wait, address spaces, files/directories/temp files, pipes, time,
signals and related semantics. User space needs shell, CRT, headers, libc, assembler/linker,
compiler support libraries and consistent sysroot. `x86_64-elf-gcc` alone provides none of a
complete user OS's headers/libc/startup/runtime and is not application-development evidence.

Prepare actual gcc/cc1/assembler/linker/collect2 components and search paths; distinguish GMP/
MPFR/MPC build and runtime dependencies. A version banner does not prove subtools can build.
Record verified configure recipes/sysroot/patches; changing `--host` does not establish a port.
Static tools reduce shared-library deployment but still need fork/exec/mmap/signals/threads/libc.
TCC may support an earlier experiment; it does not replace GCC/G++ acceptance.

C++ additionally needs cc1plus, libstdc++, libgcc/libgcc_s or declared alternatives, ABI support,
constructors/destructors and thread runtime. If exceptions/unwinding are disabled, report that
configuration explicitly and accept enabled exceptions separately.

### Accepted M11 offline workflow

The [M11 profile](../milestones/M11_TOOLCHAIN.md) implements these guest commands:

```sh
gcc main.c -o hello-c
./hello-c
g++ main.cpp -o hello-cpp
./hello-cpp
make
```

Tested C includes stdio/argv/files/malloc/multiple sources and nonzero compilation failures.
C++ covers string/vector/RAII, exception unwinding and an actual worker thread; broader library
support is per-program. Tests verify subtool/header/temp-file behavior, no-op/selective incremental
make, link failures, guest-edited sources, guest-generated ELF and reboot retention.

## 5. rustc, Cargo and LLVM

LLVM/rustc require more memory, TLS, threads, synchronization, files/mappings/signals/runtime
than a hello app. Limit backends and parallelism, measure resources with stated sampling scope,
then assess full bootstrap. Packages require matching rustc/std/target files/LLVM dependencies,
C linker and sysroot; rustc alone cannot complete the ordinary ELF link.

### Independently accepted Cargo layers

| Layer | Behavior | Additional prerequisites |
| --- | --- | --- |
| R1 | Guest rustc builds a single-file std app | Compiler, std, linker, OS semantics |
| R2 | Dependency-free Cargo application | Children, files, environment, metadata, locks |
| R3 | Locked offline dependencies | Registry/vendor/cache and Cargo.lock |
| R4 | build.rs and C FFI | Host/target distinction, scripts and C tools |
| R5 | Procedural macros | Compiler-host dynamic libraries, matching ABI/search paths |
| R6 | Online crates.io/Git | Network, DNS, TLS, CA, time and transport |

M12 accepts R1–R3, build.rs/C FFI, a standalone macro and a fixed Cargo macro/build-script graph.
The graph checks shared host/build dependencies, generated input/environment, dependency rebuilds,
rejected macro input and retained prior output. R6 independently accepts strict HTTPS sparse
registry/Git, controlled transport negatives and fresh-target cache rebuild after no-NIC reboot.
The public gate uses pinned crates.io/GitHub inputs without vendor substitution; local acceptance
uses a configured host proxy and public PEM roots. See [M12 Rust](../milestones/M12_RUST.md),
[M12 network](../milestones/M12_NETWORK.md) and [evidence](../validation/M12.md).
Rust 1.90.0/LLVM/Cargo/std are official locked musl binary tools, not a local Rust source build.

build.rs and proc-macros execute on the compiler host; this can differ from the application target.
Keep that distinction even in guest builds. Do not compile build scripts as kernel-target binaries.

### Implemented workflow

Start `cargo xtask run --rust`; inside AnovaOS:

```sh
rustc main.rs -o hello-rust
./hello-rust
cargo build --offline
./target/debug/m12-app
```

Acceptance covers String/Vec/argv/files, threads, child output/spawn/wait from a worker, build
scripts, shared macros, C FFI, locked vendor crates, no-op/source changes, real syntax failure and
clean reboot. For online `/persist` projects start `run --rust --network` and use `cargo-online build`;
cache defaults to `/persist/cargo-home`. Reboot without NIC and use `cargo-online build --offline --frozen`.
Ordinary cargo keeps its offline default. General projects, native std and compiler bootstrap are separate.
Host gates are `cargo xtask rust-build`, `rust-cross-test`, `rust-runtime-test`, `rust-test`,
`rust-online-test` and independent `rust-public-test`; full commands/bounds are in the M12 profiles.

## 6. Filesystem and package contents

The target layout anticipates `/bin`, `/usr/bin`, `/usr/lib`, `/usr/include`, `/tmp`, `/dev`
and only actually required `/proc`; current tool archives use `/tc` and `/rs`. Follow real tool
search paths. PT_INTERP determines interpreter location; do not rename it arbitrarily.
Record compiler versions, host/target ABI, sysroot/libc/CRT/libraries, hashes, patches and notices.
Record application ABI separately so user output cannot be mistaken for a kernel image.

## 7. Development versus source bootstrap

Using prepared compilers inside AnovaOS to build and run applications already meets the guest
application-development goal. Rebuilding GCC/Rust/LLVM/the kernel is additional M15 work with
extra ecosystem/capacity requirements; it must not become an implicit app-development prerequisite.

[M15](../ROADMAP.md#m15-optional-bootstrap-and-kernel-reconstruction) separates capacity/workspace,
a small source-built C/C++ tool, GCC, Rust/LLVM/Cargo, kernel boot and repeated comparison/recovery.
[M15.1/M15.2](../milestones/M15_BOOTSTRAP.md) and [local evidence](../validation/M15.1-M15.2.md)
accept bounded bzip2 reconstruction. [M15.3 GCC](../milestones/M15_GCC.md) audits locked GCC/GMP/MPFR/MPC
and utilities, tests inherited C++11, missing headers/tools, configure startup and capacity recovery.
[Evidence](../validation/M15.3.md) is successful assessment, not a rebuilt compiler stage.

[M15.4 Rust](../milestones/M15_RUST.md) independently audits distribution/LLVM/Cargo/vendor,
release-specific stage0 and dependencies. Actual Rust std/C++17 seeds, tool exec failures and an
isolated offline Cargo dependency rejection have [local evidence](../validation/M15.4.md).
No LLVM/rustc/Cargo component is rebuilt; inherited Rust 1.90.0 is separate from prescribed 1.89.0 stage0.
GCC and Rust/LLVM compiler reconstruction remain blocked on measured tools/dependencies/capacity.
Full archives are not minimum source closures; any pruned build route needs a new assessment.

[M15.5 kernel](../milestones/M15_KERNEL.md) uses inherited bare-metal libraries, exact offline
sources and actual guest Cargo/rustc/rust-lld. The host packages the retained ELF without rebuilding,
compares extracted ISO bytes and verifies boot/recovery; see [evidence](../validation/M15.5.md).
Guest-only image generation remains separate. Kernel construction can use the accepted guest rustc
without first rebuilding that compiler.

[M15.6 repeat](../milestones/M15_REPEAT.md) is locally accepted: two independent clean bzip2 and
kernel builds, equal declared inputs/final artifact bytes and all 12 boot/rejection/recovery sessions.
Kernel intermediate differences are retained; see [evidence](../validation/M15.6.md).
These comparisons establish only the declared conditions, not universal reproducibility or seed-free bootstrap.
Keep guest kernel compilation, host image packaging and guest-only image generation separate;
retain input/seed/recipe identities and disclose nondeterminism and host assistance.

## 8. M13 native SDK and separate GUI SDK

The source-distributed [SDK 1.0.0](../milestones/M13_SDK.md) exports independent host C/C++/Rust
native v1 projects, a locked local Rust dependency, stable errors and explicit package metadata.
Freestanding templates have no libc, C++ STL/exceptions or Rust std. Normal user target flags and
the SDK linker remain separate from the kernel. Runtime argument helpers follow existing native
limits of 64 entries and 1,023 bytes/string, with exact-boundary and over-limit tests.
M11/M12 compilers keep their independently accepted Linux-musl tools and explicit Linux disk policy.
Native SDK export is not a native libc/std port or compiler-bootstrap proof. M14.4's separately
versioned [GUI SDK](../../../sdk/gui/README.md) is locally accepted and uses the same native
application personality through the independent `anova.gui.v1` library ABI.
