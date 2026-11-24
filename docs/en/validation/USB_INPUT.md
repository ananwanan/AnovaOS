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
