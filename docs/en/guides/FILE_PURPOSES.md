<!-- Inventory all repository files and explain purpose-header exceptions and bilingual maintenance. -->
# Repository file purposes

**English** | [简体中文](../../zh-CN/guides/FILE_PURPOSES.md)

Every existing tracked or non-ignored untracked file is listed below. `cargo xtask
check-files` checks inventory coverage and source purpose headers; files removed
by a pending move are ignored. Review descriptions and semantic accuracy manually.
Keep this inventory and its Chinese counterpart synchronized with DEV-001/DEV-003.
Generated lockfiles and JSON do not accept ordinary comments; LICENSE/NOTICE and
upstream legal texts remain verbatim. Markdown uses a top English purpose comment.
New shared assets are listed here when actual images/diagrams are introduced.

| File | Purpose |
| --- | --- |
| `.cargo/config.toml` | Routes host commands through xtask and confines kernel code-generation flags to its target. |
| `.gitattributes` | Normalize first-party text to LF while preserving binary images and archives. |
| `.github/workflows/boot.yml` | Gate bilingual docs, host/user/kernel, graphics/input/composition and QEMU regressions, retain GUI evidence and run separate guest toolchain jobs. |
| `.gitignore` | Ignore generated host/user builds, images, toolchains and local machine secrets. |
| `AGENTS.md` | Define mandatory repository safety, validation, delivery and same-batch bilingual documentation maintenance conventions. |
| `Cargo.lock` | Generated locked kernel/host dependency graph and registry checksums; never insert manual comments. |
| `Cargo.toml` | Define kernel, host and shared policy/VFS packages; user binaries use their own workspace. |
| `LICENSE` | Original Apache-2.0 legal terms; preserve verbatim instead of adding a source header. |
| `NOTICE` | Project attribution and notice information; preserve existing legal content. |
| `README.md` | Introduce AnovaOS, its verified capabilities, build entry points and project goals. |
| `README.zh-CN.md` | Provide the synchronized Simplified Chinese project introduction and build guide. |
| `boot/dependencies.toml` | Locks verified compiler/QEMU versions and bootloader/firmware sources and hashes. Change only with refreshed integrity checks and the complete M1 validation. |
| `boot/limine.conf` | Defines the UEFI kernel entry; xtask appends explicitly tagged native ELF modules. |
| `crates/abi/Cargo.toml` | Defines the allocation-free native ABI shared by the kernel and application SDK. |
| `crates/abi/src/lib.rs` | Native v1 constants, checked user addresses and padding-free process/query layouts. These are AnovaOS interfaces; they do not implement the Linux syscall ABI. |
| `crates/abi/src/linux.rs` | Translate native errors and supported Linux open flags without contaminating generic subsystems. |
| `crates/abi/src/linux/strings.rs` | Decode page-bounded C strings and pointer vectors through a checked copy callback without allocation. |
| `crates/elf/Cargo.toml` | Hosts bounded ELF validation and native startup-stack construction without hardware. |
| `crates/elf/src/lib.rs` | Validate bounded native ET_EXEC and Linux ET_EXEC/ET_DYN segments, interpreter paths and TLS before mapping. |
| `crates/elf/src/stack.rs` | Encode bounded native/Linux arguments, environment and executable/interpreter auxv in checked startup storage. |
| `crates/graphics/Cargo.toml` | Build the dependency-free no_std layout and software drawing library for kernel and host checks. |
| `crates/graphics/src/lib.rs` | Validate bounded RGB modes, provide a fixed cursor raster and render clipped pixels, rectangles and blits into caller-owned buffers. |
| `crates/graphics/src/tests.rs` | Check RGB packing, bounds, canaries, overflow, clipping, literal cursor shape and deterministic pattern independently of hardware. |
| `crates/graphics/src/virgl.rs` | Encode bounded VirGL clear commands with integer UNORM conversion and independent host reference tests. |
| `crates/gui-runtime/Cargo.toml` | Package the dependency-free user-space GUI C ABI and safe Rust library. |
| `crates/gui-runtime/src/lib.rs` | Implement frozen GUI v1 FFI, bounded widgets, UTF-8 storage and Rust handle ownership. |
| `crates/gui-runtime/src/render.rs` | Clip RGB primitives and rasterize original ASCII glyphs with Unicode replacement boxes. |
| `crates/gui-runtime/src/runtime.rs` | Own copied native RPC, private raster lifetime and ordered loss-reset input delivery. |
| `crates/gui-runtime/src/tests.rs` | Check ABI rejection, lifecycle, rendering, UTF-8, resize and queue-pressure recovery. |
| `crates/gui/Cargo.toml` | Define the no_std internal GUI wire and software composition library. |
| `crates/gui/src/lib.rs` | Expose bounded user-space window policy and the experimental transport contract. |
| `crates/gui/src/policy.rs` | Validate private surfaces and implement damage, stacking, movement, resizing and focus. |
| `crates/gui/src/tests.rs` | Exercise adversarial wire packets, ownership, quotas, damage and routed input as pure logic. |
| `crates/gui/src/transport.rs` | Define copied kernel message, hardware-event and checked scanout layouts. |
| `crates/gui/src/wire.rs` | Serialize fixed-width versioned GUI requests/replies without pointers or padding. |
| `crates/input/Cargo.toml` | Build dependency-free no_std PS/2 decoding and bounded diagnostic event delivery for kernel and host checks. |
| `crates/input/src/lib.rs` | Normalize physical set-1 keys, three-byte mouse packets, bounded queue/reset/reconnect semantics and clipped pointer state without allocation or GUI policy. |
| `crates/input/src/tests.rs` | Check key/modifier/special-sequence state, full nine-bit motion, malformed/overflow recovery, consumer lifecycle and pointer bounds. |
| `crates/memory/Cargo.toml` | Packages reusable no_std frame, heap and page-table policies for kernel/host tests. |
| `crates/memory/src/frames.rs` | Only explicitly supplied usable RAM enters the allocator. All other memory stays reserved, including holes and every bootloader-reclaimable byte. |
| `crates/memory/src/heap.rs` | An intrusive buddy heap. Free blocks hold their own links; allocation and coalescing never allocate metadata or call another allocator. |
| `crates/memory/src/lib.rs` | Allocation and page-table policies shared by the kernel and host tests. |
| `crates/memory/src/paging.rs` | Four-level kernel/private-user tables, permission checks and table reclamation. User roots borrow supervisor upper entries; only their lower tables are owned. |
| `crates/memory/src/shared.rs` | Fallible single-thread reference-counted metadata ownership with exact final destruction and allocation-failure tests. |
| `crates/platform/Cargo.toml` | Packages hardware-independent ACPI parsing and typed physical/virtual addresses. |
| `crates/platform/src/acpi.rs` | ACPI 6.6, sections 5.2.5, 5.2.8, 5.2.9 and 5.2.12. Decode bytes instead of creating references to packed firmware structures. |
| `crates/platform/src/clock.rs` | Validate Gregorian UTC dates and Unix epoch conversion, with leap/boundary tests. |
| `crates/platform/src/lib.rs` | Hardware-independent address types and bounded ACPI decoding. |
| `crates/platform/src/power.rs` | Validate the restricted reference-QEMU PM1a and constant ACPI S5 handoff, with corruption tests. |
| `crates/sched/Cargo.toml` | Shares bounded process state, parent/wait semantics and round-robin policy with host tests. |
| `crates/sched/src/lib.rs` | Bounded BSP scheduling policy and parent/child lifecycle without hardware or allocation. The kernel owns contexts/resources; this policy never switches stacks or takes locks. |
| `crates/storage/Cargo.toml` | Declare the no_std checked sector/ext2 crate and its native error dependency. |
| `crates/storage/src/ext2.rs` | Implement the declared ext2 profile, checked allocation, retained modes/timestamps, ordered writes and clean shutdown. |
| `crates/storage/src/ext4.rs` | Read controlled ext4 extents and dispatch explicit audited non-journal/JBD2 writes with bounded allocation ownership. |
| `crates/storage/src/filesystem.rs` | Define persistent backend capabilities, stable metadata and bounded directory cursor operations. |
| `crates/storage/src/lib.rs` | Define owned synchronous sector devices, read-only/flush/stop contracts and checked ranges. |
| `crates/storage/tests/ext2.rs` | Verify on-disk ext2 bytes, indirection, sparse data, exhaustion, reboot and injected I/O failures. |
| `crates/storage/tests/ext4.rs` | Compare e2fsprogs images and inject extent, geometry, allocation and readonly device failures. |
| `crates/vfs/Cargo.toml` | Share bounded in-memory VFS, archive, open-file and pipe semantics with host tests. |
| `crates/vfs/src/archive.rs` | Decode the project-owned ANOVAFS1 archive using checked byte ranges, never casts. Entries are parent-first absolute UTF-8 paths; payloads remain borrowed from boot RAM. |
| `crates/vfs/src/fd.rs` | Own fallible shared descriptor tables, close-on-exec state and allocation-free empty placeholders. |
| `crates/vfs/src/file_mapping.rs` | Retain bounded shared-file bytes and version successful writes independently of descriptor lifetime. |
| `crates/vfs/src/foundation_tests.rs` | Stress on-demand lookup and 10,001-entry traversal, cache eviction, identities, shared offsets and maps. |
| `crates/vfs/src/lib.rs` | Own bounded memory/ext2 namespaces, explicit ABI inheritance, coherent backing-store timestamp precision, private mapping snapshots, shared opens, pipes and PTYs. |
| `crates/vfs/src/tests.rs` | Test VFS path/mount boundaries, descriptor ownership, private snapshots, persistent timestamp coherence, pipe shutdown and capacity rollback. |
| `docs/en/ARCHITECTURE.md` | Describe implemented subsystem boundaries and the planned operating-system architecture. |
| `docs/en/COMPATIBILITY.md` | Separate verified native support from planned existing-software ABI compatibility. |
| `docs/en/PROJECT_OUTLINE.md` | Original project goals, delivery levels, scope, work packages and risks. |
| `docs/en/ROADMAP.md` | Track milestone dependencies, implementation checklists and acceptance criteria. |
| `docs/en/guides/CODING_STYLE.md` | English development conventions, mandatory file purposes and stable Rust formatting. |
| `docs/en/guides/DEVELOPMENT.md` | Host setup, target separation, app/image commands, validation and evidence handling. |
| `docs/en/guides/FILE_PURPOSES.md` | Per-file purpose inventory and generated/legal file explanation exceptions. |
| `docs/en/guides/FIRST_TASK.md` | Historical M0/M1 implementation order and concrete initial acceptance criteria. |
| `docs/en/guides/GITHUB_SETUP.md` | Explain development branches, current subsystem entry points and contribution conventions. |
| `docs/en/guides/LICENSING.md` | Apache-2.0 usage and third-party licensing boundaries. |
| `docs/en/guides/REFERENCES.md` | Primary architecture, protocol, ABI and toolchain reference links. |
| `docs/en/guides/REQUIREMENTS.md` | Owner-mandated DEV-001 file-purpose rules and DEV-002 native GUI/SDK milestones. |
| `docs/en/guides/STATUS.md` | Record verified operating-system/SDK profiles, independent graphics stages and the next development task. |
| `docs/en/guides/TOOLCHAINS.md` | Distinguish guest C/C++/Rust application builds from online Cargo and optional compiler self-hosting. |
| `docs/en/index.md` | Navigate all English guides, milestone contracts, historical validation and third-party inputs. |
| `docs/en/milestones/M10_NETWORK_PACKAGES.md` | Define accepted IPv4/socket, DNS, TLS and package contracts and explicit exclusions. |
| `docs/en/milestones/M11_TOOLCHAIN.md` | Define the offline source-built GCC profile, resource/ownership boundaries and acceptance workflow. |
| `docs/en/milestones/M12_NETWORK.md` | Describe the in-progress online Cargo commands, runtime limits, trust inputs and independent acceptance gates. |
| `docs/en/milestones/M12_RUST.md` | Specify the optional Rust/Cargo profile, construction commands, ownership and separate acceptance gates. |
| `docs/en/milestones/M13_SDK.md` | Define SDK/ABI versions, package policy, developer workflow, diagnostics and future GUI delivery boundaries. |
| `docs/en/milestones/M14_COMPOSITOR.md` | Specify experimental GUI authority, copied surfaces, damage, input and restart semantics. |
| `docs/en/milestones/M14_GPU.md` | Define the bounded offscreen GPU profile, ownership, user interface, synchronization and fallback. |
| `docs/en/milestones/M14_GUI.md` | Plan the Rust-native GUI, C ABI and C/C++/Rust SDK architecture, security boundaries, five stage gates and limits. |
| `docs/en/milestones/M14_RUNTIME.md` | Specify implemented GUI ABI, rendering, event ownership and native guest build boundaries. |
| `docs/en/milestones/M15_BOOTSTRAP.md` | Define the bounded source-reconstruction workflow, preparation provenance and measurement limitations. |
| `docs/en/milestones/M15_GCC.md` | Define reproducible GCC feasibility assessment, pinned dependencies, measured blockers and conditions for a future compiler stage. |
| `docs/en/milestones/M15_KERNEL.md` | Define guest kernel construction, retained-ELF packaging, source/seed identities and separate image-bootstrap acceptance boundaries. |
| `docs/en/milestones/M15_REPEAT.md` | Define repeated clean reconstruction, archival, artifact comparison and recovery boundaries. |
| `docs/en/milestones/M15_RUST.md` | Define independent Rust, LLVM and Cargo feasibility, prescribed stage0, measured blockers and reconstruction prerequisites. |
| `docs/en/milestones/M16_FOUNDATION.md` | Plan measured resources, filesystem backend isolation, controlled ext4 and verified migration before desktop work. |
| `docs/en/milestones/M17_DESKTOP.md` | Plan the supervised development desktop, PTY terminal, reliable editing, diagnostics and graphical acceptance. |
| `docs/en/milestones/M2_CPU.md` | BSP traps, ISTs, ACPI/APIC timer, interrupt context and M2 limitations. |
| `docs/en/milestones/M3_MEMORY.md` | Frame/table/heap ownership, virtual layout, locks and original M3 limitations. |
| `docs/en/milestones/M4_USERSPACE.md` | Historical M4 entry/loader/copy/SDK contracts, with M5 supersession and ownership notes. |
| `docs/en/milestones/M5_PROCESSES.md` | Define BSP scheduling, CPU/FPU state, native process syscalls and ownership/locking boundaries. |
| `docs/en/milestones/M6_FILESYSTEM.md` | Define VFS/archive, fd/pipe ownership, native ABI, terminal, init and shell contracts. |
| `docs/en/milestones/M7_STORAGE.md` | Define PCI/DMA/queue ownership, ext2 bounds, native durability ABI, disk images and source editor. |
| `docs/en/milestones/M8_LINUX.md` | Define supported Linux startup/syscalls, bounded semantics, build recipes and explicit unsupported behavior. |
| `docs/en/milestones/M9_RUNTIME.md` | Exact M9 ownership, flags, limits, unsupported operations, runtime checksum and build/verification contract. |
| `docs/en/third-party/m10.md` | Record locked network/TLS sources, notices, redistribution and fixture-key boundaries. |
| `docs/en/third-party/m11.md` | Record pinned compiler licenses, source obligations and the local-validation distribution boundary. |
| `docs/en/third-party/m12.md` | Record official Rust/Cargo/LLVM distribution provenance, licenses and unwinder boundaries. |
| `docs/en/third-party/m15.md` | Record official bzip2 source checksums, selected notices, licensing and inherited seed-tool boundaries. |
| `docs/en/third-party/musl.md` | Preserve the unmodified tested musl package copyright and MIT notices for guest distribution. |
| `docs/en/third-party/network-licenses.md` | Preserve upstream license texts for locked network runtime dependencies. |
| `docs/en/validation/M0-M1.md` | Historical local engineering/boot acceptance evidence and input hashes. |
| `docs/en/validation/M10.md` | Record exact M10 build inputs, guest evidence and complete regression results. |
| `docs/en/validation/M11.md` | Record actual M11 source-build inputs, guest outputs, negative cases and complete regression evidence. |
| `docs/en/validation/M12.md` | Record actual M12 guest compilation, feature-specific evidence and inherited regression results. |
| `docs/en/validation/M13.md` | Record actual M13 SDK/package/crash diagnostics and preserved regression evidence. |
| `docs/en/validation/M14.1.md` | Record actual graphics/negative tests, artifacts and the scope of preserved local regression checks. |
| `docs/en/validation/M14.2.md` | Record actual polled PS/2/cursor gate results, bounded recovery cases, artifacts and regression limits. |
| `docs/en/validation/M14.3.md` | Record actual M14.3 execution, scanouts, fault/OOM recovery and regression evidence. |
| `docs/en/validation/M14.4.md` | Record actual independent language, graphical terminal, fault and reboot validation evidence. |
| `docs/en/validation/M14.5.md` | Record actual local hardware rendering, fallback, regression evidence and explicit limits. |
| `docs/en/validation/M15.1-M15.2.md` | Record actual local capacity, guest bzip2 build, reboot, rejection, hash and regression evidence. |
| `docs/en/validation/M15.3.md` | Record local GCC feasibility probes, exact artifacts, reboot checks and unverified compiler stages. |
| `docs/en/validation/M15.4.md` | Record local Rust/LLVM/Cargo feasibility probes, exact retained artifacts, reboot evidence and unverified source-build layers. |
| `docs/en/validation/M15.5.md` | Record actual guest kernel compilation, exact retained-ELF boot and recovery evidence, capacity regressions and host-assistance limits. |
| `docs/en/validation/M15.6.md` | Record actual two-round guest reconstruction, artifact comparison and recovery execution evidence. |
| `docs/en/validation/M16.md` | Record partial M16 acquisition, measured guest build, backend/cache and controlled ext4 readonly acceptance. |
| `docs/en/validation/M2.md` | Historical local CPU/timer/trap validation evidence and limitations. |
| `docs/en/validation/M3.md` | Historical local memory/protection/OOM validation evidence and limitations. |
| `docs/en/validation/M4.md` | Local native app/isolation/teardown results, environment, artifacts and limitations. |
| `docs/en/validation/M5.md` | Record actual M5 host/QEMU acceptance, resource recovery, inputs, outputs and limitations. |
| `docs/en/validation/M6.md` | Record actual host/guest/serial/two-boot M6 validation, resource recovery and limits. |
| `docs/en/validation/M7.md` | Record actual M7 host/guest/editor/reboot/error checks, integrity evidence and remaining limits. |
| `docs/en/validation/M8.md` | Record actual M8 binaries, host traces, guest cases, regression evidence and limitations. |
| `docs/en/validation/M8_OPTIMIZATION.md` | Record M8 string/stack/I/O improvements, measurable copy counts, boundary tests and regression evidence. |
| `docs/en/validation/M9.md` | Measured M9 program hashes, host-trace boundaries, actual guest results and inherited regression evidence. |
| `docs/zh-CN/ARCHITECTURE.md` | Provide the complete Simplified Chinese counterpart of `docs/en/ARCHITECTURE.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/COMPATIBILITY.md` | Provide the complete Simplified Chinese counterpart of `docs/en/COMPATIBILITY.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/PROJECT_OUTLINE.md` | Provide the complete Simplified Chinese counterpart of `docs/en/PROJECT_OUTLINE.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/ROADMAP.md` | Provide the complete Simplified Chinese counterpart of `docs/en/ROADMAP.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/guides/CODING_STYLE.md` | Provide the complete Simplified Chinese counterpart of `docs/en/guides/CODING_STYLE.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/guides/DEVELOPMENT.md` | Provide the complete Simplified Chinese counterpart of `docs/en/guides/DEVELOPMENT.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/guides/FILE_PURPOSES.md` | Provide the complete Simplified Chinese counterpart of `docs/en/guides/FILE_PURPOSES.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/guides/FIRST_TASK.md` | Provide the complete Simplified Chinese counterpart of `docs/en/guides/FIRST_TASK.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/guides/GITHUB_SETUP.md` | Provide the complete Simplified Chinese counterpart of `docs/en/guides/GITHUB_SETUP.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/guides/LICENSING.md` | Provide the complete Simplified Chinese counterpart of `docs/en/guides/LICENSING.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/guides/REFERENCES.md` | Provide the complete Simplified Chinese counterpart of `docs/en/guides/REFERENCES.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/guides/REQUIREMENTS.md` | Provide the complete Simplified Chinese counterpart of `docs/en/guides/REQUIREMENTS.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/guides/STATUS.md` | Provide the complete Simplified Chinese counterpart of `docs/en/guides/STATUS.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/guides/TOOLCHAINS.md` | Provide the complete Simplified Chinese counterpart of `docs/en/guides/TOOLCHAINS.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/index.md` | Provide the complete Simplified Chinese counterpart of `docs/en/index.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M10_NETWORK_PACKAGES.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M10_NETWORK_PACKAGES.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M11_TOOLCHAIN.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M11_TOOLCHAIN.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M12_NETWORK.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M12_NETWORK.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M12_RUST.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M12_RUST.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M13_SDK.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M13_SDK.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M14_COMPOSITOR.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M14_COMPOSITOR.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M14_GPU.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M14_GPU.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M14_GUI.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M14_GUI.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M14_RUNTIME.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M14_RUNTIME.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M15_BOOTSTRAP.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M15_BOOTSTRAP.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M15_GCC.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M15_GCC.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M15_KERNEL.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M15_KERNEL.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M15_REPEAT.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M15_REPEAT.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M15_RUST.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M15_RUST.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M16_FOUNDATION.md` | Translate the planned M16 resource, VFS, ext4 and migration contract with matching acceptance boundaries. |
| `docs/zh-CN/milestones/M17_DESKTOP.md` | Translate the planned M17 development desktop, graphical workflow and independent recovery contract. |
| `docs/zh-CN/milestones/M2_CPU.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M2_CPU.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M3_MEMORY.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M3_MEMORY.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M4_USERSPACE.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M4_USERSPACE.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M5_PROCESSES.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M5_PROCESSES.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M6_FILESYSTEM.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M6_FILESYSTEM.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M7_STORAGE.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M7_STORAGE.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M8_LINUX.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M8_LINUX.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/milestones/M9_RUNTIME.md` | Provide the complete Simplified Chinese counterpart of `docs/en/milestones/M9_RUNTIME.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/third-party/m10.md` | Provide the complete Simplified Chinese counterpart of `docs/en/third-party/m10.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/third-party/m11.md` | Provide the complete Simplified Chinese counterpart of `docs/en/third-party/m11.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/third-party/m12.md` | Provide the complete Simplified Chinese counterpart of `docs/en/third-party/m12.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/third-party/m15.md` | Provide the complete Simplified Chinese counterpart of `docs/en/third-party/m15.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/third-party/musl.md` | Provide the complete Simplified Chinese counterpart of `docs/en/third-party/musl.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/third-party/network-licenses.md` | Provide the complete Simplified Chinese counterpart of `docs/en/third-party/network-licenses.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M0-M1.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M0-M1.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M10.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M10.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M11.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M11.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M12.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M12.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M13.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M13.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M14.1.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M14.1.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M14.2.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M14.2.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M14.3.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M14.3.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M14.4.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M14.4.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M14.5.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M14.5.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M15.1-M15.2.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M15.1-M15.2.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M15.3.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M15.3.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M15.4.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M15.4.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M15.5.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M15.5.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M15.6.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M15.6.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M16.md` | Provide the synchronized Chinese partial M16 evidence and remaining-stage boundaries. |
| `docs/zh-CN/validation/M2.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M2.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M3.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M3.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M4.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M4.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M5.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M5.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M6.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M6.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M7.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M7.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M8.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M8.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M8_OPTIMIZATION.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M8_OPTIMIZATION.md`, preserving technical scope, commands, evidence and original legal texts. |
| `docs/zh-CN/validation/M9.md` | Provide the complete Simplified Chinese counterpart of `docs/en/validation/M9.md`, preserving technical scope, commands, evidence and original legal texts. |
| `kernel/Cargo.toml` | Build the bare-metal kernel, sampled desktop sessions and opt-in isolated QEMU failure tests. |
| `kernel/build.rs` | Reject host-target kernel builds and select the kernel-only linker script. |
| `kernel/src/abi/mod.rs` | Adapt native system queries; task owns process lifecycle and fs owns descriptor calls. |
| `kernel/src/arch/mod.rs` | Select the implemented x86_64 hardware architecture boundary. |
| `kernel/src/arch/x86_64/acpi.rs` | Read only the firmware tables needed by the q35 BSP interrupt baseline. |
| `kernel/src/arch/x86_64/apic.rs` | BSP xAPIC periodic timer, calibrated against the 3.579545 MHz ACPI PM timer. External I/O APIC lines and legacy PICs remain masked until device drivers exist. |
| `kernel/src/arch/x86_64/clock.rs` | Sample stable binary/BCD CMOS UTC and advance realtime from calibrated monotonic ticks. |
| `kernel/src/arch/x86_64/cpu.rs` | Privileged operations for the single BSP. No FPU/SIMD is enabled here. |
| `kernel/src/arch/x86_64/interrupts.S` | Build uniform saved-GPR frames for CPU traps, IRQs and the native syscall entry; restore the checked frame with IRETQ. Long mode always saves SS:RSP. |
| `kernel/src/arch/x86_64/mod.rs` | Select the x86_64 CPU, paging, interrupt and user-transition implementations. |
| `kernel/src/arch/x86_64/paging.rs` | A bounded BSP-only copy-on-write extension of the retained Limine tables. M3 consumes this mapper and clones every live table into owned frame storage. |
| `kernel/src/arch/x86_64/traps.rs` | Dispatch kernel diagnostics, native user faults, Linux signals, syscalls and checked usercopy recovery with IF=0 and IRETQ. |
| `kernel/src/arch/x86_64/user.S` | Resume saved user GPR/FPU state and use its own trusted SYSCALL stack. Every user trap returns to a single BSP scheduler continuation with IF=0. |
| `kernel/src/arch/x86_64/user.rs` | BSP saved GPR/segment/x87/SSE2 contexts, native SYSCALL and IRETQ transitions. Each user trap returns to the trusted scheduler root/stack; kernel code remains SIMD-free. |
| `kernel/src/boot.rs` | Borrow retained Limine data. M3 allocates only USABLE RAM; bootloader data, original stacks/tables, modules, firmware and the early pool remain reserved. Retained native ELF modules support isolation tests; a tagged archive supplies the M6 root. |
| `kernel/src/drivers/gpu.rs` | Own an optional polled modern virtio-gpu queue, fenced hardware readback, reset and DMA quarantine. |
| `kernel/src/drivers/input.rs` | Own i8042 setup/polling/expiry, hand off exclusive physical events to the admitted user compositor, and retain diagnostic/lifecycle checks. |
| `kernel/src/drivers/mod.rs` | Own BSP device initialization, persistent VFS handoff and optional scheduler-polled PS/2 input. |
| `kernel/src/drivers/network.rs` | Own modern virtio-net split queues, frame/DMA bounds, ownership fences and reset/quarantine. |
| `kernel/src/drivers/pci.rs` | Enumerate PCI bridges/functions and admit bounded modern virtio MMIO capabilities/BARs. |
| `kernel/src/drivers/virtio.rs` | Own a coherent split block queue, checked transfers/FLUSH, timeout, reset and DMA quarantine. |
| `kernel/src/fs/mod.rs` | Bridge checked native user buffers to the shared VFS and polled canonical terminal. Calls run on the BSP at IF=0; blocking retains only a copied syscall frame, never a lock. |
| `kernel/src/fs/tty.rs` | Bounded COM1/four-PTY line disciplines, input/output queues, settings, foreground signals and hangup. |
| `kernel/src/graphics.rs` | Validate display mappings, present/reclaim the boot buffer, manage the diagnostic cursor and copy checked user compositor damage. |
| `kernel/src/gui.rs` | Authenticate executable/server and process/client identities, copy bounded IPC/scanout and clean up lifetimes. |
| `kernel/src/main.rs` | Boot owned CPU/memory/devices, optional checked software graphics, and isolated user workloads. |
| `kernel/src/mm/lock.rs` | Non-sleeping BSP locks. Order: memory manager before heap; the fixed heap never calls the frame allocator. IRQ/NMI/fatal paths must not take either. |
| `kernel/src/mm/mod.rs` | BSP memory ownership: checked USABLE frames, fully owned table hierarchy, guarded stacks, reclaiming buddy heap and private process memory integration. |
| `kernel/src/mm/uaccess.rs` | Copy bounded user bytes with page checks and REP MOVSB fault recovery; IF=0 pins shared mappings and no copy spans a scheduling/blocking boundary. |
| `kernel/src/mm/user.rs` | Own shared user roots and reference-counted private/shared pages, permissions and guarded task stacks. |
| `kernel/src/mm/user/file_mapping.rs` | Pin readonly shared-file mappings and refresh changed VFS bytes with physical-page and fork ownership. |
| `kernel/src/mm/validation.rs` | Runtime invariants and isolated QEMU stress/fault validation. |
| `kernel/src/net/mod.rs` | Integrate bounded smoltcp IPv4 sockets, readiness, fallible buffers and deferred TCP close. |
| `kernel/src/serial.rs` | Single-BSP COM1 output and nonblocking byte input; no heap, locks or firmware. |
| `kernel/src/task/bootstrap.rs` | Sample scheduler-instance frame, heap, task, descriptor and namespace use without allocation or a public ABI. |
| `kernel/src/task/diagnostics.rs` | Copy bounded user frame-pointer chains safely and report captured fault registers and resources. |
| `kernel/src/task/linux.rs` | Dispatch bounded Linux calls including vfork, modes/umask and resource queries; delegate memory, thread, signal and terminal layouts. |
| `kernel/src/task/linux/io.rs` | Import Linux writev descriptors before I/O and preserve completed bytes across later buffer faults. |
| `kernel/src/task/linux/memory.rs` | Translate bounded mmap/brk/munmap/mprotect, real PROT_NONE/W^X, private file backing and shared anonymous pages. |
| `kernel/src/task/linux/select.rs` | Implement checked select descriptor sets, true readiness, finite waits, remaining time and interruptible blocking. |
| `kernel/src/task/linux/signals.rs` | Translate signal actions/masks and addressing, build AMD64 user frames and import checked signal-return state. |
| `kernel/src/task/linux/sockets.rs` | Translate checked Linux IPv4 socket calls, pointer layouts, deadlines and interruptible frames. |
| `kernel/src/task/linux/terminal.rs` | Translate poll/ppoll readiness, terminal ioctls, PTY control and bounded process-group/session interfaces. |
| `kernel/src/task/linux/threads.rs` | Share clone resources, suspend/wake vfork parents and manage futex waits/requeue/deadlines and group/clear-TID lifecycle. |
| `kernel/src/task/linux/time.rs` | Adapt utimensat/futimesat, dirfd/descriptor paths, NOW/OMIT and checked timestamp precision. |
| `kernel/src/task/loader.rs` | Validate and map native/Linux executables and actual musl interpreters, then construct checked startup state. |
| `kernel/src/task/mod.rs` | Schedule bounded native/Linux tasks, suspended vfork parents and shared-root threads; lifecycle, copying and reclamation run on the kernel root. |
| `kernel/src/task/validation.rs` | Verify loader/syscall/fault containment plus transactional process/fork/exec failures. |
| `linker/user-x86_64.ld` | Place native static user text/rodata/data in separate pages below the image limit. |
| `linker/x86_64.ld` | Link the high-half kernel with retained Limine requests and separate RX/R/RW segments. |
| `ports/bootstrap/Makefile.anova` | Compile eight unchanged bzip2 C units sequentially with the guest GCC and test retained-output compiler rejection. |
| `ports/bootstrap/README.md` | Explain the locked source selection, guest recipe and host preparation boundary. |
| `ports/bootstrap/README.zh-CN.md` | Translate locked source recipes, guest reconstruction, compiler prerequisite assessments and host preparation boundaries. |
| `ports/bootstrap/gcc_probe.py` | Verify seven locked compiler dependency archives and publish source inventory plus bounded unchanged configure-startup inputs. |
| `ports/bootstrap/inputs.json` | Lock official bzip2 1.0.8 checksums, source selection, version and licensing metadata. |
| `ports/bootstrap/kernel-build.c` | Run real offline guest Cargo compilation, validate and atomically publish a kernel ELF, and exercise retained-kernel compiler failure. |
| `ports/bootstrap/kernel-inputs.json` | Lock the official Rust 1.90.0 bare-metal target archive and its inherited binary library provenance. |
| `ports/bootstrap/kernel_prepare.py` | Snapshot exact kernel sources, isolate metadata, authenticate offline registry sources and prepare inherited bare-metal target libraries. |
| `ports/bootstrap/prepare.py` | Verify official source archives and safely publish a source-only offline guest workspace. |
| `ports/bootstrap/rust-inputs.json` | Lock the official complete Rust 1.90.0 source archive, published checksum URL and assessed musl target. |
| `ports/bootstrap/rust-probe/.cargo/config.toml` | Restrict the isolated Cargo prerequisite fixture to the inherited offline vendor directory. |
| `ports/bootstrap/rust-probe/bad.rs` | Trigger a real rejected guest Rust compilation while retaining the prior successful seed executable. |
| `ports/bootstrap/rust-probe/cargo-dependency/Cargo.toml` | Pin the curl dependency selected from upstream Cargo as an isolated offline closure-availability probe. |
| `ports/bootstrap/rust-probe/cargo-dependency/src/main.rs` | Supply a minimal application body for the isolated offline Cargo prerequisite probe. |
| `ports/bootstrap/rust-probe/llvm_seed.cpp` | Exercise the inherited guest C++17 seed with a fold expression, std::optional and result 42. |
| `ports/bootstrap/rust-probe/probe.c` | Verify actual missing guest tools and bounded Cargo offline dependency failure with checked child errno/status reporting. |
| `ports/bootstrap/rust-probe/seed.rs` | Exercise the inherited guest Rust std seed with vectors, a worker thread, persistent file bytes and result 42. |
| `ports/bootstrap/rust_probe.py` | Verify the locked official Rust source distribution and publish independent component inventories plus bounded unchanged build inputs and notices. |
| `ports/bootstrap/test_gcc_probe.py` | Reject unsafe or tampered compiler archives, oversized or missing probe inputs and changed or symlinked output caches. |
| `ports/bootstrap/test_kernel_prepare.py` | Reject altered sources, lock drift, unsafe archives, redirected or corrupted publications and excessive guest paths. |
| `ports/bootstrap/test_prepare.py` | Reject tampered archives, unsafe members, missing inputs and selected files beyond the guest limit. |
| `ports/bootstrap/test_rust_probe.py` | Reject changed Rust sources, unsafe archive members and changed or redirected source publications. |
| `ports/gcc/build.py` | Verify archives, cross-build GCC/binutils/make and target runtimes, and verify the complete installed-file cache. |
| `ports/gcc/build.sh` | Enter the source-build recipe from the repository root with strict shell error handling. |
| `ports/gcc/fetch.py` | Acquire locked sources through verified transport fallback/cache admission and atomic publication. |
| `ports/gcc/inputs.json` | Lock compiler bootstrap and upstream source URLs/hashes; JSON carries an explicit purpose field instead of comments. |
| `ports/gcc/package.py` | Select the bounded source-built C/C++ sysroot, preserve licenses and hash every packaged file. |
| `ports/gcc/test_fetch.py` | Verify transport failure, checksum rejection, cache/symlink admission and partial cleanup. |
| `ports/rust/build.py` | Verify archives, isolate strip scratch output and publish the inventoried guest Rust sysroot. |
| `ports/rust/inputs.json` | Lock official Rust musl binary archives and the vendored itoa registry checksum. |
| `rust-toolchain.toml` | Pins the compiler, lint/format tools and bare-metal standard-library artifacts. |
| `rustfmt.toml` | Use stable rustfmt options and the same style in editors and on the command line. |
| `scripts/check_docs.py` | Check bilingual page coverage, reciprocal language navigation, purpose comments and local Markdown files/heading links without network access. |
| `sdk/README.md` | Document the native SDK release, checked 32 KiB startup block, templates, standalone build driver and independent GUI companion. |
| `sdk/README.zh-CN.md` | Translate the native SDK release contract, checked startup limits, independent templates and GUI companion boundary. |
| `sdk/build.py` | Build independent native SDK projects with isolated user flags, offline Rust inputs and explicit ABI metadata. |
| `sdk/gui/README.md` | Specify GUI ABI versions, ownership, errors, linkage and bounded guest SDK usage. |
| `sdk/gui/README.zh-CN.md` | Translate GUI ABI layout, version/capability checks, lifetimes, errors, linkage and actual guest language workflows. |
| `sdk/gui/generate.py` | Generate and verify fixed GUI v1 C layouts, capabilities and function declarations. |
| `sdk/gui/include/anova_gui.h` | Declare generated fixed-width GUI v1 C calls and compile-time layout assertions. |
| `sdk/gui/include/anova_gui.hpp` | Transfer GUI window ownership with exception-free C++ move-only RAII. |
| `sdk/include/anova.hpp` | Wrap native C descriptors with move-safe C++ ownership without STL or exception dependencies. |
| `sdk/native-v1.ld` | Link SDK user images with distinct W^X pages and valid nonempty load segments under GNU ld and rust-lld. |
| `sdk/templates/c/main.c` | Exercise native SDK version negotiation, console I/O and a diagnosed missing-file error in C. |
| `sdk/templates/cpp/main.cpp` | Exercise equivalent native SDK behavior and C++ descriptor ownership. |
| `sdk/templates/rust/Cargo.toml` | Define the exported standalone no_std Rust project with local runtime and offline dependency paths. |
| `sdk/templates/rust/main.rs` | Exercise native typed errors and a local offline dependency without Rust std. |
| `sdk/templates/rust/offline-message/Cargo.toml` | Pin the local first-party dependency used by the offline native Rust template. |
| `sdk/templates/rust/offline-message/src/lib.rs` | Supply a no_std message through the template local Cargo dependency. |
| `sdk/tests/contract.c` | Assert native C/C++ syscall numbers, stable errors, structure sizes, alignments and field offsets. |
| `tests/guest/gui/contract.c` | Run an independent guest-compiled ABI boundary probe with a real compositor. |
| `tests/guest/gui/contract.h` | Exercise real C/C++ version, UTF-8, lifecycle, layout and capacity failure contracts. |
| `tests/guest/gui/fault.c` | Deliberately fault a GUI caller pointer to verify process isolation and cleanup. |
| `tests/guest/gui/main.c` | Compile an independent native C GUI client with physical key and button handling. |
| `tests/guest/gui/main.cpp` | Compile an independent C++ GUI client and exercise move/RAII ownership. |
| `tests/guest/gui/main.rs` | Compile an independent safe Rust GUI client with equivalent visible behavior. |
| `tests/guest/gui/note.rs` | Provide a bounded editable Rust GUI note and explicit private clipboard copying. |
| `tests/guest/gui/terminal.rs` | Run keyboard-entered native commands through a pre-GUI helper and render pipe output. |
| `tests/guest/m10_https_peer.py` | Serve controlled TLS 1.2 positive/expired fixtures using only Python's standard library. |
| `tests/guest/m11/Makefile` | Drive separate C compilation, linking, no-op make and a selective source rebuild inside the guest. |
| `tests/guest/m11/bad.c` | Deliberately fail actual guest C parsing and require a nonzero compiler status. |
| `tests/guest/m11/badlink.c` | Deliberately reference an undefined symbol and require a nonzero guest linker status. |
| `tests/guest/m11/helper.c` | Supply a separately compiled function changed by the editor for selective make rebuild checks. |
| `tests/guest/m11/main.c` | Validate guest C stdio, allocation, arguments, file output and multi-source linking. |
| `tests/guest/m11/main.cpp` | Validate C++ string/vector, RAII, exceptions and a real standard-library thread. |
| `tests/guest/m11/runtime.c` | Probe real vfork/shared-root suspension, deferred signals, snapshots, modes, limits and temporary-file cleanup. |
| `tests/guest/m12/Cargo.lock` | Generated lockfile for the fixed offline guest registry dependency; do not hand-edit. |
| `tests/guest/m12/Cargo.toml` | Define the M12 offline Rust/Cargo acceptance fixture and its declared behavior. |
| `tests/guest/m12/bad.rs` | Deliberately fail guest Rust parsing and verify nonzero status and retained earlier outputs. |
| `tests/guest/m12/build.rs` | Run guest GCC/ar, generate OUT_DIR input and configure Rust FFI linkage. |
| `tests/guest/m12/cargo.rs` | Define the M12 offline Rust/Cargo acceptance fixture and its declared behavior. |
| `tests/guest/m12/config.toml` | Define the M12 offline Rust/Cargo acceptance fixture and its declared behavior. |
| `tests/guest/m12/helper.c` | Provide the C function compiled by guest build.rs and statically linked through Rust FFI. |
| `tests/guest/m12/macro-support/Cargo.toml` | Define the shared host dependency for the macro and its build script. |
| `tests/guest/m12/macro-support/src/lib.rs` | Supply the edited value used to verify transitive Cargo rebuilds. |
| `tests/guest/m12/macro.rs` | Export a real procedural macro compiled into a guest shared library. |
| `tests/guest/m12/macro/Cargo.toml` | Define the Cargo procedural macro and its runtime/build path dependency. |
| `tests/guest/m12/macro/build.rs` | Generate macro input and environment from the host support crate. |
| `tests/guest/m12/macro/src/lib.rs` | Expand the Cargo macro from generated input and reject unexpected tokens. |
| `tests/guest/m12/main.rs` | Define the M12 offline Rust/Cargo acceptance fixture and its declared behavior. |
| `tests/guest/m12/online/Cargo.toml.in` | Instantiate the online guest project with an exact registry version and the generated full Git revision. |
| `tests/guest/m12/online/main.rs` | Run separately downloaded registry and Git dependencies in the guest-built application. |
| `tests/guest/m12/plain/Cargo.lock` | Cargo-generated lock for the dependency-free guest project; do not hand-edit. |
| `tests/guest/m12/plain/Cargo.toml` | Define an independent dependency-free offline Cargo acceptance project. |
| `tests/guest/m12/plain/src/main.rs` | Run an actual guest-built dependency-free Cargo executable and check its output. |
| `tests/guest/m12/public/Cargo.toml` | Pin independent public registry and Git copies of itoa without vendor replacement. |
| `tests/guest/m12/public/main.rs` | Run public dependencies linked into the actual guest-built executable. |
| `tests/guest/m12/runtime.c` | Exercise invalid return/detached exit, alternate signals, positioned/resized files, persistent/tmpfs timestamp precision and OFD locks in a real guest. |
| `tests/guest/m12/use_macro.rs` | Require actual loading/expansion of the guest-built macro and validate its result. |
| `tests/guest/m12_online_peer.py` | Serve deterministic source archives and revision-locked Git smart HTTP with real TLS trust/name/date and transfer-failure fixtures. |
| `tests/guest/m13/crash.c` | Trigger real native user faults through named stack frames and reject a corrupted frame pointer. |
| `tests/guest/m15/capacity.c` | Probe persistent file, descriptor and task limits, resource recovery and preserved file bytes. |
| `tests/guest/m15/gcc_probe.c` | Diagnose actual missing build-tool exec errors and bounded upstream configure startup failure in the guest. |
| `tests/guest/m15/gcc_seed.cpp` | Exercise C++11 compilation with the inherited guest seed without reconstructing a compiler. |
| `tests/guest/m15/rejected.c` | Trigger a deliberate real guest compiler error while retaining the prior reconstructed tool. |
| `user/Cargo.lock` | Generated locked native SDK/application dependency graph; never insert manual comments. |
| `user/Cargo.toml` | Keeps native user ELF builds separate from the bare-metal kernel's target flags. |
| `user/build.rs` | Rebuild native app binaries when their external user-only linker script changes. |
| `user/c/anova.h` | Define native v1 console/process wire layouts and C syscall wrappers without libc/Linux. |
| `user/c/crt.S` | Convert the native startup stack to C main arguments and exit through SYSCALL. |
| `user/c/hello.c` | Demonstrate the C SDK, arguments, BSS/SSE2, scheduling and descriptor calls before exit 7. |
| `user/compositor/Cargo.toml` | Build the isolated Rust native software window compositor. |
| `user/compositor/src/gpu.rs` | Exercise the server-only GPU API, guarded readback comparisons and user-selected CPU fallback. |
| `user/compositor/src/main.rs` | Receive authenticated packets, compose private surfaces and route hardware input in user space. |
| `user/editor/Cargo.toml` | Build the standalone native serial source editor with the no_std SDK. |
| `user/editor/src/main.rs` | Open/edit bounded ASCII text, retain failed-save buffers and fsync successful saves. |
| `user/fs-test/Cargo.toml` | Build the standalone native fs-test application with the shared no_std runtime. |
| `user/fs-test/src/disk.rs` | Exercise real ext2 fd/indirect/sparse/unlink syscalls and verify persisted source/ELF data. |
| `user/fs-test/src/foundation.rs` | Verify actual readonly ext4 directory/file syscalls, sparse zeros and mutation rejection in the guest. |
| `user/fs-test/src/main.rs` | Exercise real VFS syscalls, inherited open offsets, exec/cwd, user buffers and blocking pipes. Child modes validate resources after fork/exec; shell-check reads files produced by its script. |
| `user/gui-client/Cargo.toml` | Build independent native window clients and adversarial transport fixtures. |
| `user/gui-client/src/main.rs` | Upload private red/blue surfaces, handle focused input and test crash/queue/ownership boundaries. |
| `user/gui-common/Cargo.toml` | Share internal native GUI transport helpers across the compositor and test programs. |
| `user/gui-common/src/lib.rs` | Wrap synchronous copied GUI packets and preserve explicit pointer safety contracts. |
| `user/gui-link.rs` | Link independent guest client objects with the Rust GUI library, native CRT and panic boundary. |
| `user/gui-run/Cargo.toml` | Package the native compositor/client lifecycle supervisor for guest SDK applications. |
| `user/gui-run/src/main.rs` | Launch isolated GUI processes and require actual expected wait statuses and reaping. |
| `user/gui-test/Cargo.toml` | Build the native process supervisor for actual GUI lifecycle acceptance. |
| `user/gui-test/src/main.rs` | Spawn/reap isolated windows and compositor sessions, test quotas and retain COM1 recovery. |
| `user/hello/Cargo.toml` | Builds the Rust hello ELF that verifies startup arguments, zeroed BSS and SSE2. |
| `user/hello/src/main.rs` | Independent Rust ELF showing argc/argv, BSS and CPU state through native syscalls. |
| `user/init/Cargo.toml` | Build the standalone native init application with the shared no_std runtime. |
| `user/init/src/main.rs` | Start the native shell, wait for child status and restart interactive sessions after exit. Test mode first runs the independent filesystem suite and then a shell script. |
| `user/linux/busybox.config` | Select the pinned static BusyBox applet profile from allnoconfig defaults. |
| `user/linux/cc.c` | Set the controlled offline compiler/make environment and exec the real guest tools. |
| `user/linux/dns.c` | Resolve bounded DNS A replies with checked compression, randomized IDs, source validation and timeouts. |
| `user/linux/dns.h` | Share the guest DNS resolver and checked reply-parser interfaces. |
| `user/linux/gui-build.c` | Run real guest GCC/G++/rustc and publish native GUI executables only after successful linking. |
| `user/linux/m9.c` | Independent musl static/PIE/dynamic Ring 3 fixture for threading, signals, PTYs, exec/fault/exit negatives and UART interruption. |
| `user/linux/musl.c` | Verify unmodified static musl startup, compiler TLS, allocation, errno and actual file I/O. |
| `user/linux/network.c` | Execute real Ring 3 TCP/UDP/DNS, timeout, pointer, signal and fd-reuse probes. |
| `user/linux/pkg.c` | Install/check native packages, validate hashes/ABI/license and maintain persistent alternating commits. |
| `user/linux/probe.S` | Pass the real Linux startup stack to the freestanding ABI probe with SysV alignment. |
| `user/linux/probe.c` | Exercise Linux stack, errno, memory, TLS, fork/exec/env/wait, CLOEXEC, SIGPIPE, random and sleep. |
| `user/linux/rust.c` | Configure offline Rust/Cargo execution and the isolated guest sysroot and linker. |
| `user/linux/shared.c` | Initialized shared-object TLS used by startup linking and dlopen after worker threads already exist. |
| `user/linux/tls.c` | Fetch bounded HTTPS with mandatory CA/name/date verification and exact negative-test reasons. |
| `user/linux/tls.h` | Share the strict user-space HTTPS acquisition interface. |
| `user/probe/Cargo.toml` | Builds the native negative-case ELF used by the QEMU isolation acceptance gate. |
| `user/probe/src/main.rs` | Trigger real Ring 3 syscall errors, protection faults and unsafe-return attempts. The kernel acceptance runner supplies one named case and verifies its outcome. |
| `user/process/Cargo.toml` | Builds the real native multiprocess workload used for scheduling/lifecycle acceptance. |
| `user/process/src/context.S` | Keep distinct GPR, all XMM lanes, x87 stack/control and MXCSR sentinels live across syscall-free timer preemption and optional eager fork, then verify them. |
| `user/process/src/main.rs` | Exercise real scheduling, blocking, fork/exec, parent waits and descriptor lifetimes. Distinct CPU sentinels survive syscall-free loops; intentional child faults stay isolated. |
| `user/rt/Cargo.toml` | Packages the no_std startup entry and native v1 syscall wrappers for Rust apps. |
| `user/rt/src/lib.rs` | Native Rust CRT, startup arguments and checked process/file syscall wrappers. Built with the x86_64 user ABI and SSE2, without libc, allocation or Linux calls. |
| `user/rt/src/memory.rs` | Supply freestanding compiler memory intrinsics without linking host libc. |
| `user/rt/src/v1.rs` | Provide additive native v1 typed errors and owned descriptor wrappers while retaining raw interfaces. |
| `user/shell/Cargo.toml` | Build the standalone native shell application with the shared no_std runtime. |
| `user/shell/src/lib.rs` | Parse a bounded shell subset: four stages, quotes/escapes and per-stage redirection. Commands receive one joined argument string under the existing native v1 exec contract. |
| `user/shell/src/main.rs` | Run the native shell and small multicall tools with fork/exec/wait, pipes and redirection. Keep all parsing, command policy and line assembly in user space, without a libc or allocator. |
| `xtask/Cargo.toml` | Builds host-side dependency, native-app, image, QEMU and evidence commands. |
| `xtask/src/app.rs` | Validate external package hashes, build application images, launch isolated disks and verify the public SDK installation workflow. |
| `xtask/src/apps.rs` | Build standalone native Rust/C ELF files with user-only target/linker flags. Validate each output and record its explicit ABI identity and SHA-256 manifest. |
| `xtask/src/assets.rs` | Fetch, checksum and locally extract locked bootloader/firmware inputs. |
| `xtask/src/bootstrap.rs` | Prepare a locked bzip2 work disk and verify guest compilation, independent compression checks, limits and reboot recovery. |
| `xtask/src/bootstrap_gcc.rs` | Audit GCC source capacity and verify actual guest seed, header/tool/configure rejection and reboot evidence without claiming compiler reconstruction. |
| `xtask/src/bootstrap_kernel.rs` | Construct the kernel in guest processes, preserve and verify its exact ELF in a host-packaged ISO, then check boot identity and failure recovery. |
| `xtask/src/bootstrap_repeat.rs` | Repeat clean guest bzip2/kernel builds, retain independent artifacts and compare exact inputs, bytes and recovery evidence. |
| `xtask/src/bootstrap_rust.rs` | Audit independent Rust/LLVM/Cargo source capacity and verify real guest seed, prerequisite failures and reboot evidence without claiming component reconstruction. |
| `xtask/src/debug.rs` | Resolve captured guest fault PCs against exact ELF symbols and verify bounded real guest crash traces. |
| `xtask/src/disk.rs` | Create safe native/Linux ext2 profiles and drive watchdog-bounded reboot/integrity/device-error scenarios. |
| `xtask/src/foundation.rs` | Create/retain controlled ext4 images and run four QEMU readonly/reboot/rejection gates with source/artifact hashes. |
| `xtask/src/gpu.rs` | Gate exact-device hardware rendering, full scanout comparisons, fault/reset recovery and headless CPU fallback. |
| `xtask/src/graphics.rs` | Verify actual graphics/fallback and share bounded Unix/loopback-TCP QMP transactions with desktop acceptance. |
| `xtask/src/gui.rs` | Inject actual QMP input and compare native windows, failure recovery and every scanout pixel. |
| `xtask/src/gui_sdk.rs` | Verify guest edits/builds, physical GUI input, literal scanout, faults and retained reboot hashes. |
| `xtask/src/gui_sdk_build.rs` | Export a checksummed GUI source SDK with both language guides and package its guest compiler/display image. |
| `xtask/src/input.rs` | Inject QMP keyboard/mouse sequences, compare all cursor/background pixels and require missing-device serial recovery and clean shutdown. |
| `xtask/src/linux.rs` | Build and hash pinned static/dynamic musl/BusyBox fixtures, package upstream sources and record bounded Linux-host traces. |
| `xtask/src/main.rs` | Host CLI for reproducible kernel/app builds, ISO assembly and QEMU validation. |
| `xtask/src/network.rs` | Run controlled loopback peers and watchdog-driven real QEMU network acceptance. |
| `xtask/src/packages.rs` | Pin/build user TLS clients, construct locked packages and verify install/reboot/HTTPS behavior. |
| `xtask/src/purposes.rs` | Check file-purpose inventory and first-party headers for existing tracked/untracked files, ignoring cached paths removed during a move; human review verifies responsibility accuracy. |
| `xtask/src/rust.rs` | Build Rust images, exercise real guest compilation and independently inspect rebooted source and ELF files. |
| `xtask/src/rust_online.rs` | Verify real guest HTTPS sparse-registry/Git acquisition, strict transport failures and offline reboot cache reuse. |
| `xtask/src/rust_public.rs` | Verify public crates.io/GitHub source acquisition, pinned revisions, actual guest compilation and offline reboot reuse. |
| `xtask/src/sdk.rs` | Export the native SDK with both language guides and create/build isolated C/C++/offline Rust projects. |
| `xtask/src/smoke.rs` | Require real CPU/memory/user/process evidence together with the QEMU exit result. |
| `xtask/src/terminal.rs` | Drive ordinary UART editing/EOF, dynamic musl launch, real Ctrl+C interruption and volatile reboot checks. |
| `xtask/src/toolchain.rs` | Build/package the optional development ISO and verify guest editing, compilation, make, failures and reboot hashes. |
| `xtask/src/transcript.rs` | Capture ordered guest stdout, preserve UART evidence and match raw bytes independently of filesystem snapshots or partial UTF-8. |

| `crates/storage/src/ext4/audit.rs` | Audit writable bitmap/counter, global block ownership and rooted directory/link invariants. |
| `crates/storage/src/ext4/directory.rs` | Stage ext4 directory growth, rename replacement/moves, parent links and orphan namespace changes. |
| `crates/storage/src/ext4/write.rs` | Stage bounded ext4 allocation/data/extent mutations, fail closed on publication errors and flush clean shutdown. |
| `crates/storage/tests/ext4_write.rs` | Verify real writable ext4 images, exhaustive preflight allocation failures, ENOSPC, metadata and I/O boundaries. |
| `user/linux/ext4_write.c` | Exercise actual musl ext4 writes, directory/cwd moves, orphan/mapping/fork lifetimes, metadata and disk-full reuse. |
| `xtask/src/foundation_write.rs` | Create opt-in ext4 work images and retain six QEMU write/build/reboot/virtio-failure sessions and host checks. |
| `docs/en/validation/M16.4.md` | Specify the controlled ext4 write-v1 contract, failures and independently checked local evidence. |
| `docs/zh-CN/validation/M16.4.md` | Translate the controlled ext4 write-v1 contract and exact local acceptance boundaries. |
| `crates/storage/src/ext4/journal.rs` | Implement bounded checksum-v3 JBD2 commit/replay/revoke and persistent orphan recovery. |
| `crates/storage/tests/journal.rs` | Verify sector/flush/half-sector power cuts, actual allocation failure and independent e2fsck interoperability. |
| `scripts/reconstruct_journal_cut.py` | Reconstruct any retained journal fault from an immutable baseline and exact sector event trace. |
| `xtask/src/foundation_journal.rs` | Create fresh JBD2 images and retain host matrix plus real guest write/power-cut/recovery evidence. |
| `xtask/src/foundation_migrate.rs` | Copy readonly ext2 sources to new JBD2 disks and verify actual builds, interruption and UART budgets. |
| `tests/guest/m16/workload.c` | Supervise actual guest make/GCC, parallel storage writes and UART command/save/log/interruption probes. |
| `docs/en/validation/M16.5.md` | Specify the bounded JBD2 profile, ordering, failure/replay and actual local acceptance. |
| `docs/zh-CN/validation/M16.5.md` | Translate the JBD2 profile, fault models, evidence and exact recovery limits. |
| `docs/en/validation/M16.6.md` | Specify fresh-disk migration, real build closure, foreground budgets and exact local evidence. |
| `docs/zh-CN/validation/M16.6.md` | Translate migration ownership, actual build/UART acceptance and independent remaining work. |

| `xtask/src/foundation_capacity.rs` | Accept separate ext4 kernel-build budgets, pressure recovery and independently checked allocation peaks. |
| `tests/guest/m16/capacity.c` | Exhaust pinned VFS directory slots twice and verify rejected creation, removal and reuse. |
| `docs/en/validation/M16_CAPACITY.md` | Record exact ext4 kernel capacity, recovery and remote CI execution evidence. |
| `docs/zh-CN/validation/M16_CAPACITY.md` | Translate the ext4 kernel capacity and remote CI closeout record with matching boundaries. |
| `docs/assets/validation/M16_CI.json` | Retain exact remote run/job/step observations, input failures, repairs and artifact identities. |
| `docs/assets/validation/M16_CORE_CI54.json` | Preserve digest-verified core CI artifact result records, exact source identity and bounded accepted scope. |
| `docs/assets/validation/M16_CAPACITY_LOCAL.json` | Preserve accepted local ext4 kernel capacity, budgets, complete provenance and digest-verified raw logs. |
| `docs/assets/validation/M16_CORE_CI57.json` | Retain verified current-code core CI artifact results, source identity and artifact digest. |
| `crates/abi/src/process.rs` | Define the separate versioned direct-child supervision contract and reject invalid versions. |
| `kernel/src/task/process_control.rs` | Authorize direct-child polling and stopping, preserve status on copy failure and reuse kernel cleanup. |
| `user/rt/src/process.rs` | Wrap experimental nonblocking child polling and identity-checked stop without changing native v1. |
| `user/desktop/Cargo.toml` | Build the Rust desktop session supervisor against native process and GUI interfaces. |
| `user/desktop/src/main.rs` | Own admitted client/compositor lifetimes, bounded session replacement and graphical launcher actions. |
| `xtask/src/desktop.rs` | Build desktop inputs, explicit Linux boundary/PTY shell admissions and host-built C/C++/Rust clients in opt-in images. |
| `docs/en/validation/M17.1.md` | Record accepted local desktop input/lifecycle/reboot evidence, budgets, original failure and remaining scope. |
| `docs/zh-CN/validation/M17.1.md` | Translate accepted desktop scope, process boundary, measurements, failure evidence and remaining work. |
| `tests/guest/desktop/linux-park.S` | Execute real Linux nanosleep/exit to verify native supervision rejects live Linux children. |
| `user/desktop/src/checks.rs` | Exhaust and reuse real child/task/FD resources while checking nonchild and Linux boundaries. |
| `xtask/src/desktop_test.rs` | Drive five desktop boots, physical input/scanout, actual faults, bounded recovery and exact persisted bytes. |
| `docs/assets/validation/M17_DESKTOP_LOCAL.json` | Retain desktop source/input identities, predeclared budgets, raw UART/QMP/failure logs and compressed exact screenshots; JSON uses its purpose field instead of a comment. |
| `crates/abi/src/pty.rs` | Define the separate versioned native PTY wire contract, ownership and checked transfer/geometry bounds. |
| `crates/terminal/Cargo.toml` | Package the allocation-free terminal cell/history model for native and host tests. |
| `crates/terminal/src/lib.rs` | Parse fragmented bounded ANSI, clip/resize cells and retain a fixed 256-line history. |
| `crates/terminal/src/tests.rs` | Verify cursor/colors/erasure, fragmented controls, history bounds, resize and fresh-producer recovery. |
| `kernel/src/task/pty.rs` | Own private PTY masters, exact creator handles, admitted Linux families, failed-copy retention and reclamation snapshots. |
| `user/rt/src/pty.rs` | Wrap the experimental native PTY session with bounded nonblocking transfers and owned cleanup. |
| `user/terminal/Cargo.toml` | Build the native graphical PTY client separately from its Linux shell and compiler inputs. |
| `user/terminal/src/main.rs` | Run bounded GUI/PTY input/output turns, history, visible shell status and explicit restart/close. |
| `user/terminal/src/window.rs` | Rasterize immutable private frames and pump acknowledged GUI chunks without blocking terminal input. |
| `user/terminal/src/checks.rs` | Exercise real PTY/creator/stale/copy/status/task/global-description boundaries and repeated recovery. |
| `tests/guest/terminal/fixture.c` | Generate actual stdout/stderr, stopped-child, SIGWINCH/output, interrupt and detached-family workloads. |
| `tests/guest/terminal/hello.c` | Provide the exact C source compiled and rerun through physical graphical-terminal input. |
| `xtask/src/pty_terminal.rs` | Admit the native terminal, separately pinned interactive ash and optional verified M11 toolchain. |
| `xtask/src/pty_terminal_test.rs` | Drive graphical PTY/GCC input, real scanout, interruption/resize/history/pressure and exact reboot dumps. |
| `docs/en/milestones/M17_PTY.md` | Define PTY admission/ownership, async frame pumping, ANSI bounds and shell controls. |
| `docs/zh-CN/milestones/M17_PTY.md` | Translate the implemented PTY interface, executable policy, limits and controls. |
| `docs/en/validation/M17.2.md` | Record actual PTY/GCC/reboot acceptance, fixed budgets, retained bytes and failure corrections. |
| `docs/zh-CN/validation/M17.2.md` | Translate PTY acceptance scope, measurements, recovery proof and remaining work. |
| `docs/assets/validation/M17_PTY_LOCAL.json` | Retain exact PTY source/tool/input identities, raw successful/failure logs and compressed genuine scanouts; purpose is a JSON field. |
