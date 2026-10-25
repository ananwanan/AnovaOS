// SPDX-License-Identifier: Apache-2.0
//! Verify actual fenced virgl rendering/readback, independently checked scanout and CPU recovery.
//! Keep standard VGA as the recovery display and record the renderer reported by this QEMU device.
use super::*;
use anova_graphics::Surface;
use graphics::{Qmp, parse_ppm, read_ppm};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{io::Write, process::ChildStdin};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;
const TILE_X: u32 = 16;
const TILE_Y: u32 = 16;
const TILE_SIZE: u32 = 64;
const TILE_RGB: u32 = 0x00ff00;
const REFERENCE_CHECKSUM: u64 = 0x9ed0_6ea9_b3ba_5f91;
const DEADLINE: Duration = Duration::from_secs(90);

#[derive(Clone, Copy)]
enum Profile {
    Virgl,
    TwoDimensional,
    Absent,
}

impl Profile {
    fn name(self) -> &'static str {
        match self {
            Self::Virgl => "gpu-virgl",
            Self::TwoDimensional => "gpu-two-dimensional",
            Self::Absent => "gpu-absent",
        }
    }

    fn accelerated(self) -> bool {
        matches!(self, Self::Virgl)
    }
}

pub fn verify(root: &Path) -> Result<()> {
    verify_profiles(root, false)
}

/// Exercise real device rejection and CPU recovery on hosts without an OpenGL display/GPU.
pub fn verify_fallback(root: &Path) -> Result<()> {
    verify_profiles(root, true)
}

fn verify_profiles(root: &Path, fallback_only: bool) -> Result<()> {
    let artifacts = directory(root, "gpu")?;
    let result = artifacts.join(if fallback_only {
        "gpu-fallback-test.result.txt"
    } else {
        "gpu-test.result.txt"
    });
    let profiles: &[Profile] = if fallback_only {
        &[Profile::TwoDimensional, Profile::Absent]
    } else {
        &[Profile::Virgl, Profile::TwoDimensional, Profile::Absent]
    };
    output_path(&result)?;
    fs::write(&result, "result=Running\n")?;
    for profile in profiles {
        let path = artifacts.join(format!("{}.result.txt", profile.name()));
        output_path(&path)?;
        fs::write(path, "result=Pending\n")?;
        for phase in ["render", "timeout", "recovered"] {
            let path = artifacts.join(format!("{}-{phase}.ppm", profile.name()));
            output_path(&path)?;
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
    }
    let outcome = (|| -> Result<()> {
        let iso = image(root, Mode::Gpu)?;
        for profile in profiles {
            session(root, &iso, &artifacts, *profile)?;
        }
        Ok(())
    })();
    fs::write(
        &result,
        match &outcome {
            Ok(()) => format!(
                "result=Ok\nscenarios={}\nhardware_profile={}\n",
                profiles
                    .iter()
                    .map(|profile| profile.name())
                    .collect::<Vec<_>>()
                    .join(","),
                !fallback_only,
            ),
            Err(error) => format!("result=Err\nerror={error}\n"),
        },
    )?;
    outcome?;
    if fallback_only {
        println!(
            "Verified absent/2D-only GPU rejection, independent CPU fallback scanout and COM1 recovery"
        );
    } else {
        println!(
            "Verified actual virgl rendering and fenced readback, hardware renderer identity, independent scanout, timeout/reset CPU fallback, absent/2D-only devices and COM1 recovery"
        );
    }
    Ok(())
}

fn session(root: &Path, iso: &Path, artifacts: &Path, profile: Profile) -> Result<()> {
    let name = profile.name();
    let serial = artifacts.join(format!("{name}.serial.log"));
    let diagnostic = artifacts.join(format!("{name}.qemu.log"));
    let qmp_log = artifacts.join(format!("{name}.qmp.log"));
    let result = artifacts.join(format!("{name}.result.txt"));
    for path in [&serial, &diagnostic, &qmp_log, &result] {
        output_path(path)?;
    }
    fs::write(&result, "result=Running\n")?;
    let temporary = Temporary::new(root, "gpu")?;
    let socket = temporary.0.join("qmp.sock");
    let mut command = qemu(root, iso, &temporary.0, false)?;
    command.args(["-vga", "std"]);
    match profile {
        Profile::Virgl => {
            let display = env::var("ANOVA_GPU_DISPLAY").unwrap_or_else(|_| "gtk,gl=on".into());
            command.args(["-display", &display, "-device", "virtio-gpu-gl-pci"]);
            // WSL's D3D12 Gallium driver uses /dev/dxg rather than a Linux DRM render node.
            // Select it only for this child and preserve an explicit caller selection.
            if Path::new("/dev/dxg").exists() && env::var_os("GALLIUM_DRIVER").is_none() {
                command.env("GALLIUM_DRIVER", "d3d12");
            }
            if Path::new("/dev/dxg").exists()
                && env::var_os("WAYLAND_DISPLAY").is_some()
                && env::var_os("GDK_BACKEND").is_none()
            {
                command.env("GDK_BACKEND", "wayland");
            }
        }
        Profile::TwoDimensional => {
            command.args(["-device", "virtio-gpu-pci"]);
        }
        Profile::Absent => {}
    }
    command
        .arg("-qmp")
        .arg(format!("unix:{},server=on,wait=off", socket.display()))
        .args(["-serial", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(File::create(&diagnostic)?);
    let mut evidence = format!(
        "scenario={name}\nqemu={command:?}\nprofile=q35,accel=tcg cpu=qemu64,+rdrand memory=256M smp=1 primary=standard-vga secondary={}\niso_sha256={:x}\nkernel_sha256={:x}\ncompositor_sha256={:x}\n",
        match profile {
            Profile::Virgl => "virtio-gpu-gl-pci",
            Profile::TwoDimensional => "virtio-gpu-pci",
            Profile::Absent => "none",
        },
        Sha256::digest(fs::read(iso)?),
        Sha256::digest(fs::read(
            root.join("target").join(TARGET).join("debug/anova-kernel")
        )?),
        Sha256::digest(fs::read(root.join("build/apps/compositor.elf"))?),
    );
    evidence.push_str(&format!(
        "qemu_version={}\n",
        version("qemu-system-x86_64", &["--version"])?
            .lines()
            .next()
            .unwrap_or("unknown"),
    ));
    for package in ["libvirglrenderer1", "libegl-mesa0", "libgl1-mesa-dri"] {
        let package_version = Command::new("dpkg-query")
            .args(["-W", "-f=${Version}", package])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "unavailable".into());
        evidence.push_str(&format!("host_package_{package}={package_version}\n"));
    }
    println!("Running {name}: {command:?}");
    let outcome = (|| -> Result<()> {
        let mut guest = Guest(command.spawn()?);
        let output = transcript::Transcript::capture(
            guest.0.stdout.take().ok_or("Missing UART output pipe")?,
            File::create(&serial)?,
        )?;
        let uart = guest.0.stdin.take().ok_or("Missing UART input pipe")?;
        let start = Instant::now();
        let qmp = Qmp::connect(&socket, &qmp_log, start)?;
        let mut driver = Driver {
            guest,
            output,
            uart,
            qmp,
            start,
            cursor: 0,
        };
        driver.wait("anova> ")?;
        let boot = driver.output.text();
        if !boot.contains("[GRAPHICS:PRESENT]")
            || !boot.contains("[GRAPHICS:RECLAIM] heap_restored=true")
            || boot.contains("[GRAPHICS:FALLBACK]")
        {
            return Err("GPU profile did not retain its checked CPU recovery display".into());
        }
        thread::sleep(Duration::from_millis(80));
        let phase_start = driver.output.text().len();
        driver.serial(b"compositor gpu\r")?;
        driver.wait("[GPU:USER-DONE]")?;
        driver.wait("anova> ")?;
        let transcript = driver.output.text();
        let phase = &transcript[phase_start..];
        check_completion(phase)?;
        check_render(phase, profile)?;
        if profile.accelerated() {
            for renderer in renderers(phase)? {
                evidence.push_str(&format!("renderer={renderer}\n"));
            }
            evidence.push_str("hardware_acceleration=true\nfenced_gpu_readback=true\n");
        } else {
            evidence.push_str("hardware_acceleration=false\nunsupported_cpu_fallback=true\n");
        }
        driver.screenshot(artifacts, name, "render", &mut evidence)?;
        if profile.accelerated() {
            let phase_start = driver.output.text().len();
            driver.serial(b"compositor gpu-fail\r")?;
            driver.wait("[GPU:USER-DONE]")?;
            driver.wait("anova> ")?;
            let transcript = driver.output.text();
            check_completion(&transcript[phase_start..])?;
            check_timeout(&transcript[phase_start..])?;
            driver.screenshot(artifacts, name, "timeout", &mut evidence)?;
            evidence.push_str(
                "actual_timeout=true\nreset_acknowledged=true\npost_failure_cpu_fallback=true\n",
            );
            let phase_start = driver.output.text().len();
            driver.serial(b"compositor gpu\r")?;
            driver.wait("[GPU:USER-DONE]")?;
            driver.wait("anova> ")?;
            let transcript = driver.output.text();
            check_completion(&transcript[phase_start..])?;
            check_render(&transcript[phase_start..], profile)?;
            driver.screenshot(artifacts, name, "recovered", &mut evidence)?;
            evidence.push_str("gpu_reinitialized=true\npost_reset_hardware_readback=true\n");
        }
        driver.serial(b"echo M14-GPU-SERIAL-RECOVERY\r")?;
        driver.wait("\r\nM14-GPU-SERIAL-RECOVERY\r\n")?;
        driver.wait("anova> ")?;
        driver.serial(b"shutdown\r")?;
        driver.shutdown()?;
        evidence
            .push_str("serial_recovery=true\nacpi_shutdown=true\nqemu_exit=0\ntimed_out=false\n");
        drop(driver);
        temporary.cleanup()?;
        Ok(())
    })();
    match &outcome {
        Ok(()) => evidence.push_str("result=Ok\n"),
        Err(error) => evidence.push_str(&format!("result=Err\nerror={error}\n")),
    }
    fs::write(&result, evidence)?;
    outcome.map_err(|error| format!("{name}: {error}; see {}", result.display()).into())
}

struct Driver {
    guest: Guest,
    output: transcript::Transcript,
    uart: ChildStdin,
    qmp: Qmp,
    start: Instant,
    cursor: usize,
}

impl Driver {
    fn poll(&mut self) -> Result<Option<std::process::ExitStatus>> {
        self.output.poll()?;
        let exit = self.guest.0.try_wait()?;
        if exit.is_some() {
            self.output.finish()?;
        }
        if self.start.elapsed() >= DEADLINE {
            return Err("GPU guest exceeded the 90-second watchdog".into());
        }
        let text = self.output.text();
        if [
            "[PANIC]",
            "[EXCEPTION]",
            "[INIT:FAIL]",
            "[SHELL:FAIL]",
            "[POWER:FAIL]",
            "[GUI:FAIL]",
        ]
        .iter()
        .any(|marker| text.contains(marker))
        {
            return Err("GPU guest reported a fatal boot, runtime or shutdown failure".into());
        }
        Ok(exit)
    }

    fn wait(&mut self, marker: &str) -> Result<()> {
        loop {
            let exit = self.poll()?;
            if let Some(at) = self.output.find_from(self.cursor, marker) {
                self.cursor = at + marker.len();
                return Ok(());
            }
            if let Some(exit) = exit {
                return Err(format!("GPU guest exited {exit} before {marker}").into());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn serial(&mut self, bytes: &[u8]) -> Result<()> {
        self.uart.write_all(bytes)?;
        self.uart.flush()?;
        Ok(())
    }

    fn screenshot(
        &mut self,
        artifacts: &Path,
        name: &str,
        phase: &str,
        evidence: &mut String,
    ) -> Result<()> {
        let screenshot = artifacts.join(format!("{name}-{phase}.ppm"));
        output_path(&screenshot)?;
        self.qmp.command(
            "screendump",
            json!({"filename": screenshot, "format": "ppm"}),
        )?;
        let bytes = read_ppm(&screenshot)?;
        verify_pixels(&bytes)?;
        evidence.push_str(&format!(
            "{phase}_screenshot={}\n{phase}_screenshot_sha256={:x}\n{phase}_compared_pixels={}\n",
            screenshot.display(),
            Sha256::digest(&bytes),
            WIDTH * HEIGHT,
        ));
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        loop {
            if let Some(exit) = self.poll()? {
                if exit.code() != Some(0)
                    || !self
                        .output
                        .text()
                        .contains("[POWER:OFF] synced=true clean=true dma_reset=true")
                {
                    return Err(format!("GPU guest did not cleanly shut down: {exit}").into());
                }
                return Ok(());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

fn check_render(text: &str, profile: Profile) -> Result<()> {
    if !text.contains("[GPU:USER-BOUNDS] authorization=true dimensions=true format=true pointer=true unchanged=true") {
        return Err("GPU compositor did not prove authorization, argument and pointer rejection".into());
    }
    if profile.accelerated() {
        if !text.contains("[GPU:USER] backend=virgl pixels=4096 clears=4")
            || !text.contains("[GPU:READY] virtio=modern virgl=true queue=8 irq=poll")
            || !text.contains("[GPU:RESET] acknowledged=true dma_released=true")
            || !text.contains("[GPU:RECLAIM] frames_restored=true heap_restored=true")
            || !text.contains("[GPU:REJECT] invalid_capset=true response=0x1205")
            || text.contains("[GPU:USER] backend=cpu")
        {
            return Err(
                "Virgl profile did not prove real fenced render/readback and reset reclamation"
                    .into(),
            );
        }
        if !text.contains("[GPU:USER-ODD] width=7 height=5 rgb=001280fe pixels=35 canary=true")
            || !text.lines().any(|line| {
                line.contains("[GPU:READBACK] width=7 height=5 pixels=35 rgb=001280fe")
                    && line.contains("verified=true")
                    && line.contains("fence=")
            })
        {
            return Err(
                "GPU rendering did not prove odd dimensions, exact UNORM color and output canaries"
                    .into(),
            );
        }
        for rgb in ["00ff0000", "000000ff", "00ffffff", "0000ff00"] {
            if !text.lines().any(|line| {
                line.contains("[GPU:READBACK]")
                    && line.contains("width=64 height=64 pixels=4096")
                    && line.contains("verified=true")
                    && line.contains("fence=")
                    && line.contains(&format!("rgb={rgb}"))
            }) {
                return Err(format!("Missing actual fenced GPU readback for color {rgb}").into());
            }
        }
        renderers(text)?;
    } else if !text.contains("[GPU:USER] backend=cpu pixels=4096 clears=4")
        || text.contains("[GPU:READBACK]")
        || text.contains("[GPU:USER] backend=virgl")
    {
        return Err("Unavailable/2D-only GPU did not use the actual CPU fallback".into());
    }
    Ok(())
}

fn integer_field(line: &str, key: &str) -> Option<u64> {
    line.split_ascii_whitespace()
        .find_map(|field| field.strip_prefix(key))?
        .parse()
        .ok()
}

/// Ordinary shell commands report the child exit rather than a shell-status line on success.
/// Bind the exit to the compositor admitted in this phase, after its final output and before prompt.
fn check_completion(text: &str) -> Result<()> {
    let mut owner = None;
    let mut done = false;
    let mut exited = false;
    let mut prompted = false;
    for line in text.lines() {
        if line.starts_with("[GUI:SERVER-ACQUIRE] ") {
            let pid = integer_field(line, "pid=")
                .filter(|pid| *pid != 0)
                .ok_or("GPU compositor acquisition did not report its process identity")?;
            if owner.replace(pid).is_some() {
                return Err("GPU phase unexpectedly acquired a second compositor".into());
            }
        } else if line == "[GPU:USER-DONE]" {
            if owner.is_none() || done {
                return Err("GPU completion did not follow one compositor acquisition".into());
            }
            done = true;
        } else if line.starts_with("[USER:FAULT] ") && integer_field(line, "pid=") == owner {
            return Err("GPU compositor faulted before returning to its shell".into());
        } else if line.starts_with("[USER:EXIT] ") && integer_field(line, "pid=") == owner {
            if !done || exited || integer_field(line, "status=") != Some(0) {
                return Err(
                    "GPU compositor did not exit successfully after its completed output".into(),
                );
            }
            exited = true;
        } else if line.starts_with("anova> ") && done {
            if !exited {
                return Err("GPU shell prompt preceded its compositor's successful exit".into());
            }
            prompted = true;
        }
    }
    if !prompted {
        return Err(
            "GPU phase lacks a completed compositor exit followed by its shell prompt".into(),
        );
    }
    Ok(())
}

fn renderers(text: &str) -> Result<Vec<&str>> {
    let mut names = Vec::new();
    for line in text.lines().filter(|line| line.contains("[GPU:RENDERER]")) {
        let name = line
            .split_once("[GPU:RENDERER] name=")
            .map(|(_, name)| name)
            .filter(|name| !name.is_empty() && name.len() < 64)
            .ok_or("Missing bounded device-reported virgl renderer name")?;
        if !hardware_renderer(name) {
            return Err(format!("Virgl renderer is software or unrecognized: {name}; select a hardware OpenGL backend (ANOVA_GPU_DISPLAY overrides gtk,gl=on)").into());
        }
        if !names.contains(&name) {
            names.push(name);
        }
    }
    if names.is_empty() {
        return Err("No exact-device renderer evidence was reported".into());
    }
    Ok(names)
}

fn hardware_renderer(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if [
        "llvmpipe",
        "softpipe",
        "swrast",
        "software",
        "lavapipe",
        "warp",
        "swiftshader",
        "basic render",
    ]
    .iter()
    .any(|token| lower.contains(token))
    {
        return false;
    }
    [
        "nvidia", "geforce", "radeon", "radeonsi", "intel", "nouveau", "apple", "adreno", "mali",
        "powervr",
    ]
    .iter()
    .any(|token| lower.contains(token))
}

fn check_timeout(text: &str) -> Result<()> {
    let mut cursor = 0;
    for marker in [
        "[GPU:INJECT] lost_doorbell=true published=true",
        "[GPU:TIMEOUT] watchdog=pm-timer deadline_ms=2000 reset_required=true",
        "[GPU:RESET] acknowledged=true dma_released=true",
        "[GPU:RECLAIM] frames_restored=true heap_restored=true",
        "[GPU:USER] backend=cpu pixels=4096",
        "[GPU:USER-DONE]",
    ] {
        let offset = text[cursor..]
            .find(marker)
            .ok_or_else(|| format!("Missing ordered GPU failure/recovery marker: {marker}"))?;
        cursor += offset + marker.len();
    }
    if text.contains("[GPU:USER] backend=virgl") {
        return Err("Injected GPU timeout was reported as successful GPU rendering".into());
    }
    let timed_out = text.lines().any(|line| {
        line.starts_with("[GPU:TIMEOUT] ")
            && line
                .split_ascii_whitespace()
                .any(|field| field == "reason=pm-timer")
            && integer_field(line, "elapsed_pm_ticks=").is_some_and(|ticks| ticks >= 7_159_090)
    });
    if !timed_out {
        return Err(
            "GPU timeout did not measure the full calibrated two-second PM-timer deadline".into(),
        );
    }
    Ok(())
}

fn verify_pixels(bytes: &[u8]) -> Result<()> {
    let ppm = parse_ppm(bytes)?;
    if ppm.width != WIDTH || ppm.height != HEIGHT {
        return Err(format!(
            "Expected {WIDTH}x{HEIGHT} recovery display, got {}x{}",
            ppm.width, ppm.height
        )
        .into());
    }
    let mut pixels = vec![0; (WIDTH * HEIGHT) as usize];
    let mut reference = Surface::new(WIDTH, HEIGHT, &mut pixels)
        .map_err(|error| format!("Cannot create GPU software reference: {error:?}"))?;
    reference.draw_test_pattern();
    if reference.checksum() != REFERENCE_CHECKSUM {
        return Err("GPU recovery background differs from the pinned CPU pattern".into());
    }
    for (index, rgb) in ppm.rgb.chunks_exact(3).enumerate() {
        let x = index as u32 % WIDTH;
        let y = index as u32 / WIDTH;
        let expected = if (TILE_X..TILE_X + TILE_SIZE).contains(&x)
            && (TILE_Y..TILE_Y + TILE_SIZE).contains(&y)
        {
            TILE_RGB
        } else {
            reference.pixels()[index]
        };
        let actual = u32::from(rgb[0]) << 16 | u32::from(rgb[1]) << 8 | u32::from(rgb[2]);
        if actual != expected {
            return Err(format!(
                "GPU recovery scanout pixel ({x},{y}) is {actual:#08x}, expected {expected:#08x}"
            )
            .into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn software_renderer_never_counts_as_hardware() {
        for renderer in [
            "llvmpipe (LLVM 20)",
            "LLVMPIPE (LLVM 20)",
            "Microsoft Basic Render Driver",
            "D3D12 (Microsoft WARP)",
            "softpipe",
            "NVIDIA software rasterizer",
            "unknown",
        ] {
            assert!(
                !hardware_renderer(renderer),
                "Accepted software or unknown renderer: {renderer}"
            );
        }
        assert!(hardware_renderer("D3D12 (NVIDIA GeForce RTX 4060 Ti)"));
        assert!(hardware_renderer("Mesa Intel(R) UHD Graphics"));
        assert!(hardware_renderer("AMD Radeon RX 6600"));
        assert!(renderers("[GPU:RENDERER] name=LLVMPIPE (LLVM 20)").is_err());
        assert!(renderers("[GPU:RENDERER] name=").is_err());
        assert!(renderers("unrelated host probe renderer=NVIDIA").is_err());
        assert_eq!(
            renderers("[GPU:RENDERER] name=D3D12 (NVIDIA GeForce RTX 4060 Ti)").unwrap(),
            ["D3D12 (NVIDIA GeForce RTX 4060 Ti)"]
        );
    }

    #[test]
    fn timeout_requires_ordered_actual_reset_and_cpu_fallback() {
        let complete = "[GPU:INJECT] lost_doorbell=true published=true\n[GPU:TIMEOUT] watchdog=pm-timer deadline_ms=2000 reset_required=true reason=pm-timer elapsed_pm_ticks=7159090\n[GPU:RESET] acknowledged=true dma_released=true\n[GPU:RECLAIM] frames_restored=true heap_restored=true\n[GPU:USER] backend=cpu pixels=4096\n[GPU:USER-DONE]\n";
        check_timeout(complete).unwrap();
        assert!(
            check_timeout(&complete.replace("acknowledged=true", "acknowledged=false")).is_err()
        );
        assert!(check_timeout(&complete.replace("backend=cpu", "backend=virgl")).is_err());
        assert!(check_timeout(&complete.replace("[GPU:TIMEOUT]", "[GPU:NOT-TIMED-OUT]")).is_err());
        assert!(
            check_timeout(&complete.replace("reason=pm-timer", "reason=iteration-limit")).is_err()
        );
        assert!(check_timeout(&complete.replace("ticks=7159090", "ticks=7159089")).is_err());
    }

    #[test]
    fn ordinary_command_completion_requires_the_admitted_childs_successful_exit() {
        let complete = "compositor gpu\r\n[GUI:SERVER-ACQUIRE] pid=3 session=1\r\n[GPU:USER] backend=virgl pixels=4096 clears=4\r\n[GUI:SERVER-RELEASE] pid=3 session=1 clients=0\r\n[GPU:USER-DONE]\r\n[USER:EXIT] pid=3 status=0\r\nanova> ";
        check_completion(complete).unwrap();
        assert!(check_completion(&complete.replace("pid=3 status=0", "pid=30 status=0")).is_err());
        assert!(check_completion(&complete.replace("status=0", "status=1")).is_err());
        assert!(check_completion(&complete.replace("[USER:EXIT]", "[USER:FAULT]")).is_err());
        assert!(check_completion(&complete.replace("[GPU:USER-DONE]\r\n", "")).is_err());
        assert!(
            check_completion(&complete.replace(
                "[USER:EXIT] pid=3 status=0\r\nanova> ",
                "anova> \r\n[USER:EXIT] pid=3 status=0\r\n"
            ))
            .is_err()
        );
    }
}
