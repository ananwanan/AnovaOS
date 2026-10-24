<!-- Define the versioned native SDK, explicit application packages and bounded developer workflow. -->
# M13 application SDK and packages

**English** | [简体中文](../../zh-CN/milestones/M13_SDK.md)

M13 packages the existing native v1 runtime and the separate Linux-musl
development profile into an explicit developer contract. Acceptance evidence is
recorded in [validation/M13](../validation/M13.md). Existing platform limits in
[M12_RUST](M12_RUST.md) and [M10_NETWORK_PACKAGES](M10_NETWORK_PACKAGES.md)
continue to apply.

## SDK and ABI versions

The native SDK has its own semantic version, initially **1.0.0**. It targets
`x86_64` and `anova.native.v1`; this is distinct from the Linux toolchain target
`x86_64-unknown-linux-musl` and personality `linux.x86_64.v1`. The native Rust
template is freestanding `no_std`, not an AnovaOS `std` port. The pinned GNU
Rust target supplies core and ordinary user CPU code generation only. Native
C/C++ templates use the native CRT, syscall wrappers and user linker script;
the C++ profile has no exceptions, RTTI, STL or dynamic allocation runtime.
For accepted C++ exceptions/STL and Rust std use the separate M11/M12 guest
Linux-musl profile.

ABI v1 syscall numbers, negative errors, startup layout and wire structures
remain frozen. SDK releases may add wrappers and documentation without
renumbering existing values. New optional syscalls must fail with the existing
unsupported result on older kernels. Changing an existing number, wire layout,
error meaning, calling convention or startup contract requires a new ABI
personality and a separate compatibility gate. The SDK package version is not
a kernel syscall ABI version. Native startup permits at most 64 argv entries and
1,023 bytes per string; strings, terminators and pointer tables must also fit
the loader's checked 32 KiB startup block. The native user stack remains 64 KiB.

Native errors are independent of Linux errno. Raw native results remain
available; typed Rust wrappers preserve unknown negative values for forward
compatibility. C and C++ expose the same stable codes. Callers must handle
partial I/O, distinguish EOF from errors, and inspect child exit/fault status.
The ABI host tests and old native Rust/C workloads are compatibility gates.

## Cross-development

Use the repository's pinned Rust 1.90.0, host GCC/G++, GNU binutils and the host
`xtask`. Install the QEMU/image/compiler prerequisites from
[DEVELOPMENT](../guides/DEVELOPMENT.md) before booting an application. No kernel target
flags are applied to user programs.

```sh
cargo xtask fetch
cargo xtask doctor
cargo xtask sdk
cargo xtask app-new c build/examples/hello-c
cargo xtask app-build build/examples/hello-c
cargo xtask app-pack build/examples/hello-c/app.toml build/examples/hello-c/hello-c.apkg
cargo xtask root-image build/disks/sdk-native.ext2
APP_SHA=$(sha256sum build/examples/hello-c/hello-c.apkg | cut -d' ' -f1)
cargo xtask app-run build/examples/hello-c/hello-c.apkg "$APP_SHA" --disk build/disks/sdk-native.ext2
```

`app-run` validates the external hash, runs the real package parser, adds the
archive at `/repo/local.apkg` in a bootable ISO, and prints the exact guest
install/inspect/run commands with that hash. In the serial shell, paste those
commands, verify `status` is zero and use `shutdown`. The example's explicit
disk preserves the installed package and data. Without `--disk`, `app-run`
creates a temporary disk whose ABI matches the validated package. An existing
disk is never reformatted; a policy mismatch is rejected by the guest installer.
`app-image PACKAGE SHA256` creates `build/images/anova-app.iso` without launching
QEMU. `app-test` exercises this public image path through actual guest execution,
including a wrong external lock and the C template's stable missing-file error.

To upgrade, edit the source and change `version = 1` to `version = 2` in
`project.toml`, repeat `app-build`, `app-pack` and the hash calculation, then
`app-run` with the same disk. Install the new `/repo/local.apkg`, run
`pkg status hello-c` and execute `/persist/pkg/hello-c-b/file0` after the
second successful installation. The status output identifies the current slot;
subsequent upgrades alternate slots. Verify `status`, shut down cleanly and
restart with the same disk to check persistence. `pkg remove hello-c` preserves
`/persist/data/hello-c`.

Use `cpp` or `rust` with `app-new` for the other templates. The Rust template
includes an offline path dependency; its generated lockfile is consumed with
`--offline --locked`. Generated projects include source, project settings,
license, build output and an application manifest. Preserve the unstripped
`app.elf`, source and its SHA-256 when distributing a package so a crash can be
matched to the exact build. Package bytes and ELF debug symbols are separate
artifacts. `app-pack` verifies the source manifest hash, strips a temporary
payload copy to fit the 128 KiB profile, restores the ABI trailer, validates it
again and hashes the resulting package entries. The original unstripped ELF
is retained. Its SHA-256 differs from the installed compact ELF; code addresses
remain suitable for the retained symbol file. Record both hashes when matching
crashes to builds; a PID/PC trace alone carries no executable build digest.

`cargo xtask sdk-test` verifies SDK export, all three builds and ELF validation; `package-v2-test` separately verifies their actual
packaged guest executions. The SDK export carries the headers, runtime source,
ABI definitions, linker/CRT and templates with an inventory of file hashes.
Exports are assembled in a fresh candidate and published only when complete.
Unexpected extra files or symlinks in an existing release are rejected with an
identified path; move consumer/build files outside the generated release before
retrying. Keep the export immutable and build a copy when using it independently:

```sh
cp -R build/sdk/anova-sdk-1.0.0 build/sdk-consumer
python3 build/sdk-consumer/build.py rust build/sdk-consumer/templates/rust
```

The exported Rust template includes its manifest and an offline lockfile; it
does not require repository `app-new` to become buildable. The export does not
redistribute a host compiler or claim a compiler bootstrap.

## Application manifest and package admission

The `app.toml` source manifest is strict: unknown keys, duplicate entries,
invalid paths, unsupported schema/SDK/ABI/architecture, dependencies,
capabilities and permission values cause failure. It records an integer package
version, target architecture, exact ABI, entry file, required SDK or
musl profile, minimum capabilities and requested permissions. The emitted package
license is fixed to Apache-2.0 in this profile. The dependency
profile is deliberately bounded; arbitrary package dependency resolution is
not implemented.

The public packer emits an entry ELF plus LICENSE. ANOVAPK2 is a little-endian,
at most 128 KiB archive with a 256-byte header and
up to four 96-byte entry descriptors. Original ANOVAPK1 native archives retain
their existing reader. V2 records:

| Offset | Field |
| --- | --- |
| 0 | `ANOVAPK2` magic |
| 8 | Bounded package name |
| 36 | Increasing integer package version |
| 40 | SDK contract major |
| 44 | File count |
| 48 | Architecture (`x86_64`) |
| 64 | Explicit ABI personality |
| 96 | License (`Apache-2.0` in this accepted profile) |
| 112 | Supported package profile |
| 128 / 132 | Manifest revision / exact header size |
| 136 / 140 | Requested permissions / minimum capabilities |
| 144 | Entry filename |
| 192 / 224 | Dependency name / minimum dependency version |

Reserved bytes must be zero. Each descriptor declares a safe single-component
name, mode, exact length and SHA-256. A LICENSE entry is required. V2 executable
payloads additionally carry hash-covered ABI metadata, so accidentally
substituting a file built for the other declared target is rejected. These
hashes and metadata are integrity and consistency checks, not publisher
authentication or a signature scheme.

The mount-time `/persist/.anova-abi` image policy is authoritative. The VFS pins
its node identity and denies content writes, truncation, chmod, executable-bit
changes, unlink and either direction of rename while mounted, preventing the
installer's marker check from diverging from execution policy. Reads remain
available. Create a separate disk profile to change the policy. A native
disk accepts native packages; a Linux-profile disk accepts bounded static ET_EXEC
Linux packages. Dynamic/PIE Linux package admission is deferred; the existing M9
Linux runtime and its separate tests retain their broader executable support.
The installer checks this before staging any file. Execution uses the same
existing explicit VFS policy; the installer does not guess from ELF OSABI,
absence of an interpreter, or the invoking process's ABI. Mixed ABI persistent
disks are outside this profile. Initramfs still uses per-entry ABI records.

Permissions declare admission requirements on the current single-user system;
they do not create per-process filesystem/network sandboxes. Unknown or
unsupported values fail closed. M13 admits read/data permissions and VFS/persistent
storage requirements, plus the Linux capability for a Linux-profile package;
network requirements have no admission contract yet and are rejected. Capabilities describe tested platform services;
they do not authorize unimplemented kernel features. In particular GUI
capabilities are unavailable in M13.

## Install, upgrade and user data

Transfer a package together with its expected whole-archive SHA-256 into a
controlled image/repository, or use the existing controlled HTTPS repository.
`pkg install-file PATH SHA256` validates that external lock and the complete
archive before mutation. `pkg check PATH SHA256` performs host/guest format and
integrity checks; installation additionally checks the mounted disk policy.
An external hash must come from the intended source; an archive's own internal
hashes alone cannot authenticate its origin.

The two-slot installation contract remains unchanged. Files and manifest are
written/fsynced into the inactive slot; a checksummed generation record selects
the new active version. The declared entry maps to one of `file0` through
`file3`, with its original name/mode/hash retained in the installed manifest.
An upgrade must increase the version. Failed staging retains the active
version; removal publishes a tombstone. Application-owned user data belongs in
`/persist/data/NAME` and survives upgrades and removal. Never put user data in
the install slots. The non-journaled ext2 implementation still makes no
power-loss transaction guarantee.

`cargo xtask package-v2-test` covers manifest/parser negatives, ABI policy
mismatches, actual installed execution, upgrade/failure behavior and clean
reboot/data retention. Preserve `package-test` for the legacy package/TLS gate.

## Development inside AnovaOS

The M11/M12 development images provide guest-hosted GCC/G++, Rust/std and
Cargo. Those programs use the Linux-musl compatibility profile; keep their
manifests and persistent disk marked `linux.x86_64.v1`. The accepted source
editing/build/run/reboot instructions are in [M11_TOOLCHAIN](M11_TOOLCHAIN.md)
and [M12_RUST](M12_RUST.md). Offline Cargo uses the prepared dependencies and
`--offline --frozen`; online Cargo remains a separate opt-in profile.
Host cross-compilation and compilation inside AnovaOS have separate acceptance
records. The native SDK does not imply a native libc/std port.

## Logs, symbols and resource debugging

Use the UART log and native shell `status` after a command to distinguish a
normal return, package rejection and an isolated process fault. Kernel fault
records include process ID, vector, instruction/stack address and fault
address. M13 adds a bounded user frame-pointer trace and a resource snapshot
at captured hardware faults, before native termination or Linux signal handling.
The walk has at most 16 frames, a 1 MiB span above RSP and 64 KiB between links.
Only validated mapped user bytes are copied; corrupt,
unaligned, cyclic or out-of-range chains terminate the trace. Missing frame
pointers and optimized frames may produce a partial trace.

Native SDK builds retain debug symbols and frame pointers. Decode a trace
with the exact unstripped executable:

```sh
cargo xtask diagnostics-test
cargo xtask symbolize build/diagnostics/m13-crash.elf 0x40002b
# Replace 3 with the matching PID recorded in your fault log.
cargo xtask symbolize build/diagnostics/m13-crash.elf --log build/logs/diagnostics-guest.serial.log --pid 3
```

For logs with several processes, use `--pid` with the PID from the fault record;
the tool refuses to combine unrelated processes with one ELF. Position-independent/dynamically loaded code requires
the actual load bias and matching object; arbitrary dynamic symbolization is
not established. The kernel never parses DWARF in a fault handler.

The snapshot reports global free frames, global heap use and task capacity; it
is not per-process resource accounting. Compare it with the documented 32 task slots, 16 MiB kernel
heap, writable-file and address-space limits. Existing smoke/OOM gates verify
ownership recovery after process and allocation failures. Fix the failing
allocation or reduce workload size; do not treat an output marker as proof
that a compiler completed. GDB remote stubs, an interactive user debugger,
general DWARF unwinding and compiler self-bootstrap remain future work.

## Independent GUI SDK delivery contract

M14.4 implements a Rust GUI runtime with the separate `anova.gui.v1` C ABI,
C declarations, C++ move-only RAII and safe Rust bindings. Version negotiation,
opaque handles, fixed layouts, ownership, errors, event/thread rules and no
unwinding are specified in [M14 runtime](M14_RUNTIME.md) and the
[GUI SDK contract](../../../sdk/gui/README.md). The source-distributed companion SDK
has its own checksum inventory and linkage/execution metadata.

Native SDK 1.0.0 and ANOVAPK2 admission remain independent. GUI applications in
the bounded M14.4 profile statically link the runtime and are built/run on an
explicit native volume. ANOVAPK2 has no dynamic GUI service dependency admission;
M13 still rejects undeclared GUI requirements. No native libc/std, public Linux
GUI compatibility or process sandbox is implied by the companion library.
