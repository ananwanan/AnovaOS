// SPDX-License-Identifier: Apache-2.0
//! Build pinned BusyBox and musl runtime fixtures, package shared libraries and trace host behavior.
//! Keep source/license/config evidence beside generated binaries; never import BusyBox into the kernel.
use super::*;
use sha2::{Digest, Sha256};
use std::io::Write;
const COMMIT: &str = "1a64f6a20aaf6ea4dbba68bbfa8cc1ab7e5c57c4";
pub fn build(root: &Path, apps: &Path) -> Result<()> {
    let work = directory(root, "compat")?;
    let source = work.join("busybox");
    if !source.exists() {
        command(
            Command::new("git")
                .args([
                    "clone",
                    "--depth",
                    "1",
                    "--branch",
                    "1_36_1",
                    "https://github.com/mirror/busybox.git",
                ])
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
    for line in fs::read_to_string(root.join("user/linux/busybox.config"))?
        .lines()
        .filter(|l| l.starts_with("CONFIG_"))
    {
        let key = line.split_once('=').ok_or("Invalid BusyBox profile")?.0;
        let disabled = format!("# {key} is not set");
        if config.contains(&disabled) {
            config = config.replace(&disabled, line);
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
    fs::write(
        work.join("ANOVA_BUILD.txt"),
        "BusyBox 1.36.1, unmodified source, GPL-2.0-only.\nPinned commit: 1a64f6a20aaf6ea4dbba68bbfa8cc1ab7e5c57c4\nGenerated .config is included in this archive.\nBuild with musl 1.2.4, GCC 13, GNU make:\nmake oldconfig\nmake -j2 CC=musl-gcc EXTRA_CFLAGS=-fcf-protection=none\nAnovaOS integration recipe: xtask/src/linux.rs, user/linux/busybox.config\n",
    )?;
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
            .stdout(File::create(images.join("busybox-1.36.1-source.tar.gz"))?),
    )?;
    Ok(())
}

/// Build musl thread/signal/TTY probes and real dynamic-loader/TLS inputs independently of native apps.
pub fn build_m9(root: &Path, apps: &Path) -> Result<()> {
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
    let runtime = env::var_os("ANOVA_MUSL_LIBC")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/lib/x86_64-linux-musl/libc.so"));
    checked_file(&runtime)?;
    let runtime = fs::read(runtime)?;
    if format!("{:x}", Sha256::digest(&runtime))
        != "6e8ff8ad02f4a3a0b70029577e2eb9df8f6526aea302a60a8d649874d6fa45ea"
    {
        return Err(
            "M9 requires the pinned Ubuntu musl 1.2.4-2 runtime; see docs/M9_RUNTIME.md".into(),
        );
    }
    fs::write(apps.join("ld-musl-x86_64.so.1"), runtime)?;
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
