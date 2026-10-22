// SPDX-License-Identifier: Apache-2.0
//! Package an independently locked musl Rust sysroot and validate compilation inside real guests.
use super::*;
use sha2::{Digest, Sha256};

pub fn build(root: &Path) -> Result<()> {
    toolchain::build(root)?;
    command(Command::new("python3").arg(root.join("ports/rust/build.py")))
}

pub fn cross(root: &Path) -> Result<()> {
    apps::build(root, false)?;
    let work = directory(root, "rust")?;
    let elf = work.join("cross.elf");
    command(
        Command::new("rustc")
            .args([
                "--edition=2024",
                "--target=x86_64-unknown-linux-musl",
                "-Copt-level=1",
                "-Cstrip=symbols",
                "-Clinker=musl-gcc",
                "-Crelocation-model=static",
                "-Clink-arg=-no-pie",
            ])
            .arg(root.join("tests/guest/m12/main.rs"))
            .arg("-o")
            .arg(&elf),
    )?;
    let data = fs::read(elf)?;
    anova_elf::parse_linux(&data).map_err(|e| format!("Rust std cross ELF: {e:?}"))?;
    let mut bytes = fs::read(root.join("build/apps/initramfs.anovafs"))?;
    toolchain::append(&mut bytes, 5, "/bin/m12-cross", &data);
    let runtime = work.join("runtime.elf");
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
            .arg(root.join("tests/guest/m12/runtime.c"))
            .arg("-o")
            .arg(&runtime),
    )?;
    toolchain::append(&mut bytes, 5, "/bin/m12-runtime", &fs::read(runtime)?);
    let archive = work.join("cross.anovafs");
    fs::write(&archive, bytes)?;
    let iso = image_archive(root, Mode::Normal, Some(&archive))?;
    let disk = directory(root, "disks")?.join(format!(
        "rust-cross-{}.ext2",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    disk::create_profile(root, &disk, true)?;
    disk::session_timeout(
        root,
        &iso,
        &disk,
        "rust-cross",
        false,
        None,
        &[
            ("anova> ", b"cd /persist\r"),
            ("anova> ", b"m12-runtime\r"),
            (
                "[M12:RUNTIME] altstack delivery return negatives rename open-inode locks",
                b"m12-cross guest\r",
            ),
            (
                "[M12:STD] Vec String argv files thread child",
                b"shutdown\r",
            ),
        ],
        true,
        120,
    )
}

pub fn image(root: &Path) -> Result<PathBuf> {
    image_profile(root, true)
}
fn image_profile(root: &Path, c_toolchain: bool) -> Result<PathBuf> {
    apps::build(root, false)?;
    command(
        Command::new("python3")
            .arg(root.join("ports/rust/build.py"))
            .arg("--verify"),
    )?;
    let base = if c_toolchain {
        toolchain::pack(root)?
    } else {
        root.join("build/apps/initramfs.anovafs")
    };
    let work = root.join("build/rust");
    let mut bytes = fs::read(base)?;
    let stage = work.join("stage");
    let mut paths = vec![stage.join("rs")];
    while let Some(path) = paths.pop() {
        let guest = format!("/{}", path.strip_prefix(&stage)?.display());
        if path.is_dir() {
            toolchain::append(&mut bytes, 2, &guest, &[]);
            let mut children = fs::read_dir(&path)?
                .map(|p| p.map(|p| p.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            children.sort();
            paths.extend(children.into_iter().rev());
        } else {
            checked_file(&path)?;
            let data = fs::read(&path)?;
            let kind = if guest.starts_with("/rs/bin/") {
                anova_elf::parse_linux(&data)
                    .map_err(|e| format!("Rust executable {guest}: {e:?}"))?;
                5
            } else {
                1
            };
            toolchain::append(&mut bytes, kind, &guest, &data);
        }
    }
    let wrapper = work.join("launcher.elf");
    toolchain::append(&mut bytes, 3, "/rs/lib/libgcc_s.so", b"libgcc_s.so.1");
    toolchain::append(
        &mut bytes,
        3,
        "/rs/lib/libc.so",
        b"/lib/ld-musl-x86_64.so.1",
    );
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
            .arg(root.join("user/linux/rust.c"))
            .arg("-o")
            .arg(&wrapper),
    )?;
    toolchain::append(&mut bytes, 5, "/bin/rustc", &fs::read(wrapper)?);
    toolchain::append(&mut bytes, 3, "/bin/cargo", b"rustc");
    let mut archive =
        anova_vfs::archive::Archive::new(&bytes).map_err(|e| format!("Rust archive: {e}"))?;
    while archive
        .entry()
        .map_err(|e| format!("Rust entry: {e}"))?
        .is_some()
    {}
    let archive = work.join("initramfs.anovafs");
    fs::write(&archive, &bytes)?;
    fs::write(
        work.join("archive.sha256"),
        format!("{:x}\n", Sha256::digest(&bytes)),
    )?;
    let iso = image_archive(root, Mode::Normal, Some(&archive))?;
    let target = root.join("build/images/anova-rust.iso");
    output_path(&target)?;
    fs::rename(iso, &target)?;
    println!("Rust image: {}", target.display());
    Ok(target)
}

pub fn runtime(root: &Path) -> Result<()> {
    let iso = image_profile(root, false)?;
    let path = directory(root, "disks")?.join(format!(
        "rust-runtime-{}.ext2",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    disk::create_profile(root, &path, true)?;
    disk::session_timeout(
        root,
        &iso,
        &path,
        "rust-runtime",
        false,
        None,
        &[
            ("anova> ", b"rustc --version\r"),
            ("rustc 1.90.0 (1159e78c4 2025-09-14)", b"cargo --version\r"),
            ("cargo 1.90.0", b"shutdown\r"),
        ],
        true,
        180,
    )
}

pub fn verify(root: &Path) -> Result<()> {
    let iso = image(root)?;
    let path = directory(root, "disks")?.join(format!(
        "rust-{}.ext2",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    disk::create_profile(root, &path, true)?;
    let mut steps = Vec::new();
    send(&mut steps, "anova> ", "cd /persist\r");
    success(&mut steps, "rustc --version\r");
    success(&mut steps, "cargo --version\r");
    success(&mut steps, "mkdir plain\r");
    success(&mut steps, "mkdir plain/src\r");
    for name in ["macro", "macro/src", "macro-support", "macro-support/src"] {
        success(&mut steps, &format!("mkdir {name}\r"));
    }
    for (guest, fixture) in [
        ("main.rs", "main.rs"),
        ("Cargo.toml", "Cargo.toml"),
        ("Cargo.lock", "Cargo.lock"),
        ("src/main.rs", "cargo.rs"),
        ("build.rs", "build.rs"),
        ("helper.c", "helper.c"),
        ("macro/Cargo.toml", "macro/Cargo.toml"),
        ("macro/build.rs", "macro/build.rs"),
        ("macro/src/lib.rs", "macro/src/lib.rs"),
        ("macro-support/Cargo.toml", "macro-support/Cargo.toml"),
        ("macro-support/src/lib.rs", "macro-support/src/lib.rs"),
        ("macro.rs", "macro.rs"),
        ("use_macro.rs", "use_macro.rs"),
        ("bad.rs", "bad.rs"),
        ("plain/Cargo.toml", "plain/Cargo.toml"),
        ("plain/Cargo.lock", "plain/Cargo.lock"),
        ("plain/src/main.rs", "plain/src/main.rs"),
    ] {
        edit(
            &mut steps,
            guest,
            &fs::read_to_string(root.join("tests/guest/m12").join(fixture))?,
        )?;
    }
    success(&mut steps, "mkdir .cargo\r");
    edit(
        &mut steps,
        ".cargo/config.toml",
        &fs::read_to_string(root.join("tests/guest/m12/config.toml"))?,
    )?;
    success(
        &mut steps,
        "cargo build --offline --frozen --manifest-path plain/Cargo.toml\r",
    );
    success(&mut steps, "./plain/target/debug/m12-plain\r");
    success(&mut steps, "cargo build --offline --frozen\r");
    success(&mut steps, "./target/debug/m12-app\r");
    success(&mut steps, "cargo build --offline --frozen\r");
    let changed_source = fs::read_to_string(root.join("tests/guest/m12/cargo.rs"))?
        .replace("buffer.format(42)", "buffer.format(43)")
        .replace("value, \"42\"", "value, \"43\"");
    edit(&mut steps, "src/main.rs", &changed_source)?;
    success(&mut steps, "cargo build --offline --frozen\r");
    success(&mut steps, "./target/debug/m12-app\r");
    edit(
        &mut steps,
        "macro-support/src/lib.rs",
        &fs::read_to_string(root.join("tests/guest/m12/macro-support/src/lib.rs"))?
            .replace("\"42\"", "\"84\""),
    )?;
    success(&mut steps, "cargo build --offline --frozen\r");
    success(&mut steps, "./target/debug/m12-app 84\r");
    edit(
        &mut steps,
        "src/main.rs",
        &changed_source.replace("m12_macro::answer!()", "m12_macro::answer!(invalid)"),
    )?;
    send(&mut steps, "anova> ", "cargo build --offline --frozen\r");
    send(&mut steps, "anova> ", "status\r");
    send(&mut steps, "[SHELL:STATUS] code=101", "");
    success(&mut steps, "./target/debug/m12-app 84\r");
    edit(&mut steps, "src/main.rs", &changed_source)?;
    success(&mut steps, "cargo build --offline --frozen\r");
    success(&mut steps, "./target/debug/m12-app 84\r");
    success(&mut steps, "rustc --edition=2024 main.rs -o hello-rust\r");
    success(&mut steps, "./hello-rust guest\r");
    success(
        &mut steps,
        "rustc --crate-name m12macro --crate-type proc-macro macro.rs -o libm12macro.so\r",
    );
    success(
        &mut steps,
        "rustc --edition=2024 --extern m12macro=libm12macro.so use_macro.rs -o hello-macro\r",
    );
    success(&mut steps, "./hello-macro\r");
    send(&mut steps, "anova> ", "rustc bad.rs -o hello-rust\r");
    send(&mut steps, "anova> ", "status\r");
    send(&mut steps, "[SHELL:STATUS] code=1", "");
    success(&mut steps, "./hello-rust guest\r");
    send(&mut steps, "anova> ", "shutdown\r");
    let exchanges: Vec<_> = steps
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_slice()))
        .collect();
    disk::session_timeout(
        root,
        &iso,
        &path,
        "rust-build",
        false,
        None,
        &exchanges,
        true,
        1200,
    )?;
    let serial = fs::read_to_string(root.join("build/logs/rust-build.serial.log"))?;
    for marker in [
        "rustc 1.90.0",
        "cargo 1.90.0",
        "[M12:STD] Vec String argv files thread child",
        "[M12:MACRO] loaded shared object and expanded answer=42",
        "[M12:PLAIN] dependency-free Cargo answer=42",
        "[M12:FFI] build.rs gcc and static C linkage answer=84",
        "[M12:CARGO] offline vendor value=42",
        "[M12:CARGO] offline vendor value=43",
        "[M12:CARGO_MACRO] answer=42",
        "[M12:CARGO_MACRO] answer=84",
        "m12-macro expects no tokens",
        "error:",
    ] {
        if !serial.contains(marker) {
            return Err(format!("Missing actual Rust acceptance marker: {marker}").into());
        }
    }
    for (name, expected) in [
        ("itoa", 1),
        ("m12-app", 5),
        ("m12-plain", 1),
        ("m12-macro", 2),
        ("m12-macro-support", 2),
    ] {
        let count = serial.matches(&format!("Compiling {name} v")).count();
        if count != expected {
            return Err(
                format!("Cargo compile count for {name}: {count}, expected {expected}").into(),
            );
        }
    }
    let before = inspect(root, &path)?;
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(&path))?;
    disk::session_timeout(
        root,
        &iso,
        &path,
        "rust-reboot",
        false,
        None,
        &[
            ("anova> ", b"cd /persist\r"),
            ("anova> ", b"./hello-rust guest\r"),
            (
                "[M12:STD] Vec String argv files thread child",
                b"./hello-macro\r",
            ),
            (
                "[M12:MACRO] loaded shared object and expanded answer=42",
                b"./target/debug/m12-app 84\r",
            ),
            (
                "[M12:CARGO] offline vendor value=43",
                b"./plain/target/debug/m12-plain\r",
            ),
            ("[M12:PLAIN] dependency-free Cargo answer=42", b"shutdown\r"),
        ],
        true,
        600,
    )?;
    if before != inspect(root, &path)? {
        return Err("Guest Rust source or ELF changed across reboot".into());
    }
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(&path))?;
    fs::write(
        root.join("build/logs/rust.result.txt"),
        format!(
            "result=Ok\ncompiler=guest\noffline=true\nvendor=itoa-1.0.15\ncargo_proc_macro=true\nmacro_dependency_rebuild=42_to_84\nmacro_error_exit=101\npersisted_files={}\nreboot=true\ndisk={}\n",
            before.len(),
            path.display()
        ),
    )?;
    Ok(())
}
fn send(steps: &mut Vec<(String, Vec<u8>)>, marker: &str, command: &str) {
    steps.push((marker.into(), command.as_bytes().to_vec()));
}
fn success(steps: &mut Vec<(String, Vec<u8>)>, command: &str) {
    send(steps, "anova> ", command);
    send(steps, "anova> ", "status\r");
    send(steps, "[SHELL:STATUS] code=0", "");
}
fn edit(steps: &mut Vec<(String, Vec<u8>)>, name: &str, source: &str) -> Result<()> {
    if source.lines().any(|line| line.len() > 250) {
        return Err("Rust editor fixture line too long".into());
    }
    let command = format!("edit {name}\r");
    send(steps, "anova> ", &command);
    // These fixed source edits preserve line counts, including the deliberately invalid macro call.
    let mut input = String::new();
    if steps
        .iter()
        .filter(|(_, b)| b.as_slice() == command.as_bytes())
        .count()
        > 1
    {
        for _ in source.lines() {
            input.push_str("d 1\r");
        }
    }
    input.push_str(&format!("a\r{}.\rw\r", source.replace('\n', "\r")));
    send(steps, "AnovaOS line editor:", &input);
    send(steps, "[EDIT:SAVED] fsync=ok", "q\r");
    Ok(())
}
fn inspect(root: &Path, path: &Path) -> Result<Vec<(String, [u8; 32])>> {
    use anova_storage::ext2::Ext2;
    let mut disk = Ext2::mount(Box::new(disk::Image::open(path)?))
        .map_err(|e| format!("Rust disk mount: {e}"))?;
    let mut hashes: Vec<(String, [u8; 32])> = Vec::new();
    for name in [
        "main.rs",
        "Cargo.toml",
        "Cargo.lock",
        "src/main.rs",
        "build.rs",
        "helper.c",
        "macro/Cargo.toml",
        "macro/build.rs",
        "macro/src/lib.rs",
        "macro-support/Cargo.toml",
        "macro-support/src/lib.rs",
        "macro.rs",
        "use_macro.rs",
        ".cargo/config.toml",
        "bad.rs",
        "plain/Cargo.toml",
        "plain/Cargo.lock",
        "plain/src/main.rs",
        "hello-rust",
        "hello-macro",
        "libm12macro.so",
        "target/debug/m12-app",
        "plain/target/debug/m12-plain",
    ] {
        let mut node = 2;
        for part in name.split('/') {
            node = disk
                .entries(node)
                .map_err(|e| format!("Rust disk directory: {e}"))?
                .iter()
                .find(|entry| &entry.name[..entry.length] == part.as_bytes())
                .ok_or_else(|| format!("Missing persisted Rust file: {name}"))?
                .inode;
        }
        let mut bytes = vec![0; anova_storage::ext2::MAX_FILE];
        let n = disk
            .read(node, 0, &mut bytes)
            .map_err(|e| format!("Rust disk file {name}: {e}"))?;
        bytes.truncate(n);
        if name == "libm12macro.so" {
            if bytes.get(..7) != Some(b"\x7fELF\x02\x01\x01")
                || bytes.get(16..18) != Some(&3u16.to_le_bytes())
            {
                return Err("Persisted procedural macro is not an ELF64 shared object".into());
            }
        } else if matches!(
            name,
            "hello-rust" | "hello-macro" | "target/debug/m12-app" | "plain/target/debug/m12-plain"
        ) {
            anova_elf::parse_linux(&bytes)
                .map_err(|e| format!("Guest-built Rust ELF {name}: {e:?}"))?;
            if !disk
                .inode(node)
                .map_err(|e| format!("Rust inode: {e}"))?
                .executable
            {
                return Err("Rust output lost execute mode".into());
            }
        } else {
            let fixture = match name {
                "src/main.rs" => "cargo.rs",
                ".cargo/config.toml" => "config.toml",
                x => x,
            };
            let mut expected = fs::read_to_string(root.join("tests/guest/m12").join(fixture))?;
            if name == "src/main.rs" {
                expected = expected
                    .replace("buffer.format(42)", "buffer.format(43)")
                    .replace("value, \"42\"", "value, \"43\"");
            } else if name == "macro-support/src/lib.rs" {
                expected = expected.replace("\"42\"", "\"84\"");
            }
            if bytes != expected.as_bytes() {
                return Err(format!("Persisted Rust source differs: {name}").into());
            }
        }
        hashes.push((name.into(), Sha256::digest(&bytes).into()));
    }
    fs::write(
        root.join("build/logs/rust-files.sha256"),
        hashes
            .iter()
            .map(|(name, hash)| {
                format!(
                    "{}  {name}\n",
                    hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
                )
            })
            .collect::<String>(),
    )?;
    Ok(hashes)
}
