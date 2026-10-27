// SPDX-License-Identifier: Apache-2.0
//! Create opt-in non-journal ext4 work images and retain actual QEMU write/build/reboot/fault evidence.
//! Normalize seed ownership for non-root hosts; independently inspect every stopped disk.
use super::*;
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom};

pub fn create(root: &Path, path: &Path, linux: bool) -> Result<()> {
    e2fsprogs_version()?;
    output_path(path)?;
    let source = Temporary::new(root, "ext4-source")?;
    fs::write(
        source.0.join(".anova-abi"),
        if linux {
            b"linux.x86_64.v1\n".as_slice()
        } else {
            b"anova.native.v1\n".as_slice()
        },
    )?;
    fs::write(
        source.0.join("read-fault"),
        b"independent read-error fixture\n",
    )?;
    fs::create_dir(source.0.join("tmp"))?;
    fs::write(source.0.join("main.c"), b"/* Exercise guest compilation on controlled ext4. */\n#include <stdio.h>\nint main(void) { puts(\"M16-ext4-built\"); return 42; }\n")?;
    fs::write(source.0.join("bad.c"), b"/* Exercise syntax-error retention of a prior output. */\nint main(void) { invalid syntax here }\n")?;
    fs::write(source.0.join("Makefile"), b"# Build the controlled ext4 guest C fixture.\nall: app\napp: main.c\n\tgcc main.c -o app\n")?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .set_len(8 * 1024 * 1024)?;
    command(
        Command::new("mke2fs")
            .env("E2FSPROGS_FAKE_TIME", "1700000000")
            .args([
                "-q",
                "-t",
                "ext4",
                "-b",
                "4096",
                "-I",
                "256",
                "-g",
                "256",
                "-N",
                "128",
                "-m",
                "0",
                "-O",
                "none,extent,filetype,sparse_super,large_file",
                "-E",
                "lazy_itable_init=0,root_owner=0:0",
                "-d",
            ])
            .arg(&source.0)
            .arg(path),
    )?;
    root_owned_fixture(path, SEED_PATHS)?;
    check(path)?;
    source.cleanup()?;
    Ok(())
}

const SEED_PATHS: &[&str] = &[
    "/.anova-abi",
    "/Makefile",
    "/bad.c",
    "/main.c",
    "/read-fault",
    "/tmp",
];

/// Only normalize explicitly seeded paths while constructing a fresh controlled fixture.
/// mke2fs -d copies host IDs; guest writable admission must remain strictly root-owned.
pub(super) fn root_owned_fixture(path: &Path, names: &[&str]) -> Result<()> {
    checked_file(path)?;
    for name in names {
        if !name.starts_with('/')
            || name.len() > 512
            || name
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && !b"/._-".contains(&b))
            || name
                .split('/')
                .skip(1)
                .any(|p| matches!(p, "" | "." | ".."))
        {
            return Err("Invalid fresh-fixture seed path".into());
        }
    }
    for name in names {
        for field in ["uid", "gid"] {
            command(
                Command::new("debugfs")
                    .args(["-w", "-R", &format!("set_inode_field {name} {field} 0")])
                    .arg(path),
            )?;
        }
    }
    // debugfs can return success for a rejected command; audit the actual resulting bytes.
    use anova_storage::filesystem::Filesystem;
    let mut disk = anova_storage::ext4::Ext4::mount_writable(Box::new(disk::Image::open(path)?))
        .map_err(|error| format!("Fresh root-owned ext4 fixture audit failed: {error}"))?;
    disk.shutdown()
        .map_err(|error| format!("Fresh fixture shutdown failed: {error}"))?;
    Ok(())
}
fn check(path: &Path) -> Result<()> {
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(path))
}
fn digest(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
fn e2fsprogs_version() -> Result<String> {
    let output = Command::new("mke2fs").arg("-V").output()?;
    let version = String::from_utf8(output.stderr)?;
    if !output.status.success() || !version.starts_with("mke2fs 1.47.0 ") {
        return Err(format!("Controlled ext4 requires e2fsprogs 1.47.0: {version}").into());
    }
    Ok(version.trim().to_owned())
}
fn image_profile(root: &Path, toolchain: bool) -> Result<PathBuf> {
    apps::build(root, false)?;
    if toolchain {
        let archive = toolchain::pack(root)?;
        image_archive(root, Mode::Ext4Write, Some(&archive))
    } else {
        image(root, Mode::Ext4Write)
    }
}
pub fn run(root: &Path, disk: Option<&Path>, toolchain: bool, network: bool) -> Result<()> {
    let disk = disk.ok_or("--ext4-write requires an explicit --disk image")?;
    let iso = image_profile(root, toolchain)?;
    let vars = Temporary::new(root, "ext4-run")?;
    let mut cmd = qemu_disk(root, &iso, &vars.0, false, Some(disk), false)?;
    if network {
        network::attach(&mut cmd);
    }
    let result = command(cmd.args(["-serial", "stdio"]));
    vars.cleanup()?;
    result
}
fn dump(disk: &Path, name: &str, destination: &Path) -> Result<()> {
    command(
        Command::new("debugfs")
            .args(["-R", &format!("dump {name} {}", destination.display())])
            .arg(disk),
    )?;
    checked_file(destination)
}
pub fn verify(root: &Path) -> Result<()> {
    let identity = foundation::source_identity(root)?;
    let built = image_profile(root, true)?;
    let work = directory(root, "foundation")?.join(format!(
        "write-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir(&work)?;
    let e2fsprogs = e2fsprogs_version()?;
    fs::write(work.join("e2fsprogs-version.txt"), &e2fsprogs)?;
    let iso = work.join("anova-ext4-write-devel.iso");
    fs::copy(built, &iso)?;
    let disk = work.join("work.ext4");
    create(root, &disk, true)?;
    let baseline = work.join("baseline.ext4");
    fs::copy(&disk, &baseline)?;
    let start = Instant::now();
    disk::session_timeout(
        root,
        &iso,
        &disk,
        "foundation-write",
        false,
        None,
        &[
            ("anova> ", b"ext4-write-test suite\r"),
            (
                "[M16:WRITE] sparse=zero resize=exact rename=checked orphan=retained maps=coherent fork=shared metadata=exact",
                b"status\r",
            ),
            ("[SHELL:STATUS] code=0", b"fs-test foundation-oom\r"),
            (
                "[M16:OOM] heap=exhausted bytes=retained offset=retained recovered=true",
                b"status\r",
            ),
            ("[SHELL:STATUS] code=0", b"cd /persist\r"),
            ("anova> ", b"make\r"),
            ("anova> ", b"status\r"),
            ("[SHELL:STATUS] code=0", b"./app\r"),
            ("M16-ext4-built", b"status\r"),
            ("[SHELL:STATUS] code=42", b"gcc bad.c -o app\r"),
            ("anova> ", b"status\r"),
            ("[SHELL:STATUS] code=1", b"./app\r"),
            ("M16-ext4-built", b"status\r"),
            ("[SHELL:STATUS] code=42", b"ext4-write-test full\r"),
            (
                "[M16:ENOSPC] bytes=retained offset=retained blocks=reused",
                b"status\r",
            ),
            ("[SHELL:STATUS] code=0", b"/linux/cat /persist/main.c\r"),
            ("return 42; }", b"shutdown\r"),
        ],
        true,
        240,
    )?;
    check(&disk)?;
    dump(&disk, "/app", &work.join("app-before.elf"))?;
    dump(&disk, "/metadata", &work.join("metadata"))?;
    if fs::read(work.join("metadata"))? != b"metadata bytes\n" {
        return Err("Host metadata bytes differ".into());
    }
    disk::session_timeout(
        root,
        &iso,
        &disk,
        "foundation-write-reboot",
        false,
        None,
        &[
            ("anova> ", b"ext4-write-test reboot\r"),
            (
                "[M16:REBOOT] bytes=exact mode=0751 times=exact",
                b"status\r",
            ),
            ("[SHELL:STATUS] code=0", b"/persist/app\r"),
            ("M16-ext4-built", b"status\r"),
            ("[SHELL:STATUS] code=42", b"shutdown\r"),
        ],
        true,
        120,
    )?;
    check(&disk)?;
    dump(&disk, "/app", &work.join("app-after.elf"))?;
    if digest(&work.join("app-before.elf"))? != digest(&work.join("app-after.elf"))? {
        return Err("Reboot ELF hash differs".into());
    }
    for (name, event, kind, delayed) in [
        ("foundation-write-error", "write_aio", "write", false),
        (
            "foundation-write-flush-error",
            "flush_to_disk",
            "flush",
            true,
        ),
    ] {
        let failed = work.join(format!("{name}.ext4"));
        fs::copy(&baseline, &failed)?;
        let config = format!(
            "[inject-error]\nevent = \"{event}\"\niotype = \"{kind}\"\nerrno = \"5\"\nonce = \"off\"\nimmediately = \"on\"\n{}",
            if delayed {
                "state = \"2\"\n\n[set-state]\nevent = \"flush_to_disk\"\nstate = \"1\"\nnew_state = \"2\"\n"
            } else {
                ""
            }
        );
        disk::session_timeout(
            root,
            &iso,
            &failed,
            name,
            false,
            Some(&config),
            &[
                ("anova> ", b"fs-test disk-error\r"),
                (
                    "[DISK:IOERROR] create=error sync=error retry=error kernel=alive",
                    b"status\r",
                ),
                ("[SHELL:STATUS] code=0", b""),
            ],
            false,
            120,
        )?;
        let log = fs::read_to_string(root.join(format!("build/logs/{name}.serial.log")))?;
        let operation = if kind == "write" { 1 } else { 4 };
        if !log.contains(&format!("[BLK:ERROR] operation={operation} status=1")) {
            return Err("Missing actual virtio I/O error".into());
        }
        if delayed {
            let mut f = File::open(&failed)?;
            f.seek(SeekFrom::Start(1082))?;
            let mut b = [0; 2];
            f.read_exact(&mut b)?;
            if u16::from_le_bytes(b) != 0 {
                return Err("Failed flush image was marked clean".into());
            }
        } else {
            check(&failed)?;
        }
    }
    let failed = work.join("read-error.ext4");
    fs::copy(&baseline, &failed)?;
    let output = Command::new("debugfs")
        .args(["-R", "blocks /read-fault"])
        .arg(&failed)
        .output()?;
    let block = String::from_utf8(output.stdout)?
        .split_whitespace()
        .next()
        .ok_or("Missing read fixture block")?
        .parse::<u64>()?;
    let config = format!(
        "[inject-error]\nevent = \"read_aio\"\niotype = \"read\"\nsector = \"{}\"\nerrno = \"5\"\nonce = \"off\"\nimmediately = \"on\"\n",
        block * 8
    );
    disk::session_timeout(
        root,
        &iso,
        &failed,
        "foundation-read-error",
        false,
        Some(&config),
        &[
            ("anova> ", b"fs-test foundation-read-error\r"),
            (
                "[M16:READERROR] error=reported offset=retained unrelated_write=ok",
                b"status\r",
            ),
            ("[SHELL:STATUS] code=0", b"shutdown\r"),
        ],
        true,
        120,
    )?;
    check(&failed)?;
    // The writable kernel must still mount a device-protected disk readonly, without filesystem writes.
    let before = digest(&disk)?;
    disk::session(
        root,
        &iso,
        &disk,
        "foundation-write-readonly",
        true,
        None,
        &[
            ("anova> ", b"echo blocked > /persist/blocked\r"),
            ("shell: echo failed (-11)", b"shutdown\r"),
        ],
        true,
    )?;
    if digest(&disk)? != before {
        return Err("Readonly device changed".into());
    }
    for name in [
        "foundation-write",
        "foundation-write-reboot",
        "foundation-write-error",
        "foundation-write-flush-error",
        "foundation-read-error",
        "foundation-write-readonly",
    ] {
        for suffix in ["serial.log", "qemu.log", "result.txt"] {
            fs::copy(
                root.join(format!("build/logs/{name}.{suffix}")),
                work.join(format!("{name}.{suffix}")),
            )?;
        }
    }
    if foundation::source_identity(root)? != identity {
        return Err("Implementation changed during writable acceptance".into());
    }
    let evidence = serde_json::json!({"profile":"ext4-write-v1", "implementation_sha256":identity,"base_commit":version("git", &["rev-parse", "HEAD"] )?,"iso_sha256":digest(&iso)?,"disk_sha256":digest(&disk)?,"app_sha256":digest(&work.join("app-after.elf"))?,"wall_ms":start.elapsed().as_millis(),"cpus":1,"ram_bytes":1073741824u64,"accelerator":"tcg","e2fsprogs":e2fsprogs,"journal_recovery":false});
    fs::write(
        work.join("result.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    println!(
        "Verified controlled ext4 writes, real guest build, OOM/ENOSPC, reboot and virtio failures; {}",
        work.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anova_storage::{ext4::Ext4, filesystem::Filesystem};

    #[test]
    fn fresh_fixtures_remove_host_ids_without_relaxing_guest_admission() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let work = Temporary::new(root, "fixture-owners")?;
        for journal in [false, true] {
            let path = work.0.join(if journal {
                "journal.ext4"
            } else {
                "write.ext4"
            });
            let names = if journal {
                &["/.anova-abi"][..]
            } else {
                SEED_PATHS
            };
            if journal {
                foundation_journal::create(root, &path, true)?;
            } else {
                create(root, &path, true)?;
            }
            // Reproduce mke2fs -d copying a non-root CI runner's ownership into every seed.
            for name in names {
                for field in ["uid", "gid"] {
                    command(
                        Command::new("debugfs")
                            .args(["-w", "-R", &format!("set_inode_field {name} {field} 1001")])
                            .arg(&path),
                    )?;
                }
            }
            assert!(Ext4::mount_writable(Box::new(disk::Image::open(&path)?)).is_err());
            let before = digest(&path)?;
            assert!(root_owned_fixture(&path, &["/.anova-abi", "/bad\ncommand"]).is_err());
            assert_eq!(digest(&path)?, before);
            root_owned_fixture(&path, names)?;
            let mut disk = Ext4::mount_writable(Box::new(disk::Image::open(&path)?)).unwrap();
            let marker = disk.lookup(2, ".anova-abi").unwrap().unwrap();
            let mut bytes = [0; 16];
            assert_eq!(disk.read(marker.inode, 0, &mut bytes), Ok(bytes.len()));
            assert_eq!(&bytes, b"linux.x86_64.v1\n");
            disk.shutdown().unwrap();
            check(&path)?;
        }
        work.cleanup()
    }
}
