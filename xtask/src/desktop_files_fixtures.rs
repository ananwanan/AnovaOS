// SPDX-License-Identifier: Apache-2.0
//! Construct regular-image fixtures with exact editor bounds and real ext4 allocation pressure.
//! Retain debugfs scripts, counters and independent filesystem checks beside each test case.

use super::*;
use anova_storage::filesystem::Filesystem;
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom};

pub(super) const PRIOR: &[u8] = b"// M17-FAULT-OLD\n";

#[derive(Clone, Copy, Debug)]
pub(super) enum Pressure {
    None,
    Inodes(u32),
    Blocks(u32),
}

pub(super) struct Fixture {
    pub recovery_commands: Vec<String>,
    pub free_blocks: u32,
    pub free_inodes: u32,
}

pub(super) fn limit_bytes() -> Vec<u8> {
    let mut bytes = b"// M17-LIMIT\n".to_vec();
    bytes.resize(16_383, b'a');
    bytes.push(b'\n');
    bytes
}

pub(super) fn oversize_bytes() -> Vec<u8> {
    let mut bytes = limit_bytes();
    bytes.push(b'x');
    bytes
}

/// Create only a fresh ordinary work image; pressure never edits filesystem counters.
pub(super) fn create(root: &Path, path: &Path, work: &Path, pressure: Pressure) -> Result<Fixture> {
    let disks = directory(root, "disks")?.canonicalize()?;
    if path
        .parent()
        .ok_or("Missing fixture parent")?
        .canonicalize()?
        != disks
    {
        return Err("Editor fault images must be direct regular files under build/disks".into());
    }
    output_path(path)?;
    if path.exists() {
        return Err("Editor fault fixture must be a new disk image".into());
    }
    foundation_journal::create(root, path, true)?;
    checked_file(path)?;
    let mut script = String::new();
    for (name, bytes) in [
        ("case.c", PRIOR.to_vec()),
        ("limit.c", limit_bytes()),
        ("oversize.c", oversize_bytes()),
    ] {
        let source = work.join(format!("fixture-{name}"));
        output_path(&source)?;
        fs::write(&source, &bytes)?;
        if source
            .to_string_lossy()
            .bytes()
            .any(|b| b.is_ascii_whitespace())
        {
            return Err("Fixture debugfs paths cannot contain whitespace".into());
        }
        script.push_str(&format!("write {} /{name}\n", source.display()));
        script.push_str(&format!("set_inode_field /{name} uid 0\n"));
        script.push_str(&format!("set_inode_field /{name} gid 0\n"));
    }
    apply(path, work, "seed", &script)?;
    let mut recovery_commands = Vec::new();
    match pressure {
        Pressure::None => {}
        Pressure::Inodes(reserve) => {
            apply(path, work, "pressure-directory", "mkdir /pressure\n")?;
            let (_, free) = counts(path)?;
            let allocated = free.checked_sub(reserve).ok_or("Invalid inode reserve")?;
            if allocated < 8 {
                return Err("Inode-pressure fixture needs eight releasable inodes".into());
            }
            let mut script = String::new();
            for n in 0..allocated {
                script.push_str(&format!("write /dev/null /pressure/i{n:04}\n"));
                if n >= allocated - 8 {
                    recovery_commands.push(format!("rm /persist/pressure/i{n:04}"));
                }
            }
            apply(path, work, "inode-pressure", &script)?;
        }
        Pressure::Blocks(reserve) => {
            apply(
                path,
                work,
                "pressure-file",
                "write /dev/null /pressure.bin\n",
            )?;
            let (free, _) = counts(path)?;
            let allocated = free.checked_sub(reserve).ok_or("Invalid block reserve")?;
            if allocated == 0 {
                return Err("Block-pressure fixture needs allocated blocks".into());
            }
            apply(
                path,
                work,
                "block-pressure",
                &format!(
                    "fallocate /pressure.bin 0 {}\nset_inode_field /pressure.bin size {}\n",
                    allocated - 1,
                    u64::from(allocated) * 4096,
                ),
            )?;
            recovery_commands.push("rm /persist/pressure.bin".into());
        }
    }
    let (free_blocks, free_inodes) = counts(path)?;
    let exact = match pressure {
        Pressure::None => true,
        Pressure::Inodes(reserve) => free_inodes == reserve,
        Pressure::Blocks(reserve) => free_blocks == reserve,
    };
    if !exact {
        return Err(format!(
            "Pressure allocation differs: {pressure:?}, blocks={free_blocks}, inodes={free_inodes}"
        )
        .into());
    }
    // Audit actual ownership/allocation before boot without any editor model operations.
    let mut volume = anova_storage::ext4::Ext4::mount_writable(Box::new(disk::Image::open(path)?))
        .map_err(|error| format!("Pressure fixture writable audit failed: {error}"))?;
    volume
        .shutdown()
        .map_err(|error| format!("Pressure fixture shutdown failed: {error}"))?;
    let check = Command::new("e2fsck")
        .args(["-f", "-n"])
        .arg(path)
        .output()?;
    let mut check_log = check.stdout;
    check_log.extend(check.stderr);
    fs::write(work.join("fixture-e2fsck.log"), check_log)?;
    if !check.status.success() {
        return Err("Independent pressure fixture e2fsck failed".into());
    }
    let listing = Command::new("debugfs")
        .args(["-R", "ls -l /"])
        .arg(path)
        .output()?;
    let mut listing_log = listing.stdout;
    listing_log.extend(listing.stderr);
    fs::write(work.join("fixture-root-listing.log"), listing_log)?;
    fs::write(
        work.join("fixture.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "pressure":format!("{pressure:?}"),
            "disk":path,
            "allocation":"real inode/block bitmaps, no counter modification",
            "free_blocks":free_blocks,
            "free_inodes":free_inodes,
            "block_bytes":4096,
            "prior_sha256":format!("{:x}",Sha256::digest(PRIOR)),
            "limit_sha256":format!("{:x}",Sha256::digest(limit_bytes())),
            "oversize_sha256":format!("{:x}",Sha256::digest(oversize_bytes())),
            "recovery_commands":recovery_commands,
            "writable_audit":true,
            "e2fsck":true,
        }))?,
    )?;
    Ok(Fixture {
        recovery_commands,
        free_blocks,
        free_inodes,
    })
}

fn counts(path: &Path) -> Result<(u32, u32)> {
    checked_file(path)?;
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(1024 + 12))?;
    let mut fields = [0u8; 8];
    file.read_exact(&mut fields)?;
    Ok((
        u32::from_le_bytes(fields[..4].try_into()?),
        u32::from_le_bytes(fields[4..].try_into()?),
    ))
}

fn apply(path: &Path, work: &Path, name: &str, script: &str) -> Result<()> {
    checked_file(path)?;
    let input = work.join(format!("fixture-{name}.debugfs"));
    output_path(&input)?;
    fs::write(&input, script)?;
    let output = Command::new("debugfs")
        .arg("-w")
        .arg("-f")
        .arg(&input)
        .arg(path)
        .output()?;
    let mut log = output.stdout;
    log.extend(output.stderr);
    fs::write(work.join(format!("fixture-{name}.log")), log)?;
    if !output.status.success() {
        return Err(format!("Fixture debugfs command failed: {name}").into());
    }
    // debugfs can return zero for rejected subcommands. Counters, the writable
    // audit, root listing and e2fsck above independently reject incomplete fixtures.
    Ok(())
}
