// SPDX-License-Identifier: Apache-2.0
//! Build and independently verify the Rust-fork LLVM demangler on two fresh guest work disks.
//! Host work admits locked sources and seed tools, drives QEMU and inspects guest outputs.
use super::*;
use anova_storage::{ext4::Ext4, filesystem::Filesystem};
use bootstrap::{checked, hash, hashes, resource_summary};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Files = BTreeMap<String, Vec<u8>>;
const OBJECTS: [&str; 6] = [
    "DLangDemangle",
    "Demangle",
    "ItaniumDemangle",
    "MicrosoftDemangle",
    "MicrosoftDemangleNodes",
    "RustDemangle",
];
const ARTIFACTS: [&str; 2] = ["libLLVMDemangle.a", "demangle"];
const ACTIVE: &str = "[M18:LLVM:ACTIVE] translation-unit=ItaniumDemangle";
const PONG: &str = "[M18:LLVM:PONG] compiler-live=true";
const SUCCESS: &str =
    "[M18:LLVM:DEMANGLE] itanium=ok microsoft=ok rust-v0=ok d=ok invalid=rejected";
const BUILT: &str = "[M18:LLVM:BUILT] objects=6";
const RECOVERED: &str =
    "[M18:LLVM:RECOVERY] syntax=1 link=1 archive-linked=true prior-executed=true";
const SESSION_SECONDS: u64 = 900;
const RESPONSE_MS: u128 = 5_000;
const LIMITS: [(&str, u64); 5] = [
    ("heap_peak_bytes", 16 * 1024 * 1024),
    ("peak_frame_delta", 196_608),
    ("vfs_node_slots_peak", 2_048),
    ("occupied_slots_peak", 32),
    ("fd_per_process_peak", 32),
];
const SOURCE_FIXTURES: [(&str, &str); 2] = [
    ("tests/guest/m18/llvm_demangle.cpp", "check.cpp"),
    ("tests/guest/m18/llvm_demangle.sh", "build.sh"),
];

fn inspect(disk_path: &Path) -> Result<Files> {
    let mut disk = Ext4::mount(Box::new(disk::Image::open(disk_path)?))
        .map_err(|error| format!("LLVM work disk inspection: {error}"))?;
    let workspace = disk
        .lookup(2, "llvm-demangle")
        .map_err(|error| format!("LLVM work directory lookup: {error}"))?
        .ok_or("Missing persisted LLVM work directory")?
        .inode;
    let mut files = Files::new();
    bootstrap_kernel::read_files(&mut disk, workspace, "", &mut files, 0)?;
    if files.get("retained.txt").map(Vec::as_slice) != Some(b"LLVM build workspace data\n") {
        return Err("Missing or changed LLVM work data".into());
    }
    let binary = files
        .get("demangle")
        .ok_or("Missing guest LLVM executable")?;
    anova_elf::parse_linux(binary)
        .map_err(|error| format!("Guest LLVM fixture ELF rejected: {error:?}"))?;
    let executable = disk
        .lookup(workspace, "demangle")
        .map_err(|error| format!("LLVM fixture lookup: {error}"))?
        .ok_or("Missing persisted LLVM fixture inode")?
        .inode;
    if binary.get(16..18) != Some(b"\x02\0")
        || anova_elf::interpreter(binary)
            .map_err(|error| format!("LLVM fixture interpreter: {error:?}"))?
            .is_some()
        || !disk
            .inode(executable)
            .map_err(|error| format!("LLVM fixture metadata: {error}"))?
            .executable
    {
        return Err("Guest LLVM fixture is not a static executable ET_EXEC".into());
    }
    disk.shutdown()
        .map_err(|error| format!("LLVM work disk shutdown: {error}"))?;
    foundation_journal::check(disk_path)?;
    Ok(files)
}

fn verify_outputs(work: &Path, files: &Files, sources: &Value) -> Result<BTreeSet<String>> {
    let mut compiled_sources = BTreeSet::new();
    let expected_sources: BTreeSet<_> = sources["files_sha256"]
        .as_object()
        .ok_or("Missing LLVM source manifest inventory")?
        .keys()
        .filter(|path| path.starts_with("include/") || path.ends_with(".cpp"))
        .cloned()
        .collect();
    for unit in OBJECTS {
        let object = files
            .get(&format!("{unit}.o"))
            .ok_or("Missing LLVM object")?;
        if object.get(..4) != Some(b"\x7fELF") || object.get(16..18) != Some(b"\x01\0") {
            return Err(format!("LLVM object is not relocatable: {unit}").into());
        }
        let deps = files
            .get(&format!("{unit}.d"))
            .ok_or("Missing compiler dependency record")?;
        for path in std::str::from_utf8(deps)?.split_whitespace().skip(1) {
            if path == "\\" {
                continue;
            }
            let path = path
                .strip_prefix("/m18-llvm/")
                .ok_or("LLVM compiler used an undeclared non-system build input")?;
            compiled_sources.insert(path.to_owned());
        }
    }
    if compiled_sources != expected_sources {
        return Err("Observed non-system LLVM inputs differ from the locked build closure".into());
    }
    for name in ARTIFACTS {
        let path = work.join(name);
        output_path(&path)?;
        fs::write(&path, files.get(name).ok_or("Missing LLVM build artifact")?)?;
    }
    let archive = work.join(ARTIFACTS[0]);
    let output = Command::new("ar").arg("t").arg(&archive).output()?;
    if !output.status.success()
        || String::from_utf8(output.stdout)?
            .lines()
            .ne(OBJECTS.map(|unit| format!("{unit}.o")))
    {
        return Err("Guest archive member list differs from the upstream target".into());
    }
    for unit in OBJECTS {
        let object = format!("{unit}.o");
        let extracted = Command::new("ar")
            .arg("p")
            .arg(&archive)
            .arg(&object)
            .output()?;
        if !extracted.status.success() || extracted.stdout != files[&object] {
            return Err(format!("Guest archive member bytes differ: {object}").into());
        }
    }
    Ok(compiled_sources)
}

fn session(root: &Path, iso: &Path, disk_path: &Path, name: &str, build: bool) -> Result<Value> {
    let mut steps = Vec::new();
    let first = if build { "build" } else { "recover" };
    checked(
        &mut steps,
        &format!("/bootstrap/sh /m18-llvm/build.sh {first}"),
        0,
    );
    if build {
        steps.insert(1, (ACTIVE.into(), b"ping\n".to_vec()));
        steps.insert(2, (PONG.into(), Vec::new()));
    }
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
    let exchanges: Vec<_> = steps
        .iter()
        .map(|(m, b)| (m.as_str(), b.as_slice()))
        .collect();
    let start = Instant::now();
    let timestamps = disk::session_observed(
        root,
        iso,
        disk_path,
        name,
        false,
        None,
        &exchanges,
        true,
        SESSION_SECONDS,
        false,
    )?;
    let wall_ms = start.elapsed().as_millis();
    let serial_path = root.join(format!("build/logs/{name}.serial.log"));
    let qemu_path = root.join(format!("build/logs/{name}.qemu.log"));
    let result_path = root.join(format!("build/logs/{name}.result.txt"));
    let serial = fs::read_to_string(&serial_path)?;
    let expected = if build { BUILT } else { RECOVERED };
    if serial.contains("[M18:LLVM:FAIL]") || !serial.contains(expected) || !serial.contains(SUCCESS)
    {
        return Err(format!("Incomplete LLVM guest evidence: {}", serial_path.display()).into());
    }
    let response = if build {
        let elapsed = timestamps[2] - timestamps[1];
        if elapsed > RESPONSE_MS {
            return Err(
                format!("LLVM live-compiler UART response exceeded budget: {elapsed} ms").into(),
            );
        }
        Some(elapsed)
    } else {
        None
    };
    let resources = resource_summary(&serial)?;
    for (metric, limit) in LIMITS {
        if resources[metric].parse::<u64>()? > limit {
            return Err(format!("LLVM build exceeded sampled {metric} budget").into());
        }
    }
    Ok(json!({
        "serial": serial_path,
        "serial_sha256": hash(serial.as_bytes()),
        "qemu_log": qemu_path,
        "qemu_log_sha256": hash(&fs::read(&qemu_path)?),
        "session_result": result_path,
        "session_result_sha256": hash(&fs::read(&result_path)?),
        "host_wall_ms_including_boot_and_shutdown": wall_ms,
        "live_compiler_uart_roundtrip_ms": response,
        "resources": resources,
        "storage": foundation_capacity::storage_summary(disk_path, &serial)?,
    }))
}

fn round(
    root: &Path,
    iso: &Path,
    work: &Path,
    name: &str,
    sources: &Value,
) -> Result<(Value, Files)> {
    fs::create_dir(work)?;
    let disk_path = work.join("work.ext4");
    foundation_journal::create(root, &disk_path, true)?;
    let build = session(root, iso, &disk_path, &format!("{name}-build"), true)?;
    let first = inspect(&disk_path)?;
    let compiled_sources = verify_outputs(work, &first, sources)?;
    let reboot = session(root, iso, &disk_path, &format!("{name}-reboot"), false)?;
    let mut after = inspect(&disk_path)?;
    let syntax = after
        .remove("syntax.log")
        .ok_or("Missing rejected C++ compiler log")?;
    let link = after
        .remove("link.log")
        .ok_or("Missing rejected LLVM link log")?;
    if !std::str::from_utf8(&syntax)?.contains("error: expected primary-expression")
        || !std::str::from_utf8(&link)?.contains("undefined reference")
        || !std::str::from_utf8(&link)?.contains("llvm::demangle")
    {
        return Err("Unexpected rejection diagnostics in LLVM reboot test".into());
    }
    fs::write(work.join("syntax.log"), &syntax)?;
    fs::write(work.join("link.log"), &link)?;
    if after.remove("recovered").as_ref() != first.get("demangle") || first != after {
        return Err("LLVM work files changed across failed/recovery operations and reboot".into());
    }
    let artifacts = ARTIFACTS
        .map(|name| (name.to_owned(), first[name].clone()))
        .into_iter()
        .collect();
    let report = json!({
        "disk": disk_path,
        "build": build,
        "reboot": reboot,
        "observed_non_system_build_inputs": compiled_sources,
        "work_files_sha256": hashes(&first),
        "retained_work_bytes": first.values().map(Vec::len).sum::<usize>(),
        "largest_work_file_bytes": first.values().map(Vec::len).max(),
        "artifact_sha256": hashes(&artifacts),
        "syntax_log_sha256": hash(&syntax),
        "link_log_sha256": hash(&link),
        "exact_artifact_and_work_file_retention": true,
        "e2fsck": "clean after both boots",
    });
    fs::write(
        work.join("result.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok((report, artifacts))
}

pub fn verify(root: &Path) -> Result<()> {
    command(Command::new("python3").arg(root.join("ports/bootstrap/m18_llvm.py")))?;
    apps::build(root, false)?;
    let mut archive = fs::read(toolchain::pack(root)?)?;
    let busybox = linux::append_bootstrap(root, &mut archive)?;
    let base = directory(root, "m18-llvm")?;
    let source = base.join("source");
    let manifest: Value = serde_json::from_slice(&fs::read(source.join("INPUTS.json"))?)?;
    let mut files = Files::new();
    bootstrap_kernel::collect(&source, "/m18-llvm/", &mut files)?;
    for (path, name) in SOURCE_FIXTURES {
        files.insert(format!("/m18-llvm/{name}"), fs::read(root.join(path))?);
    }
    files.insert(
        "/m18-llvm/bad.cpp".into(),
        b"// Deliberately invalid expression to verify failed compilation.\nint broken = ;\n"
            .to_vec(),
    );
    bootstrap_kernel::append_inputs(&mut archive, &files)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let work = base.join(format!("run-{stamp}"));
    fs::create_dir(&work)?;
    let packed = work.join("initramfs.anovafs");
    fs::write(&packed, &archive)?;
    let initramfs_entries = u32::from_le_bytes(archive[8..12].try_into()?);
    let archive_sha256 = hash(&archive);
    drop(archive);
    let built = image_archive(root, Mode::Foundation, Some(&packed))?;
    let iso = work.join("anova-m18-llvm.iso");
    fs::copy(built, &iso)?;
    let identity: Files = [
        "xtask/src/m18_llvm.rs",
        "xtask/src/main.rs",
        "xtask/src/linux.rs",
        "ports/bootstrap/m18_llvm.py",
        "ports/bootstrap/rust_probe.py",
        "user/linux/busybox-bootstrap.config",
    ]
    .into_iter()
    .chain(SOURCE_FIXTURES.map(|(path, _)| path))
    .map(|path| Ok((path.into(), fs::read(root.join(path))?)))
    .collect::<Result<_>>()?;
    let prepared = json!({
        "profile": "LLVMDemangle 20.1.8 Rust fork standalone guest reconstruction",
        "build_host_target": "AnovaOS Linux x86_64 musl compatibility personality",
        "source": manifest,
        "admitted_inputs_sha256": hashes(&files),
        "admitted_input_count": files.len(),
        "admitted_input_bytes": files.values().map(Vec::len).sum::<usize>(),
        "source_commit": version("git", &["rev-parse", "HEAD"])?,
        "source_branch": version("git", &["branch", "--show-current"])?,
        "source_worktree": version("git", &["status", "--porcelain"])?,
        "implementation_sha256": hashes(&identity),
        "seed": "Inherited host-built GCC/G++ 11.2.0, binutils 2.37, musl 1.2.4",
        "seed_source_inventory_sha256": hash(&fs::read(root.join("build/toolchain/stage.sha256"))?),
        "seed_package_inventory_sha256": hash(&fs::read(root.join("build/toolchain/package.sha256"))?),
        "busybox": busybox,
        "host_participation": "Select locked inputs, prepare seeds, build the launcher ISO, drive QEMU and inspect; no guest LLVM artifact was built on the host for this gate.",
        "initramfs_entries": initramfs_entries, "initramfs_sha256": archive_sha256,
        "iso": iso, "iso_sha256": hash(&fs::read(&iso)?),
        "budgets": {
            "disk_bytes": 64 * 1024 * 1024, "disk_blocks": 16_384, "disk_inodes": 4_096,
            "internal_journal_bytes": 8 * 1024 * 1024, "tmpfs_file_bytes": 1024 * 1024,
            "sampled_limits": BTreeMap::from(LIMITS), "session_seconds": SESSION_SECONDS,
            "uart_response_ms": RESPONSE_MS, "guest_ram_bytes": 3 * 1024 * 1024 * 1024u64,
            "cpus": 1, "accelerator": "tcg", "jobs": 1, "network": false,
        },
        "measurement_scope": "Successful-mutation ext4 block/inode peaks and scheduler-boundary resource samples; neither exact RSS nor uncommitted transaction storage.",
    });
    fs::write(
        work.join("prepared.json"),
        serde_json::to_vec_pretty(&prepared)?,
    )?;
    let (round_a, artifacts_a) = round(
        root,
        &iso,
        &work.join("round-a"),
        &format!("m18-llvm-{stamp}-a"),
        &manifest,
    )?;
    let (round_b, artifacts_b) = round(
        root,
        &iso,
        &work.join("round-b"),
        &format!("m18-llvm-{stamp}-b"),
        &manifest,
    )?;
    let identical = artifacts_a == artifacts_b;
    let report = json!({
        "result": if identical { "ok" } else { "artifact bytes differ" },
        "prepared": prepared,
        "rounds": [round_a, round_b],
        "identical_artifacts": identical,
        "workspace": work,
        "scope": "Standalone LLVM demangling library; LLVM backend, rustc, Cargo, std and linker reconstruction are separate.",
    });
    fs::write(
        work.join("result.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    if !identical {
        return Err(format!("Repeated LLVM artifact bytes differ: {}", work.display()).into());
    }
    println!(
        "Verified guest LLVMDemangle reconstruction and repeat/recovery: {}",
        work.display()
    );
    Ok(())
}
