// SPDX-License-Identifier: Apache-2.0
//! Repeat both accepted guest reconstructions on fresh disks and retain independent evidence.
//! Compare exact inputs and artifact bytes; preserve metadata differences and reject build drift.
use super::*;
use bootstrap::{hashes, mount, read_tree};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;

const BZIP2_OUTPUTS: &[&str] = &[
    "bzip2",
    "blocksort.o",
    "huffman.o",
    "crctable.o",
    "randtable.o",
    "compress.o",
    "decompress.o",
    "bzlib.o",
    "bzip2.o",
    "sample1.out.bz2",
    "roundtrip",
];
const BZIP2_INPUTS: &[&str] = &[
    "profile",
    "source_commit",
    "source_worktree",
    "input_files",
    "build",
    "host",
    "target",
    "sysroot",
    "compiler",
    "compiler_source_inventory_sha256",
    "compiler_package_inventory_sha256",
    "diagnostic_sha256",
    "initramfs_sha256",
    "ram_bytes",
    "cpus",
    "network",
];
const KERNEL_INPUTS: &[&str] = &[
    "source_commit",
    "source_worktree",
    "input_files_sha256",
    "implementation_files_sha256",
    "build_identity",
    "driver_source_sha256",
    "host_compiled_driver_sha256",
    "host_compiled_capacity_sha256",
    "seed_kernel_sha256",
    "rust_seed_inventory_sha256",
    "initramfs_sha256",
    "build_host",
    "target",
    "profile",
    "seed",
    "ram_bytes",
    "cpus",
    "network",
];

fn write_json(path: &Path, value: &Value) -> Result<()> {
    output_path(path)?;
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn file_hash(path: &Path) -> Result<String> {
    checked_file(path)?;
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let count = file.read(&mut bytes)?;
        if count == 0 {
            return Ok(format!("{:x}", digest.finalize()));
        }
        digest.update(&bytes[..count]);
    }
}

fn sources(root: &Path) -> Result<BTreeMap<String, String>> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err("Cannot inventory reconstruction implementation".into());
    }
    let mut files = BTreeMap::new();
    for name in std::str::from_utf8(&output.stdout)?
        .split('\0')
        .filter(|name| !name.is_empty())
    {
        files.insert(name.into(), file_hash(&root.join(name))?);
    }
    Ok(files)
}

fn copy(source: &Path, destination: &Path, inventory: &mut BTreeMap<String, String>) -> Result<()> {
    checked_file(source)?;
    // Evidence directories are newly created; never overwrite or follow a redirected output.
    if destination.try_exists()? || fs::symlink_metadata(destination).is_ok() {
        return Err(format!("Evidence output already exists: {}", destination.display()).into());
    }
    fs::copy(source, destination)?;
    let digest = file_hash(destination)?;
    if file_hash(source)? != digest {
        return Err("Evidence source changed during archival".into());
    }
    inventory.insert(
        destination.file_name().unwrap().to_string_lossy().into(),
        digest,
    );
    Ok(())
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .filter(|v| !v.is_null())
        .ok_or_else(|| format!("Missing comparison evidence: {key}").into())
}

fn text_field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    field(value, key)?
        .as_str()
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("Invalid comparison text: {key}").into())
}

fn digest_field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    let digest = text_field(value, key)?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!("Invalid comparison SHA-256: {key}").into());
    }
    Ok(digest)
}

fn differences(first: &Value, second: &Value) -> Value {
    let mut keys = std::collections::BTreeSet::new();
    for value in [first, second] {
        if let Some(object) = value.as_object() {
            keys.extend(object.keys());
        }
    }
    let mut changed = serde_json::Map::new();
    for key in keys {
        if first.get(key) != second.get(key) {
            changed.insert(
                key.clone(),
                json!({"first": first.get(key), "second": second.get(key)}),
            );
        }
    }
    Value::Object(changed)
}

fn compare(first: &Value, second: &Value, kernel: bool) -> Result<Value> {
    for report in [first, second] {
        if text_field(report, "result")? != "Ok" || field(report, "clean_build")? != true {
            return Err("Comparison requires successful independently clean builds".into());
        }
    }
    let a = field(first, "prepared")?;
    let b = field(second, "prepared")?;
    if text_field(a, "work_disk")? == text_field(b, "work_disk")? {
        return Err("Repeated builds reused the same work disk".into());
    }
    let keys = if kernel { KERNEL_INPUTS } else { BZIP2_INPUTS };
    let mut input_differences = serde_json::Map::new();
    for key in keys {
        if field(a, key)? != field(b, key)? {
            input_differences.insert((*key).into(), json!({"first": a[key], "second": b[key]}));
        }
    }
    let mut artifacts = serde_json::Map::new();
    if kernel {
        let a = digest_field(first, "guest_kernel_sha256")?;
        let b = digest_field(second, "guest_kernel_sha256")?;
        artifacts.insert(
            "kernel.elf".into(),
            json!({"first": a, "second": b, "equal": a == b}),
        );
    } else {
        for name in BZIP2_OUTPUTS {
            let a = digest_field(field(first, "files")?, name)?;
            let b = digest_field(field(second, "files")?, name)?;
            artifacts.insert(
                (*name).into(),
                json!({"first": a, "second": b, "equal": a == b}),
            );
        }
    }
    let equal =
        input_differences.is_empty() && artifacts.values().all(|entry| entry["equal"] == true);
    Ok(json!({
        "equal": equal, "input_differences": input_differences, "artifacts": artifacts,
        "prepared_metadata_differences": differences(a, b),
        "report_differences": differences(first, second),
        "policy": "Require identical declared inputs and full artifact bytes. Record all other report differences without normalizing ELF bytes or claiming ISO/disk reproducibility.",
    }))
}

fn snapshot(root: &Path, output: &Path, kernel: bool) -> Result<Value> {
    let name = if kernel {
        "bootstrap-kernel"
    } else {
        "bootstrap"
    };
    let work = root.join("build").join(name);
    let logs = root.join("build/logs");
    let report_path = logs.join(format!("{name}.result.json"));
    checked_file(&report_path)?;
    let report: Value = serde_json::from_slice(&fs::read(&report_path)?)?;
    if text_field(&report, "result")? != "Ok" {
        return Err("Cannot archive unsuccessful reconstruction as acceptance".into());
    }
    let mut inventory = BTreeMap::new();
    copy(&report_path, &output.join("result.json"), &mut inventory)?;
    for name in ["prepared.json", "inputs.json", "initramfs.anovafs"] {
        copy(&work.join(name), &output.join(name), &mut inventory)?;
    }
    let sessions: &[&str] = if kernel {
        &[
            "bootstrap-kernel-build",
            "bootstrap-kernel-reboot",
            "bootstrap-kernel-recovery",
        ]
    } else {
        &["bootstrap-capacity", "bootstrap-build", "bootstrap-reboot"]
    };
    for entry in fs::read_dir(&logs)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if sessions
            .iter()
            .any(|session| name.starts_with(&format!("{session}.")))
            || (kernel
                && matches!(
                    name.as_str(),
                    "bootstrap-kernel-build.cargo.log" | "bootstrap-kernel-failure.cargo.log"
                ))
        {
            copy(&entry.path(), &output.join(name), &mut inventory)?;
        }
    }
    let prepared = field(&report, "prepared")?;
    for (key, name) in if kernel {
        &[("seed_iso", "seed.iso"), ("work_disk", "work.ext2")][..]
    } else {
        &[("iso", "seed.iso"), ("work_disk", "work.ext2")][..]
    } {
        copy(
            Path::new(text_field(prepared, key)?),
            &output.join(name),
            &mut inventory,
        )?;
    }
    if kernel {
        copy(
            Path::new(text_field(&report, "guest_iso")?),
            &output.join("guest.iso"),
            &mut inventory,
        )?;
        copy(
            &work.join("guest-kernel.elf"),
            &output.join("kernel.elf"),
            &mut inventory,
        )?;
        if file_hash(&output.join("kernel.elf"))? != digest_field(&report, "guest_kernel_sha256")? {
            return Err("Archived kernel differs from accepted report".into());
        }
        let inspection = File::create_new(output.join("kernel.readelf.txt"))?;
        command(
            Command::new("readelf")
                .args(["-h", "-l", "-S", "-n", "-W"])
                .arg(output.join("kernel.elf"))
                .stdout(inspection),
        )?;
    } else {
        let mut disk = mount(&output.join("work.ext2"))?;
        let parent = disk
            .entries(2)
            .map_err(|e| format!("Archive disk: {e}"))?
            .into_iter()
            .find(|entry| &entry.name[..entry.length] == b"bootstrap")
            .ok_or("Missing archived bzip2 workspace")?
            .inode;
        let mut files = BTreeMap::new();
        read_tree(&mut disk, parent, "", &mut files)?;
        disk.shutdown()
            .map_err(|e| format!("Archive disk shutdown: {e}"))?;
        if serde_json::to_value(hashes(&files))? != *field(&report, "files")? {
            return Err("Archived bzip2 workspace differs from accepted report".into());
        }
        for name in BZIP2_OUTPUTS {
            fs::write(
                output.join(name),
                files.get(*name).ok_or("Missing bzip2 artifact")?,
            )?;
        }
    }
    for (source, name) in if kernel {
        &[("build/rust/stage.sha256", "rust-seeds.sha256")][..]
    } else {
        &[
            ("build/toolchain/stage.sha256", "toolchain-stage.sha256"),
            ("build/toolchain/package.sha256", "toolchain-package.sha256"),
        ][..]
    } {
        copy(&root.join(source), &output.join(name), &mut inventory)?;
    }
    // Hash every retained file, including invocation output, extracted artifacts and readelf data.
    inventory.clear();
    for entry in fs::read_dir(output)? {
        let entry = entry?;
        inventory.insert(
            entry.file_name().to_string_lossy().into_owned(),
            file_hash(&entry.path())?,
        );
    }
    write_json(&output.join("inventory.json"), &json!(inventory))?;
    Ok(report)
}

fn run(root: &Path, output: &Path, kernel: bool) -> Result<Value> {
    fs::create_dir(output)?;
    let log = File::create_new(output.join("invocation.log"))?;
    let name = if kernel {
        "bootstrap-kernel-test"
    } else {
        "bootstrap-test"
    };
    println!("Repeat gate: {name}; evidence {}", output.display());
    let status = Command::new(env::current_exe()?)
        .arg(name)
        .current_dir(root)
        .stdout(log.try_clone()?)
        .stderr(log)
        .status()?;
    if !status.success() {
        // Failed canonical files can be partial or stale; preserve them as unvalidated diagnostics.
        let prefix = if kernel {
            "bootstrap-kernel"
        } else {
            "bootstrap"
        };
        let failed = output.join("unvalidated");
        fs::create_dir(&failed)?;
        let mut inventory = BTreeMap::new();
        for directory in [root.join("build/logs"), root.join("build").join(prefix)] {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if entry.file_type()?.is_file()
                    && (name.starts_with(prefix)
                        || matches!(name.as_str(), "prepared.json" | "inputs.json"))
                {
                    copy(&entry.path(), &failed.join(name), &mut inventory)?;
                }
            }
        }
        write_json(
            &failed.join("failure.json"),
            &json!({
                "result": "Failed", "child_status": status.to_string(), "files_sha256": inventory,
                "warning": "Unvalidated canonical files may be partial or from earlier runs. Only invocation.log belongs unconditionally to this child. No acceptance is claimed.",
            }),
        )?;
        return Err(format!(
            "{name} failed ({status}); retained invocation {}",
            output.display()
        )
        .into());
    }
    snapshot(root, output, kernel)
}

pub fn verify(root: &Path) -> Result<()> {
    let result = directory(root, "logs")?.join("bootstrap-repeat.result.json");
    output_path(&result)?;
    if result.exists() {
        fs::remove_file(&result)?;
    }
    let work = directory(root, "bootstrap-repeat")?.join(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
            .to_string(),
    );
    fs::create_dir(&work)?;
    let source_files = sources(root)?;
    let source_snapshot = work.join("source");
    fs::create_dir(&source_snapshot)?;
    for name in source_files.keys() {
        let target = source_snapshot.join(name);
        fs::create_dir_all(target.parent().ok_or("Missing source parent")?)?;
        fs::copy(root.join(name), target)?;
    }
    write_json(&work.join("source-files.json"), &json!(source_files))?;
    let environment = json!({
        "source_commit": version("git", &["rev-parse", "HEAD"] )?,
        "source_branch": version("git", &["branch", "--show-current"] )?,
        "qemu": version("qemu-system-x86_64", &["--version"] )?,
        "rustc": version("rustc", &["--version"] )?,
        "musl_gcc": version("musl-gcc", &["--version"] )?,
        "xtask_executable_sha256": file_hash(&env::current_exe()?)?,
        "source_files_sha256": source_files,
    });
    write_json(&work.join("environment.json"), &environment)?;
    let start = Instant::now();
    let mut rounds = Vec::new();
    for index in 1..=2 {
        let output = work.join(format!("round-{index}"));
        fs::create_dir(&output)?;
        let bzip2 = run(root, &output.join("bzip2"), false)?;
        if sources(root)? != source_files {
            return Err("Repository inputs changed during repeated reconstruction".into());
        }
        let kernel = run(root, &output.join("kernel"), true)?;
        if sources(root)? != source_files {
            return Err("Repository inputs changed during repeated reconstruction".into());
        }
        rounds.push(json!({"evidence": output, "bzip2": bzip2, "kernel": kernel}));
    }
    let bzip2 = compare(&rounds[0]["bzip2"], &rounds[1]["bzip2"], false)?;
    let kernel = compare(&rounds[0]["kernel"], &rounds[1]["kernel"], true)?;
    let mut byte_equal = true;
    for (profile, names) in [("bzip2", BZIP2_OUTPUTS), ("kernel", &["kernel.elf"][..])] {
        for name in names {
            let first = fs::read(work.join("round-1").join(profile).join(name))?;
            let second = fs::read(work.join("round-2").join(profile).join(name))?;
            byte_equal &= first == second;
        }
    }
    let comparison = json!({"bzip2": bzip2, "kernel": kernel, "artifact_bytes_equal": byte_equal});
    write_json(&work.join("comparison.json"), &comparison)?;
    if !byte_equal || bzip2["equal"] != true || kernel["equal"] != true {
        return Err(format!(
            "Independent build comparison failed; see {}",
            work.join("comparison.json").display()
        )
        .into());
    }
    write_json(
        &result,
        &json!({
            "result": "Ok", "scope": "Two independent clean guest bzip2 and kernel builds, full artifact comparison and each gate's rejection/reboot/recovery",
        "rounds": rounds, "comparison": comparison, "evidence": work, "environment": environment,
            "host_wall_ms": start.elapsed().as_millis(), "boots": 12,
            "compiler_reconstruction": false, "image_generation_in_guest": false,
            "limitations": "Same inherited compilers/libraries, fixed guest paths and recipes. Two matching builds do not establish general reproducibility or seed-free bootstrap. Kernel failure uses a small rejected Rust source, not a full failed kernel Cargo rebuild. Host prepares sources/seeds/disks, compiles drivers/seed kernel/apps, packages and inspects images. Normal shutdown only; no power-loss recovery.",
        }),
    )?;
    println!(
        "Verified two independent clean bzip2/kernel builds and 12 boot/recovery sessions: {}",
        result.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(kernel: bool, disk: &str) -> Value {
        let mut inputs = serde_json::Map::new();
        for key in if kernel { KERNEL_INPUTS } else { BZIP2_INPUTS } {
            inputs.insert((*key).into(), json!("fixed"));
        }
        inputs.insert("work_disk".into(), json!(disk));
        let mut files = serde_json::Map::new();
        for key in BZIP2_OUTPUTS {
            files.insert((*key).into(), json!("a".repeat(64)));
        }
        json!({"result": "Ok", "clean_build": true, "prepared": inputs,
            "files": files, "guest_kernel_sha256": "b".repeat(64)})
    }

    #[test]
    fn records_metadata_differences_but_requires_equal_inputs_and_artifacts() {
        for kernel in [false, true] {
            let first = report(kernel, "fresh-1");
            let mut second = report(kernel, "fresh-2");
            second["prepared"]["iso_sha256"] = json!("timestamp differs");
            let comparison = compare(&first, &second, kernel).unwrap();
            assert_eq!(comparison["equal"], true);
            assert!(
                !comparison["prepared_metadata_differences"]
                    .as_object()
                    .unwrap()
                    .is_empty()
            );
            second["prepared"]["target"] = json!("drift");
            assert_eq!(compare(&first, &second, kernel).unwrap()["equal"], false);
        }
    }

    #[test]
    fn rejects_stale_incomplete_reused_or_changed_evidence() {
        for kernel in [false, true] {
            let first = report(kernel, "fresh-1");
            assert!(compare(&first, &first, kernel).is_err());
            for key in ["result", "clean_build", "prepared"] {
                let mut second = report(kernel, "fresh-2");
                second.as_object_mut().unwrap().remove(key);
                assert!(compare(&first, &second, kernel).is_err());
            }
            let mut second = report(kernel, "fresh-2");
            second["clean_build"] = json!(false);
            assert!(compare(&first, &second, kernel).is_err());
            let mut second = report(kernel, "fresh-2");
            if kernel {
                second["guest_kernel_sha256"] = json!("c".repeat(64));
            } else {
                second["files"]["blocksort.o"] = json!("c".repeat(64));
            }
            assert_eq!(compare(&first, &second, kernel).unwrap()["equal"], false);
            if kernel {
                second["guest_kernel_sha256"] = json!("invalid");
            } else {
                second["files"]["bzip2"] = Value::Null;
            }
            assert!(compare(&first, &second, kernel).is_err());
        }
    }

    #[test]
    fn archival_rejects_redirected_inputs_and_outputs() {
        let base = env::temp_dir().join(format!(
            "anova-repeat-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&base).unwrap();
        let source = base.join("source");
        fs::write(&source, b"retained artifact").unwrap();
        let target = base.join("copy");
        let mut inventory = BTreeMap::new();
        copy(&source, &target, &mut inventory).unwrap();
        assert!(copy(&source, &target, &mut inventory).is_err());
        #[cfg(unix)]
        {
            let redirect = base.join("redirect");
            std::os::unix::fs::symlink(&source, &redirect).unwrap();
            assert!(copy(&redirect, &base.join("bad-copy"), &mut inventory).is_err());
            assert!(copy(&source, &redirect, &mut inventory).is_err());
            assert_eq!(fs::read(&source).unwrap(), b"retained artifact");
        }
        fs::remove_dir_all(base).unwrap();
    }
}
