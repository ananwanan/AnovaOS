// SPDX-License-Identifier: Apache-2.0
//! Build a native-only supervised desktop with unchanged C/C++/Rust GUI reference clients.
//! Host-built clients establish session inputs, independently of guest compiler acceptance.

use super::*;
use sha2::{Digest, Sha256};

pub fn prepare(root: &Path) -> Result<PathBuf> {
    apps::native(root, false)?;
    let work = directory(root, "desktop")?;
    let sysroot = version("rustc", &["--print", "sysroot"])?;
    let linker =
        Path::new(sysroot.trim()).join("lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld");
    checked_file(&linker)?;
    let runtime = work.join("libanova_gui_runtime.rlib");
    output_path(&runtime)?;
    command(
        Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-name=anova_gui_runtime",
                "--crate-type=rlib",
                "--target=x86_64-unknown-linux-gnu",
                "-Cpanic=abort",
                "-Copt-level=1",
                "-Crelocation-model=static",
            ])
            .arg(root.join("crates/gui-runtime/src/lib.rs"))
            .arg("-o")
            .arg(&runtime),
    )?;

    let mut inventory = String::from("profile=M17.1-development\nclient_build=host\n");
    for (name, source) in [("c", "main.c"), ("cpp", "main.cpp"), ("rust", "main.rs")] {
        let object = work.join(format!("{name}.o"));
        let executable = work.join(format!("desktop-{name}.elf"));
        output_path(&object)?;
        output_path(&executable)?;
        if name == "rust" {
            command(
                Command::new("rustc")
                    .args([
                        "--edition=2024",
                        "--crate-type=lib",
                        "--emit=obj",
                        "--target=x86_64-unknown-linux-gnu",
                        "-Cpanic=abort",
                        "-Copt-level=1",
                        "-Crelocation-model=static",
                        "--extern",
                    ])
                    .arg(format!("anova_gui_runtime={}", runtime.display()))
                    .arg(root.join("tests/guest/gui").join(source))
                    .arg("-o")
                    .arg(&object),
            )?;
        } else {
            command(
                Command::new(if name == "cpp" { "g++" } else { "gcc" })
                    .arg(if name == "cpp" {
                        "-std=c++17"
                    } else {
                        "-std=c11"
                    })
                    .args([
                        "-Wall",
                        "-Wextra",
                        "-Werror",
                        "-O1",
                        "-ffreestanding",
                        "-fno-stack-protector",
                        "-fno-builtin",
                        "-fno-pie",
                        "-fno-exceptions",
                        "-fno-unwind-tables",
                        "-fno-asynchronous-unwind-tables",
                    ])
                    .arg("-I")
                    .arg(root.join("sdk/gui/include"))
                    .arg("-I")
                    .arg(root.join("user/c"))
                    .arg("-c")
                    .arg(root.join("tests/guest/gui").join(source))
                    .arg("-o")
                    .arg(&object),
            )?;
        }
        command(
            Command::new("rustc")
                .args([
                    "--edition=2024",
                    "--target=x86_64-unknown-linux-gnu",
                    "-Cpanic=abort",
                    "-Copt-level=1",
                    "-Crelocation-model=static",
                    "-Clinker-flavor=ld.lld",
                    "-Clink-arg=--undefined=_start",
                    "-Clink-arg=--gc-sections",
                    "-Clink-arg=--build-id=none",
                    "--extern",
                ])
                .arg(format!("anova_gui_runtime={}", runtime.display()))
                .arg(format!("-Clinker={}", linker.display()))
                .arg(format!(
                    "-Clink-arg=-T{}",
                    root.join("sdk/native-v1.ld").display()
                ))
                .arg(format!("-Clink-arg={}", object.display()))
                .arg(root.join("user/gui-link.rs"))
                .arg("-o")
                .arg(&executable),
        )?;
        let bytes = fs::read(&executable)?;
        anova_elf::parse(&bytes).map_err(|error| format!("Desktop client {name}: {error:?}"))?;
        inventory.push_str(&format!(
            "desktop-{name}_sha256={:x}\n",
            Sha256::digest(bytes)
        ));
    }
    let mut archive = b"ANOVAFS1".to_vec();
    // See the checked archive encoder: 12-byte header includes the entry count at bytes 8..12.
    archive.extend_from_slice(&0_u32.to_le_bytes());
    for path in ["/bin", "/etc", "/tmp", "/dev", "/persist"] {
        toolchain::append(&mut archive, 2, path, &[]);
    }
    for name in apps::NAMES {
        toolchain::append(
            &mut archive,
            4,
            &format!("/bin/{name}"),
            &fs::read(root.join("build/apps").join(format!("{name}.elf")))?,
        );
    }
    for name in [
        "ls", "cat", "echo", "pwd", "mkdir", "rm", "true", "false", "sync", "shutdown",
    ] {
        toolchain::append(&mut archive, 3, &format!("/bin/{name}"), b"shell");
    }
    let desktop = root.join("build/user-target/x86_64-unknown-linux-gnu/release/desktop");
    let bytes = fs::read(&desktop)?;
    anova_elf::parse(&bytes).map_err(|error| format!("Desktop supervisor: {error:?}"))?;
    toolchain::append(&mut archive, 4, "/bin/desktop", &bytes);
    inventory.push_str(&format!("desktop_sha256={:x}\n", Sha256::digest(bytes)));
    for name in ["c", "cpp", "rust"] {
        toolchain::append(
            &mut archive,
            4,
            &format!("/bin/desktop-{name}"),
            &fs::read(work.join(format!("desktop-{name}.elf")))?,
        );
    }
    toolchain::append(
        &mut archive,
        1,
        "/etc/motd",
        b"AnovaOS supervised desktop development profile\n",
    );
    anova_vfs::Vfs::new(&archive).map_err(|error| format!("Desktop archive: {error}"))?;
    let path = work.join("initramfs.anovafs");
    output_path(&path)?;
    fs::write(&path, archive)?;
    let manifest = work.join("inputs.txt");
    output_path(&manifest)?;
    fs::write(manifest, inventory)?;
    Ok(path)
}

pub fn image(root: &Path) -> Result<PathBuf> {
    let archive = prepare(root)?;
    assets::check(root)?;
    let kernel = build(root, Mode::Desktop)?;
    image_contents(root, &kernel, Some(&archive), "desktop")
}

pub fn run(root: &Path, disk: Option<&Path>) -> Result<()> {
    let iso = image(root)?;
    let vars = Temporary::new(root, "desktop-run")?;
    let mut cmd = qemu_disk(root, &iso, &vars.0, false, disk, false)?;
    cmd.args([
        "-m", "3G", "-vga", "std", "-display", "gtk", "-serial", "stdio",
    ]);
    let result = command(&mut cmd);
    vars.cleanup()?;
    result
}
