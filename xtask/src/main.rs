// SPDX-License-Identifier: Apache-2.0
//! Host CLI for SDK/app packages, bootstrap workspaces, images and serial/graphical validation.
mod app;
mod apps;
mod assets;
mod bootstrap;
mod bootstrap_gcc;
mod debug;
mod disk;
mod gpu;
mod graphics;
mod gui;
mod gui_sdk;
mod gui_sdk_build;
mod input;
mod linux;
mod network;
mod packages;
mod purposes;
mod rust;
mod rust_online;
mod rust_public;
mod sdk;
mod smoke;
mod terminal;
mod toolchain;
mod transcript;

use std::{
    boxed::Box,
    env,
    error::Error,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TARGET: &str = "x86_64-unknown-none";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Bootstrap,
    Graphics,
    Input,
    Gui,
    GuiTest,
    Gpu,
    Boot,
    Panic,
    Timeout,
    InvalidOpcode,
    GeneralProtection,
    PageFault,
    DoubleFault,
    StackGuard,
    ReadOnly,
    NoExecute,
    DirectNoExecute,
    Unmapped,
    Oom,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Bootstrap => "bootstrap",
            Self::Graphics => "graphics",
            Self::Input => "input",
            Self::Gui => "gui",
            Self::GuiTest => "gui-test",
            Self::Gpu => "gpu",
            Self::Boot => "boot",
            Self::Panic => "panic",
            Self::Timeout => "timeout",
            Self::InvalidOpcode => "invalid-opcode",
            Self::GeneralProtection => "general-protection",
            Self::PageFault => "page-fault",
            Self::DoubleFault => "double-fault",
            Self::StackGuard => "stack-guard",
            Self::ReadOnly => "read-only",
            Self::NoExecute => "no-execute",
            Self::DirectNoExecute => "direct-no-execute",
            Self::Unmapped => "unmapped",
            Self::Oom => "oom",
        }
    }
    fn features(self) -> Option<&'static str> {
        match self {
            Self::Normal => None,
            Self::Bootstrap => Some("bootstrap-metrics"),
            Self::Graphics => Some("graphics"),
            Self::Input => Some("graphics,input-test"),
            Self::Gui => Some("graphics,gui"),
            Self::GuiTest => Some("graphics,gui,gui-test"),
            Self::Gpu => Some("graphics,gui,gpu-test"),
            Self::Boot => Some("qemu-test"),
            Self::Panic => Some("test-panic"),
            Self::Timeout => Some("test-timeout"),
            Self::InvalidOpcode => Some("test-invalid-opcode"),
            Self::GeneralProtection => Some("test-general-protection"),
            Self::PageFault => Some("test-page-fault"),
            Self::DoubleFault => Some("test-double-fault"),
            Self::StackGuard => Some("test-stack-guard"),
            Self::ReadOnly => Some("test-read-only"),
            Self::NoExecute => Some("test-no-execute"),
            Self::DirectNoExecute => Some("test-direct-no-execute"),
            Self::Unmapped => Some("test-unmapped"),
            Self::Oom => Some("test-oom"),
        }
    }

    fn fault(self) -> Option<smoke::Fault> {
        Some(match self {
            Self::InvalidOpcode => smoke::Fault {
                name: self.name(),
                vector: 6,
                error: "0x0",
                ist: 2,
            },
            Self::GeneralProtection => smoke::Fault {
                name: self.name(),
                vector: 13,
                error: "0xfff8",
                ist: 2,
            },
            Self::PageFault => smoke::Fault {
                name: self.name(),
                vector: 14,
                error: "0x2",
                ist: 2,
            },
            Self::DoubleFault => smoke::Fault {
                name: self.name(),
                vector: 8,
                error: "0x0",
                ist: 1,
            },
            Self::StackGuard
            | Self::ReadOnly
            | Self::NoExecute
            | Self::DirectNoExecute
            | Self::Unmapped => smoke::Fault {
                name: self.name(),
                vector: 14,
                error: match self {
                    Self::ReadOnly => "0x3",
                    Self::NoExecute | Self::DirectNoExecute => "0x11",
                    _ => "0x2",
                },
                ist: 2,
            },
            _ => return None,
        })
    }
}

fn main() {
    if let Err(error) = execute() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn execute() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .canonicalize()?;
    env::set_current_dir(&root)?;
    let args: Vec<_> = env::args().skip(1).collect();
    let action = args.first().map_or("help", String::as_str);
    if !matches!(
        action,
        "smoke"
            | "root-image"
            | "run"
            | "sdk"
            | "app-new"
            | "app-build"
            | "app-pack"
            | "symbolize"
            | "app-image"
            | "app-run"
    ) && args.len() > 1
    {
        return Err("Unexpected arguments; run cargo xtask help".into());
    }
    match action {
        "doctor" => doctor(&root),
        "fetch" => assets::fetch(&root),
        "build" => {
            build(&root, Mode::Normal)?;
            Ok(())
        }
        "apps" => apps::build(&root, false),
        "trace-compat" => linux::trace(&root),
        "test-compat" => smoke(&root, Mode::Boot, 60)?
            .map_err(|e| format!("Compatibility test failed: {e:?}").into()),
        "check-apps" => apps::build(&root, true),
        "check-files" => purposes::check(&root),
        "image" => {
            image(&root, Mode::Normal)?;
            Ok(())
        }
        "graphics-image" => image(&root, Mode::Graphics).map(|_| ()),
        "graphics-test" => graphics::verify(&root),
        "gpu-test" => gpu::verify(&root),
        "gpu-fallback-test" => gpu::verify_fallback(&root),
        "input-test" => input::verify(&root),
        "gui-image" => image(&root, Mode::Gui).map(|_| ()),
        "gui-test" => gui::verify(&root),
        "gui-sdk" => gui_sdk_build::export(&root).map(|_| ()),
        "gui-sdk-image" => gui_sdk_build::image(&root).map(|_| ()),
        "gui-sdk-test" => {
            let iso = gui_sdk_build::image(&root)?;
            gui_sdk::verify(&root, &iso)
        }
        "gui-sdk-replay" => gui_sdk::replay(&root),
        "run" => {
            let mut disk = None;
            let mut network = false;
            let mut toolchain = false;
            let mut rust = false;
            let mut graphics = false;
            let mut gui = false;
            let mut gui_sdk = false;
            let mut options = args.iter().skip(1);
            while let Some(option) = options.next() {
                match option.as_str() {
                    "--network" if !network => network = true,
                    "--toolchain" if !toolchain && !rust => toolchain = true,
                    "--rust" if !rust && !toolchain => rust = true,
                    "--graphics" if !graphics => graphics = true,
                    "--gui" if !gui => gui = true,
                    "--gui-sdk" if !gui_sdk => gui_sdk = true,
                    "--disk" if disk.is_none() => {
                        disk = Some(disk::user_path(
                            &root,
                            options.next().ok_or("Missing disk path")?,
                        )?)
                    }
                    _ => {
                        return Err(
                        "Usage: cargo xtask run [--graphics | --gui | --gui-sdk] [--network] [--toolchain | --rust] [--disk build/disks/NAME.ext2]"
                                .into(),
                        );
                    }
                }
            }
            if gui_sdk && (graphics || gui || toolchain || rust) {
                return Err("Select --gui-sdk without another image profile".into());
            }
            if (graphics || gui) && (toolchain || rust) || (graphics && gui) {
                return Err(
                    "Select one of --graphics or --gui with the normal recovery image".into(),
                );
            }
            if gui_sdk {
                return gui_sdk_build::run(&root, disk.as_deref(), network);
            }
            run(
                &root,
                disk.as_deref(),
                network,
                toolchain,
                rust,
                graphics,
                gui,
            )
        }
        "root-image" => {
            let (linux, name) = match &args[1..] {
                [name] => (false, name),
                [flag, name] if flag == "--linux" => (true, name),
                _ => {
                    return Err(
                        "Usage: cargo xtask root-image [--linux] build/disks/NAME.ext2".into(),
                    );
                }
            };
            apps::build(&root, false)?;
            let path = disk::user_path(&root, name)?;
            disk::create_profile(&root, &path, linux)
        }
        "package-test" => packages::verify(&root),
        "package-v2-test" => packages::verify_v2(&root),
        "sdk" => sdk::dispatch(&root, &args[1..]),
        "app-new" => sdk::app_new(&root, &args[1..]),
        "app-build" => sdk::app_build(&root, &args[1..]),
        "app-pack" => match &args[1..] {
            [manifest, output] => {
                packages::pack_manifest(&root, Path::new(manifest), Path::new(output))
            }
            _ => Err("Usage: cargo xtask app-pack MANIFEST OUTPUT".into()),
        },
        "sdk-test" => sdk::verify(&root),
        "app-image" => app::image(&root, &args[1..]),
        "app-run" => app::run(&root, &args[1..]),
        "app-test" => app::verify(&root),
        "symbolize" => debug::run(&args[1..]),
        "diagnostics-test" => debug::verify(&root),
        "toolchain-build" => toolchain::build(&root),
        "toolchain-image" => toolchain::image(&root).map(|_| ()),
        "toolchain-test" => toolchain::verify(&root),
        "bootstrap-prepare" => bootstrap::prepare(&root).map(|_| ()),
        "bootstrap-test" => bootstrap::verify(&root),
        "bootstrap-gcc-test" => bootstrap_gcc::verify(&root),
        "rust-build" => rust::build(&root),
        "rust-cross-test" => rust::cross(&root),
        "rust-runtime-test" => rust::runtime(&root),
        "rust-image" => rust::image(&root).map(|_| ()),
        "rust-test" => rust::verify(&root),
        "rust-online-test" => rust_online::verify(&root),
        "rust-public-test" => rust_public::verify(&root),
        "network-test" => network::verify(&root),
        "terminal-test" => terminal::verify(&root),
        "persistence-test" => disk::verify(&root),
        "smoke" => {
            let (mode, timeout) = smoke_args(&args[1..])?;
            let failure = smoke(&root, mode, timeout)?;
            failure.map_err(|e| {
                format!(
                    "Smoke failed: {e:?}; see build/logs/{}.serial.log and .qemu.log",
                    mode.name()
                )
                .into()
            })
        }
        "verify" => {
            for (mode, timeout, expected) in [
                (Mode::Boot, 30, Ok(())),
                (Mode::Panic, 30, Err(smoke::Failure::Panic)),
                (Mode::Timeout, 30, Err(smoke::Failure::Timeout)),
                (Mode::InvalidOpcode, 30, Err(smoke::Failure::Exception)),
                (Mode::GeneralProtection, 30, Err(smoke::Failure::Exception)),
                (Mode::PageFault, 30, Err(smoke::Failure::Exception)),
                (Mode::DoubleFault, 30, Err(smoke::Failure::Exception)),
                (Mode::StackGuard, 30, Err(smoke::Failure::Exception)),
                (Mode::ReadOnly, 30, Err(smoke::Failure::Exception)),
                (Mode::NoExecute, 30, Err(smoke::Failure::Exception)),
                (Mode::DirectNoExecute, 30, Err(smoke::Failure::Exception)),
                (Mode::Unmapped, 30, Err(smoke::Failure::Exception)),
                (Mode::Oom, 30, Err(smoke::Failure::Panic)),
            ] {
                let actual = smoke(&root, mode, timeout)?;
                if actual != expected {
                    return Err(
                        format!("{}: expected {expected:?}, got {actual:?}", mode.name()).into(),
                    );
                }
                println!("Verified {}: {actual:?}", mode.name());
            }
            Ok(())
        }
        "test-host" => {
            command(Command::new("cargo").args([
                "test",
                "--locked",
                "-p",
                "xtask",
                "-p",
                "anova-platform",
                "-p",
                "anova-memory",
                "-p",
                "anova-abi",
                "-p",
                "anova-elf",
                "-p",
                "anova-sched",
                "-p",
                "anova-vfs",
                "-p",
                "anova-storage",
                "-p",
                "anova-graphics",
                "-p",
                "anova-input",
                "-p",
                "anova-gui",
                "-p",
                "anova-gui-runtime",
            ]))?;
            command(Command::new("cargo").args([
                "test",
                "--locked",
                "--manifest-path",
                "user/Cargo.toml",
                "-p",
                "anova-rt",
                "--lib",
                "--target-dir",
                "build/user-host-target",
            ]))?;
            command(Command::new("cargo").args([
                "test",
                "--locked",
                "--manifest-path",
                "user/Cargo.toml",
                "-p",
                "shell",
                "--lib",
                "--target-dir",
                "build/user-host-target",
            ]))
        }
        "help" | "--help" | "-h" => {
            println!(
                "AnovaOS host tools\n\n  cargo xtask doctor\n  cargo xtask fetch\n  cargo xtask build\n  cargo xtask image\n  cargo xtask run [--graphics | --gui | --gui-sdk] [--network] [--toolchain | --rust] [--disk build/disks/NAME.ext2]\n  cargo xtask smoke [--case CASE] [--timeout-secs 1..300]\n  cargo xtask verify\n  cargo xtask test-host\n  cargo xtask test-compat\n  cargo xtask trace-compat\n  cargo xtask apps\n  cargo xtask check-apps\n  cargo xtask check-files\n  cargo xtask network-test\n  cargo xtask package-test\n  cargo xtask sdk\n  cargo xtask app-new <c|cpp|rust> PROJECT\n  cargo xtask app-build PROJECT\n  cargo xtask app-pack MANIFEST OUTPUT\n  cargo xtask app-image PACKAGE SHA256\n  cargo xtask app-run PACKAGE SHA256 [--disk build/disks/NAME.ext2]\n  cargo xtask app-test\n  cargo xtask sdk-test\n  cargo xtask package-v2-test\n  cargo xtask diagnostics-test\n  cargo xtask symbolize ELF [--base 0xBIAS] ADDRESS... | --log LOG [--pid PID]\n  cargo xtask toolchain-build\n  cargo xtask toolchain-image\n  cargo xtask toolchain-test\n  cargo xtask bootstrap-prepare\n  cargo xtask bootstrap-test\n  cargo xtask bootstrap-gcc-test\n  cargo xtask rust-build\n  cargo xtask rust-image\n  cargo xtask rust-cross-test\n  cargo xtask rust-runtime-test\n  cargo xtask rust-test\n  cargo xtask rust-online-test\n  cargo xtask rust-public-test\n  cargo xtask terminal-test\n  cargo xtask root-image [--linux] build/disks/NAME.ext2\n  cargo xtask persistence-test\n  cargo xtask graphics-image\n  cargo xtask graphics-test\n  cargo xtask gpu-test\n  cargo xtask gpu-fallback-test\n  cargo xtask input-test\n  cargo xtask gui-image\n  cargo xtask gui-test\n  cargo xtask gui-sdk\n  cargo xtask gui-sdk-image\n  cargo xtask gui-sdk-test\n  cargo xtask gui-sdk-replay\n\nCASE: boot, panic, timeout, invalid-opcode, general-protection, page-fault, double-fault, stack-guard, read-only, no-execute, direct-no-execute, unmapped, oom.\nOutputs stay in build/; run uses disposable OVMF variables and a temporary data disk unless --disk is explicit. Use guest shutdown for persistent images. TCG is mandatory."
            );
            Ok(())
        }
        _ => Err(format!("Unknown command {action}; run cargo xtask help").into()),
    }
}

fn smoke_args(args: &[String]) -> Result<(Mode, u64)> {
    let mut mode = Mode::Boot;
    let mut seconds = 30;
    let mut pairs = args.chunks_exact(2);
    for pair in &mut pairs {
        match pair[0].as_str() {
            "--case" => {
                mode = match pair[1].as_str() {
                    "boot" => Mode::Boot,
                    "panic" => Mode::Panic,
                    "timeout" => Mode::Timeout,
                    "invalid-opcode" => Mode::InvalidOpcode,
                    "general-protection" => Mode::GeneralProtection,
                    "page-fault" => Mode::PageFault,
                    "double-fault" => Mode::DoubleFault,
                    "stack-guard" => Mode::StackGuard,
                    "read-only" => Mode::ReadOnly,
                    "no-execute" => Mode::NoExecute,
                    "direct-no-execute" => Mode::DirectNoExecute,
                    "unmapped" => Mode::Unmapped,
                    "oom" => Mode::Oom,
                    _ => return Err("Unknown smoke case".into()),
                }
            }
            "--timeout-secs" => {
                seconds = pair[1].parse()?;
                if !(1..=300).contains(&seconds) {
                    return Err("Timeout must be 1..300 seconds".into());
                }
            }
            _ => return Err(format!("Unknown smoke option {}", pair[0]).into()),
        }
    }
    if !pairs.remainder().is_empty() {
        return Err("Smoke option is missing its value".into());
    }
    Ok((mode, seconds))
}

fn command(command: &mut Command) -> Result<()> {
    println!("Running {command:?}");
    let status = command.status()?;
    if !status.success() {
        return Err(format!("Command failed ({status}): {command:?}").into());
    }
    Ok(())
}

fn version(name: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(name).args(args).output()?;
    if !output.status.success() {
        return Err(format!("{name} failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn doctor(root: &Path) -> Result<()> {
    let lock = assets::lock(root)?;
    let mut errors = Vec::new();
    for (name, args, expected) in [
        (
            "rustc",
            vec!["--version"],
            Some(format!("rustc {} ", lock.rust)),
        ),
        (
            "cargo",
            vec!["--version"],
            Some(format!("cargo {} ", lock.rust)),
        ),
        ("cargo", vec!["fmt", "--version"], None),
        ("cargo", vec!["clippy", "--version"], None),
        (
            "qemu-system-x86_64",
            vec!["--version"],
            Some(format!("QEMU emulator version {} ", lock.qemu)),
        ),
        ("xorriso", vec!["-version"], None),
        ("curl", vec!["--version"], None),
        ("dpkg-deb", vec!["--version"], None),
        ("gcc", vec!["--version"], None),
        ("g++", vec!["--version"], None),
        ("addr2line", vec!["--version"], None),
        ("strip", vec!["--version"], None),
        ("readelf", vec!["--version"], None),
        ("musl-gcc", vec!["--version"], None),
        ("make", vec!["--version"], None),
        ("openssl", vec!["version"], None),
        ("python3", vec!["--version"], None),
        ("git", vec!["--version"], None),
        ("tar", vec!["--version"], None),
        ("gzip", vec!["--version"], None),
        ("mke2fs", vec!["-V"], None),
        ("e2fsck", vec!["-V"], None),
    ] {
        match version(name, &args) {
            Ok(value) if expected.as_ref().is_none_or(|p| value.starts_with(p)) => {
                println!("OK {name}: {}", value.lines().next().unwrap_or_default())
            }
            Ok(value) => errors.push(format!("{name}: expected {expected:?}, got {value}")),
            Err(error) => errors.push(format!("{name}: {error}")),
        }
    }
    match version("rustc", &["--print", "target-libdir", "--target", TARGET]) {
        Ok(path) if Path::new(&path).is_dir() => println!("OK target: {TARGET}"),
        _ => errors.push(format!("Missing {TARGET}; run rustup target add {TARGET}")),
    }
    match assets::check(root) {
        Ok(()) => println!("OK boot assets; protocol commit {}", lock.protocol_commit),
        Err(error) => errors.push(format!("Boot assets: {error}; run cargo xtask fetch")),
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Doctor found {} problem(s):\n{}",
            errors.len(),
            errors.join("\n")
        )
        .into())
    }
}

fn checked_file(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(format!(
            "Expected a regular file, refusing device or symlink: {}",
            path.display()
        )
        .into());
    }
    Ok(())
}

fn output_path(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => checked_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn directory(root: &Path, name: &str) -> Result<PathBuf> {
    let base = root.join("build");
    for dir in [&base, &base.join(name)] {
        match fs::symlink_metadata(dir) {
            Ok(meta) if !meta.file_type().is_dir() => {
                return Err(format!("Refusing non-directory or symlink: {}", dir.display()).into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(dir)?,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(base.join(name))
}

fn build(root: &Path, mode: Mode) -> Result<PathBuf> {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "build",
        "--locked",
        "-p",
        "anova-kernel",
        "--target",
        TARGET,
    ]);
    cmd.arg("--target-dir").arg(root.join("target"));
    if let Some(features) = mode.features() {
        cmd.args(["--features", features]);
    }
    command(&mut cmd)?;
    let kernel = root.join("target").join(TARGET).join("debug/anova-kernel");
    validate_elf(&fs::read(&kernel)?)?;
    println!("Verified ELF64 x86_64 static kernel: {}", kernel.display());
    Ok(kernel)
}

fn validate_elf(bytes: &[u8]) -> Result<()> {
    if bytes.len() < 64 || &bytes[..7] != b"\x7fELF\x02\x01\x01" {
        return Err("Kernel is not a little-endian ELF64 image".into());
    }
    let u16_at = |i| u16::from_le_bytes(bytes[i..i + 2].try_into().unwrap());
    let u64_at = |i| u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
    if u16_at(16) != 2 || u16_at(18) != 62 || u16_at(54) != 56 {
        return Err("Kernel must be static ET_EXEC for x86_64".into());
    }
    let entry = u64_at(24);
    let offset = usize::try_from(u64_at(32))?;
    let count = usize::from(u16_at(56));
    if count == 0
        || count > 128
        || offset
            .checked_add(count * 56)
            .is_none_or(|end| end > bytes.len())
    {
        return Err("Invalid ELF program header bounds".into());
    }
    let mut executable_entry = false;
    for header in bytes[offset..offset + count * 56].chunks_exact(56) {
        let kind = u32::from_le_bytes(header[..4].try_into().unwrap());
        if kind == 2 || kind == 3 {
            return Err("Kernel contains PT_DYNAMIC or PT_INTERP".into());
        }
        if kind != 1 {
            continue;
        }
        let flags = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let read = |i| u64::from_le_bytes(header[i..i + 8].try_into().unwrap());
        let (file, addr, filesz, memsz) = (read(8), read(16), read(32), read(40));
        let end = addr
            .checked_add(memsz)
            .ok_or("ELF virtual range overflow")?;
        if filesz > memsz
            || file
                .checked_add(filesz)
                .is_none_or(|end| end > bytes.len() as u64)
            || addr < 0xffff_ffff_8000_0000
            || flags & 3 == 3
        {
            return Err("Invalid or writable-executable kernel segment".into());
        }
        executable_entry |= flags & 1 != 0 && (addr..end).contains(&entry);
    }
    if !executable_entry {
        return Err("ELF entry is outside executable segments".into());
    }
    Ok(())
}

struct Temporary(PathBuf);
impl Temporary {
    fn new(root: &Path, prefix: &str) -> Result<Self> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let base = directory(root, "tmp")?;
        let path = base.join(format!("{prefix}-{}-{unique}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn cleanup(&self) -> Result<()> {
        fs::remove_dir_all(&self.0)?;
        Ok(())
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn image(root: &Path, mode: Mode) -> Result<PathBuf> {
    image_archive(root, mode, None)
}

fn image_archive(root: &Path, mode: Mode, archive: Option<&Path>) -> Result<PathBuf> {
    assets::check(root)?;
    apps::build(root, false)?;
    let kernel = build(root, mode)?;
    let stage = Temporary::new(root, "iso")?;
    fs::create_dir_all(stage.0.join("boot/limine"))?;
    fs::create_dir_all(stage.0.join("EFI/BOOT"))?;
    fs::copy(kernel, stage.0.join("boot/kernel.elf"))?;
    fs::create_dir_all(stage.0.join("boot/apps"))?;
    let mut config = fs::read_to_string(root.join("boot/limine.conf"))?;
    for name in apps::NAMES {
        fs::copy(
            root.join("build/apps").join(format!("{name}.elf")),
            stage.0.join(format!("boot/apps/{name}.elf")),
        )?;
        config.push_str(&format!(
            "    module_path: boot():/boot/apps/{name}.elf\n    module_string: {}:{name}\n",
            anova_abi::PERSONALITY
        ));
    }
    fs::copy(
        root.join("build/apps/manifest.toml"),
        stage.0.join("boot/apps/manifest.toml"),
    )?;
    fs::copy(
        archive.map_or_else(
            || root.join("build/apps/initramfs.anovafs"),
            Path::to_path_buf,
        ),
        stage.0.join("boot/initramfs.anovafs"),
    )?;
    config.push_str(
        "    module_path: boot():/boot/initramfs.anovafs\n    module_string: anova.initramfs.v1\n",
    );
    fs::write(stage.0.join("boot/limine/limine.conf"), config)?;
    for (input, output) in [
        ("BOOTX64.EFI", "EFI/BOOT/BOOTX64.EFI"),
        ("limine-uefi-cd.bin", "boot/limine/limine-uefi-cd.bin"),
        ("LIMINE-LICENSE", "boot/limine/LICENSE"),
    ] {
        fs::copy(root.join("build/cache").join(input), stage.0.join(output))?;
    }
    let name = if mode == Mode::Bootstrap {
        mode.name()
    } else if archive.is_some() {
        "devel"
    } else {
        mode.name()
    };
    let iso = directory(root, "images")?.join(format!("anova-{name}.iso"));
    output_path(&iso)?;
    command(
        Command::new("xorriso")
            .args([
                "-as",
                "mkisofs",
                "-R",
                "-J",
                "-V",
                "ANOVAOS",
                "-e",
                "boot/limine/limine-uefi-cd.bin",
                "-no-emul-boot",
                "-efi-boot-part",
                "--efi-boot-image",
                "--protective-msdos-label",
            ])
            .arg(&stage.0)
            .arg("-o")
            .arg(&iso),
    )?;
    println!("Image: {}", iso.display());
    stage.cleanup()?;
    Ok(iso)
}

fn qemu(root: &Path, iso: &Path, vars_dir: &Path, test: bool) -> Result<Command> {
    qemu_disk(root, iso, vars_dir, test, None, false)
}
fn qemu_disk(
    root: &Path,
    iso: &Path,
    vars_dir: &Path,
    test: bool,
    disk: Option<&Path>,
    read_only: bool,
) -> Result<Command> {
    let lock = assets::lock(root)?;
    let version = version("qemu-system-x86_64", &["--version"])?;
    if !version.starts_with(&format!("QEMU emulator version {} ", lock.qemu)) {
        return Err(format!("QEMU {} required, got {version}", lock.qemu).into());
    }
    let (code, vars_template) = assets::firmware(root)?;
    let vars = vars_dir.join("OVMF_VARS.fd");
    fs::copy(vars_template, &vars)?;
    let mut cmd = Command::new("qemu-system-x86_64");
    cmd.args([
        "-machine",
        "q35,accel=tcg",
        "-cpu",
        "qemu64,+rdrand",
        "-m",
        if iso.file_name().is_some_and(|name| name == "anova-rust.iso") {
            "3G"
        } else if iso
            .file_name()
            .is_some_and(|name| name == "anova-devel.iso" || name == "anova-bootstrap.iso")
        {
            "1G"
        } else {
            "256M"
        },
        "-smp",
        "1",
        "-display",
        "none",
        "-monitor",
        "none",
        "-nic",
        "none",
        "-rtc",
        "base=utc,clock=vm",
        "-no-reboot",
        "-boot",
        "d",
    ]);
    cmd.arg("-drive").arg(format!(
        "if=pflash,format=raw,unit=0,readonly=on,file={}",
        code.display()
    ));
    cmd.arg("-drive").arg(format!(
        "if=pflash,format=raw,unit=1,file={}",
        vars.display()
    ));
    cmd.arg("-cdrom").arg(iso);
    disk::attach(root, vars_dir, &mut cmd, disk, read_only)?;
    if test {
        cmd.args(["-device", "isa-debug-exit,iobase=0xf4,iosize=0x04"]);
    }
    Ok(cmd)
}

fn run(
    root: &Path,
    disk: Option<&Path>,
    network: bool,
    devel: bool,
    rust: bool,
    graphics: bool,
    gui: bool,
) -> Result<()> {
    let iso = if rust {
        rust::image(root)?
    } else if devel {
        toolchain::image(root)?
    } else if gui {
        image(root, Mode::Gui)?
    } else if graphics {
        image(root, Mode::Graphics)?
    } else {
        image(root, Mode::Normal)?
    };
    let vars = Temporary::new(root, "run")?;
    let temporary;
    let disk = if (devel || rust) && disk.is_none() {
        temporary = vars.0.join("devel.ext2");
        disk::create_profile(root, &temporary, true)?;
        Some(temporary.as_path())
    } else {
        disk
    };
    let mut cmd = qemu_disk(root, &iso, &vars.0, false, disk, false)?;
    if graphics || gui {
        cmd.args(["-vga", "std", "-display", "gtk"]);
    }
    if network {
        network::attach(&mut cmd);
    }
    let result = command(cmd.args(["-serial", "stdio"]));
    vars.cleanup()?;
    result
}

struct Guest(Child);
impl Drop for Guest {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn smoke(root: &Path, mode: Mode, seconds: u64) -> Result<std::result::Result<(), smoke::Failure>> {
    let iso = image(root, mode)?;
    let vars = Temporary::new(root, "smoke")?;
    let logs = directory(root, "logs")?;
    let serial = logs.join(format!("{}.serial.log", mode.name()));
    let diagnostic = logs.join(format!("{}.qemu.log", mode.name()));
    output_path(&serial)?;
    output_path(&diagnostic)?;
    fs::write(&serial, [])?;
    let log = File::create(&diagnostic)?;
    let mut cmd = qemu(root, &iso, &vars.0, true)?;
    cmd.arg("-serial").arg(format!("file:{}", serial.display()));
    cmd.stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    println!("Running {cmd:?}");
    let mut child = Guest(cmd.spawn()?);
    let start = Instant::now();
    let (code, timed_out) = loop {
        if let Some(status) = child.0.try_wait()? {
            break (status.code(), false);
        }
        if start.elapsed() >= Duration::from_secs(seconds) {
            child.0.kill()?;
            let status = child.0.wait()?;
            break (status.code(), true);
        }
        thread::sleep(Duration::from_millis(20));
    };
    let transcript = fs::read_to_string(&serial)?;
    drop(child);
    vars.cleanup()?;
    if let Some(start) = transcript.find("[BOOT:ENTRY]") {
        print!("{}", &transcript[start..]);
    }
    let result = smoke::assess(&transcript, code, timed_out);
    println!(
        "QEMU case={} exit={code:?} timed_out={timed_out} result={result:?}",
        mode.name()
    );
    let evidence = logs.join(format!("{}.result.txt", mode.name()));
    output_path(&evidence)?;
    fs::write(
        evidence,
        format!(
            "case={}\nqemu_exit={code:?}\ntimed_out={timed_out}\nresult={result:?}\n",
            mode.name()
        ),
    )?;
    // A broken boot must not accidentally satisfy the negative-case gate.
    if mode == Mode::Panic
        && (code != Some(35) || !transcript.contains("Controlled M1 panic injection"))
    {
        return Err("Panic injection did not reach its expected guest failure and exit 35".into());
    }
    if mode == Mode::Timeout && (!timed_out || !transcript.contains("[TEST:HANG]")) {
        return Err("Timeout injection did not reach the controlled guest hang".into());
    }
    if mode == Mode::Oom && !smoke::reached_oom(&transcript, code) {
        return Err("OOM injection did not reach alloc's default panic handler and exit 35".into());
    }
    if let Some(fault) = mode.fault()
        && (timed_out || !smoke::reached_fault(&transcript, code, fault))
    {
        return Err(format!(
            "{} injection did not reach the expected exception, IST, address and exit 37",
            mode.name()
        )
        .into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_truncated_and_wrong_architecture_elf() {
        assert!(validate_elf(b"\x7fELF").is_err());
        let mut bytes = vec![0_u8; 64];
        bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        assert!(validate_elf(&bytes).is_err());
    }

    #[test]
    fn rejects_invalid_watchdog_arguments() {
        for args in [
            ["--timeout-secs", "0"],
            ["--timeout-secs", "301"],
            ["--case", "other"],
        ] {
            assert!(smoke_args(&args.map(String::from)).is_err());
        }
        assert!(smoke_args(&["--case".into()]).is_err());
    }
}
