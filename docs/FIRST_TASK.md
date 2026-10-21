# M0/M1 implementation order and acceptance

The starting revision was `847f7c66ddef25616c27a088d0c7044c6f9e8c0d` on `dev`:
planning documents only. No Cargo command or kernel was assumed to exist.
This change implements the following sequence. Follow [AGENTS.md](../AGENTS.md);
actual outcomes are in [validation/M0-M1.md](validation/M0-M1.md).

| Order | Implementation | Executable check | Completion criterion |
| --- | --- | --- | --- |
| 1 | Inspect branch, status, instructions and real files | `git status --short --branch` | Start from dev; preserve existing changes and identify absent commands |
| 2 | Create workspace, real kernel/host xtask and alias | `cargo metadata --locked --no-deps` | Host tools use host std; no global bare-metal target |
| 3 | Pin tested Rust, target, rustfmt/Clippy and Cargo.lock | `cargo fmt --all -- --check` | Rust 1.90.0 works; first-party packages use Rust 2024/Apache-2.0 |
| 4 | Add no_std entry, abort panic and kernel-only linker script | `cargo xtask build` | Static ELF64 x86_64 ET_EXEC; high-half entry in RX segment; no PT_INTERP/PT_DYNAMIC/RWX |
| 5 | Lock and verify boot inputs; implement diagnostics | `cargo xtask fetch`; `cargo xtask doctor` | Limine/OVMF hashes verified; missing tools and modified inputs return nonzero |
| 6 | Retain requests and validate protocol responses | `cargo xtask smoke` | Base 3, UEFI64, four-level paging, HHDM, map and kernel reservation validated |
| 7 | Initialize COM1 and independent panic output | `cargo xtask smoke --case panic` | Identity/phase logged; controlled panic and QEMU 35 observed; xtask returns 1 |
| 8 | Retain boot memory and report optional data | Inspect boot.serial.log | Map/reservations reported; no allocation/reclamation; framebuffer can be absent |
| 9 | Generate UEFI ISO and run QEMU/OVMF | `cargo xtask image`; `cargo xtask run` | Normal kernel reaches BOOT:OK then idle; CODE read-only, VARS copied per run |
| 10 | Implement watchdog and fault injections | `cargo xtask verify` | Ordered stages plus final marker plus QEMU 33 pass; reached panic/hang are rejected |
| 11 | Synchronize status, commands, evidence and CI | Targeted Clippy, host tests, workflow | Implemented commands are used; remote CI results remain separate from local evidence |

The alias and executable are created before the first `cargo xtask` invocation.
`fetch` downloads; `doctor` only checks and never installs host packages.
See [DEVELOPMENT.md](DEVELOPMENT.md) for setup and command contracts.

## M0 gate

Host xtask builds with host std and linker. The kernel uses precompiled bare-metal
core and bundled LLD; nightly and custom target JSON are unnecessary. Formatting,
separate host/kernel Clippy, and actual host logic tests pass. Tool versions,
boot-input commits and checksums are recorded. Missing dependencies return 1.

## M1 gate

q35/TCG + OVMF + Limine boots the actual Rust ELF with 256 MiB and one vCPU.
Success requires ordered `[BOOT:ENTRY]`, `[BOOT:PROTOCOL]`, `[BOOT:MEMORY]`,
`[BOOT:OK]`, `[TEST:PASS]`, no `[PANIC]`, and QEMU exit 33.

Controlled panic must reach its injection text and QEMU 35. Controlled timeout
must reach `[TEST:HANG]` before the watchdog kills and reaps QEMU. The standalone
negative smoke commands return 1. `verify` expects those failures; an unreached
injection point fails the whole gate. Corrupt input stops image generation.

Normal startup stays in `cli; hlt`, with no test exit device access. Firmware
source hashes stay unchanged and outputs remain under target/ or build/.

## Ownership and next task

BootInfo borrows the immutable Limine map for all of M1. There is no allocator;
all memory remains retained, including bootloader stack, page tables, responses,
ACPI, kernel and modules. Reserved entries may overlap; usable and bootloader
reclaimable ranges are checked for alignment and overlap. M3 must establish
explicit ownership before reclaiming anything.

Base revision 3 supplies a physical RSDP address. M1 reports it without parsing
ACPI, using firmware services, drawing to the framebuffer, or executing modules.
M2 next adds GDT/TSS/IDT, diagnostic exceptions, a double fault path and a timer
interrupt. Page allocation and a heap follow the roadmap dependencies.
