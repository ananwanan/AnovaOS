// SPDX-License-Identifier: Apache-2.0
//! Build and pack a source-built offline GCC profile, then verify actual guest compilation and reboot.
use super::*;
use sha2::{Digest, Sha256};

pub fn build(root: &Path) -> Result<()> {
    command(Command::new("sh").arg(root.join("ports/gcc/build.sh")))
}

pub(super) fn append(bytes: &mut Vec<u8>, kind: u8, path: &str, data: &[u8]) {
    bytes.extend_from_slice(&[kind, 0]);
    bytes.extend_from_slice(&(path.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&(data.len() as u64).to_le_bytes());
    bytes.extend_from_slice(path.as_bytes());
    bytes.extend_from_slice(data);
    let count = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) + 1;
    bytes[8..12].copy_from_slice(&count.to_le_bytes());
}
pub(super) fn pack(root: &Path) -> Result<PathBuf> {
    let mut bytes = fs::read(root.join("build/apps/initramfs.anovafs"))?;
    append_profile(root, &mut bytes)?;
    // Decode the entire archive now, before handing any of it to a kernel.
    let mut archive =
        anova_vfs::archive::Archive::new(&bytes).map_err(|e| format!("Toolchain archive: {e}"))?;
    while archive
        .entry()
        .map_err(|e| format!("Toolchain entry: {e}"))?
        .is_some()
    {}
    let work = root.join("build/toolchain");
    let path = work.join("initramfs.anovafs");
    fs::write(&path, &bytes)?;
    fs::write(
        work.join("archive.sha256"),
        format!("{:x}\n", Sha256::digest(&bytes)),
    )?;
    Ok(path)
}

/// Append the same verified GCC/sysroot admission to a native desktop archive.
pub(super) fn append_profile(root: &Path, bytes: &mut Vec<u8>) -> Result<()> {
    command(
        Command::new("python3")
            .arg(root.join("ports/gcc/build.py"))
            .arg("--verify"),
    )?;
    command(Command::new("python3").arg(root.join("ports/gcc/package.py")))?;
    let work = root.join("build/toolchain");
    let mut paths = vec![work.join("package/tc")];
    while let Some(path) = paths.pop() {
        let guest = format!("/{}", path.strip_prefix(work.join("package"))?.display());
        if path.is_dir() {
            append(bytes, 2, &guest, &[]);
            let mut children = fs::read_dir(&path)?
                .map(|p| p.map(|p| p.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            children.sort();
            paths.extend(children.into_iter().rev());
        } else {
            checked_file(&path)?;
            let data = fs::read(&path)?;
            let kind = if guest.starts_with("/tc/bin/")
                || guest.starts_with("/tc/libexec/")
                || guest.starts_with("/tc/x86_64-linux-musl/bin/")
            {
                anova_elf::parse_linux(&data)
                    .map_err(|e| format!("Unsupported toolchain executable {guest}: {e:?}"))?;
                5
            } else {
                1
            };
            append(bytes, kind, &guest, &data);
        }
    }
    // GCC's standard sysroot layout uses /usr/include and /usr/lib below /tc.
    append(bytes, 3, "/tc/usr", b".");
    let wrapper = work.join("cc.elf");
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
            .arg(root.join("user/linux/cc.c"))
            .arg("-o")
            .arg(&wrapper),
    )?;
    append(bytes, 5, "/bin/gcc", &fs::read(wrapper)?);
    let probe = work.join("runtime.elf");
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
            .arg(root.join("tests/guest/m11/runtime.c"))
            .arg("-o")
            .arg(&probe),
    )?;
    append(bytes, 5, "/bin/m11-runtime", &fs::read(probe)?);

    for name in ["g++", "make", "cc"] {
        append(bytes, 3, &format!("/bin/{name}"), b"gcc");
    }
    Ok(())
}
pub fn image(root: &Path) -> Result<PathBuf> {
    apps::build(root, false)?;
    let archive = pack(root)?;
    image_archive(root, Mode::Normal, Some(&archive))
}

pub fn verify(root: &Path) -> Result<()> {
    let iso = image(root)?;
    let path = directory(root, "disks")?.join(format!(
        "toolchain-{}.ext2",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    disk::create_profile(root, &path, true)?;
    let mut steps: Vec<(String, Vec<u8>)> = Vec::new();
    let mut send =
        |marker: &str, command: &str| steps.push((marker.into(), command.as_bytes().to_vec()));
    send("anova> ", "m11-runtime\r");
    send(
        "[M11:RUNTIME] vfork exec-failure snapshots umask modes limits",
        "cd /persist\r",
    );
    send("anova> ", "gcc --version\r");
    for name in [
        "main.c",
        "helper.c",
        "main.cpp",
        "Makefile",
        "bad.c",
        "badlink.c",
    ] {
        send("anova> ", &format!("edit {name}\r"));
        let source = fs::read_to_string(root.join("tests/guest/m11").join(name))?;
        if source.lines().any(|line| line.len() > 250) {
            return Err("M11 editor fixture line too long".into());
        }
        send(
            "AnovaOS line editor:",
            &format!("a\r{}.\rw\r", source.replace('\n', "\r")),
        );
        send("[EDIT:SAVED] fsync=ok", "q\r");
    }
    send("anova> ", "gcc main.c helper.c -o hello-c\r");
    send("anova> ", "status\r");
    send("[SHELL:STATUS] code=0", "./hello-c guest\r");
    send(
        "[M11:C] stdio malloc files argv multi-source",
        "cat result.txt\r",
    );
    send(
        "guest-C:42",
        "g++ -std=c++17 -pthread main.cpp -o hello-cpp\r",
    );
    send("anova> ", "status\r");
    send("[SHELL:STATUS] code=0", "./hello-cpp\r");
    send("[M11:CPP] string vector RAII exceptions thread", "make\r");
    send("anova> ", "status\r");
    send("[SHELL:STATUS] code=0", "./made guest\r");
    send("[M11:C] stdio malloc files argv multi-source", "make\r");
    send("anova> ", "status\r");
    send("[SHELL:STATUS] code=0", "edit helper.c\r");
    send(
        "AnovaOS line editor:",
        "c 3\rint helper(void) { return 43; }\rw\r",
    );
    send("[EDIT:SAVED] fsync=ok", "q\r");
    send("anova> ", "make\r");
    send("anova> ", "status\r");
    send("[SHELL:STATUS] code=0", "./made guest\r");
    send(
        "[M11:C] stdio malloc files argv multi-source",
        "cat result.txt\r",
    );
    send("guest-C:43", "gcc bad.c -o rejected-compile\r");
    send("anova> ", "status\r");
    send("[SHELL:STATUS] code=1", "gcc badlink.c -o rejected-link\r");
    send("anova> ", "status\r");
    send("[SHELL:STATUS] code=1", "./hello-c guest\r");
    send(
        "[M11:C] stdio malloc files argv multi-source",
        "m11-runtime temps\r",
    );
    send("[M11:TEMP] clean", "shutdown\r");
    let exchanges: Vec<_> = steps
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_slice()))
        .collect();
    disk::session_timeout(
        root,
        &iso,
        &path,
        "toolchain-build",
        false,
        None,
        &exchanges,
        true,
        900,
    )?;
    let serial = fs::read_to_string(root.join("build/logs/toolchain-build.serial.log"))?;
    if !serial.contains("Nothing to be done for 'all'")
        || serial.matches("gcc -c main.c -o main.o\r\n").count() != 1
        || serial.matches("gcc -c helper.c -o helper.o\r\n").count() != 2
    {
        return Err("Make did not perform the expected no-op and incremental rebuild".into());
    }
    if !serial.contains("bad.c:3:11: error: expected declaration specifiers")
        || !serial.contains("undefined reference to `missing_m11_symbol'")
        || !serial.contains("collect2: error: ld returned 1 exit status")
    {
        return Err(
            "Negative builds did not reach the intended compiler/linker diagnostics".into(),
        );
    }
    let before = inspect(root, &path)?;
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(&path))?;
    disk::session_timeout(
        root,
        &iso,
        &path,
        "toolchain-reboot",
        false,
        None,
        &[
            ("anova> ", b"cd /persist\r"),
            ("anova> ", b"./hello-c guest\r"),
            (
                "[M11:C] stdio malloc files argv multi-source",
                b"./hello-cpp\r",
            ),
            (
                "[M11:CPP] string vector RAII exceptions thread",
                b"shutdown\r",
            ),
        ],
        true,
        300,
    )?;
    let after = inspect(root, &path)?;
    if before != after {
        return Err("M11 binaries or source changed across reboot".into());
    }
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(&path))?;
    fs::write(
        root.join("build/logs/toolchain.result.txt"),
        format!(
            "result=Ok\nimage={}\ncompiler=guest\nreboot=true\n",
            path.display()
        ),
    )?;
    Ok(())
}

fn inspect(root: &Path, path: &Path) -> Result<Vec<(String, String)>> {
    use anova_storage::ext2::Ext2;
    let mut disk =
        Ext2::mount(Box::new(disk::Image::open(path)?)).map_err(|e| format!("M11 disk: {e}"))?;
    let entries = disk.entries(2).map_err(|e| format!("M11 directory: {e}"))?;
    let mut hashes = Vec::new();
    for name in [
        "main.c",
        "helper.c",
        "main.cpp",
        "Makefile",
        "bad.c",
        "badlink.c",
        "hello-c",
        "hello-cpp",
        "made",
    ] {
        let entry = entries
            .iter()
            .find(|e| &e.name[..e.length] == name.as_bytes())
            .ok_or_else(|| format!("Missing guest output {name}"))?;
        let inode = disk
            .inode(entry.inode)
            .map_err(|e| format!("M11 inode: {e}"))?;
        let mut bytes = vec![0; inode.size];
        let count = disk
            .read(entry.inode, 0, &mut bytes)
            .map_err(|e| format!("M11 read: {e}"))?;
        if count != bytes.len() {
            return Err("Short M11 disk read".into());
        }
        if name.starts_with("hello-") || name == "made" {
            anova_elf::parse_linux(&bytes)
                .map_err(|e| format!("Invalid guest-built ELF: {e:?}"))?;
            if !inode.executable {
                return Err("Guest-built ELF lacks executable mode".into());
            }
        } else {
            let mut expected = fs::read(root.join("tests/guest/m11").join(name))?;
            if name == "helper.c" {
                expected = String::from_utf8(expected)?
                    .replace("return 42", "return 43")
                    .into_bytes();
            }
            if expected != bytes {
                return Err(format!("Guest editor bytes differ: {name}").into());
            }
        }
        hashes.push((name.to_string(), format!("{:x}", Sha256::digest(&bytes))));
    }
    if entries
        .iter()
        .any(|e| matches!(&e.name[..e.length], b"rejected-compile" | b"rejected-link"))
    {
        return Err("A rejected build left an executable candidate".into());
    }
    disk.shutdown()
        .map_err(|e| format!("M11 inspection shutdown: {e}"))?;
    fs::write(
        root.join("build/logs/toolchain-files.sha256"),
        hashes
            .iter()
            .map(|(n, h)| format!("{h}  {n}\n"))
            .collect::<String>(),
    )?;
    Ok(hashes)
}
