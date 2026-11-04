// SPDX-License-Identifier: Apache-2.0
//! Verify launcher GCC diagnostics and separately admitted job-fault fixtures with real QEMU.
//! Preserve per-run UART/QMP/scanout/disk evidence, fixed deadlines and actual failure results.

use super::*;
use desktop_test::Driver;
use graphics::{parse_ppm, read_ppm};
use pty_terminal_test::{type_command, type_text};
use serde_json::json;
use sha2::{Digest, Sha256};

const BAD: &str = "int main(void) { return ???; }\n";
const GOOD: &str = "#include <stdio.h>\nint main(void) { puts(\"M17-BUILD-OK\"); return 7; }\n";
const INTERACTION: Duration = Duration::from_secs(5);
const BUILD: Duration = Duration::from_secs(35);

fn build_deadline(elapsed: Duration) -> Result<()> {
    if elapsed > BUILD {
        return Err(format!(
            "Controlled build exceeded 35000 ms (observed {} ms)",
            elapsed.as_millis()
        )
        .into());
    }
    Ok(())
}

fn glyph(byte: u8) -> [u8; 7] {
    // Independent literal V extends the PTY gate's deliberately smaller alphabet.
    if byte == b'V' {
        [17, 17, 17, 17, 17, 10, 4]
    } else {
        pty_terminal_test::glyph(byte)
    }
}

pub fn verify(root: &Path) -> Result<()> {
    let base = directory(root, "desktop-build-test")?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let work = base.join(stamp.to_string());
    fs::create_dir(&work)?;
    fs::write(base.join("latest.txt"), format!("{}\n", work.display()))?;
    let mut evidence = String::from(
        "profile=M17.4-batch1-q35-tcg-1BSP-3GiB-800x600\nrecords=4\nlog_bytes_per_record=8192\nread_bytes_per_turn=4096\nevents_per_turn=16\ninteraction_budget_ms=5000\nbuild_budget_ms=35000\ncompiler_observer_ms=500\n",
    );
    fs::write(work.join("result.txt"), "result=Running\n")?;
    let snapshot = implementation(root)?;
    fs::write(
        work.join("implementation.json"),
        serde_json::to_vec_pretty(&snapshot)?,
    )?;
    evidence.push_str(&format!(
        "rustc={}\nqemu={}\n",
        version("rustc", &["--version"])?.trim(),
        version("qemu-system-x86_64", &["--version"])?.trim()
    ));
    let outcome = run(root, &work, &mut evidence).and_then(|()| {
        if implementation(root)? != snapshot {
            return Err("Build gate sources changed during acceptance".into());
        }
        Ok(())
    });
    evidence.push_str(&match &outcome {
        Ok(()) => "result=Ok\n".into(),
        Err(e) => format!("result=Err\nerror={e}\n"),
    });
    fs::write(work.join("result.txt"), evidence)?;
    outcome
}

fn run(root: &Path, work: &Path, evidence: &mut String) -> Result<()> {
    let iso = pty_terminal::image(root, true)?;
    evidence.push_str(&format!(
        "compiler_iso_sha256={:x}\n",
        Sha256::digest(fs::read(&iso)?)
    ));
    let disk = directory(root, "disks")?.join(format!(
        "desktop-build-{}.ext4",
        work.file_name().unwrap().to_string_lossy()
    ));
    foundation_journal::create(root, &disk, true)?;
    let (mut d, vars) = Driver::boot(root, &iso, &disk, work, "compiler")?;
    d.wait("anova> ")?;
    d.ready(0, 0)?;
    d.click(35, 60)?;
    d.key("f")?;
    d.wait("[FILES:READY]")?;
    d.click(290, 320)?;
    desktop_files_test::prompt(&mut d, "f2", "/persist/build.c")?;
    d.wait("[FILES:NEW]")?;
    type_text(&mut d, BAD)?;
    save(&mut d)?;
    desktop_files_test::serial_exact(&mut d, "/persist/build.c", BAD)?;
    diagnostics(&mut d)?;
    let first = build(&mut d, work, 1, false, evidence)?;
    expect(&first, 0, 1, false)?;
    capture(
        &mut d,
        work,
        "syntax-error",
        &[(76, "ATTEMPT 1"), (84, "CODE=1")],
        evidence,
    )?;
    // The real diagnostic is retained in the bounded log, not inferred from status.
    capture(
        &mut d,
        work,
        "compiler-diagnostic",
        &[(0, "ERROR")],
        evidence,
    )?;
    d.key("esc")?;
    d.click(290, 320)?;
    for _ in 0..BAD.len() {
        d.key("backspace")?;
    }
    type_text(&mut d, GOOD)?;
    save(&mut d)?;
    desktop_files_test::serial_exact(&mut d, "/persist/build.c", GOOD)?;
    diagnostics(&mut d)?;
    let second = build(&mut d, work, 2, false, evidence)?;
    expect(&second, 0, 0, true)?;
    if field(&second, "id")? <= field(&first, "id")? {
        return Err("Compiler task identity was reused".into());
    }
    capture(
        &mut d,
        work,
        "successful-build",
        &[(76, "ATTEMPT 2"), (84, "CODE=0"), (92, "PUBLISHED")],
        evidence,
    )?;
    let prior = artifact(&mut d, &disk, work, "successful")?;
    anova_elf::parse_linux(&prior).map_err(|e| format!("Actual compiler artifact: {e:?}"))?;
    d.key("bracket_left")?;
    capture(
        &mut d,
        work,
        "prior-failure",
        &[(76, "ATTEMPT 1"), (84, "CODE=1")],
        evidence,
    )?;
    d.key("bracket_right")?;
    d.key("esc")?;
    launch_terminal(&mut d)?;
    type_command(&mut d, "/persist/build-app; /linux/echo RUN-$?")?;
    pty_terminal_test::screenshot(
        &mut d,
        work,
        "actual-artifact-run",
        &["M17-BUILD-OK", "RUN-7"],
        evidence,
    )?;
    let from = d.output.position();
    d.key("f4")?;
    d.wait_since(from, "[DESKTOP:EXIT] app=2")?;
    diagnostics(&mut d)?;
    capture(
        &mut d,
        work,
        "after-terminal-close",
        &[(76, "ATTEMPT 2"), (84, "CODE=0")],
        evidence,
    )?;
    d.key("esc")?;
    d.click(290, 320)?;
    type_text(&mut d, "broken syntax;\n")?;
    save(&mut d)?;
    diagnostics(&mut d)?;
    let third = build(&mut d, work, 3, false, evidence)?;
    expect(&third, 0, 1, false)?;
    let retained = artifact(&mut d, &disk, work, "after-failure")?;
    if prior != retained {
        return Err("Failed build replaced the prior usable artifact".into());
    }
    evidence.push_str(&format!(
        "retained_artifact_sha256={:x}\n",
        Sha256::digest(&prior)
    ));
    // A dirty Files buffer must still block ordinary launcher restart/exit.
    d.key("esc")?;
    d.click(290, 320)?;
    type_text(&mut d, "// PENDING\n")?;
    d.click(35, 60)?;
    let from = d.output.position();
    d.key("r")?;
    d.key("q")?;
    d.command("echo BUILD-SERIAL-RECOVERY", 0)?;
    if d.output.text()[from..].contains("[DESKTOP:RESTART]")
        || d.output.text()[from..].contains("[DESKTOP:CLEANUP]")
    {
        return Err("Launcher bypassed Files unsaved protection".into());
    }
    d.click(290, 320)?;
    d.key("f4")?;
    d.wait("[FILES:UNSAVED]")?;
    let from = d.output.position();
    d.key("d")?;
    d.wait_since(from, "[DESKTOP:EXIT] app=1")?;
    d.click(35, 60)?;
    let from = d.output.position();
    d.key("r")?;
    d.ready(from, 1)?;
    diagnostics(&mut d)?;
    capture(
        &mut d,
        work,
        "after-session-replacement",
        &[(76, "ATTEMPT 3"), (84, "CODE=1")],
        evidence,
    )?;
    d.key("esc")?;
    d.key("q")?;
    d.wait("[INIT] desktop exited; COM1 shell remains available")?;
    d.command("sync", 0)?;
    d.shutdown()?;
    resources(&d.output.text(), evidence)?;
    vars.cleanup()?;
    foundation_journal::check(&disk)?;
    let (mut d, vars) = Driver::boot(root, &iso, &disk, work, "reboot")?;
    d.wait("anova> ")?;
    d.ready(0, 0)?;
    diagnostics(&mut d)?;
    capture(
        &mut d,
        work,
        "reboot-empty-records",
        &[(76, "ATTEMPT 0")],
        evidence,
    )?;
    d.key("esc")?;
    launch_terminal(&mut d)?;
    type_command(&mut d, "/persist/build-app; /linux/echo RUN-$?")?;
    pty_terminal_test::screenshot(
        &mut d,
        work,
        "reboot-artifact",
        &["M17-BUILD-OK", "RUN-7"],
        evidence,
    )?;
    d.key("f4")?;
    d.click(35, 60)?;
    d.key("q")?;
    d.wait("[INIT] desktop exited; COM1 shell remains available")?;
    d.shutdown()?;
    resources(&d.output.text(), evidence)?;
    vars.cleanup()?;
    foundation_journal::check(&disk)?;
    fixtures(root, work, evidence)?;
    Ok(())
}

fn save(d: &mut Driver) -> Result<()> {
    let from = d.output.position();
    d.key("f5")?;
    d.wait_since(from, "[FILES:SAVED]")
}
fn diagnostics(d: &mut Driver) -> Result<()> {
    d.click(35, 60)?;
    d.key("b")
}
fn launch_terminal(d: &mut Driver) -> Result<()> {
    d.click(35, 60)?;
    let from = d.output.position();
    d.key("t")?;
    d.wait_since(from, "[TERMINAL:READY]")?;
    d.wait_since(from, "[TERMINAL:PRESENT]")?;
    d.click(470, 90)
}
fn field(line: &str, name: &str) -> Result<u64> {
    let prefix = format!("{name}=");
    let mut values = line
        .split_whitespace()
        .filter_map(|s| s.strip_prefix(&prefix));
    let n = values.next().ok_or("Missing build record field")?.parse()?;
    if values.next().is_some() {
        return Err("Duplicate build record field".into());
    }
    Ok(n)
}
fn expect(line: &str, reason: u64, code: u64, published: bool) -> Result<()> {
    if field(line, "reason")? != reason
        || field(line, "code")? != code
        || !line
            .split_whitespace()
            .any(|s| s == format!("published={published}"))
        || field(line, "error")? != 0
    {
        return Err(format!("Unexpected real build result: {line}").into());
    }
    Ok(())
}
fn build(
    d: &mut Driver,
    work: &Path,
    attempt: u64,
    fault: bool,
    evidence: &mut String,
) -> Result<String> {
    let from = d.output.position();
    d.key("g")?;
    let began = Instant::now();
    loop {
        // Register the exact deliberate UD2 victim before the ordinary guest-fault audit.
        d.output.poll()?;
        let text = d.output.text().to_string();
        if fault
            && let Some(line) = text[from..]
                .split_inclusive('\n')
                .find(|l| l.ends_with('\n') && l.contains("[BUILD:START]"))
        {
            let id = field(line, "id")?;
            if !d.faults.contains(&id) {
                d.faults.push(id);
            }
        }
        d.poll()?;
        build_deadline(began.elapsed())?;
        if let Some(line) = text[from..]
            .split_inclusive('\n')
            .find(|l| l.ends_with('\n') && l.contains("[BUILD:END]"))
        {
            if field(line, "attempt")? != attempt {
                return Err("Wrong compiler attempt completed".into());
            }
            evidence.push_str(&format!(
                "build_elapsed_ms={} {line}",
                began.elapsed().as_millis()
            ));
            let attempt_text = format!("ATTEMPT {attempt}");
            let code_text = format!("CODE={}", field(line, "code")?);
            capture(
                d,
                work,
                &format!("attempt-{attempt}-task-{}", field(line, "id")?),
                &[(76, &attempt_text), (84, &code_text)],
                evidence,
            )?;
            return Ok(line.into());
        }
        thread::sleep(Duration::from_millis(500));
    }
}
fn artifact(d: &mut Driver, disk: &Path, work: &Path, name: &str) -> Result<Vec<u8>> {
    d.command("sync", 0)?;
    let snapshot = work.join(format!("{name}.ext4"));
    d.qmp.command("stop", json!({}))?;
    let copied = fs::copy(disk, &snapshot);
    d.qmp.command("cont", json!({}))?;
    copied?;
    let path = work.join(format!("{name}.elf"));
    command(
        Command::new("debugfs")
            .args(["-R", &format!("dump /build-app {}", path.display())])
            .arg(snapshot),
    )?;
    Ok(fs::read(path)?)
}

fn capture(
    d: &mut Driver,
    work: &Path,
    name: &str,
    expected: &[(usize, &str)],
    evidence: &mut String,
) -> Result<()> {
    d.move_to(790, 580)?;
    let path = work.join(format!("{name}.ppm"));
    let began = Instant::now();
    loop {
        d.poll()?;
        d.qmp
            .command("screendump", json!({"filename":path,"format":"ppm"}))?;
        let bytes = read_ppm(&path)?;
        let image = parse_ppm(&bytes)?;
        if image.width != 800 || image.height != 600 {
            return Err("Build scanout geometry differs".into());
        }
        let matches = expected.iter().all(|(y, text)| {
            let ys: Vec<_> = if *y == 0 {
                (0..12).map(|row| 124 + row * 8).collect()
            } else {
                vec![*y]
            };
            ys.iter().any(|&y| {
                (0..=50usize.saturating_sub(text.len())).any(|col| {
                    text.bytes().enumerate().all(|(index, b)| {
                        glyph(b).iter().enumerate().all(|(dy, bits)| {
                            (0..5).all(|dx| {
                                let at = ((y + dy) * 800 + 24 + (col + index) * 6 + dx) * 3;
                                let color = if bits & (16 >> dx) != 0 {
                                    if y == 116 {
                                        [0xff, 0xcc, 0x66]
                                    } else {
                                        [255, 255, 255]
                                    }
                                } else {
                                    [0x18, 0x28, 0x38]
                                };
                                image.rgb.get(at..at + 3) == Some(&color[..])
                            })
                        })
                    })
                })
            })
        });
        if matches {
            evidence.push_str(&format!(
                "{name}_scanout_sha256={:x}\n",
                Sha256::digest(bytes)
            ));
            return Ok(());
        }
        if began.elapsed() > INTERACTION {
            return Err(format!("Build scanout missing {expected:?} in {name}").into());
        }
        thread::sleep(Duration::from_millis(30));
    }
}

fn fixtures(root: &Path, work: &Path, evidence: &mut String) -> Result<()> {
    // This ISO admits a deterministic Linux fault producer at /bin/gcc, separately
    // from the real compiler ISO. It cannot establish compiler-build acceptance.
    let archive = desktop::prepare(root)?;
    let fixture = work.join("build-fixture.elf");
    command(
        Command::new("musl-gcc")
            .args([
                "-static",
                "-no-pie",
                "-O2",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-Wl,--build-id=none",
            ])
            .arg(root.join("tests/guest/desktop/build-fixture.c"))
            .arg("-o")
            .arg(&fixture),
    )?;
    let fixture = fs::read(fixture)?;
    anova_elf::parse_linux(&fixture).map_err(|e| format!("Build fixture ELF: {e:?}"))?;
    let mut bytes = fs::read(archive)?;
    toolchain::append(&mut bytes, 5, "/bin/gcc", &fixture);
    let archive = work.join("fixture.anovafs");
    fs::write(&archive, bytes)?;
    let kernel = build_kernel(root)?;
    let iso = image_contents(root, &kernel, Some(&archive), "desktop-build-fixture")?;
    evidence.push_str(&format!(
        "fixture_iso_sha256={:x}\n",
        Sha256::digest(fs::read(&iso)?)
    ));
    let disk = directory(root, "disks")?.join(format!(
        "desktop-build-fixture-{}.ext4",
        work.file_name().unwrap().to_string_lossy()
    ));
    foundation_journal::create(root, &disk, true)?;
    let (mut d, vars) = Driver::boot(root, &iso, &disk, work, "fixture")?;
    d.wait("anova> ")?;
    d.ready(0, 0)?;
    d.command("desktop build-check", 0)?;
    reclaim(&d.output.text())?;
    pressure_count(&d.output.text(), 24)?;
    gui_slot_recovery(&mut d)?;
    diagnostics(&mut d)?;
    d.command("echo c > /persist/build.c", 0)?;
    let line = build(&mut d, work, 1, true, evidence)?;
    expect(&line, 3, 4, false)?;
    capture(
        &mut d,
        work,
        "abnormal-exit",
        &[(84, "REASON=3 CODE=4")],
        evidence,
    )?;
    d.command("echo d > /persist/build.c", 0)?;
    let line = build(&mut d, work, 2, false, evidence)?;
    expect(&line, 0, 31, false)?;
    if !line.contains("incomplete=true") {
        return Err("Detached writer capture cut was not reported".into());
    }
    d.command("echo l > /persist/build.c", 0)?;
    let from = d.output.position();
    d.key("g")?;
    d.wait_since(from, "[BUILD:START] attempt=3")?;
    let began = Instant::now();
    d.command("echo BUILD-COM1-LIVE", 0)?;
    if began.elapsed() > INTERACTION {
        return Err("Build output blocked COM1".into());
    }
    evidence.push_str(&format!("output_com1_ms={}\n", began.elapsed().as_millis()));
    let pid = d.client_pid(0)?;
    let (x, y) = d.window(pid)?;
    let began = Instant::now();
    d.click(x + 30, y + 96)?;
    d.key("a")?;
    d.wait_since(from, "[GUI:C:KEY] codepoint=97")?;
    peer_scanout(&mut d, work, x, y, evidence)?;
    if began.elapsed() > INTERACTION {
        return Err("Build output blocked peer GUI".into());
    }
    evidence.push_str(&format!(
        "output_peer_key_ms={}\n",
        began.elapsed().as_millis()
    ));
    d.click(35, 60)?;
    d.output.poll()?;
    if d.output.text()[from..].contains("[BUILD:END]") {
        return Err("Long-output task ended before active-session replacement".into());
    }
    d.key("esc")?;
    let restart = d.output.position();
    d.key("r")?;
    d.ready(restart, 1)?;
    diagnostics(&mut d)?;
    let began = Instant::now();
    let line = loop {
        d.poll()?;
        build_deadline(began.elapsed())?;
        let text = d.output.text();
        if let Some(l) = text[from..]
            .split_inclusive('\n')
            .find(|l| l.ends_with('\n') && l.contains("[BUILD:END]"))
        {
            break l.to_string();
        }
        thread::sleep(Duration::from_millis(30));
    };
    expect(&line, 0, 29, false)?;
    if field(&line, "bytes")? != 8192 || field(&line, "dropped")? == 0 {
        return Err("Build log did not reach its exact bound".into());
    }
    evidence.push_str(&line);
    capture(
        &mut d,
        work,
        "bounded-output",
        &[
            (76, "ATTEMPT 3"),
            (84, "CODE=29"),
            (116, "DROPPED"),
            (0, "OUTPUT"),
        ],
        evidence,
    )?;
    d.key("pgup")?;
    d.key("pgdn")?;
    for attempt in 4..=6 {
        d.command("echo x > /persist/build.c", 0)?;
        let line = build(&mut d, work, attempt, false, evidence)?;
        expect(&line, 0, 23, false)?;
    }
    capture(
        &mut d,
        work,
        "record-eviction",
        &[(76, "ATTEMPT 6"), (84, "CODE=23"), (116, "EVICTED 2")],
        evidence,
    )?;
    for _ in 0..4 {
        d.key("bracket_left")?;
    }
    capture(
        &mut d,
        work,
        "oldest-retained-record",
        &[(76, "ATTEMPT 3"), (84, "CODE=29")],
        evidence,
    )?;
    d.command("echo PRIOR-ARTIFACT > /persist/build-app", 0)?;
    d.command("echo w > /persist/build.c", 0)?;
    let began = Instant::now();
    let line = build(&mut d, work, 7, false, evidence)?;
    expect(&line, 0, 0, false)?;
    let turns = field(&line, "drain_turns")?;
    if !line.contains("incomplete=true")
        || !(1..=3).contains(&turns)
        || began.elapsed() > INTERACTION
    {
        return Err("Continuous detached writer escaped bounded post-exit cleanup".into());
    }
    capture(
        &mut d,
        work,
        "continuous-writer-cut",
        &[(76, "ATTEMPT 7"), (84, "CODE=0"), (92, "CAPTURE CUT")],
        evidence,
    )?;
    desktop_files_test::serial_exact(&mut d, "/persist/build-app", "PRIOR-ARTIFACT\n")?;
    d.command("echo x > /persist/build.c", 0)?;
    d.key("esc")?;
    d.key("q")?;
    d.wait("[INIT] desktop exited; COM1 shell remains available")?;
    d.command("desktop build-check", 0)?;
    reclaim(&d.output.text())?;
    pressure_count(&d.output.text(), 29)?;
    d.shutdown()?;
    resources(&d.output.text(), evidence)?;
    vars.cleanup()?;
    foundation_journal::check(&disk)?;
    // Ordinary shell-only desktop must report missing compiler as a failed attempt.
    let iso = desktop::image(root)?;
    let (mut d, vars) = Driver::boot(root, &iso, &disk, work, "missing-compiler")?;
    d.wait("anova> ")?;
    d.ready(0, 0)?;
    diagnostics(&mut d)?;
    let from = d.output.position();
    d.key("g")?;
    d.wait_since(from, "[BUILD:START_ERROR] attempt=1")?;
    capture(
        &mut d,
        work,
        "start-failure",
        &[(76, "STARTFAILED"), (84, "ERROR=-")],
        evidence,
    )?;
    d.command("echo BUILD-START-RECOVERED", 0)?;
    d.key("esc")?;
    d.key("q")?;
    d.wait("[INIT] desktop exited; COM1 shell remains available")?;
    d.shutdown()?;
    resources(&d.output.text(), evidence)?;
    vars.cleanup()?;
    Ok(())
}
fn build_kernel(root: &Path) -> Result<PathBuf> {
    super::build(root, Mode::Desktop)
}

fn gui_slot_recovery(d: &mut Driver) -> Result<()> {
    d.command("desktop connect-full", 0)?;
    d.click(35, 60)?;
    let from = d.output.position();
    d.serial(b"desktop connect-release\r")?;
    d.wait_since(from, "[GUI:CONNECT_WAIT] quota_busy=true")?;
    d.key("1")?;
    d.wait_since(from, "[DESKTOP:CONNECT_RELEASE_PASS]")?;
    d.wait_since(from, "anova> ")?;
    let pid = {
        let text = d.output.text();
        let line = text[from..]
            .lines()
            .find(|l| l.contains("[DESKTOP:CONNECT_RELEASE_PASS]"))
            .ok_or("Missing GUI slot recovery record")?;
        field(line, "pid")?
    };
    d.wait_since(from, &format!("[GUI:CLIENT_GONE] client={pid}"))?;
    let from = d.output.position();
    d.key("1")?;
    d.wait_since(from, "[GUI:C:READY]")?;
    d.command("echo GUI-SLOTS-RECOVERED", 0)
}

fn peer_scanout(d: &mut Driver, work: &Path, x: u32, y: u32, evidence: &mut String) -> Result<()> {
    let path = work.join("output-peer.ppm");
    let began = Instant::now();
    loop {
        d.poll()?;
        d.qmp
            .command("screendump", json!({"filename":path,"format":"ppm"}))?;
        let bytes = read_ppm(&path)?;
        let image = parse_ppm(&bytes)?;
        let matches = glyph(b'A').iter().enumerate().all(|(row, bits)| {
            (0..5).all(|col| {
                let at = ((y as usize + 89 + row) * 800 + x as usize + 16 + col) * 3;
                let expected = if bits & (16 >> col) != 0 {
                    [0x18, 0x28, 0x38]
                } else {
                    [255, 255, 255]
                };
                image.rgb.get(at..at + 3) == Some(&expected[..])
            })
        });
        if matches {
            evidence.push_str(&format!(
                "output_peer_scanout_sha256={:x}\n",
                Sha256::digest(bytes)
            ));
            return Ok(());
        }
        if began.elapsed() > INTERACTION {
            return Err("Build blocked actual peer scanout".into());
        }
        thread::sleep(Duration::from_millis(30));
    }
}

fn implementation(root: &Path) -> Result<std::collections::BTreeMap<String, String>> {
    [
        "crates/abi/src/build.rs",
        "crates/abi/src/lib.rs",
        "crates/terminal/src/build_log.rs",
        "crates/terminal/Cargo.toml",
        "Cargo.lock",
        "user/Cargo.lock",
        "user/desktop/Cargo.toml",
        "user/desktop/src/main.rs",
        "user/desktop/src/diagnostics.rs",
        "user/desktop/src/build_checks.rs",
        "user/rt/src/build.rs",
        "user/rt/src/lib.rs",
        "user/terminal/src/window.rs",
        "user/gui-common/src/lib.rs",
        "kernel/src/drivers/virtio.rs",
        "kernel/src/fs/tty.rs",
        "kernel/src/fs/tty_ready.rs",
        "kernel/src/task/mod.rs",
        "kernel/src/task/pty.rs",
        "crates/sched/src/lib.rs",
        "xtask/src/desktop_build_test.rs",
        "xtask/src/desktop_files_test.rs",
        "xtask/src/graphics.rs",
        "xtask/src/main.rs",
        "xtask/src/desktop_test.rs",
        "xtask/src/pty_terminal_test.rs",
        "xtask/src/pty_terminal.rs",
        "tests/guest/desktop/build-fixture.c",
    ]
    .into_iter()
    .map(|p| {
        Ok((
            p.to_string(),
            format!("{:x}", Sha256::digest(fs::read(root.join(p))?)),
        ))
    })
    .collect()
}
fn resources(text: &str, evidence: &mut String) -> Result<()> {
    let line = text
        .lines()
        .rev()
        .find(|l| l.contains("[M15:RESOURCES]"))
        .ok_or("Missing build resource samples")?;
    for (name, limit) in [
        ("heap_peak_bytes", 12 * 1024 * 1024),
        ("peak_frame_delta", 131072),
        ("live_tasks_peak", 32),
        ("occupied_slots_peak", 32),
        ("fd_per_process_peak", 32),
        ("vfs_node_slots_peak", 2048),
    ] {
        if field(line, name)? > limit {
            return Err(format!("Build exceeded declared resource budget {name}").into());
        }
    }
    if field(line, "samples")? == 0 {
        return Err("Build resource samples are empty".into());
    }
    evidence.push_str(line);
    evidence.push('\n');
    Ok(())
}

fn reclaim(text: &str) -> Result<()> {
    let summary = text
        .lines()
        .rev()
        .find(|l| l.contains("[BUILD:CHECK_PASS]"))
        .ok_or("Missing job boundary summary")?;
    let owner = field(summary, "owner")?;
    let records: Vec<_> = text
        .lines()
        .rev()
        .filter(|l| l.contains("[PTY:RECLAIM]") && field(l, "owner").ok() == Some(owner))
        .take(4)
        .collect();
    if records.len() != 4 || records.iter().any(|l| !l.contains("exact=true")) {
        return Err("Isolated jobs failed exact heap/frame/open/pipe reclamation".into());
    }
    Ok(())
}
fn pressure_count(text: &str, expected: u64) -> Result<()> {
    let line = text
        .lines()
        .rev()
        .find(|l| l.contains("[BUILD:CHECK_PASS]"))
        .ok_or("Missing build pressure count")?;
    if field(line, "task_pressure")? != expected {
        return Err("Build family left occupied scheduler slots".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_observer_rejects_wrong_and_duplicate_fields() {
        assert!(expect("reason=0 code=1 published=false error=0", 0, 1, false).is_ok());
        assert!(expect("reason=0 code=0 published=false error=0", 0, 1, false).is_err());
        assert!(field("id=1 id=2", "id").is_err());
        assert!(field("pid=3", "id").is_err());
    }
    #[test]
    fn completed_results_cannot_bypass_the_declared_deadline() {
        assert!(build_deadline(BUILD).is_ok());
        assert!(build_deadline(BUILD - Duration::from_nanos(1)).is_ok());
        assert!(build_deadline(BUILD + Duration::from_nanos(1)).is_err());
        assert!(build_deadline(Duration::from_millis(35157)).is_err());
    }
}
