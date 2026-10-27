// SPDX-License-Identifier: Apache-2.0
//! Create fresh internal-JBD2 work images and verify sector/flush recovery plus real virtio guest durability.
//! Normalize copied seed ownership; keep journal adoption explicit and preserve failed images.
use super::*;

pub(super) fn check(path: &Path) -> Result<()> {
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(path))
}
pub fn create(root: &Path, path: &Path, linux: bool) -> Result<()> {
    let version = Command::new("mke2fs").arg("-V").output()?;
    if !version.status.success()
        || !String::from_utf8_lossy(&version.stderr).starts_with("mke2fs 1.47.0 ")
    {
        return Err("The controlled journal profile requires e2fsprogs 1.47.0".into());
    }
    output_path(path)?;
    let source = Temporary::new(root, "journal-source")?;
    fs::write(
        source.0.join(".anova-abi"),
        if linux {
            b"linux.x86_64.v1\n".as_slice()
        } else {
            b"anova.native.v1\n".as_slice()
        },
    )?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .set_len(64 * 1024 * 1024)?;
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
                "8192",
                "-N",
                "4096",
                "-m",
                "0",
                "-O",
                "none,extent,filetype,sparse_super,large_file,has_journal",
                "-J",
                "size=8",
                "-E",
                "lazy_itable_init=0,root_owner=0:0",
                "-d",
            ])
            .arg(&source.0)
            .arg(path),
    )?;
    foundation_write::root_owned_fixture(path, &["/.anova-abi"])?;
    check(path)?;
    source.cleanup()
}
pub(super) fn image_profile(root: &Path, workload: bool) -> Result<PathBuf> {
    apps::build(root, false)?;
    let packed = toolchain::pack(root)?;
    if !workload {
        return image_archive(root, Mode::Foundation, Some(&packed));
    }
    let mut archive = fs::read(packed)?;
    let probe = root.join("build/toolchain/m16-workload.elf");
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
            .arg(root.join("tests/guest/m16/workload.c"))
            .arg("-o")
            .arg(&probe),
    )?;
    toolchain::append(&mut archive, 5, "/bin/m16-workload", &fs::read(probe)?);
    let path = root.join("build/toolchain/m16-initramfs.anovafs");
    fs::write(&path, archive)?;
    image_archive(root, Mode::Foundation, Some(&path))
}
pub fn verify(root: &Path) -> Result<()> {
    let identity = foundation::source_identity(root)?;
    command(Command::new("cargo").args([
        "test",
        "--locked",
        "-p",
        "anova-storage",
        "--test",
        "journal",
    ]))?;
    let built = image_profile(root, false)?;
    let work = directory(root, "foundation")?.join(format!(
        "journal-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir(&work)?;
    let iso = work.join("anova-journal.iso");
    fs::copy(built, &iso)?;
    let disk = work.join("work.ext4");
    create(root, &disk, true)?;
    disk::session_timeout(
        root,
        &iso,
        &disk,
        "foundation-journal",
        false,
        None,
        &[
            ("anova> ", b"ext4-write-test suite\r"),
            (
                "[M16:WRITE] sparse=zero resize=exact rename=checked orphan=retained maps=coherent fork=shared metadata=exact",
                b"status\r",
            ),
            ("[SHELL:STATUS] code=0", b"sync\r"),
            (
                "anova> ",
                b"echo journal-durable > /persist/journal-proof\r",
            ),
            ("anova> ", b"sync\r"),
            ("anova> ", b"cat /persist/journal-proof\r"),
            ("journal-durable\r\n", b""),
        ],
        false,
        240,
    )?;
    let failed = work.join("power-cut.ext4");
    fs::copy(&disk, &failed)?;
    // Kill above bypasses guest shutdown. Preserve that disk and replay only a stopped copy.
    disk::session_timeout(
        root,
        &iso,
        &disk,
        "foundation-journal-replay",
        false,
        None,
        &[
            ("anova> ", b"ext4-write-test reboot\r"),
            (
                "[M16:REBOOT] bytes=exact mode=0751 times=exact",
                b"cat /persist/journal-proof\r",
            ),
            ("journal-durable\r\n", b"shutdown\r"),
        ],
        true,
        240,
    )?;
    check(&disk)?;
    for name in ["foundation-journal", "foundation-journal-replay"] {
        for suffix in ["serial.log", "qemu.log", "result.txt"] {
            fs::copy(
                root.join(format!("build/logs/{name}.{suffix}")),
                work.join(format!("{name}.{suffix}")),
            )?;
        }
    }
    if foundation::source_identity(root)? != identity {
        return Err("Journal sources changed during acceptance".into());
    }
    fs::write(
        work.join("result.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "profile":"ext4-jbd2-v1", "implementation_sha256":identity,
            "iso_sha256":bootstrap::hash(&fs::read(&iso)?), "disk_sha256":bootstrap::hash(&fs::read(&disk)?),
            "fault_matrix":"build/journal-host/matrix/results.txt", "cpus":1,"ram_bytes":1073741824u64,"accelerator":"tcg"
        }))?,
    )?;
    println!(
        "Verified JBD2 fault matrix, virtio writes and unclean guest restart: {}",
        work.display()
    );
    Ok(())
}
