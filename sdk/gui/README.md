<!-- Explain GUI ABI layouts, lifetimes, linkage and the bounded three-language guest SDK workflow. -->
# AnovaOS GUI SDK 1.0.0

**English** | [简体中文](README.zh-CN.md)

This source SDK targets `anova.gui.v1` on little-endian x86_64 with the System V
function calling convention. The GUI ABI version is `0x00010000`, independent
of native syscall v1 and the SDK release. The Rust `no_std` runtime implements
every exported C function. C uses `include/anova_gui.h`; C++ uses the move-only,
exception-free `include/anova_gui.hpp`; Rust uses the safe `anova_gui_runtime`
crate. All three language demos render the same primitives, text and controls.
Version negotiation exposes capability bits: `CAP_RASTER` (1) for rectangles and
text, `CAP_WIDGETS` (2) for controls and layout, and `CAP_CLIPBOARD` (4) for the
bounded process-local clipboard.

Run `python3 sdk/gui/generate.py` to regenerate the C header or add `--check`
to reject stale output. The generator's fixed schema specifies declarations;
Rust runtime layout tests and the independently compiled guest C/C++/Rust
contracts check sizes, alignments, offsets, version negotiation and by-value
rectangle calls. C and C++ expose negative statuses numerically; the safe Rust wrapper maps the
known exact-version v1 statuses to its `Error` enum.

Calls run on one UI thread with no callbacks, re-entry or unwinding. Client
code must provide valid, aligned, live pointers for the stated byte lengths;
invalid pointer faults stay in that client process. Runtime input validation
does not turn arbitrary caller pointers into trusted memory. Borrowed text is
UTF-8, not NUL-terminated, and must remain valid only until the call returns.
Lengths and capacities count bytes. No allocator or runtime-owned pointer
crosses the ABI. Textfield storage and output buffers belong to the caller.

One process owns at most one live window, between 32 by 24 and 320 by 240 pixels.
Handles are opaque nonzero 64-bit values with generation checks. Destroy releases
the server window and invalidates the handle; stale, double-destroy and moved-from
uses return `STALE`. C++ destruction is best-effort; `close()` exposes errors.
Move construction and assignment transfer sole ownership and invalidate the
source. Rust uses `Result`, `Drop`, mutable text slices and UI-thread affinity.
Applications should close explicitly when they need observable cleanup results.

Colors are `0x00RRGGBB`; nonzero upper bits are invalid. Rectangles clip to the
window. Text supports at most 1024 UTF-8 bytes per call; the bundled bitmap font
draws ASCII and a replacement box for unsupported glyphs. The layout helper emits
fixed-height vertical rows with a gap and returns `LIMIT` when exhausted. Buttons
activate on a left-button press inside their rectangle. Textfields append valid
text events and handle backspace within the caller's capacity. They are bounded
single-line controls, with focus and selection policies deferred.

Polling preserves compositor event order and returns `WOULD_BLOCK` when empty.
Presentation can also return `WOULD_BLOCK` if a pending compositor resize requires
the application to poll its resize/reset event and redraw before retrying.
Key events retain physical key code, state and an optional Unicode scalar from
the tested US layout. Pointer coordinates are local to the window. `RESET`
clears held input after focus change or queue loss; clients must discard their
own held state. No client callback executes inside the runtime. Clipboard calls
copy up to 1024 UTF-8 bytes into process-local storage, independent of window
lifetime. The clipboard is not persistent and does not transfer data between
processes. Small output buffers return `BUFFER` without truncating valid UTF-8.

The guest build profile compiles C/C++ into ordinary user `.o` files with the
accepted GCC/G++ tools, disables C++ exceptions and RTTI, and links all clients
with the accepted guest Rust compiler, the runtime rlib, native CRT and linker
script. A separate `no_std` link anchor supplies panic termination and memory
intrinsics. Rust demo source compiles into an independent `no_std` object whose
`main` export is called by the same CRT. No libc, C++ standard library, native
Rust `std`, Linux GUI stack or host-prebuilt application is required at runtime.
Guest compilation must be tested separately from preliminary host cross builds.

`tests/guest/gui/main.c`, `main.cpp` and `main.rs` are independent language
templates. The guest editor saves each source before the build/run cycle. Each
opens the same 240 by 160 window at (80,80), emits `READY`, accepts physical `a`
into its textfield, emits `KEY`, accepts a physical click at local (40,130), emits
`POINTER`, then accepts `q`, closes and emits `DONE`. The C/C++ contract header
must accompany the templates. `note.rs` is a bounded editable note with explicit
clipboard copying. `terminal.rs` launches a native helper before creating GUI
state. Its graphical command field accepts `echo TEXT` and `pwd`; Enter sends
the bounded command through a pipe. The helper forks/executes the real native
`/bin/echo` or `/bin/pwd`, captures output through another pipe and checks the
child exit status. The GUI displays that output and remains usable for another
command. Commands and output are bounded to 128 bytes. Press `q` with an empty
command field to close and reap the helper. Shell expansion, history, PTY job
control, arbitrary executables and terminal escape sequences remain outside
this initial graphical terminal profile.

The source export includes both `README.md` and `README.zh-CN.md` in its
checksum inventory, so language switching is available in the exported SDK.
