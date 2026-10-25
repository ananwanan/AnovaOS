// SPDX-License-Identifier: Apache-2.0
//! Symbolize bounded guest fault records against retained ELF debug data and verify real crashes.
//! Host addr2line resolves source locations; no debugger or guest-side DWARF unwinder is implied.
use super::*;
use sha2::{Digest, Sha256};

const MAX_ADDRESSES: usize = 256;

fn address(value: &str) -> Result<u64> {
    let hex = value
        .strip_prefix("0x")
        .ok_or("Addresses must use the 0x hexadecimal prefix")?;
    if hex.is_empty() || hex.len() > 16 {
        return Err("Invalid address width".into());
    }
    Ok(u64::from_str_radix(hex, 16)?)
}

/// Parse the checked lookup address, already adjusted for call-site return PCs by the kernel.
pub fn addresses(log: &str) -> Result<Vec<u64>> {
    trace_addresses(log, None)
}

fn trace_addresses(log: &str, selected_pid: Option<u64>) -> Result<Vec<u64>> {
    let mut result = Vec::new();
    let mut first_pid = selected_pid;
    for line in log.lines() {
        if !line.starts_with("[USER:BACKTRACE] ") {
            continue;
        }
        let pid = line
            .split_ascii_whitespace()
            .find_map(|field| field.strip_prefix("pid="))
            .ok_or("Backtrace record has no process ID")?
            .parse::<u64>()?;
        if selected_pid.is_some_and(|selected| selected != pid) {
            continue;
        }
        if first_pid.is_some_and(|first| first != pid) {
            return Err(
                "Log contains multiple process backtraces; select the matching ELF with --pid PID"
                    .into(),
            );
        }
        first_pid = Some(pid);
        let value = line
            .split_ascii_whitespace()
            .find_map(|field| field.strip_prefix("lookup="))
            .ok_or("Backtrace record has no lookup address")?;
        if result.len() == MAX_ADDRESSES {
            return Err("Too many backtrace addresses; select one crash from the log".into());
        }
        result.push(address(value)?);
    }
    if result.is_empty() {
        return Err("No user backtrace records found".into());
    }
    Ok(result)
}

/// Preserve the exact ELF digest beside output so users can identify the matching unstripped build.
pub fn symbolize(elf: &Path, addresses: &[u64]) -> Result<String> {
    checked_file(elf)?;
    if addresses.is_empty() || addresses.len() > MAX_ADDRESSES {
        return Err("Symbolization needs 1..256 addresses".into());
    }
    let bytes = fs::read(elf)?;
    if bytes.len() < 64 || &bytes[..7] != b"\x7fELF\x02\x01\x01" || bytes[18..20] != [62, 0] {
        return Err("Symbolization requires an x86_64 little-endian ELF64 file".into());
    }
    let sections = Command::new("readelf").args(["-SW"]).arg(elf).output()?;
    let names = String::from_utf8_lossy(&sections.stdout);
    if !sections.status.success() || !names.contains(".symtab ") || !names.contains(".debug_info ")
    {
        return Err(
            "ELF lacks retained .symtab/.debug_info; use the matching unstripped debug build"
                .into(),
        );
    }
    let mut command = Command::new("addr2line");
    command.args(["-a", "-f", "-C", "-e"]).arg(elf);
    for value in addresses {
        command.arg(format!("{value:#x}"));
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "addr2line failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(format!(
        "ELF={}\nSHA256={:x}\n{}",
        elf.display(),
        Sha256::digest(&bytes),
        String::from_utf8(output.stdout)?
    ))
}

/// CLI: symbolize ELF [--base 0xLOAD_BIAS] ADDRESS... or ELF --log LOG [--pid PID].
pub fn run(args: &[String]) -> Result<()> {
    let elf = args.first().ok_or(
        "Usage: cargo xtask symbolize ELF [--base 0xBIAS] ADDRESS... | --log LOG [--pid PID]",
    )?;
    let mut result = Vec::new();
    let mut logs = Vec::new();
    let mut base = 0;
    let mut pid = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--log" => {
                index += 1;
                let path = args.get(index).ok_or("--log requires a log path")?;
                checked_file(Path::new(path))?;
                logs.push(path);
            }
            "--pid" => {
                index += 1;
                pid = Some(
                    args.get(index)
                        .ok_or("--pid requires a decimal process ID")?
                        .parse::<u64>()?,
                );
            }
            "--base" => {
                index += 1;
                base = address(
                    args.get(index)
                        .ok_or("--base requires a hexadecimal load bias")?,
                )?;
            }
            value => result.push(address(value)?),
        }
        index += 1;
    }
    for path in logs {
        result.extend(trace_addresses(&fs::read_to_string(path)?, pid)?);
    }
    for value in &mut result {
        *value = value
            .checked_sub(base)
            .ok_or("Address is below the ELF load bias")?;
    }
    print!("{}", symbolize(Path::new(elf), &result)?);
    Ok(())
}

pub fn verify(root: &Path) -> Result<()> {
    let logs = directory(root, "logs")?;
    let result_path = logs.join("diagnostics.result.txt");
    output_path(&result_path)?;
    fs::write(&result_path, "result=Running\n")?;
    apps::build(root, false)?;
    let work = directory(root, "diagnostics")?;
    let mut archive = fs::read(root.join("build/apps/initramfs.anovafs"))?;
    for (name, invalid) in [("m13-crash", false), ("m13-invalid-frame", true)] {
        let elf = work.join(format!("{name}.elf"));
        output_path(&elf)?;
        let mut compiler = Command::new("gcc");
        compiler.args([
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-O1",
            "-g",
            "-fno-omit-frame-pointer",
            "-fno-optimize-sibling-calls",
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
        ]);
        if invalid {
            compiler.arg("-DM13_INVALID_FRAME");
        }
        command(
            compiler
                .arg(format!(
                    "-Wl,-T,{}",
                    root.join("sdk/native-v1.ld").display()
                ))
                .arg(root.join("user/c/crt.S"))
                .arg(root.join("tests/guest/m13/crash.c"))
                .arg("-o")
                .arg(&elf),
        )?;
        let bytes = fs::read(&elf)?;
        anova_elf::parse(&bytes).map_err(|error| format!("Crash fixture ELF: {error:?}"))?;
        toolchain::append(&mut archive, 4, &format!("/bin/{name}"), &bytes);
    }
    let archive_path = work.join("initramfs.anovafs");
    output_path(&archive_path)?;
    fs::write(&archive_path, archive)?;
    let iso = image_archive(root, Mode::Normal, Some(&archive_path))?;
    let disk_path = directory(root, "disks")?.join(format!(
        "diagnostics-{}.ext2",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    disk::create_profile(root, &disk_path, true)?;
    disk::session_timeout(
        root,
        &iso,
        &disk_path,
        "diagnostics-guest",
        false,
        None,
        &[
            ("anova> ", b"m13-crash\r"),
            ("[USER:FAULT]", b"status\r"),
            ("[SHELL:STATUS] code=134", b"m13-invalid-frame\r"),
            ("reason=invalid-frame", b"status\r"),
            ("[SHELL:STATUS] code=134", b"echo M13-HEALTHY-AFTER-FAULT\r"),
            ("M13-HEALTHY-AFTER-FAULT\r\n", b"shutdown\r"),
        ],
        true,
        120,
    )?;
    let log = fs::read_to_string(logs.join("diagnostics-guest.serial.log"))?;
    let first_trace = log
        .split("[USER:BACKTRACE-END]")
        .next()
        .ok_or("Missing guest backtrace")?;
    let pcs = addresses(first_trace)?;
    if pcs.len() < 4 {
        return Err("Guest crash lacks four real stack frames".into());
    }
    let symbols = symbolize(&work.join("m13-crash.elf"), &pcs)?;
    for expected in [
        "m13_crash_leaf",
        "m13_crash_middle",
        "m13_crash_entry",
        "main",
        "tests/guest/m13/crash.c:",
    ] {
        if !symbols.contains(expected) {
            return Err(format!("Guest crash symbolization missing {expected}").into());
        }
    }
    if !log.contains("frames=1 reason=invalid-frame")
        || !log.contains("[USER:RESOURCES]")
        || log.matches("[USER:FAULT]").count() != 2
        || !log
            .lines()
            .any(|line| line.trim_end() == "M13-HEALTHY-AFTER-FAULT")
    {
        return Err(
            "Missing corrupt-frame rejection, resource snapshot or healthy post-fault command"
                .into(),
        );
    }
    let path = logs.join("diagnostics.symbols.log");
    output_path(&path)?;
    fs::write(&path, &symbols)?;
    fs::write(
        &result_path,
        format!(
            "result=Ok\nfaults=2\nsymbols=true\nsource_lines=true\nframes={}\ncorrupt_frame=rejected\nshell_survived=true\nacpi_shutdown=true\n",
            pcs.len()
        ),
    )?;
    println!(
        "{symbols}[M13:DIAGNOSTICS] fault=true symbols=true source_lines=true frames={} corrupt_frame=rejected shell_survived=true\nEvidence: {}",
        pcs.len(),
        path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_only_checked_backtrace_lookup_fields() {
        let log = "[USER:FAULT] rip=0x123\n[USER:BACKTRACE] pid=4 index=0 pc=0x123 lookup=0x123\n[USER:BACKTRACE] pid=4 index=1 pc=0x456 lookup=0x455\n[USER:BACKTRACE-END] frames=2\n";
        assert_eq!(addresses(log).unwrap(), [0x123, 0x455]);
        assert!(addresses("[USER:BACKTRACE] pid=4 pc=0x123").is_err());
        assert!(addresses("[USER:BACKTRACE] pid=4 lookup=0x10000000000000000").is_err());
        assert!(addresses("[USER:BACKTRACE] pid=4 lookup=0xzz").is_err());
        assert!(addresses("[USER:FAULT] rip=0x123").is_err());
    }

    #[test]
    fn bound_log_addresses() {
        let log = "[USER:BACKTRACE] pid=1 lookup=0x123\n".repeat(MAX_ADDRESSES + 1);
        assert!(addresses(&log).is_err());
        assert!(address("123").is_err());
        assert!(address("0x").is_err());
    }

    #[test]
    fn select_one_process_from_a_multi_image_log() {
        let log = "[USER:BACKTRACE] pid=1 lookup=0x123\n[USER:BACKTRACE] pid=2 lookup=0x456\n";
        assert!(addresses(log).is_err());
        assert_eq!(trace_addresses(log, Some(1)).unwrap(), [0x123]);
        assert_eq!(trace_addresses(log, Some(2)).unwrap(), [0x456]);
        assert!(trace_addresses(log, Some(3)).is_err());
    }
}
