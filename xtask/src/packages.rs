// SPDX-License-Identifier: Apache-2.0
//! Pin user-space TLS, build strict package clients and construct checksum-locked local repositories.
//! Generated TLS fixture private keys stay in build/compat and are never copied to a guest image.
use super::*;
use sha2::{Digest, Sha256};
use std::io::Write;
const TLS_COMMIT: &str = "2fc8413bfcb51354c8e679141b17b3f1a5942561";
pub fn build(root: &Path, apps: &Path) -> Result<()> {
    let work = directory(root, "compat")?;
    let source = work.join("mbedtls");
    if !source.exists() {
        command(
            Command::new("git")
                .args([
                    "clone",
                    "--depth",
                    "1",
                    "--branch",
                    "v2.28.10",
                    "https://github.com/Mbed-TLS/mbedtls.git",
                ])
                .arg(&source),
        )?;
    }
    let rev = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !rev.status.success() || String::from_utf8(rev.stdout)?.trim() != TLS_COMMIT {
        return Err("Mbed TLS source differs from locked 2.28.10 commit".into());
    }
    command(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["diff", "--exit-code", "HEAD", "--"])
            .stdout(Stdio::null()),
    )?;
    let log = File::create(work.join("mbedtls-build.log"))?;
    command(
        Command::new("make")
            .arg("-C")
            .arg(source.join("library"))
            .args([
                "-j4",
                "CC=musl-gcc",
                "CFLAGS=-O2 -ffunction-sections -fdata-sections -fcf-protection=none",
            ])
            .stdout(log.try_clone()?)
            .stderr(log),
    )?;
    for (name, entry) in [("tls-test", "tls.c"), ("pkg", "pkg.c")] {
        let mut cc = Command::new("musl-gcc");
        cc.args([
            "-static",
            "-no-pie",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-ffunction-sections",
            "-fdata-sections",
            "-fcf-protection=none",
            "-Wl,--gc-sections",
            "-Wl,--build-id=none",
        ])
        .arg("-I")
        .arg(source.join("include"))
        .arg(root.join("user/linux").join(entry));
        if name == "tls-test" {
            cc.arg("-DANOVA_TLS_TEST");
        } else {
            cc.arg(root.join("user/linux/tls.c"));
        }
        cc.arg("-L")
            .arg(source.join("library"))
            .args(["-lmbedtls", "-lmbedx509", "-lmbedcrypto", "-o"])
            .arg(apps.join(format!("{name}.elf")));
        command(&mut cc)?;
    }
    for name in ["pkg", "tls-test"] {
        let bytes = fs::read(apps.join(format!("{name}.elf")))?;
        anova_elf::parse_linux(&bytes).map_err(|e| format!("Invalid {name} ELF: {e:?}"))?;
        let mut manifest = fs::OpenOptions::new()
            .append(true)
            .open(apps.join("manifest.toml"))?;
        writeln!(
            manifest,
            "\n[[apps]]\nname = \"{name}\"\nabi = \"{}\"\nsha256 = \"{:x}\"\nmbedtls_commit = \"{TLS_COMMIT}\"",
            anova_abi::linux::PERSONALITY,
            Sha256::digest(bytes)
        )?;
    }
    fs::copy(source.join("LICENSE"), apps.join("mbedtls.LICENSE"))?;
    fs::copy(
        root.join("docs/third-party/network-licenses.md"),
        apps.join("network.LICENSE"),
    )?;
    let tls = work.join("tls-fixture");
    fs::create_dir_all(&tls)?;
    let cert = tls.join("server.pem");
    let valid = cert.exists()
        && Command::new("openssl")
            .args(["x509", "-checkend", "3600", "-noout", "-in"])
            .arg(&cert)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success();
    if !valid {
        command(
            Command::new("openssl")
                .args([
                    "req",
                    "-x509",
                    "-newkey",
                    "rsa:2048",
                    "-nodes",
                    "-days",
                    "2",
                    "-subj",
                    "/CN=repo.anova",
                    "-addext",
                    "subjectAltName=DNS:repo.anova",
                    "-addext",
                    "basicConstraints=critical,CA:TRUE",
                    "-keyout",
                ])
                .arg(tls.join("server.key"))
                .arg("-out")
                .arg(&cert)
                .stderr(File::create(tls.join("certificate.log"))?),
        )?;
    }
    fs::copy(&cert, apps.join("pkg-ca.pem"))?;
    command(
        Command::new("openssl")
            .args(["x509", "-in"])
            .arg(&cert)
            .arg("-signkey")
            .arg(tls.join("server.key"))
            .args(["-days", "-1", "-out"])
            .arg(tls.join("expired.pem")),
    )?;

    // Independent root ensures untrusted certificates fail for CA validation as well as names.
    if !tls.join("untrusted.pem").exists() {
        command(
            Command::new("openssl")
                .args([
                    "req",
                    "-x509",
                    "-newkey",
                    "rsa:2048",
                    "-nodes",
                    "-days",
                    "2",
                    "-subj",
                    "/CN=untrusted",
                    "-keyout",
                ])
                .arg(tls.join("untrusted.key"))
                .arg("-out")
                .arg(tls.join("untrusted.pem"))
                .stderr(Stdio::null()),
        )?;
    }
    fs::copy(tls.join("untrusted.pem"), apps.join("untrusted-ca.pem"))?;
    command(
        Command::new("musl-gcc")
            .args([
                "-static",
                "-no-pie",
                "-O2",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-DANOVA_DNS_COMMAND",
                "-fcf-protection=none",
                "-Wl,--build-id=none",
            ])
            .arg(root.join("user/linux/dns.c"))
            .arg("-o")
            .arg(apps.join("dns.elf")),
    )?;
    for name in ["dns", "network-test"] {
        let bytes = fs::read(apps.join(format!("{name}.elf")))?;
        anova_elf::parse_linux(&bytes).map_err(|e| format!("Invalid M10 ELF: {e:?}"))?;
        let mut manifest = fs::OpenOptions::new()
            .append(true)
            .open(apps.join("manifest.toml"))?;
        writeln!(
            manifest,
            r#"
[[apps]]
name = "{name}"
abi = "{}"
sha256 = "{:x}""#,
            anova_abi::linux::PERSONALITY,
            Sha256::digest(bytes)
        )?;
    }
    repository(root, apps)?;
    host_verify(apps)?;
    Ok(())
}
fn word(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn text_field(bytes: &mut [u8], at: usize, value: &str) {
    bytes[at..at + value.len()].copy_from_slice(value.as_bytes());
}
fn package(root: &Path, apps: &Path, version: u32) -> Result<Vec<u8>> {
    let contents = [
        ("hello", 0o755, fs::read(apps.join("hello-c.elf"))?),
        (
            "message.txt",
            0o644,
            format!("AnovaOS package demo version {version}\n").into_bytes(),
        ),
        ("LICENSE", 0o644, fs::read(root.join("LICENSE"))?),
    ];
    let mut bytes = vec![0; 128 + contents.len() * 96];
    bytes[..8].copy_from_slice(b"ANOVAPK1");
    text_field(&mut bytes, 8, "demo");
    word(&mut bytes, 36, version);
    word(&mut bytes, 40, 1);
    word(&mut bytes, 44, contents.len() as u32);
    text_field(&mut bytes, 48, "x86_64");
    text_field(&mut bytes, 64, "anova.native.v1");
    text_field(&mut bytes, 96, "Apache-2.0");
    word(&mut bytes, 112, 1);
    for (k, (name, mode, data)) in contents.into_iter().enumerate() {
        let at = 128 + k * 96;
        text_field(&mut bytes, at, name);
        word(&mut bytes, at + 48, mode);
        word(&mut bytes, at + 52, data.len() as u32);
        bytes[at + 56..at + 88].copy_from_slice(&Sha256::digest(&data));
        bytes.extend_from_slice(&data);
    }
    Ok(bytes)
}
fn repository(root: &Path, apps: &Path) -> Result<()> {
    let mut index = String::new();
    for version in [1, 2] {
        let bytes = package(root, apps, version)?;
        let name = format!("demo-{version}.apkg");
        index.push_str(&format!("{name} {:x}\n", Sha256::digest(&bytes)));
        fs::write(apps.join(name), bytes)?;
    }
    let mut corrupt = fs::read(apps.join("demo-2.apkg"))?;
    *corrupt.last_mut().unwrap() ^= 1;
    fs::write(apps.join("corrupt.apkg"), corrupt)?;
    for (name, at, text) in [
        ("traversal.apkg", 128, "../escape"),
        ("dependency.apkg", 64, "unsupported.abi"),
    ] {
        let mut bad = fs::read(apps.join("demo-2.apkg"))?;
        bad[at..at + 32].fill(0);
        text_field(&mut bad, at, text);
        index.push_str(&format!("{name} {:x}\n", Sha256::digest(&bad)));
        fs::write(apps.join(name), bad)?;
    }
    fs::write(apps.join("INDEX"), index)?;
    Ok(())
}

/// Exercise the package client through real guest syscalls, clean reboots and strict HTTPS failures.
pub fn verify(root: &Path) -> Result<()> {
    let iso = image(root, Mode::Normal)?;
    let vars = Temporary::new(root, "package-suite")?;
    let disk = vars.0.join("packages.ext2");
    disk::create(root, &disk)?;
    let apps = directory(root, "apps")?;
    let good_hash = format!("{:x}", Sha256::digest(fs::read(apps.join("demo-2.apkg"))?));
    let traversal_hash = format!(
        "{:x}",
        Sha256::digest(fs::read(apps.join("traversal.apkg"))?)
    );
    let abi_hash = format!(
        "{:x}",
        Sha256::digest(fs::read(apps.join("dependency.apkg"))?)
    );
    let offline = vec![
        (
            "pkg install demo 1".into(),
            "[PKG:INSTALLED] name=demo version=1".into(),
        ),
        (
            "pkg status demo".into(),
            "[PKG:STATUS] name=demo installed=1 version=1 slot=a".into(),
        ),
        ("/persist/pkg/demo-a/file0".into(), "[APP:C]".into()),
        (
            "echo retained-user-data > /persist/data/demo/user.txt".into(),
            "[USER:EXIT]".into(),
        ),
        (
            "pkg fail-test demo 2".into(),
            "[PKG:ROLLBACK] name=demo retained=1".into(),
        ),
        (
            "pkg status demo".into(),
            "[PKG:STATUS] name=demo installed=1 version=1".into(),
        ),
        (
            format!("pkg install-file /repo/corrupt.apkg {good_hash}"),
            "[PKG:REJECT]".into(),
        ),
        (
            format!("pkg install-file /repo/traversal.apkg {traversal_hash}"),
            "[PKG:REJECT]".into(),
        ),
        (
            format!("pkg install-file /repo/dependency.apkg {abi_hash}"),
            "[PKG:REJECT]".into(),
        ),
        (
            "pkg install demo 2".into(),
            "[PKG:INSTALLED] name=demo version=2 slot=b".into(),
        ),
        (
            "shutdown".into(),
            "[POWER:OFF] synced=true clean=true".into(),
        ),
    ];
    session(root, &iso, &disk, "packages-offline", false, &offline)?;
    check_disk(&disk)?;
    let reboot = vec![
        (
            "pkg status demo".into(),
            "[PKG:STATUS] name=demo installed=1 version=2 slot=b".into(),
        ),
        (
            "cat /persist/pkg/demo-b/file1".into(),
            "AnovaOS package demo version 2".into(),
        ),
        ("/persist/pkg/demo-b/file0".into(), "[APP:C]".into()),
        (
            "cat /persist/data/demo/user.txt".into(),
            "\r\nretained-user-data\r\n".into(),
        ),
        (
            "pkg remove demo".into(),
            "[PKG:REMOVED] name=demo user_data=preserved".into(),
        ),
        (
            "pkg status demo".into(),
            "[PKG:STATUS] name=demo installed=0".into(),
        ),
        (
            "cat /persist/data/demo/user.txt".into(),
            "\r\nretained-user-data\r\n".into(),
        ),
        (
            "shutdown".into(),
            "[POWER:OFF] synced=true clean=true".into(),
        ),
    ];
    session(root, &iso, &disk, "packages-reboot", false, &reboot)?;
    check_disk(&disk)?;
    let tls = directory(root, "compat")?.join("tls-fixture");
    let logs = directory(root, "logs")?;
    let mut cmd = Command::new("python3");
    cmd.arg(root.join("tests/guest/m10_https_peer.py"))
        .arg("--root")
        .arg(&apps)
        .arg("--certificates")
        .arg(tls)
        .stdout(Stdio::piped())
        .stderr(File::create(logs.join("https-peer.log"))?);
    let mut peer = Guest(cmd.spawn()?);
    let stdout = peer.0.stdout.take().ok_or("Missing peer readiness pipe")?;
    let mut ready =
        transcript::Transcript::capture(stdout, File::create(logs.join("https-peer.out"))?)?;
    let start = Instant::now();
    loop {
        ready.poll()?;
        if ready.find_from(0, "[M10:HTTPS:READY]").is_some() {
            break;
        }
        if peer.0.try_wait()?.is_some() || start.elapsed() > Duration::from_secs(10) {
            return Err("HTTPS peer failed to start; see https-peer.log".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let online = vec![
        (
            "tls-test repo.anova 18083 /etc/pkg-ca.pem accept".into(),
            "[M10:TLS] accepted host=repo.anova".into(),
        ),
        (
            "tls-test wrong.anova 18083 /etc/pkg-ca.pem reject-name".into(),
            "[M10:TLS] rejected host=wrong.anova".into(),
        ),
        (
            "tls-test repo.anova 18083 /etc/untrusted-ca.pem reject-ca".into(),
            "[M10:TLS] rejected host=repo.anova".into(),
        ),
        (
            "tls-test repo.anova 18085 /etc/pkg-ca.pem reject-expired".into(),
            "[M10:TLS] rejected host=repo.anova".into(),
        ),
        (
            "pkg fetch demo 2 18083".into(),
            "[PKG:INSTALLED] name=demo version=2 slot=b".into(),
        ),
        (
            "cat /persist/pkg/demo-b/file1".into(),
            "AnovaOS package demo version 2".into(),
        ),
        (
            "shutdown".into(),
            "[POWER:OFF] synced=true clean=true".into(),
        ),
    ];
    session(root, &iso, &disk, "packages-online", true, &online)?;
    check_disk(&disk)?;
    println!(
        "Verified offline install/upgrade/rollback/remove, reboot/user data, HTTPS fetch and CA/name/expiry rejection"
    );
    Ok(())
}
fn check_disk(path: &Path) -> Result<()> {
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(path))
}
fn session(
    root: &Path,
    iso: &Path,
    disk: &Path,
    name: &str,
    network: bool,
    exchanges: &[(String, String)],
) -> Result<()> {
    use std::io::Write;
    let vars = Temporary::new(root, name)?;
    let logs = directory(root, "logs")?;
    let mut cmd = qemu_disk(root, iso, &vars.0, false, Some(disk), false)?;
    if network {
        crate::network::attach(&mut cmd);
    }
    cmd.args(["-serial", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(File::create(logs.join(format!("{name}.qemu.log")))?);
    let mut guest = Guest(cmd.spawn()?);
    let mut transcript = transcript::Transcript::capture(
        guest.0.stdout.take().ok_or("Missing UART stdout")?,
        File::create(logs.join(format!("{name}.serial.log")))?,
    )?;
    let mut input = guest.0.stdin.take().ok_or("Missing UART stdin")?;
    let mut cursor = 0;
    let mut sent = false;
    let mut step = 0;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(120) {
        transcript.poll()?;
        if sent && step < exchanges.len() && transcript.find_from(cursor, "Usage: pkg").is_some() {
            return Err(format!("Package CLI failed: {}", transcript.text()).into());
        }
        if transcript.find_from(0, "[PANIC]").is_some()
            || transcript.find_from(0, "[M10:TLS:FAIL]").is_some()
        {
            return Err(format!("{name} failed: {}", transcript.text()).into());
        }
        if !sent {
            if let Some(at) = transcript.find_from(cursor, "anova> ") {
                cursor = at + 7;
                if step < exchanges.len() {
                    writeln!(input, "{}", exchanges[step].0)?;
                    input.flush()?;
                    sent = true;
                }
            }
        } else if let Some(at) = transcript.find_from(cursor, &exchanges[step].1) {
            cursor = at + exchanges[step].1.len();
            sent = false;
            step += 1;
        }
        if let Some(status) = guest.0.try_wait()? {
            transcript.finish()?;
            if sent
                && step + 1 == exchanges.len()
                && transcript.find_from(cursor, &exchanges[step].1).is_some()
            {
                step += 1;
            }
            if step == exchanges.len() && status.success() {
                return Ok(());
            }
            return Err(format!(
                "{name} exited {status} at exchange {step}: {}",
                transcript.text()
            )
            .into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(format!("{name} timed out at exchange {step}: {}", transcript.text()).into())
}

/// Run the actual package parser on independently corrupted, hash-locked adversarial archives.
fn host_verify(apps: &Path) -> Result<()> {
    let good = fs::read(apps.join("demo-1.apkg"))?;
    let binary = apps.join("pkg.elf");
    for (name, expected, accept) in [
        ("demo-1.apkg", format!("{:x}", Sha256::digest(&good)), true),
        (
            "corrupt.apkg",
            format!("{:x}", Sha256::digest(fs::read(apps.join("demo-2.apkg"))?)),
            false,
        ),
        (
            "traversal.apkg",
            format!(
                "{:x}",
                Sha256::digest(fs::read(apps.join("traversal.apkg"))?)
            ),
            false,
        ),
        (
            "dependency.apkg",
            format!(
                "{:x}",
                Sha256::digest(fs::read(apps.join("dependency.apkg"))?)
            ),
            false,
        ),
    ] {
        let output = Command::new(&binary)
            .arg("check")
            .arg(apps.join(name))
            .arg(expected)
            .output()?;
        if output.status.success() != accept {
            return Err(format!(
                "Host package parser disagreed for {name}: {}",
                String::from_utf8_lossy(&output.stdout)
            )
            .into());
        }
    }
    println!(
        "Verified real package parser: valid, corrupt, traversal and incompatible ABI fixtures"
    );
    Ok(())
}
