// SPDX-License-Identifier: Apache-2.0
//! Build native/Linux, DNS/TLS/package fixtures with separate target flags and explicit initramfs policy.
//! Validate every ELF and record its explicit ABI identity and SHA-256 before packing initramfs.
use super::*;
use sha2::{Digest, Sha256};

pub const NAMES: [&str; 8] = [
    "hello-rust",
    "hello-c",
    "probe",
    "process-test",
    "init",
    "shell",
    "fs-test",
    "edit",
];
const USER_TARGET: &str = "x86_64-unknown-linux-gnu";

// A multi-image command uses one immutable, verified user-space build snapshot.
// Rehash outputs before each reuse so an altered/missing artifact fails the entire command.
type Snapshot = Vec<(PathBuf, [u8; 32])>;
static SNAPSHOT: std::sync::OnceLock<Snapshot> = std::sync::OnceLock::new();
fn reuse_snapshot() -> Result<bool> {
    let Some(snapshot) = SNAPSHOT.get() else {
        return Ok(false);
    };
    for (path, expected) in snapshot {
        checked_file(path)?;
        let actual: [u8; 32] = Sha256::digest(fs::read(path)?).into();
        if actual != *expected {
            return Err(format!(
                "Verified user artifact changed during this command: {}",
                path.display()
            )
            .into());
        }
    }
    println!("Reusing verified user-space build snapshot");
    Ok(true)
}
fn remember_snapshot(root: &Path, apps: &Path) -> Result<()> {
    let mut paths = fs::read_dir(apps)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.push(root.join("build/images/busybox-1.36.1-source.tar.gz"));
    let mut snapshot = Vec::new();
    for path in paths {
        checked_file(&path)?;
        let digest = Sha256::digest(fs::read(&path)?).into();
        snapshot.push((path, digest));
    }
    SNAPSHOT
        .set(snapshot)
        .map_err(|_| "User build snapshot initialized twice")?;
    Ok(())
}

pub fn build(root: &Path, clippy: bool) -> Result<()> {
    if !clippy && reuse_snapshot()? {
        return Ok(());
    }
    let output = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()?;
    if !output.status.success() {
        return Err("Cannot locate Rust sysroot".into());
    }
    let sysroot = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    let linker = sysroot
        .join("lib/rustlib")
        .join(USER_TARGET)
        .join("bin/rust-lld");
    checked_file(&linker)?;
    let flags = [
        "-C".into(),
        format!("linker={}", linker.display()),
        "-C".into(),
        "linker-flavor=ld.lld".into(),
        "-C".into(),
        format!(
            "link-arg=-T{}",
            root.join("linker/user-x86_64.ld").display()
        ),
        "-C".into(),
        "link-arg=--undefined=_start".into(),
        "-C".into(),
        "link-arg=--build-id=none".into(),
        "-C".into(),
        "relocation-model=static".into(),
        "-C".into(),
        "code-model=small".into(),
        "-C".into(),
        "no-redzone=no".into(),
        "-C".into(),
        "target-cpu=x86-64".into(),
        "-C".into(),
        "target-feature=-avx,-avx2".into(),
    ];
    let target = directory(root, "user-target")?;
    let mut rust = Command::new("cargo");
    rust.args([
        if clippy { "clippy" } else { "build" },
        "--locked",
        "--manifest-path",
    ])
    .arg(root.join("user/Cargo.toml"))
    .args(["--target", USER_TARGET, "--release", "--target-dir"])
    .arg(&target)
    .env("CARGO_ENCODED_RUSTFLAGS", flags.join("\x1f"));
    if clippy {
        rust.args(["--", "-D", "warnings"]);
    }
    command(&mut rust)?;
    if clippy {
        return Ok(());
    }
    let apps = directory(root, "apps")?;
    for name in [
        "hello-rust",
        "probe",
        "process-test",
        "init",
        "shell",
        "fs-test",
        "edit",
    ] {
        let path = apps.join(format!("{name}.elf"));
        output_path(&path)?;
        fs::copy(target.join(USER_TARGET).join("release").join(name), path)?;
    }
    let c = apps.join("hello-c.elf");
    output_path(&c)?;
    command(
        Command::new("gcc")
            .args([
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-O2",
                "-g",
                "-ffreestanding",
                "-fno-stack-protector",
                "-fno-pie",
                "-fno-builtin",
                "-static",
                "-nostdlib",
                "-m64",
                "-march=x86-64",
                "-mno-avx",
                "-mno-avx2",
                "-Wl,--build-id=none",
                "-Wl,-z,noexecstack",
            ])
            .arg(format!(
                "-Wl,-T,{}",
                root.join("linker/user-x86_64.ld").display()
            ))
            .arg(root.join("user/c/crt.S"))
            .arg(root.join("user/c/hello.c"))
            .arg("-o")
            .arg(&c),
    )?;
    let mut manifest =
        String::from("# Generated native image inventory: ABI identity and exact ELF content.\n");
    for name in NAMES {
        let file = apps.join(format!("{name}.elf"));
        let bytes = fs::read(&file)?;
        anova_elf::parse(&bytes).map_err(|e| format!("Invalid user ELF {name}: {e:?}"))?;
        manifest.push_str(&format!("\n[[apps]]\nname = \"{name}\"\nabi = \"{}\"\nfile = \"{name}.elf\"\nsha256 = \"{:x}\"\n",
            anova_abi::PERSONALITY,Sha256::digest(&bytes)));
    }
    let path = apps.join("manifest.toml");
    output_path(&path)?;
    fs::write(path, manifest)?;
    for (name, source, musl) in [
        ("linux-probe", "probe.c", false),
        ("linux-musl", "musl.c", true),
    ] {
        let path = apps.join(format!("{name}.elf"));
        let mut cc = Command::new(if musl { "musl-gcc" } else { "gcc" });
        cc.args([
            "-static",
            "-no-pie",
            "-fcf-protection=none",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-fno-stack-protector",
            "-Wl,--build-id=none",
            "-Wl,-z,noexecstack",
        ]);
        if !musl {
            cc.arg(root.join("user/linux/probe.S"));
            cc.args(["-nostdlib", "-fno-builtin"]);
        }
        command(
            cc.arg(root.join("user/linux").join(source))
                .arg("-o")
                .arg(&path),
        )?;
        let bytes = fs::read(&path)?;
        anova_elf::parse_linux(&bytes).map_err(|e| format!("Invalid Linux ELF {name}: {e:?}"))?;
        use std::io::Write;
        let mut manifest = fs::OpenOptions::new()
            .append(true)
            .open(apps.join("manifest.toml"))?;
        writeln!(
            manifest,
            "\n[[apps]]\nname = \"{name}\"\nabi = \"{}\"\nfile = \"{name}.elf\"\nsha256 = \"{:x}\"",
            anova_abi::linux::PERSONALITY,
            Sha256::digest(&bytes)
        )?;
    }
    linux::build(root, &apps)?;
    linux::build_m9(root, &apps)?;
    packages::build(root, &apps)?;
    initramfs(&apps)?;
    remember_snapshot(root, &apps)?;
    println!("Verified native Rust/C ELF outputs in {}", apps.display());
    Ok(())
}

/// Pack the bounded project-owned archive; names and permissions are explicit image policy.
fn initramfs(apps: &Path) -> Result<()> {
    let mut entries: Vec<(u8, String, Vec<u8>)> = Vec::new();
    for path in [
        "/bin", "/etc", "/tmp", "/dev", "/persist", "/linux", "/lib", "/repo",
    ] {
        entries.push((2, path.into(), Vec::new()));
    }
    for name in NAMES {
        entries.push((
            4,
            format!("/bin/{name}"),
            fs::read(apps.join(format!("{name}.elf")))?,
        ));
    }
    entries.push((
        1,
        "/etc/musl.LICENSE".into(),
        fs::read(
            apps.parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("docs/third-party/musl.md"),
        )?,
    ));
    entries.push((
        1,
        "/etc/busybox.LICENSE".into(),
        fs::read(apps.join("busybox.LICENSE"))?,
    ));
    entries.push((
        5,
        "/linux/busybox".into(),
        fs::read(apps.join("busybox.elf"))?,
    ));
    for name in ["echo", "cat", "ls", "sh", "true", "false"] {
        entries.push((3, format!("/linux/{name}"), b"busybox".to_vec()));
    }
    for name in [
        "dns",
        "pkg",
        "tls-test",
        "network-test",
        "linux-probe",
        "linux-musl",
        "linux-threads",
        "linux-dynamic",
        "linux-pie",
        "linux-bad-interpreter",
    ] {
        entries.push((
            5,
            format!("/bin/{name}"),
            fs::read(apps.join(format!("{name}.elf")))?,
        ));
    }
    for name in [
        "pkg-ca.pem",
        "untrusted-ca.pem",
        "mbedtls.LICENSE",
        "network.LICENSE",
    ] {
        entries.push((1, format!("/etc/{name}"), fs::read(apps.join(name))?));
    }
    for name in [
        "INDEX",
        "demo-1.apkg",
        "demo-2.apkg",
        "corrupt.apkg",
        "traversal.apkg",
        "dependency.apkg",
    ] {
        entries.push((1, format!("/repo/{name}"), fs::read(apps.join(name))?));
    }
    entries.push((
        1,
        "/etc/resolv.conf".into(),
        b"nameserver 10.0.2.3\noptions attempts:1 timeout:2\n".to_vec(),
    ));
    for file in ["ld-musl-x86_64.so.1", "libm9.so", "libm9extra.so"] {
        entries.push((
            if file.starts_with("ld-") { 5 } else { 1 },
            format!("/lib/{file}"),
            fs::read(apps.join(file))?,
        ));
    }
    for name in [
        "ls", "cat", "echo", "pwd", "mkdir", "rm", "true", "false", "cp", "install", "sync",
        "shutdown",
    ] {
        entries.push((3, format!("/bin/{name}"), b"shell".to_vec()));
    }
    entries.push((
        1,
        "/etc/motd".into(),
        b"AnovaOS native v1; /tmp is volatile.\n".to_vec(),
    ));
    entries.push((1, "/etc/large".into(), vec![b'Q'; 5000]));
    entries.push((
        1,
        "/etc/smoke.sh".into(),
        br#"ls /
echo M6-shell-ok > /tmp/message
cat /tmp/message
echo pipeline-data | cat > /tmp/pipe
cat < /tmp/pipe
echo again >> /tmp/message
cat /etc/large | cat | cat > /tmp/large
cat /etc/large | true
status
cd /tmp/work
pwd
echo relative > note
cat note
cd /
ls /dev
hello-rust from-shell
hello-c from-shell
status
missing-command
status
echo cannot-write > /etc/motd
status
echo malformed |
status
fs-test shell-check
exit 0
"#
        .to_vec(),
    ));
    let mut bytes = b"ANOVAFS1".to_vec();
    bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (kind, path, data) in entries {
        bytes.extend_from_slice(&[kind, 0]);
        bytes.extend_from_slice(&(path.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u64).to_le_bytes());
        bytes.extend_from_slice(path.as_bytes());
        bytes.extend_from_slice(&data);
    }
    let path = apps.join("initramfs.anovafs");
    output_path(&path)?;
    fs::write(&path, &bytes)?;
    let fs = anova_vfs::Vfs::new(&bytes).map_err(|e| format!("Invalid initramfs: {e}"))?;
    for name in NAMES {
        let data = fs
            .executable(anova_vfs::ROOT, &format!("/bin/{name}"))
            .map_err(|e| format!("Missing executable: {e}"))?;
        anova_elf::parse(data).map_err(|e| format!("Invalid archived ELF: {e:?}"))?;
    }
    let manifest = apps.join("manifest.toml");
    let mut file = fs::OpenOptions::new().append(true).open(manifest)?;
    use std::io::Write;
    writeln!(
        file,
        "\n[initramfs]\nformat = \"anova.initramfs.v1\"\nfile = \"initramfs.anovafs\"\nsha256 = \"{:x}\"",
        Sha256::digest(&bytes)
    )?;
    Ok(())
}
