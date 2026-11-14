// SPDX-License-Identifier: Apache-2.0
//! Verify three fixed guest compiler recipes via physical input, live peer scanout and a fresh reboot.
//! Save original QMP/UART logs, exact disk dumps and all bounded result observations.

use super::*;
use anova_abi::workflow::Recipe;
use desktop_files_device::Fault;
use desktop_files_test::{prompt, wait_record};
use desktop_test::{Driver, field, last_line};
use graphics::{parse_ppm, read_ppm};
use pty_terminal_test::{type_command, type_text};
use serde_json::json;
use sha2::{Digest, Sha256};

const INTERACTION: Duration = Duration::from_secs(5);
const C_DEADLINE: Duration = Duration::from_secs(35);
const CPP_RUST_DEADLINE: Duration = Duration::from_secs(120);
const WATCHDOG: Duration = Duration::from_secs(600);
const OBSERVER: Duration = Duration::from_millis(500);
const BROKEN: &str = "???\n";
const C: &str = "#include <stdio.h>\nint main() { puts(\"C-ANSWER-42\"); return 7; }\n";
const C_ALTERNATE: &str = "#include <stdio.h>\nint main() { puts(\"UNPUBLISHED\"); return 19; }\n";
const CPP: &str = "#include <vector>\n#include <string>\n#include <cstdio>\nint sum();\nstruct A { ~A() { puts(\"RAII\"); } };\nint main() { A a; std::string s=\"CPP\"; std::vector<int> v={sum(),2};\nprintf(\"%s-%d\\n\",s.c_str(),v[0]+v[1]); return 8; }\n";
const HELPER: &str = "int sum() { return 40; }\n";
const RUST: &str = "fn main() { let v=vec![40,2]; println!(\"RUST-{}\",v.iter().sum::<i32>()); std::process::exit(9); }\n";

struct Profile {
    recipe: Recipe,
    name: &'static str,
    key: &'static str,
    sources: &'static [(&'static str, &'static str)],
    output: &'static [&'static str],
    build_budget: Duration,
}

#[derive(Clone, Copy)]
enum Peer {
    Shell,
    Pressure,
}

impl Peer {
    fn echo(self) -> &'static str {
        match self {
            Self::Shell => "PTY$ A",
            Self::Pressure => "HOLD A",
        }
    }
}

const PUBLISH_FAULTS: [(&str, Fault); 2] = [
    ("publish-write", Fault::WriteAfterFlush(6)),
    ("publish-flush", Fault::Flush(7)),
];

const PROFILES: [Profile; 3] = [
    Profile {
        recipe: Recipe::C,
        name: "c",
        key: "g",
        sources: &[("/persist/build.c", C)],
        output: &["C-ANSWER-42", "RUN-7"],
        build_budget: C_DEADLINE,
    },
    Profile {
        recipe: Recipe::Cpp,
        name: "cpp",
        key: "c",
        sources: &[("/persist/build.cpp", CPP), ("/persist/helper.cpp", HELPER)],
        output: &["CPP-42", "RAII", "RUN-8"],
        build_budget: CPP_RUST_DEADLINE,
    },
    Profile {
        recipe: Recipe::Rust,
        name: "rust",
        key: "r",
        sources: &[("/persist/build.rs", RUST)],
        output: &["RUST-42", "RUN-9"],
        build_budget: CPP_RUST_DEADLINE,
    },
];

pub fn verify(root: &Path, args: &[String]) -> Result<()> {
    let only = match args {
        [] => None,
        [flag, name] if flag == "--case" && PROFILES.iter().any(|p| p.name == name) => Some(name),
        _ => return Err("Usage: cargo xtask desktop-workflow-test [--case c|cpp|rust]".into()),
    };
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let base = directory(root, "desktop-workflow-test")?;
    let work = base.join(stamp.to_string());
    fs::create_dir(&work)?;
    fs::write(base.join("latest.txt"), format!("{}\n", work.display()))?;
    let mut evidence = format!(
        "profile=q35,TCG,1BSP,3GiB,800x600,PS2,JBD2\nwatchdog_ms={}\nc_build_ms={}\ncpp_rust_build_ms={}\ninteraction_ms={}\nbuild_observer_ms={}\n",
        WATCHDOG.as_millis(),
        C_DEADLINE.as_millis(),
        CPP_RUST_DEADLINE.as_millis(),
        INTERACTION.as_millis(),
        OBSERVER.as_millis(),
    );
    fs::write(work.join("result.txt"), &evidence)?;
    let result = (|| {
        let iso = if only.is_some_and(|name| name != "rust") {
            pty_terminal::image(root, true)?
        } else {
            pty_terminal::development_image(root)?
        };
        evidence.push_str(&format!(
            "iso_sha256={:x}\nqemu={}\n",
            Sha256::digest(fs::read(&iso)?),
            version("qemu-system-x86_64", &["--version"])?
                .lines()
                .next()
                .unwrap_or("?"),
        ));
        for profile in &PROFILES {
            if only.is_none_or(|name| name == profile.name) {
                let case = work.join(profile.name);
                fs::create_dir(&case)?;
                let result = test_recipe(root, &iso, &case, profile, &mut evidence);
                fs::write(case.join("result.txt"), &evidence)?;
                result?;
            }
        }
        if only.is_none_or(|name| name == "c") {
            for scenario in PUBLISH_FAULTS {
                let (name, _) = scenario;
                let case = work.join(name);
                fs::create_dir(&case)?;
                let result =
                    publish_fault(root, &iso, &work.join("c"), &case, scenario, &mut evidence);
                fs::write(case.join("result.txt"), &evidence)?;
                result?;
            }
        }
        Ok(())
    })();
    evidence.push_str(&match &result {
        Ok(()) => "result=Ok\n".into(),
        Err(err) => format!("result=Err\nerror={err}\n"),
    });
    fs::write(work.join("result.txt"), evidence)?;
    result
}

fn test_recipe(
    root: &Path,
    iso: &Path,
    work: &Path,
    p: &Profile,
    evidence: &mut String,
) -> Result<()> {
    let disk = work.join("work.ext4");
    foundation_journal::create(root, &disk, true)?;
    let (mut d, vars) =
        Driver::boot_with_watchdog(root, iso, &disk, work, "development", WATCHDOG)?;
    d.ready(0, 0)?;
    d.wait_since(0, "anova> ")?;
    // A tool version probe through the independent console observes the guest seed.
    let compiler = p.recipe.args()[0];
    evidence.push_str(&format!("\n[{}]\n", p.name));
    let from = d.output.position();
    d.command(&format!("{compiler} --version"), 0)?;
    let version = match p.recipe {
        Recipe::Rust => "rustc 1.90.0",
        Recipe::C | Recipe::Cpp => "11.2.0",
    };
    evidence.push_str(last_line(&d.output.text()[from..], version)?);
    evidence.push('\n');
    files(&mut d)?;
    for &(path, bytes) in p.sources {
        create_source(&mut d, path, bytes)?;
    }
    close_files(&mut d)?;
    terminal(&mut d)?;
    let i = start(&mut d, p, 1, evidence)?;
    live_peer(&mut d, work, &i, "first", Peer::Shell, evidence)?;
    let first = finish(&mut d, &i, p.build_budget, evidence)?;
    status(&first, 0, 0, true)?;
    run_artifact(&mut d, p, work, "first-run", evidence)?;
    let original = observe_disk(&mut d, &disk, work, p.recipe.output(), "first")?;
    anova_elf::parse_linux(&original).map_err(|e| format!("Guest output ELF: {e:?}"))?;

    files(&mut d)?;
    let (path, good) = p.sources[0];
    let bad = format!("{good}{BROKEN}");
    let from = d.output.position();
    prompt(&mut d, "f1", path)?;
    wait_record(&mut d, from, "[FILES:OPEN]", path, good.len())?;
    for _ in 0..good.lines().count() {
        d.key("down")?;
    }
    d.key("end")?;
    type_text(&mut d, BROKEN)?;
    save(&mut d, path, &bad)?;
    close_files(&mut d)?;
    let i = start(&mut d, p, 2, evidence)?;
    live_peer(&mut d, work, &i, "error", Peer::Shell, evidence)?;
    let failed = finish(&mut d, &i, p.build_budget, evidence)?;
    status(&failed, 0, 1, false)?;
    d.click(35, 60)?;
    desktop_build_test::capture(
        &mut d,
        work,
        "real-diagnostic",
        &[(84, "CODE=1"), (0, "ERROR")],
        evidence,
    )?;
    if observe_disk(&mut d, &disk, work, p.recipe.output(), "failed")? != original {
        return Err("Compiler error changed the previous ELF".into());
    }
    d.key("esc")?;

    files(&mut d)?;
    let from = d.output.position();
    prompt(&mut d, "f1", path)?;
    wait_record(&mut d, from, "[FILES:OPEN]", path, bad.len())?;
    for _ in 0..bad.lines().count() {
        d.key("down")?;
    }
    d.key("end")?;
    for _ in 0..BROKEN.len() {
        d.key("backspace")?;
    }
    save(&mut d, path, good)?;
    close_files(&mut d)?;
    let i = start(&mut d, p, 3, evidence)?;
    live_peer(&mut d, work, &i, "repair", Peer::Shell, evidence)?;
    let repaired = finish(&mut d, &i, p.build_budget, evidence)?;
    status(&repaired, 0, 0, true)?;
    run_artifact(&mut d, p, work, "repaired-run", evidence)?;
    let restored = observe_disk(&mut d, &disk, work, p.recipe.output(), "repaired")?;

    // Cancelling uses the exact current task identity and leaves the published ELF intact.
    let build = start(&mut d, p, 4, evidence)?;
    // The C fixture can finish before a focus round-trip. Use the longer actual
    // compiler jobs to observe the peer both before cancellation and after it.
    if p.recipe != Recipe::C {
        live_peer(&mut d, work, &build, "before-cancel", Peer::Shell, evidence)?;
        d.click(35, 60)?;
    }
    require_compiler_live(&mut d, &build)?;
    let from = d.output.position();
    let cancelled_at = Instant::now();
    d.key("x")?;
    d.require_live(
        from,
        &format!("[BUILD:CANCEL] id={} result=Ok(())", build.pid),
        cancelled_at + INTERACTION,
    )?;
    evidence.push_str(&format!(
        "task={} cancel_ack_ms={}\n",
        build.pid,
        cancelled_at.elapsed().as_millis()
    ));
    live_peer(
        &mut d,
        work,
        &build,
        "cancel-recovery",
        Peer::Shell,
        evidence,
    )?;
    let cancelled = finish(&mut d, &build, p.build_budget, evidence)?;
    if !cancelled.contains("cancelled=true") {
        return Err("The exact recipe attempt did not record cancellation".into());
    }
    status(&cancelled, 3, 15, false)?;
    if observe_disk(&mut d, &disk, work, p.recipe.output(), "cancelled")? != restored {
        return Err("Cancelled build changed the previous ELF".into());
    }
    let mut records = vec![first, failed, repaired, cancelled];
    if p.recipe == Recipe::C {
        d.click(470, 90)?;
        type_command(&mut d, "/linux/pty-fixture pressure")?;
        pty_terminal_test::screenshot(
            &mut d,
            work,
            "pressure-start",
            &["PRESSURE-18", "HOLD"],
            evidence,
        )?;
        let build = start(&mut d, p, 5, evidence)?;
        live_peer(&mut d, work, &build, "pressure", Peer::Pressure, evidence)?;
        let end = finish(&mut d, &build, p.build_budget, evidence)?;
        status(&end, 0, 0, true)?;
        if observe_disk(&mut d, &disk, work, p.recipe.output(), "pressure")? != restored {
            return Err("Compiler under task pressure changed the expected ELF".into());
        }
        records.push(end);
        d.click(470, 90)?;
        d.key("ret")?;
        pty_terminal_test::screenshot(
            &mut d,
            work,
            "pressure-release",
            &["PRESSURE-FREE"],
            evidence,
        )?;
    }
    for job in &records {
        let id = field(job, "id")?;
        let uart = d.output.text();
        let closed = last_line(&uart, &format!("session={id} descendants_reaped=true"))?;
        evidence.push_str(closed);
        evidence.push('\n');
    }
    verify_staging_is_empty(&mut d, p.recipe)?;
    d.click(35, 60)?;
    d.key("esc")?;
    poweroff(&mut d)?;
    desktop_build_test::resources(&d.output.text(), evidence)?;
    vars.cleanup()?;
    foundation_journal::check(&disk)?;
    verify_disk(&disk, work, p, &restored, "shutdown", evidence)?;

    let (mut d, vars) = Driver::boot_with_watchdog(root, iso, &disk, work, "reboot", WATCHDOG)?;
    d.ready(0, 0)?;
    files(&mut d)?;
    for &(path, source) in p.sources {
        let from = d.output.position();
        prompt(&mut d, "f1", path)?;
        wait_record(&mut d, from, "[FILES:OPEN]", path, source.len())?;
    }
    close_files(&mut d)?;
    terminal(&mut d)?;
    run_artifact(&mut d, p, work, "reboot-run", evidence)?;
    poweroff(&mut d)?;
    vars.cleanup()?;
    foundation_journal::check(&disk)?;
    verify_disk(&disk, work, p, &restored, "reboot", evidence)?;
    Ok(())
}

fn publish_fault(
    root: &Path,
    iso: &Path,
    baseline: &Path,
    work: &Path,
    (name, fault): (&str, Fault),
    evidence: &mut String,
) -> Result<()> {
    let p = &PROFILES[0];
    let old = fs::read(baseline.join("reboot-build-app"))?;
    let disk = work.join("work.ext4");
    fs::copy(baseline.join("work.ext4"), &disk)?;
    let (mut d, vars) = Driver::boot_fault(
        root,
        iso,
        &disk,
        work,
        "publication",
        "# Arm the external blkdebug filter after saving the replacement source.\n",
    )?;
    d.ready(0, 0)?;
    d.wait_since(0, "anova> ")?;
    files(&mut d)?;
    let path = p.sources[0].0;
    let from = d.output.position();
    prompt(&mut d, "f1", path)?;
    wait_record(&mut d, from, "[FILES:OPEN]", path, C.len())?;
    for _ in 0..C.lines().count() {
        d.key("down")?;
    }
    d.key("end")?;
    for _ in 0..C.len() {
        d.key("backspace")?;
    }
    type_text(&mut d, C_ALTERNATE)?;
    save(&mut d, path, C_ALTERNATE)?;
    close_files(&mut d)?;
    terminal(&mut d)?;
    // The reservation completes one six-flush JBD2 transaction. The temporary
    // linker output is volatile, so the next write and flush are staging copy.
    desktop_files_device::arm(&mut d, name, fault)?;
    let build = start(&mut d, p, 1, evidence)?;
    let end = finish(&mut d, &build, p.build_budget, evidence)?;
    let uart = d.output.text();
    let produced = format!("[USER:EXIT] pid={} status=0", build.pid);
    let tail = &uart[build.from..];
    let (Some(exit), Some(failure)) = (tail.find(&produced), tail.find(fault.error_marker()))
    else {
        return Err("Missing successful compiler exit or real publication I/O error".into());
    };
    if exit >= failure
        || !end.contains("reason=0 code=0")
        || !end.contains("published=false error=-21")
        || !tail.contains(&format!("session={} descendants_reaped=true", build.pid))
    {
        return Err(format!("Wrong publication failure and cleanup outcome: {end}").into());
    }
    d.click(35, 60)?;
    desktop_build_test::capture(
        &mut d,
        work,
        "publication-failed",
        &[(84, "CODE=0"), (84, "ERROR=-21")],
        evidence,
    )?;
    desktop_files_device::disarm(&mut d)?;
    let began = Instant::now();
    d.command("echo PUBLISH-FAULT-COM1", 0)?;
    if began.elapsed() > INTERACTION {
        return Err("Publication fault blocked COM1 recovery".into());
    }
    d.command(&format!("cat {}", p.recipe.temporary()), 1)?;
    evidence.push_str(&format!(
        "{name} actual_fault={fault:?} com1_ms={}\n",
        began.elapsed().as_millis()
    ));
    // Preserve the failed mount image after the completed device error. Recovery
    // starts from a separate byte copy, through the guest's ordinary JBD2 mount.
    d.quit()?;
    drop(d);
    vars.cleanup()?;
    if dump(&disk, work, p.recipe.output(), "failed")? != old {
        return Err("Failed publication changed the old executable on disk".into());
    }
    let recovered = work.join("recovered.ext4");
    fs::copy(&disk, &recovered)?;
    let (mut d, vars) = Driver::boot(root, iso, &recovered, work, "recovery")?;
    d.ready(0, 0)?;
    d.wait_since(0, "anova> ")?;
    desktop_files_test::serial_exact(&mut d, path, C_ALTERNATE)?;
    terminal(&mut d)?;
    run_artifact(&mut d, p, work, "retained-run", evidence)?;
    // Recovery of the fail-closed mount may leave the exclusively named .next
    // path. Inspect and explicitly discard it before starting a fresh attempt.
    d.command("ls /persist", 0)?;
    d.command(&format!("rm {}", p.recipe.staging()), 0)?;
    let build = start(&mut d, p, 1, evidence)?;
    let end = finish(&mut d, &build, p.build_budget, evidence)?;
    status(&end, 0, 0, true)?;
    d.click(35, 60)?;
    d.key("esc")?;
    d.click(470, 30)?;
    type_command(&mut d, "/persist/build-app; /linux/echo RUN-$?")?;
    pty_terminal_test::screenshot(
        &mut d,
        work,
        "replacement-run",
        &["UNPUBLISHED", "RUN-19"],
        evidence,
    )?;
    verify_staging_is_empty(&mut d, p.recipe)?;
    poweroff(&mut d)?;
    vars.cleanup()?;
    foundation_journal::check(&recovered)?;
    let new = dump(&recovered, work, p.recipe.output(), "replaced")?;
    if new == old || dump(&recovered, work, path, "recovered")? != C_ALTERNATE.as_bytes() {
        return Err(
            "Recovered rebuild did not publish distinct bytes from the saved source".into(),
        );
    }
    anova_elf::parse_linux(&new).map_err(|error| format!("Recovered Linux ELF: {error:?}"))?;
    evidence.push_str(&format!(
        "{name} old_sha256={:x} replacement_sha256={:x} result=Ok\n",
        Sha256::digest(&old),
        Sha256::digest(&new),
    ));
    Ok(())
}

struct Job {
    started: Instant,
    from: usize,
    attempt: u64,
    pid: u64,
}

fn start(d: &mut Driver, p: &Profile, attempt: u64, evidence: &mut String) -> Result<Job> {
    d.click(35, 60)?;
    d.key("b")?;
    let from = d.output.position();
    let started = Instant::now();
    d.key(p.key)?;
    let record = d.require_live(
        from,
        &format!("[BUILD:START] attempt={attempt} "),
        started + INTERACTION,
    )?;
    let pid = field(&record, "id")?;
    evidence.push_str(&record);
    Ok(Job {
        started,
        from,
        attempt,
        pid,
    })
}

fn live_peer(
    d: &mut Driver,
    work: &Path,
    job: &Job,
    label: &str,
    peer: Peer,
    evidence: &mut String,
) -> Result<()> {
    // Focus the live graphical terminal and require a fresh echoed key on scanout.
    let observe_live = label != "cancel-recovery";
    let key = if observe_live { "a" } else { "b" };
    let needle = if observe_live { peer.echo() } else { "PTY$ B" };
    let path = work.join(format!("{label}-peer.ppm"));
    d.qmp
        .command("screendump", json!({"filename":path,"format":"ppm"}))?;
    if pty_terminal_test::contains(parse_ppm(&read_ppm(&path)?)?.rgb, needle) {
        return Err("Peer already showed the input marker before the new physical key".into());
    }
    let begin = Instant::now();
    let serial_start = d.output.position();
    d.serial(b"status\r")?;
    // The software pointer must stay outside the raster used by the text oracle.
    d.click(470, 30)?;
    let Job { pid, .. } = *job;
    if observe_live {
        require_compiler_live(d, job)?;
    }
    let key_at = Instant::now();
    d.key(key)?;
    let mut visible = false;
    let mut com1_ms = None;
    loop {
        d.poll()?;
        if (!visible && key_at.elapsed() > INTERACTION)
            || (com1_ms.is_none() && begin.elapsed() > INTERACTION)
        {
            let text = d.output.text();
            return Err(format!(
                "Peer scanout deadline/lifetime at {} ms; UART since input: {}",
                key_at.elapsed().as_millis(),
                text[serial_start..]
                    .lines()
                    .rev()
                    .take(4)
                    .collect::<Vec<_>>()
                    .join(" | ")
            )
            .into());
        }
        if observe_live {
            require_compiler_live(d, job)?;
        }
        if com1_ms.is_none()
            && let Some(code) = d.output.shell_status_from(serial_start)
        {
            if code != 0 {
                return Err(format!(
                    "Unexpected prior COM1 status during the compiler run: {code}"
                )
                .into());
            }
            com1_ms = Some(begin.elapsed().as_millis());
        }
        if !visible {
            d.qmp
                .command("screendump", json!({"filename":path,"format":"ppm"}))?;
            let bytes = read_ppm(&path)?;
            let image = parse_ppm(&bytes)?;
            if image.width != 800 || image.height != 600 {
                return Err("Unexpected compiler-load scanout geometry".into());
            }
            if pty_terminal_test::contains(image.rgb, needle) {
                if observe_live {
                    require_compiler_live(d, job)?;
                }
                let elapsed = key_at.elapsed();
                if elapsed > INTERACTION {
                    return Err("Peer scanout exceeded its interaction budget".into());
                }
                visible = true;
                evidence.push_str(&format!(
                    "task={pid} case={label} peer_scanout_ms={} peer_sha256={:x}\n",
                    elapsed.as_millis(),
                    Sha256::digest(bytes)
                ));
            }
        }
        if let (true, Some(ms)) = (visible, com1_ms) {
            evidence.push_str(&format!("task={pid} case={label} com1_ms={ms}\n",));
            break;
        }
        thread::sleep(Duration::from_millis(30));
    }
    let from = d.output.position();
    d.key("backspace")?;
    d.require_live(from, "[TERMINAL:PRESENT]", Instant::now() + INTERACTION)?;
    if !observe_live {
        type_command(d, "/linux/echo CANCEL-PEER")?;
        pty_terminal_test::screenshot(
            d,
            work,
            "cancel-recovery-command",
            &["CANCEL-PEER"],
            evidence,
        )?;
    }
    Ok(())
}

fn require_compiler_live(d: &mut Driver, job: &Job) -> Result<()> {
    d.poll()?;
    let text = d.output.text();
    for line in text[job.from..].lines() {
        if [
            "[USER:EXIT]",
            "[USER:SIGNAL]",
            "[USER:FAULT]",
            "[USER:RETURNREJECT]",
        ]
        .iter()
        .any(|marker| line.contains(marker))
            && field(line, "pid").ok() == Some(job.pid)
        {
            return Err(
                format!("Compiler {} ended during peer observation: {line}", job.pid).into(),
            );
        }
    }
    Ok(())
}

fn finish(d: &mut Driver, job: &Job, budget: Duration, evidence: &mut String) -> Result<String> {
    loop {
        d.poll()?;
        let elapsed = job.started.elapsed();
        if elapsed > budget {
            return Err(format!("Job {} exceeded {} ms", job.pid, budget.as_millis()).into());
        }
        let text = d.output.text();
        if let Some(end) = text[job.from..]
            .split_inclusive('\n')
            .find(|line| line.ends_with('\n') && line.contains("[BUILD:END]"))
        {
            if field(end, "id")? != job.pid || field(end, "attempt")? != job.attempt {
                return Err("Compiler output was associated with the wrong attempt".into());
            }
            evidence.push_str(&format!("build_elapsed_ms={} {end}", elapsed.as_millis()));
            return Ok(end.to_owned());
        }
        thread::sleep(OBSERVER);
    }
}

fn status(line: &str, reason: u64, code: u64, published: bool) -> Result<()> {
    if field(line, "reason")? != reason
        || field(line, "code")? != code
        || !line
            .split_whitespace()
            .any(|part| part == format!("published={published}"))
        || field(line, "error")? != 0
    {
        return Err(format!("Unexpected recipe status: {line}").into());
    }
    Ok(())
}

fn files(d: &mut Driver) -> Result<()> {
    d.click(35, 60)?;
    let from = d.output.position();
    d.key("f")?;
    d.wait_since(from, "[FILES:READY]")?;
    d.click(290, 460)
}

fn close_files(d: &mut Driver) -> Result<()> {
    let from = d.output.position();
    d.key("f4")?;
    d.wait_since(from, "[DESKTOP:EXIT] app=1")
}

fn terminal(d: &mut Driver) -> Result<()> {
    d.click(35, 60)?;
    let from = d.output.position();
    d.key("t")?;
    d.wait_since(from, "[TERMINAL:PRESENT]")?;
    d.click(470, 90)
}

fn create_source(d: &mut Driver, path: &str, source: &str) -> Result<()> {
    let from = d.output.position();
    prompt(d, "f2", path)?;
    d.wait_since(from, "[FILES:NEW]")?;
    type_text(d, source)?;
    save(d, path, source)
}

fn save(d: &mut Driver, path: &str, source: &str) -> Result<()> {
    let from = d.output.position();
    d.key("f5")?;
    wait_record(d, from, "[FILES:SAVED]", path, source.len())?;
    desktop_files_test::serial_exact(d, path, source)
}

fn run_artifact(
    d: &mut Driver,
    p: &Profile,
    work: &Path,
    label: &str,
    evidence: &mut String,
) -> Result<()> {
    d.click(35, 60)?;
    d.key("esc")?;
    d.click(470, 90)?;
    type_command(d, &format!("{}; /linux/echo RUN-$?", p.recipe.output()))?;
    pty_terminal_test::screenshot(d, work, label, p.output, evidence)
}

fn observe_disk(
    d: &mut Driver,
    disk: &Path,
    work: &Path,
    path: &str,
    label: &str,
) -> Result<Vec<u8>> {
    d.command("sync", 0)?;
    let snapshot = work.join(format!("{label}.ext4"));
    d.qmp.command("stop", json!({}))?;
    let copied = fs::copy(disk, &snapshot);
    d.qmp.command("cont", json!({}))?;
    copied?;
    dump(&snapshot, work, path, label)
}

fn dump(disk: &Path, work: &Path, path: &str, label: &str) -> Result<Vec<u8>> {
    let name = path
        .strip_prefix("/persist/")
        .ok_or("Expected a work-volume path")?;
    let out = work.join(format!("{label}-{name}"));
    command(
        Command::new("debugfs")
            .args(["-R", &format!("dump /{name} {}", out.display())])
            .arg(disk),
    )?;
    Ok(fs::read(out)?)
}

fn verify_disk(
    disk: &Path,
    work: &Path,
    p: &Profile,
    elf: &[u8],
    phase: &str,
    evidence: &mut String,
) -> Result<()> {
    for &(path, source) in p.sources {
        let bytes = dump(disk, work, path, phase)?;
        if bytes != source.as_bytes() {
            return Err(format!("Source bytes changed: {path}").into());
        }
        evidence.push_str(&format!(
            "{phase} {path} sha256={:x}\n",
            Sha256::digest(bytes)
        ));
    }
    let bytes = dump(disk, work, p.recipe.output(), phase)?;
    if bytes != elf {
        return Err("Executed ELF changed across clean shutdown".into());
    }
    evidence.push_str(&format!(
        "{phase} {} sha256={:x}\n",
        p.recipe.output(),
        Sha256::digest(bytes)
    ));
    Ok(())
}

fn poweroff(d: &mut Driver) -> Result<()> {
    d.click(35, 60)?;
    d.key("m")?;
    d.key("3")?;
    let from = d.output.position();
    d.power_key()?;
    d.wait_poweroff_since(from, INTERACTION)
}

fn verify_staging_is_empty(d: &mut Driver, recipe: Recipe) -> Result<()> {
    let from = d.output.position();
    d.command(&format!("cat {}", recipe.staging()), 1)?;
    d.command(&format!("cat {}", recipe.temporary()), 1)?;
    let records = d.output.text();
    if records[from..].matches("failed (-7)").count() != 2 {
        return Err("A build left a staging path or returned an unexpected read error".into());
    }
    Ok(())
}
