// SPDX-License-Identifier: Apache-2.0
//! Admit the native PTY UI, pinned interactive Linux ash and independently built test inputs.
//! Optional GCC packing reuses the M11 checked source-build recipe and immutable ABI policy.

use super::*;
use sha2::{Digest, Sha256};

pub(super) fn append(root: &Path, archive: &mut Vec<u8>) -> Result<()> {
    let work = directory(root, "terminal")?;
    let manifest = work.join("manifest.toml");
    output_path(&manifest)?;
    fs::write(
        &manifest,
        "# Generated terminal shell and native image identities.\n",
    )?;
    linux::build_terminal(root, &work)?;
    let native =
        fs::read(root.join("build/user-target/x86_64-unknown-linux-gnu/release/terminal"))?;
    anova_elf::parse(&native).map_err(|e| format!("Native PTY terminal: {e:?}"))?;
    toolchain::append(archive, 4, "/bin/terminal", &native);
    let shell = fs::read(work.join("busybox.elf"))?;
    toolchain::append(archive, 5, "/linux/busybox", &shell);
    for name in ["sh", "echo", "cat", "ls", "true", "false", "test"] {
        toolchain::append(archive, 3, &format!("/linux/{name}"), b"busybox");
    }
    toolchain::append(
        archive,
        1,
        "/etc/busybox.LICENSE",
        &fs::read(work.join("busybox.LICENSE"))?,
    );
    toolchain::append(
        archive,
        1,
        "/etc/terminal-hello.c",
        &fs::read(root.join("tests/guest/terminal/hello.c"))?,
    );
    let fixture = work.join("fixture.elf");
    output_path(&fixture)?;
    command(
        Command::new("musl-gcc")
            .args([
                "-static",
                "-no-pie",
                "-O2",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fcf-protection=none",
                "-Wl,--build-id=none",
            ])
            .arg(root.join("tests/guest/terminal/fixture.c"))
            .arg("-o")
            .arg(&fixture),
    )?;
    let fixture = fs::read(fixture)?;
    anova_elf::parse_linux(&fixture).map_err(|e| format!("PTY Linux fixture: {e:?}"))?;
    toolchain::append(archive, 5, "/linux/pty-fixture", &fixture);
    fs::write(
        work.join("inputs.txt"),
        format!(
            "native_personality={}\nlinux_personality={}\nnative_sha256={:x}\nshell_sha256={:x}\nfixture_sha256={:x}\n",
            anova_abi::PERSONALITY,
            anova_abi::linux::PERSONALITY,
            Sha256::digest(&native),
            Sha256::digest(&shell),
            Sha256::digest(&fixture)
        ),
    )?;
    Ok(())
}

pub fn image(root: &Path, compiler: bool) -> Result<PathBuf> {
    let path = desktop::prepare(root)?;
    let path = if compiler {
        let mut bytes = fs::read(&path)?;
        toolchain::append_profile(root, &mut bytes)?;
        anova_vfs::Vfs::new(&bytes).map_err(|e| format!("Terminal compiler archive: {e}"))?;
        let path = directory(root, "terminal")?.join("initramfs.anovafs");
        output_path(&path)?;
        fs::write(&path, bytes)?;
        path
    } else {
        path
    };
    assets::check(root)?;
    let kernel = build(root, Mode::Desktop)?;
    image_contents(
        root,
        &kernel,
        Some(&path),
        if compiler {
            "terminal-toolchain"
        } else {
            "desktop"
        },
    )
}
