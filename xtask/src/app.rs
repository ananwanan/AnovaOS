// SPDX-License-Identifier: Apache-2.0
//! Put an externally hash-locked application in a bootable image and exercise the documented workflow.
//! Persistent disk policy is explicit; only build-directory regular disk images reach QEMU.
use super::*;
use sha2::{Digest, Sha256};

struct Prepared {
    iso: PathBuf,
    linux: bool,
    name: String,
    entry: usize,
}

fn prepare(root: &Path, package: &Path, expected: &str) -> Result<Prepared> {
    // Verify the caller's external lock before interpreting any archive field.
    checked_file(package)?;
    if fs::metadata(package)?.len() > 128 * 1024 {
        return Err("Application package exceeds the 128 KiB profile".into());
    }
    assets::verify(package, expected)?;
    apps::build(root, false)?;
    command(
        Command::new(root.join("build/apps/pkg.elf"))
            .arg("check")
            .arg(package)
            .arg(expected),
    )?;
    let bytes = fs::read(package)?;
    if bytes.len() < 128 || format!("{:x}", Sha256::digest(&bytes)) != expected {
        return Err("Application package changed after validation".into());
    }
    let name = std::str::from_utf8(&bytes[8..32])?
        .trim_end_matches('\0')
        .to_owned();
    let linux = bytes[64..96].starts_with(b"linux.x86_64.v1\0");
    let entry = if bytes.starts_with(b"ANOVAPK2") {
        let count = u32::from_le_bytes(bytes[44..48].try_into()?) as usize;
        (0..count)
            .find(|index| bytes[256 + index * 96..256 + index * 96 + 48] == bytes[144..192])
            .ok_or("Package entry missing after validation")?
    } else {
        if bytes.get(176..180) != Some(&0o755_u32.to_le_bytes()) {
            return Err("Legacy application image requires executable file0".into());
        }
        0
    };
    let mut archive = fs::read(root.join("build/apps/initramfs.anovafs"))?;
    toolchain::append(&mut archive, 1, "/repo/local.apkg", &bytes);
    let work = directory(root, "application")?;
    let path = work.join("initramfs.anovafs");
    output_path(&path)?;
    fs::write(&path, archive)?;
    let built = image_archive(root, Mode::Normal, Some(&path))?;
    let iso = root.join("build/images/anova-app.iso");
    output_path(&iso)?;
    fs::rename(built, &iso)?;
    println!(
        "Application image: {}\nDisk ABI: {}\nGuest commands for a first install:\n  pkg inspect /repo/local.apkg {expected}\n  pkg install-file /repo/local.apkg {expected}\n  pkg status {name}\n  /persist/pkg/{name}-a/file{entry}\n  status\n  shutdown\nAfter upgrades, use the active slot printed by pkg status.",
        iso.display(),
        if linux {
            anova_abi::linux::PERSONALITY
        } else {
            anova_abi::PERSONALITY
        }
    );
    Ok(Prepared {
        iso,
        linux,
        name,
        entry,
    })
}

pub fn image(root: &Path, args: &[String]) -> Result<()> {
    let [package, hash] = args else {
        return Err("Usage: cargo xtask app-image PACKAGE SHA256".into());
    };
    prepare(root, Path::new(package), hash).map(|_| ())
}

pub fn run(root: &Path, args: &[String]) -> Result<()> {
    let (package, hash, disk_path) = match args {
        [package, hash] => (package, hash, None),
        [package, hash, flag, disk] if flag == "--disk" => (package, hash, Some(disk)),
        _ => {
            return Err(
                "Usage: cargo xtask app-run PACKAGE SHA256 [--disk build/disks/NAME.ext2]".into(),
            );
        }
    };
    let prepared = prepare(root, Path::new(package), hash)?;
    let vars = Temporary::new(root, "application-run")?;
    let disk_path = if let Some(path) = disk_path {
        let path = disk::user_path(root, path)?;
        checked_file(&path)?;
        path
    } else {
        let path = vars.0.join("application.ext2");
        disk::create_profile(root, &path, prepared.linux)?;
        println!("Using a temporary disk; use --disk to preserve application/user data");
        path
    };
    // An existing disk is never reformatted; the guest installer checks its ABI policy.
    command(
        qemu_disk(root, &prepared.iso, &vars.0, false, Some(&disk_path), false)?
            .args(["-serial", "stdio"]),
    )
}

/// Exercise the public image path, external lock rejection and actual installed entry/exit status.
pub fn verify(root: &Path) -> Result<()> {
    let result = directory(root, "logs")?.join("app-workflow.result.txt");
    output_path(&result)?;
    fs::write(&result, "result=Running\n")?;
    sdk::verify(root)?;
    let package = root.join("build/sdk/templates/c/local.apkg");
    packages::pack_manifest(root, &package.with_file_name("app.toml"), &package)?;
    let hash = format!("{:x}", Sha256::digest(fs::read(&package)?));
    if prepare(root, &package, &"0".repeat(64)).is_ok() {
        return Err("Application image accepted an incorrect external package lock".into());
    }
    let prepared = prepare(root, &package, &hash)?;
    let vars = Temporary::new(root, "app-workflow")?;
    let disk_path = vars.0.join("application.ext2");
    disk::create_profile(root, &disk_path, prepared.linux)?;
    let install = format!("pkg install-file /repo/local.apkg {hash}\r");
    let entry = format!("/persist/pkg/{}-a/file{}\r", prepared.name, prepared.entry);
    disk::session_timeout(
        root,
        &prepared.iso,
        &disk_path,
        "app-workflow",
        false,
        None,
        &[
            ("anova> ", install.as_bytes()),
            ("anova> ", b"status\r"),
            ("[SHELL:STATUS] code=0", entry.as_bytes()),
            ("anova> ", b"status\r"),
            ("[SHELL:STATUS] code=0", b"shutdown\r"),
        ],
        true,
        120,
    )?;
    let log = fs::read_to_string(root.join("build/logs/app-workflow.serial.log"))?;
    if !log.contains("[M13:C] native SDK v1") || !log.contains("\r\nnot_found\r\n") {
        return Err("Application workflow missed actual SDK output and stable error".into());
    }
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(&disk_path))?;
    fs::write(
        result,
        "result=Ok\nexternal_hash=rejected\npublic_image=true\ninstalled_entry=true\nexit_status=0\nstable_error=not_found\nclean_fs=true\n",
    )?;
    println!("[M13:APP] external_lock=true image=true installed=true exit=0 error=not_found");
    Ok(())
}
