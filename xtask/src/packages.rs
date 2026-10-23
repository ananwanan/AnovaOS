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

/// Versioned app-build output consumed by app-pack; unknown keys fail closed.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplicationManifest {
    schema: u32,
    name: String,
    version: u32,
    abi: String,
    sdk: u32,
    architecture: String,
    entry: String,
    sha256: String,
    dependencies: Vec<ApplicationDependency>,
    permissions: Vec<String>,
    capabilities: Vec<String>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplicationDependency {
    name: String,
    min_version: u32,
}
fn component(value: &str, bound: usize) -> bool {
    !value.is_empty()
        && value.len() < bound
        && value != "."
        && !value.contains("..")
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}
fn flags(values: &[String], known: &[&str]) -> Result<u32> {
    let mut result = 0;
    for value in values {
        let bit = known
            .iter()
            .position(|known| known == value)
            .ok_or_else(|| format!("Unsupported application requirement: {value}"))?;
        if result & (1 << bit) != 0 {
            return Err(format!("Duplicate application requirement: {value}").into());
        }
        result |= 1 << bit;
    }
    Ok(result)
}
fn validate_tag(elf: &[u8], abi: &str) -> Result<()> {
    let tag = elf
        .get(
            elf.len()
                .checked_sub(64)
                .ok_or("Missing SDK ABI declaration")?..,
        )
        .ok_or("Missing SDK ABI declaration")?;
    let mut expected = [0; 64];
    expected[..8].copy_from_slice(b"ANOVAABI");
    text_field(&mut expected, 8, abi);
    text_field(&mut expected, 40, "x86_64");
    word(&mut expected, 56, 1);
    if tag != expected {
        return Err("ELF SDK declaration disagrees with application manifest".into());
    }
    if abi == anova_abi::PERSONALITY {
        anova_elf::parse(elf).map_err(|e| format!("Invalid native application ELF: {e:?}"))?;
    } else {
        if elf.get(16..18) != Some(&[2, 0]) {
            return Err("The version 2 package profile requires a static Linux ET_EXEC".into());
        }
        if anova_elf::interpreter(elf)
            .map_err(|e| format!("Invalid Linux interpreter: {e:?}"))?
            .is_some()
        {
            return Err("The version 2 package profile requires a static Linux executable".into());
        }
        anova_elf::parse_linux(elf).map_err(|e| format!("Invalid Linux application ELF: {e:?}"))?;
        // parse_linux establishes these bounded program-header reads before the stricter package
        // profile excludes dynamic-linker metadata, even in an otherwise ET_EXEC image.
        let offset = u64::from_le_bytes(elf[32..40].try_into()?) as usize;
        let count = u16::from_le_bytes(elf[56..58].try_into()?) as usize;
        if elf[offset..offset + count * 56]
            .chunks_exact(56)
            .any(|header| matches!(u32::from_le_bytes(header[..4].try_into().unwrap()), 2 | 3))
        {
            return Err("The version 2 package profile excludes Linux dynamic linking".into());
        }
    }
    Ok(())
}
fn application_package(root: &Path, path: &Path) -> Result<Vec<u8>> {
    let app: ApplicationManifest = toml::from_str(&fs::read_to_string(path)?)?;
    if app.schema != 1
        || app.sdk != 1
        || app.version == 0
        || !component(&app.name, 24)
        || !component(&app.entry, 48)
        || app.entry == "LICENSE"
        || app.architecture != "x86_64"
        || !matches!(app.abi.as_str(), "anova.native.v1" | "linux.x86_64.v1")
    {
        return Err("Unsupported application schema, target, SDK, version, name or entry".into());
    }
    let linux = app.abi == anova_abi::linux::PERSONALITY;
    let dependency = if linux {
        "anova.linux.musl"
    } else {
        "anova.sdk"
    };
    if app.dependencies.len() != 1
        || app.dependencies[0].name != dependency
        || app.dependencies[0].min_version == 0
        || app.dependencies[0].min_version > if linux { 1204 } else { 1 }
    {
        return Err("Unsupported application dependency or minimum version".into());
    }
    let permissions = flags(&app.permissions, &["read", "data", "network"])?;
    let capabilities = flags(&app.capabilities, &["vfs", "persist", "linux", "network"])?;
    if permissions & !3 != 0
        || capabilities & !(if linux { 7 } else { 3 }) != 0
        || permissions & 1 == 0
        || capabilities & 1 == 0
        || (permissions & 2 != 0 && capabilities & 2 == 0)
        || (permissions & 4 != 0 && capabilities & 8 == 0)
        || (linux && capabilities & 4 == 0)
    {
        return Err("Inconsistent application permissions or minimum capabilities".into());
    }
    let entry = path
        .parent()
        .ok_or("Manifest has no parent")?
        .join(&app.entry);
    checked_file(&entry)?;
    let elf = fs::read(entry)?;
    if app.sha256 != format!("{:x}", Sha256::digest(&elf)) {
        return Err("Application ELF checksum disagrees with manifest".into());
    }
    validate_tag(&elf, &app.abi)?;
    // Keep the source ELF and symbols intact. Packages carry a compact executable with the same
    // declaration restored after strip discards the non-loadable trailer.
    let staging = Temporary::new(root, "package-elf")?;
    let input = staging.0.join("app.elf");
    let compact = staging.0.join("compact.elf");
    fs::write(&input, &elf[..elf.len() - 64])?;
    command(
        Command::new("strip")
            .arg("--strip-all")
            .arg("-o")
            .arg(&compact)
            .arg(&input),
    )?;
    let mut payload = fs::read(compact)?;
    payload.extend_from_slice(&elf[elf.len() - 64..]);
    validate_tag(&payload, &app.abi)?;
    let contents = [
        (app.entry.as_str(), 0o755, payload),
        ("LICENSE", 0o644, fs::read(root.join("LICENSE"))?),
    ];
    let mut bytes = vec![0; 256 + contents.len() * 96];
    bytes[..8].copy_from_slice(b"ANOVAPK2");
    text_field(&mut bytes, 8, &app.name);
    word(&mut bytes, 36, app.version);
    word(&mut bytes, 40, 1);
    word(&mut bytes, 44, contents.len() as u32);
    text_field(&mut bytes, 48, "x86_64");
    text_field(&mut bytes, 64, &app.abi);
    text_field(&mut bytes, 96, "Apache-2.0");
    word(&mut bytes, 112, 1);
    word(&mut bytes, 128, 2);
    word(&mut bytes, 132, 256);
    word(&mut bytes, 136, permissions);
    word(&mut bytes, 140, capabilities);
    text_field(&mut bytes, 144, &app.entry);
    text_field(&mut bytes, 192, dependency);
    word(&mut bytes, 224, app.dependencies[0].min_version);
    for (k, (name, mode, data)) in contents.into_iter().enumerate() {
        let at = 256 + k * 96;
        text_field(&mut bytes, at, name);
        word(&mut bytes, at + 48, mode);
        word(&mut bytes, at + 52, data.len().try_into()?);
        bytes[at + 56..at + 88].copy_from_slice(&Sha256::digest(&data));
        bytes.extend_from_slice(&data);
    }
    if bytes.len() > 128 * 1024 {
        return Err("Application exceeds the 128 KiB package profile".into());
    }
    Ok(bytes)
}
/// Build a hash-bound ANOVAPK2 archive from the SDK's validated application manifest.
pub fn pack_manifest(root: &Path, manifest: &Path, output: &Path) -> Result<()> {
    let bytes = application_package(root, manifest)?;
    output_path(output)?;
    fs::write(output, &bytes)?;
    println!(
        "Packed {} sha256={:x}",
        output.display(),
        Sha256::digest(&bytes)
    );
    Ok(())
}

struct PackageFixture {
    name: String,
    first: Vec<u8>,
    upgrade: Vec<u8>,
    marker: &'static str,
    upgrade_marker: &'static str,
}
fn sdk_fixture(
    root: &Path,
    work: &Path,
    language: &str,
    marker: &'static str,
) -> Result<PackageFixture> {
    let source = root.join("build/sdk/templates").join(language);
    let destination = work.join(language);
    fs::create_dir_all(&destination)?;
    let mut manifest: toml::Value = toml::from_str(&fs::read_to_string(source.join("app.toml"))?)?;
    let entry = manifest["entry"]
        .as_str()
        .ok_or("SDK manifest entry missing")?;
    fs::copy(source.join(entry), destination.join(entry))?;
    let name = format!("sdk-{language}");
    manifest["name"] = toml::Value::String(name.clone());
    let path = destination.join("app.toml");
    manifest["version"] = toml::Value::Integer(1);
    fs::write(&path, toml::to_string(&manifest)?)?;
    let first = application_package(root, &path)?;
    if language == "c" {
        let source_file = source.join("main.c");
        let text = fs::read_to_string(&source_file)?;
        if !text.contains("[M13:C]") {
            return Err("C upgrade source marker missing".into());
        }
        fs::write(&source_file, text.replace("[M13:C]", "[M13:C:V2]"))?;
        crate::sdk::app_build(root, &[source.to_string_lossy().into_owned()])?;
        manifest = toml::from_str(&fs::read_to_string(source.join("app.toml"))?)?;
        manifest["name"] = toml::Value::String(name.clone());
        fs::copy(source.join("app.elf"), destination.join("app.elf"))?;
    }
    manifest["version"] = toml::Value::Integer(2);
    fs::write(&path, toml::to_string(&manifest)?)?;
    let upgrade = application_package(root, &path)?;
    Ok(PackageFixture {
        name,
        first,
        upgrade,
        marker,
        upgrade_marker: if language == "c" {
            "[M13:C:V2]"
        } else {
            marker
        },
    })
}
fn linux_fixture(root: &Path, work: &Path) -> Result<PackageFixture> {
    let folder = work.join("linux");
    fs::create_dir_all(&folder)?;
    let mut elf = fs::read(root.join("build/apps/linux-musl.elf"))?;
    let mut tag = [0; 64];
    tag[..8].copy_from_slice(b"ANOVAABI");
    text_field(&mut tag, 8, anova_abi::linux::PERSONALITY);
    text_field(&mut tag, 40, "x86_64");
    word(&mut tag, 56, 1);
    elf.extend_from_slice(&tag);
    fs::write(folder.join("app.elf"), &elf)?;
    let mut packages = Vec::new();
    for version in [1, 2] {
        fs::write(
            folder.join("app.toml"),
            format!(
                "schema=1\nname=\"sdk-linux\"\nversion={version}\nabi=\"linux.x86_64.v1\"\nsdk=1\narchitecture=\"x86_64\"\nentry=\"app.elf\"\nsha256=\"{:x}\"\npermissions=[\"read\",\"data\"]\ncapabilities=[\"vfs\",\"persist\",\"linux\"]\n[[dependencies]]\nname=\"anova.linux.musl\"\nmin_version=1204\n",
                Sha256::digest(&elf)
            ),
        )?;
        packages.push(application_package(root, &folder.join("app.toml"))?);
    }
    Ok(PackageFixture {
        name: "sdk-linux".into(),
        first: packages.remove(0),
        upgrade: packages.remove(0),
        marker: "[LINUX:MUSL] TLS=true malloc=true errno=true file=true",
        upgrade_marker: "[LINUX:MUSL] TLS=true malloc=true errno=true file=true",
    })
}
fn parser_check(binary: &Path, path: &Path, bytes: &[u8], accepted: bool) -> Result<()> {
    fs::write(path, bytes)?;
    let output = Command::new(binary)
        .arg("check")
        .arg(path)
        .arg(format!("{:x}", Sha256::digest(bytes)))
        .output()?;
    if output.status.success() != accepted {
        return Err(format!(
            "Package parser acceptance disagreed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stdout)
        )
        .into());
    }
    Ok(())
}
fn namespace(path: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    fn collect(path: &Path, result: &mut Vec<(PathBuf, Vec<u8>)>) -> Result<()> {
        let mut entries = fs::read_dir(path)?.collect::<std::result::Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if entry.file_type()?.is_dir() {
                result.push((entry.path(), Vec::new()));
                collect(&entry.path(), result)?;
            } else {
                result.push((entry.path(), fs::read(entry.path())?));
            }
        }
        Ok(())
    }
    let mut result = Vec::new();
    collect(path, &mut result)?;
    Ok(result)
}
/// Exercise the actual C parser against rehashed invalid metadata and installation policy.
fn host_v2(
    root: &Path,
    work: &Path,
    native: &PackageFixture,
    linux: &PackageFixture,
) -> Result<()> {
    let binary = root.join("build/apps/pkg.elf");
    parser_check(
        &binary,
        &work.join("valid-native.apkg"),
        &native.first,
        true,
    )?;
    parser_check(&binary, &work.join("valid-linux.apkg"), &linux.first, true)?;
    let mut invalid = Vec::new();
    for (name, offset, value) in [
        ("future-format", 128, 3),
        ("bad-header", 132, 128),
        ("permission", 136, 0x8000_0001),
        ("future-capability", 140, 0x8000_0003),
        ("native-linux-capability", 140, 7),
        ("network-capability", 140, 11),
        ("network-permission", 136, 5),
        ("future-sdk-dependency", 224, 2),
        ("zero-version", 36, 0),
        ("invalid-mode", 304, 0o777),
        ("overflow-size", 308, u32::MAX),
    ] {
        let mut bytes = native.first.clone();
        word(&mut bytes, offset, value);
        invalid.push((name, bytes));
    }
    for (name, offset, value, width) in [
        ("architecture", 48, "aarch64", 16),
        ("abi", 64, "anova.native.v2", 32),
        ("dependency", 192, "another.sdk", 32),
        ("missing-entry", 144, "missing.elf", 48),
        ("entry-traversal", 144, "../escape", 48),
        ("file-traversal", 256, "../escape", 48),
        ("duplicate", 256 + 96, "app.elf", 48),
    ] {
        let mut bytes = native.first.clone();
        bytes[offset..offset + width].fill(0);
        text_field(&mut bytes, offset, value);
        invalid.push((name, bytes));
    }
    let mut mismatch = native.first.clone();
    mismatch[64..96].fill(0);
    text_field(&mut mismatch, 64, anova_abi::linux::PERSONALITY);
    mismatch[192..224].fill(0);
    text_field(&mut mismatch, 192, "anova.linux.musl");
    word(&mut mismatch, 140, 7);
    invalid.push(("elf-abi-mismatch", mismatch));
    let mut hidden = native.first.clone();
    hidden[31] = b'X';
    invalid.push(("hidden-name-tail", hidden));
    let mut tail = native.first.clone();
    tail.push(0);
    invalid.push(("extra-byte", tail));
    invalid.push(("truncated", native.first[..native.first.len() - 1].to_vec()));
    let mut missing_license = native.first.clone();
    text_field(&mut missing_license, 352, "NOTICE_");
    invalid.push(("missing-license", missing_license));
    for (name, bytes) in &invalid {
        parser_check(&binary, &work.join(format!("{name}.apkg")), bytes, false)?;
    }
    // Recompute the entry checksum after corrupting its ELF, so rejection tests the executable
    // contract instead of merely hitting the generic file-integrity gate.
    for (name, offset, value) in [
        ("elf-machine", 18, 183),
        ("elf-type", 16, 3),
        ("elf-phdr-bounds", 32, u32::MAX),
        ("elf-entry", 24, u32::MAX),
        ("elf-writable-code", 68, 7),
    ] {
        let mut bytes = native.first.clone();
        word(&mut bytes, 448 + offset, value);
        let len = u32::from_le_bytes(bytes[308..312].try_into()?) as usize;
        let hash = Sha256::digest(&bytes[448..448 + len]);
        bytes[312..344].copy_from_slice(&hash);
        parser_check(&binary, &work.join(format!("{name}.apkg")), &bytes, false)?;
        invalid.push((name, bytes));
    }
    let linux_size = u32::from_le_bytes(linux.first[308..312].try_into()?) as usize;
    let phoff = u64::from_le_bytes(linux.first[480..488].try_into()?) as usize;
    let phnum = u16::from_le_bytes(linux.first[504..506].try_into()?) as usize;
    let tls = (0..phnum)
        .map(|k| 448 + phoff + k * 56)
        .find(|at| linux.first[*at..*at + 4] == 7u32.to_le_bytes())
        .ok_or("Linux package TLS negative lacks a real TLS segment")?;
    for (name, offset, value) in [
        ("elf-tls-outside-load", 16, u64::MAX - 4095),
        ("elf-tls-overflow", 40, u64::MAX),
        ("elf-tls-alignment", 48, 3),
    ] {
        let mut bytes = linux.first.clone();
        bytes[tls + offset..tls + offset + 8].copy_from_slice(&value.to_le_bytes());
        let hash = Sha256::digest(&bytes[448..448 + linux_size]);
        bytes[312..344].copy_from_slice(&hash);
        parser_check(&binary, &work.join(format!("{name}.apkg")), &bytes, false)?;
        invalid.push((name, bytes));
    }
    // Compile the same installer with only its persistent root relocated into disposable build data.
    let sandbox = work.join("host-root");
    if sandbox.exists() {
        fs::remove_dir_all(&sandbox)?;
    }
    fs::create_dir(&sandbox)?;
    fs::write(sandbox.join(".anova-abi"), b"anova.native.v1\n")?;
    let test_binary = work.join("pkg-host.elf");
    let mbed = root.join("build/compat/mbedtls");
    command(
        Command::new("musl-gcc")
            .args(["-static", "-no-pie", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(format!(
                "-DANOVA_PKG_PERSIST={:?}",
                sandbox.to_str().ok_or("Sandbox path is not UTF-8")?
            ))
            .arg("-I")
            .arg(mbed.join("include"))
            .arg(root.join("user/linux/pkg.c"))
            .arg(root.join("user/linux/tls.c"))
            .arg("-L")
            .arg(mbed.join("library"))
            .args(["-lmbedtls", "-lmbedx509", "-lmbedcrypto", "-o"])
            .arg(&test_binary),
    )?;
    let control = Command::new(&test_binary)
        .arg("install-file")
        .arg(work.join("valid-native.apkg"))
        .arg(format!("{:x}", Sha256::digest(&native.first)))
        .output()?;
    if !control.status.success() {
        return Err(format!(
            "Valid host installation control failed: {}",
            String::from_utf8_lossy(&control.stdout)
        )
        .into());
    }
    fs::write(
        sandbox.join("data").join(&native.name).join("user.txt"),
        b"retained data",
    )?;
    let before = namespace(&sandbox)?;
    for (name, bytes) in invalid
        .iter()
        .map(|(name, bytes)| (*name, bytes))
        .chain([("valid-linux", &linux.first)])
    {
        let path = work.join(format!("{name}.apkg"));
        let status = Command::new(&test_binary)
            .args(["install-file"])
            .arg(&path)
            .arg(format!("{:x}", Sha256::digest(bytes)))
            .stdout(Stdio::null())
            .status()?;
        if status.success() || namespace(&sandbox)? != before {
            return Err(format!("Rejected {name} package changed persistent files").into());
        }
    }
    fs::write(sandbox.join(".anova-abi"), b"linux.x86_64.v1\n")?;
    let before = namespace(&sandbox)?;
    for (path, bytes) in [
        (work.join("valid-native.apkg"), native.first.clone()),
        (
            root.join("build/apps/demo-1.apkg"),
            fs::read(root.join("build/apps/demo-1.apkg"))?,
        ),
    ] {
        if Command::new(&test_binary)
            .arg("install-file")
            .arg(path)
            .arg(format!("{:x}", Sha256::digest(bytes)))
            .stdout(Stdio::null())
            .status()?
            .success()
            || namespace(&sandbox)? != before
        {
            return Err("Linux persistent policy admitted a native v1/v2 package".into());
        }
    }
    println!(
        "[M13:PACKAGE:HOST] native=true linux=true adversarial={} rejected_before_writes=true",
        invalid.len()
    );
    Ok(())
}
fn fixture_command(fixture: &PackageFixture, version: u32, operation: &str) -> String {
    let bytes = if version == 1 {
        &fixture.first
    } else {
        &fixture.upgrade
    };
    format!(
        "pkg {operation} /repo/{}-{version}.apkg {:x}",
        fixture.name,
        Sha256::digest(bytes)
    )
}
fn entry_command(fixture: &PackageFixture, slot: char) -> String {
    format!(
        "/persist/pkg/{}-{slot}/file0{}",
        fixture.name,
        if fixture.name == "sdk-linux" {
            " test"
        } else {
            ""
        }
    )
}
fn completed_statuses(commands: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut checked = Vec::new();
    for (command, expected) in commands {
        let shutdown = command == "shutdown";
        let code = u32::from(
            expected.starts_with("[PKG:REJECT]")
                || expected.starts_with("[PKG:ROLLBACK]")
                || expected.starts_with("shell:"),
        );
        checked.push((command, expected));
        if !shutdown {
            checked.push(("status".into(), format!("[SHELL:STATUS] code={code}")));
        }
    }
    checked
}
/// Run SDK-created C/C++/Rust packages and a Linux package through independent persistent boots.
pub fn verify_v2(root: &Path) -> Result<()> {
    let result = directory(root, "logs")?.join("package-v2.result.txt");
    output_path(&result)?;
    fs::write(&result, "result=Running\n")?;
    apps::build(root, false)?;
    crate::sdk::verify(root)?;
    let work = directory(root, "packages-v2")?;
    let natives = [
        sdk_fixture(root, &work, "c", "[M13:C]")?,
        sdk_fixture(root, &work, "cpp", "[M13:CPP]")?,
        sdk_fixture(root, &work, "rust", "[M13:RUST]")?,
    ];
    let linux = linux_fixture(root, &work)?;
    host_v2(root, &work, &natives[0], &linux)?;
    let mut archive = fs::read(root.join("build/apps/initramfs.anovafs"))?;
    for fixture in natives.iter().chain([&linux]) {
        for (version, bytes) in [(1, &fixture.first), (2, &fixture.upgrade)] {
            let file = format!("{}-{version}.apkg", fixture.name);
            fs::write(work.join(&file), bytes)?;
            toolchain::append(&mut archive, 1, &format!("/repo/{file}"), bytes);
        }
    }
    let archive_path = work.join("initramfs.anovafs");
    fs::write(&archive_path, archive)?;
    let iso = image_archive(root, Mode::Normal, Some(&archive_path))?;
    let vars = Temporary::new(root, "packages-v2")?;
    for (profile, linux_disk, fixtures, conflict) in [
        ("native", false, &natives[..], &linux),
        ("linux", true, std::slice::from_ref(&linux), &natives[0]),
    ] {
        let disk = vars.0.join(format!("{profile}.ext2"));
        disk::create_profile(root, &disk, linux_disk)?;
        let policy = if linux_disk {
            "linux.x86_64.v1"
        } else {
            "anova.native.v1"
        };
        let opposite = if linux_disk {
            "anova.native.v1"
        } else {
            "linux.x86_64.v1"
        };
        let mut commands = vec![
            (
                format!("echo {opposite} > /persist/.anova-abi"),
                "shell: echo failed (-11)".into(),
            ),
            (
                "cat /persist/.anova-abi".into(),
                format!("\r\n{policy}\r\n"),
            ),
            (
                fixture_command(conflict, 1, "install-file"),
                "[PKG:REJECT]".into(),
            ),
        ];
        if linux_disk {
            commands.push(("pkg install demo 1".into(), "[PKG:REJECT]".into()));
        }
        for fixture in fixtures {
            commands.extend([
                (
                    fixture_command(fixture, 1, "install-file"),
                    format!("[PKG:INSTALLED] name={} version=1 slot=a", fixture.name),
                ),
                (entry_command(fixture, 'a'), fixture.marker.into()),
                (
                    format!(
                        "echo m13-user-data > /persist/data/{}/user.txt",
                        fixture.name
                    ),
                    "[USER:EXIT]".into(),
                ),
                (
                    fixture_command(fixture, 2, "fail-file"),
                    format!("[PKG:ROLLBACK] name={} retained=1", fixture.name),
                ),
                (entry_command(fixture, 'a'), fixture.marker.into()),
                (
                    fixture_command(fixture, 2, "install-file"),
                    format!("[PKG:INSTALLED] name={} version=2 slot=b", fixture.name),
                ),
                (entry_command(fixture, 'b'), fixture.upgrade_marker.into()),
                (
                    fixture_command(fixture, 1, "install-file"),
                    "[PKG:REJECT] version must increase".into(),
                ),
            ]);
        }
        commands.push((
            "shutdown".into(),
            "[POWER:OFF] synced=true clean=true".into(),
        ));
        session(
            root,
            &iso,
            &disk,
            &format!("packages-v2-{profile}"),
            false,
            &completed_statuses(commands),
        )?;
        check_disk(&disk)?;
        let mut reboot = Vec::new();
        for fixture in fixtures {
            reboot.extend([
                (
                    format!("pkg status {}", fixture.name),
                    format!(
                        "[PKG:STATUS] name={} installed=1 version=2 slot=b",
                        fixture.name
                    ),
                ),
                (entry_command(fixture, 'b'), fixture.upgrade_marker.into()),
                (
                    format!("cat /persist/data/{}/user.txt", fixture.name),
                    "\r\nm13-user-data\r\n".into(),
                ),
                (
                    format!("pkg remove {}", fixture.name),
                    format!("[PKG:REMOVED] name={} user_data=preserved", fixture.name),
                ),
                (
                    format!("pkg status {}", fixture.name),
                    format!("[PKG:STATUS] name={} installed=0", fixture.name),
                ),
                (
                    format!("cat /persist/data/{}/user.txt", fixture.name),
                    "\r\nm13-user-data\r\n".into(),
                ),
            ]);
        }
        reboot.push((
            "shutdown".into(),
            "[POWER:OFF] synced=true clean=true".into(),
        ));
        session(
            root,
            &iso,
            &disk,
            &format!("packages-v2-{profile}-reboot"),
            false,
            &completed_statuses(reboot),
        )?;
        check_disk(&disk)?;
    }
    fs::write(&result, "result=Ok\n")?;
    println!(
        "[M13:PACKAGE:OK] C=true CPP=true Rust=true Linux=true upgrade=true rollback=true reboot=true data=true ABI_policy=true"
    );
    Ok(())
}

#[cfg(test)]
mod manifest_tests {
    use super::*;
    #[test]
    fn manifest_names_and_requirement_sets_fail_closed() {
        for bad in ["", ".", "..", "../entry", "a/b", "a b", "a..b"] {
            assert!(!component(bad, 24));
        }
        assert!(component("native-demo_1.elf", 24));
        assert!(flags(&["read".into(), "read".into()], &["read"]).is_err());
        assert!(flags(&["root".into()], &["read"]).is_err());
        assert_eq!(flags(&["data".into()], &["read", "data"]).unwrap(), 2);
    }
    #[test]
    fn sdk_declaration_cannot_cross_the_abi_boundary() {
        let mut bytes = vec![0; 128];
        bytes[64..72].copy_from_slice(b"ANOVAABI");
        text_field(&mut bytes, 72, anova_abi::PERSONALITY);
        text_field(&mut bytes, 104, "x86_64");
        word(&mut bytes, 120, 1);
        assert!(validate_tag(&bytes, anova_abi::linux::PERSONALITY).is_err());
        // A declaration alone does not make a malformed file into an ELF.
        assert!(validate_tag(&bytes, anova_abi::PERSONALITY).is_err());
    }
}
