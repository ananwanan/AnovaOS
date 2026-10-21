# Development language and Rust formatting

These rules apply to first-party code and contributions on `dev`. Read
[AGENTS.md](../AGENTS.md) for kernel, ABI, safety, and validation requirements.

## Language

Use English for:

- Identifiers, module names, and newly created source filenames.
- Source comments, Rustdoc (`///` and `//!`), examples, and API documentation.
- `SAFETY:`, `TODO:`, and `FIXME:` explanations, including `# Safety` sections for
  unsafe APIs. Explain the actual invariant or reason rather than restating code.
- Logs, diagnostics, error and panic messages, assertion messages, and CLI help.
- Test names, descriptions, and validation markers.
- Commit messages, pull request titles and descriptions, and new development
  documentation.

Localized resources, translation files, quoted external text, and test fixtures
that intentionally exercise other languages may retain their original language.
Preserve third-party source, copyright notices, and license text.

[README.md](../README.md) is the default English entry point.
[README.zh-CN.md](../README.zh-CN.md) is its Simplified Chinese translation. Update
both in the same change when shared facts, links, commands, or project status
change. Existing Chinese planning documents may be maintained in their current
language until translated; this does not relax the English rule for code,
comments, Rustdoc, or developer-facing messages.

## File purpose (mandatory)

Every new or modified first-party file starts with a short English explanation
of what it does and the boundary it owns. Rust files use `//!` module Rustdoc;
C/assembly/linker/configuration/workflow files use their supported comment syntax.
Markdown documents explain their scope after the title. Update the explanation
when the file's responsibility changes. See [FILE_PURPOSES.md](FILE_PURPOSES.md)
for the repository inventory and generated/non-commentable file exceptions.
Preserve upstream notices and generated lockfiles without inserting manual headers.
Run `cargo xtask check-files`; [DEV-001](REQUIREMENTS.md) makes this a delivery requirement.

## Rust formatting

The root [rustfmt.toml](../rustfmt.toml) is the formatting authority. It plays the
same role for Rust source that `.clang-format` plays for C/C++ source.

| Setting | Project convention |
| --- | --- |
| Parser edition | Rust 2024; first-party Cargo packages must also declare `edition = "2024"` |
| Style edition | 2024, explicitly configured for consistent editor and CLI output |
| Indentation | Four spaces; no hard tabs |
| Preferred line width | 100 columns, subject to rustfmt's handling of comments and literals |
| Line endings | LF (`newline_style = "Unix"`) |
| Imports and modules | Let rustfmt reorder declarations where supported |
| Shorthand | Prefer field initialization shorthand and `?` over `try!` |
| Other layout decisions | Rustfmt defaults and default small-item heuristics |

Only stable configuration options are enabled. Do not add nightly-only settings,
nested formatter configs, or blanket `#[rustfmt::skip]` attributes without a
concrete reason. Do not reformat vendored or generated code merely to match
first-party conventions; keep it outside first-party formatting targets and
retain any required upstream layout.

During M0, pin a tested Rust toolchain in `rust-toolchain.toml` and include the
`rustfmt` and `clippy` components. Record the exact Rust/rustfmt versions used.
Use that toolchain for local formatting and future CI; do not select a floating
nightly just to change layout. Rustfmt does not check that comments are in
English; language and safety explanations require review.

### Workspace formatting

Run from the repository root with the pinned toolchain:

```sh
# Format all first-party workspace packages.
cargo fmt --all

# Check formatting without rewriting files.
cargo fmt --all -- --check
```

The workspace and pinned toolchain are now present, so these commands run from
the repository root.
Clippy, builds, and host/guest tests remain separate checks selected for each
target; formatting alone does not validate kernel behavior.

### Editor setup

Use rust-analyzer's rustfmt integration and enable format on save if desired.
Keep the formatter on the project toolchain and allow it to discover the root
configuration; avoid editor-specific layout overrides that disagree with it.

## Official references

- [Rustfmt configuration options](https://rust-lang.github.io/rustfmt/)
- [Rustfmt configuration and usage](https://github.com/rust-lang/rustfmt#configuring-rustfmt)
- [Rust 2024 style editions](https://doc.rust-lang.org/edition-guide/rust-2024/rustfmt-style-edition.html)
