// SPDX-License-Identifier: Apache-2.0
//! Create controlled ext4 fixtures and verify native reads/execution, reboot, rejection and zero mutation.
//! Retain exact images, host hashes and UART logs; this gate establishes neither writes nor recovery.
use super::*;
use sha2::{Digest, Sha256};
use std::io::{Seek, SeekFrom, Write};

pub fn verify(root: &Path) -> Result<()> {
    let identity = source_identity(root)?;
    let built_iso = image(root, Mode::Normal)?;
    let base = directory(root, "foundation")?;
    let work = base.join(format!(
        "readonly-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir(&work)?;
    let iso = work.join("anova-foundation.iso");
    fs::copy(built_iso, &iso)?;
    fs::copy(
        root.join("build/apps/initramfs.anovafs"),
        work.join("initramfs.anovafs"),
    )?;
    let source = work.join("source");
    fs::create_dir(&source)?;
    fs::write(source.join(".anova-abi"), b"anova.native.v1\n")?;
    fs::copy(root.join("build/apps/hello-c.elf"), source.join("hello-c"))?;
    fs::create_dir(source.join("many"))?;
    for i in 0..10_001 {
        File::create(source.join(format!("many/f{i:05}")))?;
    }
    let mut large = File::create(source.join("large"))?;
    for block in 0..5120 {
        large.write_all(&[1 + (block % 251) as u8; 4096])?;
    }
    drop(large);
    let mut sparse = File::create(source.join("sparse"))?;
    sparse.set_len(200 * 1024 * 1024)?;
    sparse.write_all(b"first")?;
    sparse.seek(SeekFrom::Start(199 * 1024 * 1024 + 17))?;
    sparse.write_all(b"last")?;
    drop(sparse);
    let disk = work.join("readonly.ext4");
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&disk)?
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
                "1024",
                "-N",
                "16384",
                "-m",
                "0",
                "-O",
                "none,extent,filetype,sparse_super,large_file",
                "-E",
                "lazy_itable_init=0",
                "-d",
            ])
            .arg(&source)
            .arg(&disk),
    )?;
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(&disk))?;
    let digest =
        |path: &Path| -> Result<String> { Ok(format!("{:x}", Sha256::digest(fs::read(path)?))) };
    let before = digest(&disk)?;
    let start = Instant::now();
    for (name, readonly) in [("foundation-readonly", false), ("foundation-reboot", true)] {
        disk::session_timeout(
            root,
            &iso,
            &disk,
            name,
            readonly,
            None,
            &[
                ("anova> ", b"fs-test foundation-readonly\r"),
                (
                    "[M16:EXT4] large=exact sparse=zero readonly=true",
                    b"status\r",
                ),
                ("[SHELL:STATUS] code=0", b"/persist/hello-c ext4\r"),
                (
                    "[APP:C] hello from Ring 3; args: /persist/hello-c ext4; BSS=zero SSE2=ok",
                    b"status\r",
                ),
                ("[SHELL:STATUS] code=7", b"shutdown\r"),
            ],
            true,
            120,
        )?;
        if digest(&disk)? != before {
            return Err("Readonly ext4 session mutated the image".into());
        }
        command(Command::new("e2fsck").args(["-f", "-n"]).arg(&disk))?;
    }
    let elapsed = start.elapsed();
    for (name, offset, value) in [
        ("foundation-replay-required", 1024 + 96, 0x46u32),
        ("foundation-unknown-feature", 1024 + 96, 0x10042u32),
    ] {
        let bad = work.join(format!("{name}.ext4"));
        fs::copy(&disk, &bad)?;
        let mut file = fs::OpenOptions::new().write(true).open(&bad)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&value.to_le_bytes())?;
        drop(file);
        let rejected_before = digest(&bad)?;
        disk::session(
            root,
            &iso,
            &bad,
            name,
            false,
            None,
            &[
                ("anova> ", b"echo M16-recovery-shell\r"),
                ("\r\nM16-recovery-shell\r\n", b"shutdown\r"),
            ],
            true,
        )?;
        if digest(&bad)? != rejected_before {
            return Err("Rejected ext4 session mutated the image".into());
        }
    }
    if source_identity(root)? != identity {
        return Err("Implementation changed during foundation acceptance".into());
    }
    let result = serde_json::json!({
        "profile": "ext4-readonly-v1", "implementation_sha256": identity,
        "base_commit": version("git", &["rev-parse", "HEAD"] )?,
        "iso_sha256": digest(&iso)?, "disk_sha256": before,
        "namespace_entries": 10001, "large_file_bytes": 20971520, "sparse_file_bytes": 209715200,
        "two_boot_wall_ms": elapsed.as_millis(), "per_boot_deadline_seconds": 120,
        "ram_bytes": 256 * 1024 * 1024, "cpus": 1, "accelerator": "tcg",
        "writes_implemented": false, "journal_recovery_implemented": false,
    });
    fs::write(
        work.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!(
        "Verified controlled ext4 readonly bytes, large namespace, execution, reboot and rejection; {}",
        work.display()
    );
    Ok(())
}

/// Hash sorted first-party source/configuration bytes; documentation is recorded after acceptance.
pub(super) fn source_identity(root: &Path) -> Result<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .output()?;
    if !output.status.success() {
        return Err("Cannot inventory foundation sources".into());
    }
    let paths = String::from_utf8(output.stdout)?;
    let mut paths: Vec<_> = paths
        .split('\0')
        .filter(|name| {
            !name.is_empty()
                && matches!(
                    Path::new(name).extension().and_then(|e| e.to_str()),
                    Some("rs" | "py" | "toml" | "json" | "lock" | "sh" | "yml")
                )
        })
        .collect();
    paths.sort_unstable();
    paths.dedup();
    let mut digest = Sha256::new();
    for name in paths {
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        let bytes = fs::read(root.join(name))?;
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    Ok(format!("{:x}", digest.finalize()))
}
