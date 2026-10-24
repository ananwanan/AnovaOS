// SPDX-License-Identifier: Apache-2.0
//! Audit locked GCC inputs and verify guest seed/dependency/configure blockers with reboot evidence.
//! A successful assessment reports blocked feasibility; it never claims compiler reconstruction.
use super::*;
use bootstrap::{checked, disk_capacity, hash, hashes, mount, read_tree, session};
use serde_json::{Value, json};
use std::collections::BTreeMap;

type Files = BTreeMap<String, Vec<u8>>;
const MISSING_TOOLS: [&str; 9] = [
    "sed", "awk", "grep", "expr", "tr", "sort", "cmp", "mv", "ln",
];

fn storage(error: i64) -> Box<dyn Error> {
    format!("M15 GCC probe ext2 operation failed: {error}").into()
}

fn diagnostic(root: &Path, work: &Path, name: &str, source: &str) -> Result<Vec<u8>> {
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
            .arg(root.join(source))
            .arg("-o")
            .arg(&path),
    )?;
    let bytes = fs::read(path)?;
    anova_elf::parse_linux(&bytes).map_err(|e| format!("Invalid GCC diagnostic: {e:?}"))?;
    Ok(bytes)
}

fn inputs(root: &Path, work: &Path) -> Result<Files> {
    let mut files = Files::new();
    for entry in fs::read_dir(work.join("source"))? {
        let entry = entry?;
        checked_file(&entry.path())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Non-UTF8 GCC probe input")?;
        files.insert(name, fs::read(entry.path())?);
    }
    files.insert(
        "seed.cpp".into(),
        fs::read(root.join("tests/guest/m15/gcc_seed.cpp"))?,
    );
    for library in ["gmp", "mpfr", "mpc"] {
        files.insert(
            format!("{library}.cpp"),
            format!("// Probe a separately required compiler development header.\n#include <{library}.h>\n").into_bytes(),
        );
    }
    Ok(files)
}

fn inspect(path: &Path, inputs: &Files) -> Result<Files> {
    let mut disk = mount(path)?;
    let parent = disk
        .entries(2)
        .map_err(storage)?
        .into_iter()
        .find(|entry| &entry.name[..entry.length] == b"gcc-probe")
        .ok_or("Missing GCC probe workspace")?
        .inode;
    let mut files = Files::new();
    read_tree(&mut disk, parent, "", &mut files)?;
    for (name, expected) in inputs {
        if files.get(name) != Some(expected) {
            return Err(format!("GCC probe input changed: {name}").into());
        }
    }
    let entry = disk
        .entries(parent)
        .map_err(storage)?
        .into_iter()
        .find(|entry| &entry.name[..entry.length] == b"seed")
        .ok_or("Missing guest-built C++11 seed probe")?;
    if !disk.inode(entry.inode).map_err(storage)?.executable {
        return Err("Guest-built seed probe is not executable".into());
    }
    anova_elf::parse_linux(files.get("seed").ok_or("Missing seed ELF bytes")?)
        .map_err(|e| format!("Invalid guest seed probe: {e:?}"))?;
    disk.shutdown().map_err(storage)?;
    let log = std::str::from_utf8(files.get("configure.log").ok_or("Missing configure log")?)?;
    if !log.contains("sed: not found") || log.contains("cannot find sources") {
        return Err("Configure did not reach the declared missing-tool blocker".into());
    }
    for library in ["gmp", "mpfr", "mpc"] {
        if files.contains_key(&format!("{library}.o")) {
            return Err("Rejected header probe unexpectedly left an object".into());
        }
    }
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(path))?;
    Ok(files)
}

fn evidence(serial: &str) -> Result<()> {
    for marker in [
        "[M15:GCC:SEED] cxx11=ok value=42",
        "[M15:GCC:CONFIGURE]",
        "selected-entry-point=blocked",
        "[M15:CAPACITY] files=efbig-preserved",
    ] {
        if !serial.contains(marker) {
            return Err(format!("Missing actual GCC feasibility evidence: {marker}").into());
        }
    }
    if serial.contains("[M15:GCC:FAIL]") {
        return Err("GCC feasibility diagnostic failed".into());
    }
    for tool in MISSING_TOOLS {
        let marker = format!("[M15:GCC:TOOL] name={tool} exec_errno=2");
        if !serial.lines().any(|line| line.trim_end() == marker) {
            return Err(format!("Missing exec failure evidence for {tool}").into());
        }
    }
    for library in ["gmp", "mpfr", "mpc"] {
        if !serial.contains(&format!(
            "fatal error: {library}.h: No such file or directory"
        )) {
            return Err(format!("Missing compiler header rejection for {library}").into());
        }
    }
    Ok(())
}

pub fn verify(root: &Path) -> Result<()> {
    let result = directory(root, "logs")?.join("bootstrap-gcc.result.json");
    output_path(&result)?;
    if result.exists() {
        fs::remove_file(&result)?;
    }
    directory(root, "bootstrap-gcc")?;
    command(Command::new("python3").arg(root.join("ports/bootstrap/gcc_probe.py")))?;
    let work = directory(root, "bootstrap-gcc")?;
    let audit: Value = serde_json::from_slice(&fs::read(work.join("source-audit.json"))?)?;
    if audit["profile"] != "m15-gcc-11.2.0-feasibility" || audit["full_source_tree_fits"] != false {
        return Err("Unexpected GCC source audit profile; reevaluate feasibility".into());
    }
    apps::build(root, false)?;
    let mut archive = fs::read(toolchain::pack(root)?)?;
    let mut diagnostics = BTreeMap::new();
    for (name, source) in [
        ("m15-capacity", "tests/guest/m15/capacity.c"),
        ("m15-gcc-probe", "tests/guest/m15/gcc_probe.c"),
    ] {
        let bytes = diagnostic(root, &work, name, source)?;
        diagnostics.insert(name, hash(&bytes));
        toolchain::append(&mut archive, 5, &format!("/bin/{name}"), &bytes);
    }
    let packed = work.join("initramfs.anovafs");
    output_path(&packed)?;
    fs::write(&packed, &archive)?;
    let iso = image_archive(root, Mode::Bootstrap, Some(&packed))?;
    let path = directory(root, "disks")?.join(format!(
        "bootstrap-gcc-{}.ext2",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    disk::create_profile(root, &path, true)?;
    let source = inputs(root, &work)?;
    let mut disk = mount(&path)?;
    let parent = disk.create(2, "gcc-probe", true).map_err(storage)?;
    for (name, bytes) in &source {
        let inode = disk.create(parent, name, false).map_err(storage)?;
        disk.write(inode, 0, bytes).map_err(storage)?;
    }
    disk.shutdown().map_err(storage)?;
    let implementation: Files = [
        "xtask/src/bootstrap_gcc.rs",
        "xtask/src/bootstrap.rs",
        "xtask/src/main.rs",
        "ports/bootstrap/gcc_probe.py",
        "tests/guest/m15/gcc_probe.c",
        "tests/guest/m15/gcc_seed.cpp",
        "tests/guest/m15/capacity.c",
    ]
    .into_iter()
    .map(|name| Ok((name.into(), fs::read(root.join(name))?)))
    .collect::<Result<_>>()?;
    let provenance = json!({
        "source_branch": version("git", &["branch", "--show-current"] )?,
        "implementation_files_sha256": hashes(&implementation),
        "source_commit": version("git", &["rev-parse", "HEAD"] )?,
        "source_worktree": version("git", &["status", "--porcelain"] )?,
        "build_host_target": "AnovaOS x86_64 Linux-musl compatibility personality",
        "seed": "M11 GCC/G++ 11.2.0, binutils 2.37, make 4.4.1, musl 1.2.4; host prepared",
        "seed_source_inventory_sha256": hash(&fs::read(root.join("build/toolchain/stage.sha256"))?),
        "seed_package_inventory_sha256": hash(&fs::read(root.join("build/toolchain/package.sha256"))?),
        "host_compiled_diagnostics_sha256": diagnostics,
        "host_participation": "Audit/select locked source, package seeds and diagnostics, create work disk, boot and inspect; no compiler reconstruction",
        "input_files": hashes(&source), "source_audit": audit,
        "iso": iso, "iso_sha256": hash(&fs::read(&iso)?),
        "initramfs_sha256": hash(&archive), "work_disk": path,
        "ram_bytes": 1024 * 1024 * 1024u64, "cpus": 1, "network": false,
    });
    let prepared = work.join("prepared.json");
    output_path(&prepared)?;
    fs::write(prepared, serde_json::to_vec_pretty(&provenance)?)?;
    let mut steps = Vec::new();
    checked(&mut steps, "cd /persist/gcc-probe", 0);
    checked(&mut steps, "g++ --version", 0);
    checked(&mut steps, "make --version", 0);
    checked(&mut steps, "g++ -std=c++11 -O1 seed.cpp -o seed", 0);
    checked(&mut steps, "./seed", 0);
    for library in ["gmp", "mpfr", "mpc"] {
        checked(
            &mut steps,
            &format!("g++ -c {library}.cpp -o {library}.o"),
            1,
        );
    }
    checked(&mut steps, "m15-gcc-probe", 0);
    checked(&mut steps, "m15-capacity", 0);
    checked(&mut steps, "./seed", 0);
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
    let probe = session(root, &iso, &path, "bootstrap-gcc-probe", &steps)?;
    let serial = fs::read_to_string(root.join("build/logs/bootstrap-gcc-probe.serial.log"))?;
    evidence(&serial)?;
    let first = inspect(&path, &source)?;
    let configure_log = directory(root, "logs")?.join("bootstrap-gcc-configure.log");
    output_path(&configure_log)?;
    fs::write(configure_log, &first["configure.log"])?;
    steps.clear();
    checked(&mut steps, "cd /persist/gcc-probe", 0);
    checked(&mut steps, "./seed", 0);
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
    let reboot = session(root, &iso, &path, "bootstrap-gcc-reboot", &steps)?;
    let reboot_serial =
        fs::read_to_string(root.join("build/logs/bootstrap-gcc-reboot.serial.log"))?;
    if !reboot_serial.contains("[M15:GCC:SEED] cxx11=ok value=42") {
        return Err("Persisted seed probe did not execute after reboot".into());
    }
    if first != inspect(&path, &source)? {
        return Err("GCC probe files changed across reboot".into());
    }
    let report = json!({
        "result": "Ok", "profile": "M15.3 GCC 11.2.0 feasibility assessment",
        "feasibility": "blocked in the current profile", "boots": 2, "prepared": provenance,
        "probe": probe, "reboot": reboot, "disk": disk_capacity(&path)?,
        "files": hashes(&first), "reboot_hashes": "identical", "e2fsck": "clean after both boots",
        "verified": ["guest C++11 seed compilation/execution", "missing development headers",
            "missing build-tool exec rejection", "selected upstream configure entry-point rejection",
            "resource-limit rejection/recovery", "persisted seed-probe execution after reboot"],
        "compiler_reconstruction": "not attempted; no new GCC stage produced",
        "multistage_bootstrap": "not attempted", "kernel_reconstruction": "not attempted",
        "measurement_limits": "Combined diagnostic session includes exhaustion probes; boundary samples are not compiler demand or peak RSS; full release inventories are not minimal build requirements",
    });
    fs::write(&result, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "Verified GCC feasibility blockers and reboot evidence: {}",
        result.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete() -> String {
        let mut serial = String::from(
            "[M15:GCC:SEED] cxx11=ok value=42\n[M15:GCC:CONFIGURE] exit=1 selected-entry-point=blocked\n[M15:CAPACITY] files=efbig-preserved recovery=ok\n",
        );
        for tool in MISSING_TOOLS {
            serial.push_str(&format!("[M15:GCC:TOOL] name={tool} exec_errno=2\n"));
        }
        for library in ["gmp", "mpfr", "mpc"] {
            serial.push_str(&format!(
                "fatal error: {library}.h: No such file or directory\n"
            ));
        }
        serial
    }

    #[test]
    fn requires_each_real_dependency_failure_and_seed_execution() {
        let serial = complete();
        assert!(evidence(&serial).is_ok());
        for marker in [
            "name=sed",
            "mpfr.h",
            "cxx11=ok",
            "selected-entry-point=blocked",
        ] {
            assert!(evidence(&serial.replace(marker, "missing")).is_err());
        }
        assert!(evidence(&(serial + "[M15:GCC:FAIL]")).is_err());
    }
}
