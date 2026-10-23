<!-- Specify delivered software GUI/input/runtime SDK stages and the independent planned GPU gate. -->
# M14: GUI architecture and five-stage delivery

Status: **M14.1–M14.4 implemented and locally verified; M14.5 GPU remains planned**
(2026-10-23). Actual local acceptance is recorded separately in
[M14.1 validation](validation/M14.1.md), [M14.2 validation](validation/M14.2.md),
[M14.3 validation](validation/M14.3.md) and [M14.4 validation](validation/M14.4.md).
This design is subordinate to [AGENTS](../AGENTS.md), [architecture](../ARCHITECTURE.md),
[roadmap](../ROADMAP.md), and [implementation status](STATUS.md). Do not interpret the
Limine framebuffer boot response or any rendered host mockup as a functioning desktop.

## 1. Decision and scope

AnovaOS will implement its native GUI system in **Rust**, and expose **C, C++, and Rust
application SDKs** over one versioned semantic/API contract.

- The kernel manages display/input drivers, safe memory mapping, IPC and process
  isolation. It does **not** implement widgets, desktop policy, or a kernel-mode
  window manager.
- A privileged **user-space** compositor/window server owns display presentation,
  window stacking, focus, input routing, damage tracking and composition.
- A user-space Rust GUI runtime owns drawing primitives, fonts, controls, layout,
  application event delivery and high-level window APIs.
- The supported language-neutral binary surface is a **C ABI implemented in Rust**;
  C gets declarations, C++ gets move-safe RAII wrappers, and Rust gets a safe idiomatic
  crate. Rust may use an optimized internal implementation only if it preserves the
  same observable semantics and protocol compatibility as the C ABI.
- Begin with **CPU/software rendering**; add an optional hardware-backed rendering
  path only after the required driver, memory synchronization, userspace interface
  and measurable validation exist. Software fallback remains supported.
- Native SDK is first-class. Linux GUI ABI and Wayland/X11/Qt/GTK compatibility are
  separate ports with their own tested versions and dependencies; they are **not**
  implied by ELF, glibc, SDL, a framebuffer, or this SDK.

This is an architectural target, not a commitment to a particular Rust graphics
crate or to complete GPU/Vulkan compatibility.

## 2. Trust and component boundaries

```text
  Rust app        C++ app          C app
     |               |               |
  Rust SDK        C++ RAII       C headers/wrappers
     |               |               |
     +------- Rust GUI runtime / stable C ABI ----+
                             |
                versioned GUI IPC + shared surfaces
                             |
              user-space Anova compositor/server
                       |             |
               software renderer  optional GPU backend
                       |             |
              kernel IPC / display / input drivers
                             |
                    framebuffer / device
```

The C ABI is a user-library boundary, **not** the kernel syscall ABI. Applications
must never acquire unrestricted physical framebuffer access or memory mappings
belonging to other processes. GUI objects are user-space resources backed by
versioned IPC and compositor capability handles. The kernel retains validation,
page ownership, buffer mapping and process isolation responsibilities.

Plan protocol and implementation as separate, documented pieces: message framing
and ABI version, surface creation and ownership, per-client quotas, geometry,
damage rectangles, format/stride checks, buffer submission/presentation,
event delivery, destruction/reconnection, and failure propagation. Validate
every client length, count, offset, pointer, handle generation and checked
width × height × bytes-per-pixel computation before use. Do not rely on a
trusted client, a single thread, or a perpetual compositor process.

IPC and shared buffer lifetime must define: who allocates, which process may
map/read/write, sealing or ownership transfer during present, how fences or
completion are reported, how close/crash releases resources, and what happens
on compositor restart. CPU/software paths must not depend on GPU availability.

## 3. SDK/ABI contract (design target)

The ABI contract below guides the implementation; [M14 runtime](M14_RUNTIME.md)
and the [SDK contract](../sdk/gui/README.md) specify the delivered v1 choices:

1. A separate `ANOVA_GUI_ABI_VERSION` and capability/feature negotiation, with
   clear unsupported-version errors. Do not confuse GUI library ABI with the
   native syscall ABI or the Linux compatibility personality.
2. `extern "C"` exported entry points, `#[repr(C)]` fixed-layout types,
   explicit-width integers, deliberate alignment/padding and call conventions.
   No Rust `String`, `Vec`, trait object, unwinding, borrowed internal pointers
   or unstable `repr(Rust)` layouts cross the FFI boundary.
3. UTF-8 text as pointer + byte length where needed; borrowed inputs valid only
   for the documented call, output buffers carrying capacity/written length
   or owning an explicit release function. Avoid hidden allocator coupling.
4. Opaque app/window/surface handles with well-defined lifecycle, ownership,
   double-destroy and stale-handle behavior. Do not cross process boundaries by
   passing raw address values as authority.
5. Error return values and a documented error model; no Rust panic or C++
   exception may unwind through the C ABI. User failure must not panic the kernel.
6. Event loop model, callbacks, UI-thread affinity, re-entrancy, ordering,
   cancellation, queue overflow/backpressure and thread-safety of each operation.
7. C++ wrapper ownership using RAII and move semantics, with exception/no-exception
   API behavior defined; Rust wrappers use types/lifetimes and a `Result`-style
   error model. Each language remains behaviorally equivalent.
8. Header generation (candidate: `cbindgen`), library packaging, symbol/export
   version policy, compatibility tests and separate SDK/toolchain targets.

M14.4 implements the separate `anova.gui.v1` library contract described in
[M14 runtime](M14_RUNTIME.md). Exported names include
`anova_gui_query_version`, `anova_gui_window_create`, `anova_gui_window_show`,
`anova_gui_poll_event` and `anova_gui_window_destroy`. The allocation-free v1
profile uses one process-local window and polling; there are no app objects,
callbacks or public shared-buffer mappings.

SDK validation must compile and run one independently built C, C++ and Rust GUI
client, plus an incompatible-version probe and negative cases for malformed
parameters, stale handles, allocation failure and client exit. Using one
language's build result as evidence for the other two is insufficient.

## 4. Candidate implementation layout (create on demand)

```text
crates/gui-protocol/          # Plain validated messages, IDs and wire versions
user/compositor/              # User-space compositor, window and focus policy
user/desktop/                 # Desktop shell, taskbar and system applications
user/gui-runtime/             # Rust software rendering, widgets and event loop
user/gui-sdk/ffi/             # Rust implementation of exported C ABI
user/gui-sdk/rust/            # Safe idiomatic Rust API
user/gui-sdk/c/               # C headers, library and examples
user/gui-sdk/cpp/             # C++ RAII header/library and examples
tests/guest/gui/              # Deterministic integration and fault scenarios
docs/validation/M14*.md      # Evidence files only once testing really exists
```

No empty directory, dummy implementation, claims of validation, or new Cargo
dependencies should be committed simply to match this future layout.

Candidate crates for evaluation (not dependencies yet): `embedded-graphics`
for simple pixel primitives, `tiny-skia` for richer software drawing, and
`fontdue` for glyph rasterization. Before selection, verify exact crate
versions, licenses, target/allocator/no_std requirements, cost, safety and
guest-side reproducibility. Fonts, rasterizer and layouts belong in user space.

## 5. Five milestones with independent gates

### M14.1 — Display and software framebuffer baseline

Dependencies: existing Limine response checks, owned paging, safe device mapping
and a repeatable QEMU video device profile.

- [x] Verify mode, pixel format, pitch/stride, dimensions and buffer bounds;
      explicitly reject unsupported formats rather than assuming RGBA.
- [x] Implement bounded pixel/rectangle/blit primitives, clipping and a software
      back buffer; present without tearing where the selected backend permits.
- [x] Provide a deterministic boot graphic/test pattern and text diagnostic
      fallback that preserves COM1 output and failures.
- [x] Keep mapped video memory out of general-purpose RAM allocation; document
      caching, pixel packing and ownership.

**Gate:** QEMU shows a correctly clipped graphical pattern; tests assert known
pixels/checksums and out-of-range/different-format negatives, and fail on
timeout/panic. State hardware/mode limitations explicitly. A framebuffer is not
a window server or GPU acceleration.

### M14.2 — Input events and cursor

Dependencies: M14.1, a selected/tested virtual input controller and its
poll/IRQ delivery contract.

- [x] Support the QEMU i8042 native set-1 keyboard and standard ID-0 three-byte
      relative mouse; normalize key state, buttons and nine-bit relative motion.
- [x] Route a bounded queue to one diagnostic target and render a clipped cursor.
      Physical keys remain separate from text; user-space window focus is M14.3.
- [x] Use explicitly temporary scheduler polling with bounded work; external
      IRQ1/12 remain masked and require separate routing/EOI/lifecycle validation.
- [x] Reset held state/history after overflow, malformed input or consumer detach;
      preserve serial operation when the display or controller is absent.

**Gate:** injected key/button/movement sequences produce ordered events and
visible cursor updates; bounds/overflow/disconnect cases are checked in QEMU.
A working virtual PS/2 path does not imply USB or real-hardware input support.
The local three-boot `input-test` gate passed; the exact profile, artifacts and
regression scope are in [M14.2 validation](validation/M14.2.md).

### M14.3 — User-space compositor and native windows

Dependencies: M14.1–M14.2 plus validated IPC and checked user buffer copying.
The accepted initial profile deliberately uses copied private surfaces; shared
user mappings are not required or implemented. See [M14_COMPOSITOR](M14_COMPOSITOR.md).

- [x] Implement a separate compositor process, authenticated per-client
      connections, surface ownership, quotas and lifecycle cleanup.
- [x] Implement damage-based software composition, window ordering, moving,
      resizing, focus, visibility and input routing; no widget logic in kernel.
- [x] Specify surface/buffer ownership and presentation completion, redraw
      after occlusion and restart behavior.
- [x] Keep terminal/serial recovery available if compositor or a client fails.

**Gate:** at least two isolated user processes display overlapping, movable
windows with correct focus, redraw and input targeting. Kill a client; verify
its resources disappear without harming other clients. Test bad surfaces,
stale handles, memory pressure and compositor restart/failure.

### M14.4 — GUI runtime and C/C++/Rust SDKs

Dependencies: M14.3, M13's app packaging/SDK discipline, and the toolchains
actually verified for each language (M11 C/C++, M12 Rust/Cargo).

- [x] Implement the Rust GUI runtime: 2D primitives, glyph/text, layout,
      widgets (button/text field), event loop and window API.
- [x] Define/freeze a versioned C ABI in Rust with header generation,
      error/handle/ownership contracts and ABI compatibility tests.
- [x] Ship a C header/library, C++ RAII wrapper and ergonomic safe Rust SDK.
      Provide build templates and ABI-specific linkage/install metadata.
- [x] Add a basic graphical terminal, clipboard policy and at least one native
      desktop GUI application; state any deferred features explicitly.
- [x] Compile/link/run separate Rust, C and C++ window demos in the guest when
      the relevant guest toolchain is accepted. Host cross-build alone is
      preliminary evidence, not completion of an in-OS development objective.

**Gate:** three independent language clients each open a window, process an
event, render content and close cleanly. Verify lifecycle errors, UTF-8,
cross-language layout/callback ABI, incompatible versions, user faults and
resource reclamation. Demonstrate the full edit/build/run cycle in AnovaOS.

### M14.5 — Optional hardware-accelerated rendering

Dependencies: working software desktop, device/PCI/DMA ownership, safe shared
memory and a selected tested graphics-device/userspace stack.

- [ ] Evaluate a selected `virtio-gpu` display path, document 2D scanout
      separately from true accelerated rendering and keep M14.1 fallback.
- [ ] Specify device queues, buffer allocation, mapping/cache coherence,
      synchronization/fences, resource resets and isolation from untrusted apps.
- [ ] Implement and verify at least one **actual GPU-rendered** composition or
      drawing path using a demonstrated device, driver and user-space interface.
- [ ] Compare deterministic rendered output with the CPU reference under
      supported modes, recovery and device-reset/failure cases.
- [ ] Record chosen versions, driver limitations, platform assumptions and
      measured performance; avoid claiming Vulkan/OpenGL support without
      testing their real user-space stacks and drivers.

**Gate:** the selected acceleration path is demonstrably GPU-backed, its
presented output and failure recovery are tested, and CPU fallback passes
regression. Virtio-gpu scanout or a framebuffer alone does **not** satisfy
the acceleration gate. If no such path is implemented, M14.5 stays open.

## 6. Stage dependency, related work and exclusions

Each M14.x gate is tracked separately in [ROADMAP](../ROADMAP.md) and
[STATUS](STATUS.md); a later gate never retroactively marks an earlier
gate complete. Record QEMU configuration, guest binary hashes/versions,
expected markers, negative cases, exit/timeout results and non-QEMU hardware
claims in a separate validation record only when tests have actually run.

SMP, ACPI/power, NVMe/AHCI, USB/xHCI, physical GPU support, glibc and
Wayland/X11 compatibility remain independent future workstreams. They are
not silently included in M14.1–M14.5; update the roadmap when scoped.
In particular, Linux GUI compatibility is not delivered by exporting
AnovaOS's C ABI, and hardware acceleration is not delivered by merely
accepting a bootloader framebuffer.

M14 must not destabilize the accepted M0–M11 serial, process, VFS, toolchain
and negative-test gates. Do not fabricate a `cargo xtask gui-test` command;
introduce a real test command only along with its working implementation
and check it on the documented reference configuration.

## 7. Implemented M14.1 display contract

`crates/graphics` is a dependency-free `no_std` software library; it does not
map hardware or allocate. `Layout` checks width/height, pixel count, row pitch,
total length and distinct eight-bit RGB masks before packing little-endian
24-bit or 32-bit pixels. The limits are 4,096 pixels per axis, 2,097,152 total
pixels, 8 MiB of RGB back buffer and 16 MiB including device row padding.
`Surface` borrows caller storage and clips signed pixel/rectangle/blit origins.
Blits validate the entire source/stride before writing any destination;
presentation touches visible bytes only and preserves row padding.

The optional `graphics` kernel feature owns one boot diagnostic presentation.
`boot::display` accepts only the primary RGB framebuffer, validates the full
visible byte span inside a framebuffer reservation and rejects overlap of its
page-rounded mapping with allocator, boot/module or firmware memory. Limine
permits a partial final framebuffer page; its mapped tail is never accessed.
`mm::check_display` verifies every inherited mapping's physical identity,
supervisor RW/NX permissions and PAT index 5. It also verifies PAT5 is WC.
The mapping is retained, with no new alias or altered PAT/MTRR configuration.
Only USABLE memory reaches the existing frame allocator; video memory is never
allocated, freed, handed to an application or added to the ordinary heap.

This follows the locked [Limine protocol caching and HHDM contract](https://github.com/Limine-Bootloader/limine-protocol/blob/3a0526b700e356f0eac1b71a77697b3fd1c707a3/PROTOCOL.md).
The back buffer uses fallible reservation on the existing heap. The kernel
renders the deterministic reference pattern, writes visible bytes through
volatile stores, executes MFENCE and compares every byte read back before
reporting `[GRAPHICS:PRESENT]`. Temporary storage is dropped before user startup,
with an asserted heap baseline and `[GRAPHICS:RECLAIM]`. Errors report
`[GRAPHICS:FALLBACK]` and leave the normal COM1 shell available. Hardware
for this initial presentation occurs only on the BSP boot path, before any process
runs. M14.2 later accesses the same reservation only from the BSP scheduler on
its permanent kernel root; no IRQ or application receives access to this memory.

`graphics-image`, `graphics-test` and `run --graphics` are implemented host
commands. The independent host gate captures a real QMP PPM image from QEMU
standard VGA, compares every pixel and canonical checksum, then exercises
serial recovery and clean ACPI shutdown. Another boot with `-vga none` must
reach the missing-display fallback and retain the same recovery/shutdown path.
The host-only JSON dependency handles QMP framing; it is not linked into the
kernel or distributed as a guest graphics library.

This CPU copy does not synchronize to vblank or promise tear-free animation;
the standard-VGA backend supplies no page-flip/completion interface. Only the
fixed 800×600×32 QEMU scanout is accepted in hardware; other admitted pixel
packings are host-tested. There is no GPU drawing, graphical text terminal,
user-space compositor or GUI syscall in this diagnostic. M14.3 separately adds
the experimental copied transport; a public GUI library ABI remains M14.4.
M14.2 extends the temporary kernel
diagnostic with physical input/cursor only. It will yield presentation ownership
to the future user-space compositor and adds no window, focus, font or widget policy.


## 8. Implemented M14.2 polled PS/2 diagnostic contract

### Device and scheduling ownership

The same optional `graphics` feature selects the QEMU q35 i8042 controller,
a keyboard explicitly configured for native scan-code set 1 with controller
translation disabled, and a standard ID-0, three-button, three-byte relative
mouse. `kernel/src/drivers/input.rs` owns ports `0x60`/`0x64`, controller/interface
checks, command responses and device enablement. Every command argument needs
its own ACK; AUX-source/status checks keep keyboard and mouse replies separate.
Initialization waits use the calibrated ACPI PM timer with a 100 ms bound plus
an attempt cap, bounded flushes and at most three device-command sends.
Unsupported/missing devices preserve the COM1 shell.

Input uses **temporary scheduler polling**, not external IRQ delivery. The sole
BSP scheduler accesses the driver at IF=0 on the permanent kernel root and
consumes at most 64 controller bytes and 64 queued events per pass. IRQ1/12
remain masked; no IRQ handler allocates, logs or accesses input/cursor state.
Each device tracks its own last-byte PM-timer timestamp. A partial keyboard
prefix or mouse packet older than 100 ms is invalidated before late continuation
bytes are consumed, even while the other device is active. PM wrap arithmetic
and independent-source expiry have test-image checks.

The protocol was checked against the selected primary implementation,
[QEMU 8.2.2 i8042 controller](https://github.com/qemu/qemu/blob/v8.2.2/hw/input/pckbd.c)
and [PS/2 device model](https://github.com/qemu/qemu/blob/v8.2.2/hw/input/ps2.c),
and [Microsoft's standard PS/2 packet format](https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/keyboard-and-mouse-hid-client-drivers).
No upstream implementation code is copied into the decoder.

### Physical events, bounds and recovery

`crates/input` is dependency-free `no_std` logic with no port I/O or allocator.
It exposes physical press/release/repeat state, extended-key identity, modifier
state and pointer/button events. PrintScreen fake shifts do not modify held
Shift, Alt+PrintScreen/SysRq normalizes to the same physical key, and canonical
Pause emits one press without a persistent held state because it has no break
sequence. Physical key numbers are diagnostic identifiers, not characters or
a frozen GUI ABI. No layout conversion, Unicode/text event, IME or shell text
injection is provided.

Mouse axes use the header's sign bit with the unsigned data byte, preserving
the full nine-bit `-256..=255` wire range. The dispatcher converts upward-positive
wire Y into downward-positive screen Y. Three button states and changes remain
explicit. An overflow packet discards both motion axes and retains button
changes; a bad header or transport error invalidates state. Header bit 3 is
used only while searching for a header, because data bytes may also contain
that bit. Arbitrary missing bytes cannot always be detected by this format;
controller errors and partial-packet expiry provide explicit recovery.

The queue has 64 fixed entries. When full, it discards the buffered history and
the triggering event, clears decoder/held state and publishes one reset before
later events. Malformed input and device errors use the same invalidation.
Consumer disconnect discards history/state and ignores bytes until reconnect;
reconnect begins with another reset. Pointer position stays clipped and retained,
while every reset releases all buttons. Consumers must invalidate all held
keys/buttons after a reset.

For this stage, **one kernel diagnostic consumer is the only input target**.
It logs physical events and updates the cursor; there are no user connections,
window focus changes or application event destinations. User-space focus,
authenticated client lifecycle and routing remain M14.3. The test-only consumer
detach/reconnect exercise is not evidence of physical device hotplug.

### Cursor and local gate

The diagnostic cursor has a fixed 7×9 footprint with space for 63 saved device
pixels, retaining their packed device bytes and unused channels. It restores
its prior background, saves the newly clipped footprint
and draws a black/white arrow through the validated supervisor WC mapping.
All coordinates/offsets are checked and writes are fenced. Input, queue and
cursor state allocate no heap; M14.1's temporary full-screen buffer is still
reclaimed. The boot pattern remains untouched until the first pointer event,
so `graphics-test` retains its original screenshot contract. A reset hides the
cursor and restores its background.

`cargo xtask input-test` builds an independent `graphics,input-test` image and
runs three 60-second-watchdog boots: standard VGA, `-vga none`, and
`-machine i8042=off`. QMP injects ordered key/release/repeat, extended-key,
modifier, relative-motion and all three button sequences. The display boot
compares every one of 480,000 screenshot pixels at the center and four corners,
checking cursor clipping and restoration of the old footprint. The no-display
boot still exercises input and serial fallback. Every boot requires a working
COM1 recovery command and clean ACPI shutdown; unexpected healthy-stream resets,
fatal markers, premature exit, wrong pixels and timeout fail the gate.

The test image independently exercises queue overflow, malformed input,
diagnostic consumer detach/reconnect, stale per-device sequence expiry and real
controller-port disable/reinitialization before announcing readiness. These
checks do not claim a general device removal/replug mechanism. Evidence,
source/artifact hashes and exact regression scope belong in
[M14.2 validation](validation/M14.2.md); remote CI execution is separate.
CI runs `input-test` and retains its `build/input/` artifacts.

Only the documented QEMU PS/2 profile and software cursor are accepted. USB,
physical hotplug, peripheral IRQ routing, real-hardware keyboard/mouse support,
graphical terminal/text, user-space compositor, public GUI event ABI and GUI
SDK remain outside M14.2. M14.3 is accepted separately under
[M14_COMPOSITOR](M14_COMPOSITOR.md) and [its validation](validation/M14.3.md);
M14.4 is separately accepted under [runtime](M14_RUNTIME.md) and
[validation](validation/M14.4.md); M14.5 GPU rendering remains open.

## 9. M14.4 runtime implementation

The Rust runtime, generated C ABI and three language SDKs are implemented in
`crates/gui-runtime` and `sdk/gui`. The graphical Rust note and bounded interactive
terminal use the same runtime as the independent language demos. The guest build
profile uses the accepted GCC/G++ and Rust compilers to generate native outputs;
it does not establish native libc or Rust `std`. See [runtime contract](M14_RUNTIME.md)
and [M14.4 validation](validation/M14.4.md) for concrete API/ownership details,
actual gate results and exclusions. M14.5 still requires a separately proven GPU path.
