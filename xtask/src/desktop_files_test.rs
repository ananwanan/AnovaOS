// SPDX-License-Identifier: Apache-2.0
//! Exercise graphical source editing, protected saves and the shared PTY work disk across clean reboot.
//! Physical QMP input, independent UART reads and stopped-disk dumps provide separate observations.

use super::*;
use desktop_test::Driver;
use graphics::{parse_ppm, read_ppm};
use pty_terminal_test::{chord, glyph, screenshot, type_command, type_text};
use serde_json::json;
use sha2::{Digest, Sha256};

const C_INITIAL: &str = "// M17-FILES-C\nint main(void) { return 17; }\n";
const C_FINAL: &str =
    "// M17-FILES-C\nint main(void) { return 17; }\n// EDITED\n// RETRY\n// CONFLICT\n";
const CPP: &str = "// M17-FILES-CPP\nint main() { return 18; }\n";
const RUST: &str = "// M17-FILES-RUST\nfn main() {}\n";

pub fn verify(root: &Path) -> Result<()> {
    let work = directory(root, "desktop-files-test")?;
    let result = work.join("result.txt");
    output_path(&result)?;
    fs::write(&result, "result=Running\n")?;
    let mut evidence = String::from(
        "profile=M17.3-minimal-q35-tcg-1BSP-3GiB-800x600\nsource_input=physical-PS2\nbuffer_limit_bytes=16384\npath_limit_bytes=120\nabrupt_power_loss=unverified\nguest_storage_exhaustion=unverified\n",
    );
    let outcome = (|| -> Result<()> {
        let iso = desktop::image(root)?;
        evidence.push_str(&format!(
            "iso_sha256={:x}\n",
            Sha256::digest(fs::read(&iso)?)
        ));
        evidence.push_str(&fs::read_to_string(root.join("build/desktop/inputs.txt"))?);
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let disk = directory(root, "disks")?.join(format!("desktop-files-{stamp}.ext4"));
        foundation_journal::create(root, &disk, true)?;
        evidence.push_str(&format!("disk={}\n", disk.display()));
        for reboot in [false, true] {
            let name = if reboot { "reboot" } else { "graphical" };
            let (mut d, vars) = Driver::boot(root, &iso, &disk, &work, name)?;
            d.wait("anova> ")?;
            d.ready(0, 0)?;
            launch_files(&mut d)?;
            if reboot {
                for (path, bytes) in sources() {
                    prompt(&mut d, "f1", path)?;
                    d.wait("[FILES:OPEN]")?;
                    check_record(&d.output.text(), "[FILES:OPEN]", path, bytes.len())?;
                }
            } else {
                scenario(&mut d)?;
            }
            editor_scanout(
                &mut d,
                &work,
                name,
                if reboot {
                    "M17-FILES-RUST"
                } else {
                    "M17-FILES-C"
                },
                &mut evidence,
            )?;
            let from = d.output.position();
            d.key("f4")?;
            d.wait_since(from, "[DESKTOP:EXIT] app=1")?;
            launch_terminal(&mut d)?;
            for (path, expected) in [
                ("/persist/src/renamed.c", "M17-FILES-C"),
                ("/persist/src/main.cpp", "M17-FILES-CPP"),
                ("/persist/src/main.rs", "M17-FILES-RUST"),
            ] {
                type_command(&mut d, &format!("/linux/cat {path}"))?;
                screenshot(
                    &mut d,
                    &work,
                    &format!("{name}-{}", path.rsplit('/').next().unwrap()),
                    &[expected],
                    &mut evidence,
                )?;
            }
            let from = d.output.position();
            d.key("f4")?;
            d.wait_since(from, "[DESKTOP:EXIT] app=2")?;
            for (path, bytes) in sources() {
                serial_exact(&mut d, path, bytes)?;
            }
            d.command("cat /persist/src/main.c", 1)?;
            d.command("cat /persist/src/trash.c", 1)?;
            d.click(35, 60)?;
            let from = d.output.position();
            d.key("q")?;
            d.wait_since(from, "[INIT] desktop exited; COM1 shell remains available")?;
            d.command("sync", 0)?;
            d.shutdown()?;
            foundation_journal::check(&disk)?;
            inspect(&disk, &work, name, &mut evidence)?;
            vars.cleanup()?;
        }
        evidence.push_str("create_c_cpp_rust=true\nmodify=true\nrename=true\ndelete=true\nunsaved_close_cancel=true\nlauncher_stop_restart_exit_dirty_guard=true\noccupied_temp_save_error_dirty_retained=true\nexternal_change_conflict=true\nexplicit_overwrite=true\nshared_graphical_pty=true\nreboot_exact_bytes=true\ne2fsck=true\n");
        Ok(())
    })();
    evidence.push_str(&match &outcome {
        Ok(()) => "result=Ok\n".into(),
        Err(error) => format!("result=Err\nerror={error}\n"),
    });
    fs::write(result, evidence)?;
    outcome?;
    println!(
        "Verified bounded graphical files/editor, failed-save and unsaved/overwrite protection, shared PTY paths and exact clean-reboot bytes"
    );
    Ok(())
}

fn sources() -> [(&'static str, &'static str); 3] {
    [
        ("/persist/src/renamed.c", C_FINAL),
        ("/persist/src/main.cpp", CPP),
        ("/persist/src/main.rs", RUST),
    ]
}

fn launch_files(d: &mut Driver) -> Result<()> {
    d.click(35, 60)?;
    let from = d.output.position();
    d.key("f")?;
    d.wait_since(from, "[FILES:READY]")?;
    d.wait_since(from, "[FILES:PRESENT]")?;
    focus_files(d)
}

fn focus_files(d: &mut Driver) -> Result<()> {
    let text = d.output.text();
    let record = text
        .lines()
        .rev()
        .find(|s| s.contains("[DESKTOP:WINDOW]"))
        .ok_or("Missing Files window geometry")?;
    let field = |name: &str| -> Result<u32> {
        let prefix = format!("{name}=");
        Ok(record
            .split_whitespace()
            .find_map(|s| s.strip_prefix(&prefix))
            .ok_or("Missing Files window field")?
            .parse()?)
    };
    d.click(field("x")? + 20, field("y")? + 70)
}

fn editor_scanout(
    d: &mut Driver,
    work: &Path,
    name: &str,
    expected: &str,
    evidence: &mut String,
) -> Result<()> {
    let text = d.output.text();
    let record = text
        .lines()
        .rev()
        .find(|s| s.contains("[DESKTOP:WINDOW]"))
        .ok_or("Missing Files geometry for scanout")?;
    let coordinate = |name: &str| -> Result<usize> {
        let prefix = format!("{name}=");
        Ok(record
            .split_whitespace()
            .find_map(|s| s.strip_prefix(&prefix))
            .ok_or("Missing Files coordinate")?
            .parse()?)
    };
    let (x, y) = (coordinate("x")?, coordinate("y")?);
    let samples = [
        (x + 4 + 3 * 6, y + 56, expected),
        (x + 4, y + 18, "F1 OPEN"),
    ];
    d.move_to(790, 580)?;
    let path = work.join(format!("{name}-editor.ppm"));
    output_path(&path)?;
    let began = Instant::now();
    loop {
        d.poll()?;
        d.qmp
            .command("screendump", json!({"filename":path,"format":"ppm"}))?;
        let bytes = read_ppm(&path)?;
        let image = parse_ppm(&bytes)?;
        if image.width != 800 || image.height != 600 {
            return Err("Editor scanout dimensions changed".into());
        }
        // Independent literal font and exact palette samples. The source begins
        // with three comment-prefix cells; no implementation renderer is invoked.
        let matches = samples.iter().all(|&(x, y, text)| {
            text.bytes().enumerate().all(|(column, byte)| {
                glyph(byte).into_iter().enumerate().all(|(row, bits)| {
                    (0..5).all(|dx| {
                        let at = ((y + row) * 800 + x + column * 6 + dx) * 3;
                        let expected = if bits & (16 >> dx) != 0 {
                            [255, 255, 255]
                        } else {
                            [0x17, 0x23, 0x2f]
                        };
                        image.rgb.get(at..at + 3) == Some(&expected[..])
                    })
                })
            })
        });
        if matches {
            evidence.push_str(&format!(
                "{name}_editor_scanout_sha256={:x}\n",
                Sha256::digest(bytes)
            ));
            return Ok(());
        }
        if began.elapsed() > Duration::from_secs(5) {
            return Err(format!("Editor scanout missing exact source glyphs: {expected}").into());
        }
        thread::sleep(Duration::from_millis(30));
    }
}

fn launch_terminal(d: &mut Driver) -> Result<()> {
    d.click(35, 60)?;
    let from = d.output.position();
    d.key("t")?;
    d.wait_since(from, "[TERMINAL:READY]")?;
    d.wait_since(from, "[TERMINAL:SHELL]")?;
    d.wait_since(from, "[TERMINAL:PRESENT]")?;
    d.click(470, 90)
}

fn prompt(d: &mut Driver, key: &str, path: &str) -> Result<()> {
    d.key(key)?;
    type_text(d, path)?;
    d.key("ret")
}

fn save(d: &mut Driver, path: &str, bytes: &str) -> Result<()> {
    let from = d.output.position();
    d.key("f5")?;
    d.wait_since(from, "[FILES:SAVED]")?;
    check_record(&d.output.text(), "[FILES:SAVED]", path, bytes.len())
}

fn create(d: &mut Driver, path: &str, bytes: &str) -> Result<()> {
    prompt(d, "f2", path)?;
    d.wait("[FILES:NEW]")?;
    type_text(d, bytes)?;
    save(d, path, bytes)
}

fn scenario(d: &mut Driver) -> Result<()> {
    prompt(d, "f8", "/persist/src")?;
    d.wait("[FILES:MKDIR]")?;
    prompt(d, "f1", "/persist/src")?;
    d.wait("[FILES:BROWSE] cwd=/persist/src")?;
    create(d, "/persist/src/main.c", C_INITIAL)?;
    type_text(d, "// EDITED\n")?;
    let from = d.output.position();
    d.key("f4")?;
    d.wait_since(from, "[FILES:UNSAVED]")?;
    d.key("esc")?;
    d.click(35, 60)?;
    let from = d.output.position();
    d.key("2")?;
    d.wait_since(from, "[DESKTOP:LAUNCH_ERROR] error=-24")?;
    d.key("r")?;
    d.key("q")?;
    focus_files(d)?;
    let edited = format!("{C_INITIAL}// EDITED\n");
    save(d, "/persist/src/main.c", &edited)?;

    // A real occupied transaction file must be diagnosed, without truncating it
    // or the prior target. UART only constructs the external collision fixture.
    d.command("echo RESERVED > /persist/src/main.c.m17-tmp", 0)?;
    type_text(d, "// RETRY\n")?;
    let from = d.output.position();
    d.key("f5")?;
    d.wait_since(from, "[FILES:ERROR]")?;
    let text = d.output.text();
    let error = text[from..]
        .lines()
        .find(|s| s.contains("[FILES:ERROR]"))
        .ok_or("Missing failed-save record")?;
    if !error.contains("dirty=true") {
        return Err("Failed editor save falsely cleared dirty state".into());
    }
    serial_exact(d, "/persist/src/main.c", &edited)?;
    serial_exact(d, "/persist/src/main.c.m17-tmp", "RESERVED\n")?;
    d.command("rm /persist/src/main.c.m17-tmp", 0)?;
    let retry = format!("{edited}// RETRY\n");
    save(d, "/persist/src/main.c", &retry)?;

    // Change the same VFS file through a separate process. The editor must
    // compare its loaded baseline and require an explicit overwrite gesture.
    d.command("echo EXTERNAL > /persist/src/main.c", 0)?;
    type_text(d, "// CONFLICT\n")?;
    let from = d.output.position();
    chord(d, "ctrl", "s")?;
    d.wait_since(from, "[FILES:CONFLICT]")?;
    d.key("esc")?;
    serial_exact(d, "/persist/src/main.c", "EXTERNAL\n")?;
    let from = d.output.position();
    d.key("f5")?;
    d.wait_since(from, "[FILES:CONFLICT]")?;
    d.key("y")?;
    d.wait_since(from, "[FILES:SAVED]")?;
    check_record(
        &d.output.text(),
        "[FILES:SAVED]",
        "/persist/src/main.c",
        C_FINAL.len(),
    )?;
    prompt(d, "f3", "/persist/src/renamed.c")?;
    d.wait("[FILES:RENAMED]")?;
    create(d, "/persist/src/main.cpp", CPP)?;
    create(d, "/persist/src/main.rs", RUST)?;
    create(d, "/persist/src/trash.c", "// DELETE\n")?;
    let from = d.output.position();
    d.key("f6")?;
    d.key("y")?;
    d.wait_since(from, "[FILES:DELETED]")?;
    d.key("f7")?;
    let from = d.output.position();
    d.key("backspace")?;
    d.wait_since(from, "[FILES:BROWSE] cwd=/persist")?;
    prompt(d, "f1", "/persist/src/renamed.c")?;
    d.wait("[FILES:OPEN]")?;
    check_record(
        &d.output.text(),
        "[FILES:OPEN]",
        "/persist/src/renamed.c",
        C_FINAL.len(),
    )
}

fn check_record(text: &str, marker: &str, path: &str, bytes: usize) -> Result<()> {
    let record = text
        .lines()
        .rev()
        .find(|s| s.contains(marker))
        .ok_or("Missing file operation record")?;
    let expected_path = format!("path={path}");
    let expected_bytes = format!("bytes={bytes}");
    if !record.split_whitespace().any(|s| s == expected_path)
        || !record.split_whitespace().any(|s| s == expected_bytes)
    {
        return Err(format!("File operation differs from expected path/bytes: {record}").into());
    }
    if marker == "[FILES:SAVED]" && !record.contains("dirty=false") {
        return Err("Successful editor save did not establish clean state".into());
    }
    Ok(())
}

fn serial_exact(d: &mut Driver, path: &str, expected: &str) -> Result<()> {
    let from = d.output.position();
    d.command(&format!("cat {path}"), 0)?;
    let rendered = expected.replace('\n', "\r\n");
    if !d.output.text()[from..].contains(&format!("\r\n{rendered}[USER:EXIT]")) {
        return Err(format!("Independent serial read differs: {path}").into());
    }
    Ok(())
}

fn inspect(disk: &Path, work: &Path, name: &str, evidence: &mut String) -> Result<()> {
    for (path, expected) in sources()
        .into_iter()
        .chain([("/persist/.anova-abi", "linux.x86_64.v1\n")])
    {
        let filename = path.rsplit('/').next().ok_or("Missing file basename")?;
        let saved = work.join(format!("{name}-{}", filename.trim_start_matches('.')));
        output_path(&saved)?;
        if saved.exists() {
            fs::remove_file(&saved)?;
        }
        let disk_path = path
            .strip_prefix("/persist")
            .ok_or("Unexpected work path")?;
        command(
            Command::new("debugfs")
                .args(["-R", &format!("dump {disk_path} {}", saved.display())])
                .arg(disk),
        )?;
        let bytes = fs::read(&saved)?;
        if bytes != expected.as_bytes() {
            return Err(format!("Stopped-disk bytes differ: {path}").into());
        }
        if name == "reboot"
            && bytes
                != fs::read(work.join(format!("graphical-{}", filename.trim_start_matches('.'))))?
        {
            return Err(format!("Clean reboot changed source bytes: {path}").into());
        }
        evidence.push_str(&format!(
            "{name}_{filename}_sha256={:x}\n",
            Sha256::digest(bytes)
        ));
    }
    Ok(())
}
