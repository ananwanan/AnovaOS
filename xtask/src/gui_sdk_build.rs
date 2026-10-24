// SPDX-License-Identifier: Apache-2.0
//! Package a source-distributed GUI SDK with existing guest-hosted compilers and native scanout.
//! GUI applications and their Rust runtime are built by the guest, never substituted by host ELFs.
use super::*;
use sha2::{Digest, Sha256};

const RELEASE: &str = "anova-gui-sdk-1.0.0";

pub fn export(root: &Path) -> Result<PathBuf> {
    command(
        Command::new("python3")
            .arg(root.join("sdk/gui/generate.py"))
            .arg("--check"),
    )?;
    let work = Temporary::new(root, "gui-sdk-export")?;
    let destination = work.0.join("candidate");
    sdk::safe_directory(&destination)?;
    for (source, relative) in [
        ("sdk/gui/include/anova_gui.h", "include/anova_gui.h"),
        ("sdk/gui/include/anova_gui.hpp", "include/anova_gui.hpp"),
        ("sdk/gui/generate.py", "generate.py"),
        ("user/c/anova.h", "include/anova.h"),
        ("user/c/crt.S", "c/crt.S"),
        ("sdk/native-v1.ld", "native-v1.ld"),
        ("user/gui-link.rs", "link.rs"),
        ("user/rt/src/memory.rs", "rt/src/memory.rs"),
        ("crates/gui/src/wire.rs", "crates/gui/src/wire.rs"),
        ("sdk/gui/README.md", "README.md"),
        ("user/linux/gui-build.c", "gui-build.c"),
        ("tests/guest/gui/contract.h", "include/contract.h"),
        ("tests/guest/gui/main.c", "templates/main.c"),
        ("tests/guest/gui/main.cpp", "templates/main.cpp"),
        ("tests/guest/gui/main.rs", "templates/main.rs"),
        ("tests/guest/gui/note.rs", "templates/note.rs"),
        ("tests/guest/gui/terminal.rs", "templates/terminal.rs"),
        ("tests/guest/gui/contract.c", "templates/contract.c"),
        ("tests/guest/gui/fault.c", "templates/fault.c"),
        ("LICENSE", "LICENSE"),
        ("NOTICE", "NOTICE"),
    ] {
        sdk::copy_file(&root.join(source), &destination.join(relative))?;
    }
    sdk::copy_tree(
        &root.join("crates/gui-runtime/src"),
        &destination.join("crates/gui-runtime/src"),
    )?;
    fs::write(
        destination.join("crates/gui-runtime/Cargo.toml"),
        "# SPDX-License-Identifier: Apache-2.0\n# Independently usable source GUI SDK crate with the same versioned C ABI and safe Rust wrapper.\n[package]\nname = \"anova-gui-runtime\"\nversion = \"1.0.0\"\nedition = \"2024\"\nlicense = \"Apache-2.0\"\n\n[lib]\ncrate-type = [\"rlib\"]\n\n[workspace]\n",
    )?;
    let metadata = "# SPDX-License-Identifier: Apache-2.0\n# Independent GUI library and native execution contract; no Linux GUI compatibility.\nversion = \"1.0.0\"\nabi = \"anova.gui.v1\"\nabi_version = 65536\nexecution_abi = \"anova.native.v1\"\narchitecture = \"x86_64\"\nlinkage = \"static-rust-rlib\"\nthreading = \"single-ui-thread\"\n";
    let path = destination.join("sdk.toml");
    output_path(&path)?;
    fs::write(path, metadata)?;
    let mut files = Vec::new();
    sdk::collect_files(&destination, &destination, &mut files)?;
    files.sort();
    let mut inventory = String::new();
    for relative in files {
        inventory.push_str(&format!(
            "{:x}  {}\n",
            Sha256::digest(fs::read(destination.join(&relative))?),
            relative.display()
        ));
    }
    let path = destination.join("SHA256SUMS");
    output_path(&path)?;
    fs::write(path, inventory)?;
    let published = directory(root, "sdk")?.join(RELEASE);
    sdk::safe_directory(published.parent().ok_or("SDK export has no parent")?)?;
    let backup = work.0.join("previous");
    if let Err(error) = sdk::publish_export(&destination, &published, &backup) {
        if backup.exists() {
            core::mem::forget(work);
        }
        return Err(error);
    }
    println!("Exported GUI source SDK: {}", published.display());
    Ok(published)
}

pub fn image(root: &Path) -> Result<PathBuf> {
    let sdk = export(root)?;
    // The Rust image already contains the accepted GCC/G++ profile and locked musl Rust tools.
    rust::image_profile(root, true)?;
    let base = root.join("build/rust/initramfs.anovafs");
    let mut bytes = fs::read(base)?;
    toolchain::append(&mut bytes, 2, "/gui", &[]);
    let mut pending = vec![sdk.clone()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let source = entry.path();
            let guest = format!("/gui/{}", source.strip_prefix(&sdk)?.display());
            if entry.file_type()?.is_dir() {
                toolchain::append(&mut bytes, 2, &guest, &[]);
                pending.push(source);
            } else {
                checked_file(&source)?;
                toolchain::append(&mut bytes, 1, &guest, &fs::read(source)?);
            }
        }
    }
    let work = directory(root, "gui-sdk")?;
    let launcher = work.join("gui-build.elf");
    output_path(&launcher)?;
    command(
        Command::new("musl-gcc")
            .args([
                "-O1",
                "-static",
                "-no-pie",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-Wl,--build-id=none",
            ])
            .arg(root.join("user/linux/gui-build.c"))
            .arg("-o")
            .arg(&launcher),
    )?;
    let launcher = fs::read(launcher)?;
    anova_elf::parse_linux(&launcher).map_err(|e| format!("GUI guest build tool: {e:?}"))?;
    toolchain::append(&mut bytes, 5, "/bin/gui-build", &launcher);
    let archive = work.join("initramfs.anovafs");
    output_path(&archive)?;
    fs::write(&archive, bytes)?;
    let iso = image_archive(root, Mode::Gui, Some(&archive))?;
    let output = root.join("build/images/anova-gui-sdk.iso");
    output_path(&output)?;
    fs::rename(iso, &output)?;
    Ok(output)
}

pub fn run(root: &Path, disk: Option<&Path>, network: bool) -> Result<()> {
    let iso = image(root)?;
    let vars = Temporary::new(root, "gui-sdk-run")?;
    let temporary;
    let disk = if let Some(disk) = disk {
        disk
    } else {
        temporary = vars.0.join("gui-native.ext2");
        disk::create_profile(root, &temporary, false)?;
        &temporary
    };
    let mut cmd = qemu_disk(root, &iso, &vars.0, false, Some(disk), false)?;
    cmd.args([
        "-m", "3G", "-vga", "std", "-display", "gtk", "-serial", "stdio",
    ]);
    if network {
        network::attach(&mut cmd);
    }
    let result = command(&mut cmd);
    vars.cleanup()?;
    result
}
