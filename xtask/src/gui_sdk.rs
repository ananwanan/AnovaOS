// SPDX-License-Identifier: Apache-2.0
//! Verify real guest GUI SDK builds, physical input and independent scanout samples.
//! Retain serial/QMP evidence, reap isolated children and compare source/ELF bytes across reboot.
use super::*;
use graphics::{Qmp, parse_ppm, read_ppm};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{io::Write, process::ChildStdin};

const DEADLINE: Duration = Duration::from_secs(1200);

#[derive(Clone, Copy)]
struct Fixture {
    language: &'static str,
    source: &'static str,
    marker: &'static str,
}

const FIXTURES: [Fixture; 7] = [
    Fixture {
        language: "contract",
        source: "contract.c",
        marker: "CONTRACT",
    },
    Fixture {
        language: "fault",
        source: "fault.c",
        marker: "FAULT",
    },
    Fixture {
        language: "c",
        source: "main.c",
        marker: "C",
    },
    Fixture {
        language: "cpp",
        source: "main.cpp",
        marker: "CPP",
    },
    Fixture {
        language: "rust",
        source: "main.rs",
        marker: "RUST",
    },
    Fixture {
        language: "note",
        source: "note.rs",
        marker: "NOTE",
    },
    Fixture {
        language: "terminal",
        source: "terminal.rs",
        marker: "TERMINAL",
    },
];

/// The caller prepares an ordinary GUI image containing the admitted compositor and compiler
/// tools. Every fixture and native application is edited and compiled inside the guest.
pub fn verify(root: &Path, iso: &Path) -> Result<()> {
    let artifacts = directory(root, "gui-sdk")?;
    let result = artifacts.join("gui-sdk-test.result.txt");
    output_path(&result)?;
    fs::write(&result, "result=Running\n")?;
    let mut evidence = format!(
        "iso={}\niso_sha256={:x}\n",
        iso.display(),
        Sha256::digest(fs::read(iso)?)
    );
    let outcome = (|| -> Result<()> {
        let disk = directory(root, "disks")?.join(format!(
            "gui-sdk-{}.ext2",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        // The mounted ABI marker admits native outputs; compiler executables remain explicitly
        // Linux-personality archive records. Do not infer an ABI from generated ELF headers.
        disk::create_profile(root, &disk, false)?;
        evidence.push_str(&format!(
            "disk={}\npersistent_abi=anova.native.v1\n",
            disk.display()
        ));
        session(root, iso, &disk, &artifacts, false, &mut evidence)?;
        let before = inspect(root, &disk)?;
        fs::write(artifacts.join("files-before.sha256"), hash_text(&before))?;
        command(Command::new("e2fsck").args(["-f", "-n"]).arg(&disk))?;
        session(root, iso, &disk, &artifacts, true, &mut evidence)?;
        let after = inspect(root, &disk)?;
        fs::write(artifacts.join("files-after.sha256"), hash_text(&after))?;
        if before != after {
            return Err("GUI SDK source or native executable changed across reboot".into());
        }
        command(Command::new("e2fsck").args(["-f", "-n"]).arg(&disk))?;
        evidence.push_str("guest_editor=true\nguest_compiler=true\nindependent_languages=c,cpp,rust\nsource_bytes_exact=true\nnative_elf_valid=true\nfailed_build_retained=true\nreboot=true\nretained_hashes=true\ne2fsck=true\n");
        Ok(())
    })();
    match &outcome {
        Ok(()) => evidence.push_str("result=Ok\n"),
        Err(error) => evidence.push_str(&format!("result=Err\nerror={error}\n")),
    }
    fs::write(&result, evidence)?;
    if let Err(error) = outcome {
        return Err(format!("GUI SDK gate: {error}; see {}", result.display()).into());
    }
    println!(
        "Verified guest-built native GUI C/C++/Rust applications, physical input, scanout and reboot retention"
    );
    Ok(())
}

fn hash_text(hashes: &[(String, String)]) -> String {
    hashes
        .iter()
        .map(|(name, hash)| format!("{hash}  {name}\n"))
        .collect()
}

/// Recheck retained guest outputs without recompiling them. Require the successful build
/// gate's exact ISO and source/ELF hashes so replay cannot stand in for guest compilation.
pub fn replay(root: &Path) -> Result<()> {
    let artifacts = directory(root, "gui-sdk")?;
    let previous = fs::read_to_string(artifacts.join("gui-sdk-test.result.txt"))?;
    let field = |name: &str| -> Result<&str> {
        let prefix = format!("{name}=");
        let mut values = previous
            .lines()
            .filter_map(|line| line.strip_prefix(&prefix));
        let value = values.next().ok_or("Missing GUI SDK build evidence")?;
        if values.next().is_some() {
            return Err("Ambiguous GUI SDK build evidence".into());
        }
        Ok(value)
    };
    if field("result")? != "Ok" {
        return Err("GUI SDK replay requires a successful gui-sdk-test build".into());
    }
    let disk = disk::user_path(root, field("disk")?)?;
    let iso = root.join("build/images/anova-gui-sdk.iso");
    checked_file(&iso)?;
    if format!("{:x}", Sha256::digest(fs::read(&iso)?)) != field("iso_sha256")? {
        return Err("GUI SDK replay ISO differs from the successful build gate".into());
    }
    let before = inspect(root, &disk)?;
    let expected = fs::read_to_string(artifacts.join("files-after.sha256"))?;
    if hash_text(&before) != expected {
        return Err("GUI SDK replay inputs differ from retained build hashes".into());
    }
    let result = artifacts.join("replay.result.txt");
    output_path(&result)?;
    fs::write(&result, "result=Running\n")?;
    let mut evidence = format!(
        "iso_sha256={}\ndisk={}\n",
        field("iso_sha256")?,
        disk.display()
    );
    let outcome = (|| -> Result<()> {
        session(root, &iso, &disk, &artifacts, true, &mut evidence)?;
        if inspect(root, &disk)? != before {
            return Err("GUI SDK replay modified retained source or ELF bytes".into());
        }
        command(Command::new("e2fsck").args(["-f", "-n"]).arg(&disk))?;
        Ok(())
    })();
    evidence.push_str(&match &outcome {
        Ok(()) => "retained_hashes=true\nresult=Ok\n".to_owned(),
        Err(error) => format!("result=Err\nerror={error}\n"),
    });
    fs::write(result, evidence)?;
    outcome
}

fn session(
    root: &Path,
    iso: &Path,
    disk: &Path,
    artifacts: &Path,
    reboot: bool,
    evidence: &mut String,
) -> Result<()> {
    let name = if reboot { "reboot" } else { "build" };
    let vars = Temporary::new(root, "gui-sdk")?;
    let socket = vars.0.join("qmp.sock");
    let serial = artifacts.join(format!("{name}.serial.log"));
    let diagnostic = artifacts.join(format!("{name}.qemu.log"));
    let qmp_log = artifacts.join(format!("{name}.qmp.log"));
    for path in [&serial, &diagnostic, &qmp_log] {
        output_path(path)?;
    }
    let mut cmd = qemu_disk(root, iso, &vars.0, false, Some(disk), false)?;
    cmd.args(["-m", "3G", "-vga", "std", "-serial", "stdio"])
        .arg("-qmp")
        .arg(format!("unix:{},server=on,wait=off", socket.display()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(File::create(&diagnostic)?);
    evidence.push_str(&format!("qemu_{name}={cmd:?}\n"));
    println!("Running GUI SDK {name}: {cmd:?}");
    let mut guest = Guest(cmd.spawn()?);
    let output = transcript::Transcript::capture(
        guest.0.stdout.take().ok_or("Missing GUI SDK UART output")?,
        File::create(&serial)?,
    )?;
    let uart = guest.0.stdin.take().ok_or("Missing GUI SDK UART input")?;
    let start = Instant::now();
    let qmp = Qmp::connect(&socket, &qmp_log, start)?;
    let mut driver = Driver {
        guest,
        output,
        uart,
        qmp,
        start,
        observed: 0,
        pointer: (400, 300),
        buttons: 0,
        expected_faults: Vec::new(),
        pending_fault: None,
    };
    driver.wait("anova> ")?;
    let boot = driver.output.text();
    for required in ["[GRAPHICS:PRESENT]", "[INPUT:READY]", "[DISK:MOUNT]"] {
        if !boot.contains(required) {
            return Err(format!("GUI SDK boot lacks {required}").into());
        }
    }
    driver.command("cd /persist", 0)?;
    if !reboot {
        for fixture in FIXTURES {
            println!("GUI SDK {name}: editing and compiling {}", fixture.language);
            let source = fs::read_to_string(root.join("tests/guest/gui").join(fixture.source))?;
            driver.edit(fixture.source, &source)?;
            driver.command(&format!("gui-build {}", fixture.language), 0)?;
        }
        driver.rejected_build(root)?;
    }
    for fixture in FIXTURES {
        println!("GUI SDK {name}: running {}", fixture.language);
        scenario(&mut driver, artifacts, name, fixture, evidence)?;
    }
    driver.serial(b"echo M14-GUI-SDK-SERIAL-RECOVERY\r")?;
    driver.wait("\r\nM14-GUI-SDK-SERIAL-RECOVERY\r\n")?;
    driver.wait("anova> ")?;
    driver.serial(b"shutdown\r")?;
    driver.shutdown()?;
    evidence.push_str(&format!("{name}_serial_recovery=true\n{name}_qemu_exit=0\n{name}_acpi_shutdown=true\n{name}_timed_out=false\n"));
    drop(driver);
    vars.cleanup()?;
    Ok(())
}

fn scenario(
    driver: &mut Driver,
    artifacts: &Path,
    boot: &str,
    fixture: Fixture,
    evidence: &mut String,
) -> Result<()> {
    let start = driver.output.text().len();
    if fixture.language == "fault" {
        driver.pending_fault = Some(start);
    }
    driver.serial(format!("gui-run /persist/gui-{}\r", fixture.language).as_bytes())?;
    if fixture.language == "contract" {
        driver.wait("[GUI:CONTRACT:DONE]")?;
    } else if fixture.language == "fault" {
        driver.wait("[GUI:FAULT:INJECT]")?;
    } else {
        driver.wait(&format!("[GUI:{}:READY]", fixture.marker))?;
        if fixture.language == "terminal" {
            driver.wait_seen(
                start,
                "[GUI:TERMINAL:PIPE] command=echo output=M14 native output exit=0",
            )?;
        }
        driver.screenshot(
            artifacts,
            boot,
            fixture.language,
            "initial",
            false,
            evidence,
        )?;
        if fixture.language != "terminal" {
            driver.key("a")?;
            driver.wait(&format!("[GUI:{}:KEY] codepoint=97", fixture.marker))?;
            driver.screenshot(artifacts, boot, fixture.language, "text", true, evidence)?;
            let before_click = driver.output.text().len();
            driver.click(120, 210)?;
            driver.wait_seen(
                before_click,
                &format!("[GUI:{}:POINTER] activated=true", fixture.marker),
            )?;
            driver.screenshot(artifacts, boot, fixture.language, "clicked", true, evidence)?;
        } else {
            for (index, key) in ["e", "c", "h", "o", "spc", "h", "i"]
                .into_iter()
                .enumerate()
            {
                driver.key(key)?;
                driver.wait(&format!("[GUI:TERMINAL:INPUT] length={}", index + 1))?;
            }
            driver.key("ret")?;
            driver.wait("[GUI:TERMINAL:COMMAND] command=echo hi output=hi exit=0")?;
            driver.wait("[GUI:TERMINAL:RENDERED]")?;
            driver.screenshot(
                artifacts,
                boot,
                fixture.language,
                "command",
                false,
                evidence,
            )?;
            evidence.push_str(&format!(
                "{boot}_terminal_physical_command=true\n{boot}_terminal_native_child_wait=true\n"
            ));
        }
        driver.key("q")?;
        driver.wait(&format!("[GUI:{}:DONE]", fixture.marker))?;
    }
    let client = driver.wait_line("[GUI-SDK:WAIT] role=client ")?;
    let client_pid = if fixture.language == "fault" {
        validate_wait_status(&client, "client", "1", "14")?
    } else {
        validate_wait(&client, "client")?
    };
    driver.key("f11")?;
    let compositor = driver.wait_line("[GUI-SDK:WAIT] role=compositor ")?;
    let compositor_pid = validate_wait(&compositor, "compositor")?;
    if client_pid == compositor_pid {
        return Err("GUI SDK client and compositor did not occupy independent processes".into());
    }
    driver.wait("[GUI-SDK:RUN] all_children_reaped=true")?;
    driver.wait("anova> ")?;
    let text = driver.output.text();
    let tail = text
        .get(start..)
        .ok_or("Lost GUI SDK scenario transcript")?;
    for pid in [client_pid, compositor_pid] {
        if fixture.language == "fault" && pid == client_pid {
            if !tail.lines().any(|line| {
                fields_match(
                    line,
                    "[USER:FAULT]",
                    &[
                        ("pid", &pid.to_string()),
                        ("vector", "14"),
                        ("address", "0x1000"),
                    ],
                )
            }) {
                return Err("GUI SDK invalid output pointer did not produce the expected isolated page fault".into());
            }
            evidence.push_str(&format!(
                "{boot}_fault_client_isolated=true\n{boot}_fault_vector=14\n"
            ));
            continue;
        }
        if !tail.lines().any(|line| {
            fields_match(
                line,
                "[USER:EXIT]",
                &[("pid", &pid.to_string()), ("status", "0")],
            )
        }) {
            return Err(
                format!("Missing independent kernel exit record for GUI SDK child {pid}").into(),
            );
        }
    }
    evidence.push_str(&format!("{boot}_{}_client_pid={client_pid}\n{boot}_{}_compositor_pid={compositor_pid}\n{boot}_{}_clean_wait=true\n", fixture.language, fixture.language, fixture.language));
    driver.status(0)
}

struct Driver {
    guest: Guest,
    output: transcript::Transcript,
    uart: ChildStdin,
    qmp: Qmp,
    start: Instant,
    observed: usize,
    pointer: (u32, u32),
    buttons: u8,
    expected_faults: Vec<u64>,
    pending_fault: Option<usize>,
}

impl Driver {
    fn poll(&mut self) -> Result<()> {
        self.output.poll()?;
        let text = self.output.text();
        if let Some(start) = self.pending_fault
            && let Some(line) = text.get(start..).and_then(|tail| {
                tail.split_inclusive('\n').find(|line| {
                    line.ends_with('\n')
                        && fields_match(line, "[GUI-SDK:CHILD]", &[("role", "client")])
                })
            })
        {
            self.expected_faults.push(pid_field(line)?);
            self.pending_fault = None;
        }
        check_guest(
            &text,
            self.start.elapsed() >= DEADLINE,
            &self.expected_faults,
        )?;
        if let Some(status) = self.guest.0.try_wait()? {
            self.output.finish()?;
            return Err(format!("GUI SDK guest exited before completion: {status}").into());
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

    fn wait_line(&mut self, marker: &str) -> Result<String> {
        loop {
            self.poll()?;
            let text = self.output.text();
            if let Some(line) = text.get(self.observed..).and_then(|tail| {
                tail.split_inclusive('\n')
                    .find(|line| line.ends_with('\n') && line.contains(marker))
            }) {
                let line = line.to_owned();
                self.observed = self
                    .output
                    .find_from(self.observed, &line)
                    .ok_or("Lost GUI SDK line cursor")?
                    + line.len();
                return Ok(line);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_seen(&mut self, start: usize, marker: &str) -> Result<()> {
        loop {
            self.poll()?;
            if self.output.find_from(start, marker).is_some() {
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

    fn command(&mut self, command: &str, expected: u32) -> Result<()> {
        self.serial(format!("{command}\r").as_bytes())?;
        self.wait("anova> ")?;
        self.status(expected)
    }

    fn status(&mut self, expected: u32) -> Result<()> {
        let cursor = self.observed;
        self.serial(b"status\r")?;
        loop {
            self.poll()?;
            if let Some(code) = self.output.shell_status_from(cursor) {
                if code != expected {
                    return Err(format!(
                        "GUI SDK shell command returned {code}; expected {expected}"
                    )
                    .into());
                }
                self.wait("anova> ")?;
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn edit(&mut self, name: &str, source: &str) -> Result<()> {
        if !source.ends_with('\n')
            || !source.is_ascii()
            || source.lines().any(|line| line.len() > 250 || line == ".")
        {
            return Err(
                format!("GUI SDK editor fixture cannot be represented exactly: {name}").into(),
            );
        }
        self.serial(format!("edit {name}\r").as_bytes())?;
        self.wait("edit> ")?;
        self.serial(b"a\r")?;
        self.wait("a\r\n")?;
        for line in source.lines() {
            self.serial(format!("{line}\r").as_bytes())?;
            self.wait(&format!("{line}\r\n"))?;
        }
        self.serial(b".\r")?;
        self.wait("edit> ")?;
        self.serial(b"w\r")?;
        self.wait("[EDIT:SAVED] fsync=ok")?;
        self.wait("edit> ")?;
        self.serial(b"q\r")?;
        self.wait("anova> ")?;
        self.status(0)
    }

    fn rejected_build(&mut self, root: &Path) -> Result<()> {
        self.command("cp /persist/gui-c /persist/gui-c-retained", 0)?;
        self.serial(b"edit main.c\r")?;
        self.wait("edit> ")?;
        self.serial(b"a\r")?;
        self.wait("a\r\n")?;
        self.serial(b"#error M14_GUI_REJECT\r")?;
        self.wait("#error M14_GUI_REJECT\r\n")?;
        self.serial(b".\r")?;
        self.wait("edit> ")?;
        self.serial(b"w\r")?;
        self.wait("[EDIT:SAVED] fsync=ok")?;
        self.wait("edit> ")?;
        self.serial(b"q\r")?;
        self.wait("anova> ")?;
        let start = self.output.text().len();
        self.command("gui-build c", 1)?;
        self.wait_seen(start, "error: #error M14_GUI_REJECT")?;
        let line = fs::read_to_string(root.join("tests/guest/gui/main.c"))?
            .lines()
            .count()
            + 1;
        self.serial(b"edit main.c\r")?;
        self.wait("edit> ")?;
        self.serial(format!("d {line}\r").as_bytes())?;
        self.wait("edit> ")?;
        self.serial(b"w\r")?;
        self.wait("[EDIT:SAVED] fsync=ok")?;
        self.wait("edit> ")?;
        self.serial(b"q\r")?;
        self.wait("anova> ")?;
        self.status(0)
    }

    fn key(&mut self, name: &str) -> Result<()> {
        for down in [true, false] {
            self.qmp.command("input-send-event", json!({ "events": [
                { "type": "key", "data": { "down": down, "key": { "type": "qcode", "data": name } } }
            ] }))?;
            thread::sleep(Duration::from_millis(30));
        }
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
            let expected = format!(
                "[GUI:POINTER] x={} y={} buttons={}",
                target.0, target.1, self.buttons
            );
            self.wait(&expected)?;
            self.pointer = target;
        }
        Ok(())
    }

    fn click(&mut self, x: u32, y: u32) -> Result<()> {
        self.move_to(x, y)?;
        for down in [true, false] {
            self.qmp.command(
                "input-send-event",
                json!({ "events": [
                { "type": "btn", "data": { "down": down, "button": "left" } }
            ] }),
            )?;
            self.buttons = u8::from(down);
            self.wait(&format!(
                "[GUI:POINTER] x={x} y={y} buttons={}",
                self.buttons
            ))?;
        }
        Ok(())
    }

    fn screenshot(
        &mut self,
        artifacts: &Path,
        boot: &str,
        language: &str,
        phase: &str,
        typed: bool,
        evidence: &mut String,
    ) -> Result<()> {
        self.poll()?;
        let path = artifacts.join(format!("{boot}-{language}-{phase}.ppm"));
        output_path(&path)?;
        if path.exists() {
            fs::remove_file(&path)?;
        }
        self.qmp
            .command("screendump", json!({ "filename": path, "format": "ppm" }))?;
        let bytes = read_ppm(&path)?;
        verify_scanout(&bytes, language, phase, typed)?;
        evidence.push_str(&format!("screenshot_{boot}_{language}_{phase}={}\nscreenshot_{boot}_{language}_{phase}_sha256={:x}\n", path.display(), Sha256::digest(&bytes)));
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
                        "GUI SDK guest did not perform clean ACPI shutdown: {status}"
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
    if !tail.is_empty() && !tail.starts_with(char::is_whitespace) {
        return false;
    }
    fields.iter().all(|(key, value)| {
        let mut found = tail
            .split_whitespace()
            .filter_map(|word| word.split_once('='))
            .filter(|(actual, _)| actual == key);
        found.next().is_some_and(|(_, actual)| actual == *value) && found.next().is_none()
    })
}

fn validate_wait(line: &str, role: &str) -> Result<u64> {
    validate_wait_status(line, role, "0", "0")
}

fn validate_wait_status(line: &str, role: &str, reason: &str, code: &str) -> Result<u64> {
    if !fields_match(
        line,
        "[GUI-SDK:WAIT]",
        &[("role", role), ("reason", reason), ("code", code)],
    ) {
        return Err(format!(
            "GUI SDK child failed or was not reaped cleanly: {}",
            line.trim()
        )
        .into());
    }
    pid_field(line)
}

fn pid_field(line: &str) -> Result<u64> {
    let mut pids = line
        .split_whitespace()
        .filter_map(|word| word.strip_prefix("pid="));
    let pid = pids
        .next()
        .ok_or("Missing GUI SDK child PID")?
        .parse::<u64>()?;
    if pid == 0 || pids.next().is_some() {
        return Err("Ambiguous GUI SDK child identity".into());
    }
    Ok(pid)
}

fn check_guest(text: &str, timed_out: bool, expected_faults: &[u64]) -> Result<()> {
    if timed_out {
        return Err("GUI SDK guest exceeded its 1,200-second watchdog".into());
    }
    for marker in [
        "[PANIC]",
        "[EXCEPTION]",
        "[INIT:FAIL]",
        "[SHELL:FAIL]",
        "[FS:FAIL]",
        "[EDIT:FAIL]",
        "edit: command/save failed; buffer retained",
        "[POWER:FAIL]",
        "[GUI:FAIL]",
        "[GUI:CLIENT_FAIL]",
        "[GUI-SDK:FAIL]",
        "[USER:RETURNREJECT]",
        "[INPUT:FAIL]",
    ] {
        if text.contains(marker) {
            return Err(format!("GUI SDK guest reported {marker}").into());
        }
    }
    for line in text
        .split_inclusive('\n')
        .filter(|line| line.ends_with('\n') && line.contains("[USER:FAULT]"))
    {
        if !expected_faults.iter().any(|pid| {
            fields_match(
                line,
                "[USER:FAULT]",
                &[
                    ("pid", &pid.to_string()),
                    ("vector", "14"),
                    ("address", "0x1000"),
                ],
            )
        }) {
            return Err(format!("Unexpected GUI SDK process fault: {}", line.trim()).into());
        }
    }
    for pid in expected_faults {
        if text
            .lines()
            .filter(|line| fields_match(line, "[USER:FAULT]", &[("pid", &pid.to_string())]))
            .count()
            > 1
        {
            return Err("GUI SDK child fault occurred more than once".into());
        }
    }
    Ok(())
}

fn verify_scanout(bytes: &[u8], language: &str, phase: &str, typed: bool) -> Result<()> {
    let image = parse_ppm(bytes)?;
    if image.width != 800 || image.height != 600 {
        return Err("GUI SDK requires an actual 800x600 scanout".into());
    }
    // These accepted literal screen coordinates, colors and bitmap rows are specified
    // independently from the runtime renderer; no production drawing function is reused.
    let mut samples = vec![(80, 80, 0xe0e8f0), (81, 81, 0x354d66), (90, 120, 0x182838)];
    if language != "terminal" {
        samples.extend([
            (92, 124, 0x3070b0),
            (307, 135, 0x3070b0),
            (308, 135, 0x182838),
        ]);
    }
    if language == "terminal" {
        // Literal bitmaps from actual native child output at local(12,76). The command phase
        // must replace M14 with HI after physical keyboard input and a successful child wait.
        let glyphs = if phase == "command" {
            vec![
                (92, [17u8, 17, 17, 31, 17, 17, 17]),
                (98, [14, 4, 4, 4, 4, 4, 14]),
            ]
        } else {
            vec![
                (92, [17u8, 27, 21, 21, 17, 17, 17]),
                (98, [4, 12, 4, 4, 4, 4, 14]),
                (104, [2, 6, 10, 18, 31, 2, 2]),
            ]
        };
        for (left, bitmap) in glyphs {
            for (row, bits) in bitmap.into_iter().enumerate() {
                for column in 0..5 {
                    samples.push((
                        left + column,
                        156 + row as u32,
                        if bits & (1 << (4 - column)) != 0 {
                            0xffffff
                        } else {
                            0x182838
                        },
                    ));
                }
            }
        }
    }
    if language != "terminal" {
        samples.extend([
            (92, 164, 0x788898),
            (93, 165, 0xffffff),
            (92, 196, 0x788898),
        ]);
        if typed {
            let left = if language == "note" { 120 } else { 96 };
            for (row, bits) in [14u8, 17, 17, 31, 17, 17, 17].into_iter().enumerate() {
                for column in 0..5 {
                    samples.push((
                        left + column,
                        169 + row as u32,
                        if bits & (1 << (4 - column)) != 0 {
                            0x182838
                        } else {
                            0xffffff
                        },
                    ));
                }
            }
        } else if language != "note" {
            samples.extend([
                (96, 169, 0xffffff),
                (97, 169, 0xffffff),
                (100, 175, 0xffffff),
            ]);
        }
    }
    for (x, y, expected) in samples {
        let offset = ((y * image.width + x) * 3) as usize;
        let actual = u32::from_be_bytes([
            0,
            image.rgb[offset],
            image.rgb[offset + 1],
            image.rgb[offset + 2],
        ]);
        if actual != expected {
            return Err(format!(
                "GUI SDK {language} scanout differs at ({x},{y}): {actual:06x} != {expected:06x}"
            )
            .into());
        }
    }
    Ok(())
}

fn inspect(root: &Path, path: &Path) -> Result<Vec<(String, String)>> {
    use anova_storage::ext2::Ext2;
    let mut disk = Ext2::mount(Box::new(disk::Image::open(path)?))
        .map_err(|e| format!("GUI SDK disk: {e}"))?;
    let entries = disk
        .entries(2)
        .map_err(|e| format!("GUI SDK directory: {e}"))?;
    let mut hashes = Vec::new();
    let mut original_c = Vec::new();
    for fixture in FIXTURES {
        for (name, source) in [
            (fixture.source.to_owned(), true),
            (format!("gui-{}", fixture.language), false),
        ] {
            let entry = entries
                .iter()
                .find(|entry| &entry.name[..entry.length] == name.as_bytes())
                .ok_or_else(|| format!("Missing guest GUI SDK file {name}"))?;
            let inode = disk
                .inode(entry.inode)
                .map_err(|e| format!("GUI SDK inode: {e}"))?;
            let mut bytes = vec![0; inode.size];
            if disk
                .read(entry.inode, 0, &mut bytes)
                .map_err(|e| format!("GUI SDK disk read: {e}"))?
                != bytes.len()
            {
                return Err(format!("Short GUI SDK file read: {name}").into());
            }
            if source {
                if bytes != fs::read(root.join("tests/guest/gui").join(&name))? {
                    return Err(format!("Guest GUI SDK editor bytes differ: {name}").into());
                }
            } else {
                anova_elf::parse(&bytes)
                    .map_err(|e| format!("Invalid guest-built native GUI ELF {name}: {e:?}"))?;
                if !inode.executable {
                    return Err(format!("Guest-built GUI ELF is not executable: {name}").into());
                }
                if name == "gui-c" {
                    original_c = bytes.clone();
                }
            }
            hashes.push((name, format!("{:x}", Sha256::digest(&bytes))));
        }
    }
    let retained = entries
        .iter()
        .find(|entry| &entry.name[..entry.length] == b"gui-c-retained")
        .ok_or("Missing retained pre-rejection GUI binary")?;
    let inode = disk
        .inode(retained.inode)
        .map_err(|e| format!("Retained GUI SDK inode: {e}"))?;
    let mut bytes = vec![0; inode.size];
    if disk
        .read(retained.inode, 0, &mut bytes)
        .map_err(|e| format!("Retained GUI SDK read: {e}"))?
        != bytes.len()
        || bytes != original_c
    {
        return Err("Rejected GUI SDK source build damaged its previous executable".into());
    }
    hashes.push((
        "gui-c-retained".into(),
        format!("{:x}", Sha256::digest(&bytes)),
    ));
    disk.shutdown()
        .map_err(|e| format!("GUI SDK inspection shutdown: {e}"))?;
    Ok(hashes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_requires_unique_successful_child_identity() {
        assert_eq!(
            validate_wait(
                "[GUI-SDK:WAIT] role=client pid=42 reason=0 code=0\n",
                "client"
            )
            .unwrap(),
            42
        );
        for line in [
            "[GUI-SDK:WAIT] role=client pid=42 reason=1 code=0\n",
            "[GUI-SDK:WAIT] role=client pid=42 reason=0 code=1\n",
            "[GUI-SDK:WAIT] role=client pid=0 reason=0 code=0\n",
            "[GUI-SDK:WAIT] role=client pid=1 pid=2 reason=0 code=0\n",
            "[GUI-SDK:WAIT] role=compositor pid=42 reason=0 code=0\n",
        ] {
            assert!(validate_wait(line, "client").is_err(), "{line}");
        }
    }

    #[test]
    fn faults_fatal_markers_and_timeout_fail_the_gate() {
        assert!(check_guest("[GUI:C:READY]\n", false, &[]).is_ok());
        assert!(check_guest("", true, &[]).is_err());
        for marker in [
            "[USER:FAULT] pid=4 vector=6\n",
            "[GUI-SDK:FAIL]",
            "[PANIC]",
            "[POWER:FAIL]",
            "edit: command/save failed; buffer retained",
        ] {
            assert!(check_guest(marker, false, &[]).is_err());
        }
    }

    #[test]
    fn only_the_declared_single_client_page_fault_is_accepted() {
        let fault = "[USER:FAULT] pid=42 vector=14 error=0x6 address=0x1000 rip=0x100000\n";
        assert!(check_guest(fault, false, &[42]).is_ok());
        assert!(check_guest(fault, false, &[]).is_err());
        assert!(check_guest(fault, false, &[41]).is_err());
        assert!(check_guest(&fault.replace("vector=14", "vector=6"), false, &[42]).is_err());
        assert!(
            check_guest(
                &fault.replace("address=0x1000", "address=0x2000"),
                false,
                &[42]
            )
            .is_err()
        );
        assert!(check_guest(&fault.repeat(2), false, &[42]).is_err());
    }
}
