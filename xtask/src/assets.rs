// SPDX-License-Identifier: Apache-2.0
use crate::{Result, checked_file, command, directory, output_path};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lock {
    pub rust: String,
    pub qemu: String,
    pub limine_version: String,
    pub limine_commit: String,
    pub protocol_commit: String,
    pub assets: Vec<Asset>,
    pub firmware: Firmware,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub file: String,
    pub url: String,
    pub sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Firmware {
    pub version: String,
    pub url: String,
    pub package_sha256: String,
    pub code_sha256: String,
    pub vars_sha256: String,
}

pub fn lock(root: &Path) -> Result<Lock> {
    Ok(toml::from_str(&fs::read_to_string(
        root.join("boot/dependencies.toml"),
    )?)?)
}

pub fn verify(path: &Path, expected: &str) -> Result<()> {
    checked_file(path)?;
    let actual = format!("{:x}", Sha256::digest(fs::read(path)?));
    if actual != expected {
        return Err(format!(
            "SHA-256 mismatch for {}: expected {expected}, got {actual}",
            path.display()
        )
        .into());
    }
    Ok(())
}

fn download(url: &str, expected: &str, path: &Path) -> Result<()> {
    if path.exists() {
        return verify(path, expected);
    }
    let part = path.with_extension("part");
    output_path(&part)?;
    command(
        Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--proto",
                "=https",
                "--tlsv1.2",
                "--retry",
                "2",
                "--max-time",
                "120",
                "--output",
            ])
            .arg(&part)
            .arg(url),
    )?;
    verify(&part, expected)?;
    fs::rename(part, path)?;
    Ok(())
}

pub fn fetch(root: &Path) -> Result<()> {
    let lock = lock(root)?;
    let cache = directory(root, "cache")?;
    for asset in &lock.assets {
        download(&asset.url, &asset.sha256, &cache.join(&asset.file))?;
    }
    let package = cache.join("ovmf.deb");
    download(&lock.firmware.url, &lock.firmware.package_sha256, &package)?;
    let firmware = directory(root, "firmware")?;
    // The archive is pinned and checked before extraction. No install scripts
    // run, no privileges are requested, and all files stay under build/.
    command(
        Command::new("dpkg-deb")
            .arg("--extract")
            .arg(package)
            .arg(&firmware),
    )?;
    check(root)?;
    println!(
        "Verified Limine {} ({}) and OVMF {}",
        lock.limine_version, lock.limine_commit, lock.firmware.version
    );
    Ok(())
}

pub fn firmware(root: &Path) -> Result<(PathBuf, PathBuf)> {
    let lock = lock(root)?;
    let defaults = root.join("build/firmware/usr/share/OVMF");
    let code = env::var_os("ANOVA_OVMF_CODE")
        .map_or_else(|| defaults.join("OVMF_CODE_4M.fd"), PathBuf::from);
    let vars = env::var_os("ANOVA_OVMF_VARS")
        .map_or_else(|| defaults.join("OVMF_VARS_4M.fd"), PathBuf::from);
    verify(&code, &lock.firmware.code_sha256)?;
    verify(&vars, &lock.firmware.vars_sha256)?;
    // Only these exact, tested firmware contents are accepted, including when
    // explicit paths are used. Paths do not disable integrity checks.
    Ok((code.canonicalize()?, vars.canonicalize()?))
}

pub fn check(root: &Path) -> Result<()> {
    let lock = lock(root)?;
    for asset in &lock.assets {
        verify(&root.join("build/cache").join(&asset.file), &asset.sha256)?;
    }
    firmware(root)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_corrupt_download() {
        let dir = env::temp_dir().join(format!("anova-hash-test-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("asset");
        fs::write(&path, b"abc").unwrap();
        verify(
            &path,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .unwrap();
        assert!(
            verify(&path, &"0".repeat(64))
                .unwrap_err()
                .to_string()
                .contains("SHA-256 mismatch")
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
