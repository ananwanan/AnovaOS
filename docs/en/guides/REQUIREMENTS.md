<!-- Record owner-approved file-purpose, GUI architecture and timely bilingual documentation requirements. -->
# Development requirements

**English** | [简体中文](../../zh-CN/guides/REQUIREMENTS.md)

These are mandatory requirements requested by the project owner. Architecture and acceptance
contracts are in [AGENTS](../../../AGENTS.md) and [ROADMAP](../ROADMAP.md).

## DEV-001: explain every file's purpose

Added 2026-10-21. Every new or modified first-party file must explain its responsibility and
relevant boundaries near the top. This covers implementation, fixes, configuration, examples,
tests and documentation, and applies to subsequent development.

- Write the purpose comment in English: Rust uses `//!`; other formats use supported comments.
- Update the explanation when behavior or ownership changes and maintain [FILE_PURPOSES](FILE_PURPOSES.md).
- Generated files, binaries and comment-free formats use the inventory or an adjacent README.
  Preserve generated lockfiles and original legal/third-party text.
- Run `cargo xtask check-files` before delivery; CI runs the same coverage check. Human review
  still determines whether explanations are accurate. The M4 change established coverage
  for pre-existing files and filled missing source/configuration headers.

## DEV-002: Rust GUI, three language SDKs and five M14 gates

Added 2026-10-22. This records the owner's architecture decision and acceptance requirements;
actual completed scope is reported in [STATUS](STATUS.md) and individual validation records.
See [M14 GUI](../milestones/M14_GUI.md), [ROADMAP](../ROADMAP.md) and [AGENTS](../../../AGENTS.md).

- Implement native rendering, compositor, runtime and desktop userland in Rust. Window/widget
  policy and the compositor run in user processes; the kernel owns safe device/IPC mechanisms.
- Expose C, C++ and Rust SDKs through one semantic contract and a Rust-implemented versioned
  C ABI: C headers, C++ RAII and idiomatic safe Rust wrappers.
- Define checked layouts, ownership, opaque handles, versions, errors, thread/event behavior,
  client IPC isolation and negative tests. Rust panics and C++ exceptions must not unwind across FFI.
- Accept separately: M14.1 checked framebuffer/software drawing; M14.2 input; M14.3 user-space
  multiwindow composition; M14.4 runtime and independent guest C/C++/Rust demos; M14.5 optional
  actual GPU rendering with tested CPU fallback.
- Linux GUI frameworks, additional hardware input, SMP and general Vulkan/OpenGL are separate
  profiles. A framebuffer or scanout is not acceleration. The accepted M14.5 profile is
  bounded hardware clear/readback; desktop composition remains software-rendered.
- Update status and per-stage evidence after implementation. Documentation and untested demos
  do not establish completion.

## DEV-003: update development documentation with every delivery

Added 2026-10-25 by project-owner request. Keep documentation current as development proceeds.

- Update both README files, affected `docs/en` and mirrored `docs/zh-CN` pages, STATUS, ROADMAP,
  architecture/ABI contracts, command descriptions and validation records in the same delivery
  as the corresponding implementation or configuration change.
- Replace completed "next steps" with the first remaining tasks. Preserve measured limitations,
  blockers, historical evidence and the distinction between implementation, local acceptance
  and remote CI.
- Keep language variants aligned; retain identifiers, paths, commands and legal text accurately.
  Check local links and file inventory after moves or additions. Documentation consistency is
  a required completion check; do not claim tests that were not run.

## DEV-004: resources and ext4 before the development desktop

Added 2026-10-26 by project-owner request. This records planned work, not implementation.

- Follow M16 resources/VFS/ext4, then M17 development desktop, then conditional M18 compiler
  reconstruction and guest-only images. Detailed stages and exits are in the
  [roadmap](../ROADMAP.md), [M16](../milestones/M16_FOUNDATION.md) and
  [M17](../milestones/M17_DESKTOP.md).
- Restore verified CI dependency acquisition and measure a declared real build before choosing
  capacities. Expand VFS/cache/heap/task resources independently of the disk format.
- Introduce controlled ext4 read-only, writes and JBD2 recovery in separate accepted stages.
  Retain ext2 images/recovery; migrate by copying and checking a fresh image, without an
  in-place conversion. A new AnovaFS format is outside the development critical path.
- Use existing guest GCC/G++ and Rust/Cargo for a real graphical PTY terminal, editing/files,
  logs/resource inspection and the all-GUI edit/build/run/error-repair/reboot workflow.
- Do not make full compiler bootstrap, native std, SMP or physical drivers prerequisites for
  the developer desktop. Retain COM1/QEMU/host recovery for kernel, boot or display failures.
- Keep every new stage planned until its actual positive, pressure, failure and recovery
  checks pass; preserve prior gates and distinguish local acceptance from remote CI.

## DEV-004 scheduling update (2026-10-27)

The owner explicitly defers M16 remote CI closeout and authorizes starting M17. Retain M16 pending status and main-push-only Actions. M17 may proceed on the locally accepted foundation; every M17 acceptance requirement remains applicable.
