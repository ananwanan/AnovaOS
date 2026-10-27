<!-- Summarize current delivery levels, architectural decisions, risks and remaining development priorities. -->
# Project outline

**English** | [简体中文](../zh-CN/PROJECT_OUTLINE.md)

Planning began on 2026-10-20; this outline was reviewed against the accepted development baseline
on 2026-10-25. Features below describe responsibilities and direction, while [STATUS](guides/STATUS.md)
and individual validation records establish actual implementation and acceptance.

## 1. Vision and technical path

AnovaOS is an operating system for individual developers. Establish a reliable command-line
application development platform, then extend the desktop experience. Its Rust kernel gradually
reuses selected Linux user software through an independently implemented ABI adapter.

The dependency order is reliable boot → resource management → isolated processes → usable
files/terminals → tested Linux programs → guest C/C++/Rust development → SDK and graphics.
Compatibility claims identify software, version, build options and tested behavior; neither
ELF loading nor a generic POSIX label establishes compatibility.

## 2. Four delivery levels

| Level | User-visible capability | Required evidence | Current bounded delivery |
| --- | --- | --- | --- |
| A: bootable kernel | Boot, inspect logs and diagnose exceptions | QEMU serial logs, image/dependency versions and decoded exits | M0–M3 locally accepted |
| B: application runtime | Run applications built on another machine | Ring 3, syscalls, files, process isolation and error recovery | Native/Linux profiles through M9 locally accepted |
| C: development platform | Edit, compile, link and run applications inside AnovaOS | C/C++/Rust/Cargo end-to-end execution and reboot retention | M11/M12 controlled profiles locally accepted |
| D: ecosystem platform | Install verified software and use GUI applications/SDKs | Version matrix, package policy, GUI input/output and recovery | Bounded M10/M13 packages and M14 profiles locally accepted |

Level C is an independently valuable result; complete desktop/drivers/compiler self-bootstrap
are not prerequisites. Accepted profiles do not establish general ecosystem compatibility.

## 3. Functional work packages

### 3.1 Engineering and boot

Pin Rust, Limine, OVMF, QEMU and image inputs with sources/checksums. Use real `xtask` commands,
separate host and kernel targets, and avoid root for ordinary runs. Boot UEFI/ELF, retain serial
and panic diagnostics, validate maps/framebuffer/initramfs, and establish ownership before any
boot-memory reclamation. Bootloader-reserved memory is currently retained.

### 3.2 Kernel foundations

GDT/TSS/IDT/IST, exceptions, ACPI/APIC and clocks; physical pages, page tables, address regions,
heap, mappings and DMA ownership; user isolation, NX/W^X, checked user access, failure recovery;
processes/threads, user preemption, synchronization, deadlines, signals and CPU/FPU context.
The current kernel runs on one BSP and is not preemptible in kernel context.

### 3.3 System services

VFS/descriptors/mounts with initramfs/tmpfs/devfs; PCI/modern virtio-blk and bounded ext2 at
`/persist`; serial TTY, pipes, redirection and bounded PTYs; opt-in virtio-net IPv4 TCP/UDP/DNS;
locked package installation, sysroots and source preparation. Root remains initramfs.
Clean-shutdown persistence is tested; journaling and power-loss recovery are future work.

### 3.4 Applications and compatibility

Checked ELF64 loading, argv/env/auxv, TLS and bounded dynamic linking; native and Linux x86_64
syscall personalities mapped to common kernel semantics; POSIX library APIs distinguished
from Linux kernel ABI requirements. Start with static musl and selected BusyBox commands,
then bounded dynamic musl and development tools. glibc, Wine/Windows, browsers, Electron,
commercial CAD, games and loading Linux kernel drivers remain outside initial scope.

### 3.5 Development inside AnovaOS

Editor, writable persistent volume, shell, temporary files and process/build tools;
GCC/G++/assembler/linker/headers/CRT/musl/libgcc/libstdc++; guest Rust std, LLVM tools,
rustc/Cargo and a C linker. Accept offline dependencies, build.rs, procedural macros,
C FFI and diagnostics independently. Host-prepared guest compilers are usable before
compiler source reconstruction. Native AnovaOS `std` is a separate port.

### 3.6 Rust graphics and C/C++/Rust SDKs

Rust implements graphics, user compositor and runtime. Kernel display/input/mapping/IPC
mechanisms remain separate from user window policy. One versioned Rust-implemented C ABI
backs C headers, C++ RAII and safe Rust wrappers with shared event/error/lifetime semantics.
The five separately accepted stages are software framebuffer, PS/2 input/cursor, isolated
multiwindow compositor, runtime and independently guest-built three-language clients,
and bounded optional hardware VirGL clear/readback with CPU fallback.
Desktop composition remains software-rendered. Scanout alone is not GPU acceleration;
general GL/Vulkan, SDL/Qt, Wayland/X11, Linux GUI programs, USB input, SMP and real hardware
require independent acceptance. See [M14 GUI](milestones/M14_GUI.md) and [ROADMAP](ROADMAP.md).

### 3.7 Controlled source reconstruction

M15.1/M15.2 measure resources and reconstruct locked bzip2 in the guest. M15.3/M15.4 audit
GCC and Rust/LLVM/Cargo sources and exercise actual prerequisite failures; compiler builds
remain blocked. M15.5 constructs the real kernel using inherited tools/target libraries,
then the host packages the exact retained ELF. M15.6 repeats bzip2/kernel construction twice
on independent clean disks, compares exact final outputs and checks rejection/reboot recovery.
Compiler reconstruction and image generation entirely inside AnovaOS remain future work.

## 4. Implementation order and remaining priorities

[ROADMAP](ROADMAP.md) records accepted M0–M15 boundaries, the bounded M16 implementation
and the planned M17–M18 sequence.
M4 supplied a minimal SDK before M13; networking did not block offline compilation.
[M16 resources/VFS/ext4](milestones/M16_FOUNDATION.md) now has local readonly/write,
JBD2, migration/bzip2/UART and fresh-ext4 kernel capacity/recovery acceptance.
Complete remote CI is tracked in [M16 capacity](validation/M16_CAPACITY.md).
Then proceed to [M17 development desktop](milestones/M17_DESKTOP.md) using the existing
guest compilers, retaining old ext2 images and all regression gates. Complete compiler reconstruction and guest-only image generation
are conditional M18 work after the desktop. A new filesystem format, native std, broader
GUI/runtime compatibility, SMP and physical hardware each need their own acceptance plan.

## 5. Architectural decisions

| Decision | Reason | Revisit when |
| --- | --- | --- |
| Modular monolithic Rust kernel | Control complexity with explicit boundaries | Stable baseline and demonstrated isolation need |
| Reuse Limine | Focus effort on kernel/user space | A concrete boot requirement is blocked |
| QEMU first | Repeatable logs/devices/faults | Reference regression gates are stable |
| One CPU before SMP | Introduce concurrency in verified steps | Single-core scheduling/locks/interrupts are tested |
| Measured Linux subset | Extend semantics for actual programs | New software traces demonstrate a requirement |
| Run tools before rebuilding them | Deliver app development earlier | Dependencies/capacity support source bootstrap |
| Rust GUI plus versioned C ABI | One implementation, three SDKs and user isolation | Actual protocol/language tests show unacceptable limits |

## 6. Risks and responses

Pin software and drive compatibility work with traces and regression tests. Keep GUI work
subject to system/toolchain acceptance. Static tools still require real syscalls/TLS/signals/
threads/files. Record locked inputs, checksums, host roles and offline provenance. Separate
disposable disks from persistence tests, preserve flush/error semantics and avoid power-loss
claims. Bound compiler backends/parallelism and measure resources with stated sampling limits.
Use Linux-musl first and scope native std separately. Track dependency versions, patches,
notices and license obligations before redistribution.

## 7. Initial exclusions

The reference platform focuses on x86_64 user space, virtual machines and developer workflows.
32-bit support, full Linux distributions/POSIX certification, arbitrary PC drivers, general
accelerated desktop graphics, full Win32, mobile platforms and production security certification
remain future work. Adjust boundaries only with concrete requirements and evidence.

## 8. Completion rules

Each milestone supplies source/dependency identities, runnable entry points, observable results,
negative cases and limits. Update [STATUS](guides/STATUS.md) only with actual normal/failure-path
evidence; identify unexecuted checks. Keep both languages and next steps current in the same
delivery. Milestones describe capabilities, not unverified calendar promises.
