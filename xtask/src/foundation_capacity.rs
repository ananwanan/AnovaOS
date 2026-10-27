// SPDX-License-Identifier: Apache-2.0
//! Accept the existing guest kernel recipe on a fresh JBD2 disk after real resource exhaustion.
//! Separate pressure from build demand; retain predeclared budgets and mutation-level disk peaks.
use super::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn verify(root: &Path) -> Result<()> {
    bootstrap_kernel::verify_profile(root, true)
}

pub(super) fn budgets() -> Value {
    json!({
        "heap_bytes": 16777216, "vfs_slots": 2048, "task_slots": 32,
        "fds_per_process": 32, "frame_delta_pages": 524288, "frame_bytes": 4096,
        "disk_bytes": 67108864, "disk_blocks": 16384, "disk_inodes": 4096,
        "journal_bytes": 8388608, "session_seconds": 3600,
        "cpus": 1, "ram_bytes": 3221225472u64, "accelerator": "tcg",
        "scope": "Existing stripped guest kernel recipe; inherited Rust 1.90.0, no compiler reconstruction",
        "measurement_limits": "Memory is sampled at scheduler boundaries, not exact peak RSS; disk peaks count successful ext4 mutations including temporary files, not staged or failed transactions",
    })
}

pub(super) fn session(
    root: &Path,
    work: &Path,
    iso: &Path,
    path: &Path,
    name: &str,
    steps: &[(String, Vec<u8>)],
) -> Result<Value> {
    let exchanges: Vec<_> = steps
        .iter()
        .map(|(m, b)| (m.as_str(), b.as_slice()))
        .collect();
    let start = Instant::now();
    let result = disk::session_timeout(root, iso, path, name, false, None, &exchanges, true, 3600);
    for suffix in ["serial.log", "qemu.log", "run.log", "result.txt"] {
        let source = root.join(format!("build/logs/{name}.{suffix}"));
        if source.is_file() {
            fs::copy(source, work.join(format!("{name}.{suffix}")))?;
        }
    }
    result?;
    let serial = fs::read_to_string(root.join(format!("build/logs/{name}.serial.log")))?;
    if serial.contains(":FAIL]") || serial.contains("[PANIC]") {
        return Err("Capacity session contains a guest failure".into());
    }
    let resources = bootstrap::resource_summary(&serial)?;
    let declarations = budgets();
    for (metric, limit) in [
        ("heap_peak_bytes", "heap_bytes"),
        ("peak_frame_delta", "frame_delta_pages"),
        ("vfs_node_slots_peak", "vfs_slots"),
        ("occupied_slots_peak", "task_slots"),
        ("fd_per_process_peak", "fds_per_process"),
    ] {
        if resources[metric].parse::<u64>()? > declarations[limit].as_u64().unwrap() {
            return Err(format!("Capacity budget exceeded: {metric}").into());
        }
    }
    let lines: Vec<_> = serial
        .lines()
        .filter_map(|l| l.strip_prefix("[M16:STORAGE] "))
        .collect();
    if lines.len() != 1 {
        return Err("Expected one ext4 allocation summary".into());
    }
    let storage: BTreeMap<_, _> = lines[0]
        .split_whitespace()
        .filter_map(|v| v.split_once('='))
        .collect();
    if storage.get("scope") != Some(&"mount")
        || storage.get("sampling") != Some(&"successful-mutations")
        || storage.get("block_bytes") != Some(&"4096")
    {
        return Err("Unexpected ext4 allocation measurement scope".into());
    }
    let number = |key: &str| -> Result<u64> {
        Ok(storage
            .get(key)
            .ok_or_else(|| format!("Missing ext4 metric: {key}"))?
            .parse()?)
    };
    let mut stopped = bootstrap::disk_capacity(path)?;
    // The shared M15 geometry reader retains its ext2 file ceiling; this disk uses ext4 admission.
    stopped["writable_file_limit_bytes"] = json!(anova_storage::ext4::MAX_FILE);
    let blocks = number("allocated_blocks")?;
    let inodes = number("allocated_inodes")?;
    if stopped["allocated_blocks"] != blocks
        || stopped["allocated_inodes"] != inodes
        || number("peak_blocks")? < blocks
        || number("peak_blocks")? > 16384
        || number("peak_inodes")? < inodes
        || number("peak_inodes")? > 4096
    {
        return Err("Ext4 usage differs from stopped superblock or exceeds budget".into());
    }
    foundation_journal::check(path)?;
    let evidence = json!({"host_wall_ms_including_boot_and_shutdown":start.elapsed().as_millis(),
        "resources":resources,"storage":storage,"stopped_disk":stopped});
    fs::write(
        work.join(format!("{name}.json")),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    Ok(evidence)
}
