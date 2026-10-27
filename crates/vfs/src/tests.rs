// SPDX-License-Identifier: Apache-2.0
//! Test VFS paths, backend file/cache limits, timestamp coherence, ownership and rollback.
use super::*;
use alloc::{boxed::Box, rc::Rc, vec};
use anova_storage::ext2::Ext2;
use anova_storage::{BlockDevice, SECTOR, check_range, ext2::BLOCK};
use core::cell::RefCell;

#[derive(Clone)]
struct TimestampDisk(Rc<RefCell<Vec<u8>>>);
impl BlockDevice for TimestampDisk {
    fn sectors(&self) -> u64 {
        (self.0.borrow().len() / SECTOR) as u64
    }
    fn read_sector(&mut self, sector: u64, output: &mut [u8; SECTOR]) -> Result<(), i64> {
        check_range(self.sectors(), sector, SECTOR)?;
        let start = sector as usize * SECTOR;
        output.copy_from_slice(&self.0.borrow()[start..start + SECTOR]);
        Ok(())
    }
    fn write_sector(&mut self, sector: u64, input: &[u8; SECTOR]) -> Result<(), i64> {
        check_range(self.sectors(), sector, SECTOR)?;
        let start = sector as usize * SECTOR;
        self.0.borrow_mut()[start..start + SECTOR].copy_from_slice(input);
        Ok(())
    }
    fn flush(&mut self) -> Result<(), i64> {
        Ok(())
    }
}

fn timestamp_disk() -> TimestampDisk {
    timestamp_disk_blocks(128)
}
fn timestamp_disk_blocks(blocks: u32) -> TimestampDisk {
    // Real one-group ext2 bytes matching the storage test profile, not a mocked inode.
    let mut bytes = vec![0; blocks as usize * BLOCK];
    let p16 = |bytes: &mut [u8], offset, value: u16| {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    };
    let p32 = |bytes: &mut [u8], offset, value: u32| {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    };
    let superblock = &mut bytes[1024..2048];
    for (offset, value) in [
        (0, 128),
        (4, blocks),
        (12, blocks - 9),
        (16, 118),
        (24, 2),
        (28, 2),
        (32, blocks),
        (36, blocks),
        (40, 128),
        (76, 1),
        (84, 11),
        (96, 2),
    ] {
        p32(superblock, offset, value);
    }
    p16(superblock, 56, 0xef53);
    p16(superblock, 58, 1);
    p16(superblock, 88, 128);
    let group = &mut bytes[BLOCK..BLOCK + 32];
    for (offset, value) in [(0, 2), (4, 3), (8, 4)] {
        p32(group, offset, value);
    }
    p16(group, 12, (blocks - 9) as u16);
    p16(group, 14, 118);
    p16(group, 16, 1);
    for bit in 0..9 {
        bytes[2 * BLOCK + bit / 8] |= 1 << (bit % 8);
    }
    for bit in 0..10 {
        bytes[3 * BLOCK + bit / 8] |= 1 << (bit % 8);
    }
    let root = &mut bytes[4 * BLOCK + 128..4 * BLOCK + 256];
    p16(root, 0, 0x41ed);
    p32(root, 4, BLOCK as u32);
    p16(root, 26, 2);
    p32(root, 28, 8);
    p32(root, 40, 8);
    let directory = &mut bytes[8 * BLOCK..9 * BLOCK];
    p32(directory, 0, 2);
    p16(directory, 4, 12);
    directory[6] = 1;
    directory[7] = 2;
    directory[8] = b'.';
    p32(directory, 12, 2);
    p16(directory, 16, (BLOCK - 12) as u16);
    directory[18] = 2;
    directory[19] = 2;
    directory[20..22].copy_from_slice(b"..");
    let device = TimestampDisk(Rc::new(RefCell::new(bytes)));
    let mut disk = Ext2::mount(Box::new(device.clone())).unwrap();
    let marker = disk.create(2, ".anova-abi", false).unwrap();
    disk.write(marker, 0, b"linux.x86_64.v1\n").unwrap();
    disk.shutdown().unwrap();
    device
}

fn assert_persistent_times(fs: &mut Vfs<'_>, inode: u64) {
    let cached = fs.times(inode);
    let number = fs.nodes[fs.identity_node(inode).unwrap()].inode.unwrap();
    let stored = fs.disk.as_mut().unwrap().inode(number).unwrap();
    assert_eq!(
        cached,
        [stored.accessed, stored.modified, stored.changed]
            .map(|time| u64::from(time) * 1_000_000_000)
    );
    assert_eq!(fs.attributes(inode).1, cached[1]);
}

struct RetryRelease {
    disk: Ext2,
    fail: Rc<core::cell::Cell<bool>>,
}
impl Filesystem for RetryRelease {
    fn capabilities(&self) -> anova_storage::filesystem::Capabilities {
        self.disk.capabilities()
    }
    fn inode(&mut self, n: u32) -> Result<anova_storage::filesystem::Metadata, i64> {
        Filesystem::inode(&mut self.disk, n)
    }
    fn read(&mut self, n: u32, offset: usize, bytes: &mut [u8]) -> Result<usize, i64> {
        self.disk.read(n, offset, bytes)
    }
    fn next_entry(&mut self, n: u32, cursor: u64) -> Result<Option<(DiskEntry, u64)>, i64> {
        self.disk.next_entry(n, cursor)
    }
    fn sync(&mut self) -> Result<(), i64> {
        self.disk.sync()
    }
    fn shutdown(&mut self) -> Result<(), i64> {
        self.disk.shutdown()
    }
    fn stop_device(&mut self) -> Result<(), i64> {
        self.disk.stop_device()
    }
    fn create(&mut self, n: u32, name: &str, directory: bool) -> Result<u32, i64> {
        self.disk.create(n, name, directory)
    }
    fn unlink(&mut self, n: u32, name: &str) -> Result<u32, i64> {
        self.disk.unlink(n, name)
    }
    fn write(&mut self, n: u32, offset: usize, bytes: &[u8]) -> Result<usize, i64> {
        self.disk.write(n, offset, bytes)
    }
    fn release_unlinked(&mut self, n: u32) -> Result<(), i64> {
        if self.fail.replace(false) {
            return Err(NO_MEMORY);
        }
        self.disk.release_unlinked(n)
    }
    fn rename(&mut self, p: u32, name: &str, d: u32, new: &str) -> Result<(), i64> {
        self.disk.rename(p, name, d, new)
    }
}
#[test]
fn final_close_and_unmap_oom_keep_orphan_identity_until_reclamation_retries() {
    let archive = image();
    let mut fs = Vfs::new(&archive).unwrap();
    fs.insert(ROOT, "persist", Data::Directory, Mount::Initramfs)
        .unwrap();
    let fail = Rc::new(core::cell::Cell::new(false));
    fs.mount_disk(RetryRelease {
        disk: Ext2::mount(Box::new(timestamp_disk())).unwrap(),
        fail: fail.clone(),
    })
    .unwrap();
    let mut fds = Descriptors::default();
    for mapped in [false, true] {
        let fd = fs
            .open(
                &mut fds,
                ROOT,
                "/persist/orphan",
                OPEN_CREATE | OPEN_READ | OPEN_WRITE,
            )
            .unwrap();
        fs.write(&fds, fd, b"retained").unwrap();
        let node = fs.resolve(ROOT, "/persist/orphan").unwrap();
        let number = fs.nodes[node].inode;
        let mapping = if mapped {
            Some(fs.shared_file_mapping(&fds, fd, 0, 4096).unwrap())
        } else {
            None
        };
        fs.unlink(ROOT, "/persist/orphan").unwrap();
        fail.set(true);
        if mapped {
            fs.close(&mut fds, fd).unwrap();
            let mut bytes = [0; 8];
            mapping.as_ref().unwrap().copy_into(0, &mut bytes);
            assert_eq!(&bytes, b"retained");
            drop(mapping);
            assert_eq!(fs.collect_mappings(), Err(NO_MEMORY));
        } else {
            assert_eq!(fs.close(&mut fds, fd), Err(NO_MEMORY));
        }
        assert_eq!(fs.nodes[node].inode, number);
        assert!(!fs.reusable(node));
        fs.sync().unwrap();
        assert_eq!(fs.nodes[node].inode, None);
        assert!(fs.reusable(node));
    }
    let source = fs
        .open(&mut fds, ROOT, "/persist/source", OPEN_CREATE | OPEN_WRITE)
        .unwrap();
    fs.write(&fds, source, b"source").unwrap();
    fs.close(&mut fds, source).unwrap();
    let target = fs
        .open(&mut fds, ROOT, "/persist/target", OPEN_CREATE | OPEN_WRITE)
        .unwrap();
    fs.close(&mut fds, target).unwrap();
    fail.set(true);
    fs.rename(ROOT, "/persist/source", "/persist/target")
        .unwrap();
    assert_eq!(fs.resolve(ROOT, "/persist/source"), Err(NOT_FOUND));
    let fd = fs
        .open(&mut fds, ROOT, "/persist/target", OPEN_READ)
        .unwrap();
    let mut bytes = [0; 6];
    fs.read(&fds, fd, &mut bytes).unwrap();
    assert_eq!(&bytes, b"source");
    fs.close(&mut fds, fd).unwrap();
    fail.set(true);
    fs.unlink(ROOT, "/persist/target").unwrap();
    fs.sync().unwrap();
    fs.shutdown().unwrap();
}

#[test]
fn uncached_ext2_children_prevent_rmdir_and_preserve_rename_open_unlink_lifetimes() {
    let archive = image();
    let device = timestamp_disk();
    let mut disk = Ext2::mount(Box::new(device.clone())).unwrap();
    let directory = disk.create(2, "unloaded", true).unwrap();
    let file = disk.create(directory, "old", false).unwrap();
    disk.write(file, 0, b"retained").unwrap();
    disk.shutdown().unwrap();
    let mut fs = Vfs::new(&archive).unwrap();
    fs.insert(ROOT, "persist", Data::Directory, Mount::Initramfs)
        .unwrap();
    fs.mount_disk(Ext2::mount(Box::new(device)).unwrap())
        .unwrap();
    assert_eq!(fs.rmdir(ROOT, "/persist/unloaded", &[]), Err(NOT_EMPTY));
    fs.rename(ROOT, "/persist/unloaded/old", "/persist/unloaded/new")
        .unwrap();
    let mut fds = Descriptors::default();
    let fd = fs
        .open(&mut fds, ROOT, "/persist/unloaded/new", OPEN_READ)
        .unwrap();
    let identity = fs.descriptor_metadata(&fds, fd).unwrap().0;
    fs.unlink(ROOT, "/persist/unloaded/new").unwrap();
    let mut bytes = [0; 8];
    assert_eq!(fs.read(&fds, fd, &mut bytes), Ok(8));
    assert_eq!(&bytes, b"retained");
    assert_eq!(fs.descriptor_metadata(&fds, fd).unwrap().0, identity);
    fs.close(&mut fds, fd).unwrap();
    fs.rmdir(ROOT, "/persist/unloaded", &[]).unwrap();
    fs.shutdown().unwrap();
}

#[test]
fn persistent_large_files_have_separate_tmpfs_and_shared_cache_limits() {
    let archive = image();
    let device = timestamp_disk_blocks(2048);
    let mut fs = Vfs::new(&archive).unwrap();
    fs.insert(ROOT, "persist", Data::Directory, Mount::Initramfs)
        .unwrap();
    fs.mount_disk(Ext2::mount(Box::new(device.clone())).unwrap())
        .unwrap();
    let mut fds = Descriptors::default();
    let flags = OPEN_CREATE | OPEN_READ | OPEN_WRITE;
    let persistent = fs.open(&mut fds, ROOT, "/persist/large", flags).unwrap();
    let temporary = fs.open(&mut fds, ROOT, "/tmp/small", flags).unwrap();
    let max = anova_storage::ext2::MAX_FILE;
    assert_eq!(fs.writable_file_limit(&fds, persistent), Ok(max));
    assert_eq!(fs.writable_file_limit(&fds, temporary), Ok(FILE_BYTES));
    assert_eq!(fs.writable_file_limit(&fds, 99), Err(BAD_DESCRIPTOR));
    fs.write(&fds, persistent, b"original").unwrap();
    let mapping = fs.shared_file_mapping(&fds, persistent, 0, 4096).unwrap();
    assert_eq!(fs.set_len(&fds, persistent, FILE_BYTES + 1), Err(BUSY));
    assert_eq!(
        fs.write_at(&fds, persistent, FILE_BYTES as u64, b"!"),
        Err(BUSY)
    );
    assert_eq!(fs.metadata(ROOT, "/persist/large").unwrap().2, 8);
    assert_eq!(mapping.length(), 8);
    drop(mapping);
    // Growth itself must discard an idle cache, even before the scheduler collection pass.
    fs.write_at(&fds, persistent, FILE_BYTES as u64, b"beyond")
        .unwrap();
    assert_eq!(
        fs.shared_file_mapping(&fds, persistent, 0, 4096).err(),
        Some(UNSUPPORTED)
    );
    fs.seek(&fds, persistent, (max - 4) as i64, 0).unwrap();
    fs.write(&fds, persistent, b"tail").unwrap();
    assert_eq!(fs.seek(&fds, persistent, 0, 1), Ok(max));
    assert_eq!(fs.seek(&fds, persistent, 1, 1), Err(INVALID_ARGUMENT));
    assert_eq!(fs.write(&fds, persistent, b"!"), Err(NO_SPACE));
    assert_eq!(fs.set_len(&fds, persistent, max + 1), Err(NO_SPACE));
    assert_eq!(fs.write_at(&fds, persistent, u64::MAX, b"!"), Err(NO_SPACE));
    let mut bytes = [0; 8];
    fs.read_at(&fds, persistent, 0, &mut bytes).unwrap();
    assert_eq!(&bytes, b"original");
    assert_eq!(fs.seek(&fds, persistent, 0, 1), Ok(max));
    assert_eq!(fs.set_len(&fds, temporary, FILE_BYTES + 1), Err(NO_SPACE));
    assert_eq!(
        fs.seek(&fds, temporary, FILE_BYTES as i64 + 1, 0),
        Err(INVALID_ARGUMENT)
    );
    fs.seek(&fds, temporary, FILE_BYTES as i64, 0).unwrap();
    assert_eq!(fs.write(&fds, temporary, b"!"), Err(NO_SPACE));
    fs.close(&mut fds, temporary).unwrap();
    fs.close(&mut fds, persistent).unwrap();
    fs.shutdown().unwrap();
    drop(fs);
    let mut fs = Vfs::new(&archive).unwrap();
    fs.insert(ROOT, "persist", Data::Directory, Mount::Initramfs)
        .unwrap();
    fs.mount_disk(Ext2::mount(Box::new(device)).unwrap())
        .unwrap();
    let fd = fs
        .open(&mut fds, ROOT, "/persist/large", OPEN_READ)
        .unwrap();
    assert_eq!(fs.metadata(ROOT, "/persist/large").unwrap().2, max);
    assert_eq!(fs.read_at(&fds, fd, (max - 4) as u64, &mut bytes), Ok(4));
    assert_eq!(&bytes[..4], b"tail");
    assert_eq!(fs.writable_file_limit(&fds, fd), Err(BAD_DESCRIPTOR));
    fs.close(&mut fds, fd).unwrap();
    fs.shutdown().unwrap();
}

#[test]
fn persistent_file_times_match_backing_inodes_and_cargo_markers_across_mutations() {
    let archive = image();
    let device = timestamp_disk();
    let mut fs = Vfs::new(&archive).unwrap();
    fs.insert(ROOT, "persist", Data::Directory, Mount::Initramfs)
        .unwrap();
    fs.mount_disk(Ext2::mount(Box::new(device.clone())).unwrap())
        .unwrap();
    let mut files = Descriptors::default();
    fs.set_clock(1_250_000_000);
    let source = fs
        .open(
            &mut files,
            ROOT,
            "/persist/source.rs",
            OPEN_CREATE | OPEN_READ | OPEN_WRITE,
        )
        .unwrap();
    let inode = fs.metadata(ROOT, "/persist/source.rs").unwrap().0;
    assert_persistent_times(&mut fs, inode);
    fs.write(&files, source, b"source").unwrap();
    assert_persistent_times(&mut fs, inode);
    // Cargo explicitly dates its invocation marker after a source saved in the same second.
    fs.set_clock(1_800_000_000);
    let marker = fs
        .open(
            &mut files,
            ROOT,
            "/persist/invoked.timestamp",
            OPEN_CREATE | OPEN_WRITE,
        )
        .unwrap();
    let marker_inode = fs.metadata(ROOT, "/persist/invoked.timestamp").unwrap().0;
    fs.set_times(marker_inode, None, Some(1_800_000_000))
        .unwrap();
    assert!(fs.times(inode)[1] <= fs.times(marker_inode)[1]);
    assert_persistent_times(&mut fs, marker_inode);
    fs.close(&mut files, marker).unwrap();
    fs.set_clock(2_100_000_000);
    fs.write(&files, source, b"more").unwrap();
    assert_persistent_times(&mut fs, inode);
    assert_eq!(fs.times(inode)[1], 2_000_000_000);
    fs.set_clock(3_400_000_000);
    fs.set_len(&files, source, 3).unwrap();
    assert_persistent_times(&mut fs, inode);
    assert_eq!(fs.times(inode)[1], 3_000_000_000);
    let unchanged = fs.times(inode);
    fs.set_clock(4_100_000_000);
    fs.set_len(&files, source, 3).unwrap();
    assert_eq!(fs.times(inode), unchanged);
    assert_persistent_times(&mut fs, inode);
    fs.close(&mut files, source).unwrap();
    fs.set_clock(4_500_000_000);
    let source = fs
        .open(
            &mut files,
            ROOT,
            "/persist/source.rs",
            OPEN_WRITE | OPEN_TRUNCATE,
        )
        .unwrap();
    assert_persistent_times(&mut fs, inode);
    assert_eq!(fs.times(inode)[1], 4_000_000_000);
    fs.set_clock(5_600_000_000);
    fs.set_times(inode, Some(10_123_000_000), Some(11_456_000_000))
        .unwrap();
    assert_persistent_times(&mut fs, inode);
    let before = fs.times(inode);
    fs.close(&mut files, source).unwrap();
    fs.shutdown().unwrap();
    drop(fs);
    let mut fs = Vfs::new(&archive).unwrap();
    fs.insert(ROOT, "persist", Data::Directory, Mount::Initramfs)
        .unwrap();
    fs.mount_disk(Ext2::mount(Box::new(device)).unwrap())
        .unwrap();
    let inode = fs.metadata(ROOT, "/persist/source.rs").unwrap().0;
    assert_eq!(fs.times(inode), before);
    assert_persistent_times(&mut fs, inode);
}

#[test]
fn tmpfs_mutations_and_explicit_times_retain_nanosecond_precision() {
    let archive = image();
    let mut fs = Vfs::new(&archive).unwrap();
    let mut files = Descriptors::default();
    fs.set_clock(1_250_000_123);
    let fd = fs
        .open(
            &mut files,
            ROOT,
            "/tmp/precision",
            OPEN_CREATE | OPEN_READ | OPEN_WRITE,
        )
        .unwrap();
    let inode = fs.metadata(ROOT, "/tmp/precision").unwrap().0;
    assert_eq!(fs.times(inode), [1_250_000_123; 3]);
    fs.set_clock(2_100_000_456);
    fs.write(&files, fd, b"data").unwrap();
    assert_eq!(fs.times(inode)[1..], [2_100_000_456; 2]);
    fs.set_clock(3_400_000_789);
    fs.set_len(&files, fd, 2).unwrap();
    assert_eq!(fs.times(inode)[1..], [3_400_000_789; 2]);
    fs.close(&mut files, fd).unwrap();
    fs.set_clock(4_500_000_321);
    let fd = fs
        .open(
            &mut files,
            ROOT,
            "/tmp/precision",
            OPEN_WRITE | OPEN_TRUNCATE,
        )
        .unwrap();
    assert_eq!(fs.times(inode)[1..], [4_500_000_321; 2]);
    fs.set_clock(5_600_000_654);
    fs.set_times(inode, Some(10_123_000_987), Some(11_456_000_111))
        .unwrap();
    assert_eq!(
        fs.times(inode),
        [10_123_000_987, 11_456_000_111, 5_600_000_654]
    );
    fs.close(&mut files, fd).unwrap();
}
#[test]
fn mounted_policy_identity_rejects_content_and_namespace_mutations() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut fds = Descriptors::default();
    let path = "/tmp/.anova-abi";
    let fd = fs
        .open(&mut fds, ROOT, path, OPEN_CREATE | OPEN_READ | OPEN_WRITE)
        .unwrap();
    fs.write(&fds, fd, b"anova.native.v1\n").unwrap();
    // Pin the same node identity mount_disk retains after reading a checked ext2 marker.
    fs.policy_node = Some(fs.resolve(ROOT, path).unwrap());
    assert_eq!(
        fs.open(&mut fds, ROOT, path, OPEN_WRITE | OPEN_TRUNCATE),
        Err(READ_ONLY)
    );
    assert_eq!(fs.write(&fds, fd, b"linux.x86_64.v1\n"), Err(READ_ONLY));
    assert_eq!(
        fs.write_at(&fds, fd, 0, b"linux.x86_64.v1\n"),
        Err(READ_ONLY)
    );
    assert_eq!(fs.set_len(&fds, fd, 0), Err(READ_ONLY));
    assert_eq!(fs.chmod(ROOT, path, 0o777), Err(READ_ONLY));
    assert_eq!(fs.fchmod(&fds, fd, 0o777), Err(READ_ONLY));
    assert_eq!(fs.set_executable(ROOT, path), Err(READ_ONLY));
    assert_eq!(fs.unlink(ROOT, path), Err(READ_ONLY));
    assert_eq!(fs.rename(ROOT, path, "/tmp/moved"), Err(READ_ONLY));
    let replacement = fs
        .open(&mut fds, ROOT, "/tmp/replace", OPEN_CREATE | OPEN_WRITE)
        .unwrap();
    fs.write(&fds, replacement, b"linux.x86_64.v1\n").unwrap();
    assert_eq!(fs.rename(ROOT, "/tmp/replace", path), Err(READ_ONLY));
    assert!(!fs.writable(ROOT, path).unwrap());
    let mut bytes = [0; 32];
    assert_eq!(fs.read_at(&fds, fd, 0, &mut bytes), Ok(16));
    assert_eq!(&bytes[..16], b"anova.native.v1\n");
    fs.clear(&mut fds);
}

#[test]
fn shared_file_maps_observe_writes_and_pin_removed_replaced_inodes() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut fds = Descriptors::default();
    let fd = fs
        .open(
            &mut fds,
            ROOT,
            "/tmp/mapped",
            OPEN_CREATE | OPEN_READ | OPEN_WRITE,
        )
        .unwrap();
    fs.write(&fds, fd, b"original").unwrap();
    let identity = fs.metadata(ROOT, "/tmp/mapped").unwrap().0;
    let mapping = fs.shared_file_mapping(&fds, fd, 0, 4096).unwrap();
    assert_eq!(fs.seek(&fds, fd, 0, 1), Ok(8));
    let version = mapping.version();
    fs.write_at(&fds, fd, 2, b"XY").unwrap();
    let mut bytes = [0; 16];
    mapping.copy_into(0, &mut bytes);
    assert_eq!(&bytes[..8], b"orXYinal");
    assert_ne!(mapping.version(), version);
    assert_eq!(fs.set_len(&fds, fd, 4), Err(BUSY));
    assert_eq!(
        fs.open(&mut fds, ROOT, "/tmp/mapped", OPEN_WRITE | OPEN_TRUNCATE),
        Err(BUSY)
    );
    fs.set_len(&fds, fd, 12).unwrap();
    fs.write_at(&fds, fd, 10, b"Z").unwrap();
    mapping.copy_into(0, &mut bytes);
    assert_eq!(&bytes[8..12], b"\0\0Z\0");
    fs.close(&mut fds, fd).unwrap();
    fs.unlink(ROOT, "/tmp/mapped").unwrap();
    fs.collect_mappings().unwrap();
    let replacement = fs
        .open(
            &mut fds,
            ROOT,
            "/tmp/mapped",
            OPEN_CREATE | OPEN_READ | OPEN_WRITE,
        )
        .unwrap();
    fs.write(&fds, replacement, b"replacement").unwrap();
    assert_ne!(identity, fs.metadata(ROOT, "/tmp/mapped").unwrap().0);
    mapping.copy_into(0, &mut bytes);
    assert_eq!(&bytes[..8], b"orXYinal");
    drop(mapping);
    fs.collect_mappings().unwrap();
    fs.set_len(&fds, replacement, 0).unwrap();
    fs.clear(&mut fds);
    assert_eq!(fs.live_objects(), (0, 0));
}
#[test]
fn removed_directories_retain_open_identity_and_reclaim_after_final_close() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut fds = Descriptors::default();
    fs.mkdir(ROOT, "/tmp/remove").unwrap();
    let node = fs.chdir(ROOT, "/tmp/remove").unwrap();
    let directory = fs
        .open(&mut fds, ROOT, "/tmp/remove", OPEN_READ | OPEN_DIRECTORY)
        .unwrap();
    let child = fs
        .open(&mut fds, node, "child", OPEN_CREATE | OPEN_WRITE)
        .unwrap();
    assert_eq!(fs.rmdir(ROOT, "/tmp/remove", &[]), Err(NOT_EMPTY));
    fs.unlink(node, "child").unwrap();
    fs.close(&mut fds, child).unwrap();
    assert_eq!(fs.rmdir(ROOT, "/tmp/remove", &[node]), Err(BUSY));
    fs.rmdir(ROOT, "/tmp/remove", &[]).unwrap();
    assert!(fs.peek_readdir(&fds, directory).unwrap().is_none());
    assert_eq!(fs.mkdir(node, "resurrect"), Err(NOT_FOUND));
    fs.mkdir(ROOT, "/tmp/remove").unwrap();
    assert_ne!(fs.chdir(ROOT, "/tmp/remove").unwrap(), node);
    fs.close(&mut fds, directory).unwrap();
    fs.rmdir(ROOT, "/tmp/remove", &[]).unwrap();
    assert_eq!(fs.rmdir(ROOT, "/tmp", &[]), Err(BUSY));
    fs.mkdir(ROOT, "/tmp/parent").unwrap();
    fs.mkdir(ROOT, "/tmp/parent/child").unwrap();
    let held = fs
        .open(
            &mut fds,
            ROOT,
            "/tmp/parent/child",
            OPEN_READ | OPEN_DIRECTORY,
        )
        .unwrap();
    fs.rmdir(ROOT, "/tmp/parent/child", &[]).unwrap();
    assert_eq!(fs.rmdir(ROOT, "/tmp/parent", &[]), Err(BUSY));
    fs.close(&mut fds, held).unwrap();
    fs.rmdir(ROOT, "/tmp/parent", &[]).unwrap();
    for _ in 0..NODE_LIMIT + 1 {
        fs.mkdir(ROOT, "/tmp/reuse").unwrap();
        fs.rmdir(ROOT, "/tmp/reuse", &[]).unwrap();
    }
    fs.clear(&mut fds);
    assert_eq!(fs.live_objects(), (0, 0));
}
#[test]
fn extended_names_keep_native_wire_records_bounded_and_failed_reads_unconsumed() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut fds = Descriptors::default();
    fs.mkdir(ROOT, "/tmp/names").unwrap();
    let name = "n".repeat(NAME_BYTES);
    let path = alloc::format!("/tmp/names/{name}");
    let file = fs
        .open(&mut fds, ROOT, &path, OPEN_CREATE | OPEN_WRITE)
        .unwrap();
    fs.write(&fds, file, b"long name").unwrap();
    let directory = fs
        .open(&mut fds, ROOT, "/tmp/names", OPEN_READ | OPEN_DIRECTORY)
        .unwrap();
    assert!(matches!(fs.readdir(&fds, directory), Err(TOO_LONG)));
    let entry = fs.peek_readdir(&fds, directory).unwrap().unwrap();
    assert_eq!(&entry.name[..entry.name_len as usize], name.as_bytes());
    fs.advance_readdir(&fds, directory).unwrap();
    assert!(fs.peek_readdir(&fds, directory).unwrap().is_none());
    assert_eq!(
        fs.open(
            &mut fds,
            ROOT,
            &alloc::format!("{path}x"),
            OPEN_CREATE | OPEN_WRITE
        ),
        Err(TOO_LONG)
    );
    assert_eq!(
        fs.metadata(ROOT, &"n".repeat(PATH_BYTES + 1)),
        Err(TOO_LONG)
    );
    fs.clear(&mut fds);
    assert_eq!(fs.live_objects(), (0, 0));
}
#[test]
fn packet_pairs_preserve_messages_duplex_backpressure_and_last_close() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut fds = Descriptors::default();
    let [a, b] = fs.packet_pair(&mut fds).unwrap();
    fs.write(&fds, a, b"first").unwrap();
    fs.write(&fds, a, b"next").unwrap();
    let mut short = [0; 2];
    assert_eq!(fs.read(&fds, b, &mut short).unwrap(), 2);
    assert_eq!(&short, b"fi");
    let mut bytes = [0; PACKET_BYTES];
    assert_eq!(fs.read(&fds, b, &mut bytes).unwrap(), 4);
    assert_eq!(&bytes[..4], b"next");
    assert_eq!(fs.read(&fds, b, &mut bytes), Err(WOULD_BLOCK));
    fs.write(&fds, b, b"reverse").unwrap();
    assert_eq!(fs.read(&fds, a, &mut bytes).unwrap(), 7);
    assert_eq!(&bytes[..7], b"reverse");
    fs.write(&fds, a, &bytes).unwrap();
    assert_eq!(fs.write(&fds, a, b"x"), Err(WOULD_BLOCK));
    assert_eq!(fs.read(&fds, b, &mut bytes).unwrap(), PACKET_BYTES);
    fs.write(&fds, a, &[]).unwrap();
    assert!(fs.readiness(&fds, b).unwrap().0);
    assert_eq!(fs.read(&fds, b, &mut []).unwrap(), 0);
    assert_eq!(fs.read(&fds, b, &mut bytes), Err(WOULD_BLOCK));
    let mut child = fs.inherit(&fds).unwrap();
    fs.clear(&mut fds);
    assert_eq!(fs.read(&child, a, &mut bytes), Err(WOULD_BLOCK));
    fs.close(&mut child, b).unwrap();
    assert_eq!(fs.read(&child, a, &mut bytes).unwrap(), 0);
    assert_eq!(fs.write(&child, a, b"x"), Err(BROKEN_PIPE));
    fs.clear(&mut child);
    assert_eq!(fs.live_objects(), (0, 0));
    for _ in 0..PIPE_LIMIT / 2 {
        fs.packet_pair(&mut fds).unwrap();
    }
    assert_eq!(fs.packet_pair(&mut fds), Err(FILE_LIMIT));
    assert_eq!(fds.count(), PIPE_LIMIT);
    fs.clear(&mut fds);
    assert_eq!(fs.live_objects(), (0, 0));
}
#[test]
fn rename_replacement_preserves_open_bytes_and_description_lock_lifetime() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut fds = Descriptors::default();
    let a = fs
        .open(
            &mut fds,
            ROOT,
            "/tmp/a",
            OPEN_READ | OPEN_WRITE | OPEN_CREATE,
        )
        .unwrap();
    let b = fs
        .open(
            &mut fds,
            ROOT,
            "/tmp/b",
            OPEN_READ | OPEN_WRITE | OPEN_CREATE,
        )
        .unwrap();
    fs.write(&fds, a, b"new").unwrap();
    fs.write_at(&fds, a, 1, b"E").unwrap();
    assert_eq!(fs.seek(&fds, a, 0, 1).unwrap(), 3);
    let mut positioned = [0; 3];
    assert_eq!(fs.read_at(&fds, a, 0, &mut positioned).unwrap(), 3);
    assert_eq!(&positioned, b"nEw");
    assert_eq!(fs.seek(&fds, a, 0, 1).unwrap(), 3);
    fs.write(&fds, b, b"old").unwrap();
    fs.seek(&fds, b, 0, 0).unwrap();
    fs.rename(ROOT, "/tmp/a", "/tmp/b").unwrap();
    assert_eq!(fs.metadata(ROOT, "/tmp/a"), Err(NOT_FOUND));
    let new = fs.open(&mut fds, ROOT, "/tmp/b", OPEN_READ).unwrap();
    let mut bytes = [0; 3];
    assert_eq!(fs.read(&fds, b, &mut bytes).unwrap(), 3);
    assert_eq!(&bytes, b"old");
    fs.set_len(&fds, b, 2).unwrap();
    fs.set_len(&fds, b, 6).unwrap();
    assert_eq!(fs.seek(&fds, b, 0, 1).unwrap(), 3);
    let mut grown = [0xff; 6];
    assert_eq!(fs.read_at(&fds, b, 0, &mut grown).unwrap(), 6);
    assert_eq!(&grown, b"ol\0\0\0\0");
    assert_eq!(fs.set_len(&fds, b, FILE_BYTES + 1), Err(NO_SPACE));
    assert_eq!(fs.set_len(&fds, new, 0), Err(BAD_DESCRIPTOR));
    fs.try_lock(&fds, a, 2).unwrap();
    assert_eq!(fs.try_lock(&fds, new, 1), Err(WOULD_BLOCK));
    let dup = fs.dup2(&mut fds, a, 20).unwrap();
    fs.close(&mut fds, a).unwrap();
    assert_eq!(fs.try_lock(&fds, new, 2), Err(WOULD_BLOCK));
    fs.close(&mut fds, dup).unwrap();
    fs.try_lock(&fds, new, 2).unwrap();
    assert_eq!(fs.rename(ROOT, "/tmp/b", "/etc/text"), Err(READ_ONLY));
    fs.mkdir(ROOT, "/tmp/staging").unwrap();
    let directory = fs.chdir(ROOT, "/tmp/staging").unwrap();
    fs.mkdir(ROOT, "/tmp/staging/child").unwrap();
    fs.mkdir(ROOT, "/tmp/other").unwrap();
    fs.rename(ROOT, "/tmp/staging", "/tmp/installed").unwrap();
    assert_eq!(fs.chdir(ROOT, "/tmp/installed").unwrap(), directory);
    assert_eq!(fs.metadata(ROOT, "/tmp/staging"), Err(NOT_FOUND));
    assert!(fs.chdir(ROOT, "/tmp/installed/child").is_ok());
    assert_eq!(
        fs.rename(ROOT, "/tmp/installed", "/tmp/other"),
        Err(UNSUPPORTED)
    );
    assert_eq!(
        fs.rename(ROOT, "/tmp/installed", "/tmp/other/sub"),
        Err(UNSUPPORTED)
    );
    fs.clear(&mut fds);
    assert_eq!(fs.live_objects(), (0, 0));
}
#[test]
fn shared_thread_table_publishes_close_and_releases_only_on_last_owner() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut parent = fs.standard().unwrap();
    let mut thread = parent.share().unwrap();
    let fd = fs.open(&mut thread, ROOT, "/etc/text", OPEN_READ).unwrap();
    assert!(parent.contains(fd));
    fs.close(&mut parent, fd).unwrap();
    assert!(!thread.contains(fd));
    fs.clear(&mut parent);
    assert_eq!(thread.count(), 3);
    fs.clear(&mut thread);
    assert_eq!(fs.live_objects(), (0, 0));
}
#[test]
fn pty_allocation_lock_and_final_open_lifetime_are_bounded() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut fds = Descriptors::default();
    let master = fs
        .open(&mut fds, ROOT, "/dev/ptmx", OPEN_READ | OPEN_WRITE)
        .unwrap();
    assert_eq!(
        fs.device(&fds, master, false),
        Ok(Some(Device::PtyMaster(0)))
    );
    assert_eq!(
        fs.open(&mut fds, ROOT, "/dev/pts/0", OPEN_READ),
        Err(IO_ERROR)
    );
    fs.pty_lock(0, false);
    let slave = fs.open(&mut fds, ROOT, "/dev/pts/0", OPEN_READ).unwrap();
    let duplicate = fs.dup2(&mut fds, master, 12).unwrap();
    fs.close(&mut fds, master).unwrap();
    assert!(fs.pty_present(0, true));
    fs.close(&mut fds, duplicate).unwrap();
    assert!(!fs.pty_present(0, true));
    fs.close(&mut fds, slave).unwrap();
    fs.clear(&mut fds);
    assert_eq!(fs.live_objects(), (0, 0));
}
fn image() -> Vec<u8> {
    let entries: &[(u8, &str, &[u8])] = &[
        (2, "/bin", b""),
        (2, "/tmp", b""),
        (2, "/dev", b""),
        (2, "/etc", b""),
        (1, "/etc/text", b"abcdef"),
        (4, "/bin/app", b"ELF"),
        (3, "/text", b"etc/text"),
        (3, "/loop", b"loop"),
        (3, "/tmp-link", b"/tmp"),
    ];
    let mut bytes = Vec::from(&b"ANOVAFS1"[..]);
    bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (kind, path, data) in entries {
        bytes.extend_from_slice(&[*kind, 0]);
        bytes.extend_from_slice(&(path.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u64).to_le_bytes());
        bytes.extend_from_slice(path.as_bytes());
        bytes.extend_from_slice(data);
    }
    bytes
}
#[test]
fn component_walk_enforces_directory_mount_symlink_and_root_boundaries() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    assert_eq!(fs.resolve(ROOT, "/text"), fs.resolve(ROOT, "/etc/text"));
    assert_eq!(fs.resolve(ROOT, "/loop"), Err(LINK_LOOP));
    assert_eq!(fs.resolve(ROOT, "/etc/text/../text"), Err(NOT_DIRECTORY));
    assert_eq!(fs.resolve(ROOT, "/etc/text/"), Err(NOT_DIRECTORY));
    assert_eq!(fs.resolve(ROOT, "/../../.."), Ok(ROOT));
    assert_eq!(fs.resolve(ROOT, ""), Err(INVALID_ARGUMENT));
    fs.mkdir(ROOT, "/tmp-link/work").unwrap();
    let cwd = fs.chdir(ROOT, "/tmp/work").unwrap();
    fs.mkdir(cwd, "sub").unwrap();
    assert_eq!(fs.resolve(cwd, "sub/.."), Ok(cwd));
    let mut path = [0; 128];
    let n = fs.canonical(cwd, &mut path).unwrap();
    assert_eq!(&path[..n], b"/tmp/work");
    assert_eq!(fs.mkdir(ROOT, "/etc/new"), Err(READ_ONLY));
    assert_eq!(fs.executable(ROOT, "/bin/app"), Ok(&b"ELF"[..]));
    assert_eq!(fs.executable(ROOT, "/etc/text"), Err(PERMISSION_DENIED));
}
#[test]
fn independent_opens_dup_and_fork_have_precise_shared_offsets_and_lifetimes() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut p = fs.standard().unwrap();
    let a = fs.open(&mut p, ROOT, "/etc/text", OPEN_READ).unwrap();
    let b = fs.open(&mut p, ROOT, "/etc/text", OPEN_READ).unwrap();
    fs.dup2(&mut p, a, 10).unwrap();
    let mut c = fs.inherit(&p).unwrap();
    let mut out = [0; 2];
    fs.read(&p, a, &mut out).unwrap();
    assert_eq!(&out, b"ab");
    fs.read(&c, 10, &mut out).unwrap();
    assert_eq!(&out, b"cd");
    fs.read(&p, a, &mut out).unwrap();
    assert_eq!(&out, b"ef");
    fs.read(&p, b, &mut out).unwrap();
    assert_eq!(&out, b"ab");
    fs.seek(&p, a, -2, 2).unwrap();
    fs.read(&c, a, &mut out).unwrap();
    assert_eq!(&out, b"ef");
    assert_eq!(fs.seek(&p, a, -99, 0), Err(INVALID_ARGUMENT));
    assert_eq!(fs.seek(&p, 1, 0, 0), Err(NOT_SEEKABLE));
    fs.close(&mut c, 1).unwrap();
    assert!(p.contains(1));
    assert!(!c.contains(1));
    assert_eq!(fs.dup2(&mut c, 99, 0), Err(BAD_DESCRIPTOR));
    fs.clear(&mut p);
    assert!(fs.live_objects().0 > 0);
    fs.clear(&mut c);
    assert_eq!(fs.live_objects(), (0, 0));
}
#[test]
fn tmpfs_sparse_append_truncate_and_unlink_keep_existing_opens_alive() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut p = Descriptors::default();
    let a = fs
        .open(&mut p, ROOT, "/tmp/f", OPEN_READ | OPEN_WRITE | OPEN_CREATE)
        .unwrap();
    fs.seek(&p, a, 3, 0).unwrap();
    fs.write(&p, a, b"a").unwrap();
    let b = fs
        .open(&mut p, ROOT, "/tmp/f", OPEN_READ | OPEN_WRITE | OPEN_APPEND)
        .unwrap();
    fs.seek(&p, b, 0, 0).unwrap();
    fs.write(&p, b, b"bc").unwrap();
    fs.seek(&p, a, 0, 0).unwrap();
    let mut out = [0xff; 6];
    assert_eq!(fs.read(&p, a, &mut out), Ok(6));
    assert_eq!(&out, b"\0\0\0abc");
    assert_eq!(fs.write(&p, b, &vec![1; FILE_BYTES + 1]), Err(NO_SPACE));
    fs.unlink(ROOT, "/tmp/f").unwrap();
    assert_eq!(fs.resolve(ROOT, "/tmp/f"), Err(NOT_FOUND));
    fs.close(&mut p, a).unwrap();
    fs.seek(&p, b, 3, 0).unwrap();
    assert_eq!(fs.read(&p, b, &mut out), Ok(3));
    assert_eq!(&out[..3], b"abc");
    let c = fs
        .open(&mut p, ROOT, "/tmp/f", OPEN_WRITE | OPEN_CREATE)
        .unwrap();
    fs.write(&p, c, b"new").unwrap();
    fs.close(&mut p, b).unwrap();
    fs.clear(&mut p);
    assert_eq!(fs.live_objects(), (0, 0));
    let d = fs
        .open(&mut p, ROOT, "/tmp/f", OPEN_WRITE | OPEN_TRUNCATE)
        .unwrap();
    fs.close(&mut p, d).unwrap();
    let r = fs.open(&mut p, ROOT, "/tmp/f", OPEN_READ).unwrap();
    assert_eq!(fs.read(&p, r, &mut out), Ok(0));
    fs.clear(&mut p);
}
#[test]
fn pipes_block_wrap_drain_eof_and_report_broken_reader_after_final_reference() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut p = Descriptors::default();
    let [r, w] = fs.pipe(&mut p).unwrap();
    let mut c = fs.inherit(&p).unwrap();
    fs.dup2(&mut c, w, 9).unwrap();
    let mut out = [0; PIPE_BYTES];
    assert_eq!(fs.read(&p, r, &mut out), Err(WOULD_BLOCK));
    assert!(!fs.ready(&p, r, false));
    assert_eq!(fs.write(&p, w, &vec![7; PIPE_BYTES + 10]), Ok(PIPE_BYTES));
    assert_eq!(fs.write(&p, w, b"x"), Err(WOULD_BLOCK));
    assert_eq!(fs.read(&p, r, &mut out[..700]), Ok(700));
    fs.write(&c, w, &vec![8; 700]).unwrap();
    assert_eq!(fs.read(&p, r, &mut out), Ok(PIPE_BYTES));
    assert_eq!(&out[..324], &[7; 324]);
    assert_eq!(&out[324..], &[8; 700]);
    fs.close(&mut p, w).unwrap();
    fs.close(&mut c, w).unwrap();
    assert_eq!(fs.read(&p, r, &mut out), Err(WOULD_BLOCK));
    fs.close(&mut c, 9).unwrap();
    assert!(fs.ready(&p, r, false));
    assert_eq!(fs.read(&p, r, &mut out), Ok(0));
    fs.clear(&mut p);
    fs.clear(&mut c);
    assert_eq!(fs.live_objects(), (0, 0));
    let [r, w] = fs.pipe(&mut p).unwrap();
    fs.close(&mut p, r).unwrap();
    assert_eq!(fs.write(&p, w, b"x"), Err(BROKEN_PIPE));
    assert_eq!(fs.write(&p, w, b""), Ok(0));
    fs.clear(&mut p);
}
#[test]
fn descriptor_and_inode_exhaustion_roll_back_and_reuse_without_leaks() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut p = Descriptors::default();
    for fd in 0..fd::FD_LIMIT {
        assert_eq!(
            fs.open(&mut p, ROOT, "/dev/null", OPEN_WRITE),
            Ok(fd as u64)
        );
    }
    let before = fs.live_objects();
    assert_eq!(fs.pipe(&mut p), Err(FILE_LIMIT));
    assert_eq!(
        fs.open(
            &mut p,
            ROOT,
            "/tmp/must-not-exist",
            OPEN_WRITE | OPEN_CREATE
        ),
        Err(FILE_LIMIT)
    );
    assert_eq!(fs.resolve(ROOT, "/tmp/must-not-exist"), Err(NOT_FOUND));
    assert_eq!(fs.live_objects(), before);
    fs.clear(&mut p);
    for _ in 0..200 {
        let [r, w] = fs.pipe(&mut p).unwrap();
        fs.close(&mut p, r).unwrap();
        fs.close(&mut p, w).unwrap();
    }
    let mut n = 0;
    loop {
        let mut name = [b'x'; 63];
        name[0] = (n % 26) as u8 + b'a';
        name[1] = ((n / 26) % 26) as u8 + b'a';
        name[2] = (n / (26 * 26)) as u8 + b'a';
        let path = core::str::from_utf8(&name).unwrap();
        let cwd = fs.resolve(ROOT, "/tmp").unwrap();
        match fs.mkdir(cwd, path) {
            Ok(()) => n += 1,
            Err(NO_SPACE) => break,
            e => panic!("Unexpected capacity result: {e:?}"),
        }
    }
    assert!(n > 50);
    assert_eq!(fs.live_objects(), (0, 0));
}

#[test]
fn global_open_and_pipe_limits_reject_new_objects_and_recover_after_close() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut tables: [Descriptors; 4] = core::array::from_fn(|_| Descriptors::default());
    for table in &mut tables {
        for _ in 0..fd::FD_LIMIT {
            fs.open(table, ROOT, "/dev/null", OPEN_READ).unwrap();
        }
    }
    let mut extra = Descriptors::default();
    assert_eq!(fs.live_objects(), (OPEN_LIMIT, 0));
    assert_eq!(
        fs.open(
            &mut extra,
            ROOT,
            "/tmp/unpublished",
            OPEN_CREATE | OPEN_WRITE
        ),
        Err(FILE_LIMIT)
    );
    assert_eq!(fs.resolve(ROOT, "/tmp/unpublished"), Err(NOT_FOUND));
    assert_eq!(fs.pipe(&mut extra), Err(FILE_LIMIT));
    for table in &mut tables {
        fs.clear(table);
    }
    for _ in 0..PIPE_LIMIT {
        fs.pipe(&mut tables[0]).unwrap();
    }
    assert_eq!(fs.live_objects(), (PIPE_LIMIT * 2, PIPE_LIMIT));
    assert_eq!(fs.pipe(&mut extra), Err(FILE_LIMIT));
    fs.clear(&mut tables[0]);
    assert_eq!(fs.live_objects(), (0, 0));
    fs.pipe(&mut extra).unwrap();
    fs.clear(&mut extra);
    assert_eq!(fs.live_objects(), (0, 0));
}

#[test]
fn descriptor_exec_flags_are_per_slot_and_inherited_without_losing_shared_offsets() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut parent = Descriptors::default();
    let fd = fs.open(&mut parent, ROOT, "/etc/text", OPEN_READ).unwrap();
    parent.set_close_on_exec(fd, true).unwrap();
    fs.dup2(&mut parent, fd, 9).unwrap();
    assert!(!parent.close_on_exec(9).unwrap());
    let mut child = fs.inherit(&parent).unwrap();
    fs.close_exec(&mut child);
    assert!(!child.contains(fd));
    assert!(child.contains(9) && parent.contains(fd));
    let mut bytes = [0; 3];
    assert_eq!(fs.read(&parent, fd, &mut bytes), Ok(3));
    assert_eq!(&bytes, b"abc");
    assert_eq!(fs.read(&child, 9, &mut bytes), Ok(3));
    assert_eq!(&bytes, b"def");
    fs.clear(&mut parent);
    fs.clear(&mut child);
    assert_eq!(fs.live_objects(), (0, 0));
}

#[test]
fn directory_peek_does_not_consume_and_device_truncate_does_not_create_storage() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut fds = Descriptors::default();
    let directory = fs
        .open(&mut fds, ROOT, "/etc", OPEN_READ | OPEN_DIRECTORY)
        .unwrap();
    let a = fs.peek_readdir(&fds, directory).unwrap().unwrap();
    let b = fs.peek_readdir(&fds, directory).unwrap().unwrap();
    assert_eq!(a.inode, b.inode);
    assert_eq!(fs.readdir(&fds, directory).unwrap().unwrap().inode, a.inode);
    assert!(fs.peek_readdir(&fds, directory).unwrap().is_none());
    let null = fs
        .open(
            &mut fds,
            ROOT,
            "/dev/null",
            OPEN_WRITE | OPEN_TRUNCATE | OPEN_CREATE,
        )
        .unwrap();
    assert_eq!(fs.write(&fds, null, b"discard"), Ok(7));
    assert_eq!(fs.descriptor_metadata(&fds, null).unwrap().2, 0);
    assert_eq!(fs.metadata_nofollow(ROOT, "/text").unwrap().1, 3);
    assert_eq!(fs.metadata(ROOT, "/text").unwrap().1, 1);
    fs.clear(&mut fds);
}

#[test]
fn socket_description_survives_dup_fork_exec_and_shared_thread_close() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut parent = fs.standard().unwrap();
    let socket = fs.socket(&mut parent, 7).unwrap();
    parent.set_close_on_exec(socket, true).unwrap();
    let alias = fs.dup2(&mut parent, socket, 10).unwrap();
    fs.set_nonblocking(&parent, alias, true).unwrap();
    assert_eq!(fs.nonblocking(&parent, socket), Ok(true));
    let mut child = fs.inherit(&parent).unwrap();
    fs.close_exec(&mut parent);
    assert!(!parent.contains(socket));
    assert_eq!(fs.socket_id(&child, socket), Ok(Some(7)));
    fs.clear(&mut parent);
    assert!(fs.socket_live(7));
    let mut thread = child.share().unwrap();
    fs.close(&mut thread, alias).unwrap();
    assert!(!child.contains(alias));
    fs.clear(&mut child);
    assert!(fs.socket_live(7));
    fs.clear(&mut thread);
    assert!(!fs.socket_live(7));
    assert_eq!(fs.live_objects(), (0, 0));
}

#[test]
fn private_snapshots_preserve_offsets_modes_and_modification_times() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut files = fs.standard().unwrap();
    fs.set_clock(1_000_000_010);
    let fd = fs
        .open(
            &mut files,
            ROOT,
            "/tmp/object",
            OPEN_CREATE | OPEN_READ | OPEN_WRITE,
        )
        .unwrap();
    fs.fchmod(&files, fd, 0o600).unwrap();
    fs.write(&files, fd, b"object-one").unwrap();
    let inode = fs.metadata(ROOT, "/tmp/object").unwrap().0;
    assert_eq!(fs.attributes(inode), (0o600, 1_000_000_010));
    let mapping = fs.mapping_bytes(&files, fd, 0, 4096).unwrap();
    assert_eq!(fs.seek(&files, fd, 0, 1), Ok(10));
    fs.set_clock(2_000_000_020);
    fs.seek(&files, fd, 0, 0).unwrap();
    fs.write(&files, fd, b"object-two").unwrap();
    fs.chmod(ROOT, "/tmp/object", 0o755).unwrap();
    assert_eq!(&*mapping, b"object-one");
    assert_eq!(fs.attributes(inode), (0o755, 2_000_000_020));
    assert_eq!(fs.mapping_bytes(&files, fd, 11, 1), Err(INVALID_ARGUMENT));
    assert_eq!(fs.fchmod(&files, fd, 0o4755), Err(UNSUPPORTED));
    fs.close(&mut files, fd).unwrap();
}
