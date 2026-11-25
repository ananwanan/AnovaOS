<!-- Define USB host, enumeration, HID and desktop responsibilities and their independent gates. -->
# USB input development

**English** | [简体中文](../../zh-CN/milestones/USB_INPUT.md)

The first target is a boot-connected keyboard and relative mouse on a q35 xHCI root port.
Initial work used `feat/usb-hid-input` from `dev`; the lifecycle follow-up uses `feat/usb-input-recovery`. This track extends M14 input and the M17 desktop
independently of M18 compiler reconstruction. See the [test guide and evidence](../validation/USB_INPUT.md).

## Ownership and event flow

| Layer | Owner and interface | Data passed upward |
| --- | --- | --- |
| Host controller | `kernel/src/drivers/xhci/`; owns the PCI function, mapped UC registers, rings, DMA pages and slot/context lifetime | Validated completion with the actual byte count, port and endpoint identity |
| USB Core | Owns device/configuration/interface/endpoint records and enumeration state. Submits setup packets and interrupt IN requests through the host | Interface descriptors, class control results and bounded report bytes |
| HID class | One binding per active interface, with its own protocol, previous report and per-ID layouts | Physical key transitions, relative motion, changed buttons and signed wheel delta |
| Input and desktop | Extend `anova-input::Dispatcher`; the kernel's scheduler poll serves existing GUI hardware events, compositor focus routes them to the window/PTY | Source-independent key/pointer events, layout and text handling in the existing consumer |

The implemented path starts when a device completes an interrupt IN transfer. The scheduler drains a
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
| USB.1 | Detect PCI 0c/03/30; own and bound MMIO, reset, DCBAA, command/event rings, ERST, slot and endpoint state. Verify command completion, ring wraps, slot release, timeouts, and absent-controller boot. | Controller/command slice and submitted EP0 failure cleanup implemented; bounded QEMU evidence separate from physical/IRQ acceptance. |
| USB.2 | Reset each connected root port per its supported protocol and speed. Enable Slot, prepare contexts for EP0, Address Device, read the first device bytes, evaluate the EP0 maximum packet size, then read the full device/configuration tree. Configure interrupt endpoints and issue Set Configuration. Bind each supported interface independently. | Implemented: contexts, Address Device, descriptors, EP0 packet evaluation, endpoint configuration and per-interface records; bounded QEMU acceptance. |
| USB.3 | Bind HID 03/01/01, issue Set Protocol(Boot), receive interrupt IN. Diff eight-byte keyboard reports, modifiers and simultaneous usages. Preserve previous keys on rollover/truncation. Cover letters, digits, editing keys, arrows, function keys, Shift/Ctrl/Alt, held and released states in the graphical PTY. | Implemented and verified in Boot QEMU desktop sessions; invalid reports preserve held state. |
| USB.4 | Bind HID 03/01/02, issue Set Protocol(Boot). Decode three button bits and signed relative X/Y from the boot prefix. Verify clipping, left/right window interaction, middle press/release, continuous movement and PS/2 coexistence. | Implemented and verified in Boot QEMU sessions with clipping, three buttons, focus, actual title drag and PS/2 coexistence. |
| USB.5 | Fetch the HID class descriptor and declared report descriptor. Parse short Main/Global/Local items, nested collections, Push/Pop, usage lists/ranges, report ID/size/count, logical bounds, Input array/variable flags, padding and independent bit offsets per report type and ID. Select Report Protocol only with a usable layout. | Implemented; bounded parser/state host matrix plus real QEMU descriptor/Report transport and terminal wheel consumer. ID/bitmap/composite/extra-button layouts have host evidence, not physical acceptance. |
| USB.6 | External USB2 hubs, runtime attach/detach and bounded fault recovery. | Implemented; separate hotplug/recovery gates and physical limits below. |

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
unconfirmed halt/reset. The controller stays optional. This section describes the historical controller-only slice; the current HID path follows below.

## USB.6 design and acceptance contract

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

## Root-port reset slice (2026-11-24)

At this historical slice, the bounded Supported Protocol walk rejected invalid/overlapping port ranges, unknown
major versions and custom speed-ID tables. Only connected ports are powered. PORTSC writes preserve
RW power/indicator/wake fields without echoing PED, reset strobes or RW1C change bits. USB 2 uses
port reset; USB 3 accepts an enabled U0 link or performs warm reset. PM-timer power/reset/recovery
waits are bounded; disconnect and bad speed fail closed. The QEMU gate verifies USB 2 ports only;
USB 3 reset remains unverified. This historical reset-only slice preceded the endpoint implementation below.

The first runtime check exposed HCE while publishing ERSTBA with PCI bus mastering disabled.
Initialize owned DMA pages, enable bus mastering, then publish controller pointers. This retains
the existing halt/reset/quarantine error path. See the validation record for actual evidence.


## USB.2–USB.5 implemented bounds (2026-11-25)

The opt-in USB driver delivers Boot/Report keyboards and relative mice through the existing desktop. USB.6 now implements cooperative root/hub attachment, held-input release, stopped-slot/DMA reclamation, bounded endpoint recovery and one controller restart. USB2 hubs have checked route/TT records; the total remains eight devices, including hubs. The validation record separates executed QEMU gates, host-only TT checks and pending physical coverage. Native/GUI public ABIs stay frozen. SuperSpeed hubs, arbitrary controller faults and physical hardware remain separate.

One controller, eight devices including hubs, four alternate-zero interfaces per device, one interrupt IN endpoint per HID interface, 64-byte packets, 2,048-byte configuration/report descriptors, eight Report IDs, 48 Input fields, 16 explicit usages per Main item, four pushed globals and eight collection levels are admitted. Usage ranges can describe larger keyboard bitmaps. Input/Output/Feature offsets are independent per ID. Unknown data fields and constant padding advance offsets; absolute axes, long items and local delimiters are outside this profile. Only the first configuration is selected. No strings or interrupt OUT/LED requests are issued.

EP0 starts at 8/64/512 bytes for low/full, high and SuperSpeed respectively; device prefix validation determines any Evaluate Context update. Address Device completion and output state are checked; no USB SET_ADDRESS is sent. Boot is selected before optional Report parsing. Parser/length rejection retains Boot on eligible interfaces. Report-descriptor STALL resets EP0 and skips the failed TD before Boot fallback; other failures use the bounded USB.6 retirement/recovery path. Interrupt NAK while idle has no artificial input timeout. HID layout allocation is fallible.

Hardware/USB 3 reset and 64-byte controller context support remain independently untested. Explicit PSI mappings are now supported for the bounded speeds below; TT hardware transport, SuperSpeed hubs, arbitrary recovery and unrestricted HID layouts are not accepted. See the validation record before using physical hardware.

Official references: [Intel xHCI 1.2b](https://cdrdv2-public.intel.com/625472/625472_xHCI_Rev1_2b.pdf), [USB-IF HID 1.11](https://www.usb.org/sites/default/files/hid1_11.pdf), [USB 2.0](https://www.usb.org/document-library/usb-20-specification).

Report selection on Boot interfaces also requires retaining keyboard input or both relative X/Y axes. Relative axis/wheel deltas are bounded to i16; unknown fields support checked signed/unsigned 32-bit logical values. QEMU acceptance covers full/high speed; low-speed physical devices remain independently pending.

The single DMA frame now contains 16 command TRBs and 64 event TRBs. A compile-time invariant leaves headroom beyond all 32 admitted interrupt requests plus 16 events. Scheduler drain stays at 16 per turn. The gate requires 130 No Ops, preserving at least two event-ring wraps as well as command-ring wraps and slot reclamation. Maximum 32-interface simultaneous traffic has capacity proof, not a QEMU saturation claim.

## USB.6 lifecycle implementation (2026-11-25)

One fallibly allocated, pinned Rust future owns the controller. Boot drives the same operations
before user startup; runtime polls them once per scheduler turn. Command/control submission,
power-good, reset and recovery waits yield. The PM timer bounds lost command/control completions
at one second; idle interrupt IN requests have no artificial timeout. Other HID completions are
handled while serialized commands or controls are pending. No IRQ, SMP or public ABI is added.

Root changes and hub interrupt bitmaps feed bounded topology scans. USB2 hubs allow 1–15 ports,
five route nibbles and at most eight total slots. Hub descriptors, speed/route/TT inheritance,
power-good delays, port status/change clearing and reset recovery are checked before addressing
a child. Hub metadata is published in the first Configure Endpoint command. Unsupported or
malformed devices are rejected without reallocating a live sibling's rings. The fixed diagnostic
QEMU timeout/STALL boot profiles retain their original controller-stop assertions.

Hub admission cross-checks device and interface protocol fields. High-speed hubs, including
multi-TT-capable hubs, stay on the default single-TT interface; no SET_INTERFACE is issued.
The xHCI MTT bit cannot be inferred from device capability alone. Selecting a multi-TT alternate
is outside this profile; synthetic topology tests cover inheritance without claiming transport.

Disconnect first clears every affected input source. Descendants retire deepest first. Running
endpoints are stopped, halted endpoints are reset, transfer events are drained, and Disable Slot
must complete before clearing DCBAA and returning DMA pages. A monotonic generation identifies
each lifetime; no slot/address is reused before quiescence. Retirement measures the exact freed
frame count. Input remains initialized when an empty controller is present, so the ordinary
no-input desktop can accept devices later. Queued releases precede the unavailable-input result.

Interrupt STALL/transaction errors clear held input and allow two endpoint recoveries using
the appropriate device clear-halt request, Reset/Stop Endpoint and Set TR Dequeue Pointer.
Exhaustion permits one port re-enumeration per attachment. A controller failure permits one
restart per boot, only after confirmed halt/reset; otherwise DMA is quarantined. Invalid report
bytes retain the prior report state. See the validation record for which fault paths were
exercised: implemented error handling is not a claim of arbitrary physical fault coverage.

Run `cargo xtask usb-hotplug-test` and `cargo xtask usb-recovery-test` alongside the original
controller, Boot/Report, PS/2, desktop and UART gates. The recovery-only image uses F12 to trigger
an actual unsupported EP0 request and then an actual controller halt; production images do not
contain these hooks. Test output stays under `build/usb-hotplug-test/` and
`build/usb-recovery-test/`, with previous attempts retained.

## No-input physical diagnostics (2026-11-25)

When both input devices are unavailable, the desktop now displays USB boot diagnostics automatically. No keyboard, mouse or COM1 is needed to read the panel. Connect devices before boot and photograph the panel; it is a boot snapshot, not a live log. Physical compatibility remains pending.

The panel is headed `USB BOOT DIAGNOSTICS V1`. It preserves the first USB identity plus the latest eleven boot lines, bounded to 126 printable ASCII bytes per line. The existing 32 KiB boot snapshot is read once with an owned descriptor; no user input or persistent disk is required. It appears only on the desktop when no input is available and disappears after a successful live attachment. `cargo xtask usb-diagnostics-test` checks missing controller, empty controller, actual malformed-hub rejection and resumed input using independent scanout glyphs. The diagnostic ISO is byte-identical to the production USB image and contains no fault-injection feature.

## Extended capabilities outside the first page (2026-11-25)

Firmware handoff maps only controller-advertised extended-capability pages as needed, with
UC/NX attributes, firmware MMIO admission and the existing 256 KiB virtual-window ceiling.
The initial ceiling is not a measured BAR size: probing the live firmware-owned BAR would
interrupt its decoding. After ownership transfer and confirmed halt, BAR sizing must cover
every bootstrap register touched before DMA setup can proceed. Both capability walks limit
work to 256 headers and reject invalid headers, bad alignment, overflow and out-of-range
links before the next access. The new `anova-usb::xhci` helpers are shared with host tests.
The screen now includes PCI vendor/device IDs, BAR/capability/runtime/doorbell offsets, legacy ownership and measured aperture where reached. A physical retest remains required; this does not add controllers, speed-ID tables or SuperSpeed hub support.

Scratchpad follow-up: the controller buffer limit is now 512 instead of 32. A separate owned 4 KiB DMA pointer array prevents overlap with command/event rings; zero buffers allocate no array. Allocation errors release unpublished buffers through ownership, while existing controller halt/quarantine rules retain published DMA. The boot log reports requested count, limit and raw HCSPARAMS2. QEMU-only checks construct 0/33/512 buffers, inspect every pointer and reject 513. This checks allocation/layout, not physical controller consumption. Hardware retest remains pending; controllers requiring more than 512 buffers remain unsupported.

## Open physical USB blocker (2026-11-25)

The latest owner photo from the scratchpad-fix image identifies PCI `00:14.0`, vendor/device
`8086:a12f`, class `0c0330`. Discovery reached xECP `0x8000`, legacy capability `0x846c`
with BIOS=0/OS=1, and a measured `0x10000` BAR. HCSPARAMS2 is `0x14200054` and the
scratchpad requirement is 34, below the new 512 limit. The previous register-range and
scratchpad admission failures were passed in this boot; DMA operation is not yet established.

**Historical failure (resolved on 2026-11-26):** `USB:REJECT phase=pci reason=port-protocol-speed-table`. At that time the Supported
Protocol parser rejected every nonzero Protocol Speed ID Count. HID enumeration
has not started, so physical keyboard/mouse usability remains unaccepted. Next implement
bounded Protocol Speed ID table parsing and speed interpretation for port reset and device
contexts, with malformed-table tests and independent QEMU/physical validation. Do not merely
remove the rejection or infer physical success from the existing QEMU gates. The photo does
not contain the speed-table entries, so their exact values remain unknown.
## Explicit USB protocol speed tables (2026-11-25)

The physical Intel 8086:A12F boot reached `port-protocol-speed-table` after reporting 34
scratchpads. The driver now parses up to 15 PSI DWORDs per Supported Protocol capability,
checks the BAR span and next-capability overlap, and retains a per-port ID-to-speed map.
Explicit tables do not inherit absent default IDs. USB2 1.5/12/480 Mb/s and symmetric
SuperSpeed 5 Gb/s are admitted; unsupported rates and asymmetric pairs remain unmapped.
Malformed IDs, duplicates, zero mantissas and broken pairs are rejected. Nonzero Protocol
Slot Type remains outside this profile. A connected port with an unsupported speed is
logged and skipped so it does not reject otherwise usable USB2 ports.

Root devices retain the actual PORTSC speed ID in Slot Context. USB2 hub children select
an advertised ID for their decoded speed; EP0 size, interval and TT decisions still use
the internal USB speed class. New `USB:PROTOCOL` and `USB:PORT_SKIP` logs expose admission.
Eight protocol groups share 128 bytes of maps plus 256 bytes of port indices; a ninth group is rejected. Existing device/DMA limits remain.
This does not establish SuperSpeedPlus, asymmetric transport or physical acceptance.
The next step is a new-image boot on the owner's machine with the same ports and full panel.

Reference: [Intel xHCI 1.2b sections 7.2.1–7.2.2](https://cdrdv2-public.intel.com/625472/625472_xHCI_Rev1_2b.pdf).
Synthetic host cases cover explicit/remapped IDs, equivalent rate encodings, no implied
fallback, unsupported faster rates, duplicate/zero IDs and malformed asymmetric pairs.
Stock QEMU tests exercise default-ID runtime paths; physical explicit-table DMA consumption
remains unverified. Evidence for this batch is retained in `build/usb-speed/`.

On 2026-11-26 the owner reported normal USB keyboard/mouse operation after testing the speed-table fix image (SHA-256 07b34f7cb0b7713e198bbe32e65bb5d329d6c6cefcf7017b844f570ab7a22cc7) on the previously photographed Intel 8086:a12f machine. This closes the reported no-input blocker for that setup. This is owner-reported physical evidence; other controllers, physical hub/hotplug/recovery stress, SuperSpeedPlus and unrestricted HID remain independently unverified. Main-push-only CI remains unchanged; no remote CI pass is claimed.
