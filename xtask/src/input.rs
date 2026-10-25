// SPDX-License-Identifier: Apache-2.0
//! Exercise the polled PS/2 diagnostic consumer through actual QMP keyboard and pointer events.
//! Compare cursor scanout with every reference pixel, preserve evidence, and require COM1 recovery.
use super::*;
use anova_graphics::{CURSOR_HEIGHT, CURSOR_WIDTH, Surface, cursor_color};
use graphics::{Qmp, parse_ppm, read_ppm};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{io::Write, process::ChildStdin};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;
const DEADLINE: Duration = Duration::from_secs(60);
const READY: &str = "[INPUT:READY] keyboard=true pointer=true focus=diagnostic queue=64";
const SELFTEST: &str =
    "[INPUT:SELFTEST] overflow=true malformed=true client_reconnect=true device_reconnect=true";

#[derive(Clone, Copy)]
struct Profile {
    name: &'static str,
    display: bool,
    controller: bool,
}

const PROFILES: [Profile; 3] = [
    Profile {
        name: "input",
        display: true,
        controller: true,
    },
    Profile {
        name: "input-no-display",
        display: false,
        controller: true,
    },
    Profile {
        name: "input-no-controller",
        display: true,
        controller: false,
    },
];

pub fn verify(root: &Path) -> Result<()> {
    let artifacts = directory(root, "input")?;
    let result = artifacts.join("input-test.result.txt");
    output_path(&result)?;
    fs::write(&result, "result=Running\n")?;
    for profile in PROFILES {
        let path = artifacts.join(format!("{}.result.txt", profile.name));
        output_path(&path)?;
        fs::write(path, "result=Pending\n")?;
        for position in [
            "center",
            "top-left",
            "top-right",
            "bottom-left",
            "bottom-right",
        ] {
            let path = artifacts.join(format!("{}-{position}.ppm", profile.name));
            output_path(&path)?;
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
    }
    let outcome = (|| -> Result<()> {
        let iso = image(root, Mode::Input)?;
        for profile in PROFILES {
            session(root, &iso, &artifacts, profile)?;
        }
        Ok(())
    })();
    fs::write(
        &result,
        match &outcome {
            Ok(()) => "result=Ok\nscenarios=input,input-no-display,input-no-controller\n".into(),
            Err(error) => format!("result=Err\nerror={error}\n"),
        },
    )?;
    outcome?;
    println!(
        "Verified PS/2 key state, repeat, extended keys, pointer/buttons, all clipped cursor pixels, bounded recovery, missing devices, COM1 recovery and clean ACPI shutdown"
    );
    Ok(())
}

fn session(root: &Path, iso: &Path, artifacts: &Path, profile: Profile) -> Result<()> {
    let serial = artifacts.join(format!("{}.serial.log", profile.name));
    let diagnostic = artifacts.join(format!("{}.qemu.log", profile.name));
    let qmp_log = artifacts.join(format!("{}.qmp.log", profile.name));
    let result = artifacts.join(format!("{}.result.txt", profile.name));
    for path in [&serial, &diagnostic, &qmp_log, &result] {
        output_path(path)?;
    }
    fs::write(&result, "result=Running\n")?;
    let vars = Temporary::new(root, "input")?;
    let socket = vars.0.join("qmp.sock");
    let mut command = qemu(root, iso, &vars.0, false)?;
    command.args(["-vga", if profile.display { "std" } else { "none" }]);
    if !profile.controller {
        command.args(["-machine", "i8042=off"]);
    }
    command
        .arg("-qmp")
        .arg(format!("unix:{},server=on,wait=off", socket.display()))
        .args(["-serial", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(File::create(diagnostic)?);
    let mut evidence = format!(
        "scenario={}\nqemu={command:?}\nprofile=q35,accel=tcg cpu=qemu64,+rdrand memory=256M smp=1 display=none vga={} i8042={}\niso_sha256={:x}\nkernel_sha256={:x}\n",
        profile.name,
        if profile.display { "std" } else { "none" },
        profile.controller,
        Sha256::digest(fs::read(iso)?),
        Sha256::digest(fs::read(
            root.join("target").join(TARGET).join("debug/anova-kernel")
        )?),
    );
    println!("Running {}: {command:?}", profile.name);
    let outcome = (|| -> Result<()> {
        let mut guest = Guest(command.spawn()?);
        let output = transcript::Transcript::capture(
            guest.0.stdout.take().ok_or("Missing UART output pipe")?,
            File::create(serial)?,
        )?;
        let uart = guest.0.stdin.take().ok_or("Missing UART input pipe")?;
        let start = Instant::now();
        let qmp = Qmp::connect(&socket, &qmp_log, start)?;
        let mut driver = Driver {
            guest,
            output,
            uart,
            qmp,
            start,
            observed: 0,
        };
        driver.wait("anova> ")?;
        // The ordinary shell must have reached its blocking read before serial exchanges begin.
        thread::sleep(Duration::from_millis(80));
        let boot = driver.output.text();
        if profile.controller {
            if !boot.contains(READY)
                || !boot.contains(SELFTEST)
                || !boot.contains("[INPUT:TIMEOUT-TEST] sources=independent prefeed=true wrap=true")
                || boot.contains("[INPUT:FALLBACK]")
            {
                return Err("Input initialization/selftest did not establish its complete controller profile".into());
            }
            if profile.display {
                if !boot.contains("[GRAPHICS:PRESENT]") || boot.contains("[GRAPHICS:FALLBACK]") {
                    return Err("Input display profile did not present its framebuffer".into());
                }
            } else if !boot.contains("[GRAPHICS:FALLBACK] reason=no-framebuffer;")
                || boot.contains("[GRAPHICS:PRESENT]")
            {
                return Err(
                    "Missing-display input profile did not preserve serial fallback".into(),
                );
            }
            driver.keys()?;
            if profile.display {
                driver.pointer(10, 7, 410, 307, 0)?;
                driver.screenshot(artifacts, profile.name, "center", 410, 307, &mut evidence)?;
                for (position, dx, dy, x, y) in [
                    ("top-left", -100, -100, 0, 0),
                    ("top-right", 100, 0, WIDTH - 1, 0),
                    ("bottom-left", -100, 100, 0, HEIGHT - 1),
                    ("bottom-right", 100, 0, WIDTH - 1, HEIGHT - 1),
                ] {
                    driver.edge(dx, dy, x, y)?;
                    driver.screenshot(artifacts, profile.name, position, x, y, &mut evidence)?;
                }
                driver.buttons(WIDTH - 1, HEIGHT - 1)?;
            } else {
                driver.pointer(10, 7, 410, 307, 0)?;
                driver.buttons(410, 307)?;
            }
            evidence.push_str("ordered_key_events=true\nrepeat=true\nextended=true\nmodifiers=true\noverflow_recovery=true\nmalformed_recovery=true\nclient_reconnect=true\ncontroller_port_disable_reinitialize=true\nindependent_partial_timeouts=true\n");
        } else if !boot.contains("[INPUT:FALLBACK] keyboard=false pointer=false")
            || boot.contains("[INPUT:READY]")
        {
            return Err(
                "Missing i8042 controller did not use the documented serial fallback".into(),
            );
        }
        driver.uart.write_all(b"echo M14-INPUT-SERIAL-RECOVERY\r")?;
        driver.uart.flush()?;
        driver.wait("\r\nM14-INPUT-SERIAL-RECOVERY\r\n")?;
        driver.wait("anova> ")?;
        thread::sleep(Duration::from_millis(80));
        driver.uart.write_all(b"shutdown\r")?;
        driver.uart.flush()?;
        driver.shutdown()?;
        evidence
            .push_str("serial_recovery=true\nqemu_exit=0\nacpi_shutdown=true\ntimed_out=false\n");
        drop(driver);
        vars.cleanup()?;
        Ok(())
    })();
    match &outcome {
        Ok(()) => evidence.push_str("result=Ok\n"),
        Err(error) => evidence.push_str(&format!("result=Err\nerror={error}\n")),
    }
    fs::write(&result, evidence)?;
    outcome.map_err(|error| format!("{}: {error}; see {}", profile.name, result.display()).into())
}

struct Driver {
    guest: Guest,
    output: transcript::Transcript,
    uart: ChildStdin,
    qmp: Qmp,
    start: Instant,
    observed: usize,
}

impl Driver {
    fn poll(&mut self) -> Result<()> {
        self.output.poll()?;
        check_guest(&self.output.text(), self.start.elapsed() >= DEADLINE)?;
        if let Some(status) = self.guest.0.try_wait()? {
            self.output.finish()?;
            return Err(
                format!("Input guest exited before the scenario completed: {status}").into(),
            );
        }
        Ok(())
    }

    fn wait(&mut self, marker: &str) -> Result<()> {
        loop {
            self.poll()?;
            if let Some(at) = self.output.find_from(self.observed, marker) {
                self.observed = at + marker.len();
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn event(&mut self, marker: &str, expected: &str) -> Result<()> {
        loop {
            self.poll()?;
            let text = self.output.text();
            if let Some(line) = text.get(self.observed..).and_then(|tail| {
                tail.split_inclusive('\n')
                    .find(|line| line.ends_with('\n') && line.contains(marker))
            }) {
                if !event_matches(line, marker, expected) {
                    return Err(format!("Expected {expected}, received {}", line.trim()).into());
                }
                let at = self
                    .output
                    .find_from(self.observed, line)
                    .ok_or("Lost serial event cursor")?;
                self.observed = at + line.len();
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn key(
        &mut self,
        name: &str,
        code: u16,
        pressed: bool,
        repeat: bool,
        modifiers: u8,
    ) -> Result<()> {
        self.qmp.command(
            "input-send-event",
            json!({ "events": [{
            "type": "key", "data": { "down": pressed, "key": { "type": "qcode", "data": name } }
        }] }),
        )?;
        self.event(
            "[INPUT:KEY]",
            &format!(
                "[INPUT:KEY] code={code} pressed={pressed} repeat={repeat} modifiers={modifiers}"
            ),
        )
    }

    fn keys(&mut self) -> Result<()> {
        self.key("a", 30, true, false, 0)?;
        self.key("a", 30, true, true, 0)?;
        self.key("a", 30, false, false, 0)?;
        self.key("right", 333, true, false, 0)?;
        self.key("right", 333, false, false, 0)?;
        self.key("shift", 42, true, false, 1)?;
        self.key("a", 30, true, false, 1)?;
        self.key("a", 30, false, false, 1)?;
        self.key("shift", 42, false, false, 0)?;
        self.qmp.command(
            "send-key",
            json!({ "keys": [{ "type": "qcode", "data": "b" }], "hold-time": 30 }),
        )?;
        self.event(
            "[INPUT:KEY]",
            "[INPUT:KEY] code=48 pressed=true repeat=false modifiers=0",
        )?;
        self.event(
            "[INPUT:KEY]",
            "[INPUT:KEY] code=48 pressed=false repeat=false modifiers=0",
        )
    }

    fn motion(&mut self, dx: i32, dy: i32) -> Result<()> {
        self.qmp.command(
            "input-send-event",
            json!({ "events": [
            { "type": "rel", "data": { "axis": "x", "value": dx } },
            { "type": "rel", "data": { "axis": "y", "value": dy } }
        ] }),
        )
    }

    fn pointer(&mut self, dx: i32, dy: i32, x: u32, y: u32, buttons: u8) -> Result<()> {
        self.motion(dx, dy)?;
        self.event(
            "[INPUT:POINTER]",
            &format!("[INPUT:POINTER] x={x} y={y} buttons={buttons}"),
        )
    }

    fn edge(&mut self, dx: i32, dy: i32, x: u32, y: u32) -> Result<()> {
        for _ in 0..9 {
            self.motion(dx, dy)?;
            thread::sleep(Duration::from_millis(25));
            self.poll()?;
        }
        self.wait(&format!("[INPUT:POINTER] x={x} y={y} buttons=0 "))?;
        // Consume repeated clamped packets before beginning the next independent exchange.
        thread::sleep(Duration::from_millis(80));
        self.poll()?;
        self.observed = self.output.text().len();
        Ok(())
    }

    fn buttons(&mut self, x: u32, y: u32) -> Result<()> {
        for (button, down, buttons) in [
            ("left", true, 1),
            ("right", true, 3),
            ("middle", true, 7),
            ("left", false, 6),
            ("right", false, 4),
            ("middle", false, 0),
        ] {
            self.qmp.command(
                "input-send-event",
                json!({ "events": [{
                "type": "btn", "data": { "down": down, "button": button }
            }] }),
            )?;
            self.event(
                "[INPUT:POINTER]",
                &format!("[INPUT:POINTER] x={x} y={y} buttons={buttons}"),
            )?;
        }
        Ok(())
    }

    fn screenshot(
        &mut self,
        artifacts: &Path,
        name: &str,
        position: &str,
        x: u32,
        y: u32,
        evidence: &mut String,
    ) -> Result<()> {
        let path = artifacts.join(format!("{name}-{position}.ppm"));
        self.qmp
            .command("screendump", json!({ "filename": path, "format": "ppm" }))?;
        let bytes = read_ppm(&path)?;
        verify_cursor(&bytes, x, y)?;
        evidence.push_str(&format!("screenshot_{position}={}\nscreenshot_{position}_sha256={:x}\ncursor_{position}={x},{y}\ncompared_pixels_{position}={}\n", path.display(), Sha256::digest(&bytes), WIDTH * HEIGHT));
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        loop {
            self.output.poll()?;
            check_guest(&self.output.text(), self.start.elapsed() >= DEADLINE)?;
            if let Some(status) = self.guest.0.try_wait()? {
                self.output.finish()?;
                check_guest(&self.output.text(), false)?;
                if status.code() != Some(0)
                    || !self
                        .output
                        .text()
                        .contains("[POWER:OFF] synced=true clean=true dma_reset=true")
                {
                    return Err(format!(
                        "Input guest did not perform a clean ACPI shutdown: {status}"
                    )
                    .into());
                }
                return Ok(());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

fn event_matches(line: &str, marker: &str, expected: &str) -> bool {
    let Some(at) = line.find(marker) else {
        return false;
    };
    line[at..]
        .trim_end()
        .strip_prefix(expected)
        .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(' '))
}

fn check_guest(text: &str, timed_out: bool) -> Result<()> {
    if timed_out {
        return Err("Input guest exceeded its 60-second watchdog".into());
    }
    if [
        "[PANIC]",
        "[EXCEPTION]",
        "[INIT:FAIL]",
        "[SHELL:FAIL]",
        "[POWER:FAIL]",
        "[INPUT:FAIL]",
        "[INPUT:RESET]",
    ]
    .iter()
    .any(|marker| text.contains(marker))
    {
        return Err(
            "Input guest reported a fatal boot, driver, runtime or shutdown failure".into(),
        );
    }
    Ok(())
}

fn verify_cursor(bytes: &[u8], x: u32, y: u32) -> Result<()> {
    let ppm = parse_ppm(bytes)?;
    if (ppm.width, ppm.height) != (WIDTH, HEIGHT) || x >= WIDTH || y >= HEIGHT {
        return Err(
            "Cursor screenshot dimensions or position are outside the selected profile".into(),
        );
    }
    let mut pixels = vec![0; (WIDTH * HEIGHT) as usize];
    let mut reference = Surface::new(WIDTH, HEIGHT, &mut pixels)
        .map_err(|error| format!("Cannot create cursor reference: {error:?}"))?;
    reference.draw_test_pattern();
    for cy in 0..CURSOR_HEIGHT {
        for cx in 0..CURSOR_WIDTH {
            if let Some(color) = cursor_color(cx, cy) {
                reference.pixel(i64::from(x + cx), i64::from(y + cy), color);
            }
        }
    }
    for (index, (rgb, expected)) in ppm.rgb.chunks_exact(3).zip(reference.pixels()).enumerate() {
        let actual = (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2]);
        if actual != *expected {
            return Err(format!(
                "Cursor scanout pixel ({},{}) is {actual:#08x}, expected {expected:#08x}",
                index % WIDTH as usize,
                index / WIDTH as usize
            )
            .into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_gate_rejects_timeout_and_driver_runtime_faults() {
        assert!(check_guest("[INPUT:READY]", false).is_ok());
        assert!(check_guest("[INPUT:READY]", true).is_err());
        for marker in [
            "[PANIC]",
            "[EXCEPTION]",
            "[INIT:FAIL]",
            "[SHELL:FAIL]",
            "[POWER:FAIL]",
            "[INPUT:FAIL]",
            "[INPUT:RESET]",
        ] {
            assert!(check_guest(marker, false).is_err());
        }
    }

    #[test]
    fn event_fields_require_complete_values_and_allow_additional_diagnostics() {
        let expected = "[INPUT:POINTER] x=799 y=599 buttons=0";
        assert!(event_matches(
            "[INPUT:POINTER] x=799 y=599 buttons=0 dx=100 dy=0 changed=0 overflow=false\r\n",
            "[INPUT:POINTER]",
            expected
        ));
        assert!(!event_matches(
            "[INPUT:POINTER] x=799 y=599 buttons=01\r\n",
            "[INPUT:POINTER]",
            expected
        ));
        assert!(!event_matches(
            "[INPUT:POINTER] x=799 y=59 buttons=0\r\n",
            "[INPUT:POINTER]",
            expected
        ));
        assert!(!event_matches(
            "[INPUT:KEY] code=30 pressed=true repeat=false modifiers=0\r\n",
            "[INPUT:POINTER]",
            expected
        ));
    }

    #[test]
    fn cursor_gate_rejects_wrong_position_corrupt_background_and_bad_dimensions() {
        let (x, y) = (WIDTH - 1, HEIGHT - 1);
        let mut pixels = vec![0; (WIDTH * HEIGHT) as usize];
        let mut reference = Surface::new(WIDTH, HEIGHT, &mut pixels).unwrap();
        reference.draw_test_pattern();
        reference.pixel(i64::from(x), i64::from(y), cursor_color(0, 0).unwrap());
        let mut bytes = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
        for pixel in reference.pixels() {
            bytes.extend_from_slice(&[(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
        }
        assert!(verify_cursor(&bytes, x, y).is_ok());
        assert!(verify_cursor(&bytes, 400, 300).is_err());
        let index = bytes.len() - 100;
        bytes[index] ^= 1;
        assert!(verify_cursor(&bytes, x, y).is_err());
        assert!(verify_cursor(b"P6\n1 1\n255\n\0\0\0", 0, 0).is_err());
        assert!(verify_cursor(&bytes, WIDTH, y).is_err());
    }
}
