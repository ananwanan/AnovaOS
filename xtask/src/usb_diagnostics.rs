// SPDX-License-Identifier: Apache-2.0
//! Verify actual no-input USB boot diagnostics and their removal after live HID attachment.
//! Inspect independent scanout glyphs using the production image; never inject hardware faults.
use super::*;
use desktop_test::Driver;
use graphics::read_ppm;
use serde_json::json;
use sha2::{Digest, Sha256};

pub fn verify(root: &Path) -> Result<()> {
    let work = directory(root, "usb-diagnostics-test")?;
    usb_input::begin_attempt(&work)?;
    let mut evidence = String::from("interaction_budget_ms=5000\nphysical_hardware=pending\n");
    let result = (|| -> Result<()> {
        let iso = desktop::image_mode(root, Mode::UsbDesktop)?;
        let diagnostic = directory(root, "images")?.join("anova-usb-diagnostics.iso");
        fs::copy(&iso, &diagnostic)?;
        evidence.push_str(&format!(
            "iso_sha256={:x}\n",
            Sha256::digest(fs::read(&iso)?)
        ));
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        for (name, options, expected) in [
            ("absent", vec!["-machine", "i8042=off"], "ABSENT"),
            (
                "empty",
                vec!["-machine", "i8042=off", "-device", "qemu-xhci,id=xhci"],
                "CONNECTED=0",
            ),
            (
                "rejected",
                vec![
                    "-machine",
                    "i8042=off",
                    "-device",
                    "qemu-xhci,id=xhci",
                    "-device",
                    "usb-hub,bus=xhci.0",
                ],
                "HUB-DESCRIPTOR-PORTS-LENGTH",
            ),
        ] {
            let disk =
                directory(root, "disks")?.join(format!("usb-diagnostics-{name}-{stamp}.ext4"));
            foundation_journal::create(root, &disk, true)?;
            let (mut d, vars) =
                Driver::boot_input(root, &diagnostic, &disk, &work, name, &options)?;
            d.wait("anova> ")?;
            d.ready(0, 0)?;
            d.wait_since(0, "[GUI:NO_INPUT]")?;
            screenshot(&mut d, &work, name, Some(expected), &mut evidence)?;
            if name == "empty" {
                for (driver, port) in [("usb-mouse", "2"), ("usb-kbd", "1")] {
                    let from = d.output.position();
                    d.qmp.command(
                        "device_add",
                        json!({"driver":driver,"bus":"xhci.0","port":port}),
                    )?;
                    d.wait_since(from, "[USB:HID]")?;
                }
                d.click(35, 60)?;
                let from = d.output.position();
                d.key("t")?;
                d.wait_since(from, "[TERMINAL:PRESENT]")?;
                screenshot(&mut d, &work, "attached", None, &mut evidence)?;
            }
            d.shutdown()?;
            foundation_journal::check(&disk)?;
            vars.cleanup()?;
            evidence.push_str(&format!("{name}=passed\n"));
        }
        Ok(())
    })();
    evidence.push_str(&format!("result={result:?}\n"));
    fs::write(work.join("result.txt"), evidence)?;
    result
}

fn screenshot(
    d: &mut Driver,
    work: &Path,
    name: &str,
    expected: Option<&str>,
    evidence: &mut String,
) -> Result<()> {
    let path = work.join(format!("{name}.ppm"));
    output_path(&path)?;
    let start = Instant::now();
    loop {
        d.poll()?;
        d.qmp
            .command("screendump", json!({"filename":path,"format":"ppm"}))?;
        let bytes = read_ppm(&path)?;
        let image = graphics::parse_ppm(&bytes)?;
        if image.width != 800 || image.height != 600 {
            return Err("USB diagnostic display dimensions differ".into());
        }
        let title = contains(image.rgb, "USB BOOT DIAGNOSTICS", &[448]);
        let rows: Vec<_> = (0..12).map(|n| 474 + n * 10).collect();
        let valid = expected.map_or(!title, |text| title && contains(image.rgb, text, &rows));
        if valid {
            evidence.push_str(&format!(
                "{name}_scanout_sha256={:x}\n",
                Sha256::digest(bytes)
            ));
            return Ok(());
        }
        if start.elapsed() >= Duration::from_secs(5) {
            return Err(format!("USB diagnostic scanout missing or stale: {name}").into());
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn contains(rgb: &[u8], text: &str, rows: &[usize]) -> bool {
    rows.iter().any(|&y| {
        (0..=126 - text.len()).any(|column| {
            text.bytes().enumerate().all(|(index, byte)| {
                pty_terminal_test::glyph(byte)
                    .iter()
                    .enumerate()
                    .all(|(dy, &bits)| {
                        (0..5).all(|dx| {
                            let at = ((y + dy) * 800 + 8 + (column + index) * 6 + dx) * 3;
                            let expected = if bits & (1 << (4 - dx)) != 0 {
                                [255; 3]
                            } else {
                                [0x10, 0x20, 0x30]
                            };
                            rgb[at..at + 3] == expected
                        })
                    })
            })
        })
    })
}
