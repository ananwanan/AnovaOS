// SPDX-License-Identifier: Apache-2.0
//! Validate real graphical editor bounds, allocation failures and virtio write/flush failures.
//! Every case keeps its own regular disk, physical input, UART/QMP, scanouts and exact bytes.

use super::*;
use desktop_files_device::Fault;
use desktop_files_fixtures::{self as fixtures, PRIOR, Pressure};
use desktop_files_test::{focus_files, launch_files, prompt, serial_exact, wait_record};
use desktop_test::Driver;
use graphics::{parse_ppm, read_ppm};
use pty_terminal_test::glyph;
use serde_json::json;
use sha2::{Digest, Sha256};

const TARGET: &str = "/persist/case.c";
const PENDING: &[u8] = b"z// M17-FAULT-OLD\n";

#[derive(Clone, Copy)]
struct DeviceCase {
    name: &'static str,
    fault: Fault,
    operation: &'static str,
    suffix: &'static str,
    published: bool,
}

const DEVICES: &[DeviceCase] = &[
    DeviceCase {
        name: "temp-create-write",
        fault: Fault::WriteAfterFlush(0),
        operation: "create",
        suffix: ".m17-tmp",
        published: false,
    },
    DeviceCase {
        name: "temp-data-write",
        fault: Fault::WriteAfterFlush(6),
        operation: "write",
        suffix: ".m17-tmp",
        published: false,
    },
    DeviceCase {
        name: "temp-fsync",
        fault: Fault::Flush(13),
        operation: "fsync",
        suffix: ".m17-tmp",
        published: false,
    },
    DeviceCase {
        name: "backup-create-write",
        fault: Fault::WriteAfterFlush(13),
        operation: "create",
        suffix: ".m17-bak",
        published: false,
    },
    DeviceCase {
        name: "backup-data-write",
        fault: Fault::WriteAfterFlush(19),
        operation: "write",
        suffix: ".m17-bak",
        published: false,
    },
    DeviceCase {
        name: "backup-fsync",
        fault: Fault::Flush(26),
        operation: "fsync",
        suffix: ".m17-bak",
        published: false,
    },
    DeviceCase {
        name: "prepublish-fsync",
        fault: Fault::Flush(27),
        operation: "parent-fsync",
        suffix: "",
        published: false,
    },
    DeviceCase {
        name: "publish-write",
        fault: Fault::WriteAfterFlush(27),
        operation: "replace",
        suffix: ".m17-tmp",
        published: false,
    },
    DeviceCase {
        name: "postpublish-fsync",
        fault: Fault::Flush(40),
        operation: "parent-fsync",
        suffix: "",
        published: true,
    },
    DeviceCase {
        name: "cleanup-write",
        fault: Fault::WriteAfterFlush(40),
        operation: "remove",
        suffix: ".m17-bak",
        published: true,
    },
    DeviceCase {
        name: "cleanup-late-flush",
        fault: Fault::Flush(46),
        operation: "remove",
        suffix: ".m17-bak",
        published: true,
    },
];

pub fn verify(root: &Path, args: &[String]) -> Result<()> {
    let selected = match args {
        [] => None,
        [flag, name] if flag == "--case" => Some(name.as_str()),
        _ => return Err("Usage: cargo xtask desktop-files-fault-test [--case NAME]".into()),
    };
    let names: Vec<_> = DEVICES
        .iter()
        .map(|case| case.name)
        .chain([
            "bounds",
            "inode-temp",
            "inode-backup",
            "blocks-temp",
            "blocks-backup",
        ])
        .collect();
    if selected.is_some_and(|s| !names.contains(&s)) {
        return Err(format!("Unknown editor fault case; available: {}", names.join(", ")).into());
    }
    let base = directory(root, "desktop-files-fault-test")?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let work = base.join(stamp.to_string());
    fs::create_dir(&work)?;
    fs::write(base.join("latest.txt"), format!("{}\n", work.display()))?;
    let mut evidence = format!(
        "profile=M17.3-real-GUI-storage\nselected={}\ninteraction_budget_ms=5000\nboot_watchdog_ms=180000\npower_cut_during_save=unverified\nwork={}\n",
        selected.unwrap_or("all"),
        work.display()
    );
    fs::write(work.join("result.txt"), "result=Running\n")?;
    let outcome = (|| -> Result<()> {
        let identity = implementation_identity(root)?;
        fs::write(
            work.join("implementation.json"),
            serde_json::to_vec_pretty(&identity)?,
        )?;
        host_tools(&work)?;
        let built = desktop::image(root)?;
        let iso = work.join("desktop.iso");
        fs::copy(&built, &iso)?;
        evidence.push_str(&format!(
            "iso_sha256={:x}\n",
            Sha256::digest(fs::read(&iso)?)
        ));
        fs::copy(
            root.join("build/desktop/inputs.txt"),
            work.join("inputs.txt"),
        )?;
        for name in names {
            if selected.is_some_and(|s| s != name) {
                continue;
            }
            let case_work = work.join(name);
            fs::create_dir(&case_work)?;
            fs::write(case_work.join("result.txt"), "result=Running\n")?;
            let result = if name == "bounds" {
                bounds(root, &iso, &case_work, stamp)
            } else if let Some(case) = DEVICES.iter().find(|c| c.name == name) {
                device(root, &iso, &case_work, stamp, *case)
            } else {
                let (pressure, operation, suffix) = match name {
                    "inode-temp" => (Pressure::Inodes(0), "create", ".m17-tmp"),
                    "inode-backup" => (Pressure::Inodes(1), "create", ".m17-bak"),
                    "blocks-temp" => (Pressure::Blocks(0), "write", ".m17-tmp"),
                    "blocks-backup" => (Pressure::Blocks(1), "write", ".m17-bak"),
                    _ => unreachable!(),
                };
                pressure_case(
                    root, &iso, &case_work, stamp, name, pressure, operation, suffix,
                )
            };
            match &result {
                Ok(()) => {
                    fs::write(case_work.join("result.txt"), "result=Ok\n")?;
                    evidence.push_str(&format!("{name}=Ok\n"));
                }
                Err(error) => {
                    fs::write(
                        case_work.join("result.txt"),
                        format!("result=Err\nerror={error}\n"),
                    )?;
                    evidence.push_str(&format!("{name}=Err\n"));
                }
            }
            result.map_err(|e| format!("{name}: {e}"))?;
        }
        if implementation_identity(root)? != identity {
            return Err("Editor acceptance implementation changed during the run".into());
        }
        Ok(())
    })();
    evidence.push_str(&match &outcome {
        Ok(()) => "result=Ok\n".into(),
        Err(e) => format!("result=Err\nerror={e}\n"),
    });
    fs::write(work.join("result.txt"), evidence)?;
    outcome?;
    println!("Verified real GUI editor storage cases: {}", work.display());
    Ok(())
}

fn implementation_identity(root: &Path) -> Result<Vec<(&'static str, String)>> {
    [
        "xtask/src/desktop_files_fault_test.rs",
        "xtask/src/desktop_files_fixtures.rs",
        "xtask/src/desktop_files_device.rs",
        "xtask/src/desktop_files_test.rs",
        "xtask/src/desktop_test.rs",
        "xtask/src/graphics.rs",
        "xtask/src/disk.rs",
        "xtask/src/main.rs",
        "user/files/src/main.rs",
        "user/files/src/fs.rs",
        "crates/editor/src/lib.rs",
        "kernel/src/drivers/virtio.rs",
        "crates/storage/src/ext4.rs",
        "crates/storage/src/ext4/write.rs",
        "crates/storage/src/ext4/journal.rs",
        "crates/vfs/src/lib.rs",
        ".github/workflows/boot.yml",
    ]
    .into_iter()
    .map(|path| {
        Ok((
            path,
            format!("{:x}", Sha256::digest(fs::read(root.join(path))?)),
        ))
    })
    .collect()
}

fn host_tools(work: &Path) -> Result<()> {
    let mut records = Vec::new();
    for (name, argument) in [
        ("qemu-system-x86_64", "--version"),
        ("debugfs", "-V"),
        ("mke2fs", "-V"),
        ("e2fsck", "-V"),
    ] {
        let binary = env::split_paths(&env::var_os("PATH").ok_or("Missing host PATH")?)
            .map(|directory| directory.join(name))
            .find(|path| path.is_file())
            .ok_or("Missing host evidence tool")?
            .canonicalize()?;
        checked_file(&binary)?;
        let output = Command::new(&binary).arg(argument).output()?;
        if !output.status.success() {
            return Err(format!("Host tool version check failed: {name}").into());
        }
        records.push(json!({"name":name,"path":binary,"sha256":format!("{:x}",Sha256::digest(fs::read(&binary)?)),"stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr)}));
    }
    fs::write(
        work.join("host-tools.json"),
        serde_json::to_vec_pretty(&records)?,
    )?;
    Ok(())
}

fn new_fixture(
    root: &Path,
    work: &Path,
    stamp: u128,
    name: &str,
    pressure: Pressure,
) -> Result<(PathBuf, fixtures::Fixture)> {
    let path = directory(root, "disks")?.join(format!("m17-files-{stamp}-{name}.ext4"));
    let fixture = fixtures::create(root, &path, work, pressure)?;
    println!(
        "Editor fault case {name}: free_blocks={} free_inodes={}",
        fixture.free_blocks, fixture.free_inodes
    );
    Ok((path, fixture))
}

fn start(
    root: &Path,
    iso: &Path,
    disk: &Path,
    work: &Path,
    name: &str,
    fault: bool,
) -> Result<(Driver, Temporary)> {
    let (mut d, vars) = if fault {
        Driver::boot_fault(
            root,
            iso,
            disk,
            work,
            name,
            "# Arm faults through a fresh QMP blkdebug filter after boot.\n",
        )?
    } else {
        Driver::boot(root, iso, disk, work, name)?
    };
    d.wait("anova> ")?;
    d.ready(0, 0)?;
    launch_files(&mut d)?;
    Ok((d, vars))
}

fn open(d: &mut Driver, path: &str, length: usize) -> Result<()> {
    let began = Instant::now();
    let from = d.output.position();
    prompt(d, "f1", path)?;
    d.wait_since(from, "[FILES:OPEN]")?;
    wait_record(d, from, "[FILES:OPEN]", path, length)?;
    if began.elapsed() >= Duration::from_secs(5) {
        return Err("GUI open exceeded five seconds".into());
    }
    Ok(())
}

fn save_exact(d: &mut Driver, path: &str, bytes: &[u8]) -> Result<()> {
    let began = Instant::now();
    let from = d.output.position();
    d.key("f5")?;
    d.wait_since(from, "[FILES:SAVED]")?;
    wait_record(d, from, "[FILES:SAVED]", path, bytes.len())?;
    if began.elapsed() >= Duration::from_secs(5) {
        return Err("GUI save exceeded five seconds".into());
    }
    serial_exact(d, path, core::str::from_utf8(bytes)?)
}

fn error(d: &mut Driver, from: usize, expected: &str, dirty: bool) -> Result<()> {
    let began = Instant::now();
    loop {
        d.poll()?;
        let text = d.output.text();
        if began.elapsed() >= Duration::from_secs(5) {
            return Err("GUI error exceeded five seconds".into());
        }
        if let Some(line) = text[from..]
            .split_inclusive('\n')
            .find(|l| l.ends_with('\n') && l.contains("[FILES:ERROR]"))
        {
            if !line.contains(&format!("error={expected} "))
                || !line.contains(&format!("dirty={dirty}"))
            {
                return Err(format!("Wrong GUI error/dirty state: {line}").into());
            }
            if text[from..].contains("[FILES:SAVED]") {
                return Err("Failed operation reported saved".into());
            }
            return Ok(());
        }
        if text[from..].contains("[FILES:SAVED]") {
            return Err("Faulted save unexpectedly succeeded".into());
        }
        if began.elapsed() >= Duration::from_secs(5) {
            return Err("GUI error exceeded five seconds".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn buffer_exact(
    d: &mut Driver,
    from: usize,
    path: &str,
    bytes: &[u8],
    cursor: usize,
    dirty: bool,
) -> Result<()> {
    let mut fingerprint = 0xcbf29ce484222325u64;
    for byte in bytes {
        fingerprint = (fingerprint ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
    let hex = if bytes.len() <= 64 {
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    } else {
        "-".into()
    };
    let expected = format!(
        "[FILES:BUFFER] path={path} bytes={} cursor={cursor} dirty={dirty} fnv64={fingerprint:016x} hex={hex}",
        bytes.len()
    );
    let began = Instant::now();
    loop {
        d.poll()?;
        if began.elapsed() >= Duration::from_secs(5) {
            return Err("Live buffer observation exceeded five seconds".into());
        }
        if d.output.text()[from..].contains(&expected) {
            return Ok(());
        }
        if began.elapsed() >= Duration::from_secs(5) {
            return Err(
                format!("Live buffer bytes/path/cursor changed; missing {expected}").into(),
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn io_failure(d: &Driver, from: usize, operation: &str, suffix: &str, errno: i64) -> Result<()> {
    let text = d.output.text();
    let lines = &text[from..];
    let expected = format!("result=Err({errno})");
    if !lines.lines().any(|line| {
        line.contains("[FILES:IO_END]")
            && line.contains(&format!("op={operation} "))
            && line.contains(suffix)
            && line.contains(&expected)
    }) {
        return Err(format!("Fault did not hit required {operation} {suffix}: {lines}").into());
    }
    Ok(())
}

fn bounds(root: &Path, iso: &Path, work: &Path, stamp: u128) -> Result<()> {
    let (disk, _) = new_fixture(root, work, stamp, "bounds", Pressure::None)?;
    let (mut d, vars) = start(root, iso, &disk, work, "graphical", false)?;
    let original = fixtures::limit_bytes();
    open(&mut d, "/persist/limit.c", original.len())?;
    let from = d.output.position();
    prompt(&mut d, "f1", "/persist/oversize.c")?;
    error(&mut d, from, "TooLong", false)?;
    buffer_exact(&mut d, from, "/persist/limit.c", &original, 0, false)?;
    scanout(
        &mut d,
        work,
        "oversize-open-clean",
        false,
        "M17-LIMIT",
        3,
        "FILE/BUFFER",
    )?;
    limits_unchanged(&mut d, &disk, work, "reject-open-clean", &original)?;
    let from = d.output.position();
    d.key("q")?;
    error(&mut d, from, "TooLong", false)?;
    buffer_exact(&mut d, from, "/persist/limit.c", &original, 0, false)?;
    limits_unchanged(&mut d, &disk, work, "reject-edit-clean", &original)?;
    save_exact(&mut d, "/persist/limit.c", &original)?;
    d.key("delete")?;
    d.key("z")?;
    let mut pending = original.clone();
    pending[0] = b'z';
    let from = d.output.position();
    d.key("q")?;
    error(&mut d, from, "TooLong", true)?;
    buffer_exact(&mut d, from, "/persist/limit.c", &pending, 1, true)?;
    scanout(&mut d, work, "oversize-edit", true, "Z", 0, "FILE/BUFFER")?;
    limits_unchanged(&mut d, &disk, work, "reject-edit", &original)?;
    let from = d.output.position();
    prompt(&mut d, "f1", "/persist/oversize.c")?;
    d.wait_since(from, "[FILES:UNSAVED]")?;
    let from = d.output.position();
    d.key("d")?;
    error(&mut d, from, "TooLong", true)?;
    buffer_exact(&mut d, from, "/persist/limit.c", &pending, 1, true)?;
    scanout(
        &mut d,
        work,
        "oversize-open-dirty",
        true,
        "Z",
        0,
        "FILE/BUFFER",
    )?;
    limits_unchanged(&mut d, &disk, work, "reject-open-dirty", &original)?;
    save_exact(&mut d, "/persist/limit.c", &pending)?;
    finish(&mut d)?;
    drop(d);
    vars.cleanup()?;
    clean_inspect(
        &disk,
        work,
        "shutdown",
        &[
            ("/limit.c", &pending),
            ("/oversize.c", &fixtures::oversize_bytes()),
        ],
    )?;
    let (mut d, vars) = start(root, iso, &disk, work, "reboot", false)?;
    open(&mut d, "/persist/limit.c", pending.len())?;
    finish(&mut d)?;
    drop(d);
    vars.cleanup()?;
    clean_inspect(
        &disk,
        work,
        "reboot",
        &[
            ("/limit.c", &pending),
            ("/oversize.c", &fixtures::oversize_bytes()),
        ],
    )
}

#[allow(clippy::too_many_arguments)]
fn pressure_case(
    root: &Path,
    iso: &Path,
    work: &Path,
    stamp: u128,
    name: &str,
    pressure: Pressure,
    operation: &str,
    suffix: &str,
) -> Result<()> {
    let (disk, fixture) = new_fixture(root, work, stamp, name, pressure)?;
    let (mut d, vars) = start(root, iso, &disk, work, "graphical", false)?;
    open(&mut d, TARGET, PRIOR.len())?;
    d.key("z")?;
    let from = d.output.position();
    d.key("f5")?;
    error(&mut d, from, "Io(-18)", true)?;
    buffer_exact(&mut d, from, TARGET, PENDING, 1, true)?;
    io_failure(&d, from, operation, suffix, -18)?;
    scanout(
        &mut d,
        work,
        "failed",
        true,
        "M17-FAULT-OLD",
        4,
        "DISK/INODE",
    )?;
    serial_exact(&mut d, TARGET, core::str::from_utf8(PRIOR)?)?;
    snapshot(&mut d, &disk, work, "failed")?;
    verify_artifacts(&work.join("failed.ext4"), work, "failed-check", name)?;
    for command in &fixture.recovery_commands {
        d.command(command, 0)?;
    }
    for path in ["/persist/case.c.m17-tmp", "/persist/case.c.m17-bak"] {
        // A failed create need not have made the sibling. Inspect snapshots first,
        // then remove only transaction artifacts created by this isolated case.
        d.command(
            &format!("rm {path}"),
            if sibling_exists(work, path)? { 0 } else { 1 },
        )?;
    }
    save_exact(&mut d, TARGET, PENDING)?;
    scanout(
        &mut d,
        work,
        "retry-saved",
        false,
        "M17-FAULT-OLD",
        4,
        "SAVED",
    )?;
    finish(&mut d)?;
    drop(d);
    vars.cleanup()?;
    clean_inspect(&disk, work, "shutdown", &[("/case.c", PENDING)])?;
    reboot_exact(root, iso, &disk, work)
}

fn device(root: &Path, iso: &Path, work: &Path, stamp: u128, case: DeviceCase) -> Result<()> {
    let (disk, _) = new_fixture(root, work, stamp, case.name, Pressure::None)?;
    let (mut d, vars) = start(root, iso, &disk, work, "graphical", true)?;
    open(&mut d, TARGET, PRIOR.len())?;
    d.key("z")?;
    fs::write(
        work.join("fault-configuration.json"),
        serde_json::to_vec_pretty(&json!({
            "fault":format!("{:?}",case.fault),"expected_operation":case.operation,
            "expected_path_suffix":case.suffix,"expected_publication":case.published,
            "device_error":case.fault.error_marker(),"filter_rules":"graphical.qmp.log",
            "mount_recovery":"fail closed until ordinary reboot/JBD2 replay"
        }))?,
    )?;
    let from = d.output.position();
    desktop_files_device::save(&mut d, work, &disk, case.name, case.fault)?;
    let cleanup = case.name.starts_with("cleanup-");
    if cleanup {
        let began = Instant::now();
        d.wait_since(from, "[FILES:SAVED]")?;
        if began.elapsed() >= Duration::from_secs(5) {
            return Err("Cleanup-error successful save exceeded five seconds".into());
        }
        wait_record(&mut d, from, "[FILES:SAVED]", TARGET, PENDING.len())?;
        if !d.output.text()[from..].contains("backup_cleanup_failed=true") {
            return Err("Cleanup error lost explicit successful-save outcome".into());
        }
    } else {
        error(&mut d, from, "Io(-21)", true)?;
        buffer_exact(&mut d, from, TARGET, PENDING, 1, true)?;
    }
    io_failure(&d, from, case.operation, case.suffix, -21)?;
    if !d.output.text()[from..].contains(case.fault.error_marker()) {
        return Err("Missing actual virtio completed error status".into());
    }
    scanout(
        &mut d,
        work,
        "failed",
        !cleanup,
        "M17-FAULT-OLD",
        4,
        if cleanup { "SAVED" } else { "STORAGE" },
    )?;
    if cleanup && d.output.text()[from..].contains("[FILES:ERROR]") {
        return Err("Successful save was overwritten by refresh error".into());
    }
    desktop_files_device::disarm(&mut d)?;
    let from = d.output.position();
    d.key("f5")?;
    error(&mut d, from, "Io(-21)", !cleanup)?;
    buffer_exact(&mut d, from, TARGET, PENDING, 1, !cleanup)?;
    scanout(
        &mut d,
        work,
        "fail-closed-retry",
        !cleanup,
        "M17-FAULT-OLD",
        4,
        "STORAGE",
    )?;
    let recovery_began = Instant::now();
    d.command("echo M17-FAULT-COM1-ALIVE", 0)?;
    if recovery_began.elapsed() >= Duration::from_secs(5) {
        return Err("Independent COM1 recovery exceeded five seconds".into());
    }
    // M16 deliberately fails the mount closed after IO uncertainty. Terminate only
    // after the completed error/retry, preserving the original disk for audit.
    d.qmp.command("quit", json!({}))?;
    drop(d);
    vars.cleanup()?;
    let raw = work.join("failed.ext4");
    fs::copy(&disk, &raw)?;
    inspect_optional(&raw, work, "raw")?;
    verify_artifacts(&raw, work, "raw-check", case.name)?;
    fs::write(
        work.join("recovery-policy.txt"),
        "device fault removed; same-mount retry still fails closed and preserves the prior dirty/clean state\nhost termination after completed save error, not a cut during save\nrecover using a separate copied disk and ordinary guest JBD2 replay\nunsaved bytes are explicitly re-entered after reboot; crash-buffer recovery is not claimed\n",
    )?;
    let recovered =
        directory(root, "disks")?.join(format!("m17-files-{stamp}-{}-recovered.ext4", case.name));
    fs::copy(&disk, &recovered)?;
    let (mut d, vars) = start(root, iso, &recovered, work, "recovery", false)?;
    snapshot(&mut d, &recovered, work, "replayed")?;
    verify_artifacts(
        &work.join("replayed.ext4"),
        work,
        "replayed-check",
        case.name,
    )?;
    let expected = if case.published { PENDING } else { PRIOR };
    serial_exact(&mut d, TARGET, core::str::from_utf8(expected)?)?;
    for suffix in [".m17-tmp", ".m17-bak"] {
        let path = format!("{TARGET}{suffix}");
        let exists = raw_optional(
            &raw,
            work,
            "recovery-reference",
            &format!("/case.c{suffix}"),
        )?
        .is_some();
        if exists {
            if suffix == ".m17-bak" && case.name != "backup-data-write" {
                serial_exact(&mut d, &path, core::str::from_utf8(PRIOR)?)?;
                d.command(&format!("cat {path} > /persist/recovered.c"), 0)?;
                serial_exact(&mut d, "/persist/recovered.c", core::str::from_utf8(PRIOR)?)?;
            }
            d.command(&format!("rm {path}"), 0)?;
        }
    }
    open(&mut d, TARGET, expected.len())?;
    if !case.published {
        d.key("z")?;
    }
    save_exact(&mut d, TARGET, PENDING)?;
    finish(&mut d)?;
    drop(d);
    vars.cleanup()?;
    clean_inspect(&recovered, work, "shutdown", &[("/case.c", PENDING)])?;
    reboot_exact(root, iso, &recovered, work)
}

fn reboot_exact(root: &Path, iso: &Path, disk: &Path, work: &Path) -> Result<()> {
    let (mut d, vars) = start(root, iso, disk, work, "reboot", false)?;
    open(&mut d, TARGET, PENDING.len())?;
    serial_exact(&mut d, TARGET, core::str::from_utf8(PENDING)?)?;
    scanout(&mut d, work, "reboot", false, "M17-FAULT-OLD", 4, "READY")?;
    finish(&mut d)?;
    drop(d);
    vars.cleanup()?;
    clean_inspect(disk, work, "reboot", &[("/case.c", PENDING)])
}

fn finish(d: &mut Driver) -> Result<()> {
    let began = Instant::now();
    let from = d.output.position();
    d.key("f4")?;
    d.wait_since(from, "[DESKTOP:EXIT] app=1")?;
    if began.elapsed() >= Duration::from_secs(5) {
        return Err("GUI close exceeded five seconds".into());
    }
    d.command("echo M17-FAULT-COM1-RECOVERY", 0)?;
    d.click(35, 60)?;
    let from = d.output.position();
    d.key("q")?;
    d.wait_since(from, "[INIT] desktop exited; COM1 shell remains available")?;
    d.command("sync", 0)?;
    d.shutdown()?;
    desktop_test::validate_resources(&d.output.text())
}

fn limits_unchanged(
    d: &mut Driver,
    disk: &Path,
    work: &Path,
    label: &str,
    original: &[u8],
) -> Result<()> {
    snapshot(d, disk, work, label)?;
    let stopped = work.join(format!("{label}.ext4"));
    for (path, bytes) in [
        ("/limit.c", original),
        ("/oversize.c", &fixtures::oversize_bytes()),
    ] {
        if raw_optional(&stopped, work, label, path)?.as_deref() != Some(bytes) {
            return Err(format!("Rejected operation changed exact disk bytes: {path}").into());
        }
    }
    Ok(())
}

fn snapshot(d: &mut Driver, disk: &Path, work: &Path, name: &str) -> Result<()> {
    d.qmp.command("stop", json!({}))?;
    let result = (|| {
        let saved = work.join(format!("{name}.ext4"));
        fs::copy(disk, &saved)?;
        inspect_optional(&saved, work, name)
    })();
    d.qmp.command("cont", json!({}))?;
    result
}

fn raw_optional(disk: &Path, work: &Path, label: &str, path: &str) -> Result<Option<Vec<u8>>> {
    let output = work.join(format!(
        "{label}-{}",
        path.trim_start_matches('/').replace('/', "_")
    ));
    output_path(&output)?;
    if output.exists() {
        fs::remove_file(&output)?;
    }
    let result = Command::new("debugfs")
        .args(["-R", &format!("dump {path} {}", output.display())])
        .arg(disk)
        .output()?;
    let diagnostic = [result.stdout, result.stderr].concat();
    fs::write(
        work.join(format!(
            "{label}-{}.debugfs.log",
            path.trim_start_matches('/').replace('/', "_")
        )),
        &diagnostic,
    )?;
    if !result.status.success() {
        return Err(format!("debugfs dump tool failed: {path}").into());
    }
    if output.exists() {
        if String::from_utf8_lossy(&diagnostic)
            .lines()
            .any(|line| !line.trim().is_empty() && !line.starts_with("debugfs "))
        {
            return Err(format!("debugfs reported a read diagnostic: {path}").into());
        }
        Ok(Some(fs::read(output)?))
    } else if String::from_utf8_lossy(&diagnostic).contains("File not found by ext2_lookup") {
        Ok(None)
    } else {
        Err(format!(
            "debugfs did not produce requested file or an explicit missing-file result: {path}"
        )
        .into())
    }
}

fn inspect_optional(disk: &Path, work: &Path, label: &str) -> Result<()> {
    let mut records = Vec::new();
    for path in ["/case.c", "/case.c.m17-tmp", "/case.c.m17-bak"] {
        let bytes = raw_optional(disk, work, label, path)?;
        records.push(json!({"path":path,"exists":bytes.is_some(),"bytes":bytes.as_ref().map(Vec::len),"sha256":bytes.as_ref().map(|b|format!("{:x}",Sha256::digest(b)))}));
    }
    fs::write(
        work.join(format!("{label}-files.json")),
        serde_json::to_vec_pretty(&records)?,
    )?;
    Ok(())
}

fn verify_artifacts(disk: &Path, work: &Path, label: &str, name: &str) -> Result<()> {
    let target = raw_optional(disk, work, label, "/case.c")?;
    let temporary = raw_optional(disk, work, label, "/case.c.m17-tmp")?;
    let backup = raw_optional(disk, work, label, "/case.c.m17-bak")?;
    desktop_files_device::check_artifacts(
        name,
        PRIOR,
        PENDING,
        [target.as_deref(), temporary.as_deref(), backup.as_deref()],
    )
}

fn sibling_exists(work: &Path, path: &str) -> Result<bool> {
    let name = path.strip_prefix("/persist/").ok_or("Unexpected sibling")?;
    Ok(work.join(format!("failed-{name}")).exists())
}

fn clean_inspect(disk: &Path, work: &Path, label: &str, expected: &[(&str, &[u8])]) -> Result<()> {
    let check = Command::new("e2fsck")
        .args(["-f", "-n"])
        .arg(disk)
        .output()?;
    fs::write(
        work.join(format!("{label}-e2fsck.log")),
        [check.stdout, check.stderr].concat(),
    )?;
    if !check.status.success() {
        return Err("Stopped clean disk failed independent e2fsck".into());
    }
    let mut hashes = Vec::new();
    for &(path, bytes) in expected
        .iter()
        .chain(std::iter::once(&("/.anova-abi", &b"linux.x86_64.v1\n"[..])))
    {
        let actual =
            raw_optional(disk, work, label, path)?.ok_or("Missing expected stopped-disk file")?;
        if actual != bytes {
            return Err(format!("Stopped disk exact bytes differ: {path}").into());
        }
        hashes.push(json!({"path":path,"bytes":actual.len(),"sha256":format!("{:x}",Sha256::digest(&actual))}));
    }
    fs::write(
        work.join(format!("{label}-exact-files.json")),
        serde_json::to_vec_pretty(&hashes)?,
    )?;
    if let Some(restored) = raw_optional(disk, work, label, "/recovered.c")?
        && restored != PRIOR
    {
        return Err("Recovered prior-copy bytes changed after clean shutdown/reboot".into());
    }
    inspect_optional(disk, work, label)
}

fn scanout(
    d: &mut Driver,
    work: &Path,
    name: &str,
    dirty: bool,
    source: &str,
    column: usize,
    notice: &str,
) -> Result<()> {
    focus_files(d)?;
    d.move_to(790, 580)?;
    let screenshot = work.join(format!("{name}.ppm"));
    let started = Instant::now();
    let bytes = loop {
        d.poll()?;
        d.qmp
            .command("screendump", json!({"filename":screenshot,"format":"ppm"}))?;
        let bytes = read_ppm(&screenshot)?;
        let image = parse_ppm(&bytes)?;
        let samples = [
            (274 + column * 6, 306, source, 0xffffff),
            (274, 290, if dirty { "*" } else { " " }, 0x60d0ff),
            (274, 438, notice, 0xffc060),
        ];
        let matches = image.width == 800
            && image.height == 600
            && samples.iter().all(|&(x, y, text, color)| {
                text.bytes().enumerate().all(|(c, b)| {
                    let glyph = match b {
                        b'*' => [0, 21, 14, 31, 14, 21, 0],
                        b'/' => [1, 2, 2, 4, 8, 8, 16],
                        b'G' => [14, 17, 16, 23, 17, 17, 15],
                        b'V' => [17, 17, 17, 17, 17, 10, 4],
                        _ => glyph(b),
                    };
                    glyph.into_iter().enumerate().all(|(r, bits)| {
                        (0..5).all(|dx| {
                            let offset = ((y + r) * 800 + x + c * 6 + dx) * 3;
                            let rgb = if bits & (16 >> dx) != 0 {
                                [(color >> 16) as u8, (color >> 8) as u8, color as u8]
                            } else {
                                [0x17, 0x23, 0x2f]
                            };
                            image.rgb.get(offset..offset + 3) == Some(&rgb[..])
                        })
                    })
                })
            });
        if started.elapsed() >= Duration::from_secs(5) {
            return Err(
                format!("Actual GUI scanout missing {name} source/dirty/error glyphs").into(),
            );
        }
        if matches {
            break bytes;
        }
        thread::sleep(Duration::from_millis(100));
    };
    fs::write(
        work.join(format!("{name}.scanout.json")),
        serde_json::to_vec_pretty(
            &json!({"sha256":format!("{:x}",Sha256::digest(bytes)),"source":source,"dirty":dirty,"notice":notice,"elapsed_ms":started.elapsed().as_millis()}),
        )?,
    )?;
    Ok(())
}
