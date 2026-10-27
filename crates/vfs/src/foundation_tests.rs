// SPDX-License-Identifier: Apache-2.0
//! Stress on-demand namespaces beyond the node quota while retaining open, map and policy identities.
//! This deterministic backend isolates cache pressure; real ext4 bytes have independent storage tests.
use super::*;
use alloc::{format, vec};
use anova_storage::filesystem::{Capabilities, Format, Metadata};

struct Namespace;
const FILES: u32 = 10_001;
impl Namespace {
    fn entry(number: u32) -> DiskEntry {
        let label = if number == 3 {
            alloc::string::String::from(".anova-abi")
        } else {
            format!("f{:05}", number - 100)
        };
        let mut name = [0; 256];
        name[..label.len()].copy_from_slice(label.as_bytes());
        DiskEntry {
            inode: number,
            name,
            length: label.len(),
        }
    }
}
impl Filesystem for Namespace {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            format: Format::Ext4,
            root: 2,
            max_file: 256 * FILE_BYTES,
            timestamp_nanos: 1_000_000_000,
            read_only: true,
        }
    }
    fn inode(&mut self, number: u32) -> Result<Metadata, i64> {
        if number != 2 && number != 3 && !(100..100 + FILES).contains(&number) {
            return Err(INVALID_FILESYSTEM);
        }
        Ok(Metadata {
            number,
            directory: number == 2,
            size: if number == 3 {
                b"anova.native.v1\n".len()
            } else {
                12
            },
            executable: false,
            permissions: 0o644,
            modified: 123,
            accessed: 124,
            changed: 125,
        })
    }
    fn read(&mut self, number: u32, offset: usize, output: &mut [u8]) -> Result<usize, i64> {
        let source: &[u8] = if number == 3 {
            b"anova.native.v1\n"
        } else {
            b"cached bytes"
        };
        let n = output.len().min(source.len().saturating_sub(offset));
        output[..n]
            .copy_from_slice(&source[offset.min(source.len())..offset.min(source.len()) + n]);
        Ok(n)
    }
    fn next_entry(&mut self, _: u32, cursor: u64) -> Result<Option<(DiskEntry, u64)>, i64> {
        if cursor > u64::from(FILES) {
            return Ok(None);
        }
        Ok(Some((
            Self::entry(if cursor == 0 { 3 } else { 99 + cursor as u32 }),
            cursor + 1,
        )))
    }
    fn lookup(&mut self, _: u32, name: &str) -> Result<Option<DiskEntry>, i64> {
        if name == ".anova-abi" {
            return Ok(Some(Self::entry(3)));
        }
        let Some(index) = name
            .strip_prefix('f')
            .and_then(|n| n.parse::<u32>().ok())
            .filter(|n| *n < FILES)
        else {
            return Ok(None);
        };
        Ok(Some(Self::entry(100 + index)))
    }
    fn sync(&mut self) -> Result<(), i64> {
        Ok(())
    }
    fn shutdown(&mut self) -> Result<(), i64> {
        Ok(())
    }
    fn stop_device(&mut self) -> Result<(), i64> {
        Ok(())
    }
}
fn namespace() -> Vfs<'static> {
    let mut fs = Vfs::new(b"ANOVAFS1\x03\0\0\0\x02\0\x04\0\0\0\0\0\0\0\0\0/tmp\x02\0\x04\0\0\0\0\0\0\0\0\0/dev\x02\0\x08\0\0\0\0\0\0\0\0\0/persist").unwrap();
    fs.mount_disk(Namespace).unwrap();
    fs
}

#[test]
fn ten_thousand_names_do_not_import_or_consume_node_slots_on_readdir() {
    let mut fs = namespace();
    let initial = fs.node_slots();
    assert!(initial < 32);
    let mut fds = fs.standard().unwrap();
    let dir = fs
        .open(&mut fds, ROOT, "/persist", OPEN_READ | OPEN_DIRECTORY)
        .unwrap();
    let mut seen = vec![false; FILES as usize];
    let mut count = 0;
    while let Some(entry) = fs.readdir(&fds, dir).unwrap() {
        let name = core::str::from_utf8(&entry.name[..entry.name_len as usize]).unwrap();
        if name == ".anova-abi" {
            assert_eq!(entry.inode, PERSISTENT_ID_BASE + 3);
            continue;
        }
        let index = name[1..].parse::<usize>().unwrap();
        assert!(!seen[index]);
        seen[index] = true;
        assert_eq!(entry.inode, PERSISTENT_ID_BASE + 100 + index as u64);
        count += 1;
    }
    assert_eq!(count, FILES);
    assert!(seen.iter().all(|n| *n));
    assert_eq!(fs.node_slots(), initial);
    fs.close(&mut fds, dir).unwrap();
    fs.clear(&mut fds);
    fs.shutdown().unwrap();
}

#[test]
fn eviction_retains_stable_stat_open_offsets_maps_and_immutable_policy() {
    let mut fs = namespace();
    let mut fds = fs.standard().unwrap();
    let open = fs
        .open(&mut fds, ROOT, "/persist/f00000", OPEN_READ)
        .unwrap();
    fs.dup2(&mut fds, open, 8).unwrap();
    let mapped = fs
        .open(&mut fds, ROOT, "/persist/f00001", OPEN_READ)
        .unwrap();
    let mapping = fs.shared_file_mapping(&fds, mapped, 0, 4096).unwrap();
    fs.close(&mut fds, mapped).unwrap();
    let identity = fs.metadata(ROOT, "/persist/f00002").unwrap().0;
    for n in 3..FILES {
        fs.metadata(ROOT, &format!("/persist/f{n:05}")).unwrap();
    }
    assert_eq!(fs.node_slots(), NODE_LIMIT);
    assert_eq!(fs.metadata(ROOT, "/persist/f00002").unwrap().0, identity);
    assert_eq!(
        fs.times(identity),
        [124, 123, 125].map(|n| n * 1_000_000_000)
    );
    let mut bytes = [0; 6];
    fs.read(&fds, open, &mut bytes).unwrap();
    assert_eq!(&bytes, b"cached");
    fs.read(&fds, 8, &mut bytes).unwrap();
    assert_eq!(&bytes, b" bytes");
    let mut page = [0; 12];
    mapping.copy_into(0, &mut page);
    assert_eq!(&page, b"cached bytes");
    assert_eq!(
        fs.open(&mut fds, ROOT, "/persist/.anova-abi", OPEN_WRITE),
        Err(READ_ONLY)
    );
    assert_eq!(fs.unlink(ROOT, "/persist/.anova-abi"), Err(READ_ONLY));
    assert_eq!(
        fs.open(&mut fds, ROOT, "/persist/new", OPEN_WRITE | OPEN_CREATE),
        Err(READ_ONLY)
    );
    assert!(!fs.linux_executable(ROOT, "/persist/f00002").unwrap());
    drop(mapping);
    fs.clear(&mut fds);
    fs.shutdown().unwrap();
}
