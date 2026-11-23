<!-- Record USB evidence separately from planned host, QEMU and physical acceptance procedures. -->
# USB input validation

**English** | [简体中文](../../zh-CN/validation/USB_INPUT.md)

## Evidence at 2026-11-23

The [USB milestone](../milestones/USB_INPUT.md) records the implementation boundaries.
The remote baseline inspected was `cdaf5d9`. The Windows process launcher failed before starting
PowerShell (HRESULT 0x8007000F). Local working-tree status, Rust compilation, QEMU and physical
behavior could not be checked. No serial transcript, screenshot or passing runtime result was
obtained. Remote CI is independent and restricted to pushes to main.

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
-device usb-mouse,bus=xhci.0`; the second omits the controller. No interaction is needed for
this controller gate. Each boot has a 90-second watchdog. Inspect `build/logs/xhci.serial.log`,
`xhci.qemu.log`, `xhci.result.txt` and the corresponding `xhci-absent` files.

Expected on the first boot: a PCI class match, a No Op completion, 34 completed No Ops in total
across command and event wraps, a valid enabled slot and its Disable Slot completion. Expected on
the second: an absent-controller marker and the normal boot/storage suite. Both require QEMU exit
33, the standard smoke evidence and no USB rejection or quarantine. `[USB:REJECT]` includes the
failing phase and reason; diagnose BAR/ownership before ring setup and inspect QEMU stderr on a
watchdog. Ring readiness does not establish root enumeration or keyboard input.

## USB.2–USB.5 acceptance to run after implementation

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
