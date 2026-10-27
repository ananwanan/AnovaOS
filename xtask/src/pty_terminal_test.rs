// SPDX-License-Identifier: Apache-2.0
//! Drive real physical GUI input through the PTY, validate actual scanout and retain reboot evidence.
//! Independent UART/QMP channels and fixed budgets distinguish UI output from diagnostic logs.

use super::*;
use desktop_test::Driver;
use graphics::{parse_ppm, read_ppm};
use serde_json::json;
use sha2::{Digest, Sha256};

const INTERACTION: Duration = Duration::from_secs(5);

pub fn verify(root: &Path, compiler: bool) -> Result<()> {
    let work = directory(root, "desktop-pty-test")?;
    let result = work.join("result.txt");
    output_path(&result)?;
    fs::write(&result, "result=Running\n")?;
    let mut evidence = format!(
        "profile=M17.2-q35-tcg-1BSP-3GiB-800x600\ncompiler={compiler}\ninteraction_budget_ms=5000\nheap_budget_bytes=12582912\nframe_budget=131072\ntask_budget=32\nfd_budget=32\nvfs_budget=2048\nscrollback_lines=256\n"
    );
    let outcome = (|| -> Result<()> {
        let iso = pty_terminal::image(root, compiler)?;
        evidence.push_str(&format!(
            "iso_sha256={:x}\n",
            Sha256::digest(fs::read(&iso)?)
        ));
        evidence.push_str(&fs::read_to_string(root.join("build/terminal/inputs.txt"))?);
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let disk = directory(root, "disks")?.join(format!("desktop-pty-{stamp}.ext4"));
        foundation_journal::create(root, &disk, true)?;
        evidence.push_str(&format!("disk={}\n", disk.display()));
        for reboot in [false, true] {
            let name = if reboot { "reboot" } else { "graphical" };
            let (mut d, vars) = Driver::boot(root, &iso, &disk, &work, name)?;
            d.wait("anova> ")?;
            d.ready(0, 0)?;
            if !reboot {
                d.command("terminal check", 0)?;
                reclaim(&d.output.text())?;
            }
            launch(&mut d)?;
            if reboot {
                type_command(&mut d, "/linux/cat /persist/pty-work.txt")?;
                screenshot(&mut d, &work, "reopened-work", &["EDIT-Y"], &mut evidence)?;
                if compiler {
                    compile_run(&mut d, &work, true, &mut evidence)?;
                }
            } else {
                scenario(&mut d, &work, compiler, &mut evidence)?;
            }
            let from = d.output.position();
            d.key("f4")?;
            d.wait_since(from, "[DESKTOP:EXIT] app=2")?;
            if !reboot {
                d.command("terminal check", 0)?;
                reclaim(&d.output.text())?;
                let text = d.output.text();
                let counts = text
                    .lines()
                    .filter(|s| s.contains("[TERMINAL:BOUNDARIES]"))
                    .map(|s| field(s, "task_pressure"))
                    .collect::<Result<Vec<_>>>()?;
                if counts.len() != 2 || counts[0] != 24 || counts[1] != 25 {
                    return Err("PTY session/escaped descendant left occupied task slots".into());
                }
                evidence.push_str("pty_task_pressure_before=24\npty_task_pressure_after=25\npty_and_global_descriptions_reused=true\n");
            }
            d.command("cat /persist/pty-work.txt", 0)?;
            if !d.output.text().contains("\r\nEDIT-Y\r\n") {
                return Err("Saved GUI-edited command bytes differ".into());
            }
            d.click(35, 60)?;
            let from = d.output.position();
            d.key("q")?;
            d.wait_since(from, "[INIT] desktop exited; COM1 shell remains available")?;
            d.command("sync", 0)?;
            d.shutdown()?;
            resources(&d.output.text(), &mut evidence)?;
            foundation_journal::check(&disk)?;
            inspect(&disk, &work, name, compiler, &mut evidence)?;
            vars.cleanup()?;
        }
        evidence.push_str("physical_input=true\nstdout_stderr_fixtures=true\nnonzero_child_status=23\ninterrupt_status=130\nresize_sigwinch=true\nscrollback_overflow=true\neof=true\nshell_restart=true\ncreator_termination_cleanup=true\nreboot=true\ne2fsck=true\n");
        Ok(())
    })();
    evidence.push_str(&match &outcome {
        Ok(()) => "result=Ok\n".into(),
        Err(e) => format!("result=Err\nerror={e}\n"),
    });
    fs::write(result, evidence)?;
    outcome?;
    println!(
        "Verified real graphical PTY input/output/status, Ctrl+C, resize, bounded history, exhaustion and clean reboot"
    );
    Ok(())
}

fn launch(d: &mut Driver) -> Result<()> {
    d.click(35, 60)?;
    let from = d.output.position();
    d.key("t")?;
    d.wait_since(from, "[TERMINAL:READY]")?;
    d.wait_since(from, "[TERMINAL:SHELL]")?;
    d.wait_since(from, "[TERMINAL:PRESENT]")?;
    d.click(470, 90)
}

fn reclaim(text: &str) -> Result<()> {
    let summary = text
        .lines()
        .rev()
        .find(|s| s.contains("[TERMINAL:BOUNDARIES]"))
        .ok_or("Missing PTY pressure summary")?;
    let owner = field(summary, "owner")?;
    let records: Vec<_> = text
        .lines()
        .rev()
        .filter(|s| s.contains("[PTY:RECLAIM]") && field(s, "owner").ok() == Some(owner))
        .take(4)
        .collect();
    if records.len() != 4 || records.iter().any(|s| !s.contains("exact=true")) {
        return Err(
            "Repeated PTY sessions did not restore exact heap/frame/open/pipe snapshots".into(),
        );
    }
    Ok(())
}

fn inspect(
    disk: &Path,
    work: &Path,
    name: &str,
    compiler: bool,
    evidence: &mut String,
) -> Result<()> {
    for (file, expected) in [
        ("pty-work.txt", Some(&b"EDIT-Y\n"[..])),
        (".anova-abi", Some(&b"linux.x86_64.v1\n"[..])),
        ("pty-app", None),
    ] {
        if file == "pty-app" && !compiler {
            continue;
        }
        let path = work.join(format!("{name}-{}", file.trim_start_matches('.')));
        output_path(&path)?;
        if path.exists() {
            fs::remove_file(&path)?;
        }
        command(
            Command::new("debugfs")
                .args(["-R", &format!("dump /{file} {}", path.display())])
                .arg(disk),
        )?;
        let bytes = fs::read(&path)?;
        if expected.is_some_and(|expected| bytes != expected) {
            return Err(format!("PTY retained bytes differ: {file}").into());
        }
        if name == "reboot"
            && bytes != fs::read(work.join(format!("graphical-{}", file.trim_start_matches('.'))))?
        {
            return Err(format!("PTY reboot changed retained bytes: {file}").into());
        }
        if file == "pty-app" {
            anova_elf::parse_linux(&bytes).map_err(|e| format!("Guest PTY output ELF: {e:?}"))?;
        }
        evidence.push_str(&format!(
            "{name}_{file}_sha256={:x}\n",
            Sha256::digest(bytes)
        ));
    }
    Ok(())
}

fn scenario(d: &mut Driver, work: &Path, compiler: bool, evidence: &mut String) -> Result<()> {
    type_command(d, "/linux/pty-fixture")?;
    screenshot(d, work, "streams", &["STDOUT-17", "STDERR-17"], evidence)?;
    // The original stream identity comes from the fixture's two explicit writes, not
    // from a claimed ability to distinguish stdout/stderr after PTY merging.
    type_command(d, "/linux/echo STATUS-$?")?;
    screenshot(d, work, "child-status", &["STATUS-23"], evidence)?;
    type_command(d, "/linux/pty-fixture jobs")?;
    screenshot(d, work, "stopped-child", &["JOBS-OK"], evidence)?;
    type_text(d, "/linux/echo EDIT-X")?;
    d.key("backspace")?;
    type_command(d, "Y > /persist/pty-work.txt")?;
    d.command("cat /persist/pty-work.txt", 0)?;
    if !d.output.text().contains("\r\nEDIT-Y\r\n") {
        return Err("PTY backspace or redirection did not execute".into());
    }

    type_command(d, "/linux/pty-fixture spin")?;
    screenshot(d, work, "running-child", &["SPIN-READY"], evidence)?;
    let began = Instant::now();
    d.command("echo PTY-INDEPENDENT-UART", 0)?;
    if began.elapsed() > INTERACTION {
        return Err("UART froze behind a live PTY child".into());
    }
    evidence.push_str(&format!(
        "uart_during_child_ms={}\n",
        began.elapsed().as_millis()
    ));
    let began = Instant::now();
    chord(d, "ctrl", "c")?;
    type_command(d, "/linux/echo INTERRUPT-$?")?;
    screenshot(d, work, "interrupt", &["INTERRUPT-130"], evidence)?;
    if began.elapsed() > INTERACTION {
        return Err("PTY Ctrl+C recovery exceeded five seconds".into());
    }
    evidence.push_str(&format!(
        "interrupt_scanout_ms={}\n",
        began.elapsed().as_millis()
    ));

    let flood_from = d.output.position();
    type_command(d, "/linux/pty-fixture flood")?;
    // The deliberately throttled 600-line child remains active during physical resize.
    d.move_to(760, 250)?;
    d.button(true)?;
    d.move_to(736, 234)?;
    d.button(false)?;
    d.wait_since(flood_from, "[TERMINAL:RESIZE] rows=24 columns=46")?;
    d.click(470, 90)?;
    screenshot(
        d,
        work,
        "flood-complete",
        &["END-17", "WINCH-SEEN", "SIZE ROWS=24 COLUMNS=46"],
        evidence,
    )?;
    d.wait_since(flood_from, "[TERMINAL:SCROLLBACK_FULL]")?;
    chord(d, "shift", "pgup")?;
    d.wait("[TERMINAL:HISTORY] offset=10")?;
    chord(d, "shift", "pgdn")?;
    d.wait("[TERMINAL:HISTORY] offset=0")?;
    // A peer still receives actual physical input after output pressure/resize.
    let (peer_x, peer_y) = d.window(d.client_pid(0)?)?;
    d.click(peer_x + 30, peer_y + 96)?;
    let began = Instant::now();
    let from = d.output.position();
    d.key("a")?;
    d.wait_since(from, "[GUI:C:KEY] codepoint=97")?;
    if began.elapsed() > INTERACTION {
        return Err("Peer GUI input exceeded five seconds".into());
    }
    evidence.push_str(&format!("peer_input_ms={}\n", began.elapsed().as_millis()));
    d.click(470, 90)?;
    let from = d.output.position();
    chord(d, "ctrl", "d")?;
    d.wait_since(from, "[TERMINAL:EOF]")?;
    d.wait_since(from, "[TERMINAL:EXIT]")?;
    let old = last_session(&d.output.text())?;
    let from = d.output.position();
    d.key("f5")?;
    d.wait_since(from, "[TERMINAL:SHELL]")?;
    if last_session(&d.output.text())? <= old {
        return Err("PTY restart reused a stale session".into());
    }
    if compiler {
        compile_run(d, work, false, evidence)?;
    }
    // Terminate a creator while a tagged descendant has escaped into a new SID.
    type_command(d, "/linux/pty-fixture family")?;
    screenshot(d, work, "family", &["FAMILY CHILD="], evidence)?;
    d.click(35, 60)?;
    let from = d.output.position();
    d.key("3")?;
    d.wait_since(from, "[PTY:CLOSED]")?;
    launch(d)?;
    Ok(())
}

fn compile_run(d: &mut Driver, work: &Path, reboot: bool, evidence: &mut String) -> Result<()> {
    if !reboot {
        type_command(
            d,
            "/bin/gcc /etc/terminal-hello.c -o /persist/pty-app; /linux/echo BUILD-$?",
        )?;
        screenshot(d, work, "compiled", &["BUILD-0"], evidence)?;
    }
    type_command(d, "/persist/pty-app; /linux/echo RUN-$?")?;
    screenshot(
        d,
        work,
        if reboot { "reboot-run" } else { "compiled-run" },
        &["M17-COMPILED", "RUN-7"],
        evidence,
    )?;
    Ok(())
}

fn type_command(d: &mut Driver, command: &str) -> Result<()> {
    type_text(d, command)?;
    d.key("ret")
}

fn type_text(d: &mut Driver, text: &str) -> Result<()> {
    // Every byte becomes physical make/break input. Small batches stay below the PS/2
    // queue bound; no UART or file injection substitutes for terminal keyboard input.
    for byte in text.bytes() {
        let (key, shift) = match byte {
            b'a'..=b'z' => ((byte as char).to_string(), false),
            b'A'..=b'Z' => (((byte + 32) as char).to_string(), true),
            b'0'..=b'9' => ((byte as char).to_string(), false),
            b' ' => ("spc".into(), false),
            b'/' => ("slash".into(), false),
            b'-' => ("minus".into(), false),
            b'.' => ("dot".into(), false),
            b';' => ("semicolon".into(), false),
            b'>' => ("dot".into(), true),
            b'$' => ("4".into(), true),
            b'?' => ("slash".into(), true),
            _ => return Err(format!("Unsupported PTY test input byte {byte}").into()),
        };
        let mut events = Vec::new();
        if shift {
            events.push(key_event("shift", true));
        }
        events.push(key_event(&key, true));
        events.push(key_event(&key, false));
        if shift {
            events.push(key_event("shift", false));
        }
        d.qmp
            .command("input-send-event", json!({"events": events}))?;
        d.poll()?;
        thread::sleep(Duration::from_millis(12));
    }
    Ok(())
}

fn key_event(key: &str, down: bool) -> serde_json::Value {
    json!({"type":"key","data":{"down":down,"key":{"type":"qcode","data":key}}})
}

fn chord(d: &mut Driver, modifier: &str, key: &str) -> Result<()> {
    d.qmp.command(
        "input-send-event",
        json!({"events":[key_event(modifier, true)]}),
    )?;
    d.key(key)?;
    d.qmp.command(
        "input-send-event",
        json!({"events":[key_event(modifier, false)]}),
    )?;
    Ok(())
}

fn screenshot(
    d: &mut Driver,
    work: &Path,
    name: &str,
    expected: &[&str],
    evidence: &mut String,
) -> Result<()> {
    // Keep the real hardware cursor outside the glyph samples; it can otherwise
    // legitimately cover a successful command's text during a scanout comparison.
    d.move_to(790, 580)?;
    let path = work.join(format!("{name}.ppm"));
    output_path(&path)?;
    let began = Instant::now();
    loop {
        d.poll()?;
        d.qmp
            .command("screendump", json!({"filename":path,"format":"ppm"}))?;
        let bytes = read_ppm(&path)?;
        let image = parse_ppm(&bytes)?;
        if image.width != 800 || image.height != 600 {
            return Err("PTY scanout dimensions changed".into());
        }
        let found = expected.iter().all(|text| contains(image.rgb, text));
        if found {
            evidence.push_str(&format!(
                "{name}_scanout_sha256={:x}\n",
                Sha256::digest(bytes)
            ));
            return Ok(());
        }
        if began.elapsed() > Duration::from_secs(35) {
            return Err(format!("PTY scanout missing {expected:?} in {name}").into());
        }
        thread::sleep(Duration::from_millis(30));
    }
}

fn contains(rgb: &[u8], text: &str) -> bool {
    // Independent literal glyphs and palette: do not call the renderer/model under test.
    for row in 0..26 {
        for column in 0..=50_usize.saturating_sub(text.len()) {
            let mut matches = true;
            for (index, byte) in text.bytes().enumerate() {
                let glyph = glyph(byte);
                let mut color = None;
                for (dy, bits) in glyph.into_iter().enumerate() {
                    for dx in 0..5 {
                        let at = ((44 + row * 8 + dy) * 800 + 454 + (column + index) * 6 + dx) * 3;
                        let pixel = &rgb[at..at + 3];
                        if bits & (16 >> dx) != 0 {
                            let p = [pixel[0], pixel[1], pixel[2]];
                            if ![[0xdd, 0xe5, 0xed], [0x66, 0xbb, 0x77], [0xcc, 0x55, 0x55]]
                                .contains(&p)
                                || color.is_some_and(|c| c != p)
                            {
                                matches = false;
                                break;
                            }
                            color = Some(p);
                        } else if pixel != [0x10, 0x18, 0x20] {
                            matches = false;
                            break;
                        }
                    }
                    if !matches {
                        break;
                    }
                }
                if !matches {
                    break;
                }
            }
            if matches {
                return true;
            }
        }
    }
    false
}

fn glyph(byte: u8) -> [u8; 7] {
    match byte {
        b'A' => [14, 17, 17, 31, 17, 17, 17],
        b'B' => [30, 17, 17, 30, 17, 17, 30],
        b'C' => [14, 17, 16, 16, 16, 17, 14],
        b'D' => [30, 17, 17, 17, 17, 17, 30],
        b'E' => [31, 16, 16, 30, 16, 16, 31],
        b'F' => [31, 16, 16, 30, 16, 16, 16],
        b'H' => [17, 17, 17, 31, 17, 17, 17],
        b'I' => [14, 4, 4, 4, 4, 4, 14],
        b'J' => [7, 2, 2, 2, 18, 18, 12],
        b'K' => [17, 18, 20, 24, 20, 18, 17],
        b'L' => [16, 16, 16, 16, 16, 16, 31],
        b'M' => [17, 27, 21, 21, 17, 17, 17],
        b'N' => [17, 25, 25, 21, 19, 19, 17],
        b'O' => [14, 17, 17, 17, 17, 17, 14],
        b'P' => [30, 17, 17, 30, 16, 16, 16],
        b'R' => [30, 17, 17, 30, 20, 18, 17],
        b'S' => [15, 16, 16, 14, 1, 1, 30],
        b'T' => [31, 4, 4, 4, 4, 4, 4],
        b'U' => [17, 17, 17, 17, 17, 17, 14],
        b'W' => [17, 17, 17, 21, 21, 27, 17],
        b'X' => [17, 17, 10, 4, 10, 17, 17],
        b'Y' => [17, 17, 10, 4, 4, 4, 4],
        b'Z' => [31, 1, 2, 4, 8, 16, 31],
        b'0' => [14, 17, 19, 21, 25, 17, 14],
        b'1' => [4, 12, 4, 4, 4, 4, 14],
        b'2' => [14, 17, 1, 2, 4, 8, 31],
        b'3' => [30, 1, 1, 14, 1, 1, 30],
        b'4' => [2, 6, 10, 18, 31, 2, 2],
        b'5' => [31, 16, 16, 30, 1, 1, 30],
        b'6' => [14, 16, 16, 30, 17, 17, 14],
        b'7' => [31, 1, 2, 4, 8, 8, 8],
        b'8' => [14, 17, 17, 14, 17, 17, 14],
        b'9' => [14, 17, 17, 15, 1, 1, 14],
        b'-' => [0, 0, 0, 31, 0, 0, 0],
        b'=' => [0, 0, 31, 0, 31, 0, 0],
        b' ' => [0; 7],
        _ => [31; 7],
    }
}

fn field(line: &str, name: &str) -> Result<u64> {
    let prefix = format!("{name}=");
    line.split_ascii_whitespace()
        .find_map(|s| s.strip_prefix(&prefix))
        .ok_or_else(|| format!("Missing PTY field {name}").into())
        .and_then(|s| Ok(s.parse()?))
}

fn last_session(text: &str) -> Result<u64> {
    field(
        text.lines()
            .rev()
            .find(|s| s.contains("[TERMINAL:SHELL]"))
            .ok_or("Missing PTY session")?,
        "session",
    )
}

fn resources(text: &str, evidence: &mut String) -> Result<()> {
    let line = text
        .lines()
        .rev()
        .find(|s| s.starts_with("[M15:RESOURCES]"))
        .ok_or("Missing PTY resource samples")?;
    for (name, max) in [
        ("heap_peak_bytes", 12 * 1024 * 1024),
        ("peak_frame_delta", 131072),
        ("live_tasks_peak", 32),
        ("occupied_slots_peak", 32),
        ("fd_per_process_peak", 32),
        ("vfs_node_slots_peak", 2048),
    ] {
        if field(line, name)? > max {
            return Err(format!("PTY resource budget exceeded: {name}").into());
        }
    }
    evidence.push_str(line);
    evidence.push('\n');
    Ok(())
}
