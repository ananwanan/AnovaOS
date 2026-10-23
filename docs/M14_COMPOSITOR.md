<!-- Specify M14.3 process identities, copied surfaces, presentation, input routing and recovery. -->
# M14.3 user-space compositor contract

This experimental native GUI transport is separate from the frozen native v1
syscalls, SDK 1.0.0, Linux compatibility and the future M14.4 GUI library C ABI.
See [M14 GUI](M14_GUI.md) and [implementation status](STATUS.md) for stage status.

## Isolation and authority

The Rust `/bin/compositor` is a normal isolated native user process. Its private
pages contain window metadata, RGB surface copies and the software screen buffer.
`crates/gui` implements window ordering, focus, movement, resizing, decoration,
visibility, damage and input targeting. The kernel implements none of those
policies. GUI clients run in their own private user address spaces.

The optional kernel `gui` feature adds experimental syscall `0x100`; existing
native v1 numbers 0 through 23 retain their meaning. Server admission compares
the loaded executable bytes with the immutable, explicitly named compositor boot
module. A pathname, command argument, packet field or client-provided PID cannot
grant authority. Fork does not copy admission or a connection; successful exec
recalculates admission and releases the previous connection. Unsuccessful exec
retains the old executable and its resources. This is executable admission,
not a general filesystem permissions or process sandbox system.

One admitted server exclusively binds display and physical input. Each bind
increments the session number. Four authenticated client connections are allowed;
the kernel derives identity from the actual calling process. A connection carries
one pending packet, at most 4,096 bytes, and sixteen 64-byte server replies.
The server's receive envelope contains the actual client PID, session, lifecycle
kind and initialized payload length. Receive scheduling rotates among clients.
Clients cannot consume another connection's replies or call server operations.

Queues are nonblocking. An occupied request/reply queue returns `WOULD_BLOCK`;
clients yield or sleep and retry their requests. A non-consuming client whose
sixteen-reply mailbox fills is evicted by the compositor through server-only
`DROP_CLIENT`: the window is removed, pending messages are discarded and client
calls return `BROKEN_PIPE` until an explicit fresh connection is established.
Other clients continue. No user borrow or active user page table crosses
a scheduling boundary. All allocation is fallible and connection admission is
transactional. Process exit, hardware fault or successful exec closes its channel.
A disconnect envelope removes its compositor window. Server death drops every
mailbox and returns physical input to the existing diagnostic consumer; old
clients observe `BROKEN_PIPE` and must reconnect to the new session. Surfaces
are not resurrected automatically after restart.

## Packets, handles and surface ownership

The internal request and reply are exactly 64 little-endian bytes, with explicit
fixed-width fields, version/session checks and reserved-field rejection. Requests
carry operation, sequence, session, window, signed position, dimensions, pixel
stride/count and flags. Authority remains in the kernel envelope, never a field
inside this untrusted packet. See
[`wire.rs`](../crates/gui/src/wire.rs) and
[`transport.rs`](../crates/gui/src/transport.rs) for exact layouts.

Each client owns one window, from 32 by 24 through 320 by 240 RGB u32 pixels.
The pixel format is fixed by the internal wire protocol, without format negotiation.
Only the low 24 RGB bits are retained; the high byte is ignored, without alpha blending. Handles include
a monotonically increasing generation and are scoped to a session. The server
rejects foreign/stale handles, duplicate windows, unsupported operations,
invalid dimensions, source length/stride mismatches and arithmetic overflow
before altering state. Surface memory belongs to the compositor and is never
mapped into a client. No process receives physical video addresses or another
process's virtual pointers.

An upload copies an entire validated bounded rectangle from the packet into the
compositor's private surface. Its acknowledgement means that the client can
reuse all source storage. Separate chunks commit separately: there is no atomic
multi-packet frame, sealing protocol, zero-copy shared surface or GPU fence.
`PRESENT` damages the window for composition. Intervening redraw can observe
already acknowledged upload chunks. Resizing clears the retained private surface;
clients repaint their content. Destruction, disconnect and restart invalidate the
old handles and release the relevant slots.

## Software composition and physical input

The user compositor tracks the union of damaged regions, including old and new
geometry, occlusion changes, focus decorations and cursor footprints. It redraws
that region in stacking order and submits only the damage rectangle. The kernel
checks dimensions, destination, source stride, checked pointer range and every
user page, then fallibly copies the complete region before touching video memory.
Failed user copying leaves the device unchanged. Checked device packing and
volatile stores use the reserved supervisor RW/NX WC mapping on the permanent
kernel root. MFENCE completes CPU stores before return; completion does not mean
vblank, atomic scanout or a page flip.

The existing bounded i8042 scheduler poll supplies physical keys, absolute clipped
pointer positions and button changes to the bound server. Its 64-event queue
retains the M14.2 reset semantics. The kernel diagnostic cursor is hidden while
the compositor owns input; the user compositor renders its own cursor. Window
focus, title drag, resize drag and event targets are user policy. A reset cancels
held state/drag tracking. Keyboard events remain physical identifiers rather than
Unicode text. USB, hardware IRQ routing, keyboard layouts and IME are outside
this profile.

The private 800 by 600 screen buffer and four maximum surfaces together occupy
3,148,800 bytes. They fit the existing 4 MiB native ELF memory bound without
increasing it; single-threaded process-owned BSS avoids a giant user stack.
The maximum display is 800 by 600. This remains CPU rendering on the existing
standard-VGA framebuffer. No GPU acceleration or public GUI widget/SDK library
is implied.

## Failure and validation boundary

COM1 and the existing native shell remain independent of GUI processes. No VGA
returns an explicit unsupported-display result. Missing i8042 returns an explicit
unsupported-input result. Client faults retain other windows; server failure
invalidates connections and allows a fresh process and session. The integration
harness uses deliberate user faults, independently spawned processes and actual
QMP physical input, rather than treating a rendered picture as isolation proof.

The `gui-test` kernel feature additionally exhausts the actual kernel heap before
calling production mailbox connection admission. It requires `NO_MEMORY`, no
partial connection or retained allocation, successful admission after releasing
pressure, the four-client quota and exact final heap recovery. This test is
separate from ordinary interactive `gui` images. Exact executed commands, hashes,
scanouts and regression limits belong in the stage validation record.

M14.4 still owns fonts, controls, text/graphical terminal, clipboard, versioned
C ABI and independently guest-built C/C++/Rust GUI applications. M14.5 owns
actual GPU rendering and its fallback gate. Physical hardware, SMP and Linux
GUI compatibility remain independent work.
