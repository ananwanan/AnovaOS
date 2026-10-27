// SPDX-License-Identifier: Apache-2.0
//! Reconstruct the actual kernel in guest processes and boot its retained ELF without rebuilding it.
//! Host-assisted image packaging and inherited compiler/target libraries are recorded separately.
//! The opt-in M16 profile uses fresh JBD2 storage and separate pressure/build measurements.
use super::*;
use bootstrap::{checked, hash, hashes, mount, session};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Files = BTreeMap<String, Vec<u8>>;

fn collect(base: &Path, prefix: &str, files: &mut Files) -> Result<()> {
    let mut entries = fs::read_dir(base)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Non-UTF8 kernel input")?;
        let key = format!("{prefix}{name}");
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect(&entry.path(), &format!("{key}/"), files)?;
        } else if kind.is_file() {
            checked_file(&entry.path())?;
            if files.insert(key.clone(), fs::read(entry.path())?).is_some() {
                return Err(format!("Duplicate kernel input: {key}").into());
            }
        } else {
            return Err("Kernel inputs must be regular files and directories".into());
        }
    }
    Ok(())
}

fn append_inputs(archive: &mut Vec<u8>, files: &Files) -> Result<()> {
    let mut names = BTreeSet::new();
    let mut parsed = anova_vfs::archive::Archive::new(archive)
        .map_err(|error| format!("Invalid seed archive: {error}"))?;
    while let Some(entry) = parsed
        .entry()
        .map_err(|error| format!("Seed entry: {error}"))?
    {
        names.insert(entry.path.to_string());
    }
    for (name, bytes) in files {
        let mut parent = String::new();
        let parts: Vec<_> = name.split('/').collect();
        if !name.starts_with('/')
            || parts
                .iter()
                .skip(1)
                .any(|part| part.is_empty() || *part == "." || *part == "..")
        {
            return Err("Invalid guest kernel input path".into());
        }
        for part in &parts[1..parts.len() - 1] {
            parent.push('/');
            parent.push_str(part);
            if names.insert(parent.clone()) {
                toolchain::append(archive, 2, &parent, &[]);
            }
        }
        if !names.insert(name.clone()) {
            return Err(format!("Kernel input replaces a seed entry: {name}").into());
        }
        toolchain::append(archive, 1, name, bytes);
    }
    // Validate entry counts, path lengths and component bounds before starting QEMU.
    let mut parsed = anova_vfs::archive::Archive::new(archive)
        .map_err(|error| format!("Kernel archive limit: {error}"))?;
    while parsed
        .entry()
        .map_err(|error| format!("Kernel archive entry: {error}"))?
        .is_some()
    {}
    Ok(())
}

fn inspect(path: &Path, ext4: bool) -> Result<Files> {
    let mut disk: Box<dyn anova_storage::filesystem::Filesystem> = if ext4 {
        Box::new(
            anova_storage::ext4::Ext4::mount(Box::new(disk::Image::open(path)?))
                .map_err(|e| format!("Kernel ext4 inspection: {e}"))?,
        )
    } else {
        Box::new(mount(path)?)
    };
    let parent = disk
        .lookup(2, "kernel")
        .map_err(|e| format!("Kernel root: {e}"))?
        .ok_or("Missing persistent kernel workspace")?
        .inode;
    let mut files = Files::new();
    read_files(&mut *disk, parent, "", &mut files, 0)?;
    disk.shutdown()
        .map_err(|e| format!("Kernel disk shutdown: {e}"))?;
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(path))?;
    let kernel = files.get("kernel.elf").ok_or("Missing guest kernel ELF")?;
    validate_elf(kernel)?;
    if files.contains_key("kernel.candidate") {
        return Err("Unpublished kernel candidate remains".into());
    }
    Ok(files)
}

fn read_files(
    disk: &mut dyn anova_storage::filesystem::Filesystem,
    parent: u32,
    prefix: &str,
    files: &mut Files,
    depth: usize,
) -> Result<()> {
    if depth > 64 || files.len() > 16384 {
        return Err("Kernel output inventory exceeds its bound".into());
    }
    let mut cursor = 0;
    while let Some((entry, next)) = disk
        .next_entry(parent, cursor)
        .map_err(|e| format!("Kernel output directory: {e}"))?
    {
        if next <= cursor {
            return Err("Invalid kernel output directory cursor".into());
        }
        cursor = next;
        let name = std::str::from_utf8(&entry.name[..entry.length])?;
        if matches!(name, "." | "..") {
            continue;
        }
        let path = format!("{prefix}{name}");
        let info = disk
            .inode(entry.inode)
            .map_err(|e| format!("Kernel output inode: {e}"))?;
        if info.directory {
            read_files(disk, entry.inode, &format!("{path}/"), files, depth + 1)?;
        } else {
            let mut bytes = vec![0; info.size];
            if disk
                .read(entry.inode, 0, &mut bytes)
                .map_err(|e| format!("Kernel output read: {e}"))?
                != bytes.len()
            {
                return Err("Short kernel output read".into());
            }
            files.insert(path, bytes);
        }
    }
    Ok(())
}

fn verify_packaged(root: &Path, iso: &Path, expected: &[u8]) -> Result<()> {
    let temporary = Temporary::new(root, "kernel-extract")?;
    let extracted = temporary.0.join("kernel.elf");
    command(
        Command::new("xorriso")
            .args(["-osirrox", "on", "-indev"])
            .arg(iso)
            .args(["-extract", "/boot/kernel.elf"])
            .arg(&extracted),
    )?;
    if fs::read(extracted)? != expected {
        return Err("Boot image does not contain the retained guest kernel bytes".into());
    }
    temporary.cleanup()
}

fn boot_identity(root: &Path, name: &str, identity: &str) -> Result<()> {
    let serial = fs::read_to_string(root.join(format!("build/logs/{name}.serial.log")))?;
    let marker = format!("[BOOT:BUILD] id={identity}");
    let identities: Vec<_> = serial
        .lines()
        .filter(|line| line.starts_with("[BOOT:BUILD]"))
        .collect();
    if identities.len() != 1 || identities[0].trim_end() != marker {
        return Err("Reboot did not establish the exact guest-built kernel identity".into());
    }
    for marker in ["[BOOT:ENTRY]", "[MM:READY]", "[CPU:TIMER]", "[DISK:MOUNT]"] {
        if !serial.contains(marker) {
            return Err(format!("Guest-built kernel lacks boot evidence: {marker}").into());
        }
    }
    Ok(())
}

pub fn verify(root: &Path) -> Result<()> {
    verify_profile(root, false)
}

pub(super) fn verify_profile(root: &Path, ext4: bool) -> Result<()> {
    let implementation_identity = foundation::source_identity(root)?;
    let result = directory(root, "logs")?.join(if ext4 {
        "foundation-capacity.result.json"
    } else {
        "bootstrap-kernel.result.json"
    });
    output_path(&result)?;
    if result.exists() {
        checked_file(&result)?;
        fs::remove_file(&result)?;
    }
    command(Command::new("python3").arg(root.join("ports/bootstrap/kernel_prepare.py")))?;
    let source_work = directory(root, "bootstrap-kernel")?;
    let inputs: Value = serde_json::from_slice(&fs::read(source_work.join("inputs.json"))?)?;
    let mut files = Files::new();
    collect(&source_work.join("source"), "/ks/source/", &mut files)?;
    collect(&source_work.join("seeds"), "/", &mut files)?;
    let work = if ext4 {
        let work = directory(root, "foundation")?.join(format!(
            "capacity-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir(&work)?;
        fs::write(
            work.join("budgets.json"),
            serde_json::to_vec_pretty(&foundation_capacity::budgets())?,
        )?;
        fs::copy(source_work.join("inputs.json"), work.join("inputs.json"))?;
        work
    } else {
        source_work
    };
    let input_hashes = hashes(&files);
    let mut declared = BTreeMap::new();
    for (key, prefix) in [
        ("source_files_sha256", "/ks/source/"),
        ("seeds_files_sha256", "/"),
    ] {
        let entries: BTreeMap<String, String> = serde_json::from_value(inputs[key].clone())?;
        for (name, digest) in entries {
            declared.insert(format!("{prefix}{name}"), digest);
        }
    }
    if declared != input_hashes {
        return Err("Prepared kernel inventory differs from the packaged input bytes".into());
    }
    let driver_source = fs::read(root.join("ports/bootstrap/kernel-build.c"))?;
    let mut implementation = Files::new();
    for name in [
        "xtask/src/bootstrap_kernel.rs",
        "xtask/src/bootstrap_repeat.rs",
        "xtask/src/main.rs",
        "xtask/src/bootstrap.rs",
        "xtask/src/rust.rs",
        "user/linux/rust.c",
        "ports/bootstrap/kernel-build.c",
        "tests/guest/m15/capacity.c",
    ] {
        implementation.insert(name.into(), fs::read(root.join(name))?);
    }
    if ext4 {
        for name in [
            "xtask/src/foundation_capacity.rs",
            "tests/guest/m16/capacity.c",
        ] {
            implementation.insert(name.into(), fs::read(root.join(name))?);
        }
    }
    let implementation_hashes = hashes(&implementation);
    let profile = if ext4 {
        "release-opt-z-ext4-write-bootstrap-metrics-v1"
    } else {
        "release-opt-z-bootstrap-metrics-v1"
    };
    let identity = hash(&serde_json::to_vec(&json!({
        "inputs": input_hashes, "driver_sha256": hash(&driver_source),
        "implementation_files_sha256": implementation_hashes,
        "rust_seed_inventory_sha256": hash(&fs::read(root.join("build/rust/stage.sha256"))?),
        "profile": profile,
    }))?);
    let mut archive = fs::read(rust::pack(root, false)?)?;
    append_inputs(&mut archive, &files)?;
    let diagnostic = work.join("kernel-build.elf");
    output_path(&diagnostic)?;
    let mut compiler = Command::new("musl-gcc");
    if ext4 {
        compiler.arg("-DANOVA_EXT4");
    }
    command(
        compiler
            .args([
                "-O2",
                "-static",
                "-no-pie",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-Wl,--build-id=none",
            ])
            .arg(root.join("ports/bootstrap/kernel-build.c"))
            .arg("-o")
            .arg(&diagnostic),
    )?;
    let diagnostic_bytes = fs::read(&diagnostic)?;
    anova_elf::parse_linux(&diagnostic_bytes)
        .map_err(|e| format!("Kernel build driver ELF: {e:?}"))?;
    toolchain::append(&mut archive, 5, "/bin/m15-kernel-build", &diagnostic_bytes);
    let capacity = work.join("capacity.elf");
    output_path(&capacity)?;
    command(
        Command::new("musl-gcc")
            .args([
                "-O2",
                "-static",
                "-no-pie",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-Wl,--build-id=none",
            ])
            .arg(root.join("tests/guest/m15/capacity.c"))
            .arg("-o")
            .arg(&capacity),
    )?;
    let capacity_bytes = fs::read(capacity)?;
    anova_elf::parse_linux(&capacity_bytes).map_err(|e| format!("Kernel capacity ELF: {e:?}"))?;
    toolchain::append(&mut archive, 5, "/bin/m15-capacity", &capacity_bytes);
    if ext4 {
        let probe = work.join("vfs-capacity.elf");
        command(
            Command::new("musl-gcc")
                .args([
                    "-O2",
                    "-static",
                    "-no-pie",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-Wl,--build-id=none",
                ])
                .arg(root.join("tests/guest/m16/capacity.c"))
                .arg("-o")
                .arg(&probe),
        )?;
        toolchain::append(&mut archive, 5, "/bin/m16-capacity", &fs::read(probe)?);
    }
    let packed = work.join("initramfs.anovafs");
    output_path(&packed)?;
    fs::write(&packed, &archive)?;
    let initial = image_archive(
        root,
        if ext4 {
            Mode::Foundation
        } else {
            Mode::Bootstrap
        },
        Some(&packed),
    )?;
    let seed_iso = if ext4 {
        work.clone()
    } else {
        directory(root, "images")?
    }
    .join("anova-bootstrap-kernel-seed.iso");
    output_path(&seed_iso)?;
    fs::rename(initial, &seed_iso)?;
    let seed_kernel_hash = hash(&fs::read(
        root.join("target").join(TARGET).join("debug/anova-kernel"),
    )?);
    let disk_path = if ext4 {
        work.join("work.ext4")
    } else {
        directory(root, "disks")?.join(format!(
            "bootstrap-kernel-{}.ext2",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ))
    };
    if ext4 {
        foundation_journal::create(root, &disk_path, true)?;
    } else {
        disk::create_profile(root, &disk_path, true)?;
    }
    let mut initial_disk =
        anova_storage::filesystem::mount(Box::new(disk::Image::open(&disk_path)?))
            .map_err(|e| format!("Fresh kernel disk: {e}"))?;
    let has_workspace = initial_disk
        .lookup(2, "kernel")
        .map_err(|e| format!("Initial kernel disk: {e}"))?
        .is_some();
    initial_disk
        .shutdown()
        .map_err(|e| format!("Initial kernel disk shutdown: {e}"))?;
    if has_workspace {
        return Err("Fresh kernel disk contains a previous build workspace".into());
    }
    let provenance = json!({
        "source_branch": version("git", &["branch", "--show-current"] )?,
        "source_commit": version("git", &["rev-parse", "HEAD"] )?,
        "source_worktree": version("git", &["status", "--porcelain"] )?,
        "source_preparation": inputs, "input_files_sha256": input_hashes,
        "implementation_files_sha256": implementation_hashes,
        "build_identity": identity, "driver_source_sha256": hash(&driver_source),
        "host_compiled_driver_sha256": hash(&diagnostic_bytes),
        "host_compiled_capacity_sha256": hash(&capacity_bytes),
        "seed_kernel_sha256": seed_kernel_hash, "seed_iso": seed_iso,
        "seed_iso_sha256": hash(&fs::read(&seed_iso)?),
        "rust_seed_inventory_sha256": hash(&fs::read(root.join("build/rust/stage.sha256"))?),
        "initramfs_sha256": hash(&archive), "work_disk": disk_path,
        "build_host": "AnovaOS x86_64 Linux-musl compatibility personality",
        "target": TARGET, "profile": profile, "ext4_work_disk": ext4,
        "implementation_sha256": implementation_identity,
        "input_file_count": files.len(),
        "input_bytes": files.values().map(Vec::len).sum::<usize>(),
        "largest_input_bytes": files.values().map(Vec::len).max(),
        "seed": "Inherited official Rust/Cargo/rust-lld 1.90.0, precompiled core/alloc/compiler_builtins",
        "host_participation": "Lock and prepare sources/seeds, compile driver/initial kernel/apps, create disk, package and independently inspect exact guest ELF; no guest kernel compilation on host",
        "image_generation_in_guest": false,
        "ram_bytes": 3 * 1024 * 1024 * 1024u64, "cpus": 1, "network": false,
    });
    let prepared = work.join("prepared.json");
    output_path(&prepared)?;
    fs::write(prepared, serde_json::to_vec_pretty(&provenance)?)?;
    let mut steps = Vec::new();
    let pressure = if ext4 {
        checked(&mut steps, "fs-test foundation-oom", 0);
        checked(&mut steps, "m15-capacity /persist/m16-capacity ext4", 0);
        checked(&mut steps, "m16-capacity", 0);
        checked(&mut steps, "ext4-write-test full", 0);
        checked(&mut steps, "hello-c", 7);
        steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
        let pressure = foundation_capacity::session(
            root,
            &work,
            &seed_iso,
            &disk_path,
            "foundation-capacity-pressure",
            &steps,
        )?;
        let serial = fs::read_to_string(work.join("foundation-capacity-pressure.serial.log"))?;
        for marker in [
            "[M16:OOM] heap=exhausted",
            "[M16:VFS]",
            "[M16:ENOSPC]",
            "[M15:CAPACITY] files=efbig-preserved",
        ] {
            if !serial.contains(marker) {
                return Err(format!("Missing resource exhaustion: {marker}").into());
            }
        }
        for key in [
            "occupied_slots_peak",
            "fd_per_process_peak",
            "vfs_node_slots_peak",
        ] {
            let expected = if key == "vfs_node_slots_peak" {
                "2048"
            } else {
                "32"
            };
            if pressure["resources"][key] != expected {
                return Err(format!("Resource limit was not reached: {key}").into());
            }
        }
        steps.clear();
        Some(pressure)
    } else {
        None
    };
    checked(&mut steps, "rustc --version", 0);
    checked(&mut steps, "cargo --version", 0);
    checked(&mut steps, &format!("m15-kernel-build build {identity}"), 0);
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
    let run = |iso: &Path, name: &str, steps: &[(String, Vec<u8>)]| -> Result<Value> {
        if ext4 {
            foundation_capacity::session(root, &work, iso, &disk_path, name, steps)
        } else {
            session(root, iso, &disk_path, name, steps)
        }
    };
    let construction = run(&seed_iso, "bootstrap-kernel-build", &steps)?;
    let first = inspect(&disk_path, ext4)?;
    let kernel = &first["kernel.elf"];
    if !kernel
        .windows(identity.len())
        .any(|window| window == identity.as_bytes())
    {
        return Err("Guest-built ELF does not contain the build identity".into());
    }
    let kernel_hash = hash(kernel);
    if kernel_hash == seed_kernel_hash {
        return Err("Guest kernel unexpectedly equals the host seed kernel".into());
    }
    let retained = work.join("guest-kernel.elf");
    output_path(&retained)?;
    fs::write(&retained, kernel)?;
    let built_iso = image_preserved(root, &retained, &packed, "bootstrap-kernel-guest")?;
    let guest_iso = if ext4 {
        let path = work.join("anova-bootstrap-kernel-guest.iso");
        fs::copy(built_iso, &path)?;
        path
    } else {
        built_iso
    };
    verify_packaged(root, &guest_iso, kernel)?;
    steps.clear();
    checked(&mut steps, "rustc --version", 0);
    if !ext4 {
        checked(&mut steps, "m15-capacity", 0);
    }
    checked(
        &mut steps,
        &format!("m15-kernel-build failure {identity}"),
        0,
    );
    checked(&mut steps, "hello-c", 7);
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
    let reboot = run(&guest_iso, "bootstrap-kernel-reboot", &steps)?;
    boot_identity(root, "bootstrap-kernel-reboot", &identity)?;
    let second = inspect(&disk_path, ext4)?;
    if second.get("kernel.elf") != Some(kernel) {
        return Err("Compiler failure changed the previously bootable guest kernel".into());
    }
    let failure = std::str::from_utf8(
        second
            .get("failure.log")
            .ok_or("Missing real compiler failure log")?,
    )?;
    if !failure.contains("expected expression") {
        return Err("Missing actual rustc syntax rejection".into());
    }
    steps.clear();
    checked(&mut steps, "hello-c", 7);
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
    let recovery = run(&guest_iso, "bootstrap-kernel-recovery", &steps)?;
    boot_identity(root, "bootstrap-kernel-recovery", &identity)?;
    let third = inspect(&disk_path, ext4)?;
    if hashes(&second) != hashes(&third) {
        return Err("Persistent kernel workspace changed across recovery reboot".into());
    }
    for (name, bytes) in [
        ("build", &first["build.log"]),
        ("failure", &second["failure.log"]),
    ] {
        let log = directory(root, "logs")?.join(format!("bootstrap-kernel-{name}.cargo.log"));
        output_path(&log)?;
        fs::write(log, bytes)?;
    }
    let evidence = json!({
        "result": "Ok", "scope": "Guest kernel ELF construction, exact host-assisted packaging and actual boot/recovery; no compiler or complete image bootstrap",
        "prepared": provenance, "construction": construction, "reboot": reboot, "recovery": recovery,
        "clean_build": true,
        "guest_kernel": retained, "guest_kernel_sha256": kernel_hash, "guest_kernel_bytes": kernel.len(),
        "guest_iso": guest_iso, "guest_iso_sha256": hash(&fs::read(&guest_iso)?),
        "packaged_kernel_byte_identical": true, "failed_compilation_preserves_kernel": true,
        "recovery_files_sha256": hashes(&third),
        "pressure_before_clean_build": pressure,
        "work_file_count_after_build": first.len(),
        "work_file_bytes_after_build": first.values().map(Vec::len).sum::<usize>(),
        "largest_work_file_bytes": first.values().map(Vec::len).max(),
    });
    if foundation::source_identity(root)? != implementation_identity {
        return Err("Implementation changed during kernel capacity acceptance".into());
    }
    if ext4 {
        fs::write(
            work.join("result.json"),
            serde_json::to_vec_pretty(&evidence)?,
        )?;
    }
    fs::write(result, serde_json::to_vec_pretty(&evidence)?)?;
    println!(
        "Verified guest-built kernel identity={identity} sha256={kernel_hash}; exact image and recovery reboot"
    );
    Ok(())
}
