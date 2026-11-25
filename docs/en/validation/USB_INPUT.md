<!-- Record USB evidence separately from planned host, QEMU and physical acceptance procedures. -->
# USB input validation

**English** | [简体中文](../../zh-CN/validation/USB_INPUT.md)

## Evidence at 2026-11-23

The [USB milestone](../milestones/USB_INPUT.md) records the implementation boundaries.
The remote baseline inspected was `cdaf5d9`. The Windows process launcher failed before starting
PowerShell (HRESULT 0x8007000F). Local working-tree status, Rust compilation, QEMU and physical
behavior could not be checked. No serial transcript, screenshot or passing runtime result was
obtained. Remote CI is independent and restricted to pushes to main.

## Local follow-up at 2026-11-24

`origin/dev` and local `dev` both resolved to `cdaf5d9`; the clean USB branch `719f4e5`
already contained that baseline. Fetch and merge of `origin/dev` required no changes.
The launcher now works through Ubuntu-24.04 WSL.

The initial real QEMU boot failed the USB gate with `phase=start reason=controller-fault`.
An instrumented rerun reported `status=0x1000 command=23` (HCE). ERSTBA publication can
fetch the event segment table while the controller is halted. Enabling PCI bus mastering
before publishing ERSTBA, after initializing owned DMA, fixed this failure. The ordinary
boot/storage suite had reached `[TEST:PASS]` even during the USB failure; the USB gate
correctly rejected it.

The controller command-ring and root-port reset slices passed the three-boot gate:
keyboard/mouse attached, empty xHCI controller, and no xHCI controller. Each completed
with QEMU exit 33, no watchdog, the normal runtime/storage evidence and `result=Ok(())`.
The attached boot reported ports 5 and 6, both `protocol=2 speed=3 enabled=true`, then
`connected=2 reset=complete`. The empty boot reported `connected=0 reset=complete`;
the absent boot reported `[USB:ABSENT] class=0c0330`. The command-ring boots each
completed 34 No Ops across wraps and an Enable/Disable Slot cycle.

The host gate rejects duplicate/out-of-range ports, wrong protocol/speed, disabled ports,
missing/count-mismatched reset evidence, quarantine, bad exit and timeout. Full xtask host
tests, strict xtask/kernel Clippy, the non-test xHCI kernel build, formatting, documentation,
file-purpose and whitespace checks are run for this batch. Physical hardware, USB 3 reset,
malformed-controller injection, endpoint timeouts and all HID reports remain unverified.
This accepts the command-ring slice and USB.2 reset prelude, not all of USB.1 or USB.2.

## Controller gate

On the supported Linux/WSL development host with the pinned tools and locked firmware:

```sh
cargo xtask doctor
cargo xtask fetch
cargo fmt --all -- --check
cargo build --locked -p anova-kernel --target x86_64-unknown-none --features xhci
cargo clippy --locked -p anova-kernel --target x86_64-unknown-none --features xhci,qemu-test -- -D warnings
cargo test --locked -p xtask xhci_gate
cargo xtask xhci-test
python3 scripts/check_docs.py
cargo xtask check-files
git diff --check
```

The new test uses the existing q35/TCG, OVMF, temporary disk, serial capture and debug-exit
setup. Its first boot attaches `-device qemu-xhci,id=xhci -device usb-kbd,bus=xhci.0
-device usb-mouse,bus=xhci.0`; the second attaches only the controller; the third omits it.
No interaction is needed for this controller gate. Each boot has a 90-second watchdog.
Inspect `build/logs/xhci.serial.log`,
`xhci.qemu.log`, `xhci.result.txt` and the corresponding `xhci-empty` and `xhci-absent` files.

Expected on the first boot: a PCI class match, a No Op completion, 130 completed No Ops in total
across command and event wraps, a valid enabled slot and its Disable Slot completion. Expected on
the second: a zero connected-port count; on the third: an absent-controller marker and the normal
boot/storage suite. All three require QEMU exit 33, the standard smoke evidence and no USB rejection
or quarantine. `[USB:REJECT]` includes the
failing phase and reason; diagnose BAR/ownership before ring setup and inspect QEMU stderr on a
watchdog. The attached boot additionally requires two distinct enabled USB 2 root ports.
Root-port readiness does not establish device addressing, descriptor enumeration or keyboard input.

The current `xhci-test` additionally requires the submitted timeout and real STALL boots. `xhci-packet-test` separately requires EP0 output-context packet-size readback.

## USB.2–USB.5 acceptance procedure

- Boot an actual desktop image with the three device arguments above. Repeat with
  `-machine i8042=off` to isolate USB. Record VID/PID, interface, endpoint, protocol and actual
  transfer completions; in a separate PS/2-only boot run `cargo xtask input-test`.
- Type `echo Aa1` in the focused graphical terminal and verify its PTY output and exit status.
  Check Shift, Ctrl, Alt, editing keys, arrows, function keys, chords, release and repeat. Hold
  Shift on a second keyboard while releasing the first. Read the resulting text back from the PTY.
- Check relative motion, clipping, left/right/middle press and release, focus and a visible window
  action. Measure interaction latency while moving continuously and retain independent QMP scanout.
  Exercise a signed wheel on a descriptor that advertises it; inspect the consuming window effect.
- Run host parser tests for descriptor chains, boot report differences/rollover, signed axes,
  independent report IDs, non-byte-aligned fields, unknown usages, truncated packets, overflow
  and excessive depth. Record the fixture and its expected transition sequence.
- Run `cargo xtask desktop-test` and `cargo xtask desktop-pty-test --without-toolchain`.
  Verify the ordinary no-input desktop and independent COM1 in a boot with both input sources
  absent, and preserve the desktop-test missing-device exit behavior. Retain logs per topology.

## Physical machine checklist — pending

1. From an existing OS record the PCI BDF/class (e.g. `lspci -nn -d ::0c03`), controller
   model, BAR0, firmware version, USB topology and device VID/PID. Check for class 0c/03/30.
2. Record UEFI, Secure Boot, xHCI handoff and legacy USB settings where available. Use the
   project's existing UEFI boot procedure, a separate test medium and an independent serial
   console. Boot with a wired keyboard and mouse connected directly to root ports.
3. Compare UART against the controller and enumeration milestones before testing desktop input.
   A firmware-responsive keyboard alone does not exercise the AnovaOS driver. Repeat with USB
   disconnected before boot and with both PS/2 and USB where available.
4. After USB.5, repeat with a composite keyboard and a wheel mouse. Keep serial logs, controller
   IDs, configuration, observed PTY bytes and window behavior. External hubs and unplug/replug are
   later USB.6 gates. No physical machine has been accepted in this development batch.


## USB.2–USB.5 follow-up (2026-11-25)

The fetched `origin/dev` was `cdaf5d9`; `feat/usb-hid-input` already contained it. A fast-forward-only merge was a no-op. All 20 existing modified files were retained. No reset, force-push, physical block write or Actions trigger change was performed. Current source, controller behavior and tests were rechecked; the earlier HCE/reset record above is historical context.

The final Boot and Report desktop rounds each passed USB-only with i8042 disabled, coexistence, a full-speed USB pair, PS/2-only and no input. Exact bytes/status, geometry, cursor pixels, latency and failure assertions passed. Final commands/results are recorded below. QEMU supplies actual descriptors and transfer completions; no pre-generated report log or screenshot substitutes for transport.

Evidence directories:

- `build/usb-input/frozen-checks.json`: command exit codes, durations and complete log paths.
- `build/logs/xhci*.serial.log`, `.qemu.log`, `.result.txt`: controller, enumeration, empty/absent, submitted EP0 timeout and actual unsupported-descriptor STALL. Timeout/STALL require confirmed halt/reset and exact free-frame restoration, ordinary OS/runtime/storage proof and QEMU exit 33.
- `build/usb-boot-test/` and `build/usb-input-test/`: each topology has its own command, UART, QMP, result, PPM scanout and exact exported PTY files. `attempts/<timestamp>/` preserves subsequent UART/QMP/results/screenshots before reruns; original disk files are kept. Some early USB UART/QMP files were overwritten before this archive was installed; original failed command logs and PNG screenshots remain. The final accepted rounds retain complete UART/QMP.
- `build/usb-input/physical-availability.json`: WSL USB/serial visibility and the explicit pending physical status.

The graphical test physically types `echo Aa1`, verifies rendered glyphs within five seconds, exports exactly `Aa1\n`, requires echo status `0\n` and Ctrl+C child status `130\n`, and inspects the files independently with debugfs after clean ACPI shutdown/e2fsck. The font renders uppercase glyphs, so scanout checks use `AA1` while file inspection retains exact mixed case. Key make/break acknowledgements bound host injection; timeout or missing acknowledgements fail the gate without input retries. The USB-only matrix checks editing/navigation/function keys, Shift/Ctrl/Alt, simultaneous keys, scheduler repeat and final release. Mouse evidence covers continuous movement, all clipped cursor pixels, three-button press/release, focus and title drag with exact resulting geometry and scanout pixels. Report wheel events change terminal history offsets `10 → 0` with corresponding scanout.

Host fixtures cover descriptor truncation/zero length/count/endpoint bounds, composite/alternate interfaces, Boot rollover/reserved/truncation, per-source aggregation, independent report IDs, unknown fields, constant padding, signed unaligned extraction, arrays/bitmaps, Push/Pop, collections and size/depth/usage/report bounds. Real QEMU currently exercises ordinary Boot keyboard and relative wheel-mouse layouts; Report ID/NKRO/composite/extra-button fixtures are host evidence, not claims of modeled or physical device transport. USB 3 reset, 64-byte context hardware, unconfirmed-halt injection, arbitrary HID layouts and USB.6 remain unverified.

Failures retained: `build/usb-input/enumeration-first.log` has the initial Rust borrow-check failure; `enumeration-second.log` has the successful enumeration round. Prior Boot attempts retain the disabled-i8042 polling bug, uppercase-font scanout mismatch, and overrun-prone batched input/status failure. The implementation now skips absent PS/2 ports, retains source-specific held state and falls back to an earlier held repeat key; the harness waits for each actual make/break before the next character. Failed intermediate watchdog/repeat runs and command-launch failures are not acceptance. No runtime assertion was relaxed to make a failure pass.

**Physical hardware pending.** No USB bus or serial-by-id path was visible in this WSL session, and no authorized physical test machine plus independent recovery path was identified. No physical boot medium was created or modified. Step 5 is not fully complete; main-push-only remote CI remains separate and was not run on this feature branch.

Additional reproduced failures and fixes: `build/usb-input/queue-overflow-original.log` / `queue-overflow-fixed.log`, `unsigned-original.log` / `unsigned-fixed.log`, and `usability-original.log` / `usability-fixed.log` preserve the pre-fix assertion and post-fix host result. `ps2-original/` retains the complete missing-controller regression; `ps2-fixed.log` records the unchanged PS/2 assertions passing after restoring early fallback. `attempt-retention.log` verifies byte-exact failed-UART archival and clearing stale success before a new run.

## Final executed checks (2026-11-25)

| Check | Result (all passed) |
| --- | --- |
| `cargo xtask xhci-test` | 5 boots; 130 No Ops, two event wraps, slots, enumeration, absent/empty, timeout/STALL and exact frame restoration |
| `cargo xtask xhci-packet-test` | 1 boot; actual EP0 8→64 context update, VID 0525/PID a4a2, configuration 2, two unbound interfaces |
| `cargo xtask usb-input-test --boot` | 5 independent topologies; full/high speed Boot keyboard/mouse |
| `cargo xtask usb-input-test` | 5 independent topologies; Report keyboard/mouse and consumed wheel history |
| `input-test`, `gui-test`, `desktop-test` | 3 input boots, isolated GUI regression, six supervised desktop boots |
| `desktop-pty-test --without-toolchain` | 2 boots; real PTY, interrupt, resize, pressure, bytes and reboot |
| `terminal-test`, `persistence-test`, `verify` | UART/reboot, storage/error recovery, all 14 runtime/fault cases |
| `test-host`, `check-apps`, `test_kernel_prepare.py` | Full host matrix, native application Clippy and source preparation rejection checks |
| `gpu-fallback-test` | Headless fallback regression; physical GPU gate not rerun |
| Strict host/kernel Clippy | 15 host packages and 12 kernel profiles, then changed-profile rechecks; -D warnings |
| `usb-image`, formatting, generated GUI header | Production USB desktop ISO built; workspace/user format and unchanged generated C ABI header checked |

`build/usb-input/final-checks.json` retains the initial full regression run and its subsequently fixed Clippy test-module ordering failure. `quality-checks.json` records the strict corrected run. `followup-checks.json` records EP0 follow-up and a fresh successful dev fetch/ancestry check. `capacity-checks.json` records the final 64-entry/130-command USB rerun. The original `clippy-host-original.log` is retained. Final source hashes and documentation checks are in `validation-summary.json` and `delivery-checks.json`.

The EP0 fixture is QEMU 8.2.2 `usb-net` with no network backend: only descriptors/configuration and control transport are exercised; it remains unbound. [Model definition](https://github.com/qemu/qemu/blob/v8.2.2/hw/usb/dev-network.c). No network capability is accepted here. Ring saturation at all 32 interfaces, low-speed physical devices, USB 3 reset and 64-byte physical contexts remain independent. Network/package/compiler/bootstrap and the long GUI Files-fault suites were preserved but not rerun by this input batch. Main-push-only workflow files remain unchanged.

## USB.6 implementation and local acceptance (2026-11-25)

This batch starts from clean `dev` at `770b881` on `feat/usb-input-recovery`. It adds a
cooperative USB worker, USB2 hub topology, live root/hub enumeration, held-state release,
confirmed endpoint/slot retirement and bounded recovery. No physical medium or real block
device was written; main-push-only Actions and the native/GUI public ABIs are unchanged.

The new gates use q35/TCG, the locked firmware, independent UART/TCP-QMP, fresh ext4 files,
180-second session watchdogs and five-second interaction/attachment ceilings. A successful
run must exit through normal guest shutdown, pass e2fsck and export exact PTY file bytes.
`build/usb-hotplug-test/` and `build/usb-recovery-test/` retain commands, serial/QMP logs,
PPM scanout, exported bytes, results and previous attempts. `build/usb-lifecycle/` retains
build/check logs, including failed development attempts. The original USB.1–USB.5 evidence
above remains historical and is not used as a substitute for this batch's reruns.

Host topology tests reject truncated/invalid descriptors and out-of-bounds depth/ports,
verify nearest-TT inheritance, speed selection and descendant membership. Lifetime
evidence tests reject stale generations, active-slot reuse, missing releases and incorrect
DMA/page counts. Production input capabilities remain bounded by the declared parser.

Final protocol review tightened hub admission to check both device and interface protocol
fields. The default interface on a multi-TT-capable high-speed hub is still single TT;
unselected multi-TT and inconsistent protocol tuples are rejected. Host fixtures cover all
three admitted tuples and seven rejected tuples (`tt-default-protocol.log`). Synthetic
multi-TT inheritance is host-only; alternate selection is not implemented. The source is
[USB 2.0 section 11.23.1](https://www.usb.org/document-library/usb-20-specification).

Development failures retained: `hotplug-first.log` records a QMP key-up request after the
only keyboard was deleted; `hotplug-second.log` records a frame-count type mismatch;
`hotplug-third.log` records waiting for an already-released button event. The test now
checks the release at removal and performs a new press/release after reattachment.
`hotplug-fourth.log` records rejection of QEMU 8.2.2's default eight-port hub descriptor:
its PortPwrCtrlMask is one byte short. The parser keeps the strict USB2 length check;
normal hub scenarios use four ports with `port-power=on`. `hotplug-fifth.log` records
successful root/nested-hub cycles. `hotplug-final.log` records that QEMU's `attached`
property is readonly; the subtree test uses real `device_del`/`device_add` instead.
`recovery-first.log` retains a host-driver visibility compile error; `recovery-second.log`
records successful actual EP0 STALL and controller halt/restart recovery.

QEMU's full-speed hub model does not implement high-speed transaction translation; TT
records have host evidence only. Physical low-speed/full/high-speed devices, USB3 hubs/reset,
64-byte physical controller contexts, arbitrary interrupt-endpoint STALL/transaction faults,
failed-halt DMA quarantine injection, all-32-interface saturation and power-loss behavior
remain outside this acceptance. A successful controlled restart does not prove recovery from
arbitrary controller corruption. Keyboard LEDs/output reports and unrestricted HID layouts
remain separate. No hardware or remote CI acceptance is claimed.

References for the exact models: [QEMU 8.2.2 hub](https://github.com/qemu/qemu/blob/v8.2.2/hw/usb/dev-hub.c)
and [USB device lifetime](https://github.com/qemu/qemu/blob/v8.2.2/hw/usb/bus.c).

### Executed USB.6 gates

| Command | Observed result |
| --- | --- |
| `cargo xtask usb-hotplug-test` | Passed two independent boots: empty root controller and two powered four-port hubs. Each performed ten held-Shift removal/reconnect cycles, actual button release, surviving mouse/UART interaction, exact frame reclamation and slot reuse beyond eight lifetimes. The hub boot additionally removed/rebuilt the whole subtree with held key/button state, then removed an empty nested hub while its sibling remained usable. Exact `before\n`/`after\n` files, scanout, e2fsck and normal shutdown passed. |
| `cargo xtask usb-recovery-test` | Passed one independent boot: actual malformed-hub rejection with sibling input retained; unsupported EP0 request returned STALL, Reset Endpoint/Set TR Dequeue Pointer enabled an actual descriptor retry; a real controller halt with outstanding interrupt TDs recovered through one confirmed reset/restart. Independent UART, keyboard/mouse, scanout and exact `control\n`/`restart\n` files passed. |

Initial successful new-gate logs are `build/usb-lifecycle/hotplug-cascade.log` and
`recovery-final.log`. After the protocol review, both complete gates passed again in
`followup-hotplug.log` and `followup-recovery.log`, with all three independent boots and
per-session results `Ok(())`. `followup-checks.json` retains these final commands and timings.
WSL exposes no USB
bus devices or `/dev/serial/by-id` entries in `build/usb-lifecycle/physical-availability.json`.
The recovery gate's endpoint STALL is EP0, not a simulated claim about interrupt-endpoint
STALL; runtime removal during an unobserved EP0 transfer remains separately unverified.

### USB.6 regression reruns

| Check | Observed result |
| --- | --- |
| `cargo xtask xhci-test` | Passed all five boots, including 130 No Ops, slot reuse, absent/empty controller and actual timeout/STALL cleanup. |
| `cargo xtask xhci-packet-test` | Passed the actual EP0 8-to-64-byte Evaluate Context and descriptor/configuration transfers. |
| `cargo xtask usb-input-test --boot` and `cargo xtask usb-input-test` | Passed five independent topologies per mode, including keyboard, mouse, Report wheel, exact PTY bytes and scanout. |
| `cargo xtask input-test`, `cargo xtask gui-test` | Passed independent PS/2, compositor input/fault/restart and missing-device fallback checks. |
| `cargo xtask desktop-test` | Passed all six supervised desktop boots and persisted bytes. |
| `cargo xtask desktop-pty-test --without-toolchain` | Passed two boots covering real PTY input, Ctrl+C, resize, bounded history, pressure and reboot. |
| `cargo xtask terminal-test` | Passed independent serial editing, CRLF/EOF and fresh-boot checks. |
| `cargo xtask verify` | Passed all 14 runtime and controlled-fault scenarios with their required exit classifications. |
| `cargo xtask test-host` | Passed the complete host matrix, including USB descriptor/HID/topology logic, source lifetimes, resource exhaustion and sector/flush journal cuts. |
| Strict Clippy | Passed `xtask`, `anova-usb` and `anova-input` with all targets, plus kernel `desktop,xhci` and `desktop,usb-recovery-test`, all with `-D warnings`. |
| `cargo xtask usb-image` | Built the production USB desktop ISO at `build/images/anova-usb-desktop.iso`. |
| Formatting, documentation and file purposes | Workspace/user formatting, 90 paired pages, 189 Markdown files, 587 purpose entries and `git diff --check` passed. Modified-file English purpose/safety explanations and LF endings were reviewed. |

`build/usb-lifecycle/regressions.json` records commands, durations, exit codes and complete
logs. Network/package/compiler/bootstrap, physical GPU and long GUI Files-fault suites
remain preserved but were not rerun in this lifecycle batch. Remote CI remains deferred
under the unchanged main-push-only policy.

Final ISO extraction checks in `build/usb-lifecycle/image-identities.json` confirm that all
three packaged kernels contain the hub/retirement implementation. The production and hotplug
images exclude the recovery injection markers; only the recovery image includes them.
`frozen-source.json` fixes the 15 changed implementation/manifests before the final gates.
`validation-summary.json` verifies those hashes still match and records all 36 modified/new
file hashes, the three ISO hashes, packaged kernel hashes, successful checks and physical
availability. `delivery-checks.json` retains the final documentation/purpose/whitespace checks.

## No-input screen diagnostic follow-up (2026-11-25)

The owner reports a physical machine with USB keyboard/mouse still showing no input, with no
usable COM1 or PS/2. This is an unresolved physical failure, not a passing hardware result.
The compositor now exposes a bounded USB boot snapshot automatically, without adding a driver
capability or changing native/GUI ABIs. It reads at most 32 KiB once, keeps the first USB line
and latest eleven, sanitizes non-printable bytes, and caps each line at 126 bytes. The overlay
is explicitly a boot snapshot; it does not capture later runtime errors.

For a machine without COM1/PS/2:

1. Use the new `build/images/anova-usb-diagnostics.iso`, generated and checked by
   `cargo xtask usb-diagnostics-test`. It is byte-identical to the production USB desktop ISO.
2. Connect wired keyboard/mouse directly before boot. Initially bypass an external hub/dock.
3. If input remains unavailable, photograph the entire screen including the bottom
   `USB BOOT DIAGNOSTICS V1` panel. Record the machine/motherboard model and physical port group.
4. Read the recorded stage: `USB:ABSENT` means no selected xHCI; `USB:REJECT` reports controller
   discovery/start failure; `USB:ATTACH_REJECT` reports device enumeration rejection;
   `USB:HID ... bound=true` establishes initial class binding only. Preserve the complete
   `phase=`/`reason=` line and PCI BDF. The driver currently selects only the first xHCI;
   it does not implement EHCI/OHCI/UHCI. Port color alone does not identify its controller.

The production diagnostic gate passed three independent q35/TCG boots: no controller, an empty
controller, and the actual malformed eight-port hub descriptor. Independent glyph/palette
checks require the visible title plus `ABSENT`, `CONNECTED=0`, or
`HUB-DESCRIPTOR-PORTS-LENGTH`. The empty session then adds real modeled keyboard/mouse,
opens the graphical terminal and requires the panel to disappear. Each session finishes with
normal shutdown and e2fsck. The five-second interaction and 180-second session limits remain.

Evidence: `build/usb-diagnostics-test/result.txt`, retained UART/QMP/PPM and exact ISO SHA;
`build/usb-screen/diagnostics-second.log` is the passing build/run log. The initial missing
host-test import remains in `diagnostics-first.log`. Rendered PNGs in `build/usb-screen/`
were visually reviewed; the original PPMs remain unchanged. No physical boot device was written
by this follow-up. The subsequent owner photo and driver fix are recorded below; this section
retains the earlier screen-only acceptance scope.

Follow-up checks also passed `cargo xtask usb-hotplug-test` (two boots),
`cargo xtask desktop-test` (six boots), `cargo xtask gui-test`, `cargo xtask check-apps`
and strict all-target `xtask` Clippy. Their logs are retained under `build/usb-screen/`.
This changes the compositor and diagnostic harness, not the USB kernel driver. The earlier
USB.6 acceptance and its source/image hashes remain historical; the new image identity is
recorded separately in `build/usb-screen/validation-summary.json`.

## Physical register-range follow-up (2026-11-25)

The owner's diagnostic-screen photo establishes PCI class `0c0330` at `00:14.0` and
`USB:REJECT phase=pci reason=register-range`. It does not identify the controller vendor,
the failing offset, or an enumerated HID device. Code inspection found that firmware handoff
incorrectly restricted the extended-capability walk to the first 4 KiB. That rejects valid
controllers advertising higher capabilities. The photograph alone does not prove which
register-range check failed; the rebuilt image adds the missing geometry and PCI identity.

The fix follows the advertised xECP chain with checked on-demand UC/NX page mapping under the
existing 256 KiB window. Firmware MMIO exclusions and the 52-bit physical-address bound remain.
Ownership transfer still precedes controller halt and BAR sizing; no DMA is allocated until
all touched bootstrap spans fit the measured BAR. Both walks cap work at 256 headers and reject
invalid headers/links. The pre-handoff ceiling is explicitly an admission bound, not proof of
BAR size; this step relies on the controller advertising its own capability registers correctly.

Primary references: [Intel xHCI 1.2b, sections 5.3.6 and 7](https://cdrdv2-public.intel.com/625472/625472_xHCI_Rev1_2b.pdf)
and the [Intel 500 Series on-package PCH register map](https://cdrdv2-public.intel.com/631120/631120-002.pdf).
The latter documents capabilities at `0x8000` and legacy ownership at `0x846c`; these are
reference geometries, not a claim that the owner's machine is that model.

Five added host cases cover a synthetic `0x8000` chain reaching `0x846c`, rejection before an
out-of-range callback, invalid/empty/unknown headers, the chain-work ceiling, cross-page spans,
alignment/overflow and a smaller subsequently measured BAR. Existing QEMU controllers keep
capabilities in the first page, so normal QEMU passes do not exercise a physical high-offset
MMIO capability or firmware ownership handoff. Hardware retest remains pending.

Validation results and source/image identities are recorded separately under
`build/usb-registers/`; earlier USB.6 and screen-only records retain their historical scope.
No physical boot medium is written by the build. Reuse the same ports and firmware settings
for the first new-image test, connect keyboard/mouse before power-on, and retain the full panel
if enumeration still fails. Native/GUI ABIs, input limits and main-push-only CI remain unchanged.

The initial `xhci-test-first.log` completed controller initialization, both HID bindings and
guest `[TEST:PASS]`, but the host rejected the changed PCI log field order. The diagnostic
identity now preserves the existing terminal `class=0c0330` field, without relaxing the
host's required markers. `checks-first.json` retains that failure separately from the rerun.

The complete rerun in `build/usb-registers/checks.json` passed all 20 USB host tests and
strict Clippy for USB/xtask and both production/recovery kernel profiles. QEMU passed 22
independent boots: `xhci-test` (five), `xhci-packet-test` (one), `usb-input-test --boot`
(five), `usb-input-test` (five), `usb-hotplug-test` (two), `usb-recovery-test` (one) and
`usb-diagnostics-test` (three). This includes unchanged PS/2-only/coexistence/no-input
profiles and actual control timeout/STALL, slot retirement and controller restart.

`fmt-first.log` retains the final PCI-log argument wrapping correction. Only formatting
changed after the full matrix; `tested-source-before-format.json` preserves its source
hashes. `delivery-checks.json` records checks and a fresh production diagnostic run after
formatting. `validation-summary.json` verifies final source hashes, byte-identical production
and diagnostic ISOs, extracted kernel identity and absence of recovery injection markers.
The five new high-offset/boundary cases are host-only. No new physical or remote CI pass
is inferred, and unrelated network/package/compiler suites were not rerun for this fix.

Scratchpad follow-up: the controller buffer limit is now 512 instead of 32. A separate owned 4 KiB DMA pointer array prevents overlap with command/event rings; zero buffers allocate no array. Allocation errors release unpublished buffers through ownership, while existing controller halt/quarantine rules retain published DMA. The boot log reports requested count, limit and raw HCSPARAMS2. QEMU-only checks construct 0/33/512 buffers, inspect every pointer and reject 513. This checks allocation/layout, not physical controller consumption. Hardware retest remains pending; controllers requiring more than 512 buffers remain unsupported.

Scratchpad validation: cargo xtask xhci-test passed five boots with actual kernel 0/33/512-buffer layout checks and 513 rejection; cargo xtask usb-recovery-test passed. Production diagnostic boots and strict production kernel Clippy passed. Logs are retained under build/usb-scratch/; the higher-count checks do not emulate hardware DMA consumption of scratchpads or establish physical acceptance.

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

The first QEMU attempt hit the kernel stack guard after copying a 4 KiB per-port map through asynchronous frames. The implementation now shares eight group maps and uses byte port indices (384 bytes total). The original failure remains in build/usb-speed/xhci-first.log and checks-first.json; no stack limit was raised.

The final speed-map batch passed 22 USB host tests, strict USB/xtask and production/recovery kernel Clippy, and 17 QEMU boots: xhci-test (5), xhci-packet-test (1), usb-input-test (5), usb-hotplug-test (2), usb-recovery-test (1), usb-diagnostics-test (3). Boot-only mode and unrelated OS/compiler suites were preserved but not rerun in this batch. Formatting, paired documentation, 590-file purpose coverage and whitespace checks passed. checks.json and delivery.json retain commands, outcomes and final source/image hashes under build/usb-speed/. The delivered production image excludes test-injection markers.

On 2026-11-26 the owner reported normal USB keyboard/mouse operation after testing the speed-table fix image (SHA-256 07b34f7cb0b7713e198bbe32e65bb5d329d6c6cefcf7017b844f570ab7a22cc7) on the previously photographed Intel 8086:a12f machine. This closes the reported no-input blocker for that setup. This is owner-reported physical evidence; other controllers, physical hub/hotplug/recovery stress, SuperSpeedPlus and unrestricted HID remain independently unverified. Main-push-only CI remains unchanged; no remote CI pass is claimed.
