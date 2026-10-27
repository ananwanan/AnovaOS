// SPDX-License-Identifier: Apache-2.0
//! Fetch pinned boot assets with transport-only mirror fallback and verified cache publication.
//! Check package and extracted firmware hashes; a checksum failure never tries another source.
use crate::{Result, Temporary, checked_file, command, directory};
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
    #[serde(default)]
    pub mirrors: Vec<String>,
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

fn download(root: &Path, url: &str, mirrors: &[String], expected: &str, path: &Path) -> Result<()> {
    download_using(root, url, mirrors, expected, path, |source, destination| {
        command(
            Command::new("curl")
                .args([
                    "--fail",
                    "--location",
                    "--proto",
                    "=https",
                    "--proto-redir",
                    "=https",
                    "--tlsv1.2",
                    "--retry",
                    "2",
                    "--retry-delay",
                    "5",
                    "--retry-max-time",
                    "360",
                    "--connect-timeout",
                    "15",
                    "--max-time",
                    "180",
                    "--output",
                ])
                .arg(destination)
                .arg(source),
        )
    })
}

fn download_using(
    root: &Path,
    url: &str,
    mirrors: &[String],
    expected: &str,
    path: &Path,
    mut transfer: impl FnMut(&str, &Path) -> Result<()>,
) -> Result<()> {
    if fs::symlink_metadata(path).is_ok() {
        return verify(path, expected);
    }
    let temporary = Temporary::new(root, "boot-download")?;
    let part = temporary.0.join("asset.part");
    let mut failure = None;
    for source in std::iter::once(url).chain(mirrors.iter().map(String::as_str)) {
        if !source.starts_with("https://") || source.contains('@') {
            return Err("Boot inputs require credential-free HTTPS URLs".into());
        }
        if let Err(error) = transfer(source, &part) {
            failure = Some(error);
            if part.exists() {
                fs::remove_file(&part)?;
            }
            continue;
        }
        // An invalid digest is an integrity failure, not permission to use a different source.
        verify(&part, expected)?;
        match fs::hard_link(&part, path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                verify(path, expected)?
            }
            Err(error) => return Err(error.into()),
        }
        return Ok(());
    }
    Err(failure.ok_or("No boot input source was attempted")?)
}

pub fn fetch(root: &Path) -> Result<()> {
    let lock = lock(root)?;
    let cache = directory(root, "cache")?;
    for asset in &lock.assets {
        download(
            root,
            &asset.url,
            &[],
            &asset.sha256,
            &cache.join(&asset.file),
        )?;
    }
    let package = cache.join("ovmf.deb");
    download(
        root,
        &lock.firmware.url,
        &lock.firmware.mirrors,
        &lock.firmware.package_sha256,
        &package,
    )?;
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

    const ABC_SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn download_fixture(name: &str) -> PathBuf {
        let path = env::temp_dir().join(format!(
            "anova-boot-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn boot_transport_failure_uses_locked_mirror_and_discards_partial() {
        let root = download_fixture("fallback");
        let target = root.join("asset");
        let mut calls = Vec::new();
        download_using(
            &root,
            "https://primary/asset",
            &["https://mirror/asset".into()],
            ABC_SHA,
            &target,
            |url, part| {
                calls.push(url.to_owned());
                assert!(!part.exists());
                if calls.len() == 1 {
                    fs::write(part, b"partial transport")?;
                    return Err("HTTP 503".into());
                }
                fs::write(part, b"abc")?;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(calls, ["https://primary/asset", "https://mirror/asset"]);
        assert_eq!(fs::read(target).unwrap(), b"abc");
        assert_eq!(fs::read_dir(root.join("build/tmp")).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn boot_integrity_failure_does_not_try_mirror_or_publish_cache() {
        let root = download_fixture("integrity");
        let target = root.join("asset");
        let mut calls = 0;
        assert!(
            download_using(
                &root,
                "https://primary/asset",
                &["https://mirror/asset".into()],
                ABC_SHA,
                &target,
                |_, part| {
                    calls += 1;
                    fs::write(part, b"corrupt")?;
                    Ok(())
                }
            )
            .is_err()
        );
        assert_eq!(calls, 1);
        assert!(!target.exists());
        assert_eq!(fs::read_dir(root.join("build/tmp")).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn boot_existing_corrupt_cache_is_preserved_without_download() {
        let root = download_fixture("cache");
        let target = root.join("asset");
        fs::write(&target, b"corrupt").unwrap();
        assert!(
            download_using(
                &root,
                "https://primary/asset",
                &[],
                ABC_SHA,
                &target,
                |_, _| panic!("Corrupt cache must not be replaced")
            )
            .is_err()
        );
        assert_eq!(fs::read(&target).unwrap(), b"corrupt");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn boot_racing_cache_publication_is_verified_without_overwrite() {
        let root = download_fixture("publish");
        let target = root.join("asset");
        assert!(
            download_using(
                &root,
                "https://primary/asset",
                &[],
                ABC_SHA,
                &target,
                |_, part| {
                    fs::write(part, b"abc")?;
                    fs::write(&target, b"unrelated publisher")?;
                    Ok(())
                }
            )
            .is_err()
        );
        assert_eq!(fs::read(&target).unwrap(), b"unrelated publisher");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn boot_cache_symlinks_are_rejected_even_when_the_target_is_missing() {
        let root = download_fixture("symlink");
        let target = root.join("asset");
        std::os::unix::fs::symlink(root.join("missing"), &target).unwrap();
        assert!(
            download_using(
                &root,
                "https://primary/asset",
                &[],
                ABC_SHA,
                &target,
                |_, _| panic!("Redirected cache must not be followed")
            )
            .is_err()
        );
        assert!(fs::symlink_metadata(&target).unwrap().is_symlink());
        fs::remove_dir_all(root).unwrap();
    }

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
