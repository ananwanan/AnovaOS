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
