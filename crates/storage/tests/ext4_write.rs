// SPDX-License-Identifier: Apache-2.0
//! Verify real writable e2fsprogs images, sparse/namespace semantics, exhaustion and commit failure boundaries.
//! A thread-local failing allocator exercises every preflight allocation without affecting parallel tests.
//! Allocation observations are checked against stopped counters and rejected transactions.
use anova_abi::*;
use anova_storage::{
    BlockDevice, SECTOR, check_range,
    ext4::{BLOCK, Ext4},
    filesystem::Filesystem,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    process::Command,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

thread_local! { static FAIL_ALLOC: Cell<Option<usize>> = const { Cell::new(None) }; }
struct Allocator;
fn deny() -> bool {
    FAIL_ALLOC
        .try_with(|v| match v.get() {
            Some(0) => true,
            Some(n) => {
                v.set(Some(n - 1));
                false
            }
            None => false,
        })
        .unwrap_or(false)
}
// SAFETY: Every successful allocation/deallocation delegates the unchanged layout to System;
// injected failures return null, and the thread-local counter owns no heap allocation.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if deny() {
            core::ptr::null_mut()
        } else {
            // SAFETY: GlobalAlloc's caller supplies a valid allocation layout.
            unsafe { System.alloc(layout) }
        }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if deny() {
            core::ptr::null_mut()
        } else {
            // SAFETY: The original layout is forwarded to the matching allocator.
            unsafe { System.alloc_zeroed(layout) }
        }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if deny() {
            core::ptr::null_mut()
        } else {
            // SAFETY: The pointer/layout/new-size contract is inherited from GlobalAlloc.
            unsafe { System.realloc(p, layout, size) }
        }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        // SAFETY: Successful allocations use System with this exact layout.
        unsafe { System.dealloc(p, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

#[derive(Default)]
struct Faults {
    reads: Cell<usize>,
    writes: Cell<usize>,
    flushes: Cell<usize>,
    read_at: Cell<Option<usize>>,
    write_at: Cell<Option<usize>>,
    flush_at: Cell<Option<usize>>,
}
struct Disk {
    file: File,
    faults: Rc<Faults>,
    readonly: bool,
}
impl BlockDevice for Disk {
    fn sectors(&self) -> u64 {
        self.file.metadata().unwrap().len() / SECTOR as u64
    }
    fn read_only(&self) -> bool {
        self.readonly
    }
    fn read_sector(&mut self, n: u64, out: &mut [u8; SECTOR]) -> Result<(), i64> {
        check_range(self.sectors(), n, SECTOR)?;
        let count = self.faults.reads.get();
        self.faults.reads.set(count + 1);
        if self.faults.read_at.get() == Some(count) {
            return Err(IO_ERROR);
        }
        self.file
            .seek(SeekFrom::Start(n * SECTOR as u64))
            .and_then(|_| self.file.read_exact(out))
            .map_err(|_| IO_ERROR)
    }
    fn write_sector(&mut self, n: u64, input: &[u8; SECTOR]) -> Result<(), i64> {
        check_range(self.sectors(), n, SECTOR)?;
        let count = self.faults.writes.get();
        self.faults.writes.set(count + 1);
        if self.readonly {
            return Err(READ_ONLY);
        }
        if self.faults.write_at.get() == Some(count) {
            return Err(IO_ERROR);
        }
        self.file
            .seek(SeekFrom::Start(n * SECTOR as u64))
            .and_then(|_| self.file.write_all(input))
            .map_err(|_| IO_ERROR)
    }
    fn flush(&mut self) -> Result<(), i64> {
        let count = self.faults.flushes.get();
        self.faults.flushes.set(count + 1);
        if self.faults.flush_at.get() == Some(count) {
            return Err(IO_ERROR);
        }
        self.file.sync_all().map_err(|_| IO_ERROR)
    }
}
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    path: PathBuf,
    faults: Rc<Faults>,
}
impl Fixture {
    fn new() -> Self {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/ext4-write-host")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("disk.ext4");
        File::create(&path)
            .unwrap()
            .set_len(8 * 1024 * 1024)
            .unwrap();
        let out = Command::new("mke2fs")
            .env("E2FSPROGS_FAKE_TIME", "1700000000")
            .args([
                "-q",
                "-t",
                "ext4",
                "-b",
                "4096",
                "-I",
                "256",
                "-g",
                "256",
                "-N",
                "128",
                "-m",
                "0",
                "-O",
                "none,extent,filetype,sparse_super,large_file",
                "-E",
                "lazy_itable_init=0",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        Self {
            path,
            faults: Rc::new(Faults::default()),
        }
    }
    fn device(&self, readonly: bool) -> Box<dyn BlockDevice> {
        Box::new(Disk {
            file: OpenOptions::new()
                .read(true)
                .write(!readonly)
                .open(&self.path)
                .unwrap(),
            faults: self.faults.clone(),
            readonly,
        })
    }
    fn mount(&self) -> Ext4 {
        Ext4::mount_writable(self.device(false)).unwrap()
    }
    fn clean(&self) {
        let out = Command::new("e2fsck")
            .args(["-f", "-n"])
            .arg(&self.path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    fn state(&self) -> u16 {
        let b = fs::read(&self.path).unwrap();
        u16::from_le_bytes(b[1082..1084].try_into().unwrap())
    }
}
fn bytes(fs: &mut Ext4, n: u32) -> Vec<u8> {
    let mut output = vec![0; fs.inode(n).unwrap().size];
    assert_eq!(fs.read(n, 0, &mut output).unwrap(), output.len());
    output
}

#[test]
fn allocation_peaks_include_freed_files_and_exclude_rejected_changes() {
    let fixture = Fixture::new();
    let mut disk = fixture.mount();
    let baseline = disk.usage().unwrap();
    let node = disk.create(2, "temporary", false).unwrap();
    disk.write(node, 0, &[42; BLOCK * 3]).unwrap();
    let high = disk.usage().unwrap();
    assert!(high.allocated_blocks >= baseline.allocated_blocks + 3);
    assert_eq!(high.allocated_inodes, baseline.allocated_inodes + 1);
    FAIL_ALLOC.with(|f| f.set(Some(0)));
    let rejected = disk.create(2, "rejected", false);
    FAIL_ALLOC.with(|f| f.set(None));
    assert_eq!(rejected, Err(NO_MEMORY));
    assert_eq!(disk.usage(), Some(high));
    let removed = disk.unlink(2, "temporary").unwrap();
    disk.release_unlinked(removed).unwrap();
    let released = disk.usage().unwrap();
    assert_eq!(released.allocated_blocks, baseline.allocated_blocks);
    assert_eq!(released.allocated_inodes, baseline.allocated_inodes);
    assert_eq!(released.peak_blocks, high.peak_blocks);
    assert_eq!(released.peak_inodes, high.peak_inodes);
    disk.shutdown().unwrap();
    let bytes = fs::read(&fixture.path).unwrap();
    let counter = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!(released.allocated_blocks, counter(1028) - counter(1036));
    assert_eq!(released.allocated_inodes, counter(1024) - counter(1040));
    drop(disk);
    fixture.clean();
    let remounted = fixture.mount().usage().unwrap();
    assert_eq!(remounted.peak_blocks, remounted.allocated_blocks);
    assert_eq!(remounted.peak_inodes, remounted.allocated_inodes);
}

#[test]
fn multigroup_sparse_resize_permissions_timestamps_and_reboot_pass_fsck() {
    let f = Fixture::new();
    let mut fs = f.mount();
    fs.set_clock(1700001234);
    let n = fs.create(2, "file", false).unwrap();
    let data: Vec<u8> = (0..1500 * 1024).map(|n| (n % 251) as u8).collect();
    assert_eq!(fs.write(n, 0, &data), Ok(data.len()));
    assert_eq!(bytes(&mut fs, n), data);
    fs.resize(n, 17).unwrap();
    fs.resize(n, 200 * 1024 * 1024).unwrap();
    fs.write(n, 199 * 1024 * 1024 + 7, b"tail").unwrap();
    let mut zero = [1; BLOCK];
    assert_eq!(fs.read(n, 4096, &mut zero), Ok(BLOCK));
    assert_eq!(zero, [0; BLOCK]);
    assert_eq!(fs.read(n, 0, &mut zero), Ok(BLOCK));
    assert_eq!(&zero[..17], &data[..17]);
    assert!(zero[17..].iter().all(|b| *b == 0));
    fs.set_permissions(n, 0o751).unwrap();
    fs.set_times(n, Some(1234), Some(5678)).unwrap();
    let info = fs.inode(n).unwrap();
    assert_eq!(
        (info.permissions, info.accessed, info.modified, info.changed),
        (0o751, 1234, 5678, 1700001234)
    );
    fs.shutdown().unwrap();
    drop(fs);
    f.clean();
    let mut fs = f.mount();
    assert_eq!(fs.inode(n).unwrap().size, 200 * 1024 * 1024);
    assert_eq!(fs.inode(n).unwrap().permissions, 0o751);
    let mut tail = [0; 4];
    fs.read(n, 199 * 1024 * 1024 + 7, &mut tail).unwrap();
    assert_eq!(&tail, b"tail");
    fs.truncate(n).unwrap();
    fs.write(n, 0, b"new").unwrap();
    fs.resize(n, 1).unwrap();
    fs.write(n, 2, b"z").unwrap();
    assert_eq!(bytes(&mut fs, n), b"n\0z");
    fs.shutdown().unwrap();
    f.clean();
}

#[test]
fn directory_growth_moves_replacement_and_unlinked_inodes_retain_identity() {
    let f = Fixture::new();
    let mut fs = f.mount();
    fs.set_clock(1700001234);
    let a = fs.create(2, "a", true).unwrap();
    let b = fs.create(2, "b", true).unwrap();
    let sub = fs.create(a, "sub", true).unwrap();
    let n = fs.create(sub, "original", false).unwrap();
    fs.write(n, 0, b"old descriptor").unwrap();
    fs.rename(a, "sub", b, "moved").unwrap();
    assert_eq!(fs.lookup(b, "moved").unwrap().unwrap().inode, sub);
    assert_eq!(fs.rename(2, "b", sub, "cycle"), Err(INVALID_ARGUMENT));
    let replacement = fs.create(sub, "replacement", false).unwrap();
    fs.write(replacement, 0, b"retained").unwrap();
    fs.rename(sub, "original", sub, "replacement").unwrap();
    assert_eq!(bytes(&mut fs, replacement), b"retained");
    fs.write(replacement, 8, b"!").unwrap();
    assert_eq!(bytes(&mut fs, replacement), b"retained!");
    assert_eq!(fs.shutdown(), Err(BUSY));
    fs.release_unlinked(replacement).unwrap();
    assert_eq!(fs.unlink(sub, "replacement"), Ok(n));
    assert_eq!(bytes(&mut fs, n), b"old descriptor");
    let fresh = fs.create(sub, "replacement", false).unwrap();
    assert_ne!(fresh, n);
    fs.release_unlinked(n).unwrap();
    for i in 0..45 {
        fs.create(sub, &format!("{i:03}-{}", "x".repeat(210)), false)
            .unwrap();
    }
    assert!(fs.inode(sub).unwrap().size > BLOCK);
    assert_eq!(fs.rmdir(b, "moved"), Err(NOT_EMPTY));
    let empty = fs.create(a, "empty", true).unwrap();
    let other = fs.create(a, "other", true).unwrap();
    fs.rename(a, "empty", a, "other").unwrap();
    fs.release_unlinked(other).unwrap();
    assert_eq!(fs.rmdir(a, "other"), Ok(empty));
    fs.release_unlinked(empty).unwrap();
    fs.shutdown().unwrap();
    drop(fs);
    f.clean();
    let _ = f.mount();
}

#[test]
fn block_and_inode_exhaustion_are_reversible_and_reusable() {
    let f = Fixture::new();
    let mut fs = f.mount();
    let n = fs.create(2, "full", false).unwrap();
    let data = vec![b'F'; 32 * BLOCK];
    let mut size = 0;
    loop {
        match fs.write(n, size, &data) {
            Ok(take) => size += take,
            Err(NO_SPACE) => break,
            other => panic!("Unexpected result: {other:?}"),
        }
    }
    let old = std::fs::read(&f.path).unwrap();
    assert_eq!(fs.write(n, size, &data), Err(NO_SPACE));
    assert_eq!(std::fs::read(&f.path).unwrap(), old);
    assert_eq!(fs.inode(n).unwrap().size, size);
    assert!(bytes(&mut fs, n).iter().all(|b| *b == b'F'));
    fs.truncate(n).unwrap();
    fs.write(n, 0, &data).unwrap();
    let mut created = Vec::new();
    loop {
        let name = format!("i{}", created.len());
        match fs.create(2, &name, false) {
            Ok(n) => created.push((name, n)),
            Err(NO_SPACE) => break,
            other => panic!("Unexpected result: {other:?}"),
        }
    }
    let (name, n) = created.pop().unwrap();
    fs.unlink(2, &name).unwrap();
    fs.release_unlinked(n).unwrap();
    assert_eq!(fs.create(2, "reuse", false), Ok(n));
    fs.shutdown().unwrap();
    f.clean();
    let _ = f.mount();
}

#[test]
fn every_preflight_allocation_failure_preserves_disk_and_mount_usability() {
    let f = Fixture::new();
    let mut fs = f.mount();
    let n = fs.create(2, "old", false).unwrap();
    fs.write(n, 0, b"old bytes").unwrap();
    for operation in 0..6 {
        let baseline = std::fs::read(&f.path).unwrap();
        let mut failures = 0;
        let mut completed = false;
        for after in 0..300 {
            FAIL_ALLOC.with(|v| v.set(Some(after)));
            let result = match operation {
                0 => fs.create(2, "new", true).map(|_| ()),
                1 => fs.write(n, 11, &[b'X'; 3 * BLOCK]).map(|_| ()),
                2 => fs.resize(n, 7),
                3 => fs.rename(2, "old", 2, "renamed"),
                4 => fs.unlink(2, "renamed").map(|_| ()),
                _ => fs.release_unlinked(n),
            };
            FAIL_ALLOC.with(|v| v.set(None));
            if result == Ok(()) {
                completed = true;
                break;
            }
            assert_eq!(
                result,
                Err(NO_MEMORY),
                "Operation {operation}, allocation {after}"
            );
            failures += 1;
            assert_eq!(std::fs::read(&f.path).unwrap(), baseline);
            fs.sync().unwrap();
        }
        assert!(failures > 0);
        assert!(
            completed,
            "Operation {operation} did not reach its successful allocation boundary"
        );
    }
    fs.shutdown().unwrap();
    f.clean();
}

#[test]
fn read_write_and_flush_faults_have_explicit_recovery_boundaries() {
    let f = Fixture::new();
    let mut fs = f.mount();
    let baseline = std::fs::read(&f.path).unwrap();
    f.faults.read_at.set(Some(f.faults.reads.get()));
    assert_eq!(fs.create(2, "read-failure", false), Err(IO_ERROR));
    assert_eq!(std::fs::read(&f.path).unwrap(), baseline);
    f.faults.read_at.set(None);
    let n = fs.create(2, "ok", false).unwrap();
    fs.shutdown().unwrap();
    drop(fs);
    f.clean();
    let mut fs = f.mount();
    f.faults.write_at.set(Some(f.faults.writes.get() + 8));
    assert_eq!(fs.write(n, 0, b"failed transfer"), Err(IO_ERROR));
    assert_eq!(fs.sync(), Err(IO_ERROR));
    assert_eq!(fs.shutdown(), Err(IO_ERROR));
    assert_eq!(fs.inode(n).err(), Some(IO_ERROR));
    drop(fs);
    assert_eq!(f.state(), 0);
    assert_eq!(
        Ext4::mount(f.device(false))
            .err()
            .map(|_| INVALID_FILESYSTEM),
        Some(INVALID_FILESYSTEM)
    );
    let f = Fixture::new();
    let mut fs = f.mount();
    let n = fs.create(2, "flush", false).unwrap();
    f.faults.flush_at.set(Some(f.faults.flushes.get()));
    assert_eq!(fs.write(n, 0, b"flush fails"), Err(IO_ERROR));
    assert_eq!(fs.shutdown(), Err(IO_ERROR));
    assert_eq!(f.state(), 0);
}

#[test]
fn writable_is_explicit_and_readonly_devices_never_receive_writes() {
    let f = Fixture::new();
    let mut ro = Ext4::mount(f.device(false)).unwrap();
    assert_eq!(ro.create(2, "no", false), Err(READ_ONLY));
    ro.shutdown().unwrap();
    assert_eq!(f.faults.writes.get(), 0);
    assert_eq!(f.faults.flushes.get(), 0);
    assert!(matches!(
        Ext4::mount_writable(f.device(true)),
        Err(READ_ONLY)
    ));
}

#[test]
fn fragmented_external_extents_unwritten_conversion_and_tree_reuse_pass_fsck() {
    let f = Fixture::new();
    let mut fs = f.mount();
    fs.set_clock(1700001234);
    let n = fs.create(2, "fragmented", false).unwrap();
    for i in 0..50 {
        fs.write(n, i * 2 * BLOCK + 7, &[i as u8 + 1; 31]).unwrap();
    }
    fs.shutdown().unwrap();
    drop(fs);
    f.clean();
    let mut fs = f.mount();
    for i in 0..50 {
        let mut out = [0; 64];
        fs.read(n, i * 2 * BLOCK, &mut out).unwrap();
        assert_eq!(&out[..7], &[0; 7]);
        assert_eq!(&out[7..38], &[i as u8 + 1; 31]);
    }
    fs.resize(n, 17).unwrap();
    fs.write(n, BLOCK + 3, b"tree block reuse").unwrap();
    let unwritten = fs.create(2, "unwritten", false).unwrap();
    fs.resize(unwritten, 20 * BLOCK).unwrap();
    fs.shutdown().unwrap();
    drop(fs);
    let out = Command::new("debugfs")
        .args(["-w", "-R", "fallocate /unwritten 0 7"])
        .arg(&f.path)
        .output()
        .unwrap();
    assert!(out.status.success());
    let mut fs = f.mount();
    fs.write(unwritten, 2 * BLOCK + 7, b"initialized").unwrap();
    let data = bytes(&mut fs, unwritten);
    assert!(data[..2 * BLOCK + 7].iter().all(|b| *b == 0));
    assert_eq!(&data[2 * BLOCK + 7..2 * BLOCK + 18], b"initialized");
    assert!(data[2 * BLOCK + 18..].iter().all(|b| *b == 0));
    fs.shutdown().unwrap();
    drop(fs);
    f.clean();
    let _ = f.mount();
}

#[test]
fn stage_limit_and_shutdown_clean_marker_flush_failure_are_reported() {
    let f = Fixture::new();
    let mut fs = f.mount();
    let n = fs.create(2, "bounded", false).unwrap();
    let before = std::fs::read(&f.path).unwrap();
    assert_eq!(fs.write(n, 0, &vec![7; 5 * 1024 * 1024]), Err(NO_MEMORY));
    assert_eq!(std::fs::read(&f.path).unwrap(), before);
    assert_eq!(fs.inode(n).unwrap().size, 0);
    // Shutdown first flushes pending data, then flushes the clean marker; fail that second flush.
    f.faults.flush_at.set(Some(f.faults.flushes.get() + 1));
    assert_eq!(fs.shutdown(), Err(IO_ERROR));
    assert_eq!(f.state(), 0);
    assert_eq!(fs.shutdown(), Err(IO_ERROR));
}

#[test]
fn every_preflight_read_failure_discards_staged_directory_creation() {
    let f = Fixture::new();
    let mut fs = f.mount();
    let before = std::fs::read(&f.path).unwrap();
    let mut failures = 0;
    let mut complete = false;
    for after in 0..2000 {
        f.faults.read_at.set(Some(f.faults.reads.get() + after));
        let result = fs.create(2, "created", true);
        f.faults.read_at.set(None);
        if result.is_ok() {
            complete = true;
            break;
        }
        assert_eq!(result, Err(IO_ERROR));
        failures += 1;
        assert_eq!(std::fs::read(&f.path).unwrap(), before);
        assert!(fs.lookup(2, "created").unwrap().is_none());
        fs.sync().unwrap();
    }
    assert!(complete && failures > 16);
    fs.shutdown().unwrap();
    f.clean();
}

#[test]
fn writable_admission_rejects_cross_inode_aliases_and_inaccurate_free_counts() {
    let f = Fixture::new();
    let mut fs = f.mount();
    let a = fs.create(2, "first", false).unwrap();
    let b = fs.create(2, "second", false).unwrap();
    fs.write(a, 0, b"first").unwrap();
    fs.write(b, 0, b"second").unwrap();
    fs.shutdown().unwrap();
    drop(fs);
    let original = std::fs::read(&f.path).unwrap();
    let u32_at =
        |bytes: &[u8], at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let address = |n: u32| {
        let group = (n - 1) / 16;
        let table = u32_at(&original, BLOCK + group as usize * 32 + 8);
        table as usize * BLOCK + ((n - 1) % 16) as usize * 256
    };
    let mut alias = original.clone();
    let physical = u32_at(&alias, address(a) + 60);
    alias[address(b) + 60..address(b) + 64].copy_from_slice(&physical.to_le_bytes());
    std::fs::write(&f.path, &alias).unwrap();
    let writes = f.faults.writes.get();
    assert!(matches!(
        Ext4::mount_writable(f.device(false)),
        Err(INVALID_FILESYSTEM)
    ));
    assert_eq!(f.faults.writes.get(), writes);
    assert_eq!(std::fs::read(&f.path).unwrap(), alias);
    let mut counters = original;
    counters[1036] ^= 1;
    std::fs::write(&f.path, &counters).unwrap();
    assert!(Ext4::mount(f.device(false)).is_ok());
    assert!(matches!(
        Ext4::mount_writable(f.device(false)),
        Err(INVALID_FILESYSTEM)
    ));
    assert_eq!(std::fs::read(&f.path).unwrap(), counters);
}
