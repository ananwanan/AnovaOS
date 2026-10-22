// SPDX-License-Identifier: Apache-2.0
//! Verify cold HTTPS registry/Git acquisition by guest Cargo, strict failures and offline reboot reuse.
//! The host serves source bytes only; every accepted executable is compiled inside AnovaOS.
use super::*;
use anova_storage::ext2::Ext2;
use rust::{edit, send, success};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Fixture {
    crate_sha256: String,
    git_rev: String,
}

pub(super) struct CacheProfile<'a> {
    pub project: &'a str,
    pub binary: &'a str,
    pub archive: &'a str,
    pub crate_hash: &'a str,
    pub git_rev: &'a str,
    pub git_source: &'a str,
}

pub fn verify(root: &Path) -> Result<()> {
    let work = directory(root, "rust-online")?;
    command(
        Command::new("python3")
            .arg(root.join("tests/guest/m12_online_peer.py"))
            .arg("--root")
            .arg(&work)
            .arg("--prepare"),
    )?;
    let fixture: Fixture = toml::from_str(&fs::read_to_string(work.join("fixture.toml"))?)?;
    rust::image_profile(root, false)?;
    let mut archive = fs::read(root.join("build/rust/initramfs.anovafs"))?;
    toolchain::append(
        &mut archive,
        1,
        "/etc/m12-online-ca.pem",
        &fs::read(work.join("ca.pem"))?,
    );
    let archive_path = work.join("initramfs.anovafs");
    fs::write(&archive_path, archive)?;
    let image = image_archive(root, Mode::Normal, Some(&archive_path))?;
    let iso = root.join("build/images/anova-rust.iso");
    fs::rename(image, &iso)?;
    let ready = work.join("peer.log");
    let requests = work.join("requests.jsonl");
    fs::write(&requests, [])?;
    let mut peer = Guest(
        Command::new("python3")
            .arg(root.join("tests/guest/m12_online_peer.py"))
            .arg("--root")
            .arg(&work)
            .stdout(File::create(&ready)?)
            .stderr(File::create(work.join("peer.errors.log"))?)
            .spawn()?,
    );
    let start = Instant::now();
    while !fs::read_to_string(&ready)?.contains("[M12:ONLINE:READY]") {
        if peer.0.try_wait()?.is_some() || start.elapsed() > Duration::from_secs(10) {
            return Err(
                "HTTPS/Git peer did not become ready; see build/rust-online/peer.errors.log".into(),
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
    let disk = directory(root, "disks")?.join(format!(
        "rust-online-{}.ext2",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    disk::create_profile(root, &disk, true)?;
    let manifest = fs::read_to_string(root.join("tests/guest/m12/online/Cargo.toml.in"))?
        .replace("@GIT_REV@", &fixture.git_rev);
    let source = fs::read_to_string(root.join("tests/guest/m12/online/main.rs"))?;
    let mut steps = Vec::new();
    for (name, port, scenario, ca) in [
        ("untrusted", 18443, "ok", "/etc/untrusted-ca.pem"),
        ("expired", 18444, "ok", "/etc/m12-online-ca.pem"),
        ("wrong-name", 18445, "ok", "/etc/m12-online-ca.pem"),
        ("corrupt", 18443, "corrupt", "/etc/m12-online-ca.pem"),
        ("truncated", 18443, "truncated", "/etc/m12-online-ca.pem"),
        ("timeout", 18443, "timeout", "/etc/m12-online-ca.pem"),
        ("online", 18443, "ok", "/etc/m12-online-ca.pem"),
    ] {
        success(&mut steps, "cd /persist\r");
        success(&mut steps, &format!("mkdir {name}\r"));
        success(&mut steps, &format!("cd {name}\r"));
        success(&mut steps, "mkdir src\r");
        success(&mut steps, "mkdir .cargo\r");
        let config = format!(
            "# Select an independently fetched HTTPS sparse index and require its CA.\n\
             [registries.m12]\nindex = \"sparse+https://10.0.2.2:{port}/{scenario}/index/\"\n\
             [http]\ncainfo = \"{ca}\"\nmultiplexing = false\ntimeout = 3\n\
             [net]\nretry = 0\noffline = false\n"
        );
        let input = if name == "online" {
            manifest.clone()
        } else {
            manifest
                .lines()
                .filter(|line| !line.starts_with("m12-git-dep ="))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        };
        edit(&mut steps, &format!("/persist/{name}/Cargo.toml"), &input)?;
        edit(&mut steps, &format!("/persist/{name}/src/main.rs"), &source)?;
        edit(
            &mut steps,
            &format!("/persist/{name}/.cargo/config.toml"),
            &config,
        )?;
        if name != "online" {
            send(&mut steps, "anova> ", "cargo-online fetch\r");
            send(&mut steps, "anova> ", "status\r");
            send(&mut steps, "[SHELL:STATUS] code=101", "");
        } else {
            success(&mut steps, "cargo-online build\r");
            success(&mut steps, "./target/debug/m12-online-app\r");
            success(&mut steps, "cargo-online build --locked\r");
        }
    }
    success(&mut steps, "cd /persist/truncated\r");
    send(&mut steps, "anova> ", "cargo-online fetch --locked\r");
    send(&mut steps, "anova> ", "status\r");
    send(&mut steps, "[SHELL:STATUS] code=101", "");
    success(&mut steps, "cd /persist/online\r");
    success(&mut steps, "./target/debug/m12-online-app\r");
    success(&mut steps, "cd /persist/corrupt\r");
    success(&mut steps, "cargo-online fetch --locked\r");
    success(&mut steps, "cd /persist/online\r");
    send(&mut steps, "anova> ", "shutdown\r");
    let exchanges: Vec<_> = steps
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_slice()))
        .collect();
    disk::session_network(
        root,
        &iso,
        &disk,
        "rust-online",
        false,
        None,
        &exchanges,
        true,
        900,
        true,
    )?;
    let serial = fs::read_to_string(root.join("build/logs/rust-online.serial.log"))?;
    let failures: Vec<_> = serial
        .split("anova> cargo-online fetch\r\n")
        .skip(1)
        .collect();
    for (index, fragments) in [
        &["[60]", "unable to get local issuer certificate"][..],
        &["[60]", "certificate has expired"],
        &["[60]", "no alternative certificate subject name matches"],
        &["failed to verify the checksum"],
        &["failed to download from", "unexpected eof"],
        &["[28]", "timed out"],
    ]
    .iter()
    .enumerate()
    {
        let section = failures
            .get(index)
            .ok_or("Missing negative Cargo acquisition")?
            .split("anova> status")
            .next()
            .unwrap();
        if !fragments
            .iter()
            .all(|fragment| section.to_ascii_lowercase().contains(fragment))
        {
            return Err(format!(
                "Wrong failure reason for online negative case {index}: {section}"
            )
            .into());
        }
    }
    if serial.contains("edit: command/save failed") {
        return Err("UART source editing failed".into());
    }
    for marker in [
        "[M12:ONLINE] registry=true git=true answer=42",
        "Downloading crates",
        "Updating git repository",
        "checksum",
        "certificate",
        "timed out",
    ] {
        if !serial
            .to_ascii_lowercase()
            .contains(&marker.to_ascii_lowercase())
        {
            return Err(format!("Missing actual online Cargo diagnostic: {marker}").into());
        }
    }
    for name in ["m12-net-dep", "m12-git-dep", "m12-online-app"] {
        if serial.matches(&format!("Compiling {name} v")).count() != 1 {
            return Err(format!("Unexpected online/no-op compile count: {name}").into());
        }
    }
    let traffic = fs::read_to_string(&requests)?;
    if traffic
        .matches("/corrupt/download/m12-net-dep/0.1.0")
        .count()
        != 2
    {
        return Err(
            "Corrupt archive was not downloaded again after a real checksum failure".into(),
        );
    }
    for path in [
        "/ok/index/config.json",
        "/ok/index/m1/2-/m12-net-dep",
        "/ok/download/m12-net-dep/0.1.0",
        "/git/dependency.git/info/refs?service=git-upload-pack",
        "/git/dependency.git/git-upload-pack",
        "/corrupt/download/m12-net-dep/0.1.0",
        "/truncated/download/m12-net-dep/0.1.0",
        "/timeout/index/config.json",
    ] {
        if !traffic.contains(path) {
            return Err(format!("Missing actual guest request: {path}").into());
        }
    }
    let profile = CacheProfile {
        project: "online",
        binary: "m12-online-app",
        archive: "m12-net-dep-0.1.0.crate",
        crate_hash: &fixture.crate_sha256,
        git_rev: &fixture.git_rev,
        git_source: "pub fn answer() -> u32 { 21 }",
    };
    let before = inspect(&disk, &profile)?;
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(&disk))?;
    // Remove the entire peer and NIC: the independent boot must use only guest-downloaded caches.
    drop(peer);
    disk::session_timeout(
        root,
        &iso,
        &disk,
        "rust-online-reboot",
        false,
        None,
        &[
            ("anova> ", b"cd /persist/online\r"),
            ("anova> ", b"cargo-online build --offline --frozen\r"),
            ("anova> ", b"status\r"),
            ("[SHELL:STATUS] code=0", b"./target/debug/m12-online-app\r"),
            (
                "[M12:ONLINE] registry=true git=true answer=42",
                b"shutdown\r",
            ),
        ],
        true,
        300,
    )?;
    if before != inspect(&disk, &profile)? {
        return Err(
            "Online source, locked cache or executable changed across offline reboot".into(),
        );
    }
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(&disk))?;
    fs::write(
        root.join("build/logs/rust-online-files.sha256"),
        before
            .iter()
            .map(|(name, hash)| format!("{hash}  {name}\n"))
            .collect::<String>(),
    )?;
    fs::write(
        root.join("build/logs/rust-online.result.txt"),
        format!(
            "result=Ok\ncompiler=guest\nregistry=https-sparse\ngit=https-smart\ngit_rev={}\n\
         crate_sha256={}\nstrict_tls=true\ncorruption=rejected\ntruncation=rejected\n\
         timeout=rejected\noffline_reboot=true\npersisted_files={}\ndisk={}\n",
            fixture.git_rev,
            fixture.crate_sha256,
            before.len(),
            disk.display()
        ),
    )?;
    Ok(())
}

pub(super) fn inspect(path: &Path, profile: &CacheProfile<'_>) -> Result<BTreeMap<String, String>> {
    let mut disk = Ext2::mount(Box::new(disk::Image::open(path)?))
        .map_err(|e| format!("Online cache disk mount: {e}"))?;
    let mut pending = vec![(2, String::new())];
    let mut hashes = BTreeMap::new();
    let mut archive = false;
    let mut git = false;
    let mut lock = false;
    let mut executable = false;
    let project = format!("/{}", profile.project);
    let binary = format!("{project}/target/debug/{}", profile.binary);
    while let Some((node, parent)) = pending.pop() {
        for entry in disk
            .entries(node)
            .map_err(|e| format!("Online directory: {e}"))?
        {
            let name = std::str::from_utf8(&entry.name[..entry.length])?;
            if matches!(name, "." | "..") {
                continue;
            }
            let name = format!("{parent}/{name}");
            let inode = disk
                .inode(entry.inode)
                .map_err(|e| format!("Online inode: {e}"))?;
            if inode.directory {
                pending.push((entry.inode, name));
                continue;
            }
            if !name.starts_with("/cargo-home/")
                && ![
                    "/Cargo.toml",
                    "/Cargo.lock",
                    "/.cargo/config.toml",
                    "/src/main.rs",
                ]
                .iter()
                .any(|suffix| name == format!("{project}{suffix}"))
                && name != binary
            {
                continue;
            }
            let mut bytes = vec![0; anova_storage::ext2::MAX_FILE];
            let length = disk
                .read(entry.inode, 0, &mut bytes)
                .map_err(|e| format!("Online file: {e}"))?;
            bytes.truncate(length);
            let digest = format!("{:x}", Sha256::digest(&bytes));
            if name.ends_with(&format!("/{}", profile.archive)) {
                if digest != profile.crate_hash {
                    return Err("Downloaded crate differs from locked bytes".into());
                }
                archive = true;
            }
            if name.starts_with("/cargo-home/git/checkouts/") && name.ends_with("/src/lib.rs") {
                git |= std::str::from_utf8(&bytes)?.contains(profile.git_source);
            }
            if name == format!("{project}/Cargo.lock") {
                let value = std::str::from_utf8(&bytes)?;
                lock = value.contains(profile.git_rev) && value.contains(profile.crate_hash);
            }
            if name == binary {
                anova_elf::parse_linux(&bytes).map_err(|e| format!("Guest online ELF: {e:?}"))?;
                executable = inode.executable;
            }
            hashes.insert(name, digest);
        }
    }
    if !archive || !git || !lock || !executable {
        return Err(format!("Incomplete downloaded-source evidence: archive={archive} git={git} lock={lock} ELF={executable}").into());
    }
    Ok(hashes)
}
