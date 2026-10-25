// SPDX-License-Identifier: Apache-2.0
//! Drive isolated native GUI clients through QMP and compare actual scanout with an independent
//! software reference. Bound each boot, retain evidence, and prove serial/device-failure recovery.
use super::*;
use anova_graphics::{CURSOR_HEIGHT, CURSOR_WIDTH, cursor_color};
use graphics::{Qmp, parse_ppm, read_ppm};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{io::Write, process::ChildStdin};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;
const DEADLINE: Duration = Duration::from_secs(90);
const RED: u32 = 0xc43b42;
const BLUE: u32 = 0x3879c9;

#[derive(Clone, Copy)]
struct Profile {
    name: &'static str,
    display: bool,
    input: bool,
}

const PROFILES: [Profile; 3] = [
    Profile {
        name: "gui",
        display: true,
        input: true,
    },
    Profile {
        name: "gui-no-display",
        display: false,
        input: true,
    },
    Profile {
        name: "gui-no-controller",
        display: true,
        input: false,
    },
];

/// The host reference deliberately specifies the accepted geometry and color policy itself.
/// Sharing the compositor's renderer here would allow a rendering defect to validate itself.
#[derive(Clone, Copy)]
struct Window {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    color: u32,
    focused: bool,
}

impl Window {
    fn red() -> Self {
        Self {
            x: 80,
            y: 80,
            width: 240,
            height: 160,
            color: RED,
            focused: false,
        }
    }

    fn blue() -> Self {
        Self {
            x: 200,
            y: 150,
            width: 240,
            height: 160,
            color: BLUE,
            focused: true,
        }
    }

    fn pixel(self, x: u32, y: u32) -> Option<u32> {
        if x < self.x || y < self.y || x >= self.x + self.width || y >= self.y + self.height {
            return None;
        }
        let rx = x - self.x;
        let ry = y - self.y;
        Some(
            if rx == 0 || ry == 0 || rx + 1 == self.width || ry + 1 == self.height {
                if self.focused { 0xe0e8f0 } else { 0x728397 }
            } else if ry < 16 {
                if self.focused { 0x354d66 } else { 0x2b3d52 }
            } else {
                self.color
            },
        )
    }
}

pub fn verify(root: &Path) -> Result<()> {
    let artifacts = directory(root, "gui")?;
    let result = artifacts.join("gui-test.result.txt");
    output_path(&result)?;
    fs::write(&result, "result=Running\n")?;
    for profile in PROFILES {
        let path = artifacts.join(format!("{}.result.txt", profile.name));
        output_path(&path)?;
        fs::write(path, "result=Pending\n")?;
        for phase in [
            "initial",
            "red-focus",
            "moved",
            "resized",
            "blue-focus",
            "client-fault",
            "restart",
        ] {
            let path = artifacts.join(format!("{}-{phase}.ppm", profile.name));
            output_path(&path)?;
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
    }
    let outcome = (|| -> Result<()> {
        let iso = image(root, Mode::GuiTest)?;
        for profile in PROFILES {
            session(root, &iso, &artifacts, profile)?;
        }
        Ok(())
    })();
    fs::write(
        &result,
        match &outcome {
            Ok(()) => "result=Ok\nscenarios=gui,gui-no-display,gui-no-controller\n".into(),
            Err(error) => format!("result=Err\nerror={error}\n"),
        },
    )?;
    outcome?;
    println!(
        "Verified isolated native windows, all scanout pixels, physical focus/move/resize/input, client fault, compositor restart, missing devices and COM1 recovery"
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
    let vars = Temporary::new(root, "gui")?;
    let socket = vars.0.join("qmp.sock");
    let mut command = qemu(root, iso, &vars.0, false)?;
    command.args(["-vga", if profile.display { "std" } else { "none" }]);
    if !profile.input {
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
        profile.input,
        Sha256::digest(fs::read(iso)?),
        Sha256::digest(fs::read(
            root.join("target").join(TARGET).join("debug/anova-kernel")
        )?),
    );
    for app in ["compositor", "gui-test", "gui-client"] {
        evidence.push_str(&format!(
            "{app}_sha256={:x}\n",
            Sha256::digest(fs::read(
                root.join("build/apps").join(format!("{app}.elf"))
            )?)
        ));
    }
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
            expected_faults: Vec::new(),
            pointer: (400, 300),
            buttons: 0,
        };
        driver.wait("anova> ")?;
        thread::sleep(Duration::from_millis(80));
        let boot = driver.output.text();
        validate_boot(&boot, profile)?;
        if !boot.contains("[GUI:TRANSPORT-TEST] oom=true rollback=true quotas=true") {
            return Err(
                "GUI image did not complete actual mailbox heap-OOM rollback and quota checks"
                    .into(),
            );
        }
        evidence.push_str("mailbox_heap_oom=true\nmailbox_rollback=true\ntransport_quotas=true\n");
        driver.serial(b"gui-test\r")?;
        if !profile.display {
            driver.wait("[GUI:NO_DISPLAY]")?;
            let server = process_pid(&driver.output.text(), "test", "compositor")?;
            if !has_exit(&driver.output.text(), server, 2) {
                return Err(
                    "Missing-display compositor did not return its explicit unsupported status"
                        .into(),
                );
            }
            evidence.push_str("missing_display_recovery=true\n");
        } else {
            driver.wait_all(&[
                "[GUI:RED_READY]",
                "[GUI:BLUE_READY]",
                "[GUI:TRANSPORT_BOUNDARIES] unauthorized=true bad_length=true",
                "[GUI:POINTER_BOUNDARY] bad_pointer=true",
                "[GUI:PRESSURE] connection_quota=true reclaimed=true",
                "[GUI:MAILBOX_BOUNDARIES] bounded=true evicted=true reconnect=true copy_failure_retained=true",
                "[GUI:RED_BOUNDARIES] bad_surface=true stale=true quota=true",
                "[GUI:BLUE_BOUNDARIES] owner=true",
            ])?;
            let red_pid = process_pid(&driver.output.text(), "red", "gui-client")?;
            let blue_pid = process_pid(&driver.output.text(), "blue", "gui-client")?;
            let server_pid = process_pid(&driver.output.text(), "test", "compositor")?;
            if red_pid == blue_pid {
                return Err("GUI clients did not occupy isolated processes".into());
            }
            evidence.push_str(&format!("red_pid={red_pid}\nblue_pid={blue_pid}\nauthenticated_transport=true\nbad_pointer=true\nbad_length=true\nbad_surface=true\nstale_handle=true\nsurface_quota=true\nforeign_surface_rejected=true\n"));
            driver.screenshot(
                artifacts,
                profile.name,
                "initial",
                &[Window::red(), Window::blue()],
                None,
                &mut evidence,
            )?;
            if profile.input {
                scenario(
                    &mut driver,
                    artifacts,
                    profile.name,
                    red_pid,
                    server_pid,
                    blue_pid,
                    &mut evidence,
                )?;
            } else {
                if !driver.output.text().contains("[GUI:NO_INPUT]") {
                    return Err("Missing explicit GUI input fallback".into());
                }
                driver.wait("[GUI:NO_INPUT_PASS]")?;
                if !has_exit(&driver.output.text(), server_pid, 74)
                    || ![red_pid, blue_pid]
                        .iter()
                        .all(|pid| has_exit(&driver.output.text(), *pid, 0))
                {
                    return Err(
                        "Missing-input GUI children did not complete their expected clean exits"
                            .into(),
                    );
                }
                evidence.push_str("missing_controller_recovery=true\n");
            }
        }
        driver.wait("anova> ")?;
        thread::sleep(Duration::from_millis(80));
        driver.serial(b"echo M14-GUI-SERIAL-RECOVERY\r")?;
        driver.wait("\r\nM14-GUI-SERIAL-RECOVERY\r\n")?;
        driver.wait("anova> ")?;
        thread::sleep(Duration::from_millis(80));
        driver.serial(b"shutdown\r")?;
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

fn scenario(
    driver: &mut Driver,
    artifacts: &Path,
    name: &str,
    red_pid: u64,
    server_pid: u64,
    blue_pid: u64,
    evidence: &mut String,
) -> Result<()> {
    let initial_session = server_session(&driver.output.text(), server_pid)?;
    let mut red = Window::red();
    red.focused = true;
    let mut blue = Window::blue();
    blue.focused = false;
    driver.click(100, 110)?;
    driver.key("a", Some("[GUI:KEY] client=red code=30 pressed=true"))?;
    driver.screenshot(
        artifacts,
        name,
        "red-focus",
        &[blue, red],
        Some(driver.pointer),
        evidence,
    )?;
    driver.move_to(100, 88)?;
    driver.button(true)?;
    driver.move_to(180, 138)?;
    driver.button(false)?;
    red.x = 160;
    red.y = 130;
    driver.screenshot(
        artifacts,
        name,
        "moved",
        &[blue, red],
        Some(driver.pointer),
        evidence,
    )?;
    driver.move_to(398, 288)?;
    driver.button(true)?;
    driver.move_to(438, 258)?;
    driver.button(false)?;
    // The client may finish repainting before the release packet's completion log.
    driver.wait_all(&["[GUI:RESIZED] client=red width=280 height=130"])?;
    red.width = 280;
    red.height = 130;
    driver.screenshot(
        artifacts,
        name,
        "resized",
        &[blue, red],
        Some(driver.pointer),
        evidence,
    )?;
    driver.click(420, 290)?;
    driver.key("b", Some("[GUI:KEY] client=blue code=48 pressed=true"))?;
    red.focused = false;
    blue.focused = true;
    driver.screenshot(
        artifacts,
        name,
        "blue-focus",
        &[red, blue],
        Some(driver.pointer),
        evidence,
    )?;
    driver.click(170, 170)?;
    driver.expected_faults.push(red_pid);
    driver.key("x", Some("[GUI:RED_FAULT]"))?;
    driver.wait_fields(
        "[USER:FAULT]",
        &[("pid", &red_pid.to_string()), ("vector", "6")],
    )?;
    driver.wait("[GUI:CLIENT_GONE]")?;
    driver.click(420, 290)?;
    driver.key("b", Some("[GUI:KEY] client=blue code=48 pressed=true"))?;
    driver.screenshot(
        artifacts,
        name,
        "client-fault",
        &[blue],
        Some(driver.pointer),
        evidence,
    )?;
    driver.expected_faults.push(server_pid);
    driver.key("f12", Some("[GUI:SERVER_FAULT]"))?;
    driver.wait_fields(
        "[USER:FAULT]",
        &[("pid", &server_pid.to_string()), ("vector", "6")],
    )?;
    driver.wait("[GUI:SERVER_FAULT_REAPED] reason=1 code=6")?;
    driver.wait("[GUI:RESTART_READY]")?;
    let restart_pid = process_pid(&driver.output.text(), "restart", "gui-client")?;
    let restart_server = process_pid(&driver.output.text(), "restart", "compositor")?;
    let restarted_session = server_session(&driver.output.text(), restart_server)?;
    if restarted_session <= initial_session {
        return Err(
            "Restarted compositor did not receive a fresh increasing session identity".into(),
        );
    }
    if restart_pid == red_pid {
        return Err("Restarted client reused a live process identity".into());
    }
    driver.move_to(500, 350)?;
    driver.screenshot(
        artifacts,
        name,
        "restart",
        &[blue],
        Some(driver.pointer),
        evidence,
    )?;
    // A freshly connected client must receive physical input after server replacement.
    driver.click(420, 290)?;
    driver.key("b", Some("[GUI:KEY] client=blue code=48 pressed=true"))?;
    driver.key("f11", None)?;
    driver.wait("[GUI:TEST_PASS]")?;
    if fault_count(&driver.output.text()) != 2 {
        return Err(
            "GUI gate did not observe exactly the isolated client and compositor faults".into(),
        );
    }
    for pid in [blue_pid, restart_pid, restart_server] {
        if !has_exit(&driver.output.text(), pid, 0) {
            return Err(format!("GUI child pid={pid} did not exit cleanly").into());
        }
    }
    if !driver
        .output
        .text()
        .contains("[GUI:FAULT_REAPED] reason=1 code=6 other_client_survived=true")
    {
        return Err("Supervisor did not verify the isolated client fault status".into());
    }
    evidence.push_str(&format!("compositor_pid={server_pid}\ninitial_session={initial_session}\nrestart_compositor_pid={restart_server}\nrestarted_session={restarted_session}\nrestart_pid={restart_pid}\nphysical_key_routing=true\nfocus_raise=true\nmove=true\nresize=true\nocclusion_redraw=true\nclient_fault_vector=6\nclient_cleanup=true\nsurviving_client_input=true\ncompositor_fault_vector=6\ncompositor_restart=true\nreconnected_client_input=true\n"));
    Ok(())
}

struct Driver {
    guest: Guest,
    output: transcript::Transcript,
    uart: ChildStdin,
    qmp: Qmp,
    start: Instant,
    observed: usize,
    expected_faults: Vec<u64>,
    pointer: (u32, u32),
    buttons: u8,
}

impl Driver {
    fn poll(&mut self) -> Result<()> {
        self.output.poll()?;
        check_guest(
            &self.output.text(),
            self.start.elapsed() >= DEADLINE,
            &self.expected_faults,
        )?;
        if let Some(status) = self.guest.0.try_wait()? {
            self.output.finish()?;
            return Err(format!("GUI guest exited before the scenario completed: {status}").into());
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

    fn wait_all(&mut self, markers: &[&str]) -> Result<()> {
        loop {
            self.poll()?;
            let text = self.output.text();
            if markers.iter().all(|marker| text.contains(marker)) {
                self.observed = text.len();
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_fields(&mut self, marker: &str, fields: &[(&str, &str)]) -> Result<()> {
        loop {
            self.poll()?;
            let text = self.output.text();
            if let Some(line) = text.get(self.observed..).and_then(|tail| {
                tail.split_inclusive('\n')
                    .find(|line| line.ends_with('\n') && fields_match(line, marker, fields))
            }) {
                self.observed = self
                    .output
                    .find_from(self.observed, line)
                    .ok_or("Lost GUI serial cursor")?
                    + line.len();
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn serial(&mut self, bytes: &[u8]) -> Result<()> {
        self.uart.write_all(bytes)?;
        self.uart.flush()?;
        Ok(())
    }

    fn move_to(&mut self, x: u32, y: u32) -> Result<()> {
        while self.pointer != (x, y) {
            let dx = (i64::from(x) - i64::from(self.pointer.0)).clamp(-80, 80);
            let dy = (i64::from(y) - i64::from(self.pointer.1)).clamp(-80, 80);
            let target = (
                (i64::from(self.pointer.0) + dx) as u32,
                (i64::from(self.pointer.1) + dy) as u32,
            );
            self.qmp.command(
                "input-send-event",
                json!({ "events": [
                { "type": "rel", "data": { "axis": "x", "value": dx } },
                { "type": "rel", "data": { "axis": "y", "value": dy } }
            ] }),
            )?;
            self.wait_fields(
                "[GUI:POINTER]",
                &[
                    ("x", &target.0.to_string()),
                    ("y", &target.1.to_string()),
                    ("buttons", &self.buttons.to_string()),
                ],
            )?;
            self.pointer = target;
        }
        Ok(())
    }

    fn button(&mut self, down: bool) -> Result<()> {
        self.qmp.command(
            "input-send-event",
            json!({ "events": [
            { "type": "btn", "data": { "down": down, "button": "left" } }
        ] }),
        )?;
        self.buttons = u8::from(down);
        self.wait_fields(
            "[GUI:POINTER]",
            &[
                ("x", &self.pointer.0.to_string()),
                ("y", &self.pointer.1.to_string()),
                ("buttons", &self.buttons.to_string()),
            ],
        )
    }

    fn click(&mut self, x: u32, y: u32) -> Result<()> {
        self.move_to(x, y)?;
        self.button(true)?;
        self.button(false)
    }

    fn key(&mut self, name: &str, expected: Option<&str>) -> Result<()> {
        for down in [true, false] {
            self.qmp.command("input-send-event", json!({ "events": [
                { "type": "key", "data": { "down": down, "key": { "type": "qcode", "data": name } } }
            ] }))?;
            thread::sleep(Duration::from_millis(30));
        }
        if let Some(marker) = expected {
            self.wait(marker)?;
        }
        Ok(())
    }

    fn screenshot(
        &mut self,
        artifacts: &Path,
        name: &str,
        phase: &str,
        windows: &[Window],
        cursor: Option<(u32, u32)>,
        evidence: &mut String,
    ) -> Result<()> {
        // PPM captures QEMU's active VGA surface after actual guest presentation completion.
        self.poll()?;
        let path = artifacts.join(format!("{name}-{phase}.ppm"));
        self.qmp
            .command("screendump", json!({ "filename": path, "format": "ppm" }))?;
        let bytes = read_ppm(&path)?;
        verify_scanout(&bytes, windows, cursor)?;
        evidence.push_str(&format!(
            "screenshot_{phase}={}\nscreenshot_{phase}_sha256={:x}\ncompared_pixels_{phase}={}\n",
            path.display(),
            Sha256::digest(&bytes),
            WIDTH * HEIGHT
        ));
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        loop {
            self.output.poll()?;
            check_guest(
                &self.output.text(),
                self.start.elapsed() >= DEADLINE,
                &self.expected_faults,
            )?;
            if let Some(status) = self.guest.0.try_wait()? {
                self.output.finish()?;
                check_guest(&self.output.text(), false, &self.expected_faults)?;
                if status.code() != Some(0)
                    || !self
                        .output
                        .text()
                        .contains("[POWER:OFF] synced=true clean=true dma_reset=true")
                {
                    return Err(format!(
                        "GUI guest did not perform a clean ACPI shutdown: {status}"
                    )
                    .into());
                }
                return Ok(());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

fn fields_match(line: &str, marker: &str, fields: &[(&str, &str)]) -> bool {
    let Some(tail) = line.find(marker).map(|at| &line[at + marker.len()..]) else {
        return false;
    };
    if !tail.starts_with(char::is_whitespace) && !tail.is_empty() {
        return false;
    }
    fields.iter().all(|(name, value)| {
        let mut found = tail
            .split_ascii_whitespace()
            .filter_map(|token| token.split_once('='))
            .filter(|(field, _)| field == name);
        found.next().is_some_and(|(_, actual)| actual == *value) && found.next().is_none()
    })
}

fn process_pid(text: &str, name: &str, image: &str) -> Result<u64> {
    let mut identities = text
        .lines()
        .filter(|line| fields_match(line, "[GUI:CLIENT]", &[("name", name), ("image", image)]));
    let line = identities
        .next()
        .ok_or("Missing GUI client process identity")?;
    if identities.next().is_some() {
        return Err("Ambiguous GUI process identity".into());
    }
    let mut fields = line
        .split_ascii_whitespace()
        .filter_map(|token| token.strip_prefix("pid="));
    let pid = fields
        .next()
        .ok_or("Missing GUI client PID")?
        .parse::<u64>()?;
    if fields.next().is_some() {
        return Err("Duplicate GUI process PID field".into());
    }
    if pid == 0 {
        return Err("GUI client PID must be nonzero".into());
    }
    Ok(pid)
}

fn has_exit(text: &str, pid: u64, status: u32) -> bool {
    text.lines().any(|line| {
        fields_match(
            line,
            "[USER:EXIT]",
            &[("pid", &pid.to_string()), ("status", &status.to_string())],
        )
    })
}

fn server_session(text: &str, pid: u64) -> Result<u64> {
    if pid == 0 {
        return Err("GUI server PID must be nonzero".into());
    }
    let mut ready = text
        .lines()
        .filter(|line| fields_match(line, "[GUI:SERVER_READY]", &[("pid", &pid.to_string())]));
    let line = ready
        .next()
        .ok_or("Missing GUI server readiness identity")?;
    if ready.next().is_some() {
        return Err("Ambiguous GUI server readiness identity".into());
    }
    let mut fields = line
        .split_ascii_whitespace()
        .filter_map(|token| token.strip_prefix("session="));
    let value = fields.next().ok_or("Missing GUI server session")?;
    if fields.next().is_some()
        || value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("GUI server session must be a unique decimal integer".into());
    }
    let session = value.parse::<u64>()?;
    if session == 0 {
        return Err("GUI server session must be nonzero".into());
    }
    Ok(session)
}

fn fault_count(text: &str) -> usize {
    text.lines()
        .filter(|line| line.contains("[USER:FAULT]"))
        .count()
}

fn check_guest(text: &str, timed_out: bool, expected_faults: &[u64]) -> Result<()> {
    if timed_out {
        return Err("GUI guest exceeded its 90-second watchdog".into());
    }
    if [
        "[PANIC]",
        "[EXCEPTION]",
        "[INIT:FAIL]",
        "[SHELL:FAIL]",
        "[POWER:FAIL]",
        "[GUI:FAIL]",
        "[GUI:CLIENT_FAIL]",
        "[GUI:SERVER_FAIL]",
        "[USER:RETURNREJECT]",
        "[INPUT:FAIL]",
    ]
    .iter()
    .any(|marker| text.contains(marker))
    {
        return Err("GUI guest reported a fatal boot, runtime or shutdown failure".into());
    }
    for line in text
        .split_inclusive('\n')
        .filter(|line| line.ends_with('\n') && line.contains("[USER:FAULT]"))
    {
        if !expected_faults.iter().any(|pid| {
            fields_match(
                line,
                "[USER:FAULT]",
                &[("pid", &pid.to_string()), ("vector", "6")],
            )
        }) {
            return Err(format!("Unexpected GUI process fault: {}", line.trim()).into());
        }
    }
    for pid in expected_faults {
        if text
            .lines()
            .filter(|line| fields_match(line, "[USER:FAULT]", &[("pid", &pid.to_string())]))
            .count()
            > 1
        {
            return Err("GUI guest reported a repeated process fault".into());
        }
    }
    Ok(())
}

fn validate_boot(text: &str, profile: Profile) -> Result<()> {
    if profile.display {
        if !text.contains("[GRAPHICS:PRESENT]") || text.contains("[GRAPHICS:FALLBACK]") {
            return Err("GUI display profile failed framebuffer presentation".into());
        }
    } else if !text.contains("[GRAPHICS:FALLBACK] reason=no-framebuffer;")
        || text.contains("[GRAPHICS:PRESENT]")
    {
        return Err("GUI absent-display profile did not preserve its explicit fallback".into());
    }
    if profile.input {
        if !text.contains("[INPUT:READY]") || text.contains("[INPUT:FALLBACK]") {
            return Err("GUI physical-input profile did not initialize the controller".into());
        }
    } else if !text.contains("[INPUT:FALLBACK] keyboard=false pointer=false")
        || text.contains("[INPUT:READY]")
    {
        return Err("GUI absent-controller profile did not preserve serial fallback".into());
    }
    Ok(())
}

fn reference_pixel(x: u32, y: u32, windows: &[Window], cursor: Option<(u32, u32)>) -> u32 {
    let mut expected = 0x18283b;
    for window in windows {
        if let Some(color) = window.pixel(x, y) {
            expected = color;
        }
    }
    if let Some((cx, cy)) = cursor
        && x >= cx
        && y >= cy
        && x - cx < CURSOR_WIDTH
        && y - cy < CURSOR_HEIGHT
        && let Some(color) = cursor_color(x - cx, y - cy)
    {
        expected = color;
    }
    expected
}

fn verify_scanout(bytes: &[u8], windows: &[Window], cursor: Option<(u32, u32)>) -> Result<()> {
    let ppm = parse_ppm(bytes)?;
    if (ppm.width, ppm.height) != (WIDTH, HEIGHT) {
        return Err("GUI scanout dimensions differ from the accepted 800x600 profile".into());
    }
    for (index, rgb) in ppm.rgb.chunks_exact(3).enumerate() {
        let x = index as u32 % WIDTH;
        let y = index as u32 / WIDTH;
        let expected = reference_pixel(x, y, windows, cursor);
        let actual = u32::from(rgb[0]) << 16 | u32::from(rgb[1]) << 8 | u32::from(rgb[2]);
        if actual != expected {
            return Err(format!(
                "GUI scanout pixel ({x},{y}) is {actual:#08x}, expected {expected:#08x}"
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
    fn gui_gate_rejects_timeout_fatal_and_unexpected_client_faults() {
        assert!(check_guest("[GUI:BLUE_READY]\n", false, &[]).is_ok());
        assert!(check_guest("", true, &[]).is_err());
        for marker in [
            "[PANIC]",
            "[EXCEPTION]",
            "[INIT:FAIL]",
            "[SHELL:FAIL]",
            "[POWER:FAIL]",
            "[GUI:FAIL]",
            "[GUI:CLIENT_FAIL]",
            "[GUI:SERVER_FAIL]",
            "[USER:RETURNREJECT]",
            "[INPUT:FAIL]",
        ] {
            assert!(check_guest(marker, false, &[]).is_err());
        }
        let fault = "[USER:FAULT] pid=12 vector=6 error=0x0 address=0x0 rip=0x40000000\n";
        assert!(check_guest(fault, false, &[12]).is_ok());
        assert!(check_guest(fault, false, &[]).is_err());
        assert!(check_guest(fault, false, &[1]).is_err());
        assert!(check_guest(&fault.replace("vector=6", "vector=14"), false, &[12]).is_err());
        assert!(check_guest(&fault.repeat(2), false, &[12]).is_err());
        let server_fault = fault.replace("pid=12", "pid=4");
        assert!(check_guest(&format!("{fault}{server_fault}"), false, &[12, 4]).is_ok());
        assert!(check_guest(&format!("{fault}{server_fault}"), false, &[12]).is_err());
    }

    #[test]
    fn gui_gate_requires_complete_unique_identity_and_event_fields() {
        assert!(fields_match(
            "[GUI:POINTER] x=170 y=170 buttons=0 damage=1\r\n",
            "[GUI:POINTER]",
            &[("x", "170"), ("buttons", "0")]
        ));
        for line in [
            "[GUI:POINTER] x=1700 y=170 buttons=0\n",
            "[GUI:POINTER] x=170 x=170 y=170 buttons=0\n",
            "[GUI:POINTER]x=170 y=170 buttons=0\n",
        ] {
            assert!(!fields_match(line, "[GUI:POINTER]", &[("x", "170")]));
        }
        assert_eq!(
            process_pid(
                "[GUI:CLIENT] name=red pid=12 image=gui-client\n",
                "red",
                "gui-client"
            )
            .unwrap(),
            12
        );
        assert!(
            process_pid(
                "[GUI:CLIENT] name=redder pid=12 image=gui-client\n",
                "red",
                "gui-client"
            )
            .is_err()
        );
        assert!(
            process_pid(
                "[GUI:CLIENT] name=red pid=12 pid=13 image=gui-client\n",
                "red",
                "gui-client"
            )
            .is_err()
        );
        assert!(process_pid("[GUI:CLIENT] name=red pid=12 image=gui-client\n[GUI:CLIENT] name=red pid=13 image=gui-client\n", "red", "gui-client").is_err());
        assert!(has_exit("[USER:EXIT] pid=12 status=0\n", 12, 0));
        assert!(!has_exit("[USER:EXIT] pid=12 status=0\n", 1, 0));
        assert!(!has_exit("[USER:EXIT] pid=12 status=74\n", 12, 0));
        assert_eq!(server_session("[GUI:SERVER_READY] pid=4 session=1 userspace=true\n[GUI:SERVER_READY] pid=12 session=2 userspace=true\n", 12).unwrap(), 2);
        for line in [
            "[GUI:SERVER_READY] pid=12 session=2 userspace=true\n",
            "[GUI:SERVER_READY] pid=4 userspace=true\n",
            "[GUI:SERVER_READY] pid=4 session=0 userspace=true\n",
            "[GUI:SERVER_READY] pid=4 session=one userspace=true\n",
            "[GUI:SERVER_READY] pid=4 session=+1 userspace=true\n",
            "[GUI:SERVER_READY] pid=4 session=-1 userspace=true\n",
            "[GUI:SERVER_READY] pid=4 session=18446744073709551616 userspace=true\n",
            "[GUI:SERVER_READY] pid=4 session=1 session=2 userspace=true\n",
            "[GUI:SERVER_READY] pid=4 session=1 userspace=true\n[GUI:SERVER_READY] pid=4 session=2 userspace=true\n",
        ] {
            assert!(server_session(line, 4).is_err());
        }
        assert!(server_session("[GUI:SERVER_READY] pid=0 session=1 userspace=true\n", 0).is_err());
        assert!(
            process_pid(
                "[GUI:CLIENT] name=red pid=0 image=gui-client\n",
                "red",
                "gui-client"
            )
            .is_err()
        );
    }

    #[test]
    fn scanout_reference_distinguishes_z_order_focus_move_resize_cursor_and_corruption() {
        let windows = [Window::red(), Window::blue()];
        let mut bytes = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let rgb = reference_pixel(x, y, &windows, None);
                bytes.extend_from_slice(&[(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]);
            }
        }
        assert!(verify_scanout(&bytes, &windows, None).is_ok());
        assert!(verify_scanout(&bytes, &[windows[1], windows[0]], None).is_err());
        let mut changed = windows;
        changed[0].focused = true;
        assert!(verify_scanout(&bytes, &changed, None).is_err());
        changed = windows;
        changed[0].x += 1;
        assert!(verify_scanout(&bytes, &changed, None).is_err());
        changed = windows;
        changed[0].width += 1;
        assert!(verify_scanout(&bytes, &changed, None).is_err());
        assert!(verify_scanout(&bytes, &windows, Some((400, 300))).is_err());
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert!(verify_scanout(&bytes, &windows, None).is_err());
        assert!(verify_scanout(b"P6\n1 1\n255\n\0\0\0", &[], None).is_err());
    }
}
