<!-- Record owner-approved, mandatory project development requirements and acceptance constraints. -->
# Development requirements

This document records mandatory development requirements added by the project
owner. Architecture and milestone acceptance remain in [AGENTS.md](../AGENTS.md)
and [ROADMAP.md](../ROADMAP.md).

## DEV-001: explain every file's purpose

Added 2026-10-21. Every new or modified first-party file must clearly explain
what it does. This applies to all subsequent development, including fixes,
configuration, SDK examples, tests and documentation.

- Place a concise English purpose explanation near the top. Rust uses `//!`;
  other source/configuration formats use their supported comment syntax.
- Explain responsibility and relevant boundaries; keep the explanation current
  when behavior or ownership changes.
- Maintain the per-file inventory in [FILE_PURPOSES.md](FILE_PURPOSES.md).
- Generated files, binaries and comment-free formats use an inventory/adjacent
  README entry. Preserve generated lockfiles and original third-party/legal text.
- Run `cargo xtask check-files` before delivery; CI runs the same coverage check.
  This check establishes coverage, while review establishes explanation quality.

The M4 change also inventories the existing repository and fills missing source
and configuration purpose headers, so the rule starts from a covered baseline.

## DEV-002: Rust GUI architecture, three language SDKs, five M14 gates

Added 2026-10-22 by project-owner request. This records the architecture
decision and planning requirements, **not** a claim of GUI implementation.
The detailed design is [M14_GUI](M14_GUI.md); implementation gates are in
[ROADMAP](../ROADMAP.md) and mandatory development rules in
[AGENTS](../AGENTS.md).

- Implement the native compositor, rendering/runtime and desktop userland in
  Rust; compositor and widget/window policy run in user processes, not kernel.
- Expose C, C++ and Rust GUI SDKs from one API/semantic contract. Implement
  the stable interoperability boundary as a versioned C ABI in Rust, with
  C headers, C++ RAII wrappers and idiomatic safe Rust wrappers.
- Define unambiguous ownership, checked layouts, opaque handles, version
  negotiation, errors, event/thread semantics, user IPC isolation and negative
  tests. No Rust panic or C++ exception may unwind through the C ABI.
- Deliver **M14.1** checked framebuffer/software draw; **M14.2** input event
  path; **M14.3** user-space compositor/multiple windows; **M14.4**
  GUI runtime + three independently tested guest SDK examples; **M14.5**
  optional genuine GPU rendering with tested software fallback.
- Explicitly keep external Linux GUI frameworks, hardware input,
  multi-core work, and Vulkan/OpenGL outside those gates unless separately
  scoped and validated. A framebuffer or scanout alone does not prove
  accelerated graphics.
- Update status and per-stage evidence when future work is done; never mark
  a milestone complete based on documentation or an untested demo.
