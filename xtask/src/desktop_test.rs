// SPDX-License-Identifier: Apache-2.0
//! Verify supervised desktop input, actual fault/restart lifetimes and saved bytes across reboot.
//! Keep independent UART/QMP evidence, fixed budgets and explicit ext2/device-fallback coverage.

use super::*;
use graphics::{Qmp, parse_ppm, read_ppm};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{io::Write, process::ChildStdin};

const DEADLINE: Duration = Duration::from_secs(180);
const INTERACTION: Duration = Duration::from_secs(5);
const SAVED: &[u8] = b"M17-persisted-work\n";

#[derive(Clone, Copy)]
struct Profile {
    name: &'static str,
    full: bool,
    display: bool,
    input: bool,
    retained: bool,
}

const JOURNAL: Profile = Profile {
    name: "journal",
    full: true,
    display: true,
    input: true,
    retained: false,
};

pub fn verify(root: &Path) -> Result<()> {
    let work = directory(root, "desktop-test")?;
    let result = work.join("desktop-test.result.txt");
    output_path(&result)?;
    fs::write(&result, "result=Running\n")?;
    for name in ["journal", "reboot", "ext2", "no-display", "no-input"] {
        let path = work.join(format!("{name}.result.txt"));
        output_path(&path)?;
        fs::write(path, "result=Pending\n")?;
    }
    // These bounds are fixed before boot and are checked against scheduler-boundary samples.
    let mut evidence = String::from(
        "profile=q35,tcg,qemu64+rdrand,1BSP,3GiB,800x600,PS2\ninteraction_budget_ms=5000\nheap_budget_bytes=12582912\nframe_delta_budget=16384\ntask_budget=32\nfd_per_process_budget=32\nvfs_slot_budget=256\ngui_clients=4\nsurface_max=320x240\n",
    );
    let outcome = (|| -> Result<()> {
        let archive = desktop::prepare(root)?;
        assets::check(root)?;
        let test_kernel = build(root, Mode::DesktopTest)?;
        let test_iso = image_contents(root, &test_kernel, Some(&archive), "desktop-test")?;
        let kernel = build(root, Mode::Desktop)?;
        let normal_iso = image_contents(root, &kernel, Some(&archive), "desktop")?;
        for (name, path) in [("test_iso", &test_iso), ("normal_iso", &normal_iso)] {
            evidence.push_str(&format!(
                "{name}_sha256={:x}\n",
                Sha256::digest(fs::read(path)?)
            ));
        }
        evidence.push_str(&fs::read_to_string(root.join("build/desktop/inputs.txt"))?);
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let disks = directory(root, "disks")?;
        let journal = disks.join(format!("desktop-{stamp}.ext4"));
        foundation_journal::create(root, &journal, false)?;
        evidence.push_str(&format!("journal_disk={}\n", journal.display()));
        session(root, &test_iso, &journal, &work, JOURNAL, &mut evidence)?;
        inspect(&journal, &work, "journal", &mut evidence)?;
        session(
            root,
            &normal_iso,
            &journal,
            &work,
            Profile {
                name: "reboot",
                full: false,
                retained: true,
                ..JOURNAL
            },
            &mut evidence,
        )?;
        inspect(&journal, &work, "reboot", &mut evidence)?;
        let ext2 = disks.join(format!("desktop-{stamp}.ext2"));
        disk::create_profile(root, &ext2, false)?;
        evidence.push_str(&format!("ext2_disk={}\n", ext2.display()));
        session(
            root,
            &normal_iso,
            &ext2,
            &work,
            Profile {
                name: "ext2",
                full: false,
                ..JOURNAL
            },
            &mut evidence,
        )?;
        inspect(&ext2, &work, "ext2", &mut evidence)?;
        for (name, display, input) in [("no-display", false, true), ("no-input", true, false)] {
            session(
                root,
                &normal_iso,
                &ext2,
                &work,
                Profile {
                    name,
                    full: false,
                    display,
                    input,
                    retained: true,
                },
                &mut evidence,
            )?;
            foundation_journal::check(&ext2)?;
        }
        Ok(())
    })();
    evidence.push_str(&match &outcome {
        Ok(()) => "result=Ok\n".to_owned(),
        Err(error) => format!("result=Err\nerror={error}\n"),
    });
    fs::write(&result, evidence)?;
    outcome?;
    println!(
        "Verified desktop clients/input, close/relaunch, quota/fault/restart recovery, UART, saved work and five clean boots"
    );
    Ok(())
}

fn inspect(disk: &Path, work: &Path, name: &str, evidence: &mut String) -> Result<()> {
    foundation_journal::check(disk)?;
    let saved = work.join(format!("{name}-saved.txt"));
    output_path(&saved)?;
    if saved.exists() {
        fs::remove_file(&saved)?;
    }
    command(
        Command::new("debugfs")
            .args(["-R", &format!("dump /m17-work.txt {}", saved.display())])
            .arg(disk),
    )?;
    if fs::read(saved)? != SAVED {
        return Err("Desktop work bytes changed across session replacement/shutdown/reboot".into());
    }
    evidence.push_str(&format!(
        "{name}_saved_sha256={:x}\n{name}_e2fsck=true\n",
        Sha256::digest(SAVED)
    ));
    Ok(())
}

fn session(
    root: &Path,
    iso: &Path,
    disk: &Path,
    work: &Path,
    profile: Profile,
    evidence: &mut String,
) -> Result<()> {
    let Profile {
        name,
        full,
        display,
        input,
        retained,
    } = profile;
    let vars = Temporary::new(root, "desktop-test")?;
    let port = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?
        .local_addr()?
        .port();
    let mut cmd = qemu_disk(root, iso, &vars.0, false, Some(disk), false)?;
    cmd.args(["-m", "3G", "-vga", if display { "std" } else { "none" }]);
    if !input {
        cmd.args(["-machine", "i8042=off"]);
    }
    cmd.arg("-qmp")
        .arg(format!("tcp:127.0.0.1:{port},server=on,wait=off"))
        .args(["-serial", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(File::create(work.join(format!("{name}.qemu.log")))?);
    evidence.push_str(&format!("{name}_command={cmd:?}\n"));
    let mut guest = Guest(cmd.spawn()?);
    let output = transcript::Transcript::capture(
        guest.0.stdout.take().ok_or("Missing desktop UART output")?,
        File::create(work.join(format!("{name}.serial.log")))?,
    )?;
    let uart = guest.0.stdin.take().ok_or("Missing desktop UART input")?;
    let start = Instant::now();
    let qmp = Qmp::connect_tcp(port, &work.join(format!("{name}.qmp.log")), start)?;
    let mut d = Driver {
        guest,
        output,
        uart,
        qmp,
        start,
        cursor: 0,
        pointer: (400, 300),
        buttons: 0,
        faults: Vec::new(),
    };
    d.wait("anova> ")?;
    if display && input {
        d.ready(0, 0)?;
        if full {
            scenario(&mut d, work, evidence)?;
        }
    } else {
        d.wait("[INIT] desktop exited; COM1 shell remains available")?;
        let boot = d.output.text();
        let marker = if display {
            "[INPUT:FALLBACK] keyboard=false pointer=false"
        } else {
            "[GRAPHICS:FALLBACK] reason=no-framebuffer;"
        };
        if !boot.contains(marker) {
            return Err("Desktop missing-device fallback was not established".into());
        }
    }
    let recovery = Instant::now();
    d.command("echo M17-SERIAL-RECOVERY", 0)?;
    if recovery.elapsed() > INTERACTION {
        return Err("Desktop UART recovery exceeded five seconds".into());
    }
    evidence.push_str(&format!(
        "{name}_uart_latency_ms={}\n",
        recovery.elapsed().as_millis()
    ));
    if retained || full {
        // The full scenario saved before the fault/replacements. Read it now without
        // rewriting it: a post-recovery write could hide lost or corrupted user data.
        let from = d.output.position();
        d.command("cat /persist/m17-work.txt", 0)?;
        if !d.output.text()[from..].contains("\r\nM17-persisted-work\r\n") {
            return Err("Rebooted desktop could not reopen saved work".into());
        }
    } else {
        d.command("echo M17-persisted-work > /persist/m17-work.txt", 0)?;
        d.command("sync", 0)?;
    }
    if display && input && !full {
        d.click(35, 60)?;
        let from = d.output.position();
        d.key("q")?;
        d.wait_since(from, "[INIT] desktop exited; COM1 shell remains available")?;
    }
    if full {
        let from = d.output.position();
        d.command("desktop check", 0)?;
        let text = d.output.text();
        let pressure = &text[from..];
        let line = last_line(pressure, "[DESKTOP:BOUNDARIES]")?;
        if field(line, "task_pressure")? != 29 || field(line, "fd_pressure")? != 29 {
            return Err("Desktop exit left task slots or descriptors unavailable".into());
        }
        evidence.push_str("post_session_task_slots_reused=29\npost_session_fds_reused=29\n");
    }
    d.cursor = d.output.position();
    d.serial(b"exit\r")?;
    // init must restart only COM1 after the GUI's normal exit or bounded failure.
    d.wait("[INIT] COM1 shell exited; restarting")?;
    d.wait("anova> ")?;
    d.command("echo M17-SHELL-RESTARTED", 0)?;
    d.shutdown()?;
    let text = d.output.text();
    check_guest(&text, false, &d.faults)?;
    validate_resources(&text)?;
    for line in text
        .lines()
        .filter(|line| line.starts_with("[M15:RESOURCES]") || line.starts_with("[M16:STORAGE]"))
    {
        evidence.push_str(&format!("{name}_sample={line}\n"));
    }
    fs::write(
        work.join(format!("{name}.result.txt")),
        "result=Ok\nshutdown=true\nshell_restart=true\n",
    )?;
    vars.cleanup()
}

fn scenario(d: &mut Driver, work: &Path, evidence: &mut String) -> Result<()> {
    // Each actual language implementation must pass its own contract before operating together.
    let mut windows = Vec::new();
    for (index, language) in ["C", "CPP", "RUST"].into_iter().enumerate() {
        let pid = d.client_pid(index)?;
        let (x, y) = d.window(pid)?;
        windows.push((x, y));
        let began = Instant::now();
        d.click(x + 30, y + 96)?;
        let from = d.output.position();
        d.key("a")?;
        d.wait_since(from, &format!("[GUI:{language}:KEY] codepoint=97"))?;
        d.screenshot(
            work,
            &format!("typed-{language}"),
            &[(x, y, true)],
            evidence,
        )?;
        if began.elapsed() > INTERACTION {
            return Err("Desktop key-to-verified-scanout exceeded five seconds".into());
        }
        evidence.push_str(&format!(
            "{language}_input_scanout_ms={}\n",
            began.elapsed().as_millis()
        ));
        let from = d.output.position();
        d.click(x + 45, y + 130)?;
        d.wait_since(from, &format!("[GUI:{language}:POINTER] activated=true"))?;
    }
    d.screenshot(
        work,
        "three-clients",
        &windows
            .iter()
            .map(|&(x, y)| (x, y, false))
            .collect::<Vec<_>>(),
        evidence,
    )?;
    d.command("gui-client reject", 0)?;
    d.command("desktop check", 0)?;
    if !d
        .output
        .text()
        .contains("[DESKTOP:BOUNDARIES] nonchild=true live_linux=true")
        || !d.output.text().contains("[DESKTOP:CONTROL_PASS]")
    {
        return Err(
            "Desktop direct-child and real task/descriptor pressure checks did not complete".into(),
        );
    }
    if field(
        last_line(&d.output.text(), "[DESKTOP:BOUNDARIES]")?,
        "task_pressure",
    )? != 24
    {
        return Err("Desktop pressure did not establish exactly seven steady session tasks".into());
    }
    d.command("echo M17-persisted-work > /persist/m17-work.txt", 0)?;
    d.command("sync", 0)?;

    let (x, y) = windows[0];
    d.move_to(x + 40, y + 8)?;
    d.button(true)?;
    d.move_to(x + 60, y + 38)?;
    d.button(false)?;
    d.screenshot(work, "moved", &[(x + 20, y + 30, false)], evidence)?;
    d.move_to(x + 60, y + 38)?;
    d.button(true)?;
    d.move_to(x + 40, y + 8)?;
    d.button(false)?;
    d.click(x + 30, y + 96)?;
    let from = d.output.position();
    d.key("q")?;
    d.wait_since(from, "[DESKTOP:EXIT] app=0")?;
    let from = d.output.position();
    d.click(35, 60)?;
    d.key("1")?;
    d.wait_since(from, "[GUI:C:READY]")?;
    let replacement = d.client_pid(0)?;
    let old = pid_field(&d.output.text(), "[DESKTOP:LAUNCH] app=0", false)?;
    if replacement <= old {
        return Err("Desktop close/relaunch reused a stale PID".into());
    }

    d.click(35, 60)?;
    let from = d.output.position();
    d.key("f")?;
    d.wait_since(from, "[GUI:RED_READY]")?;
    let red = pid_field(&d.output.text(), "[DESKTOP:PROBE] app=2 mode=red", true)?;
    d.click(35, 60)?;
    let from = d.output.position();
    d.key("b")?;
    d.wait_since(from, "[GUI:BLUE_READY]")?;
    if !d.output.text().contains("[GUI:BLUE_BOUNDARIES] owner=true") {
        return Err("Desktop foreign-handle rejection was not exercised".into());
    }
    let blue = pid_field(&d.output.text(), "[DESKTOP:PROBE] app=1 mode=blue", true)?;
    let (rx, ry) = d.window(red)?;
    let (bx, by) = d.window(blue)?;
    d.click(rx + 30, ry + 60)?;
    d.faults.push(red);
    let from = d.output.position();
    d.key("x")?;
    d.wait_since(
        from,
        &format!("[DESKTOP:EXIT] app=2 pid={red} reason=1 code=6"),
    )?;
    d.click(bx + 30, by + 60)?;
    let from = d.output.position();
    d.key("b")?;
    d.wait_since(from, "[GUI:KEY] client=blue code=48 pressed=true")?;
    let server = pid_field(&d.output.text(), "[GUI:SERVER_READY]", true)?;
    let first_session = field(
        last_line(&d.output.text(), "[GUI:SERVER_READY]")?,
        "session",
    )?;
    d.faults.push(server);
    let from = d.output.position();
    d.key("f12")?;
    d.ready(from, 1)?;
    let next_session = field(
        last_line(&d.output.text(), "[GUI:SERVER_READY]")?,
        "session",
    )?;
    if next_session <= first_session {
        return Err("Desktop compositor restart did not establish a fresh session".into());
    }
    let pid = d.client_pid(0)?;
    let (x, y) = d.window(pid)?;
    d.click(x + 30, y + 96)?;
    let from = d.output.position();
    d.key("a")?;
    d.wait_since(from, "[GUI:C:KEY] codepoint=97")?;
    d.screenshot(work, "reconnected", &[(x, y, true)], evidence)?;
    d.command("cat /persist/m17-work.txt", 0)?;
    d.click(35, 60)?;
    for generation in 2..=3 {
        let from = d.output.position();
        d.key("r")?;
        d.ready(from, generation)?;
        d.click(35, 60)?;
    }
    let from = d.output.position();
    d.key("r")?;
    d.wait_since(from, "[DESKTOP:STOPPED] error=-24 serial_recovery=true")?;
    d.wait("[INIT] desktop exited; COM1 shell remains available")?;
    let text = d.output.text();
    if text.matches("[DESKTOP:RESTART]").count() != 3
        || text.matches("[DESKTOP:CLEANUP]").count() != 4
    {
        return Err("Desktop did not enforce its shared three-replacement limit".into());
    }
    for line in text.lines().filter(|l| {
        l.starts_with("[DESKTOP:LAUNCH]")
            || l.starts_with("[DESKTOP:PROBE]")
            || l.starts_with("[GUI:SERVER_READY]")
    }) {
        let pid = field(line, "pid")?;
        if !text.lines().any(|l| {
            (l.starts_with("[DESKTOP:EXIT]") || l.starts_with("[DESKTOP:REAP]"))
                && field(l, "pid").ok() == Some(pid)
        }) && pid != server
        {
            return Err(format!("Desktop owned child {pid} was not observed and reaped").into());
        }
    }
    if text.matches("[USER:FAULT]").count() != 2 {
        return Err("Desktop did not execute exactly the two declared faults".into());
    }
    if !text.contains(&format!("[DESKTOP:REAP] pid={server} reason=1 code=6"))
        && !text.contains("[DESKTOP:COMPOSITOR_EXIT] reason=1 code=6")
    {
        return Err("Desktop did not reap the compositor's actual fault status".into());
    }
    evidence.push_str("three_languages=true\nphysical_input=true\nmove=true\nclose_relaunch=true\nquota_reuse=true\nforeign_stale_handles=true\nclient_fault=6\ncompositor_fault=6\nrestart_limit=3\nall_owned_children_reaped=true\n");
    Ok(())
}

struct Driver {
    guest: Guest,
    output: transcript::Transcript,
    uart: ChildStdin,
    qmp: Qmp,
    start: Instant,
    cursor: usize,
    pointer: (u32, u32),
    buttons: u8,
    faults: Vec<u64>,
}

impl Driver {
    fn poll(&mut self) -> Result<()> {
        self.output.poll()?;
        check_guest(
            &self.output.text(),
            self.start.elapsed() >= DEADLINE,
            &self.faults,
        )?;
        if let Some(status) = self.guest.0.try_wait()? {
            return Err(format!("Desktop exited before scenario completion: {status}").into());
        }
        Ok(())
    }
    fn wait_since(&mut self, from: usize, marker: &str) -> Result<()> {
        loop {
            self.poll()?;
            if let Some(at) = self.output.find_from(from, marker) {
                self.cursor = at + marker.len();
                return Ok(());
            }
            if marker.contains(":READY]")
                && self.output.find_from(from, "[DESKTOP:STOPPED]").is_some()
            {
                return Err(format!("Desktop stopped while waiting for {marker}").into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn wait(&mut self, marker: &str) -> Result<()> {
        self.wait_since(self.cursor, marker)
    }
    fn ready(&mut self, from: usize, generation: usize) -> Result<()> {
        self.wait_since(from, &format!("[DESKTOP:READY] generation={generation}"))?;
        for language in ["C", "CPP", "RUST"] {
            self.wait_since(from, &format!("[GUI:{language}:READY]"))?;
            if self
                .output
                .find_from(from, &format!("[GUI:{language}:CONTRACT] PASS"))
                .is_none()
            {
                return Err("A desktop language contract did not pass".into());
            }
        }
        Ok(())
    }
    fn serial(&mut self, bytes: &[u8]) -> Result<()> {
        self.uart.write_all(bytes)?;
        self.uart.flush()?;
        Ok(())
    }
    fn command(&mut self, command: &str, code: u32) -> Result<()> {
        let from = self.output.position();
        thread::sleep(Duration::from_millis(30));
        self.serial(format!("{command}\r").as_bytes())?;
        self.wait_since(from, "anova> ")?;
        if command == "exit" {
            return Ok(());
        }
        let from = self.output.position();
        self.serial(b"status\r")?;
        loop {
            self.poll()?;
            if let Some(actual) = self.output.shell_status_from(from) {
                if actual != code {
                    return Err(format!(
                        "Desktop serial command {command} returned {actual}, expected {code}"
                    )
                    .into());
                }
                self.wait_since(from, "anova> ")?;
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn key(&mut self, key: &str) -> Result<()> {
        for down in [true, false] {
            self.qmp.command("input-send-event",json!({"events":[{"type":"key","data":{"down":down,"key":{"type":"qcode","data":key}}}]}))?;
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
            let from = self.output.position();
            self.qmp.command("input-send-event",json!({"events":[{"type":"rel","data":{"axis":"x","value":dx}},{"type":"rel","data":{"axis":"y","value":dy}}]}))?;
            self.wait_since(
                from,
                &format!(
                    "[GUI:POINTER] x={} y={} buttons={}",
                    target.0, target.1, self.buttons
                ),
            )?;
            self.pointer = target;
        }
        Ok(())
    }
    fn button(&mut self, down: bool) -> Result<()> {
        let from = self.output.position();
        self.qmp.command(
            "input-send-event",
            json!({"events":[{"type":"btn","data":{"down":down,"button":"left"}}]}),
        )?;
        self.buttons = u8::from(down);
        self.wait_since(
            from,
            &format!(
                "[GUI:POINTER] x={} y={} buttons={}",
                self.pointer.0, self.pointer.1, self.buttons
            ),
        )
    }
    fn click(&mut self, x: u32, y: u32) -> Result<()> {
        self.move_to(x, y)?;
        self.button(true)?;
        self.button(false)
    }
    fn client_pid(&self, index: usize) -> Result<u64> {
        pid_field(
            &self.output.text(),
            &format!("[DESKTOP:LAUNCH] app={index}"),
            true,
        )
    }
    fn window(&self, pid: u64) -> Result<(u32, u32)> {
        let text = self.output.text();
        let line = last_line(&text, &format!("[DESKTOP:WINDOW] client={pid} "))?;
        let x = field(line, "x")?;
        let y = field(line, "y")?;
        if ![15, 270, 525].contains(&x)
            || y != 250
            || field(line, "width")? != 240
            || field(line, "height")? != 160
        {
            return Err("Desktop placement violated the declared display profile".into());
        }
        Ok((x as u32, y as u32))
    }
    fn screenshot(
        &mut self,
        work: &Path,
        phase: &str,
        windows: &[(u32, u32, bool)],
        evidence: &mut String,
    ) -> Result<()> {
        let path = work.join(format!("{phase}.ppm"));
        output_path(&path)?;
        if path.exists() {
            fs::remove_file(&path)?;
        }
        let began = Instant::now();
        let bytes = loop {
            self.poll()?;
            self.qmp
                .command("screendump", json!({"filename":path,"format":"ppm"}))?;
            let bytes = read_ppm(&path)?;
            if scanout(&bytes, windows).is_ok() {
                break bytes;
            }
            if began.elapsed() >= INTERACTION {
                scanout(&bytes, windows)?;
            }
            thread::sleep(Duration::from_millis(20));
        };
        evidence.push_str(&format!(
            "{phase}_scanout_sha256={:x}\n",
            Sha256::digest(bytes)
        ));
        Ok(())
    }
    fn shutdown(&mut self) -> Result<()> {
        self.serial(b"shutdown\r")?;
        loop {
            self.output.poll()?;
            check_guest(
                &self.output.text(),
                self.start.elapsed() >= DEADLINE,
                &self.faults,
            )?;
            if let Some(status) = self.guest.0.try_wait()? {
                self.output.finish()?;
                if !status.success()
                    || !self
                        .output
                        .text()
                        .contains("[POWER:OFF] synced=true clean=true dma_reset=true")
                {
                    return Err("Desktop did not shut down cleanly".into());
                }
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn last_line<'a>(text: &'a str, marker: &str) -> Result<&'a str> {
    text.lines()
        .rev()
        .find(|line| line.contains(marker))
        .ok_or_else(|| format!("Missing desktop record {marker}").into())
}
fn field(line: &str, name: &str) -> Result<u64> {
    let prefix = format!("{name}=");
    let mut values = line
        .split_whitespace()
        .filter_map(|s| s.strip_prefix(&prefix));
    let value = values
        .next()
        .ok_or("Missing desktop record field")?
        .parse()?;
    if values.next().is_some() {
        return Err("Duplicate desktop record field".into());
    }
    Ok(value)
}
fn pid_field(text: &str, marker: &str, last: bool) -> Result<u64> {
    let line = if last {
        last_line(text, marker)?
    } else {
        text.lines()
            .find(|l| l.contains(marker))
            .ok_or("Missing initial desktop child")?
    };
    let pid = field(line, "pid")?;
    if pid == 0 {
        return Err("Zero desktop PID".into());
    }
    Ok(pid)
}

fn check_guest(text: &str, timed_out: bool, allowed: &[u64]) -> Result<()> {
    if timed_out {
        return Err("Desktop exceeded its 180-second watchdog".into());
    }
    for marker in [
        "[PANIC]",
        "[EXCEPTION]",
        "[INIT:FAIL]",
        "[SHELL:FAIL]",
        "[POWER:FAIL]",
        "[DESKTOP:FAIL]",
        "[GUI:FAIL]",
        "[USER:RETURNREJECT]",
    ] {
        if text.contains(marker) {
            return Err(format!("Desktop fatal record {marker}").into());
        }
    }
    let mut seen = Vec::new();
    for line in text
        .split_inclusive('\n')
        .filter(|l| l.ends_with('\n') && l.starts_with("[USER:FAULT]"))
    {
        let pid = field(line, "pid")?;
        if !allowed.contains(&pid) || field(line, "vector")? != 6 || seen.contains(&pid) {
            return Err("Unexpected or repeated desktop user fault".into());
        }
        seen.push(pid);
    }
    Ok(())
}

fn validate_resources(text: &str) -> Result<()> {
    let line = last_line(text, "[M15:RESOURCES]")?;
    for (name, limit) in [
        ("heap_peak_bytes", 12 * 1024 * 1024),
        ("peak_frame_delta", 16384),
        ("live_tasks_peak", 32),
        ("occupied_slots_peak", 32),
        ("fd_per_process_peak", 32),
        ("vfs_node_slots_peak", 256),
    ] {
        if field(line, name)? > limit {
            return Err(format!("Desktop exceeded resource budget {name}={limit}").into());
        }
    }
    if field(line, "samples")? == 0 {
        return Err("Desktop resource summary has no samples".into());
    }
    Ok(())
}

fn scanout(bytes: &[u8], windows: &[(u32, u32, bool)]) -> Result<()> {
    let image = parse_ppm(bytes)?;
    if image.width != 800 || image.height != 600 {
        return Err("Desktop requires an actual 800x600 scanout".into());
    }
    for &(x, y, typed) in windows {
        let mut samples = vec![
            (x + 12, y + 44, 0x3070b0),
            (x + 227, y + 55, 0x3070b0),
            (x + 228, y + 55, 0x182838),
            (x + 12, y + 84, 0x788898),
        ];
        if typed {
            // Literal independent A glyph and field colors; no runtime rasterizer is reused.
            for (row, bits) in [14u8, 17, 17, 31, 17, 17, 17].into_iter().enumerate() {
                for col in 0..5 {
                    samples.push((
                        x + 16 + col,
                        y + 89 + row as u32,
                        if bits & (1 << (4 - col)) != 0 {
                            0x182838
                        } else {
                            0xffffff
                        },
                    ));
                }
            }
        }
        for (sx, sy, color) in samples {
            if sx >= 800 || sy >= 600 {
                return Err("Desktop sample is outside scanout".into());
            }
            let at = ((sy * 800 + sx) * 3) as usize;
            if image.rgb[at..at + 3] != [(color >> 16) as u8, (color >> 8) as u8, color as u8] {
                return Err(format!("Desktop scanout differs at ({sx},{sy})").into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unexpected_faults_duplicates_and_timeouts_fail() {
        let fault = "[USER:FAULT] pid=42 vector=6\n";
        assert!(check_guest(fault, false, &[42]).is_ok());
        for (text, timed) in [
            (fault.to_owned(), false),
            ("[PANIC]".into(), false),
            ("".into(), true),
        ] {
            assert!(check_guest(&text, timed, &[]).is_err());
        }
        assert!(check_guest(&fault.repeat(2), false, &[42]).is_err());
        assert!(check_guest(&fault.replace("vector=6", "vector=14"), false, &[42]).is_err());
    }
    #[test]
    fn resource_evidence_must_be_complete_unambiguous_and_within_budget() {
        let sample = "[M15:RESOURCES] samples=1 heap_peak_bytes=1 peak_frame_delta=1 live_tasks_peak=7 occupied_slots_peak=7 fd_per_process_peak=3 vfs_node_slots_peak=40\n";
        assert!(validate_resources(sample).is_ok());
        for text in [
            sample.replace("samples=1", "samples=0"),
            sample.replace("heap_peak_bytes=1", "heap_peak_bytes=12582913"),
            sample.replace("live_tasks_peak=7", "live_tasks_peak=33"),
            sample.replace("samples=1", "samples=1 samples=2"),
        ] {
            assert!(validate_resources(&text).is_err());
        }
        assert!(validate_resources("").is_err());
    }
}
