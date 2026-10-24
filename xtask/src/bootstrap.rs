// SPDX-License-Identifier: Apache-2.0
//! Prepare a locked offline bzip2 workspace and verify guest reconstruction, limits and reboot.
//! Host work supplies sources, seed compilers and diagnostics; only the guest creates bzip2.
use super::*;
use anova_storage::ext2::{Ext2, MAX_FILE};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};

type Files = BTreeMap<String, Vec<u8>>;

pub fn prepare(root: &Path) -> Result<(PathBuf, PathBuf)> {
    command(Command::new("python3").arg(root.join("ports/bootstrap/prepare.py")))?;
    apps::build(root, false)?;
    let work = directory(root, "bootstrap")?;
    let mut archive = fs::read(toolchain::pack(root)?)?;
    let probe = work.join("capacity.elf");
    output_path(&probe)?;
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
            .arg(&probe),
    )?;
    let diagnostic = fs::read(&probe)?;
    anova_elf::parse_linux(&diagnostic)
        .map_err(|e| format!("Invalid M15 diagnostic ELF: {e:?}"))?;
    toolchain::append(&mut archive, 5, "/bin/m15-capacity", &diagnostic);
    let packed = work.join("initramfs.anovafs");
    output_path(&packed)?;
    fs::write(&packed, &archive)?;
    let iso = image_archive(root, Mode::Bootstrap, Some(&packed))?;
    let path = directory(root, "disks")?.join(format!(
        "bootstrap-{}.ext2",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    disk::create_profile(root, &path, true)?;
    let mut disk = mount(&path)?;
    let workspace = disk.create(2, "bootstrap", true).map_err(storage)?;
    copy_sources(&mut disk, workspace, &work.join("source"))?;
    disk.shutdown().map_err(storage)?;
    let source = source_files(root)?;
    let manifest = json!({
        "profile": "m15-bzip2-1.0.8",
        "source_commit": version("git", &["rev-parse", "HEAD"] )?,
        "source_worktree": version("git", &["status", "--porcelain"] )?,
        "build": "AnovaOS Linux x86_64 musl compatibility",
        "host": "AnovaOS Linux x86_64 musl compatibility",
        "target": "x86_64-linux-musl static ET_EXEC",
        "sysroot": "/tc",
        "compiler": "source-built GCC 11.2.0 from M11, prepared by host",
        "compiler_source_inventory_sha256": hash(&fs::read(root.join("build/toolchain/stage.sha256"))?),
        "compiler_package_inventory_sha256": hash(&fs::read(root.join("build/toolchain/package.sha256"))?),
        "diagnostic": "host musl-gcc; resource probe only, not reconstructed tool",
        "diagnostic_sha256": hash(&diagnostic),
        "iso": iso,
        "iso_sha256": hash(&fs::read(&iso)?),
        "initramfs_sha256": hash(&archive),
        "work_disk": path,
        "input_files": hashes(&source),
        "input_logical_bytes": source.values().map(Vec::len).sum::<usize>(),
        "input_max_file_bytes": source.values().map(Vec::len).max(),
        "disk_capacity": disk_capacity(&path)?,
        "network": false,
        "ram_bytes": 1024 * 1024 * 1024u64,
        "cpus": 1,
    });
    let prepared = work.join("prepared.json");
    output_path(&prepared)?;
    fs::write(prepared, serde_json::to_vec_pretty(&manifest)?)?;
    println!("Prepared independent M15 work disk: {}", path.display());
    println!("Run: cargo xtask bootstrap-test (creates its own fresh disk)");
    Ok((iso, path))
}

fn storage(error: i64) -> Box<dyn Error> {
    format!("M15 ext2 operation failed: {error}").into()
}

fn mount(path: &Path) -> Result<Ext2> {
    Ext2::mount(Box::new(disk::Image::open(path)?)).map_err(storage)
}

fn copy_sources(disk: &mut Ext2, parent: u32, directory: &Path) -> Result<()> {
    let mut entries = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Non-UTF8 M15 source")?;
        let kind = entry.file_type()?;
        if !kind.is_dir() && !kind.is_file() {
            return Err("M15 sources must be regular files or directories".into());
        }
        if kind.is_file() && entry.metadata()?.len() > MAX_FILE as u64 {
            return Err(format!("M15 input exceeds persistent file limit: {name}").into());
        }
        let inode = disk.create(parent, &name, kind.is_dir()).map_err(storage)?;
        if kind.is_dir() {
            copy_sources(disk, inode, &entry.path())?;
        } else {
            disk.write(inode, 0, &fs::read(entry.path())?)
                .map_err(storage)?;
        }
    }
    Ok(())
}

fn source_files(root: &Path) -> Result<Files> {
    let base = root.join("build/bootstrap/source");
    let mut pending = vec![base.clone()];
    let mut files = Files::new();
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else if entry.file_type()?.is_file() {
                files.insert(
                    entry
                        .path()
                        .strip_prefix(&base)?
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(entry.path())?,
                );
            } else {
                return Err("Non-regular M15 source entry".into());
            }
        }
    }
    Ok(files)
}

fn disk_capacity(path: &Path) -> Result<Value> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(1024))?;
    let mut superblock = [0; 1024];
    file.read_exact(&mut superblock)?;
    let value = |offset| u32::from_le_bytes(superblock[offset..offset + 4].try_into().unwrap());
    Ok(json!({
        "image_bytes": file.metadata()?.len(),
        "blocks": value(4),
        "allocated_blocks": value(4) - value(12),
        "block_bytes": 4096,
        "inodes": value(0),
        "allocated_inodes": value(0) - value(16),
        "writable_file_limit_bytes": MAX_FILE,
    }))
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hashes(files: &Files) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|(name, bytes)| (name.clone(), hash(bytes)))
        .collect()
}

fn read_tree(disk: &mut Ext2, parent: u32, prefix: &str, files: &mut Files) -> Result<()> {
    for entry in disk.entries(parent).map_err(storage)? {
        let name = std::str::from_utf8(&entry.name[..entry.length])?;
        if matches!(name, "." | "..") {
            continue;
        }
        let path = if prefix.is_empty() {
            name.into()
        } else {
            format!("{prefix}/{name}")
        };
        let inode = disk.inode(entry.inode).map_err(storage)?;
        if inode.directory {
            read_tree(disk, entry.inode, &path, files)?;
        } else {
            let mut bytes = vec![0; inode.size];
            if disk.read(entry.inode, 0, &mut bytes).map_err(storage)? != bytes.len() {
                return Err(format!("Short M15 read: {path}").into());
            }
            if path == "bzip2" {
                anova_elf::parse_linux(&bytes).map_err(|e| format!("Guest bzip2 ELF: {e:?}"))?;
                if !inode.executable {
                    return Err("Guest bzip2 is not executable".into());
                }
            }
            files.insert(path, bytes);
        }
    }
    Ok(())
}

fn inspect(root: &Path, path: &Path, name: &str) -> Result<Files> {
    let mut disk = mount(path)?;
    let workspace = disk
        .entries(2)
        .map_err(storage)?
        .into_iter()
        .find(|entry| &entry.name[..entry.length] == b"bootstrap")
        .ok_or("Missing M15 workspace")?
        .inode;
    let mut files = Files::new();
    read_tree(&mut disk, workspace, "", &mut files)?;
    disk.shutdown().map_err(storage)?;
    for (name, expected) in source_files(root)? {
        if files.get(&name) != Some(&expected) {
            return Err(format!("M15 persisted input differs: {name}").into());
        }
    }
    if !files.contains_key("bzip2") || files.contains_key("bzip2.candidate") {
        return Err("Missing guest tool or leftover failed candidate".into());
    }
    if files.get("roundtrip") != files.get("sample1.ref") {
        return Err("Guest bzip2 round trip differs".into());
    }
    if files.get("sample1.out.bz2") != files.get("sample1.bz2") {
        return Err("Guest compression differs from the official block-size-1 sample".into());
    }
    let work = root.join("build/bootstrap");
    output_path(&work.join("guest-output.bz2"))?;
    fs::write(
        work.join("guest-output.bz2"),
        files
            .get("sample1.out.bz2")
            .ok_or("Missing compressed output")?,
    )?;
    // Independent decoding verifies real guest compression rather than matching guest log text.
    command(Command::new("python3").arg("-c").arg(
        "import bz2,pathlib,sys; p=pathlib.Path(sys.argv[1]); actual=bz2.decompress((p/'guest-output.bz2').read_bytes()); expected=(p/'source/sample1.ref').read_bytes(); sys.exit(0 if actual == expected else 'Guest compressed bytes differ from the official reference')"
    ).arg(work))?;
    let inventory = root.join(format!("build/logs/{name}.sha256"));
    output_path(&inventory)?;
    fs::write(
        inventory,
        hashes(&files)
            .iter()
            .map(|(name, digest)| format!("{digest}  {name}\n"))
            .collect::<String>(),
    )?;
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(path))?;
    Ok(files)
}

fn checked(steps: &mut Vec<(String, Vec<u8>)>, command: &str, status: u32) {
    steps.push(("anova> ".into(), format!("{command}\r").into_bytes()));
    steps.push(("anova> ".into(), b"status\r".to_vec()));
    let marker = if status == 0 {
        "[SHELL:STATUS] code=0".into()
    } else {
        format!("[SHELL:STATUS] code={status}\r\n")
    };
    steps.push((marker, Vec::new()));
}

fn session(
    root: &Path,
    iso: &Path,
    path: &Path,
    name: &str,
    steps: &[(String, Vec<u8>)],
) -> Result<Value> {
    let exchanges: Vec<_> = steps
        .iter()
        .map(|(marker, bytes)| (marker.as_str(), bytes.as_slice()))
        .collect();
    let start = Instant::now();
    disk::session_timeout(root, iso, path, name, false, None, &exchanges, true, 1200)?;
    let wall_ms = start.elapsed().as_millis();
    let serial = fs::read_to_string(root.join(format!("build/logs/{name}.serial.log")))?;
    if serial.contains("[M15:CAPACITY:FAIL]") {
        return Err("M15 resource probe failed".into());
    }
    let resources = resource_summary(&serial)?;
    Ok(
        json!({"host_wall_ms_including_boot_and_shutdown": wall_ms, "resources": resources, "disk": disk_capacity(path)?}),
    )
}

fn resource_summary(serial: &str) -> Result<BTreeMap<String, String>> {
    let lines: Vec<_> = serial
        .lines()
        .filter_map(|line| line.strip_prefix("[M15:RESOURCES] "))
        .collect();
    if lines.len() != 1 {
        return Err("Expected one M15 shutdown resource summary".into());
    }
    let fields: BTreeMap<String, String> = lines[0]
        .split_whitespace()
        .filter_map(|value| value.split_once('='))
        .map(|(key, value)| (key.into(), value.into()))
        .collect();
    for key in [
        "samples",
        "baseline_free_frames",
        "min_free_frames",
        "peak_frame_delta",
        "heap_peak_bytes",
        "live_tasks_peak",
        "occupied_slots_peak",
        "fd_per_process_peak",
        "vfs_node_slots_peak",
        "elapsed_ns",
    ] {
        fields
            .get(key)
            .ok_or_else(|| format!("Missing M15 metric {key}"))?
            .parse::<u64>()?;
    }
    if fields.get("sampling").map(String::as_str) != Some("boundaries")
        || fields.get("scope").map(String::as_str) != Some("scheduler-instance")
        || fields.get("clock").map(String::as_str) != Some("irq-monotonic")
    {
        return Err("Unrecognized M15 measurement scope".into());
    }
    let number = |key: &str| fields[key].parse::<u64>().unwrap();
    let baseline = number("baseline_free_frames");
    let minimum = number("min_free_frames");
    let live = number("live_tasks_peak");
    let occupied = number("occupied_slots_peak");
    if number("samples") == 0
        || baseline == 0
        || minimum > baseline
        || number("peak_frame_delta") != baseline - minimum
        || number("heap_peak_bytes") == 0
        || live == 0
        || live > occupied
        || occupied > 32
        || !(3..=32).contains(&number("fd_per_process_peak"))
        || !(1..=2048).contains(&number("vfs_node_slots_peak"))
    {
        return Err("Inconsistent or empty M15 resource evidence".into());
    }
    Ok(fields)
}

pub fn verify(root: &Path) -> Result<()> {
    let result = directory(root, "logs")?.join("bootstrap.result.json");
    output_path(&result)?;
    if result.exists() {
        checked_file(&result)?;
        fs::remove_file(&result)?;
    }
    let (iso, path) = prepare(root)?;
    let prepared: Value =
        serde_json::from_slice(&fs::read(root.join("build/bootstrap/prepared.json"))?)?;
    let mut steps = Vec::new();
    checked(&mut steps, "m15-capacity", 0);
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
    let capacity = session(root, &iso, &path, "bootstrap-capacity", &steps)?;
    let serial = fs::read_to_string(root.join("build/logs/bootstrap-capacity.serial.log"))?;
    if !serial.lines().any(|line| {
        line.starts_with("[M15:CAPACITY] files=efbig-preserved ") && line.ends_with("recovery=ok")
    }) {
        return Err("Missing real M15 limit/recovery checks".into());
    }
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(&path))?;
    steps.clear();
    checked(&mut steps, "cd /persist/bootstrap", 0);
    checked(&mut steps, "make -f Makefile.anova -j1", 0);
    checked(
        &mut steps,
        "./bzip2 -1 -kc sample1.ref > sample1.out.bz2",
        0,
    );
    checked(&mut steps, "./bzip2 -dc sample1.out.bz2 > roundtrip", 0);
    checked(&mut steps, "./bzip2 -t sample1.out.bz2", 0);
    checked(&mut steps, "./bzip2 -t README", 2);
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
    let build = session(root, &iso, &path, "bootstrap-build", &steps)?;
    let first = inspect(root, &path, "bootstrap-build")?;
    steps.clear();
    checked(&mut steps, "cd /persist/bootstrap", 0);
    checked(&mut steps, "./bzip2 -t sample1.out.bz2", 0);
    checked(&mut steps, "make -f Makefile.anova fail", 2);
    checked(&mut steps, "./bzip2 -t sample1.out.bz2", 0);
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
    let reboot = session(root, &iso, &path, "bootstrap-reboot", &steps)?;
    let serial = fs::read_to_string(root.join("build/logs/bootstrap-reboot.serial.log"))?;
    if !serial.contains("rejected.c:")
        || !serial.contains("error: #error M15_intentional_build_rejection")
    {
        return Err("Failed M15 build did not reach the deliberate compiler rejection".into());
    }
    let after = inspect(root, &path, "bootstrap-reboot")?;
    if first != after {
        return Err("M15 inputs or outputs changed across reboot/failed build".into());
    }
    let report = json!({
        "result": "Ok", "profile": "M15.1/M15.2 bzip2 1.0.8", "work_disk": path,
        "boots": 3, "compiler_execution": "guest", "host_compiled_bzip2": false, "prepared": prepared,
        "capacity_probe": capacity, "build": build, "reboot_and_failure_recovery": reboot,
        "files": hashes(&first), "logical_file_bytes": first.values().map(Vec::len).sum::<usize>(),
        "max_file_bytes": first.values().map(Vec::len).max(), "source_and_elf_reboot_hashes": "identical",
        "roundtrip": "guest exact; independently host decoded", "e2fsck": "clean after all three boots",
        "measurement_limits": "scheduler-boundary samples, not peak RSS; host wall time includes boot/shutdown",
        "compiler_bootstrap": "not evaluated", "kernel_rebuild": "not implemented",
    });
    fs::write(&result, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "Verified guest bzip2 reconstruction, capacity rejection/recovery and reboot: {}",
        result.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMMARY: &str = "[M15:RESOURCES] scope=scheduler-instance sampling=boundaries clock=irq-monotonic samples=100 baseline_free_frames=10000 min_free_frames=1000 peak_frame_delta=9000 heap_peak_bytes=1908640 live_tasks_peak=5 occupied_slots_peak=5 fd_per_process_peak=10 vfs_node_slots_peak=1348 elapsed_ns=100000\n";

    #[test]
    fn rejects_missing_duplicate_or_incomplete_resource_evidence() {
        assert!(resource_summary("").is_err());
        assert!(resource_summary("[M15:RESOURCES] samples=1\n").is_err());
        assert!(
            resource_summary("[M15:RESOURCES] samples=1\n[M15:RESOURCES] samples=2\n").is_err()
        );
    }

    #[test]
    fn requires_active_samples_consistent_bounds_and_declared_scope() {
        assert!(resource_summary(SUMMARY).is_ok());
        for (from, to) in [
            ("samples=100", "samples=0"),
            ("peak_frame_delta=9000", "peak_frame_delta=8000"),
            ("live_tasks_peak=5", "live_tasks_peak=6"),
            ("fd_per_process_peak=10", "fd_per_process_peak=33"),
            ("sampling=boundaries", "sampling=rss"),
            ("clock=irq-monotonic", "clock=wall"),
            ("heap_peak_bytes=1908640", "heap_peak_bytes=0"),
        ] {
            assert!(resource_summary(&SUMMARY.replace(from, to)).is_err());
        }
    }
}
