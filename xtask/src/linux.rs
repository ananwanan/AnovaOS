// SPDX-License-Identifier: Apache-2.0
//! Build pinned BusyBox profiles and musl/socket fixtures, and verify shared libc linkage.
//! Keep source/license/config evidence beside generated binaries; never import BusyBox into the kernel.
use super::*;
use sha2::{Digest, Sha256};
use std::io::Write;
const COMMIT: &str = "1a64f6a20aaf6ea4dbba68bbfa8cc1ab7e5c57c4";
const BOOTSTRAP_CONFIG: &str = "user/linux/busybox-bootstrap.config";
const BOOTSTRAP_APPLETS: &[&str] = &[
    "awk", "basename", "cat", "chmod", "cmp", "cp", "cut", "diff", "dirname", "echo", "egrep",
    "env", "expr", "false", "find", "grep", "head", "install", "kill", "ln", "ls", "mkdir",
    "mktemp", "mv", "printf", "pwd", "rm", "rmdir", "sed", "sh", "sleep", "sort", "tail", "test",
    "touch", "tr", "true", "uname", "uniq", "wc",
];

#[derive(Clone, Copy)]
enum BusyBoxProfile {
    Compatibility,
    Terminal,
    Bootstrap,
}

pub fn build(root: &Path, apps: &Path) -> Result<()> {
    build_busybox(root, apps, BusyBoxProfile::Compatibility)
}

/// Build a separate pinned ash with foreground job control; keep the M8/M9 seed unchanged.
pub fn build_terminal(root: &Path, apps: &Path) -> Result<()> {
    build_busybox(root, apps, BusyBoxProfile::Terminal)
}

/// Admit the separate M18 applet set without changing the original M15 image or its PATH.
pub(super) fn append_bootstrap(root: &Path, archive: &mut Vec<u8>) -> Result<serde_json::Value> {
    let work = directory(root, "compat-bootstrap")?;
    let manifest = work.join("manifest.toml");
    output_path(&manifest)?;
    fs::write(&manifest, "# Generated M18 BusyBox build identity.\n")?;
    build_busybox(root, &work, BusyBoxProfile::Bootstrap)?;
    let bytes = fs::read(work.join("busybox.elf"))?;
    toolchain::append(archive, 2, "/bootstrap", &[]);
    toolchain::append(archive, 5, "/bootstrap/busybox", &bytes);
    for name in BOOTSTRAP_APPLETS {
        toolchain::append(archive, 3, &format!("/bootstrap/{name}"), b"busybox");
    }
    toolchain::append(
        archive,
        1,
        "/bootstrap/LICENSE",
        &fs::read(work.join("busybox.LICENSE"))?,
    );
    let corresponding_source = root.join("build/images/busybox-bootstrap-1.36.1-source.tar.gz");
    Ok(serde_json::json!({
        "version": "1.36.1",
        "source_commit": COMMIT,
        "built_on": "host, static musl Linux ELF",
        "applets": BOOTSTRAP_APPLETS,
        "elf_sha256": format!("{:x}", Sha256::digest(&bytes)),
        "resolved_config_sha256": format!("{:x}", Sha256::digest(fs::read(work.join("busybox.config"))?)),
        "corresponding_source_sha256": format!("{:x}", Sha256::digest(fs::read(&corresponding_source)?)),
        "corresponding_source": corresponding_source,
    }))
}

fn build_busybox(root: &Path, apps: &Path, profile: BusyBoxProfile) -> Result<()> {
    let (work_name, source_name) = match profile {
        BusyBoxProfile::Compatibility => ("compat", "busybox-1.36.1-source.tar.gz"),
        BusyBoxProfile::Terminal => ("compat-terminal", "busybox-terminal-1.36.1-source.tar.gz"),
        BusyBoxProfile::Bootstrap => ("compat-bootstrap", "busybox-bootstrap-1.36.1-source.tar.gz"),
    };
    let work = directory(root, work_name)?;
    let source = work.join("busybox");
    if !source.exists() {
        // M18 has already prepared the same pinned source for the baseline initramfs.
        // Clone separately so per-profile Kconfig and objects never overwrite older evidence.
        let upstream = if matches!(profile, BusyBoxProfile::Bootstrap) {
            root.join("build/compat/busybox")
        } else {
            PathBuf::from("https://github.com/mirror/busybox.git")
        };
        command(
            Command::new("git")
                .args(["clone", "--no-local", "--depth", "1", "--branch", "1_36_1"])
                .arg(upstream)
                .arg(&source),
        )?;
    }
    if !fs::symlink_metadata(&source)?.is_dir() {
        return Err("BusyBox source must be a real directory".into());
    }
    let revision = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !revision.status.success() || String::from_utf8(revision.stdout)?.trim() != COMMIT {
        return Err(
            "BusyBox source commit differs from the pinned upstream 1.36.1 revision".into(),
        );
    }
    command(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["diff", "--exit-code", "HEAD", "--"])
            .stdout(Stdio::null()),
    )?;
    command(
        Command::new("make")
            .current_dir(&source)
            .arg("allnoconfig")
            .stdout(Stdio::null()),
    )?;
    let config_path = source.join(".config");
    let mut config = fs::read_to_string(&config_path)?;
    let mut selected = fs::read_to_string(root.join("user/linux/busybox.config"))?;
    match profile {
        BusyBoxProfile::Compatibility => {}
        BusyBoxProfile::Terminal => selected.push_str("\nCONFIG_ASH_JOB_CONTROL=y\n"),
        BusyBoxProfile::Bootstrap => {
            selected.push('\n');
            selected.push_str(&fs::read_to_string(root.join(BOOTSTRAP_CONFIG))?);
        }
    }
    for line in selected.lines().filter(|l| l.starts_with("CONFIG_")) {
        let key = line.split_once('=').ok_or("Invalid BusyBox profile")?.0;
        let disabled = format!("# {key} is not set");
        let previous =
            if matches!(profile, BusyBoxProfile::Bootstrap) && key == "CONFIG_BUSYBOX_EXEC_PATH" {
                // The pinned ash retries scripts through this path after ENOEXEC.
                "CONFIG_BUSYBOX_EXEC_PATH=\"/proc/self/exe\""
            } else {
                &disabled
            };
        if config.lines().any(|setting| setting == previous) {
            config = config.replacen(previous, line, 1);
        } else if !config.lines().any(|l| l == line) {
            return Err(format!("Unrecognized BusyBox profile setting: {line}").into());
        }
    }
    fs::write(&config_path, config)?;
    let answers = work.join("default-answers.txt");
    fs::write(&answers, "\n".repeat(2048))?;
    command(
        Command::new("make")
            .current_dir(&source)
            .arg("oldconfig")
            .stdin(File::open(answers)?)
            .stdout(Stdio::null()),
    )?;
    let log = File::create(work.join("busybox-build.log"))?;
    command(
        Command::new("make")
            .current_dir(&source)
            .args(["-j2", "CC=musl-gcc", "EXTRA_CFLAGS=-fcf-protection=none"])
            .stdout(log.try_clone()?)
            .stderr(log),
    )?;
    // Kconfig embeds the host date in comments; preserve all resolved settings without that date.
    let resolved = fs::read_to_string(&config_path)?;
    if matches!(profile, BusyBoxProfile::Bootstrap) {
        for setting in selected.lines().filter(|l| l.starts_with("CONFIG_")) {
            if !resolved.lines().any(|line| line == setting) {
                return Err(format!("BusyBox bootstrap setting was disabled: {setting}").into());
            }
        }
    }
    let canonical = resolved
        .lines()
        .filter(|line| line.starts_with("CONFIG_") || line.starts_with("# CONFIG_"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(&config_path, canonical)?;
    let bytes = fs::read(source.join("busybox"))?;
    anova_elf::parse_linux(&bytes).map_err(|e| format!("Unsupported BusyBox ELF: {e:?}"))?;
    fs::write(apps.join("busybox.elf"), &bytes)?;
    fs::set_permissions(
        apps.join("busybox.elf"),
        fs::metadata(source.join("busybox"))?.permissions(),
    )?;
    fs::copy(source.join("LICENSE"), apps.join("busybox.LICENSE"))?;
    fs::copy(&config_path, work.join("busybox.config"))?;
    let mut manifest = fs::OpenOptions::new()
        .append(true)
        .open(apps.join("manifest.toml"))?;
    writeln!(
        manifest,
        "\n[[apps]]\nname = \"busybox\"\nversion = \"1.36.1\"\nsource_commit = \"{COMMIT}\"\nabi = \"{}\"\nfile = \"busybox.elf\"\nsha256 = \"{:x}\"\nconfig_sha256 = \"{:x}\"",
        anova_abi::linux::PERSONALITY,
        Sha256::digest(&bytes),
        Sha256::digest(fs::read(config_path)?)
    )?;
    let archive = work.join("busybox-source.tar");
    command(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["archive", "--format=tar", "--output"])
            .arg(&archive)
            .arg(COMMIT),
    )?;
    let mut recipe = String::from(
        "BusyBox 1.36.1, unmodified source, GPL-2.0-only.\nPinned commit: 1a64f6a20aaf6ea4dbba68bbfa8cc1ab7e5c57c4\nGenerated .config is included in this archive.\nBuild with musl 1.2.4, GCC 13, GNU make:\nmake oldconfig\nmake -j2 CC=musl-gcc EXTRA_CFLAGS=-fcf-protection=none\nAnovaOS integration recipe: xtask/src/linux.rs, user/linux/busybox.config\n",
    );
    if matches!(profile, BusyBoxProfile::Bootstrap) {
        recipe.push_str(&format!(
            "Additional bootstrap profile: {BOOTSTRAP_CONFIG}\n"
        ));
    }
    fs::write(work.join("ANOVA_BUILD.txt"), recipe)?;
    command(
        Command::new("tar")
            .arg("--append")
            .arg("--file")
            .arg(&archive)
            .arg("--directory")
            .arg(&source)
            .arg(".config"),
    )?;
    command(
        Command::new("tar")
            .arg("--append")
            .arg("--file")
            .arg(&archive)
            .arg("--directory")
            .arg(&work)
            .arg("ANOVA_BUILD.txt"),
    )?;
    let images = directory(root, "images")?;
    command(
        Command::new("gzip")
            .args(["-n", "-c"])
            .arg(archive)
            .stdout(File::create(images.join(source_name))?),
    )?;
    Ok(())
}

/// Build musl thread/signal/TTY probes and real dynamic-loader/TLS inputs independently of native apps.
pub fn build_m9(root: &Path, apps: &Path) -> Result<()> {
    command(
        Command::new("musl-gcc")
            .args([
                "-static",
                "-no-pie",
                "-O2",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fcf-protection=none",
                "-Wl,--build-id=none",
            ])
            .arg(root.join("user/linux/network.c"))
            .arg(root.join("user/linux/dns.c"))
            .arg("-o")
            .arg(apps.join("network-test.elf")),
    )?;
    for name in ["libm9.so", "libm9extra.so"] {
        command(
            Command::new("musl-gcc")
                .args([
                    "-shared",
                    "-fPIC",
                    "-O2",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-fcf-protection=none",
                    "-Wl,--build-id=none",
                ])
                .arg(root.join("user/linux/shared.c"))
                .arg("-o")
                .arg(apps.join(name)),
        )?;
    }
    // A relocated musl toolchain can have headers/static archives but no shared libc linker input.
    // Reject that environment: silently linking libc.a into a TLS DSO breaks dynamic TLS.
    for name in ["libm9.so", "libm9extra.so"] {
        let dynamic = Command::new("readelf")
            .arg("-d")
            .arg(apps.join(name))
            .output()?;
        if !dynamic.status.success()
            || !String::from_utf8(dynamic.stdout)?.contains("Shared library: [libc.so]")
        {
            return Err("musl shared-library linker input is missing; install musl-dev with its matching shared libc (see M9_RUNTIME.md)".into());
        }
    }
    let (runtime, runtime_permissions) = musl_runtime()?;
    fs::write(apps.join("ld-musl-x86_64.so.1"), runtime)?;
    fs::set_permissions(apps.join("ld-musl-x86_64.so.1"), runtime_permissions)?;
    for (name, flags) in [
        ("linux-threads", vec!["-static", "-no-pie"]),
        (
            "linux-dynamic",
            vec!["-pie", "-DM9_DYNAMIC", "-Wl,-rpath,/lib"],
        ),
        ("linux-pie", vec!["-static-pie"]),
    ] {
        let path = apps.join(format!("{name}.elf"));
        let mut cc = Command::new("musl-gcc");
        cc.args([
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pthread",
            "-fcf-protection=none",
            "-Wl,--build-id=none",
        ])
        .args(flags)
        .arg(root.join("user/linux/m9.c"));
        if name == "linux-dynamic" {
            cc.arg("-L").arg(apps).arg("-lm9");
        }
        command(cc.arg("-o").arg(&path))?;
        anova_elf::parse_linux(&fs::read(&path)?)
            .map_err(|e| format!("Invalid M9 ELF {name}: {e:?}"))?;
    }
    command(
        Command::new("musl-gcc")
            .args([
                "-pie",
                "-DM9_DYNAMIC",
                "-O2",
                "-pthread",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fcf-protection=none",
                "-Wl,--build-id=none",
                "-Wl,--dynamic-linker=/lib/missing-m9-loader.so",
            ])
            .arg(root.join("user/linux/m9.c"))
            .arg("-L")
            .arg(apps)
            .arg("-lm9")
            .arg("-o")
            .arg(apps.join("linux-bad-interpreter.elf")),
    )?;
    let mut manifest = fs::OpenOptions::new()
        .append(true)
        .open(apps.join("manifest.toml"))?;
    for file in [
        "linux-threads.elf",
        "linux-dynamic.elf",
        "linux-pie.elf",
        "linux-bad-interpreter.elf",
        "libm9.so",
        "libm9extra.so",
        "ld-musl-x86_64.so.1",
    ] {
        writeln!(
            manifest,
            "\n[[linux_runtime]]\nfile = \"{file}\"\nlibc = \"musl-1.2.4\"\nsha256 = \"{:x}\"",
            Sha256::digest(fs::read(apps.join(file))?)
        )?;
    }
    Ok(())
}

/// Load the pinned interpreter independently of the M9 application fixtures.
pub(super) fn musl_runtime() -> Result<(Vec<u8>, fs::Permissions)> {
    let runtime = env::var_os("ANOVA_MUSL_LIBC")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/lib/x86_64-linux-musl/libc.so"));
    checked_file(&runtime)?;
    let permissions = fs::metadata(&runtime)?.permissions();
    let bytes = fs::read(runtime)?;
    if format!("{:x}", Sha256::digest(&bytes))
        != "6e8ff8ad02f4a3a0b70029577e2eb9df8f6526aea302a60a8d649874d6fa45ea"
    {
        return Err(
            "M9 requires the pinned Ubuntu musl 1.2.4-2 runtime; see docs/en/milestones/M9_RUNTIME.md".into(),
        );
    }
    Ok((bytes, permissions))
}

/// Trace the exact guest binaries against a Linux host without requiring ptrace permissions.
pub fn trace(root: &Path) -> Result<()> {
    apps::build(root, false)?;
    let apps = directory(root, "apps")?;
    let work = directory(root, "compat-host")?;
    let logs = directory(root, "logs")?;
    let bb = apps.join("busybox.elf");
    let output = work.join("pipeline.txt");
    let musl_path = work.join("musl.txt");
    fs::write(&output, b"M8-host-cat\n")?;
    let mut cases: Vec<(&str, PathBuf, Vec<String>)> = vec![
        (
            "musl",
            apps.join("linux-musl.elf"),
            vec!["test".into(), musl_path.display().to_string()],
        ),
        (
            "echo",
            bb.clone(),
            vec!["echo".into(), "M8-host-echo".into()],
        ),
        (
            "ls",
            bb.clone(),
            vec!["ls".into(), work.display().to_string()],
        ),
        (
            "cat",
            bb.clone(),
            vec!["cat".into(), output.display().to_string()],
        ),
        (
            "sh",
            bb.clone(),
            vec![
                "sh".into(),
                "-c".into(),
                format!(
                    "{} echo M8-host-pipeline | {} cat > {}; {} cat {}",
                    bb.display(),
                    bb.display(),
                    output.display(),
                    bb.display(),
                    output.display()
                ),
            ],
        ),
    ];
    for (name, binary) in [
        ("m9-threads", "linux-threads.elf"),
        ("m9-pie", "linux-pie.elf"),
    ] {
        cases.push((name, apps.join(binary), vec![]));
    }
    cases.push((
        "m9-dynamic",
        apps.join("ld-musl-x86_64.so.1"),
        vec![
            "--library-path".into(),
            apps.display().to_string(),
            apps.join("linux-dynamic.elf").display().to_string(),
        ],
    ));
    for (name, binary, args) in cases {
        let stdout = File::create(logs.join(format!("compat-host-{name}.out")))?;
        let stderr = File::create(logs.join(format!("compat-host-{name}.trace")))?;
        command(
            Command::new("timeout")
                .args(["30", "qemu-x86_64", "-strace"])
                .arg(binary)
                .args(args)
                .env_clear()
                .env("PATH", env::var_os("PATH").unwrap_or_default())
                .env("M9_HOST", "1")
                .env("M9_QEMU_USER", "1")
                .env("M9_FILE", root.join("Cargo.toml"))
                .env("M9_EXEC", apps.join("linux-threads.elf"))
                .env("M9_EXTRA", apps.join("libm9extra.so"))
                .env("M9_BAD_INTERP", apps.join("linux-bad-interpreter.elf"))
                .stdout(stdout)
                .stderr(stderr),
        )?;
    }
    println!("Linux-host QEMU-user traces: {}", logs.display());
    Ok(())
}
