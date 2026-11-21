// SPDX-License-Identifier: Apache-2.0
//! Assemble the final optical image in AnovaOS from a retained guest-built kernel.
//! Compare independent publications, inspect extracted bytes, and boot without host repackaging.
use super::*;
use anova_storage::{ext4::Ext4, filesystem::Filesystem};
use bootstrap::{checked, hash, hashes};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Write};

const MAX_IMAGE: usize = 32 * 1024 * 1024;
const MAX_INPUT: usize = 16 * 1024 * 1024;
const BUILD_SECONDS: u64 = 600;
const REBOOT_SECONDS: u64 = 90;
const RESPONSE_MS: u128 = 5_000;
const KERNEL_PROFILE: &str = "release-opt-z-ext4-write-bootstrap-metrics-v1";
const POWEROFF: &str = "[POWER:OFF] synced=true clean=true dma_reset=true";
const PACK: &str = "m18-image /im/kernel.elf /im /persist/anova.iso";
const USER_DATA: &[u8] = b"m18-retained-user-bytes\n";
type Files = BTreeMap<String, Vec<u8>>;
type Steps = Vec<(String, Vec<u8>)>;

fn regular(path: &Path, max: usize) -> Result<Vec<u8>> {
    checked_file(path)?;
    if fs::metadata(path)?.len() > max as u64 {
        return Err(format!("Input exceeds the declared size limit: {}", path.display()).into());
    }
    Ok(fs::read(path)?)
}

fn boot_id(prepared: &Value, kernel: &[u8]) -> Result<String> {
    let id = prepared["build_identity"]
        .as_str()
        .ok_or("Kernel provenance is missing the build identity")?;
    if id.len() != 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || !kernel.windows(id.len()).any(|bytes| bytes == id.as_bytes())
        || prepared["profile"] != KERNEL_PROFILE
        || prepared["ext4_work_disk"] != true
        || prepared["image_generation_in_guest"] != false
    {
        return Err("Expected an identified guest-built metrics/Ext4 kernel".into());
    }
    validate_elf(kernel)?;
    Ok(id.into())
}

fn storage(error: i64) -> Box<dyn Error> {
    format!("M18 image ext4 inspection failed: {error}").into()
}

fn mount(path: &Path) -> Result<Ext4> {
    Ext4::mount(Box::new(disk::Image::open(path)?)).map_err(storage)
}

fn disk_file(disk: &mut Ext4, parent: u32, path: &str, max: usize) -> Result<Vec<u8>> {
    let node = disk
        .lookup(parent, path)
        .map_err(storage)?
        .ok_or_else(|| format!("Missing persisted M18 image file: {path}"))?;
    let info = disk.inode(node.inode).map_err(storage)?;
    if info.directory || info.size > max {
        return Err(format!("Invalid persisted M18 image file: {path}").into());
    }
    let mut bytes = vec![0; info.size];
    if disk.read(node.inode, 0, &mut bytes).map_err(storage)? != bytes.len() {
        return Err(format!("Short persisted M18 image read: {path}").into());
    }
    Ok(bytes)
}

fn replay_provenance(report: &str, path: &Path, kernel: &[u8]) -> Result<(String, Value)> {
    let last = report.lines().last().ok_or("Empty kernel replay log")?;
    let replay: Value = serde_json::from_str(last)?;
    let parent = path
        .parent()
        .ok_or("Missing historical kernel evidence directory")?;
    let prepared: Value =
        serde_json::from_slice(&regular(&parent.join("prepared.json"), 2 << 20)?)?;
    let id = boot_id(&prepared, kernel)?;
    if replay["result"] != "Ok"
        || replay["boundary"]
            != "Two exact recovery boots on original artifacts from the RAM-backed run"
        || replay["guest_kernel"] != hash(kernel)
        || replay["repeat_identity"] != id
    {
        return Err("Historical replay receipt does not attest the supplied kernel".into());
    }
    let mut uart = BTreeMap::new();
    for (stage, marker) in [
        ("build", format!("[M15:KERNEL:BUILT] id={id} exit=0")),
        ("reboot", format!("[BOOT:BUILD] id={id}")),
        ("recovery", format!("[BOOT:BUILD] id={id}")),
    ] {
        let log = parent.join(format!("bootstrap-kernel-{stage}.serial.log"));
        let data = regular(&log, 512 * 1024)?;
        let text = std::str::from_utf8(&data)?;
        if text.lines().filter(|line| *line == marker).count() != 1
            || !text.contains(POWEROFF)
            || text.contains("[PANIC]")
        {
            return Err(format!(
                "Historical {stage} UART evidence differs from its replay receipt"
            )
            .into());
        }
        uart.insert(stage, hash(&data));
    }
    let mut disk = mount(&parent.join("work.ext4"))?;
    let workspace = disk
        .lookup(2, "kernel")
        .map_err(storage)?
        .ok_or("Missing historical build workspace")?;
    if disk_file(&mut disk, workspace.inode, "kernel.elf", MAX_INPUT)? != kernel {
        return Err("Historical build disk contains different kernel bytes".into());
    }
    disk.shutdown().map_err(storage)?;
    Ok((
        id,
        json!({"kind":"historical-capacity-replay", "prepared":prepared,
                  "replay":replay, "uart_sha256":uart}),
    ))
}

fn provenance(report: &Path, path: &Path, kernel: &[u8]) -> Result<(String, Value)> {
    let bytes = regular(report, 8 * 1024 * 1024)?;
    let (id, evidence) = if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
        let receipt = value.get("report").unwrap_or(&value);
        if receipt["result"] != "Ok"
            || receipt["clean_build"] != true
            || receipt["packaged_kernel_byte_identical"] != true
            || receipt["failed_compilation_preserves_kernel"] != true
            || receipt["guest_kernel_sha256"] != hash(kernel)
            || receipt["guest_kernel_bytes"] != kernel.len()
        {
            return Err("Capacity result does not attest the supplied kernel".into());
        }
        (
            boot_id(&receipt["prepared"], kernel)?,
            json!({"kind":"capacity-result", "receipt":receipt}),
        )
    } else {
        replay_provenance(std::str::from_utf8(&bytes)?, path, kernel)?
    };
    let import = json!({"file_sha256":hash(&bytes), "report_path":report, "kernel_path":path,
                        "build_identity":id, "evidence":evidence});
    Ok((id, import))
}

fn image_inputs(root: &Path, kernel: Vec<u8>) -> Result<Files> {
    let cache = root.join("build/cache");
    let apps = root.join("build/apps");
    let mut files = Files::from([
        ("boot/kernel.elf".into(), kernel),
        (
            "boot/limine/limine.conf".into(),
            limine_boot_config(root)?.into_bytes(),
        ),
    ]);
    for (source, name) in [
        (cache.join("BOOTX64.EFI"), "EFI/BOOT/BOOTX64.EFI"),
        (
            cache.join("limine-uefi-cd.bin"),
            "boot/limine/limine-uefi-cd.bin",
        ),
        (cache.join("LIMINE-LICENSE"), "boot/limine/LICENSE"),
        (apps.join("initramfs.anovafs"), "boot/initramfs.anovafs"),
        (apps.join("manifest.toml"), "boot/apps/manifest.toml"),
    ] {
        files.insert(name.into(), regular(&source, MAX_INPUT)?);
    }
    for name in apps::NAMES {
        let file = format!("{name}.elf");
        files.insert(
            format!("boot/apps/{file}"),
            regular(&apps.join(file), MAX_INPUT)?,
        );
    }
    Ok(files)
}

fn seed_archive(root: &Path, work: &Path, inputs: &Files) -> Result<(PathBuf, Value)> {
    let source = root.join("ports/bootstrap/image-build.c");
    let tool = work.join("m18-image.elf");
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
            .arg(&source)
            .arg("-o")
            .arg(&tool),
    )?;
    let writer = regular(&tool, MAX_INPUT)?;
    anova_elf::parse_linux(&writer)
        .map_err(|error| format!("Invalid guest image writer: {error:?}"))?;
    let mut archive = inputs["boot/initramfs.anovafs"].clone();
    let mut staged = Files::new();
    for (name, bytes) in inputs {
        let name = if name == "boot/limine/LICENSE" {
            "LIMINE-LICENSE"
        } else {
            name.rsplit('/').next().ok_or("Invalid image input name")?
        };
        if staged
            .insert(format!("/im/{name}"), bytes.clone())
            .is_some()
        {
            return Err(format!("Conflicting image staging name: {name}").into());
        }
    }
    bootstrap_kernel::append_inputs(&mut archive, &staged)?;
    toolchain::append(
        &mut archive,
        anova_vfs::archive::LINUX_EXECUTABLE,
        "/bin/m18-image",
        &writer,
    );
    let packed = work.join("seed-initramfs.anovafs");
    fs::write(&packed, &archive)?;
    let manifest = json!({
        "boot_inputs_sha256":hashes(inputs), "guest_inputs_sha256":hashes(&staged),
        "logical_input_bytes":inputs.values().map(Vec::len).sum::<usize>(),
        "largest_input_bytes":inputs.values().map(Vec::len).max(),
        "writer_source_sha256":hash(&fs::read(source)?), "writer_sha256":hash(&writer),
        "host_compiler":version("musl-gcc", &["--version"] )?,
        "seed_archive_sha256":hash(&archive),
    });
    Ok((packed, manifest))
}

fn session(
    root: &Path,
    work: &Path,
    iso: &Path,
    disk: &Path,
    label: &str,
    steps: &Steps,
    seconds: u64,
) -> Result<(Value, String, Vec<u128>)> {
    let run = work
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid image run directory")?;
    let name = format!("m18-image-{run}-{label}");
    let exchanges: Vec<_> = steps
        .iter()
        .map(|(marker, bytes)| (marker.as_str(), bytes.as_slice()))
        .collect();
    let start = Instant::now();
    let result = disk::session_observed(
        root, iso, disk, &name, false, None, &exchanges, true, seconds, false,
    );
    for suffix in ["serial.log", "qemu.log", "result.txt"] {
        let source = root.join(format!("build/logs/{name}.{suffix}"));
        if source.is_file() {
            fs::copy(source, work.join(format!("{label}.{suffix}")))?;
        }
    }
    let timings = result?;
    let serial = fs::read_to_string(work.join(format!("{label}.serial.log")))?;
    let resources = bootstrap::resource_summary(&serial)?;
    for (name, ceiling) in [
        ("heap_peak_bytes", 16_777_216),
        ("peak_frame_delta", 32_768),
    ] {
        if resources[name].parse::<u64>()? > ceiling {
            return Err(format!("Image session exceeds declared resource budget: {name}").into());
        }
    }
    let counters = foundation_capacity::storage_summary(disk, &serial)?;
    foundation_journal::check(disk)?;
    let evidence = json!({"name":name, "serial_sha256":hash(serial.as_bytes()),
        "host_wall_ms_including_boot_and_shutdown":start.elapsed().as_millis(),
        "resources":resources, "disk":counters, "uart_exchange_ms":timings});
    fs::write(
        work.join(format!("{label}.json")),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    Ok((evidence, serial, timings))
}

fn inspect_disk(path: &Path, has_blocked_target: bool) -> Result<Vec<u8>> {
    let mut disk = mount(path)?;
    for candidate in ["anova.iso.candidate", "blocked.iso.candidate"] {
        if disk.lookup(2, candidate).map_err(storage)?.is_some() {
            return Err(format!("Unpublished image candidate survived: {candidate}").into());
        }
    }
    if disk_file(&mut disk, 2, "user-data", 1024)? != USER_DATA
        || disk_file(&mut disk, 2, ".anova-abi", 1024)? != b"linux.x86_64.v1\n"
    {
        return Err("Image workflow changed the prior user bytes or ABI identity".into());
    }
    if has_blocked_target {
        let blocked = disk
            .lookup(2, "blocked.iso")
            .map_err(storage)?
            .ok_or("Missing preserved publication target directory")?;
        if disk_file(&mut disk, blocked.inode, "keep", 1024)? != b"keep-directory\n" {
            return Err("Failed publication changed the directory contents".into());
        }
    }
    let image = disk_file(&mut disk, 2, "anova.iso", MAX_IMAGE)?;
    disk.shutdown().map_err(storage)?;
    Ok(image)
}

fn extract(work: &Path, iso: &Path, inputs: &Files, label: &str) -> Result<()> {
    checked_file(iso)?;
    let extracted = work.join(format!("{label}-extracted"));
    let result = Command::new("xorriso")
        .args(["-osirrox", "on", "-indev"])
        .arg(format!("stdio:{}", iso.display()))
        .args(["-report_el_torito", "plain", "-extract", "/"])
        .arg(&extracted)
        .output()?;
    fs::write(
        work.join(format!("{label}-xorriso.stdout.log")),
        &result.stdout,
    )?;
    fs::write(
        work.join(format!("{label}-xorriso.stderr.log")),
        &result.stderr,
    )?;
    let report = std::str::from_utf8(&result.stdout)?;
    if !result.status.success()
        || !report.lines().any(|line| {
            line.strip_prefix("El Torito boot img :")
                .is_some_and(|fields| {
                    fields
                        .split_whitespace()
                        .take(4)
                        .eq(["1", "UEFI", "y", "none"])
                })
        })
    {
        return Err("Independent xorriso reader rejected the guest UEFI El Torito image".into());
    }
    let mut files = Files::new();
    bootstrap_kernel::collect(&extracted, "", &mut files)?;
    if files != *inputs {
        return Err(
            "Independently extracted ISO files differ from the exact guest input inventory".into(),
        );
    }
    Ok(())
}

fn shutdown(steps: &mut Steps) {
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
}

fn reboot(
    root: &Path,
    work: &Path,
    iso: &Path,
    disk: &Path,
    label: &str,
    id: &str,
    recover_space: bool,
) -> Result<Value> {
    let mut steps = Steps::new();
    checked(&mut steps, "cat /persist/user-data", 0);
    if recover_space {
        checked(&mut steps, "rm /persist/space-reserve", 0);
        checked(&mut steps, "echo reused-space > /persist/reused", 0);
    }
    checked(&mut steps, "hello-c", 7);
    shutdown(&mut steps);
    let (result, serial, _) = session(root, work, iso, disk, label, &steps, REBOOT_SECONDS)?;
    let name = result["name"]
        .as_str()
        .ok_or("Missing image boot scenario name")?;
    bootstrap_kernel::boot_identity(root, name, id)?;
    if !serial.contains("\r\nm18-retained-user-bytes\r\n") {
        return Err("Final guest-built ISO did not boot and read the retained user data".into());
    }
    Ok(result)
}

fn construct(
    root: &Path,
    work: &Path,
    seed: &Path,
    disk: &Path,
    round: &str,
    negatives: bool,
) -> Result<Value> {
    foundation_journal::create(root, disk, true)?;
    let mut steps = Steps::new();
    checked(
        &mut steps,
        "echo m18-retained-user-bytes > /persist/user-data",
        0,
    );
    checked(&mut steps, PACK, 0);
    let mut cancel_at = None;
    if negatives {
        checked(
            &mut steps,
            "m18-image /im/missing.elf /im /persist/anova.iso",
            1,
        );
        checked(&mut steps, "mkdir /persist/blocked.iso", 0);
        checked(
            &mut steps,
            "echo keep-directory > /persist/blocked.iso/keep",
            0,
        );
        checked(
            &mut steps,
            "m18-image /im/kernel.elf /im /persist/blocked.iso",
            1,
        );
        steps.push(("anova> ".into(), format!("{PACK}\r").into_bytes()));
        cancel_at = Some(steps.len());
        steps.push(("[M18:IMAGE:WRITE] bytes=".into(), b"\x03".to_vec()));
        steps.push(("anova> ".into(), b"status\r".to_vec()));
        steps.push(("[SHELL:STATUS] code=130\r\n".into(), Vec::new()));
        checked(&mut steps, "m18-image cleanup /persist/anova.iso", 0);
        checked(&mut steps, "hello-c", 7);
    }
    shutdown(&mut steps);
    let (mut result, serial, timings) =
        session(root, work, seed, disk, round, &steps, BUILD_SECONDS)?;
    if serial.matches("[M18:IMAGE:PUBLISHED]").count() != 1 {
        return Err("Expected exactly one successfully published image".into());
    }
    if let Some(at) = cancel_at {
        for marker in [
            "open input /im/missing.elf",
            "publish candidate /persist/blocked.iso",
            "[M18:IMAGE:CLEANUP] path=/persist/anova.iso.candidate",
        ] {
            if !serial.contains(marker) {
                return Err(format!("Missing actual negative image evidence: {marker}").into());
            }
        }
        let milliseconds = timings[at + 1] - timings[at];
        if milliseconds > RESPONSE_MS {
            return Err(
                format!("COM1 cancellation response exceeded budget: {milliseconds} ms").into(),
            );
        }
        result["cancel_to_prompt_host_ms"] = json!(milliseconds);
    }
    Ok(result)
}

fn exhaust(
    root: &Path,
    work: &Path,
    seed: &Path,
    previous_disk: &Path,
    iso: &Path,
    id: &str,
    expected: &[u8],
) -> Result<Value> {
    let disk = work.join("pressure.ext4");
    fs::copy(previous_disk, &disk)?;
    let capacity = bootstrap::disk_capacity(&disk)?;
    let free = capacity["blocks"].as_u64().ok_or("Missing disk size")?
        - capacity["allocated_blocks"]
            .as_u64()
            .ok_or("Missing allocated blocks")?;
    let reserved = free
        .checked_sub(64)
        .ok_or("No room to seed image storage pressure")?;
    let filler = Temporary::new(root, "image-pressure")?;
    let mut file = File::create(filler.0.join("nonzero"))?;
    for _ in 0..reserved {
        file.write_all(&[0x5a; 4096])?;
    }
    file.sync_all()?;
    drop(file);
    command(
        Command::new("debugfs")
            .current_dir(&filler.0)
            .args(["-w", "-R", "write nonzero /space-reserve"])
            .arg(&disk),
    )?;
    foundation_write::root_owned_fixture(&disk, &["/space-reserve"])?;
    foundation_journal::check(&disk)?;
    let mut steps = Steps::new();
    checked(&mut steps, PACK, 1);
    checked(&mut steps, "hello-c", 7);
    shutdown(&mut steps);
    let (pressure, serial, _) =
        session(root, work, seed, &disk, "pressure", &steps, BUILD_SECONDS)?;
    if !serial.contains("stream image /persist/anova.iso.candidate: No space left on device")
        || serial.contains("[M18:IMAGE:PUBLISHED]")
        || inspect_disk(&disk, true)? != expected
    {
        return Err("Actual ENOSPC did not retain the previous boot image and user bytes".into());
    }
    let recovery = reboot(root, work, iso, &disk, "pressure-reboot", id, true)?;
    let mut volume = mount(&disk)?;
    if volume
        .lookup(2, "space-reserve")
        .map_err(storage)?
        .is_some()
        || disk_file(&mut volume, 2, "reused", 1024)? != b"reused-space\n"
        || inspect_disk(&disk, true)? != expected
    {
        return Err("Recovered work disk did not reuse its released space".into());
    }
    volume.shutdown().map_err(storage)?;
    Ok(json!({"reserved_payload_bytes":reserved * 4096, "pressure":pressure, "recovery":recovery}))
}

pub fn verify(root: &Path, args: &[String]) -> Result<()> {
    let [report, path] = args else {
        return Err("Usage: cargo xtask m18-image-test REPORT GUEST_ELF".into());
    };
    let (report, path) = (Path::new(report), Path::new(path));
    let kernel = regular(path, MAX_INPUT)?;
    let (id, provenance) = provenance(report, path, &kernel)?;
    let implementation = foundation::source_identity(root)?;
    let run = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let work = directory(root, "m18-image")?.join(format!("run-{run}"));
    fs::create_dir(&work)?;
    let budgets = json!({
        "image_bytes":MAX_IMAGE, "input_file_bytes":MAX_INPUT, "stream_chunk_bytes":4096,
        "work_disk_bytes":67108864, "blocks":16384, "inodes":4096,
        "ram_bytes":268435456, "cpus":1, "network":false, "accelerator":"tcg",
        "heap_bytes":16777216, "vfs_slots":2048, "tasks":32, "fds_per_process":32,
        "additional_frames":32768, "build_session_seconds":BUILD_SECONDS,
        "reboot_session_seconds":REBOOT_SECONDS, "cancel_to_prompt_host_ms":RESPONSE_MS,
        "measurements":"Scheduler boundary samples and successful ext4 mutations; COM1 includes host observation polling",
    });
    fs::write(
        work.join("budgets.json"),
        serde_json::to_vec_pretty(&budgets)?,
    )?;
    assets::check(root)?;
    apps::build(root, false)?;
    let inputs = image_inputs(root, kernel)?;
    let (archive, seeds) = seed_archive(root, &work, &inputs)?;
    let seed = work.join("host-seed.iso");
    fs::copy(
        image_archive(root, Mode::Foundation, Some(&archive))?,
        &seed,
    )?;
    let prepared = json!({"kernel_provenance":provenance, "seeds":seeds,
        "host_seed_iso_sha256":hash(&fs::read(&seed)?), "implementation_sha256":implementation,
        "source_commit":version("git", &["rev-parse", "HEAD"] )?,
        "source_worktree":version("git", &["status", "--porcelain"] )?,
        "boot_assets_lock_sha256":hash(&fs::read(root.join("boot/dependencies.toml"))?),
        "host_qemu":version("qemu-system-x86_64", &["--version"] )?,
        "host_xorriso":version("xorriso", &["-version"] )?,
        "inherited":"Retained guest-built non-GUI ext4-write/bootstrap-metrics ELF, locked Limine UEFI image/EFI/license, host-built native applications and base initramfs, host-compiled first-party image writer",
        "host_scope":"Validate provenance; package a separate boot seed; create ordinary work disks; export and independently inspect the final ISO; drive QEMU",
    });
    fs::write(
        work.join("prepared.json"),
        serde_json::to_vec_pretty(&prepared)?,
    )?;

    let mut rounds = Vec::new();
    let mut previous = None;
    for number in 1..=2 {
        let label = format!("round-{number}");
        let disk = work.join(format!("{label}.ext4"));
        let build = construct(root, &work, &seed, &disk, &label, number == 1)?;
        let bytes = inspect_disk(&disk, number == 1)?;
        let iso = work.join(format!("{label}.iso"));
        fs::write(&iso, &bytes)?;
        extract(&work, &iso, &inputs, &label)?;
        let boot = reboot(
            root,
            &work,
            &iso,
            &disk,
            &format!("{label}-reboot"),
            &id,
            false,
        )?;
        if inspect_disk(&disk, number == 1)? != bytes {
            return Err("Booting the final guest image changed the persisted ISO bytes".into());
        }
        if let Some(first) = &previous {
            if *first != bytes {
                return Err(
                    "Independent guest image publications differ; both are retained".into(),
                );
            }
        } else {
            previous = Some(bytes.clone());
        }
        rounds.push(
            json!({"image":iso, "image_sha256":hash(&bytes), "img_bytes":bytes.len(),
                           "build":build, "reboot":boot}),
        );
    }
    let bytes = previous.ok_or("Missing first-round image bytes")?;
    let pressure = exhaust(
        root,
        &work,
        &seed,
        &work.join("round-1.ext4"),
        &work.join("round-1.iso"),
        &id,
        &bytes,
    )?;
    if foundation::source_identity(root)? != implementation {
        return Err("Implementation changed during guest image verification".into());
    }
    let result = json!({"result":"Ok", "image_generation_in_guest":true,
        "same_kernel_input_repackaged_twice":true, "independent_guest_images_byte_identical":true,
        "all_extracted_files_byte_identical":true, "kernel_sha256":hash(&inputs["boot/kernel.elf"]),
        "rounds":rounds, "pressure":pressure, "prepared":prepared, "budgets":budgets});
    let result_path = work.join("result.json");
    fs::write(&result_path, serde_json::to_vec_pretty(&result)?)?;
    println!(
        "Verified two exact guest-built optical images and actual recovery boots: {}",
        result_path.display()
    );
    Ok(())
}
