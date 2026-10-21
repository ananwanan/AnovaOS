// SPDX-License-Identifier: Apache-2.0
//! Build standalone native Rust/C ELF files with user-only target/linker flags.
//! Validate each output and record its explicit ABI identity and SHA-256 manifest.
use super::*;
use sha2::{Digest, Sha256};

pub const NAMES: [&str; 4] = ["hello-rust", "hello-c", "probe", "process-test"];
const USER_TARGET: &str = "x86_64-unknown-linux-gnu";

pub fn build(root: &Path, clippy: bool) -> Result<()> {
    let output = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()?;
    if !output.status.success() {
        return Err("Cannot locate Rust sysroot".into());
    }
    let sysroot = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    let linker = sysroot
        .join("lib/rustlib")
        .join(USER_TARGET)
        .join("bin/rust-lld");
    checked_file(&linker)?;
    let flags = [
        "-C".into(),
        format!("linker={}", linker.display()),
        "-C".into(),
        "linker-flavor=ld.lld".into(),
        "-C".into(),
        format!(
            "link-arg=-T{}",
            root.join("linker/user-x86_64.ld").display()
        ),
        "-C".into(),
        "link-arg=--undefined=_start".into(),
        "-C".into(),
        "link-arg=--build-id=none".into(),
        "-C".into(),
        "relocation-model=static".into(),
        "-C".into(),
        "code-model=small".into(),
        "-C".into(),
        "no-redzone=no".into(),
        "-C".into(),
        "target-cpu=x86-64".into(),
        "-C".into(),
        "target-feature=-avx,-avx2".into(),
    ];
    let target = directory(root, "user-target")?;
    let mut rust = Command::new("cargo");
    rust.args([
        if clippy { "clippy" } else { "build" },
        "--locked",
        "--manifest-path",
    ])
    .arg(root.join("user/Cargo.toml"))
    .args(["--target", USER_TARGET, "--release", "--target-dir"])
    .arg(&target)
    .env("CARGO_ENCODED_RUSTFLAGS", flags.join("\x1f"));
    if clippy {
        rust.args(["--", "-D", "warnings"]);
    }
    command(&mut rust)?;
    if clippy {
        return Ok(());
    }
    let apps = directory(root, "apps")?;
    for name in ["hello-rust", "probe", "process-test"] {
        let path = apps.join(format!("{name}.elf"));
        output_path(&path)?;
        fs::copy(target.join(USER_TARGET).join("release").join(name), path)?;
    }
    let c = apps.join("hello-c.elf");
    output_path(&c)?;
    command(
        Command::new("gcc")
            .args([
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-O2",
                "-g",
                "-ffreestanding",
                "-fno-stack-protector",
                "-fno-pie",
                "-fno-builtin",
                "-static",
                "-nostdlib",
                "-m64",
                "-march=x86-64",
                "-mno-avx",
                "-mno-avx2",
                "-Wl,--build-id=none",
                "-Wl,-z,noexecstack",
            ])
            .arg(format!(
                "-Wl,-T,{}",
                root.join("linker/user-x86_64.ld").display()
            ))
            .arg(root.join("user/c/crt.S"))
            .arg(root.join("user/c/hello.c"))
            .arg("-o")
            .arg(&c),
    )?;
    let mut manifest =
        String::from("# Generated native image inventory: ABI identity and exact ELF content.\n");
    for name in NAMES {
        let file = apps.join(format!("{name}.elf"));
        let bytes = fs::read(&file)?;
        anova_elf::parse(&bytes).map_err(|e| format!("Invalid user ELF {name}: {e:?}"))?;
        manifest.push_str(&format!("\n[[apps]]\nname = \"{name}\"\nabi = \"{}\"\nfile = \"{name}.elf\"\nsha256 = \"{:x}\"\n",
            anova_abi::PERSONALITY,Sha256::digest(&bytes)));
    }
    let path = apps.join("manifest.toml");
    output_path(&path)?;
    fs::write(path, manifest)?;
    println!("Verified native Rust/C ELF outputs in {}", apps.display());
    Ok(())
}
