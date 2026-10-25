// SPDX-License-Identifier: Apache-2.0
//! Compare QEMU's actual standard-VGA scanout with the software reference and prove COM1 recovery.
//! Bound QMP/PPM input, retain evidence, and require clean ACPI shutdown with or without a display.
use super::*;
use anova_graphics::Surface;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;
// Independently calculated from the documented integer geometry and FNV-1a RGB byte order.
const REFERENCE_CHECKSUM: u64 = 0x9ed0_6ea9_b3ba_5f91;
const DEADLINE: Duration = Duration::from_secs(30);
const QMP_LIMIT: usize = 64 * 1024;
const PPM_LIMIT: usize = 16 * 1024 * 1024;

pub fn verify(root: &Path) -> Result<()> {
    let logs = directory(root, "logs")?;
    let result = logs.join("graphics-test.result.txt");
    output_path(&result)?;
    fs::write(&result, "result=Running\n")?;
    for name in ["graphics", "graphics-no-display"] {
        let pending = logs.join(format!("{name}.result.txt"));
        output_path(&pending)?;
        fs::write(pending, "result=Pending\n")?;
        let screenshot = logs.join(format!("{name}.ppm"));
        output_path(&screenshot)?;
        if screenshot.exists() {
            fs::remove_file(screenshot)?;
        }
    }
    let outcome = (|| -> Result<()> {
        let iso = image(root, Mode::Graphics)?;
        session(root, &iso, true)?;
        session(root, &iso, false)
    })();
    fs::write(
        result,
        match &outcome {
            Ok(()) => "result=Ok\nscenarios=graphics,graphics-no-display\n".into(),
            Err(error) => format!("result=Err\nerror={error}\n"),
        },
    )?;
    outcome?;
    println!(
        "Verified actual 800x600 standard-VGA scanout, all reference pixels, absent-display fallback, COM1 recovery and clean ACPI shutdown"
    );
    Ok(())
}

fn session(root: &Path, iso: &Path, display: bool) -> Result<()> {
    let name = if display {
        "graphics"
    } else {
        "graphics-no-display"
    };
    let logs = directory(root, "logs")?;
    let serial = logs.join(format!("{name}.serial.log"));
    let diagnostic = logs.join(format!("{name}.qemu.log"));
    let qmp_log = logs.join(format!("{name}.qmp.log"));
    let screenshot = logs.join(format!("{name}.ppm"));
    let result_path = logs.join(format!("{name}.result.txt"));
    for path in [&serial, &diagnostic, &qmp_log, &screenshot, &result_path] {
        output_path(path)?;
    }
    fs::write(&result_path, "result=Running\n")?;
    // A failed QMP capture must never pass by reading an earlier run's screenshot.
    if screenshot.exists() {
        fs::remove_file(&screenshot)?;
    }
    let vars = Temporary::new(root, "graphics")?;
    let socket = vars.0.join("qmp.sock");
    let mut cmd = qemu(root, iso, &vars.0, false)?;
    cmd.args(["-vga", if display { "std" } else { "none" }])
        .arg("-qmp")
        .arg(format!("unix:{},server=on,wait=off", socket.display()))
        .args(["-serial", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(File::create(&diagnostic)?);
    let mut evidence = format!(
        "scenario={name}\nqemu={cmd:?}\nprofile=q35,accel=tcg cpu=qemu64,+rdrand memory=256M smp=1 display=none vga={}\niso_sha256={:x}\nkernel_sha256={:x}\n",
        if display { "std" } else { "none" },
        Sha256::digest(fs::read(iso)?),
        Sha256::digest(fs::read(
            root.join("target").join(TARGET).join("debug/anova-kernel")
        )?),
    );
    println!("Running {name}: {cmd:?}");
    let outcome = (|| -> Result<()> {
        let mut guest = Guest(cmd.spawn()?);
        let mut output = transcript::Transcript::capture(
            guest.0.stdout.take().ok_or("Missing UART output pipe")?,
            File::create(&serial)?,
        )?;
        let mut input = guest.0.stdin.take().ok_or("Missing UART input pipe")?;
        let start = Instant::now();
        let mut qmp = Qmp::connect(&socket, &qmp_log, start)?;
        let mut step = 0;
        let mut cursor = 0;
        let mut ready_at = None;
        let mut verified = false;
        loop {
            output.poll()?;
            let exit = guest.0.try_wait()?;
            if exit.is_some() {
                output.finish()?;
            }
            let text = output.text();
            check_guest(&text, start.elapsed() >= DEADLINE)?;
            if display && text.contains("[GRAPHICS:FALLBACK]") {
                return Err("Standard-VGA guest rejected its framebuffer; inspect the fallback reason in the serial log".into());
            }
            if let Some(status) = exit {
                check_exit(&text, status.code(), step)?;
                evidence.push_str(&format!(
                    "qemu_exit={:?}\ntimed_out=false\nserial_recovery=true\nacpi_shutdown=true\n",
                    status.code()
                ));
                break;
            }
            if step == 0 {
                let marker = if display {
                    "[GRAPHICS:PRESENT]"
                } else {
                    "[GRAPHICS:FALLBACK]"
                };
                if text.contains("anova> ") && text.contains(marker) {
                    if display && !verified {
                        if text.contains("[GRAPHICS:FALLBACK]")
                            || !text.contains("[GRAPHICS:RECLAIM] heap_restored=true")
                        {
                            return Err(
                                "Graphics present did not reclaim its software buffer".into()
                            );
                        }
                        qmp.command(
                            "screendump",
                            json!({ "filename": screenshot, "format": "ppm" }),
                        )?;
                        let bytes = read_ppm(&screenshot)?;
                        let checksum = verify_pattern(&bytes)?;
                        evidence.push_str(&format!(
                            "screenshot={}\nscreenshot_sha256={:x}\nwidth={WIDTH}\nheight={HEIGHT}\ncompared_pixels={}\nreference_checksum={checksum:#x}\n",
                            screenshot.display(), Sha256::digest(&bytes), WIDTH * HEIGHT,
                        ));
                        if !text.lines().any(|line| {
                            line.contains("[GRAPHICS:PRESENT]")
                                && line.contains("readback=true")
                                && line.split_ascii_whitespace().any(|field| {
                                    field
                                        .strip_prefix("checksum=0x")
                                        .and_then(|value| u64::from_str_radix(value, 16).ok())
                                        == Some(checksum)
                                })
                        }) {
                            return Err(
                                "Guest present checksum differs from reference scanout".into()
                            );
                        }
                    } else if !display
                        && (text.contains("[GRAPHICS:PRESENT]")
                            || !text.contains("[GRAPHICS:FALLBACK] reason=no-framebuffer;")
                            || !text.contains("[GRAPHICS:RECLAIM] heap_restored=true"))
                    {
                        return Err("Absent-display case did not reclaim and recover from its missing framebuffer".into());
                    }
                    verified = true;
                    // Wait for the ordinary shell's blocking read after recording its prompt.
                    let since = *ready_at.get_or_insert_with(Instant::now);
                    if since.elapsed() >= Duration::from_millis(80) {
                        cursor = output.find_from(0, "anova> ").unwrap() + "anova> ".len();
                        input.write_all(b"echo M14-SERIAL-RECOVERY\r")?;
                        input.flush()?;
                        step = 1;
                    }
                }
            } else if step == 1
                && let Some(at) = output.find_from(cursor, "\r\nM14-SERIAL-RECOVERY\r\n")
            {
                cursor = at + "\r\nM14-SERIAL-RECOVERY\r\n".len();
                step = 2;
            } else if step == 2
                && let Some(at) = output.find_from(cursor, "anova> ")
            {
                cursor = at + "anova> ".len();
                input.write_all(b"shutdown\r")?;
                input.flush()?;
                step = 3;
            }
            thread::sleep(Duration::from_millis(20));
        }
        drop(guest);
        vars.cleanup()?;
        Ok(())
    })();
    match &outcome {
        Ok(()) => evidence.push_str("result=Ok\n"),
        Err(error) => evidence.push_str(&format!("result=Err\nerror={error}\n")),
    }
    fs::write(&result_path, evidence)?;
    outcome.map_err(|error| format!("{name}: {error}; see {}", result_path.display()).into())
}

fn check_guest(text: &str, timed_out: bool) -> Result<()> {
    if timed_out {
        return Err("Graphics guest exceeded the 30-second watchdog".into());
    }
    if [
        "[PANIC]",
        "[EXCEPTION]",
        "[INIT:FAIL]",
        "[SHELL:FAIL]",
        "[POWER:FAIL]",
    ]
    .iter()
    .any(|marker| text.contains(marker))
    {
        return Err("Graphics guest reported a fatal boot, runtime or shutdown failure".into());
    }
    Ok(())
}

fn check_exit(text: &str, code: Option<i32>, step: usize) -> Result<()> {
    if code != Some(0)
        || step != 3
        || !text.contains("[POWER:OFF] synced=true clean=true dma_reset=true")
    {
        return Err(format!("Unexpected graphics guest exit={code:?} step={step}").into());
    }
    Ok(())
}

pub(super) struct Qmp {
    stream: UnixStream,
    log: File,
    id: u64,
}

impl Qmp {
    pub(super) fn connect(socket: &Path, log: &Path, start: Instant) -> Result<Self> {
        let stream = loop {
            match UnixStream::connect(socket) {
                Ok(stream) => break stream,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                    ) && start.elapsed() < Duration::from_secs(5) =>
                {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(error) => return Err(format!("Cannot connect QMP: {error}").into()),
            }
        };
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let mut qmp = Self {
            stream,
            log: File::create(log)?,
            id: 0,
        };
        if qmp.message(Instant::now())?.get("QMP").is_none() {
            return Err("Missing QMP greeting".into());
        }
        qmp.command("qmp_capabilities", json!({}))?;
        Ok(qmp)
    }

    fn message(&mut self, start: Instant) -> Result<Value> {
        let mut bytes = Vec::new();
        loop {
            let remaining = Duration::from_secs(5)
                .checked_sub(start.elapsed())
                .filter(|remaining| !remaining.is_zero())
                .ok_or("QMP reply exceeded five seconds")?;
            self.stream.set_read_timeout(Some(remaining))?;
            let mut byte = [0];
            self.stream.read_exact(&mut byte)?;
            bytes.push(byte[0]);
            if bytes.len() > QMP_LIMIT {
                return Err("QMP message exceeds 64 KiB".into());
            }
            if byte[0] == b'\n' {
                self.log.write_all(&bytes)?;
                self.log.flush()?;
                let value: Value = serde_json::from_slice(&bytes)?;
                if !value.is_object() {
                    return Err("QMP response is not an object".into());
                }
                return Ok(value);
            }
        }
    }

    pub(super) fn command(&mut self, name: &str, arguments: Value) -> Result<()> {
        self.id += 1;
        let value = json!({ "execute": name, "arguments": arguments, "id": self.id });
        let encoded = serde_json::to_vec(&value)?;
        self.log.write_all(b"-> ")?;
        self.log.write_all(&encoded)?;
        self.log.write_all(b"\n")?;
        self.stream.write_all(&encoded)?;
        self.stream.write_all(b"\r\n")?;
        let start = Instant::now();
        loop {
            let reply = self.message(start)?;
            if reply.get("event").is_some() {
                continue;
            }
            if reply.get("id").and_then(Value::as_u64) != Some(self.id) {
                return Err("QMP response transaction does not match the request".into());
            }
            if let Some(error) = reply.get("error") {
                return Err(format!("QMP {name} failed: {error}").into());
            }
            if reply.get("return").is_none() {
                return Err("QMP response has neither return nor error".into());
            }
            return Ok(());
        }
    }
}

pub(super) struct Ppm<'a> {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) rgb: &'a [u8],
}

pub(super) fn read_ppm(path: &Path) -> Result<Vec<u8>> {
    checked_file(path)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(PPM_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > PPM_LIMIT {
        return Err("Screenshot exceeds 16 MiB".into());
    }
    Ok(bytes)
}

pub(super) fn parse_ppm(bytes: &[u8]) -> Result<Ppm<'_>> {
    fn token<'a>(bytes: &'a [u8], cursor: &mut usize) -> Result<&'a [u8]> {
        loop {
            while bytes.get(*cursor).is_some_and(u8::is_ascii_whitespace) {
                *cursor += 1;
            }
            if bytes.get(*cursor) != Some(&b'#') {
                break;
            }
            while bytes.get(*cursor).is_some_and(|byte| *byte != b'\n') {
                *cursor += 1;
            }
        }
        let begin = *cursor;
        while bytes
            .get(*cursor)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'#')
        {
            *cursor += 1;
        }
        if begin == *cursor || *cursor > 1024 {
            return Err("Missing or overlong PPM header token".into());
        }
        Ok(&bytes[begin..*cursor])
    }
    fn dimension(token: &[u8]) -> Result<u32> {
        if token.is_empty() || !token.iter().all(u8::is_ascii_digit) {
            return Err("Non-decimal PPM dimension".into());
        }
        let value = std::str::from_utf8(token)?.parse()?;
        if !(1..=4096).contains(&value) {
            return Err("PPM dimension is outside 1..4096".into());
        }
        Ok(value)
    }
    if bytes.len() > PPM_LIMIT {
        return Err("Screenshot exceeds 16 MiB".into());
    }
    let mut cursor = 0;
    if token(bytes, &mut cursor)? != b"P6" {
        return Err("Screenshot is not a binary P6 PPM".into());
    }
    let width = dimension(token(bytes, &mut cursor)?)?;
    let height = dimension(token(bytes, &mut cursor)?)?;
    if token(bytes, &mut cursor)? != b"255"
        || !bytes.get(cursor).is_some_and(u8::is_ascii_whitespace)
    {
        return Err("Screenshot requires an 8-bit RGB PPM raster".into());
    }
    // P6 separates the raster with one whitespace byte; further whitespace may be pixel data.
    cursor += 1;
    let length = usize::try_from(width)?
        .checked_mul(usize::try_from(height)?)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or("PPM raster length overflow")?;
    if length > PPM_LIMIT || bytes.len().checked_sub(cursor) != Some(length) {
        return Err("Truncated, excessive or trailing PPM raster bytes".into());
    }
    Ok(Ppm {
        width,
        height,
        rgb: &bytes[cursor..],
    })
}

fn verify_pattern(bytes: &[u8]) -> Result<u64> {
    let ppm = parse_ppm(bytes)?;
    if ppm.width != WIDTH || ppm.height != HEIGHT {
        return Err(format!(
            "Expected {WIDTH}x{HEIGHT} scanout, got {}x{}",
            ppm.width, ppm.height
        )
        .into());
    }
    let mut pixels = vec![0; (WIDTH * HEIGHT) as usize];
    let mut reference = Surface::new(WIDTH, HEIGHT, &mut pixels)
        .map_err(|error| format!("Cannot create reference surface: {error:?}"))?;
    reference.draw_test_pattern();
    if reference.checksum() != REFERENCE_CHECKSUM {
        return Err("Software reference differs from the pinned 800x600 pattern checksum".into());
    }
    for (index, (rgb, expected)) in ppm.rgb.chunks_exact(3).zip(reference.pixels()).enumerate() {
        let actual = (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2]);
        if actual != *expected {
            return Err(format!(
                "Scanout pixel ({},{}) is {actual:#08x}, expected {expected:#08x}",
                index % WIDTH as usize,
                index / WIDTH as usize,
            )
            .into());
        }
    }
    Ok(reference.checksum())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};

    fn qmp_reply(reply: &[u8]) -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let temp = Temporary::new(root, "qmp-host-test")?;
        let (client, server) = UnixStream::pair()?;
        let reply = reply.to_vec();
        let server_thread = thread::spawn(move || {
            let mut server = BufReader::new(server);
            let mut command = String::new();
            server.read_line(&mut command).unwrap();
            let command: Value = serde_json::from_str(&command).unwrap();
            assert_eq!(command["id"], 1);
            assert_eq!(command["execute"], "test-command");
            // Invalid or overlong responses can close the client before this write completes.
            let _ = server.get_mut().write_all(&reply);
        });
        let mut qmp = Qmp {
            stream: client,
            log: File::create(temp.0.join("qmp.log"))?,
            id: 0,
        };
        let result = qmp.command("test-command", json!({}));
        drop(qmp);
        server_thread.join().unwrap();
        result
    }

    #[test]
    fn qmp_ignores_events_and_requires_matching_successful_transactions() {
        assert!(qmp_reply(b"{\"event\":\"RESET\"}\r\n{\"id\":1,\"return\":{}}\r\n").is_ok());
        for bytes in [
            &b"{\"return\":{},\"id\":2}\r\n"[..],
            b"{\"return\":{}}\r\n",
            b"{\"id\":1,\"error\":{\"desc\":\"capture failed\"}}\r\n",
            b"{\"id\":1}\r\n",
            b"[]\r\n",
            b"malformed\r\n",
            b"{\"id\":1,\"return\":{}}",
        ] {
            assert!(
                qmp_reply(bytes).is_err(),
                "Accepted bad QMP reply: {bytes:?}"
            );
        }
        assert!(qmp_reply(&vec![b' '; QMP_LIMIT + 1]).is_err());
    }

    #[test]
    fn ppm_header_comments_preserve_whitespace_and_hash_in_the_raster() {
        let bytes = b"P6\n# QEMU scanout\n1 1\n255\n\n# ";
        let ppm = parse_ppm(bytes).unwrap();
        assert_eq!((ppm.width, ppm.height), (1, 1));
        assert_eq!(ppm.rgb, b"\n# ");
    }

    #[test]
    fn ppm_rejects_formats_bounds_overflow_truncation_and_trailing_data() {
        for bytes in [
            &b"P3\n1 1\n255\n\0\0\0"[..],
            b"P6\n0 1\n255\n",
            b"P6\n-1 1\n255\n",
            b"P6\n4294967296 1\n255\n",
            b"P6\n4096 4096\n255\n",
            b"P6\n1 1\n65535\n\0\0\0\0\0\0",
            b"P6\n1 1\n255\n\0\0",
            b"P6\n1 1\n255\n\0\0\0\0",
            b"P6\n1 1\n255",
            b"P6\n# incomplete",
        ] {
            assert!(parse_ppm(bytes).is_err(), "Accepted invalid PPM: {bytes:?}");
        }
        assert!(parse_ppm(&vec![b' '; 1025]).is_err());
    }

    #[test]
    fn complete_scanout_gate_rejects_any_corrupted_pixel_and_wrong_dimensions() {
        let mut pixels = vec![0; (WIDTH * HEIGHT) as usize];
        let mut surface = Surface::new(WIDTH, HEIGHT, &mut pixels).unwrap();
        surface.draw_test_pattern();
        let checksum = surface.checksum();
        let mut bytes = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
        for pixel in surface.pixels() {
            bytes.extend_from_slice(&[(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
        }
        assert_eq!(verify_pattern(&bytes).unwrap(), checksum);
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert!(verify_pattern(&bytes).is_err());
        assert!(verify_pattern(b"P6\n1 1\n255\n\0\0\0").is_err());
    }

    #[test]
    fn graphics_gate_rejects_timeouts_faults_early_exits_and_fake_shutdown() {
        let clean = "[GRAPHICS:PRESENT]\nM14-SERIAL-RECOVERY\n[POWER:OFF] synced=true clean=true dma_reset=true\n";
        assert!(check_guest(clean, false).is_ok());
        assert!(check_guest(clean, true).is_err());
        for marker in ["[PANIC]", "[EXCEPTION]", "[INIT:FAIL]", "[POWER:FAIL]"] {
            assert!(check_guest(&format!("{clean}{marker}"), false).is_err());
        }
        assert!(check_exit(clean, Some(0), 3).is_ok());
        assert!(check_exit(clean, Some(33), 3).is_err());
        assert!(check_exit(clean, None, 3).is_err());
        assert!(check_exit(clean, Some(0), 2).is_err());
        assert!(check_exit("anova> ", Some(0), 3).is_err());
    }
}
