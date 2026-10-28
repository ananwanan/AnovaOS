// SPDX-License-Identifier: Apache-2.0
//! Arm externally configured QEMU blkdebug faults after the real desktop disk is mounted.
//! Count actual flush events before one completed virtio write/flush error; never mock a guest syscall.

use super::{Result, desktop_test::Driver};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
pub(super) enum Fault {
    /// Fail the first write after this many successful device flushes, including zero.
    WriteAfterFlush(u32),
    /// Fail the numbered device flush; numbering begins at one after arming.
    Flush(u32),
}

impl Fault {
    pub(super) fn error_marker(self) -> &'static str {
        match self {
            Self::WriteAfterFlush(_) => "[BLK:ERROR] operation=1 status=1",
            Self::Flush(_) => "[BLK:ERROR] operation=4 status=1",
        }
    }
}

fn options(name: &str, fault: Fault) -> Result<Value> {
    if name.is_empty()
        || name.len() > 26
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err("Invalid editor blkdebug node name".into());
    }
    let (state, event, iotype) = match fault {
        Fault::WriteAfterFlush(n) => (
            n.checked_add(1).ok_or("Fault count overflow")?,
            "write_aio",
            "write",
        ),
        Fault::Flush(0) => return Err("Device flush numbering begins at one".into()),
        Fault::Flush(n) => (n, "flush_to_os", "flush"),
    };
    if state > 128 {
        return Err("Editor fault counter exceeds its declared bound".into());
    }
    let transitions: Vec<_> = (1..state)
        .map(|n| json!({"event":"flush_to_os","state":n,"new_state":n+1}))
        .collect();
    let mut configuration = json!({
        "driver":"blkdebug", "node-name":format!("m17-{name}"), "image":"m17-file",
        "inject-error":[{"event":event,"state":state,"iotype":iotype,"errno":5,"once":true,"immediately":true}],
    });
    // QEMU flattens nonempty option arrays. An empty array remains an unsupported
    // bare driver option, so omit it entirely when the initial state is sufficient.
    if !transitions.is_empty() {
        configuration["set-state"] = json!(transitions);
    }
    Ok(configuration)
}

pub(super) fn arm(d: &mut Driver, name: &str, fault: Fault) -> Result<()> {
    // QEMU creates a fresh state machine per case; blockdev-reopen drains the old graph.
    // Driver::boot_fault names only ordinary build-directory image nodes, outside the guest.
    d.qmp.command("blockdev-add", options(name, fault)?)?;
    d.qmp.command(
        "blockdev-reopen",
        json!({"options":[{"driver":"raw","node-name":"m17-root","file":format!("m17-{name}")}]}),
    )
}

pub(super) fn disarm(d: &mut Driver) -> Result<()> {
    // Removing the external failure does not clear the filesystem's fail-closed state.
    // Recovery of that state still requires the existing journal remount/reboot boundary.
    d.qmp.command(
        "blockdev-reopen",
        json!({"options":[{"driver":"raw","node-name":"m17-root","file":"m17-file"}]}),
    )
}

fn hmp(d: &mut Driver, command: &str) -> Result<()> {
    let reply = d.qmp.command_value(
        "human-monitor-command",
        json!({"command-line":format!("qemu-io anovadisk \"{command}\"")}),
    )?;
    if reply.as_str() != Some("") {
        return Err(format!("Editor block observer command failed: {reply}").into());
    }
    Ok(())
}

fn save_budget(start: Instant) -> Result<()> {
    if start.elapsed() >= Duration::from_secs(5) {
        Err("Graphical save and device fault control exceeded five seconds".into())
    } else {
        Ok(())
    }
}

/// Use real QEMU breakpoints to make no-write file/directory flush errors observable.
/// QEMU otherwise skips a blkdebug flush callback when no write changed its generation.
/// At only the selected suspended flush, rewrite the disk's exact existing tail sector
/// through the existing BlockBackend, then let the original virtio FLUSH complete with EIO.
/// The bytes do not change; no guest syscall result or filesystem state is mocked.
pub(super) fn save(
    d: &mut Driver,
    work: &Path,
    disk: &Path,
    name: &str,
    fault: Fault,
) -> Result<()> {
    arm(d, name, fault)?;
    let Fault::Flush(number) = fault else {
        return d.key("f5");
    };
    let tag = |number| format!("m17-{name}-{number}");
    hmp(d, &format!("break flush_to_os {}", tag(1)))?;
    let start = Instant::now();
    let uart_start = d.output.position();
    d.key("f5")?;
    for number_at in 1..=number {
        save_budget(start)?;
        // A synchronous HMP wait_break holds the main-loop lock and can prevent
        // the guest from submitting its next request. Observe the actual suspension
        // asynchronously before issuing any further monitor command.
        loop {
            d.poll()?;
            save_budget(start)?;
            let text = d.output.text();
            if text.lines().any(|line| {
                line.contains("blkdebug: Suspended request")
                    && line.contains(&format!(" {}", char::from(39)))
                    && line.contains(&format!("{}{}", tag(number_at), char::from(39)))
            }) {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        save_budget(start)?;
        if number_at < number {
            // Install the next breakpoint before releasing this request. The
            // serial driver cannot reach the next flush without this resume.
            hmp(d, &format!("break flush_to_os {}", tag(number_at + 1)))?;
        } else {
            super::checked_file(disk)?;
            let mut file = File::open(disk)?;
            let length = file.metadata()?.len();
            if length < 512 || !length.is_multiple_of(512) {
                return Err("Editor fault disk has invalid sector geometry".into());
            }
            let offset = length - 512;
            file.seek(SeekFrom::Start(offset))?;
            let mut before = [0; 512];
            file.read_exact(&mut before)?;
            let source = work.join(format!("{name}.unchanged-tail.bin"));
            super::output_path(&source)?;
            if source
                .to_string_lossy()
                .bytes()
                .any(|b| b.is_ascii_whitespace() || b == b'"')
            {
                return Err("Editor fault observer source path has unsupported quoting".into());
            }
            fs::write(&source, before)?;
            hmp(d, &format!("write -s {} {offset} 512", source.display()))?;
            file.seek(SeekFrom::Start(offset))?;
            let mut after = [0; 512];
            file.read_exact(&mut after)?;
            if before != after {
                return Err("Editor fault generation write changed existing disk bytes".into());
            }
            fs::write(
                work.join(format!("{name}.flush-control.json")),
                serde_json::to_vec_pretty(&json!({
                    "flush_number":number,
                    "method":"QEMU breakpoint and exact same-sector generation write",
                    "backend":"anovadisk",
                    "offset":offset,
                    "length":512,
                    "source":source,
                    "before_sha256":format!("{:x}",Sha256::digest(before)),
                    "after_sha256":format!("{:x}",Sha256::digest(after)),
                    "unchanged":true,
                }))?,
            )?;
        }
        hmp(d, &format!("resume {}", tag(number_at)))?;
        save_budget(start)?;
    }
    loop {
        d.poll()?;
        save_budget(start)?;
        if d.output.text()[uart_start..].contains(fault.error_marker()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    // Check again after the original virtio FLUSH completed with its actual error.
    let source = work.join(format!("{name}.unchanged-tail.bin"));
    let expected = fs::read(&source)?;
    let mut file = File::open(disk)?;
    file.seek(SeekFrom::End(-512))?;
    let mut completed = [0; 512];
    file.read_exact(&mut completed)?;
    if completed.as_slice() != expected {
        return Err("Editor fault tail sector changed after completed virtio flush".into());
    }
    fs::write(
        work.join(format!("{name}.completed-tail-sha256.txt")),
        format!("{:x}", Sha256::digest(completed)),
    )?;
    save_budget(start)?;
    fs::write(
        work.join(format!("{name}.fault-control-ms.txt")),
        format!("elapsed_ms={}\n", start.elapsed().as_millis()),
    )?;
    Ok(())
}

/// Check exact stopped/replayed namespace bytes before recovery evidence is removed.
/// These cuts fail a transaction's first write or an explicit fsync after completed
/// transactions, so the raw home bytes and the ordinary replay result have the same contract.
pub(super) fn check_artifacts(
    name: &str,
    prior: &[u8],
    pending: &[u8],
    actual: [Option<&[u8]>; 3],
) -> Result<()> {
    let empty = &[][..];
    let expected = match name {
        "temp-create-write" | "inode-temp" => [Some(prior), None, None],
        "temp-data-write" | "blocks-temp" => [Some(prior), Some(empty), None],
        "temp-fsync" | "backup-create-write" | "inode-backup" => [Some(prior), Some(pending), None],
        "backup-data-write" | "blocks-backup" => [Some(prior), Some(pending), Some(empty)],
        "backup-fsync" | "prepublish-fsync" | "publish-write" => {
            [Some(prior), Some(pending), Some(prior)]
        }
        "postpublish-fsync" | "cleanup-write" => [Some(pending), None, Some(prior)],
        "cleanup-late-flush" => [Some(pending), None, None],
        _ => return Err("Unknown editor artifact policy case".into()),
    };
    for ((path, actual), expected) in ["target", "temporary", "backup"]
        .into_iter()
        .zip(actual)
        .zip(expected)
    {
        if actual != expected {
            return Err(format!(
                "Editor {name} {path} exact bytes differ: expected length {:?}, actual length {:?}",
                expected.map(<[u8]>::len),
                actual.map(<[u8]>::len),
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
    fn counters_match_actual_flush_events_and_reject_invalid_rules() {
        let first = options("first", Fault::WriteAfterFlush(0)).unwrap();
        assert!(first.get("set-state").is_none());
        assert_eq!(first["inject-error"][0]["state"], 1);
        let phase = options("phase", Fault::Flush(13)).unwrap();
        assert_eq!(phase["set-state"].as_array().unwrap().len(), 12);
        assert_eq!(phase["set-state"][0]["state"], 1);
        assert_eq!(phase["set-state"][11]["new_state"], 13);
        assert_eq!(phase["inject-error"][0]["event"], "flush_to_os");
        assert_eq!(phase["inject-error"][0]["iotype"], "flush");
        assert_eq!(phase["inject-error"][0]["state"], 13);
        let after = options("after", Fault::WriteAfterFlush(27)).unwrap();
        assert_eq!(after["set-state"].as_array().unwrap().len(), 27);
        assert_eq!(after["inject-error"][0]["state"], 28);
        assert!(options("bad/name", Fault::Flush(1)).is_err());
        assert!(options("", Fault::Flush(1)).is_err());
        assert!(options("zero", Fault::Flush(0)).is_err());
        assert!(options("large", Fault::Flush(129)).is_err());
        assert!(options("overflow", Fault::WriteAfterFlush(u32::MAX)).is_err());
    }

    #[test]
    fn artifact_oracle_rejects_corruption_absence_and_empty_recovery_confusion() {
        let prior = b"prior\n";
        let pending = b"pending\n";
        assert!(
            check_artifacts(
                "backup-data-write",
                prior,
                pending,
                [Some(prior), Some(pending), Some(&[])],
            )
            .is_ok()
        );
        for changed in [
            [Some(&b"PRIOR\n"[..]), Some(&pending[..]), Some(&[][..])],
            [Some(&prior[..]), Some(&b"PENDING\n"[..]), Some(&[][..])],
            [Some(&prior[..]), Some(&pending[..]), None],
            [Some(&prior[..]), Some(&pending[..]), Some(&prior[..])],
        ] {
            assert!(check_artifacts("backup-data-write", prior, pending, changed).is_err());
        }
        assert!(
            check_artifacts(
                "postpublish-fsync",
                prior,
                pending,
                [Some(pending), None, Some(prior)],
            )
            .is_ok()
        );
        assert!(
            check_artifacts(
                "postpublish-fsync",
                prior,
                pending,
                [Some(prior), Some(pending), Some(prior)],
            )
            .is_err()
        );
    }
}
