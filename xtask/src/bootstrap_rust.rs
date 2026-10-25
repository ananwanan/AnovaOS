// SPDX-License-Identifier: Apache-2.0
//! Assess locked Rust/LLVM/Cargo source prerequisites with real guest seeds and reboot evidence.
//! Source inventories and diagnostic probes do not establish compiler or Cargo reconstruction.
use super::*;
use bootstrap::{checked, disk_capacity, hash, hashes, mount, read_tree, session};
use serde_json::{Value, json};
use std::collections::BTreeMap;

type Files = BTreeMap<String, Vec<u8>>;
const MISSING_TOOLS: [&str; 6] = ["python3", "cmake", "ninja", "git", "perl", "pkg-config"];
const SEEDS: [(&str, &str); 2] = [
    ("seed-rust", "[M15:RUST:SEED] std=ok value=42"),
    ("seed-llvm", "[M15:LLVM:SEED] cxx17=ok value=42"),
];

fn storage(error: i64) -> Box<dyn Error> {
    format!("M15 Rust probe ext2 operation failed: {error}").into()
}

fn collect(base: &Path, prefix: &str, files: &mut Files) -> Result<()> {
    for entry in fs::read_dir(base)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Non-UTF8 Rust probe input")?;
        let key = format!("{prefix}{name}");
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect(&entry.path(), &format!("{key}/"), files)?;
        } else if kind.is_file() {
            checked_file(&entry.path())?;
            if files.insert(key.clone(), fs::read(entry.path())?).is_some() {
                return Err(format!("Duplicate Rust probe input: {key}").into());
            }
        } else {
            return Err("Rust probe inputs must be regular files or directories".into());
        }
    }
    Ok(())
}

fn import(disk: &mut anova_storage::ext2::Ext2, parent: u32, files: &Files) -> Result<()> {
    let mut directories = BTreeMap::from([(String::new(), parent)]);
    for (name, bytes) in files {
        let mut parent = parent;
        let mut prefix = String::new();
        let mut parts = name.split('/').peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_some() {
                prefix.push_str(part);
                prefix.push('/');
                parent = match directories.get(&prefix) {
                    Some(inode) => *inode,
                    None => {
                        let inode = disk.create(parent, part, true).map_err(storage)?;
                        directories.insert(prefix.clone(), inode);
                        inode
                    }
                };
            } else {
                let inode = disk.create(parent, part, false).map_err(storage)?;
                disk.write(inode, 0, bytes).map_err(storage)?;
            }
        }
    }
    Ok(())
}

fn inspect(path: &Path, inputs: &Files) -> Result<Files> {
    let mut disk = mount(path)?;
    let parent = disk
        .entries(2)
        .map_err(storage)?
        .into_iter()
        .find(|entry| &entry.name[..entry.length] == b"rust-probe")
        .ok_or("Missing Rust probe workspace")?
        .inode;
    let mut files = Files::new();
    read_tree(&mut disk, parent, "", &mut files)?;
    for (name, expected) in inputs {
        if files.get(name) != Some(expected) {
            return Err(format!("Rust probe input changed: {name}").into());
        }
    }
    let entries = disk.entries(parent).map_err(storage)?;
    for (name, _) in SEEDS {
        let entry = entries
            .iter()
            .find(|entry| &entry.name[..entry.length] == name.as_bytes())
            .ok_or_else(|| format!("Missing guest-built probe: {name}"))?;
        if !disk.inode(entry.inode).map_err(storage)?.executable {
            return Err(format!("Guest-built probe is not executable: {name}").into());
        }
        anova_elf::parse_linux(files.get(name).ok_or("Missing guest seed ELF bytes")?)
            .map_err(|e| format!("Invalid guest seed probe {name}: {e:?}"))?;
    }
    if files.get("seed-before") != files.get("seed-rust") {
        return Err("Rejected Rust compilation changed the retained seed executable".into());
    }
    let cargo_log = std::str::from_utf8(
        files
            .get("cargo-dependency.log")
            .ok_or("Missing captured Cargo dependency rejection")?,
    )?;
    if !cargo_log.contains("no matching package named `curl` found")
        || !cargo_log.contains("/rs/vendor")
    {
        return Err(
            "Persisted Cargo log does not establish the selected dependency blocker".into(),
        );
    }
    disk.shutdown().map_err(storage)?;
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(path))?;
    Ok(files)
}

fn evidence(serial: &str) -> Result<()> {
    if serial.contains("[M15:RUST:FAIL]") {
        return Err("Rust feasibility diagnostic failed".into());
    }
    for marker in SEEDS.into_iter().map(|(_, marker)| marker).chain([
        "[M15:RUST:BOOTSTRAP] entry=rust-x.py exec_errno=2",
        "[M15:CARGO:DEPENDENCY] name=curl version=0.4.48 exit=101 offline-vendor=missing",
    ]) {
        if !serial.lines().any(|line| line.trim_end() == marker) {
            return Err(format!("Missing actual Rust feasibility evidence: {marker}").into());
        }
    }
    if !serial.lines().any(|line| {
        line.starts_with("[M15:CAPACITY] files=efbig-preserved ")
            && line.trim_end().ends_with(" recovery=ok")
    }) || !serial.contains("expected expression")
    {
        return Err("Missing capacity recovery or actual Rust syntax rejection".into());
    }
    for tool in MISSING_TOOLS {
        let marker = format!("[M15:RUST:TOOL] name={tool} exec_errno=2");
        if !serial.lines().any(|line| line.trim_end() == marker) {
            return Err(format!("Missing actual Rust build-tool exec failure: {tool}").into());
        }
    }
    Ok(())
}

pub fn verify(root: &Path) -> Result<()> {
    let result = directory(root, "logs")?.join("bootstrap-rust.result.json");
    output_path(&result)?;
    if result.exists() {
        checked_file(&result)?;
        fs::remove_file(&result)?;
    }
    directory(root, "bootstrap-rust")?;
    command(Command::new("python3").arg(root.join("ports/bootstrap/rust_probe.py")))?;
    let work = directory(root, "bootstrap-rust")?;
    let audit: Value = serde_json::from_slice(&fs::read(work.join("source-audit.json"))?)?;
    if audit["profile"] != "m15-rust-1.90.0-feasibility" || audit["full_source_tree_fits"] != false
    {
        return Err("Unexpected Rust source audit profile; reevaluate feasibility".into());
    }
    let mut source = Files::new();
    collect(&work.join("source"), "", &mut source)?;
    collect(&root.join("ports/bootstrap/rust-probe"), "", &mut source)?;
    let mut archive = fs::read(rust::pack(root, true)?)?;
    let mut diagnostics = BTreeMap::new();
    for (name, fixture) in [
        ("m15-capacity", "tests/guest/m15/capacity.c"),
        ("m15-rust-probe", "ports/bootstrap/rust-probe/probe.c"),
    ] {
        let path = work.join(format!("{name}.elf"));
        output_path(&path)?;
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
                .arg(root.join(fixture))
                .arg("-o")
                .arg(&path),
        )?;
        let bytes = fs::read(path)?;
        anova_elf::parse_linux(&bytes).map_err(|e| format!("Invalid Rust diagnostic: {e:?}"))?;
        diagnostics.insert(name, hash(&bytes));
        toolchain::append(&mut archive, 5, &format!("/bin/{name}"), &bytes);
    }
    let packed = work.join("initramfs.anovafs");
    output_path(&packed)?;
    fs::write(&packed, &archive)?;
    let built = image_archive(root, Mode::Bootstrap, Some(&packed))?;
    let iso = directory(root, "images")?.join("anova-bootstrap-rust.iso");
    output_path(&iso)?;
    fs::rename(built, &iso)?;
    let path = directory(root, "disks")?.join(format!(
        "bootstrap-rust-{}.ext2",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    disk::create_profile(root, &path, true)?;
    let mut disk = mount(&path)?;
    let parent = disk.create(2, "rust-probe", true).map_err(storage)?;
    import(&mut disk, parent, &source)?;
    disk.shutdown().map_err(storage)?;
    let mut implementation = Files::new();
    for name in [
        "xtask/src/bootstrap_rust.rs",
        "xtask/src/bootstrap.rs",
        "xtask/src/main.rs",
        "xtask/src/rust.rs",
        "crates/vfs/src/lib.rs",
        "crates/vfs/src/tests.rs",
        "tests/guest/m12/runtime.c",
        "ports/bootstrap/rust_probe.py",
        "ports/bootstrap/rust-inputs.json",
        "tests/guest/m15/capacity.c",
    ] {
        implementation.insert(name.into(), fs::read(root.join(name))?);
    }
    collect(
        &root.join("ports/bootstrap/rust-probe"),
        "ports/bootstrap/rust-probe/",
        &mut implementation,
    )?;
    let provenance = json!({
        "source_branch": version("git", &["branch", "--show-current"] )?,
        "source_commit": version("git", &["rev-parse", "HEAD"] )?,
        "source_worktree": version("git", &["status", "--porcelain"] )?,
        "implementation_files_sha256": hashes(&implementation),
        "build_host": "AnovaOS x86_64 Linux-musl compatibility personality",
        "target": "x86_64-unknown-linux-musl; no native compiler or bare-metal kernel construction",
        "seed": "M12 official Rust/Cargo 1.90.0; M11 GCC/G++ 11.2.0 and musl sysroot; host prepared",
        "rust_seed_inventory_sha256": hash(&fs::read(root.join("build/rust/stage.sha256"))?),
        "c_seed_source_inventory_sha256": hash(&fs::read(root.join("build/toolchain/stage.sha256"))?),
        "c_seed_package_inventory_sha256": hash(&fs::read(root.join("build/toolchain/package.sha256"))?),
        "host_compiled_diagnostics_sha256": diagnostics,
        "host_participation": "Audit/select locked sources, package inherited seeds/diagnostics, create disk, boot and inspect; no compiler source reconstruction",
        "input_files": hashes(&source), "source_audit": audit,
        "iso": iso, "iso_sha256": hash(&fs::read(&iso)?),
        "initramfs_sha256": hash(&archive), "work_disk": path,
        "ram_bytes": 3 * 1024 * 1024 * 1024u64, "cpus": 1, "network": false,
    });
    let prepared = work.join("prepared.json");
    output_path(&prepared)?;
    fs::write(prepared, serde_json::to_vec_pretty(&provenance)?)?;
    let mut steps = Vec::new();
    checked(&mut steps, "cd /persist/rust-probe", 0);
    checked(&mut steps, "rustc --version", 0);
    checked(&mut steps, "cargo --version", 0);
    checked(
        &mut steps,
        "rustc --edition=2024 -C opt-level=1 seed.rs -o seed-rust",
        0,
    );
    checked(
        &mut steps,
        "g++ -std=c++17 -O1 llvm_seed.cpp -o seed-llvm",
        0,
    );
    for (name, _) in SEEDS {
        checked(&mut steps, &format!("./{name}"), 0);
    }
    checked(&mut steps, "cp seed-rust seed-before", 0);
    checked(&mut steps, "rustc --edition=2024 bad.rs -o seed-rust", 1);
    checked(&mut steps, "m15-rust-probe", 0);
    checked(&mut steps, "m15-capacity", 0);
    for (name, _) in SEEDS {
        checked(&mut steps, &format!("./{name}"), 0);
    }
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
    let probe = session(root, &iso, &path, "bootstrap-rust-probe", &steps)?;
    let serial = fs::read_to_string(root.join("build/logs/bootstrap-rust-probe.serial.log"))?;
    evidence(&serial)?;
    let first = inspect(&path, &source)?;
    let cargo_log = directory(root, "logs")?.join("bootstrap-rust-cargo-dependency.log");
    output_path(&cargo_log)?;
    fs::write(cargo_log, &first["cargo-dependency.log"])?;
    steps.clear();
    checked(&mut steps, "cd /persist/rust-probe", 0);
    for (name, _) in SEEDS {
        checked(&mut steps, &format!("./{name}"), 0);
    }
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
    let reboot = session(root, &iso, &path, "bootstrap-rust-reboot", &steps)?;
    let reboot_serial =
        fs::read_to_string(root.join("build/logs/bootstrap-rust-reboot.serial.log"))?;
    for (_, marker) in SEEDS {
        if !reboot_serial.lines().any(|line| line.trim_end() == marker) {
            return Err("Persisted guest seed probe did not execute after reboot".into());
        }
    }
    if first != inspect(&path, &source)? {
        return Err("Rust probe workspace bytes changed across reboot".into());
    }
    let report = json!({
        "result": "Ok", "profile": "M15.4 Rust 1.90.0 / LLVM / Cargo feasibility assessment",
        "feasibility": "blocked in the current profile", "boots": 2, "prepared": provenance,
        "probe": probe, "reboot": reboot, "disk": disk_capacity(&path)?,
        "files": hashes(&first), "reboot_hashes": "identical", "e2fsck": "clean after both boots",
        "verified": ["guest Rust std and C++17 seed compilation/execution", "actual build-tool and Python entry exec failure", "offline dependency rejection", "actual Rust compilation rejection with byte-identical prior ELF", "resource rejection/recovery", "persisted seed execution after reboot"],
        "reconstructed_components": [], "rustc_reconstruction": "not attempted",
        "llvm_reconstruction": "not attempted", "cargo_reconstruction": "not attempted",
        "measurement_limits": "Full-source inventories are not minimal build requirements; boundary samples include deliberate exhaustion and are not compiler demand or peak RSS",
    });
    fs::write(&result, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "Verified Rust/LLVM/Cargo feasibility evidence: {}",
        result.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete() -> String {
        let mut serial = String::from(
            "[M15:RUST:SEED] std=ok value=42\n[M15:LLVM:SEED] cxx17=ok value=42\n\
             [M15:RUST:BOOTSTRAP] entry=rust-x.py exec_errno=2\n\
             [M15:CARGO:DEPENDENCY] name=curl version=0.4.48 exit=101 offline-vendor=missing\n\
             [M15:CAPACITY] files=efbig-preserved fds=29 children=29 recovery=ok\n\
             error: expected expression, found `;`\n",
        );
        for tool in MISSING_TOOLS {
            serial.push_str(&format!("[M15:RUST:TOOL] name={tool} exec_errno=2\n"));
        }
        serial
    }

    #[test]
    fn rejects_missing_or_failed_component_prerequisite_evidence() {
        let serial = complete();
        assert!(evidence(&serial).is_ok());
        for marker in [
            "std=ok",
            "cxx17=ok",
            "entry=rust-x.py",
            "offline-vendor=missing",
            "expected expression",
            "recovery=ok",
            "name=cmake",
        ] {
            assert!(evidence(&serial.replace(marker, "missing")).is_err());
        }
        assert!(evidence(&(serial + "[M15:RUST:FAIL]")).is_err());
    }
}
