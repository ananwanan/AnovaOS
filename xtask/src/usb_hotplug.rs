// SPDX-License-Identifier: Apache-2.0
//! Verify live USB attachment, held-input removal, slot reuse and nested hub input using QMP.
//! Exercise actual EP0 STALL and controller restart in an isolated fault-injection image.
//! Independent UART, scanout, exact persisted PTY bytes and clean shutdown are mandatory.
use super::*;
use desktop_test::Driver;
use serde_json::json;
use sha2::{Digest, Sha256};

pub fn verify(root: &Path) -> Result<()> {
    let work = directory(root, "usb-hotplug-test")?;
    usb_input::begin_attempt(&work)?;
    for name in ["root-empty", "nested-hub"] {
        fs::write(work.join(format!("{name}.result.txt")), "result=Pending\n")?;
    }
    let mut evidence = String::from(
        "interaction_budget_ms=5000\nattach_budget_ms=5000\nphysical_hardware=pending\n",
    );
    let outcome = (|| -> Result<()> {
        let archive = desktop::prepare(root)?;
        let kernel = build(root, Mode::UsbTestDesktop)?;
        let iso = image_contents(root, &kernel, Some(&archive), "usb-hotplug")?;
        evidence.push_str(&format!(
            "iso_sha256={:x}\n",
            Sha256::digest(fs::read(&iso)?)
        ));
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        for (name, hubs) in [("root-empty", false), ("nested-hub", true)] {
            fs::write(work.join(format!("{name}.result.txt")), "result=Running\n")?;
            let result = (|| -> Result<()> {
                let disk =
                    directory(root, "disks")?.join(format!("usb-hotplug-{name}-{stamp}.ext4"));
                foundation_journal::create(root, &disk, true)?;
                let options: &[&str] = if hubs {
                    &[
                        "-machine",
                        "i8042=off",
                        "-device",
                        "qemu-xhci,id=xhci",
                        "-device",
                        "usb-hub,id=hub,bus=xhci.0,port=1,ports=4,port-power=on",
                        "-device",
                        "usb-hub,id=nested,bus=xhci.0,port=1.1,ports=4,port-power=on",
                        "-device",
                        "usb-kbd,id=kbd,bus=xhci.0,port=1.1.1,usb_version=1",
                        "-device",
                        "usb-mouse,id=mouse,bus=xhci.0,port=1.2,usb_version=1",
                    ]
                } else {
                    &["-machine", "i8042=off", "-device", "qemu-xhci,id=xhci"]
                };
                let (mut d, vars) = Driver::boot_input(root, &iso, &disk, &work, name, options)?;
                d.wait("anova> ")?;
                d.ready(0, 0)?;
                if d.output.text().contains("[USB:REJECT]") {
                    return Err("USB controller rejected the hotplug topology".into());
                }
                if hubs {
                    d.wait_since(0, "route=0x11 depth=2")?;
                } else {
                    d.wait_since(0, "[GUI:NO_INPUT]")?;
                    add(&mut d, "usb-mouse", "mouse", "2", false)?;
                    add(&mut d, "usb-kbd", "kbd", "1", false)?;
                }
                d.click(35, 60)?;
                let from = d.output.position();
                d.key("t")?;
                d.wait_since(from, "[TERMINAL:READY]")?;
                d.wait_since(from, "[TERMINAL:SHELL]")?;
                d.wait_since(from, "[TERMINAL:PRESENT]")?;
                d.click(470, 90)?;
                usb_input::type_command(&mut d, "echo before > /persist/hotplug-before.txt")?;
                for round in 0..10 {
                    usb_input::ack_key(&mut d, "shift", 42, true)?;
                    let from = d.output.position();
                    let start = Instant::now();
                    d.qmp.command("device_del", json!({"id":"kbd"}))?;
                    d.wait_since(from, "[USB:KEY] code=42 state=Released modifiers=0")?;
                    d.wait_since(from, "[USB:DETACH]")?;
                    if start.elapsed() > Duration::from_secs(5) {
                        return Err("Held-key detach exceeded interaction budget".into());
                    }
                    d.command("echo USB-HOTPLUG-UART", 0)?;
                    d.move_to(480 + round, 90)?;
                    add(
                        &mut d,
                        "usb-kbd",
                        "kbd",
                        if hubs { "1.1.1" } else { "1" },
                        hubs,
                    )?;
                }
                d.button(true)?;
                let from = d.output.position();
                d.qmp.command("device_del", json!({"id":"mouse"}))?;
                d.wait_since(from, "[GUI:POINTER] x=489 y=90 buttons=0")?;
                d.wait_since(from, "[USB:DETACH]")?;
                // Keyboard and the existing PTY remain usable while the pointer is absent.
                usb_input::type_command(&mut d, "echo after > /persist/hotplug-after.txt")?;
                add(
                    &mut d,
                    "usb-mouse",
                    "mouse",
                    if hubs { "1.2" } else { "2" },
                    hubs,
                )?;
                // Driver's cached button state still reflects the deliberate held press.
                d.button(true)?;
                d.button(false)?;
                if hubs {
                    // Removing the upstream hub also removes its modeled descendants.
                    // The guest must retire the entire subtree before any new slot reuse.
                    usb_input::ack_key(&mut d, "shift", 42, true)?;
                    d.button(true)?;
                    let from = d.output.position();
                    let start = Instant::now();
                    d.qmp.command("device_del", json!({"id":"hub"}))?;
                    d.wait_since(from, "[USB:KEY] code=42 state=Released modifiers=0")?;
                    d.wait_since(from, "[GUI:POINTER] x=489 y=90 buttons=0")?;
                    d.wait_since(from, "dma_released=true devices=0")?;
                    d.wait_since(from, "[GUI:NO_INPUT]")?;
                    d.command("echo HUB-DETACHED-UART", 0)?;
                    if start.elapsed() > Duration::from_secs(5) {
                        return Err("Hub subtree retirement exceeded five seconds".into());
                    }
                    let from = d.output.position();
                    let start = Instant::now();
                    for (id, port) in [("hub", "1"), ("nested", "1.1")] {
                        d.qmp.command("device_add", json!({"driver":"usb-hub","id":id,"bus":"xhci.0","port":port,"ports":4,"port-power":true}))?;
                    }
                    for (driver, id, port) in
                        [("usb-kbd", "kbd", "1.1.1"), ("usb-mouse", "mouse", "1.2")]
                    {
                        d.qmp.command("device_add", json!({"driver":driver,"id":id,"bus":"xhci.0","port":port,"usb_version":1}))?;
                    }
                    d.wait_since(from, "route=0x11 depth=2")?;
                    d.wait("[USB:HID]")?;
                    if start.elapsed() > Duration::from_secs(5) {
                        return Err("Hub subtree attachment exceeded five seconds".into());
                    }
                    d.button(true)?;
                    d.button(false)?;
                    let from = d.output.position();
                    d.qmp.command("device_del", json!({"id":"kbd"}))?;
                    d.wait_since(from, "[USB:DETACH]")?;
                    let from = d.output.position();
                    d.qmp.command("device_del", json!({"id":"nested"}))?;
                    d.wait_since(from, "[USB:DETACH]")?;
                    add(&mut d, "usb-kbd", "kbd", "1.3", true)?;
                }
                usb_input::type_command(&mut d, "/linux/cat /persist/hotplug-after.txt")?;
                pty_terminal_test::screenshot(
                    &mut d,
                    &work,
                    &format!("{name}-after"),
                    &["AFTER"],
                    &mut evidence,
                )?;
                d.command("echo USB-FINAL-UART", 0)?;
                d.shutdown()?;
                foundation_journal::check(&disk)?;
                for (file, expected) in [
                    ("before", b"before\n".as_slice()),
                    ("after", b"after\n".as_slice()),
                ] {
                    let output = work.join(format!("{name}-{file}.bin"));
                    command(
                        Command::new("debugfs")
                            .args([
                                "-R",
                                &format!("dump /hotplug-{file}.txt {}", output.display()),
                            ])
                            .arg(&disk),
                    )?;
                    if fs::read(output)? != expected {
                        return Err("Hotplug PTY bytes differ".into());
                    }
                }
                let text = d.output.text();
                validate_lifetimes(&text)?;
                if [
                    "[USB:QUARANTINE]",
                    "[USB:RUNTIME_STOP]",
                    "[USB:REJECT]",
                    "[USB:ATTACH_REJECT]",
                    "[USB:HUB_PORT_REJECT]",
                    "[USB:INPUT_ERROR]",
                ]
                .iter()
                .any(|m| text.contains(m))
                {
                    return Err("Hotplug run reported a USB failure".into());
                }
                evidence.push_str(&format!("{name}_cycles=10\n{name}_held_key_release=true\n{name}_held_button_release=true\n{name}_uart=true\n{name}_pty_bytes=exact\n"));
                vars.cleanup()?;
                Ok(())
            })();
            fs::write(
                work.join(format!("{name}.result.txt")),
                format!("result={result:?}\n"),
            )?;
            result?;
        }
        Ok(())
    })();
    evidence.push_str(&format!("result={outcome:?}\n"));
    fs::write(work.join("result.txt"), evidence)?;
    outcome
}

fn add(d: &mut Driver, driver: &str, id: &str, port: &str, fullspeed: bool) -> Result<()> {
    let from = d.output.position();
    let start = Instant::now();
    let mut arguments = json!({"driver":driver,"id":id,"bus":"xhci.0","port":port});
    if fullspeed {
        arguments["usb_version"] = json!(1);
    }
    d.qmp.command("device_add", arguments)?;
    d.wait_since(from, "[USB:HID]")?;
    if start.elapsed() > Duration::from_secs(5) {
        return Err("USB attachment exceeded five seconds".into());
    }
    Ok(())
}

fn validate_lifetimes(text: &str) -> Result<()> {
    let mut live = std::collections::BTreeMap::new();
    let mut last = 0;
    let mut retired = 0;
    for line in text.lines() {
        if !line.starts_with("[USB:ATTACH]") && !line.starts_with("[USB:DETACH]") {
            continue;
        }
        let field = |name: &str| -> Result<u64> {
            line.split_whitespace()
                .find_map(|part| part.strip_prefix(name))
                .ok_or("Missing USB lifetime field")?
                .parse()
                .map_err(Into::into)
        };
        let slot = field("slot=")?;
        let generation = field("generation=")?;
        if !(1..=8).contains(&slot) {
            return Err("USB lifetime slot out of range".into());
        }
        if line.starts_with("[USB:ATTACH]") {
            if generation <= last || live.insert(slot, generation).is_some() {
                return Err("USB lifetime reused an active slot or generation".into());
            }
            last = generation;
        } else {
            if live.remove(&slot) != Some(generation)
                || !line.contains("dma_released=true")
                || !(3..=7).contains(&field("pages=")?)
                || field("devices=")? != live.len() as u64
            {
                return Err("USB lifetime reclamation evidence differs".into());
            }
            retired += 1;
        }
    }
    if retired < 11 || last <= 8 {
        return Err("USB hotplug did not exceed the slot capacity through reuse".into());
    }
    Ok(())
}

pub fn recovery(root: &Path) -> Result<()> {
    let work = directory(root, "usb-recovery-test")?;
    usb_input::begin_attempt(&work)?;
    let mut evidence = String::from("interaction_budget_ms=5000\ncontroller_restart_limit=1\n");
    let outcome = (|| -> Result<()> {
        let archive = desktop::prepare(root)?;
        let kernel = build(root, Mode::UsbRecoveryDesktop)?;
        let iso = image_contents(root, &kernel, Some(&archive), "usb-recovery")?;
        evidence.push_str(&format!(
            "iso_sha256={:x}\n",
            Sha256::digest(fs::read(&iso)?)
        ));
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let disk = directory(root, "disks")?.join(format!("usb-recovery-{stamp}.ext4"));
        foundation_journal::create(root, &disk, true)?;
        let (mut d, vars) = Driver::boot_input(
            root,
            &iso,
            &disk,
            &work,
            "recovery",
            &[
                "-machine",
                "i8042=off",
                "-device",
                "qemu-xhci,id=xhci",
                "-device",
                "usb-kbd,bus=xhci.0",
                "-device",
                "usb-mouse,bus=xhci.0",
            ],
        )?;
        d.wait("anova> ")?;
        d.ready(0, 0)?;
        d.click(35, 60)?;
        let from = d.output.position();
        d.key("t")?;
        d.wait_since(from, "[TERMINAL:PRESENT]")?;
        d.click(470, 90)?;
        // QEMU 8.2's default eight-port hub returns a truncated power-mask bitmap.
        // Reject it through actual control transfers while keeping the root pair usable.
        let from = d.output.position();
        d.qmp.command(
            "device_add",
            json!({"driver":"usb-hub","id":"bad-hub","bus":"xhci.0","port":"3"}),
        )?;
        d.wait_since(from, "reason=hub-descriptor-ports-length")?;
        d.wait_since(from, "dma_released=true devices=2")?;
        d.command("echo REJECTED-HUB-UART", 0)?;
        let from = d.output.position();
        let start = Instant::now();
        d.key("f12")?;
        d.wait_since(
            from,
            "[USB:RECOVERY_TEST] ep0_stall=true descriptor_retry=true",
        )?;
        if start.elapsed() > Duration::from_secs(5) {
            return Err("EP0 recovery exceeded five seconds".into());
        }
        usb_input::type_command(&mut d, "echo control > /persist/control.txt")?;
        let from = d.output.position();
        let start = Instant::now();
        d.key("f12")?;
        d.command("echo RECOVERY-UART", 0)?;
        d.wait_since(from, "[USB:RESTART] attempt=1 result=Ok(())")?;
        if start.elapsed() > Duration::from_secs(5) {
            return Err("Controller recovery exceeded five seconds".into());
        }
        usb_input::type_command(&mut d, "echo restart > /persist/restart.txt")?;
        usb_input::type_command(&mut d, "/linux/cat /persist/restart.txt")?;
        d.move_to(490, 90)?;
        pty_terminal_test::screenshot(&mut d, &work, "recovered", &["RESTART"], &mut evidence)?;
        d.shutdown()?;
        foundation_journal::check(&disk)?;
        for (file, expected) in [
            ("control", b"control\n".as_slice()),
            ("restart", b"restart\n".as_slice()),
        ] {
            let saved = work.join(format!("{file}.bin"));
            command(
                Command::new("debugfs")
                    .args(["-R", &format!("dump /{file}.txt {}", saved.display())])
                    .arg(&disk),
            )?;
            if fs::read(saved)? != expected {
                return Err("Recovery PTY bytes differ".into());
            }
        }
        let text = d.output.text();
        if text.matches("[USB:RESTART]").count() != 1
            || text.contains("[USB:QUARANTINE]")
            || !text.contains("code=6")
        {
            return Err("USB recovery evidence differs".into());
        }
        evidence.push_str("ep0_stall=actual\ndescriptor_retry=actual\ncontroller_halt=actual\nrestart=1\npty_bytes=exact\nuart=true\n");
        vars.cleanup()?;
        Ok(())
    })();
    evidence.push_str(&format!("result={outcome:?}\n"));
    fs::write(work.join("result.txt"), evidence)?;
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifetime_gate_rejects_stale_identity_missing_reclamation_and_slot_leaks() {
        let mut text = String::new();
        for generation in 1..=12 {
            text.push_str(&format!("[USB:ATTACH] slot=1 generation={generation}\n[USB:DETACH] slot=1 generation={generation} dma_released=true devices=0 pages=4\n"));
        }
        assert!(validate_lifetimes(&text).is_ok());
        for (old, new) in [
            ("dma_released=true", "dma_released=false"),
            ("devices=0", "devices=1"),
            ("pages=4", "pages=0"),
            ("slot=1", "slot=9"),
            ("generation=12", "generation=1"),
        ] {
            assert!(validate_lifetimes(&text.replace(old, new)).is_err());
        }
        assert!(validate_lifetimes("").is_err());
        assert!(
            validate_lifetimes(&text.replace(
                "[USB:DETACH] slot=1 generation=5 dma_released=true devices=0 pages=4\n",
                ""
            ))
            .is_err()
        );
    }
}
