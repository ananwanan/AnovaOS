<!-- Explain the released native SDK contract, offline templates and separate future GUI ABI delivery. -->
# AnovaOS SDK 1.0.0

The released SDK targets `anova.native.v1` on little-endian x86_64. Its wire
version remains 1 independently of the SDK release number. Existing syscall
numbers, negative errors and public structure layouts are frozen. Additive SDK
wrappers may share v1; changed wire semantics require a new personality and
independent compatibility tests. Unknown negative errors retain their numeric
value and use `unknown_native_error` as their diagnostic name.

Native startup allows up to 64 argv entries and 1023 bytes per string; the
combined strings and pointer table must fit the loader's 4096-byte startup page.
The runtime argument iterator accepts those current bounds and rejects excessive
counts or unterminated strings without silently truncating them.

Run `cargo xtask sdk` to export a self-contained SDK to
`build/sdk/anova-sdk-1.0.0`, including CRT, linker script, C/C++ headers, Rust
sources, three templates and SHA-256 inventory. `cargo xtask app-new c
build/my-app` creates a project with its own SDK snapshot. Replace `c` with
`cpp` or `rust` for the other templates, then run
`cargo xtask app-build build/my-app`. The output is `app.elf` plus a checked
`app.toml` package manifest. Project versions are positive upgrade integers in
`project.toml`; SDK release versions are separate.

All templates use native syscall wrappers and a normal x86_64 user code model,
red zone and SSE2. They have no libc, C++ standard library, exception runtime,
allocator, Rust `std` or Linux ABI dependency. C++ demonstrates scoped resource
ownership. Rust demonstrates typed errors and an actual local dependency;
Cargo builds use `--offline --locked` after generating the local lock once.
Builds retain debug information, ELF symbols and frame pointers for the M13
diagnostic tools. Host GCC/G++, Rust 1.90.0 and rust-lld are prerequisites.

The export contains a Python build driver for independent host use:
`python3 SDK/build.py c /absolute/project` (or `cpp`/`rust`). This emits the same native
ELF and ABI trailer; `app-build` additionally validates the ELF and creates
the package manifest. A release is source-distributed and checksummed; the
checksum inventory does not provide publisher authentication.

The exported Rust template includes its complete Cargo manifest and an
offline-generated lockfile. Copy the release into a fresh consumer directory
before building a standalone template so generated artifacts stay outside the
immutable SDK export:

```sh
cp -a build/sdk/anova-sdk-1.0.0 build/sdk/standalone-consumer
python3 build/sdk/standalone-consumer/build.py rust build/sdk/standalone-consumer/templates/rust
```

The C and C++ directories can be built in the same consumer with `c` and
`templates/c`, or `cpp` and `templates/cpp`. A later `cargo xtask sdk` stages a
fresh source-only release before publishing. Unknown files in the old export,
including generated Cargo target directories, cause an explicit error; move
them outside the generated release and rerun. They are never silently included
in the checksum inventory or deleted.

Guest-hosted Linux-musl development remains the separately tested M11/M12
profile. Native SDK cross builds do not establish a native `std` port, native
guest compiler support or arbitrary C++ library support. Application permissions
and capability declarations are installer policy metadata, not a security sandbox.

GUI delivery is available separately in the M14.4 companion source SDK as a versioned C ABI
(`anova.gui.v1`), with matching C headers, a C++ ownership wrapper and a Rust safe
wrapper generated or checked against the same semantic contract. GUI protocol
version negotiation, handle lifetime/error mapping, cross-language behavior and
resource failure tests are checked independently. This native SDK 1.0.0 contains no GUI runtime;
use `cargo xtask gui-sdk` for the independent companion release. See the
[GUI SDK contract](gui/README.md) and [runtime profile](../docs/M14_RUNTIME.md).
