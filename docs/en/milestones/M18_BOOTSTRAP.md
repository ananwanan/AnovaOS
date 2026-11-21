<!-- Define the independent M18 compiler, image-generation and repeat/recovery profiles. -->
# M18: compiler and boot-image reconstruction

**English** | [简体中文](../../zh-CN/milestones/M18_BOOTSTRAP.md)

Development is on `feat/m18-bootstrap` from `dev`, with pull requests targeting `dev`.
M16 remote closeout remains deferred and GitHub Actions still runs only on pushes to
`main`. The [roadmap](../ROADMAP.md#m18-conditional-self-hosting) tracks exits per route.

## GCC first stage

The GCC 11.2.0 stage uses the explicit Linux-musl personality on AnovaOS x86_64. The upstream
build, host and target arguments use `x86_64-linux-musl`. The C/C++ stage uses the matching `/tc`
musl and C++ sysroot, disables bootstrap and multilib, and starts with a single make job. The
new drivers and frontends stay in a separate workspace and must compile, link and run both
languages before stage2/stage3 evaluation. The historical
[M15 GCC profile](M15_GCC.md) keeps its original tools and missing-prerequisite checks.

`cargo xtask m18-gcc-test` exercises the first stage. The `--entry` diagnostic retains the
original 12-file configuration subset and its missing-source boundary. Both admit a host-built,
static BusyBox 1.36.1 at `/bootstrap` from the existing pinned source commit with a separate
resolved configuration, binary digest, license and corresponding-source archive. The fixture
checks real output and status for `sed`, `awk`, `grep`, `expr`, `tr`, `sort` and `cmp` and
checks that `mv` replacement leaves the original open descriptor readable. Ash's built-in
`printf` handles the 13,312-byte fixture operand and `install` preserves execute mode and
exact bytes. BusyBox `kill -0` observes the live configure process for the COM1 response
check. The guest compares the exact 2,049 heredoc bytes through ash's inline pipe path.
The fixture verifies an
archive-provided link resolves and records missing-source and unsupported hard/symbolic-link
creation through `ln`. The profile can use an upstream configure copy fallback where available.

GMP 6.2.1, MPFR 4.1.0 and MPC 1.2.1 use the [existing source lock](../../../ports/gcc/inputs.json).
The host prepares their Linux-musl static archives, headers and notices. A guest application
compiled and linked against them tests consumption of those inputs. Rebuilding each dependency
inside AnovaOS would require separate evidence. The inherited GCC/G++, binutils, make and
sysroot remain the M11 seed. Versions, recipes and inventories are retained per run.

The source preparer verifies the upstream archive digest and retains a per-file inventory.
Stage1 puts the selected C/C++ source, x86 generators and host support libraries on a fresh
ext4 disk. It preserves the original `configure` scripts, `Makefile.in` files, release input
times and execute bits. The guest runs top-level `configure` with the selected ash, seed
GCC/G++ and staged arithmetic inputs. The recipe disables site and probe caches with
`/dev/null`, sets `--prefix=/persist/m18/gcc-stage1`, `--with-sysroot=/tc` and
`--disable-nls --disable-shared --disable-lto --without-isl`. It uses the original
`/tc/bin/make -j1 SHELL=/bootstrap/sh all-gcc` graph. Logs, exit statuses and the stopped disk
show which target was actually reached. After a successful make, the gate requires
`xgcc` and `xg++` to select the guest-built `cc1` and `cc1plus`, assemble and link with the
seed binutils and sysroot, run both programs, reject invalid C and an unresolved C++ symbol,
and verify the retained outputs after reboot. The stage fixture puts the immutable ELF
`as` and `real-ld` aliases first in the driver search path and checks the selected tools.
Both aliases resolve to existing `/tc/bin` tools. See [M18.1](../validation/M18.1.md) for results.

The local first-stage acceptance used the original guest build graph resumed from
retained ext4 work disks, then separate application and reboot verification with
KVM. The independent clean TCG gate has its own measured watchdog result. Stage2
and stage3 require their own compiler-stage evidence.

The declared TCG profiles use one BSP, 1 GiB guest RAM, no NIC, a 196,608-page sampled
frame-delta ceiling and a 3-second COM1 probe. Stage1 uses a fresh 512 MiB JBD2 ext4 disk
with 131,072 4 KiB blocks and 16,384 inodes and a 14,400-second build deadline. The entry
diagnostic uses a fresh 64 MiB disk with 16,384 blocks and 4,096 inodes and a 900-second
deadline. The initial
[300-second failure](../validation/M18.1.md) has a separate record. Current shared limits include a 16 MiB
kernel heap, 32 task slots, 32 user FDs/process, 2,048 VFS nodes, 256 MiB controlled ext4
files, 4 MiB ext2 files, and 1 MiB tmpfs files and readonly shared-file caches. The gate
retains clean shutdown/reboot and independently checks work-disk content. Disk peaks
cover successful allocation mutations. Scheduler-boundary frame samples describe the
observed pressure and do not establish exact peak RSS.

## Independent Rust components

The locked Rust 1.90.0 source's `src/stage0` requires Rust 1.89.0. The M12 Rust 1.90.0 musl
binary set can compile applications and is an inherited seed only. Retain the original
[M15 Rust feasibility assessment](M15_RUST.md). Record each LLVM, rustc, Cargo, std library
and linker identity independently. Full LLVM construction requires working CMake, C++ and
build execution with the selected source and measured budgets. A rustc stage requires
the prescribed stage0 and its matching host libraries, plus a verified offline source and
dependency closure. Cargo also needs its own locked offline closure and build/test evidence.
No component can substantiate the others or a multistage bootstrap.

`cargo xtask m18-llvm-test` selects only the original standalone `LLVMDemangle` library from
the locked Rust release's LLVM fork. It uses inherited guest G++ and binutils to compile six
translation units and archive them, then compiles, links and runs its own test program. The
gate checks actual compiler-emitted non-system dependencies against the source selection and
compares archive members with the objects. Its work disks are independent 64 MiB JBD2 ext4
images, with one BSP, 3 GiB RAM, a 900-second session limit and a 5-second live COM1 check.
The two builds produced exact archive and fixture matches and retained the prior bytes through
real syntax/link failures and fresh-boot relinking. The compile uses `-pipe` because the largest
assembly intermediate measured on the host exceeds the 1 MiB tmpfs file bound.
See [M18.2 evidence](../validation/M18.2.md) for executed results and remaining Rust components.

## Guest construction of the boot image

The first M18.3 profile uses an independently verified retained guest-built base kernel and
the existing UEFI/Limine optical boot path. The guest image writer consumes checked regular
files, including the exact kernel ELF, initramfs, native app modules, Limine configuration,
EFI executable and optical boot image. It writes a fixed ECMA-119 ISO with Rock Ridge names
and an El Torito UEFI catalog. Host preparation of inputs and a runner ISO, export
of the guest output, independent image inspection and QEMU boot remain explicit. The final
ISO must come from the guest writer. Extract the kernel and compare exact bytes and SHA-256,
boot the resulting ISO under QEMU and check its embedded identity and applicable regression
markers. The retained-kernel [M15](M15_KERNEL.md)/[M16](../validation/M16_CAPACITY.md) checks
continue separately. GUI-enabled kernel construction is a separate profile.

`cargo xtask m18-image-test REPORT GUEST_ELF` accepts the preserved non-GUI kernel
build profile (`release`, `opt-level=z`, `ext4-write,bootstrap-metrics`). It verifies the
M16 capacity report or the matching retained replay receipt, UART and stopped work disk.
The host prepares the locked Limine files and compiles the first-party packer. The guest
runs it against the verified inputs and publishes the final image. The packer streams
4 KiB at a time, with a 16 MiB per-input cap and a 32 MiB output cap. A candidate is synced
before rename, with explicit stale-candidate cleanup. The gate declares 64 MiB JBD2 ext4
work disks, one BSP, 256 MiB RAM, 600-second build/90-second boot sessions, and a 5-second
COM1 cancellation response bound. See [M18.3 evidence](../validation/M18.3.md).

## Repetition and recovery

Use independent clean work disks for repetitions. Compare exact inputs and outputs by path
and SHA-256 and preserve raw status, kernel/serial logs and all observed differences. Record
what the failure leaves visible and what a fresh boot can recover. Keep old tool and kernel
paths and the independent ext2/COM1 recovery route. A repeated bounded preflight or ISO
construction only covers the components actually exercised. Compiler failure, cancellation,
resource exhaustion and publication failure require component-specific proofs before declaring
M18.4 complete.
