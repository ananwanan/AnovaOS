<!-- Specify the allocation-free Rust GUI runtime, stable C ABI and guest native build boundary. -->
# M14.4 GUI runtime and SDK contract

This stage layers a user-space Rust library over the accepted
[M14.3 compositor](M14_COMPOSITOR.md). It adds no kernel syscall, display
mapping, Linux GUI personality or native Rust standard-library port.
The runtime owns private raster storage, text, layout, widgets and the
application event interface. The compositor retains window policy, focus,
input routing and scanout. See [M14 GUI](M14_GUI.md) for the milestone split.

## Library ABI and ownership

The separately versioned library interface is `anova.gui.v1`, encoded as
`0x00010000`. It is independent of native syscall v1, experimental GUI wire
version 1 and source SDK release 1.0.0. Rust implements all exported
`extern "C"` functions. Generated C declarations, move-only C++ RAII and
safe Rust wrappers call the same implementation. No callback is registered;
applications poll ordered events and decide when to redraw.

Version, window description, rectangle, event and layout records are fixed
`repr(C)` integer-only structures. The C header asserts sizes, alignments and
selected offsets; independent language clients check actual negotiation and
linkage. Header regeneration is deterministic via `sdk/gui/generate.py`.
`--check` rejects a stale checked-in header before exporting an SDK.
No Rust layout, exception, allocator ownership or internal borrowed pointer
crosses the ABI. All text lengths count UTF-8 bytes. Inputs are borrowed only
for a call; outputs occupy caller-owned writable storage. Null, alignment,
size, version, text and color checks precede mutation. Like a normal C library,
an arbitrary nonnull caller pointer must still designate live accessible
memory; violating that contract faults the client process, never the kernel.

One process owns one runtime and at most one window, with a private raster
bounded to 320 by 240 RGB pixels. A monotonically increasing local generation
makes a destroyed handle stale even after another window is created. Handles
are process-local integers, never display addresses or compositor authority.
The compositor independently checks authenticated owner/session/generation.
No heap allocation is needed. Capacity pressure reports a stable negative
error; it does not pretend that a heap allocation occurred.

Applications serialize calls on one UI thread. An atomic guard rejects
simultaneous access or reentry rather than aliasing global mutable storage.
There are no callbacks, user threads or signal callbacks in this profile.
Fork after GUI initialization is unsupported; the terminal forks its command
child before creating GUI state. Destroy releases the remote window, closes
the client connection and invalidates local state. Process exit remains a
kernel-enforced cleanup path. Compositor failure invalidates connections;
applications may close local state and explicitly create a fresh window.

## Rendering and input

RPC send and reply phases each wait at most 500 native timer ticks; polling
never waits. The local event queue holds 32 events and reports overflow with
RESET while retaining the latest resize geometry. A resize during an upload
returns `WOULD_BLOCK`; the application polls queued geometry and redraws before
retrying presentation.

The runtime draws clipped rectangles, original five-by-seven bitmap glyphs,
vertical layout rows, buttons and text fields. ASCII glyphs are supported;
lowercase shares uppercase glyph shapes and other valid Unicode scalars use
a visible replacement box. UTF-8 validation and storage preserve the original
bytes. This is not a Unicode shaping, proportional-font or IME implementation.

The native client transport copies bounded upload chunks, waits for their
acknowledgements and then presents the window. Input events received during
RPC are retained in a bounded queue; overflow resets held-input history.
Polling returns `WOULD_BLOCK` when empty. Focus changes and device resets
clear held input. Physical US key mapping is separate from raw key codes;
text fields accept text input and bounded editing. Buttons use window-local
pointer hit testing. The exact per-call bounds and errors are documented in
the [SDK contract](../sdk/gui/README.md).

Clipboard storage is UTF-8, bounded, process-local and nonpersistent. It has
no cross-process selection owner or background access to another application's
text. A wider desktop clipboard needs a separate broker and permission policy.
CPU copying provides no atomic multi-chunk frame or vblank guarantee.

## Native guest development

`cargo xtask gui-sdk` exports a checksummed source SDK to
`build/sdk/anova-gui-sdk-1.0.0`; it is separate from the unchanged native
SDK 1.0.0 contract. `cargo xtask gui-sdk-image` packages it with the accepted
M11 GCC/G++ and locked M12 Rust musl compiler tools. The native data volume's
immutable `/persist/.anova-abi` selects `anova.native.v1`; the compiler tools
remain explicitly Linux-personality initramfs executables.

Within this image, `gui-build c`, `gui-build cpp` and `gui-build rust` compile
independent `/persist/main.c`, `main.cpp` and `main.rs` sources using the real
guest tools. GCC/G++ produce freestanding client objects. Guest rustc builds
the dependency-free runtime rlib, then links each client with a small native
CRT/panic/intrinsics anchor. C++ uses move-only RAII with exceptions and RTTI
disabled. Rust uses `no_std` safe wrappers. The compiler's musl target supplies
`core` and code generation; resulting executables call native syscalls and
have native admission policy. This does not establish native libc or `std`.

`gui-run /persist/gui-c` starts an isolated compositor and client, reports
actual child wait statuses and reaps both. The demo exits after receiving
its documented physical key/button events; F11 then stops the compositor.
The Rust note application exercises text/widgets. The bounded Rust graphical
terminal accepts keyboard-entered `echo TEXT` and `pwd`, executes real native
commands through pipes and displays their captured output. It starts its helper
process before GUI initialization and reaps the helper on close. It does not
supply a general PTY, shell job control, ANSI terminal emulation or an
unrestricted command environment.

`cargo xtask gui-sdk-test` edits sources through the guest editor, builds
independent language executables, injects physical QMP input, checks actual
scanout and compares source/executable bytes after clean shutdown and reboot.
The gate includes version/lifecycle/UTF-8/capacity failures and a deliberate
client pointer fault. Actual results belong in [M14.4 validation](validation/M14.4.md).
M14.5 GPU rendering remains a separate stage.
