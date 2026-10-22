// SPDX-License-Identifier: Apache-2.0
//! Exercise unmodified public crates.io and GitHub HTTPS from guest Cargo with pinned dependencies.
//! Optional proxy/CA inputs describe the test host; secrets and compilation services are never packed.
use super::*;
use rust::{edit, send, success};
use rust_online::{CacheProfile, inspect};

const REVISION: &str = "e2766b868e4ac1ae2bf5bea1ac43d4c0da23b899";
const CRATE_HASH: &str = "4a5f13b858c8d314ee3e8f639011f7ccefe71f97f96e50151fb991f267928e2c";

pub fn verify(root: &Path) -> Result<()> {
    let work = directory(root, "rust-public")?;
    rust::image_profile(root, false)?;
    let mut config = String::from(
        "# Require HTTPS, bounded transfers and ordinary public Cargo sources.\n\
         [http]\nmultiplexing = false\ntimeout = 60\n\
         [net]\nretry = 1\noffline = false\n",
    );
    let mut archive = fs::read(root.join("build/rust/initramfs.anovafs"))?;
    if let Some(path) = env::var_os("ANOVA_CARGO_TEST_CA") {
        let path = PathBuf::from(path);
        checked_file(&path)?;
        let bytes = fs::read(&path)?;
        let text = std::str::from_utf8(&bytes)?;
        if bytes.len() > 1024 * 1024
            || !text.contains("-----BEGIN CERTIFICATE-----")
            || text.contains("PRIVATE KEY")
        {
            return Err("ANOVA_CARGO_TEST_CA must contain public PEM certificates only".into());
        }
        toolchain::append(&mut archive, 1, "/etc/m12-public-ca.pem", &bytes);
        config = config.replace("[http]\n", "[http]\ncainfo = \"/etc/m12-public-ca.pem\"\n");
    }
    if let Ok(proxy) = env::var("ANOVA_CARGO_TEST_PROXY") {
        if !proxy.starts_with("http://")
            || proxy.contains(['@', '"', '\n', '\r', '\\'])
            || proxy.len() > 200
        {
            return Err("Use a reachable HTTP test proxy without embedded credentials".into());
        }
        config = config.replace("[http]\n", &format!("[http]\nproxy = \"{proxy}\"\n"));
    }
    let archive_path = work.join("initramfs.anovafs");
    fs::write(&archive_path, archive)?;
    let image = image_archive(root, Mode::Normal, Some(&archive_path))?;
    let iso = root.join("build/images/anova-rust.iso");
    fs::rename(image, &iso)?;
    let disk = directory(root, "disks")?.join(format!(
        "rust-public-{}.ext2",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    disk::create_profile(root, &disk, true)?;
    let manifest = fs::read_to_string(root.join("tests/guest/m12/public/Cargo.toml"))?;
    if !manifest.contains(REVISION) {
        return Err("Public Git fixture revision differs from its accepted lock".into());
    }
    let source = fs::read_to_string(root.join("tests/guest/m12/public/main.rs"))?;
    let mut steps = Vec::new();
    for line in [
        "cd /persist\r",
        "mkdir public\r",
        "cd public\r",
        "mkdir src\r",
        "mkdir .cargo\r",
    ] {
        success(&mut steps, line);
    }
    edit(&mut steps, "Cargo.toml", &manifest)?;
    edit(&mut steps, "src/main.rs", &source)?;
    edit(&mut steps, ".cargo/config.toml", &config)?;
    success(&mut steps, "cargo-online build\r");
    success(&mut steps, "./target/debug/m12-public-app\r");
    success(&mut steps, "cargo-online build --locked\r");
    send(&mut steps, "anova> ", "shutdown\r");
    let exchanges: Vec<_> = steps
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_slice()))
        .collect();
    disk::session_network(
        root,
        &iso,
        &disk,
        "rust-public",
        false,
        None,
        &exchanges,
        true,
        900,
        true,
    )?;
    let serial = fs::read_to_string(root.join("build/logs/rust-public.serial.log"))?;
    for marker in [
        "Updating crates.io index",
        "https://github.com/dtolnay/itoa",
        "Downloaded itoa v1.0.15",
        "[M12:PUBLIC] crates.io=true github=true answer=42",
    ] {
        if !serial.contains(marker) {
            return Err(format!("Missing actual public Cargo acquisition: {marker}").into());
        }
    }
    let profile = CacheProfile {
        project: "public",
        binary: "m12-public-app",
        archive: "itoa-1.0.15.crate",
        crate_hash: CRATE_HASH,
        git_rev: REVISION,
        git_source: "mod buffer;",
    };
    let before = inspect(&disk, &profile)?;
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(&disk))?;
    disk::session_timeout(
        root,
        &iso,
        &disk,
        "rust-public-reboot",
        false,
        None,
        &[
            ("anova> ", b"cd /persist/public\r"),
            ("anova> ", b"cargo-online build --offline --frozen\r"),
            ("anova> ", b"status\r"),
            ("[SHELL:STATUS] code=0", b"./target/debug/m12-public-app\r"),
            (
                "[M12:PUBLIC] crates.io=true github=true answer=42",
                b"shutdown\r",
            ),
        ],
        true,
        300,
    )?;
    if before != inspect(&disk, &profile)? {
        return Err("Public source, cache or executable changed across offline reboot".into());
    }
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(&disk))?;
    fs::write(
        root.join("build/logs/rust-public-files.sha256"),
        before
            .iter()
            .map(|(name, hash)| format!("{hash}  {name}\n"))
            .collect::<String>(),
    )?;
    fs::write(
        root.join("build/logs/rust-public.result.txt"),
        format!(
            "result=Ok\ncompiler=guest\nregistry=crates.io\ngit=github.com/dtolnay/itoa\n\
         git_rev={REVISION}\ncrate_sha256={CRATE_HASH}\nstrict_tls=true\n\
         offline_reboot=true\npersisted_files={}\ndisk={}\n",
            before.len(),
            disk.display()
        ),
    )?;
    Ok(())
}
