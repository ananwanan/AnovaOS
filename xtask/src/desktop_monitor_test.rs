// SPDX-License-Identifier: Apache-2.0
//! Observe real native resource pressure, task authority and graphical service recovery.
//! Compare mounted counts to independent e2fsprogs output and retain hardware input/scanout.

use super::*;
use desktop_files_test::{prompt, wait_line, wait_record};
use desktop_test::{Driver, field, last_line};
use pty_terminal_test::{type_command, type_text};

pub fn verify(root: &Path) -> Result<()> {
    let work = directory(root, "desktop-monitor-test")?.join(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
            .to_string(),
    );
    fs::create_dir(&work)?;
    fs::write(
        work.parent().unwrap().join("latest.txt"),
        format!("{}\n", work.display()),
    )?;
    let mut evidence = String::from("sample_period_ticks=50\ninteraction_budget_ms=5000\n");
    fs::write(work.join("result.txt"), "result=Running\n")?;
    let result = run(root, &work, &mut evidence);
    evidence.push_str(&match &result {
        Ok(()) => "result=Ok\n".into(),
        Err(error) => format!("result=Err\nerror={error}\n"),
    });
    fs::write(work.join("result.txt"), evidence)?;
    result
}

fn run(root: &Path, work: &Path, evidence: &mut String) -> Result<()> {
    let archive = desktop::prepare(root)?;
    assets::check(root)?;
    let kernel = build(root, Mode::DesktopTest)?;
    let iso = image_contents(root, &kernel, Some(&archive), "desktop-monitor-test")?;
    let disk = work.join("monitor.ext4");
    foundation_journal::create(root, &disk, false)?;
    let initial = header(&disk)?;
    let (mut d, vars) = Driver::boot(root, &iso, &disk, work, "journal")?;
    d.ready(0, 0)?;
    d.wait_since(0, "anova> ")?;
    let from = d.output.position();
    d.command("desktop monitor-check", 0)?;
    let sample = wait_line(&mut d, from, "[MONITOR:PASS]")?;
    check_disk(&sample, &initial, 4)?;
    evidence.push_str(&sample);

    d.click(35, 60)?;
    d.key("m")?;
    desktop_build_test::capture(
        &mut d,
        work,
        "resources",
        &[(36, "RESOURCES"), (64, "PAGES")],
        evidence,
    )?;
    d.key("2")?;
    desktop_build_test::capture(
        &mut d,
        work,
        "owned-tasks",
        &[(52, "OWNED TASKS")],
        evidence,
    )?;
    let pid = d.client_pid(0)?;
    // Rows sort by PID. Select launcher, compositor and the first application.
    for _ in 0..3 {
        d.key("down")?;
    }
    let from = d.output.position();
    d.key("s")?;
    wait_line(
        &mut d,
        from,
        &format!("[DESKTOP:TASK_STOP] pid={pid} result=Ok(true)"),
    )?;
    wait_line(
        &mut d,
        from,
        &format!("[DESKTOP:EXIT] app=0 pid={pid} reason=3 code=15"),
    )?;
    desktop_build_test::capture(
        &mut d,
        work,
        "task-stopped",
        &[(200, "TASK STOPPED")],
        evidence,
    )?;
    d.key("esc")?;
    let from = d.output.position();
    d.key("1")?;
    d.wait_since(from, "[GUI:C:READY]")?;
    d.click(35, 60)?;
    d.key("m")?;
    d.key("2")?;
    let from = d.output.position();
    d.key("s")?;
    wait_line(
        &mut d,
        from,
        &format!("[DESKTOP:TASK_STOP] pid={pid} result=Err(-5)"),
    )?;
    desktop_build_test::capture(
        &mut d,
        work,
        "stale-stop-denied",
        &[(200, "TASK STOP DENIED")],
        evidence,
    )?;
    d.key("esc")?;

    d.key("f6")?;
    d.wait("[FILES:READY]")?;
    d.click(290, 320)?;
    let from = d.output.position();
    const SAVED: &str = "saved\n";
    const DIRTY: &str = "dirty\n";
    prompt(&mut d, "f2", "/persist/monitor.txt")?;
    d.wait_since(from, "[FILES:NEW]")?;
    type_text(&mut d, SAVED)?;
    let from = d.output.position();
    d.key("f5")?;
    wait_record(
        &mut d,
        from,
        "[FILES:SAVED]",
        "/persist/monitor.txt",
        SAVED.len(),
    )?;
    type_text(&mut d, DIRTY)?;
    d.click(35, 60)?;
    d.key("t")?;
    d.wait("[TERMINAL:READY]")?;
    let terminal = field(last_line(&d.output.text(), "[TERMINAL:SHELL]")?, "session")?;
    d.click(35, 60)?;
    d.key("m")?;
    d.key("3")?;
    let from = d.output.position();
    d.key("r")?;
    wait_line(
        &mut d,
        from,
        "[DESKTOP:SERVICE] action=Close Files with F4 result=-24",
    )?;
    let from = d.output.position();
    d.key("c")?;
    reconnect(
        &mut d,
        from,
        1,
        terminal,
        SAVED.len() + DIRTY.len(),
        evidence,
    )?;

    let server = field(last_line(&d.output.text(), "[GUI:SERVER_READY]")?, "pid")?;
    d.faults.push(server);
    let from = d.output.position();
    d.key("f12")?;
    reconnect(
        &mut d,
        from,
        2,
        terminal,
        SAVED.len() + DIRTY.len(),
        evidence,
    )?;
    let fault = wait_line(&mut d, from, &format!("[USER:FAULT] pid={server} vector=6"))?;
    evidence.push_str(&fault);
    d.command("echo MONITOR-UART", 0)?;
    desktop_files_test::serial_exact(&mut d, "/persist/monitor.txt", SAVED)?;
    d.click(290, 320)?;
    let from = d.output.position();
    d.key("f5")?;
    wait_record(
        &mut d,
        from,
        "[FILES:SAVED]",
        "/persist/monitor.txt",
        SAVED.len() + DIRTY.len(),
    )?;
    desktop_files_test::serial_exact(&mut d, "/persist/monitor.txt", "saved\ndirty\n")?;
    d.click(470, 90)?;
    type_command(&mut d, "/linux/echo MONITOR-PTY")?;
    pty_terminal_test::screenshot(
        &mut d,
        work,
        "terminal-after-fault",
        &["MONITOR-PTY"],
        evidence,
    )?;
    d.key("f4")?;
    d.wait("[TERMINAL:DONE]")?;
    d.click(290, 320)?;
    d.key("f4")?;
    d.wait("[FILES:DONE]")?;
    d.click(35, 60)?;
    d.power_key()?;
    d.wait_poweroff()?;
    desktop_test::validate_resources(&d.output.text())?;
    evidence.push_str(last_line(&d.output.text(), "[M15:RESOURCES]")?);
    evidence.push('\n');
    vars.cleanup()?;
    foundation_journal::check(&disk)?;

    let ext2 = work.join("monitor.ext2");
    disk::create_profile(root, &ext2, false)?;
    let initial = header(&ext2)?;
    let (mut d, vars) = Driver::boot(root, &iso, &ext2, work, "ext2")?;
    d.ready(0, 0)?;
    d.wait_since(0, "anova> ")?;
    let from = d.output.position();
    d.command("desktop monitor-check", 0)?;
    let sample = wait_line(&mut d, from, "[MONITOR:PASS]")?;
    check_disk(&sample, &initial, 2)?;
    evidence.push_str(&sample);
    d.shutdown()?;
    vars.cleanup()?;
    foundation_journal::check(&ext2)
}

fn reconnect(
    d: &mut Driver,
    from: usize,
    generation: usize,
    terminal: u64,
    bytes: usize,
    evidence: &mut String,
) -> Result<()> {
    let ready = wait_line(d, from, &format!("[DESKTOP:READY] generation={generation}"))?;
    let files = wait_line(d, from, "[FILES:RECONNECTED]")?;
    let pty = wait_line(d, from, "[TERMINAL:RECONNECTED]")?;
    let session = field(&ready, "session")?;
    if !files.contains("dirty=true")
        || field(&files, "bytes")? != bytes as u64
        || field(&files, "old")? >= session
        || field(&files, "session")? != session
        || field(&pty, "session")? != session
        || field(last_line(&d.output.text(), "[TERMINAL:SHELL]")?, "session")? != terminal
    {
        return Err("Compositor replacement changed dirty bytes or PTY identity".into());
    }
    evidence.push_str(&ready);
    evidence.push_str(&files);
    evidence.push_str(&pty);
    Ok(())
}

fn header(disk: &Path) -> Result<String> {
    let output = Command::new("dumpe2fs").arg("-h").arg(disk).output()?;
    if !output.status.success() {
        return Err("Could not independently read work-disk counters".into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn check_disk(sample: &str, header: &str, format: u64) -> Result<()> {
    if field(sample, "disk")? != format {
        return Err("Incorrect mounted disk format".into());
    }
    for (wire, label) in [
        ("blocks", "Block count:"),
        ("free_blocks", "Free blocks:"),
        ("inodes", "Inode count:"),
        ("free_inodes", "Free inodes:"),
    ] {
        let expected: u64 = header
            .lines()
            .find_map(|line| line.strip_prefix(label))
            .ok_or("Missing independent disk counter")?
            .trim()
            .parse()?;
        if field(sample, wire)? != expected {
            return Err(format!("Mounted {wire} differs from independent count {expected}").into());
        }
    }
    Ok(())
}
