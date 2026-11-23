<!-- Define USB host, enumeration, HID and desktop responsibilities and their independent gates. -->
# USB input development

**English** | [简体中文](../../zh-CN/milestones/USB_INPUT.md)

The first target is a boot-connected keyboard and relative mouse on a q35 xHCI root port.
Work starts on `feat/usb-hid-input` from `dev`. This track extends M14 input and the M17 desktop
independently of M18 compiler reconstruction. See the [test guide and evidence](../validation/USB_INPUT.md).

## Ownership and event flow

| Layer | Owner and interface | Data passed upward |
| --- | --- | --- |
| Host controller | `kernel/src/drivers/xhci/`; owns the PCI function, mapped UC registers, rings, DMA pages and slot/context lifetime | Validated completion with the actual byte count, port and endpoint identity |
| USB Core (planned) | Owns device/configuration/interface/endpoint records and enumeration state. Submits setup packets and interrupt IN requests through the host | Interface descriptors, class control results and bounded report bytes |
| HID class (planned) | One binding per active interface, with its own protocol, previous report and per-ID layouts | Physical key transitions, relative motion, changed buttons and signed wheel delta |
| Input and desktop | Extend `anova-input::Dispatcher`; the kernel's scheduler poll serves existing GUI hardware events, compositor focus routes them to the window/PTY | Source-independent key/pointer events, layout and text handling in the existing consumer |

The planned path starts when a device completes an interrupt IN transfer. The scheduler drains a
bounded number of event TRBs, checks slot, endpoint, outstanding TRB identity, completion code and
residual length, then hands only completed bytes to the bound interface. HID decodes into input
events and re-arms the endpoint. The desktop never waits on a command completion. Keep the current
PS/2 decoder as a producer. Maintain device-local key/button sets and aggregate them before emitting
transitions so releasing one keyboard's Shift does not release another's. Reuse cursor clipping,
queue-overflow reset, layout and repeat policy. Define a compatible wheel event in the experimental
transport and its consumers before accepting wheel delivery; keep the frozen public ABI intact.

## Increments and exit criteria

| Stage | Implementation and exit | Current state |
| --- | --- | --- |
| USB.1 | Detect PCI 0c/03/30; own and bound MMIO, reset, DCBAA, command/event rings, ERST, slot and endpoint state. Verify command completion, ring wraps, slot release, timeouts, and absent-controller boot. | PCI and controller command-ring slice written. Build and QEMU pending. Endpoint contexts, transfer rings and external IRQ integration pending. |
| USB.2 | Reset each connected root port per its supported protocol and speed. Enable Slot, prepare contexts for EP0, Address Device, read the first device bytes, evaluate the EP0 maximum packet size, then read the full device/configuration tree. Configure interrupt endpoints and issue Set Configuration. Bind each supported interface independently. | Planned after USB.1 verification. |
| USB.3 | Bind HID 03/01/01, issue Set Protocol(Boot), receive interrupt IN. Diff eight-byte keyboard reports, modifiers and simultaneous usages. Preserve previous keys on rollover/truncation. Cover letters, digits, editing keys, arrows, function keys, Shift/Ctrl/Alt, held and released states in the graphical PTY. | Planned after enumeration. |
| USB.4 | Bind HID 03/01/02, issue Set Protocol(Boot). Decode three button bits and signed relative X/Y from the boot prefix. Verify clipping, left/right window interaction, middle press/release, continuous movement and PS/2 coexistence. | Planned after the keyboard gate. |
| USB.5 | Fetch the HID class descriptor and declared report descriptor. Parse short Main/Global/Local items, nested collections, Push/Pop, usage lists/ranges, report ID/size/count, logical bounds, Input array/variable flags, padding and independent bit offsets per report type and ID. Select Report Protocol only with a usable layout. | Planned after both boot drivers. |
| USB.6 | External hubs, runtime attach/detach and fault recovery have separate acceptance below. | Design only. |

USB Core must reject zero lengths, short headers, inconsistent total lengths, endpoint zero used as
a data endpoint and resource exhaustion. Retain VID, PID, class, configuration, interface/alternate
setting and endpoint address/type/max packet/interval in diagnostics. Optional strings use bounded
UTF-16LE reads. Unsupported interfaces leave their siblings usable. Never issue standard
SET_ADDRESS behind xHCI's Address Device command.

USB.5 tracks Generic Desktop, Keyboard/Keypad and Button pages, including relative X/Y/Wheel and
additional buttons. Account for constant fields and unknown usages when advancing bit offsets.
Check all lengths and count-times-size arithmetic before extraction. Bound descriptor bytes, item
depth, globals, report IDs and fields. Validate actual packet length before committing any state.
Test signed cross-byte extraction, multiple IDs, keyboard arrays and bitmaps, invalid ID zero,
truncation, missing End Collection, excessive nesting and oversized fields. Fall back to a working
boot protocol only on an eligible boot interface. Absolute axes and vendor usages remain separate.

## USB.1 implementation boundary

`pci::first_xhci` reuses the bridge walk. Early boot reserves a distinct virtual window and uses
the boot memory map to reject aliases to RAM, firmware or framebuffer. The bootstrap page admits
the capability header and ownership chain, followed by a confirmed halt and BAR sizing. The gate
covers the first controller only, the first 256 KiB of BAR0, 4 KiB pages, at most eight slots and
32 scratchpads. An ownership chain outside the bootstrap page returns an error. These bounds
require hardware verification before use on other controllers.

After memory initialization, `DmaPage` supplies zeroed coherent DMA32 frames. DCBAA, scratchpad
pointers, ERST and the two aligned ring segments occupy disjoint parts of a single frame.
The host publishes the cycle bit last and correlates command completions to their physical TRB
address. The consumer advances ERDP after reading the complete event. Polling uses the ACPI PM
timer and an attempt bound. A failed start disables bus mastering and retains DMA on an
unconfirmed halt/reset. The controller stays optional. No HID data reaches the desktop in this slice.

## USB.6 design and acceptance

1. **Hub discovery.** Add a topology record containing controller, root port, parent slot/port,
   depth, route string, speed and TT hub slot/port/think time as applicable. A hub class driver
   reads the hub descriptor, powers ports with their power-good delay, monitors its interrupt
   bitmap, reads/clears individual change bits and resets downstream ports. Reuse root enumeration
   with a child topology record. Bound depth, ports and route nibbles. Test USB 2 single/multi-TT,
   nested hubs, unsupported topology, missing power and an unaffected sibling.
2. **Hotplug.** Route root port and hub notifications to a bounded scheduler work queue.
   Address devices with a slot plus generation. Transition through connected, enumerating,
   configured, disconnecting and retired states. Remove input bindings, release keys/buttons,
   stop endpoints and retire confirmed completions before returning DMA pages. Ignore stale
   completions after slot reuse. Test unplug during EP0/interrupt IN, reconnect storms, two
   keyboards and unaffected pointer input; compare per-cycle DMA, slot and queue counts.
3. **Recovery.** Track per-endpoint error codes, counters, timeouts and retry limits. Clear a
   halted device endpoint where appropriate, Reset Endpoint and Set TR Dequeue Pointer before
   resuming. Escalate to port reset/re-enumeration, then a bounded controller restart. Pin DMA
   until hardware quiescence is known. Test STALL, short packets, late completions, failed halt
   and controller failure with independent COM1 and desktop responsiveness evidence.
