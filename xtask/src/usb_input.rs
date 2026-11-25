// SPDX-License-Identifier: Apache-2.0
//! Exercise actual USB Boot/Report input through xHCI, compositor focus and graphical PTY.
//! Retain independent UART/QMP, exact persisted PTY bytes, scanout and topology results.
use super::*;
use desktop_test::Driver;
use serde_json::json;
use sha2::{Digest, Sha256};

pub fn verify(root: &Path, boot: bool) -> Result<()> {
    let work = directory(
        root,
        if boot {
            "usb-boot-test"
        } else {
            "usb-input-test"
        },
    )?;
    begin_attempt(&work)?;
    let mut evidence =
        String::from("deadline_secs=180\ninteraction_budget_ms=5000\nphysical_hardware=pending\n");
    let outcome = (|| -> Result<()> {
        let archive = desktop::prepare(root)?;
        let kernel = build(
            root,
            if boot {
                Mode::UsbBootDesktop
            } else {
                Mode::UsbTestDesktop
            },
        )?;
        let iso = image_contents(
            root,
            &kernel,
            Some(&archive),
            if boot { "usb-boot" } else { "usb-report" },
        )?;
        evidence.push_str(&format!(
            "iso_sha256={:x}\n",
            Sha256::digest(fs::read(&iso)?)
        ));
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let topologies: &[(&str, &[&str])] = &[
            (
                "usb-only",
                &[
                    "-machine",
                    "i8042=off",
                    "-device",
                    "qemu-xhci,id=xhci",
                    "-device",
                    "usb-kbd,id=usbkbd,bus=xhci.0",
                    "-device",
                    "usb-mouse,id=usbmouse,bus=xhci.0",
                ],
            ),
            (
                "coexist",
                &[
                    "-device",
                    "qemu-xhci,id=xhci",
                    "-device",
                    "usb-kbd,id=usbkbd,bus=xhci.0",
                    "-device",
                    "usb-mouse,id=usbmouse,bus=xhci.0",
                ],
            ),
            (
                "usb-fullspeed",
                &[
                    "-machine",
                    "i8042=off",
                    "-device",
                    "qemu-xhci,id=xhci",
                    "-device",
                    "usb-kbd,id=usbkbd,bus=xhci.0,usb_version=1",
                    "-device",
                    "usb-mouse,id=usbmouse,bus=xhci.0,usb_version=1",
                ],
            ),
            ("ps2-only", &[]),
            ("no-input", &["-machine", "i8042=off"]),
        ];
        for &(name, options) in topologies {
            let result = work.join(format!("{name}.result.txt"));
            fs::write(&result, "result=Running\n")?;
            let run = (|| -> Result<()> {
                let disk = directory(root, "disks")?.join(format!(
                    "usb-{}-{name}-{stamp}.ext4",
                    if boot { "boot" } else { "report" }
                ));
                foundation_journal::create(root, &disk, true)?;
                let (mut d, vars) = Driver::boot_input(root, &iso, &disk, &work, name, options)?;
                d.wait("anova> ")?;
                d.ready(0, 0)?;
                let text = d.output.text();
                if name.starts_with("usb") || name == "coexist" {
                    if text.matches("[USB:DEVICE]").count() != 2
                        || text.matches("[USB:HID]").count() != 2
                        || text.contains("[USB:REJECT]")
                    {
                        return Err("USB devices did not enumerate/bind".into());
                    }
                    let protocol = if boot {
                        "protocol=boot"
                    } else {
                        "protocol=report"
                    };
                    if text
                        .lines()
                        .filter(|l| l.starts_with("[USB:HID]"))
                        .any(|l| !l.contains(protocol))
                    {
                        return Err("HID protocol did not match test profile".into());
                    }
                }
                if name == "no-input" {
                    d.wait_since(0, "[GUI:NO_INPUT]")?;
                    screenshot(&mut d, &work, &format!("{name}-desktop"), &mut evidence)?;
                } else {
                    // Prove focused window policy with an actual title drag and three buttons.
                    d.click(35, 60)?;
                    let from = d.output.position();
                    d.key("t")?;
                    d.wait_since(from, "[TERMINAL:READY]")?;
                    d.wait_since(from, "[TERMINAL:SHELL]")?;
                    d.wait_since(from, "[TERMINAL:PRESENT]")?;
                    d.click(470, 90)?;
                    if name == "usb-only" {
                        key_matrix(&mut d)?;
                        clip_edges(&mut d, &work, &mut evidence)?;
                        d.click(470, 90)?;
                    }
                    let interaction = Instant::now();
                    type_command(&mut d, "echo Aa1")?;
                    pty_terminal_test::screenshot(
                        &mut d,
                        &work,
                        &format!("{name}-echo"),
                        &["AA1"],
                        &mut evidence,
                    )?;
                    if interaction.elapsed() > Duration::from_secs(5) {
                        return Err("USB echo input-to-scanout exceeded five seconds".into());
                    }
                    evidence.push_str(&format!(
                        "{name}_echo_scanout_ms={}\n",
                        interaction.elapsed().as_millis()
                    ));
                    type_command(&mut d, "echo $? > /persist/usb-echo-status.txt")?;
                    type_command(&mut d, "echo Aa1 > /persist/usb-input.txt")?;
                    type_command(&mut d, "/linux/cat /persist/usb-input.txt")?;
                    pty_terminal_test::screenshot(
                        &mut d,
                        &work,
                        &format!("{name}-Aa1"),
                        &["AA1"],
                        &mut evidence,
                    )?;
                    // Editing and navigation must actually change the command accepted by ash.
                    type_text(&mut d, "echo BAD")?;
                    for _ in 0..3 {
                        d.key("backspace")?;
                    }
                    type_text(&mut d, "EDIT")?;
                    d.key("left")?;
                    d.key("right")?;
                    d.key("ret")?;
                    pty_terminal_test::screenshot(
                        &mut d,
                        &work,
                        &format!("{name}-edit"),
                        &["EDIT"],
                        &mut evidence,
                    )?;
                    let from = d.output.position();
                    type_command(&mut d, "/linux/pty-fixture spin")?;
                    pty_terminal_test::screenshot(
                        &mut d,
                        &work,
                        &format!("{name}-spin"),
                        &["SPIN-READY"],
                        &mut evidence,
                    )?;
                    pty_terminal_test::chord(&mut d, "ctrl", "c")?;
                    type_command(&mut d, "echo $? > /persist/usb-status.txt")?;
                    type_command(&mut d, "/linux/cat /persist/usb-status.txt")?;
                    pty_terminal_test::screenshot(
                        &mut d,
                        &work,
                        &format!("{name}-interrupt"),
                        &["130"],
                        &mut evidence,
                    )?;
                    // Reuse terminal shell replacement/function-key path independently of PTY output.
                    d.key("f5")?;
                    d.wait_since(from, "[TERMINAL:SHELL]")?;
                    if !boot && (name == "usb-only" || name == "coexist") {
                        type_command(&mut d, "/linux/pty-fixture flood")?;
                        pty_terminal_test::screenshot(
                            &mut d,
                            &work,
                            &format!("{name}-flood"),
                            &["END-17"],
                            &mut evidence,
                        )?;
                        let from = d.output.position();
                        wheel(&mut d, true)?;
                        d.wait_since(from, "[GUI:WHEEL] delta=1")?;
                        d.wait_since(from, "[TERMINAL:HISTORY] offset=10")?;
                        screenshot(&mut d, &work, &format!("{name}-wheel-up"), &mut evidence)?;
                        let from = d.output.position();
                        wheel(&mut d, false)?;
                        d.wait_since(from, "[GUI:WHEEL] delta=-1")?;
                        d.wait_since(from, "[TERMINAL:HISTORY] offset=0")?;
                        pty_terminal_test::screenshot(
                            &mut d,
                            &work,
                            &format!("{name}-wheel-down"),
                            &["END-17"],
                            &mut evidence,
                        )?;
                        evidence.push_str(&format!("{name}_wheel_consumer_history=10,0\n"));
                    }
                    d.move_to(470, 28)?;
                    d.button(true)?;
                    let moved = d.output.position();
                    d.move_to(490, 38)?;
                    d.wait_since(moved, "[GUI:INPUT_WINDOW] x=470 y=30 width=312 height=232")?;
                    d.button(false)?;
                    for (button, bits) in [("right", 2), ("middle", 4)] {
                        for down in [true, false] {
                            let from = d.output.position();
                            d.qmp.command("input-send-event",json!({"events":[{"type":"btn","data":{"down":down,"button":button}}]}))?;
                            d.wait_since(
                                from,
                                &format!(
                                    "[GUI:POINTER] x=490 y=38 buttons={}",
                                    if down { bits } else { 0 }
                                ),
                            )?;
                        }
                    }
                    screenshot(&mut d, &work, &format!("{name}-drag"), &mut evidence)?;
                    let from = d.output.position();
                    d.key("f4")?;
                    d.wait_since(from, "[DESKTOP:EXIT] app=2")?;
                    d.command("cat /persist/usb-input.txt", 0)?;
                    if !d.output.text().contains("\r\nAa1\r\n") {
                        return Err("Graphical PTY saved bytes differ".into());
                    }
                    evidence.push_str(&format!("{name}_pty=Aa1\n{name}_edit=true\n{name}_buttons=left,right,middle\n{name}_function_keys=true\n"));
                }
                d.command("echo USB-UART-RECOVERY", 0)?;
                d.shutdown()?;
                foundation_journal::check(&disk)?;
                if name != "no-input" {
                    let retained = work.join(format!("{name}-pty.bin"));
                    command(
                        Command::new("debugfs")
                            .args(["-R", &format!("dump /usb-input.txt {}", retained.display())])
                            .arg(&disk),
                    )?;
                    if fs::read(&retained)? != b"Aa1\n" {
                        return Err("Independent ext4 PTY byte inspection differs".into());
                    }
                }
                if name != "no-input" {
                    let retained = work.join(format!("{name}-status.bin"));
                    command(
                        Command::new("debugfs")
                            .args([
                                "-R",
                                &format!("dump /usb-status.txt {}", retained.display()),
                            ])
                            .arg(&disk),
                    )?;
                    if fs::read(retained)? != b"130\n" {
                        return Err("USB Ctrl+C PTY status differs".into());
                    }
                    let retained = work.join(format!("{name}-echo-status.bin"));
                    command(
                        Command::new("debugfs")
                            .args([
                                "-R",
                                &format!("dump /usb-echo-status.txt {}", retained.display()),
                            ])
                            .arg(&disk),
                    )?;
                    if fs::read(retained)? != b"0\n" {
                        return Err("USB echo PTY status differs".into());
                    }
                }
                let text = d.output.text();
                if [
                    "[USB:REJECT]",
                    "[USB:QUARANTINE]",
                    "[USB:INPUT_ERROR]",
                    "[USB:REPORT_REJECT]",
                ]
                .iter()
                .any(|m| text.contains(m))
                {
                    return Err("USB runtime reported a transport/report failure".into());
                }
                vars.cleanup()?;
                Ok(())
            })();
            fs::write(
                result,
                match &run {
                    Ok(()) => "result=Ok\n".into(),
                    Err(e) => format!("result=Err\nerror={e}\n"),
                },
            )?;
            run?;
        }
        Ok(())
    })();
    evidence.push_str(&match &outcome {
        Ok(()) => "result=Ok\n".into(),
        Err(e) => format!("result=Err\nerror={e}\n"),
    });
    fs::write(work.join("result.txt"), evidence)?;
    outcome
}
/// Archive existing regular evidence before publishing current Running/Pending states.
/// A previous successful result must never describe a new, incomplete or failed invocation.
pub(super) fn begin_attempt(work: &Path) -> Result<()> {
    let result = work.join("result.txt");
    output_path(&result)?;
    if result.exists() {
        let attempts = work.join("attempts");
        match fs::symlink_metadata(&attempts) {
            Ok(m) if !m.file_type().is_dir() => {
                return Err("USB attempts path is not a real directory".into());
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&attempts)?,
            Err(e) => return Err(e.into()),
        }
        let saved = attempts.join(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)?
                .as_nanos()
                .to_string(),
        );
        fs::create_dir(&saved)?;
        for entry in fs::read_dir(work)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                fs::copy(entry.path(), saved.join(entry.file_name()))?;
            }
        }
    }
    fs::write(&result, "result=Running\n")?;
    for name in [
        "usb-only",
        "coexist",
        "usb-fullspeed",
        "ps2-only",
        "no-input",
    ] {
        let path = work.join(format!("{name}.result.txt"));
        output_path(&path)?;
        fs::write(path, "result=Pending\n")?;
    }
    Ok(())
}

fn screenshot(d: &mut Driver, work: &Path, name: &str, evidence: &mut String) -> Result<()> {
    let path = work.join(format!("{name}.ppm"));
    output_path(&path)?;
    d.qmp.command("screendump", json!({"filename":path}))?;
    let bytes = graphics::read_ppm(&path)?;
    let screen = graphics::parse_ppm(&bytes)?;
    if screen.width != 800
        || screen.height != 600
        || screen.rgb.chunks_exact(3).all(|c| c == [0, 0, 0])
    {
        return Err("USB desktop has invalid scanout".into());
    }
    if name.ends_with("-drag") {
        let pixel = |x: usize, y: usize| &screen.rgb[(y * 800 + x) * 3..(y * 800 + x) * 3 + 3];
        if pixel(470, 30) != [0xe0, 0xe8, 0xf0] || pixel(450, 20) != [0x18, 0x28, 0x3b] {
            return Err("Dragged terminal scanout geometry differs".into());
        }
    }
    evidence.push_str(&format!(
        "{name}_scanout_sha256={:x}\n",
        Sha256::digest(bytes)
    ));
    Ok(())
}

fn type_text(d: &mut Driver, text: &str) -> Result<()> {
    // Wait for each physical make/break through the real compositor consumer before
    // injecting the next character. This bounds the QEMU HID event FIFO without retries.
    for byte in text.bytes() {
        let (key, shift) = pty_terminal_test::text_key(byte)?;
        if shift {
            ack_key(d, "shift", 42, true)?;
        }
        let code = match key.as_str() {
            _ if key.len() == 1 && key.as_bytes()[0].is_ascii_lowercase() => [
                30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22,
                47, 17, 45, 21, 44,
            ][usize::from(key.as_bytes()[0] - b'a')],
            _ if key.len() == 1 && (b'1'..=b'9').contains(&key.as_bytes()[0]) => {
                u16::from(key.as_bytes()[0] - b'1') + 2
            }
            "0" => 11,
            "spc" => 57,
            "slash" => 53,
            "minus" => 12,
            "dot" => 52,
            "semicolon" => 39,
            "equal" => 13,
            "apostrophe" => 40,
            "ret" => 28,
            _ => return Err(format!("Missing USB physical text key {key}").into()),
        };
        ack_key(d, &key, code, true)?;
        ack_key(d, &key, code, false)?;
        if shift {
            ack_key(d, "shift", 42, false)?;
        }
    }
    Ok(())
}
pub(super) fn ack_key(d: &mut Driver, key: &str, code: u16, down: bool) -> Result<()> {
    let began = Instant::now();
    let from = d.output.position();
    raw_key(d, key, down)?;
    d.wait_since(
        from,
        &format!(
            "[USB:KEY] code={code} state={}",
            if down { "Pressed" } else { "Released" }
        ),
    )?;
    if began.elapsed() > Duration::from_secs(5) {
        return Err("USB key delivery exceeded five seconds".into());
    }
    Ok(())
}
pub(super) fn type_command(d: &mut Driver, text: &str) -> Result<()> {
    type_text(d, text)?;
    ack_key(d, "ret", 28, true)?;
    ack_key(d, "ret", 28, false)
}

fn wheel(d: &mut Driver, up: bool) -> Result<()> {
    let button = if up { "wheel-up" } else { "wheel-down" };
    for down in [true, false] {
        d.qmp.command(
            "input-send-event",
            json!({"events":[{"type":"btn","data":{"button":button,"down":down}}]}),
        )?;
        thread::sleep(Duration::from_millis(30));
    }
    Ok(())
}
fn raw_key(d: &mut Driver, key: &str, down: bool) -> Result<()> {
    d.qmp.command(
        "input-send-event",
        json!({"events":[{"type":"key","data":{"down":down,"key":{"type":"qcode","data":key}}}]}),
    )?;
    thread::sleep(Duration::from_millis(40));
    Ok(())
}
fn key_matrix(d: &mut Driver) -> Result<()> {
    for (key, code) in [
        ("shift", 42),
        ("ctrl", 29),
        ("alt", 56),
        ("up", 328),
        ("down", 336),
        ("left", 331),
        ("right", 333),
        ("home", 327),
        ("end", 335),
        ("insert", 338),
        ("delete", 339),
        ("pgup", 329),
        ("pgdn", 337),
        ("f1", 59),
        ("f2", 60),
        ("f3", 61),
        ("f6", 64),
        ("f7", 65),
        ("f8", 66),
        ("f9", 67),
        ("f10", 68),
        ("f11", 87),
        ("f12", 88),
    ] {
        let from = d.output.position();
        d.key(key)?;
        d.wait_since(from, &format!("[USB:KEY] code={code} state=Pressed"))?;
        d.wait_since(from, &format!("[USB:KEY] code={code} state=Released"))?;
    }
    // Real simultaneous key snapshots and a scheduler-timed repeat followed by release.
    raw_key(d, "shift", true)?;
    raw_key(d, "a", true)?;
    let from = d.output.position();
    raw_key(d, "b", true)?;
    d.wait_since(from, "[USB:KEY] code=48 state=Pressed modifiers=1")?;
    raw_key(d, "shift", false)?;
    raw_key(d, "b", false)?;
    let from = d.output.position();
    d.wait_since(from, "[USB:KEY] code=30 state=Repeat modifiers=0")?;
    let from = d.output.position();
    raw_key(d, "a", false)?;
    d.wait_since(from, "[USB:KEY] code=30 state=Released modifiers=0")?;
    pty_terminal_test::chord(d, "ctrl", "c")?;
    Ok(())
}

fn clip_edges(d: &mut Driver, work: &Path, evidence: &mut String) -> Result<()> {
    for (name, dx, dy, x, y) in [
        ("top-left", -127, -127, 0, 0),
        ("bottom-right", 127, 127, 799, 599),
        ("top-right", 0, -127, 799, 0),
        ("bottom-left", -127, 127, 0, 599),
    ] {
        let from = d.output.position();
        for _ in 0..8 {
            d.qmp.command("input-send-event",json!({"events":[{"type":"rel","data":{"axis":"x","value":dx}},{"type":"rel","data":{"axis":"y","value":dy}}]}))?;
            thread::sleep(Duration::from_millis(30));
        }
        d.wait_since(from, &format!("[GUI:POINTER] x={x} y={y} buttons=0"))?;
        d.pointer = (x, y);
        screenshot(d, work, &format!("clip-{name}"), evidence)?;
        let bytes = graphics::read_ppm(&work.join(format!("clip-{name}.ppm")))?;
        let screen = graphics::parse_ppm(&bytes)?;
        for cy in 0..anova_graphics::CURSOR_HEIGHT {
            for cx in 0..anova_graphics::CURSOR_WIDTH {
                if x + cx < 800
                    && y + cy < 600
                    && let Some(color) = anova_graphics::cursor_color(cx, cy)
                {
                    let at = ((y + cy) * 800 + x + cx) as usize * 3;
                    let expected = [(color >> 16) as u8, (color >> 8) as u8, color as u8];
                    if screen.rgb[at..at + 3] != expected {
                        return Err("USB clipped cursor pixels differ".into());
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod evidence_tests {
    use super::*;
    #[test]
    fn rerun_preserves_failed_uart_and_clears_stale_success_before_work() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let temp = Temporary::new(root, "usb-evidence").unwrap();
        fs::write(
            temp.0.join("result.txt"),
            "result=Err\noriginal_failure=true\n",
        )
        .unwrap();
        fs::write(
            temp.0.join("usb-only.serial.log"),
            b"original UART bytes\r\n",
        )
        .unwrap();
        fs::write(temp.0.join("no-input.result.txt"), "result=Ok\n").unwrap();
        begin_attempt(&temp.0).unwrap();
        assert_eq!(
            fs::read_to_string(temp.0.join("result.txt")).unwrap(),
            "result=Running\n"
        );
        assert_eq!(
            fs::read_to_string(temp.0.join("no-input.result.txt")).unwrap(),
            "result=Pending\n"
        );
        let archive = fs::read_dir(temp.0.join("attempts"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            fs::read(archive.join("usb-only.serial.log")).unwrap(),
            b"original UART bytes\r\n"
        );
        assert!(
            fs::read_to_string(archive.join("result.txt"))
                .unwrap()
                .contains("original_failure=true")
        );
        temp.cleanup().unwrap();
    }
}
