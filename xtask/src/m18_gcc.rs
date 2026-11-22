// SPDX-License-Identifier: Apache-2.0
//! Verify original GCC build targets and guest-built C/C++ tools on independent JBD2 disks.
//! Keep diagnostics for incomplete stages and compare observed workspace bytes after reboot.
use super::*;
use anova_storage::{ext4::Ext4, filesystem::Filesystem};
use bootstrap::{checked, hash, hashes};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;

type Files = BTreeMap<String, Vec<u8>>;
const UART_MS: u128 = 3000;
const UART_WAIT: &str = "[M18:GCC:UART] waiting";
const UART_ACK: &str = "[M18:GCC:UART] configure-live";
const ARITHMETIC: &str = "[M18:GCC:ARITHMETIC] gmp=42 mpfr=42 mpc=42+1i";
const EXEC_BOUNDS: &str = "[M18:GCC:EXEC] success";
const BINUTILS_ALIASES: [(&str, &str); 2] = [
    ("/m18-gcc/tools/as", "/tc/bin/as"),
    ("/m18-gcc/tools/real-ld", "/tc/bin/ld"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Profile {
    Entry,
    Stage1,
}

impl Profile {
    fn name(self) -> &'static str {
        match self {
            Self::Entry => "entry",
            Self::Stage1 => "stage1",
        }
    }

    fn disk_blocks(self) -> u64 {
        match self {
            Self::Entry => 16_384,
            Self::Stage1 => 131_072,
        }
    }

    fn disk_inodes(self) -> u64 {
        match self {
            Self::Entry => 4_096,
            Self::Stage1 => 16_384,
        }
    }

    fn session_seconds(self) -> u64 {
        match self {
            Self::Entry => 900,
            Self::Stage1 => 14_400,
        }
    }
}
const STAGE1: &str = "[M18:GCC:STAGE1] c=17 cpp=23 syntax=1 link=1";
const RECOVERY: &str = "[M18:GCC:RECOVERY] c=17 cpp=23 frontends=verified";
// Export named evidence without copying source/object trees off the work disk.
// The last column sets how many trailing bytes the verifier needs to read back.
const OBSERVED_FILES: &[(&str, bool, usize)] = &[
    ("arithmetic", true, 1024 * 1024),
    ("missing-header.log", true, 8192),
    ("missing-library.log", true, 8192),
    ("rejected", false, 0),
    ("rejected.o", false, 0),
    ("configured/configure.exit", true, 16),
    ("configured/configure.stdout", true, 65536),
    ("configured/config.log", true, 0),
    ("configured/Makefile", true, 0),
    ("configured/make.exit", true, 16),
    ("configured/make.stdout", true, 8192),
    ("configured/recursive-probe.exit", true, 16),
    ("configured/recursive-probe.stdout", true, 8192),
    ("configured/intl/config.log", true, 0),
    ("configured/stage1.exit", true, 16),
    ("configured/gcc/config.log", true, 0),
    ("configured/gcc/Makefile", true, 0),
    ("configured/gcc/cc1", false, 0),
    ("configured/gcc/cc1plus", false, 0),
    ("configured/gcc/collect2", false, 0),
    ("configured/gcc/xgcc", false, 0),
    ("configured/gcc/xg++", false, 0),
    ("stage1-check/exit", true, 16),
    ("stage1-check/driver.log", true, 8192),
    ("stage1-check/c-compile.log", true, 16384),
    ("stage1-check/c-link.log", true, 16384),
    ("stage1-check/c.elf", true, 1024 * 1024),
    ("stage1-check/c.out", true, 128),
    ("stage1-check/cpp-compile.log", true, 16384),
    ("stage1-check/cpp-link.log", true, 16384),
    ("stage1-check/cpp.elf", true, 1024 * 1024),
    ("stage1-check/cpp.out", true, 128),
    ("stage1-check/syntax.log", true, 8192),
    ("stage1-check/link.log", true, 8192),
    ("stage1-check/rejected.o", false, 0),
    ("stage1-check/rejected.elf", false, 0),
];

struct Workspace {
    artifacts: BTreeMap<&'static str, Value>,
    retained: Files,
}

pub(super) fn verify(root: &Path, args: &[String]) -> Result<()> {
    let profile = match args {
        [] => Profile::Stage1,
        [arg] if arg == "--entry" => Profile::Entry,
        _ => return Err("Usage: cargo xtask m18-gcc-test [--entry]".into()),
    };
    let work = directory(
        root,
        &format!(
            "m18-gcc-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ),
    )?;
    println!("M18 GCC evidence: {}", work.display());
    let budgets = json!({
        "profile": profile.name(), "session_seconds": profile.session_seconds(),
        "uart_response_ms": UART_MS, "cpus": 1, "tcg": true, "network": false,
        "ram_bytes": 1073741824, "frame_delta_pages": 196608, "heap_bytes": 16777216,
        "task_slots": 32, "fds_per_process": 32, "vfs_slots": 2048,
        "disk_bytes": profile.disk_blocks() * 4096, "disk_blocks": profile.disk_blocks(),
        "disk_inodes": profile.disk_inodes(),
        "persistent_file_bytes": anova_storage::ext4::MAX_FILE,
        "working_space_scope": "Mount-wide successful allocation mutations; no exact RSS",
    });
    save(&work, "budgets.json", &budgets)?;
    let (iso, provenance) = prepare(root, &work, profile)?;
    save(&work, "prepared.json", &provenance)?;
    let observed = trial(root, &work, &iso, profile);
    let mut report = json!({
        "result": "Incomplete", "compiler_reconstruction": false, "multistage_bootstrap": false,
        "budgets": budgets, "prepared": provenance,
    });
    let observed = match observed {
        Ok(observed) => observed,
        Err(error) => {
            report["error"] = json!(error.to_string());
            save(&work, "result.json", &report)?;
            return Err(error);
        }
    };
    let outcome = &observed["outcome"];
    let diagnostic = profile == Profile::Entry
        && outcome["tree_c_rejected"] == true
        && outcome["recursive_probe"]["exit"]
            .as_u64()
            .is_some_and(|exit| exit > 0);
    let reconstructed = profile == Profile::Stage1 && outcome["compiler_reconstruction"] == true;
    if diagnostic {
        report["result"] = json!("Diagnostic");
        report["scope"] = json!("Original GCC build entry on a limited source subset");
    }
    if reconstructed {
        report["result"] = json!("Ok");
        report["scope"] = json!("GCC and G++ 11.2.0 first stage with inherited runtime inputs");
        report["compiler_reconstruction"] = json!(true);
    }
    let boundary = outcome["boundary"]
        .as_str()
        .unwrap_or("missing outcome")
        .to_string();
    report["rounds"] = json!([observed]);
    save(&work, "result.json", &report)?;
    let path = work.join("result.json");
    if !diagnostic && !reconstructed {
        return Err(format!(
            "M18 GCC {} incomplete ({boundary}); see {}",
            profile.name(),
            path.display()
        )
        .into());
    }
    println!(
        "Verified M18 GCC {} evidence: {}",
        profile.name(),
        path.display()
    );
    Ok(())
}

fn prepare(root: &Path, work: &Path, profile: Profile) -> Result<(PathBuf, Value)> {
    let source = match profile {
        Profile::Entry => work.join("source"),
        Profile::Stage1 => {
            let seed = work.join("disk-seed");
            fs::create_dir(&seed)?;
            fs::write(seed.join(".anova-abi"), b"linux.x86_64.v1\n")?;
            seed.join("m18-source")
        }
    };
    let mut prepare_source = Command::new("python3");
    prepare_source
        .arg(root.join("ports/bootstrap/m18_gcc_source.py"))
        .arg(&source);
    if profile == Profile::Stage1 {
        prepare_source.args(["--profile", "stage1"]);
    }
    command(&mut prepare_source)?;
    let source_receipt = fs::read(source.join("INPUTS.json"))?;
    command(Command::new("python3").arg(root.join("ports/bootstrap/m18_arithmetic.py")))?;
    apps::build(root, false)?;
    let mut archive = fs::read(toolchain::pack(root)?)?;
    let busybox = linux::append_bootstrap(root, &mut archive)?;
    let mut inputs = Files::new();
    if profile == Profile::Entry {
        bootstrap_kernel::collect(&source, "/m18-source/", &mut inputs)?;
    }
    bootstrap_kernel::collect(
        &root.join("build/m18-arithmetic/package"),
        "/m18-arithmetic/",
        &mut inputs,
    )?;
    for name in [
        "arithmetic.c",
        "gcc_configure.sh",
        "posix.sh",
        "gcc_stage1.sh",
        "stage1.c",
        "stage1.cpp",
    ] {
        inputs.insert(
            format!("/m18-gcc/{name}"),
            fs::read(root.join("tests/guest/m18").join(name))?,
        );
    }
    bootstrap_kernel::append_inputs(&mut archive, &inputs)?;
    let exec_fixture_sha256 = if profile == Profile::Stage1 {
        toolchain::append(&mut archive, 2, "/m18-gcc/tools", &[]);
        // GCC and collect2 search as and real-ld in the leading -B directory.
        for (path, target) in BINUTILS_ALIASES {
            toolchain::append(&mut archive, 3, path, target.as_bytes());
        }
        let fixture = work.join("exec-bounds.elf");
        command(
            Command::new("musl-gcc")
                .args([
                    "-std=c11",
                    "-O2",
                    "-static",
                    "-no-pie",
                    "-fcf-protection=none",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-Wl,--build-id=none",
                ])
                .arg(root.join("tests/guest/m18/exec_bounds.c"))
                .arg("-o")
                .arg(&fixture),
        )?;
        let bytes = fs::read(fixture)?;
        toolchain::append(&mut archive, 5, "/m18-gcc/exec-bounds", &bytes);
        Some(hash(&bytes))
    } else {
        None
    };
    let packed = work.join("initramfs.anovafs");
    fs::write(&packed, &archive)?;
    let archive_sha256 = hash(&archive);
    drop(archive);
    let iso = work.join("anova-devel.iso");
    fs::rename(image_archive(root, Mode::Foundation, Some(&packed))?, &iso)?;
    let mut implementation = BTreeMap::new();
    for path in [
        "xtask/src/m18_gcc.rs",
        "xtask/src/main.rs",
        "xtask/src/linux.rs",
        "xtask/src/foundation_journal.rs",
        "xtask/src/foundation_capacity.rs",
        "xtask/src/bootstrap_kernel.rs",
        "kernel/src/task/bootstrap.rs",
        "kernel/src/task/linux.rs",
        "kernel/src/task/linux/io.rs",
        "kernel/src/task/loader.rs",
        "kernel/src/task/mod.rs",
        "crates/abi/src/linux/strings.rs",
        "crates/elf/src/lib.rs",
        "crates/elf/src/stack.rs",
        "crates/storage/src/ext4.rs",
        "crates/storage/src/ext4/write.rs",
        "crates/vfs/src/lib.rs",
        "tests/guest/m18/exec_bounds.c",
        "ports/bootstrap/m18_gcc_source.py",
        "ports/bootstrap/m18_arithmetic.py",
        "user/linux/busybox-bootstrap.config",
    ] {
        implementation.insert(path, hash(&fs::read(root.join(path))?));
    }
    let provenance = json!({
        "source_branch": version("git", &["branch", "--show-current"])?,
        "source_commit": version("git", &["rev-parse", "HEAD"])?,
        "source_worktree": version("git", &["status", "--porcelain"])?,
        "implementation_files_sha256": implementation,
        "qemu_version": version("qemu-system-x86_64", &["--version"])?,
        "build_host_target": "x86_64-linux-musl using AnovaOS Linux compatibility for build and host",
        "initial_iso_sha256": hash(&fs::read(&iso)?), "initramfs_sha256": archive_sha256,
        "input_files": hashes(&inputs), "input_bytes": inputs.values().map(Vec::len).sum::<usize>(),
        "busybox": busybox,
        "gcc_stage_inventory_sha256": hash(&fs::read(root.join("build/toolchain/stage.sha256"))?),
        "gcc_package_inventory_sha256": hash(&fs::read(root.join("build/toolchain/package.sha256"))?),
        "gcc_source": serde_json::from_slice::<Value>(&source_receipt)?,
        "gcc_source_receipt_sha256": hash(&source_receipt),
        "gcc_source_medium": match profile { Profile::Entry => "initramfs", Profile::Stage1 => "seeded ext4" },
        "exec_bounds_fixture_sha256": exec_fixture_sha256,
        "stage1_binutils_aliases": (profile == Profile::Stage1)
            .then(|| BINUTILS_ALIASES.into_iter().collect::<BTreeMap<_, _>>()),
        "arithmetic": serde_json::from_slice::<Value>(&inputs["/m18-arithmetic/INPUTS.json"])?,
    });
    Ok((iso, provenance))
}

fn create_work_disk(root: &Path, work: &Path, disk: &Path, profile: Profile) -> Result<()> {
    match profile {
        Profile::Entry => foundation_journal::create(root, disk, true),
        Profile::Stage1 => foundation_journal::create_seeded(
            disk,
            &work.join("disk-seed"),
            profile.disk_blocks(),
            profile.disk_inodes(),
        ),
    }
}

fn trial(root: &Path, work: &Path, iso: &Path, profile: Profile) -> Result<Value> {
    let round = work.join("round-1");
    fs::create_dir(&round)?;
    let disk = round.join("work.ext4");
    create_work_disk(root, work, &disk, profile)?;
    let mut steps = vec![
        (
            "anova> ".into(),
            format!(
                "/bootstrap/sh /m18-gcc/gcc_configure.sh {}\r",
                profile.name()
            )
            .into_bytes(),
        ),
        (UART_WAIT.into(), b"probe\r".to_vec()),
        (UART_ACK.into(), Vec::new()),
        ("anova> ".into(), b"status\r".to_vec()),
        ("[SHELL:STATUS] code=0".into(), Vec::new()),
    ];
    checked(&mut steps, "hello-c", 7);
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
    let build = run(root, &round, iso, &disk, profile, "build", &steps);
    let first = inspect(&disk, &round, "build", true);
    let build = build?;
    let first = first?;
    let build_uart = fs::read_to_string(round.join("build.serial.log"))?;
    let outcome = match check_outputs(&first, &build_uart) {
        Ok(outcome) => outcome,
        Err(error) => json!({
            "compiler_reconstruction": false, "boundary": "validate retained guest data",
            "verification_error": error.to_string(),
        }),
    };
    steps.clear();
    checked(&mut steps, "cd /persist/m18", 0);
    checked(&mut steps, "./arithmetic", 0);
    checked(&mut steps, "hello-c", 7);
    if outcome["compiler_reconstruction"] == true {
        checked(
            &mut steps,
            "/bootstrap/sh /m18-gcc/gcc_stage1.sh recover",
            0,
        );
    }
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
    let recovery = run(root, &round, iso, &disk, profile, "reboot", &steps);
    let after_reboot = inspect(&disk, &round, "reboot", false);
    let recovery = recovery?;
    if first.artifacts != after_reboot?.artifacts {
        return Err("Observed GCC workspace files changed across reboot".into());
    }
    let reboot_uart = fs::read_to_string(round.join("reboot.serial.log"))?;
    if !build_uart.contains("[M18:POSIX] success")
        || profile == Profile::Stage1 && !build_uart.contains(EXEC_BOUNDS)
        || build_uart.matches(ARITHMETIC).count() != 2
        || reboot_uart.matches(ARITHMETIC).count() != 1
        || outcome["compiler_reconstruction"] == true && !reboot_uart.contains(RECOVERY)
    {
        return Err("Missing POSIX, seed arithmetic or rebuilt compiler reboot evidence".into());
    }
    let report = json!({
        "number": 1, "build": build, "reboot": recovery,
        "observed_files": first.artifacts,
        "outcome": outcome, "observed_files_reboot_bytes_identical": true,
        "seed_reboot_verified": true,
    });
    save(&round, "result.json", &report)?;
    Ok(report)
}

fn save(work: &Path, name: &str, data: &Value) -> Result<()> {
    fs::write(work.join(name), serde_json::to_vec_pretty(data)?)?;
    Ok(())
}

fn inspect(path: &Path, round: &Path, phase: &str, retain: bool) -> Result<Workspace> {
    let check = Command::new("e2fsck")
        .args(["-f", "-n"])
        .arg(path)
        .output()?;
    let mut log = File::create(round.join(format!("{phase}.e2fsck.log")))?;
    log.write_all(&check.stdout)?;
    log.write_all(&check.stderr)?;
    if !check.status.success() {
        return Err(format!("Stopped GCC disk failed e2fsck: {}", check.status).into());
    }
    let mut disk = Ext4::mount(Box::new(disk::Image::open(path)?))
        .map_err(|e| format!("Inspect M18 disk: {e}"))?;
    let mut workspace = Workspace {
        artifacts: BTreeMap::new(),
        retained: Files::new(),
    };
    for (name, export, readback) in OBSERVED_FILES {
        let Some(inode) = workspace_inode(&mut disk, name)? else {
            continue;
        };
        let info = disk
            .inode(inode)
            .map_err(|e| format!("Read M18 inode {name}: {e}"))?;
        if info.directory {
            return Err(format!("Expected a GCC workspace file at {name}").into());
        }
        let mut file = if retain && *export {
            let destination = round.join("outputs").join(name);
            fs::create_dir_all(destination.parent().ok_or("Missing output parent")?)?;
            Some(File::create(destination)?)
        } else {
            None
        };
        let mut data = Vec::new();
        let mut sha = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        let start = info.size.saturating_sub(*readback);
        for offset in (0..info.size).step_by(buffer.len()) {
            let count = (info.size - offset).min(buffer.len());
            let chunk = &mut buffer[..count];
            if disk
                .read(inode, offset, chunk)
                .map_err(|e| format!("Read {name}: {e}"))?
                != count
            {
                return Err(format!("Short GCC workspace read: {name}").into());
            }
            sha.update(&*chunk);
            if let Some(file) = &mut file {
                file.write_all(chunk)?;
            }
            if retain && offset + count > start {
                data.extend_from_slice(&chunk[start.saturating_sub(offset)..]);
            }
        }
        if retain && *readback != 0 {
            workspace.retained.insert((*name).into(), data);
        }
        workspace.artifacts.insert(
            *name,
            json!({
                "bytes": info.size, "sha256": format!("{:x}", sha.finalize()),
                "executable": info.executable,
            }),
        );
    }
    disk.shutdown()
        .map_err(|e| format!("Shutdown inspected M18 disk: {e}"))?;
    Ok(workspace)
}

fn workspace_inode(disk: &mut Ext4, path: &str) -> Result<Option<u32>> {
    let mut parent = disk.capabilities().root;
    for component in std::iter::once("m18").chain(path.split('/')) {
        let Some(entry) = disk
            .lookup(parent, component)
            .map_err(|e| format!("Lookup {path}: {e}"))?
        else {
            return Ok(None);
        };
        parent = entry.inode;
    }
    Ok(Some(parent))
}

fn diagnostic_tail(text: &str) -> Vec<&str> {
    let mut lines: Vec<_> = text.lines().rev().take(12).collect();
    lines.reverse();
    lines
}

fn check_outputs(workspace: &Workspace, serial: &str) -> Result<Value> {
    let files = &workspace.retained;
    let artifacts = &workspace.artifacts;
    let read = |name: &str| -> Result<&str> {
        Ok(std::str::from_utf8(
            files
                .get(name)
                .ok_or_else(|| format!("Missing guest output: {name}"))?,
        )?)
    };
    anova_elf::parse_linux(
        files
            .get("arithmetic")
            .ok_or("Missing guest arithmetic ELF")?,
    )
    .map_err(|e| format!("Invalid arithmetic ELF: {e:?}"))?;
    if artifacts.contains_key("rejected")
        || artifacts.contains_key("rejected.o")
        || !read("missing-header.log")?.contains("gmp.h: No such file or directory")
        || !read("missing-library.log")?.contains("undefined reference to `mpc_")
    {
        return Err("Missing original compiler and linker rejection".into());
    }
    let configure: u32 = read("configured/configure.exit")?.trim().parse()?;
    let mut outcome = json!({"compiler_reconstruction": false, "configure_exit": configure});
    if configure != 0 {
        outcome["boundary"] = json!("top-level configure");
        return Ok(outcome);
    }
    let output = read("configured/configure.stdout")?;
    for check in [
        "checking for the correct version of gmp.h... yes",
        "checking for the correct version of mpfr.h... yes",
        "checking for the correct version of mpc.h... yes",
        "checking for the correct version of the gmp/mpfr/mpc libraries... yes",
    ] {
        if !output.contains(check) {
            return Err(format!("Original configure did not pass: {check}").into());
        }
    }
    if !artifacts.contains_key("configured/Makefile") {
        return Err("Successful GCC configure omitted the top-level Makefile".into());
    }
    let make: u32 = read("configured/make.exit")?.trim().parse()?;
    outcome["make_exit"] = json!(make);
    if make != 0 {
        let log = String::from_utf8_lossy(
            files
                .get("configured/make.stdout")
                .ok_or("Missing actual make output")?,
        );
        let tree_missing = log.contains("cannot find sources (tree.c)");
        outcome["boundary"] = if tree_missing {
            json!("GCC subconfigure reached and rejected absent tree.c")
        } else {
            json!("original make all-gcc")
        };
        outcome["tree_c_rejected"] = json!(tree_missing);
        outcome["make_diagnostics"] = json!(diagnostic_tail(&log));
        if files.contains_key("configured/recursive-probe.exit") {
            let exit: u32 = read("configured/recursive-probe.exit")?.trim().parse()?;
            let diagnostics = String::from_utf8_lossy(
                files
                    .get("configured/recursive-probe.stdout")
                    .ok_or("Missing recursive probe output")?,
            );
            outcome["recursive_probe"] = json!({
                "scope": "Skipped configure-gcc prerequisite for the original recursive recipe diagnostic",
                "exit": exit,
                "diagnostics": diagnostic_tail(&diagnostics),
            });
        }
        return Ok(outcome);
    }
    let stage_status = files
        .get("configured/stage1.exit")
        .or_else(|| files.get("stage1-check/exit"))
        .map(|bytes| -> Result<u32> { Ok(std::str::from_utf8(bytes)?.trim().parse()?) })
        .transpose()?;
    outcome["stage1_verification_exit"] = json!(stage_status);
    if stage_status != Some(0) {
        outcome["boundary"] = json!("guest-built compiler verification");
        return Ok(outcome);
    }
    let driver = read("stage1-check/driver.log")?;
    if !artifacts.contains_key("configured/gcc/Makefile")
        || !["cc1", "cc1plus", "collect2", "xgcc", "xg++"]
            .iter()
            .all(|name| {
                artifacts
                    .get(format!("configured/gcc/{name}").as_str())
                    .is_some_and(|artifact| {
                        artifact["executable"] == true
                            && artifact["bytes"].as_u64().unwrap_or(0) > 0
                    })
            })
        || !driver.contains("cc1=/persist/m18/configured/gcc/cc1\n")
        || !driver.contains("cc1plus=/persist/m18/configured/gcc/cc1plus\n")
        || !driver.contains("as=/m18-gcc/tools/as\n")
        || !serial.contains(STAGE1)
        || read("stage1-check/c.out")? != "stage1-c=42\n"
        || read("stage1-check/cpp.out")? != "stage1-cpp=42\n"
        || !read("stage1-check/syntax.log")?.contains("m18-stage1-syntax")
        || !read("stage1-check/link.log")?.contains("undefined reference to `m18_stage1_missing")
        || artifacts.contains_key("stage1-check/rejected.o")
        || artifacts.contains_key("stage1-check/rejected.elf")
    {
        return Err("Guest compiler verification did not retain the expected proof".into());
    }
    for (fixture, frontend) in [("c", "cc1"), ("cpp", "cc1plus")] {
        let compile = read(&format!("stage1-check/{fixture}-compile.log"))?;
        let link = read(&format!("stage1-check/{fixture}-link.log"))?;
        if !compile.contains(&format!(" /persist/m18/configured/gcc/{frontend} "))
            || !compile.contains(" /m18-gcc/tools/as ")
            || !link.contains(" /persist/m18/configured/gcc/collect2 ")
            || !link.contains("ld_file_name        = /m18-gcc/tools/real-ld")
        {
            return Err(format!("Unexpected compiler or binutils selection for {fixture}").into());
        }
    }
    for name in ["stage1-check/c.elf", "stage1-check/cpp.elf"] {
        anova_elf::parse_linux(files.get(name).ok_or_else(|| format!("Missing {name}"))?)
            .map_err(|e| format!("Invalid {name}: {e:?}"))?;
    }
    outcome["compiler_reconstruction"] = json!(true);
    outcome["boundary"] = json!("compiled, linked and executed C and C++ with guest-built tools");
    Ok(outcome)
}

fn run(
    root: &Path,
    round: &Path,
    iso: &Path,
    path: &Path,
    profile: Profile,
    phase: &str,
    steps: &[(String, Vec<u8>)],
) -> Result<Value> {
    let work = round
        .parent()
        .and_then(Path::file_name)
        .ok_or("Missing run name")?
        .to_string_lossy();
    let name = format!(
        "{work}-{}-{phase}",
        round
            .file_name()
            .ok_or("Missing round name")?
            .to_string_lossy()
    );
    let exchanges: Vec<_> = steps
        .iter()
        .map(|(m, b)| (m.as_str(), b.as_slice()))
        .collect();
    let start = Instant::now();
    let result = disk::session_observed(
        root,
        iso,
        path,
        &name,
        false,
        None,
        &exchanges,
        true,
        profile.session_seconds(),
        false,
    );
    let wall = start.elapsed().as_millis();
    for suffix in ["serial.log", "qemu.log", "result.txt"] {
        let source = root.join(format!("build/logs/{name}.{suffix}"));
        if source.is_file() {
            fs::copy(source, round.join(format!("{phase}.{suffix}")))?;
        }
    }
    let timings = match result {
        Ok(timings) => timings,
        Err(error) => {
            save(
                round,
                &format!("{phase}.json"),
                &json!({
                    "host_wall_ms": wall, "session_error": error.to_string(),
                }),
            )?;
            return Err(error);
        }
    };
    let uart_ms = if phase == "build" {
        Some(timings[2] - timings[1])
    } else {
        None
    };
    let serial = fs::read_to_string(round.join(format!("{phase}.serial.log")))?;
    let resources = bootstrap::resource_summary(&serial)?;
    let storage = foundation_capacity::storage_summary_with_geometry(
        path,
        &serial,
        profile.disk_blocks(),
        profile.disk_inodes(),
    )?;
    let evidence = json!({"host_wall_ms": wall, "live_configure_uart_ms": uart_ms, "resources": resources, "disk": storage});
    save(round, &format!("{phase}.json"), &evidence)?;
    if let Some(elapsed) = uart_ms.filter(|elapsed| *elapsed > UART_MS) {
        return Err(
            format!("M18 GCC configure UART response exceeded {UART_MS} ms: {elapsed}").into(),
        );
    }
    if resources["peak_frame_delta"].parse::<u64>()? > 196608
        || resources["heap_peak_bytes"].parse::<u64>()? > 16777216
        || resources["vfs_node_slots_peak"].parse::<u64>()? > 2048
        || resources["occupied_slots_peak"].parse::<u64>()? > 32
        || resources["fd_per_process_peak"].parse::<u64>()? > 32
    {
        return Err("M18 GCC trial exceeded the declared resource budget".into());
    }
    Ok(evidence)
}
