// SPDX-License-Identifier: Apache-2.0
//! Exercise real ext2 bytes, timestamp/reboot invariants, allocation and injected device failures.
//! The in-memory sector device shares its backing state only with this deterministic test harness.
use anova_abi::*;
use anova_storage::{
    BlockDevice, SECTOR, check_range,
    ext2::{BLOCK, Ext2},
};
use std::{cell::RefCell, rc::Rc};

struct State {
    bytes: Vec<u8>,
    writes: usize,
    fail_write: Option<usize>,
    fail_flush: bool,
}
#[test]
fn explicit_timestamps_preserve_omitted_fields_and_survive_clean_reboot() {
    let device = fixture();
    let mut fs = mount(&device);
    fs.set_clock(9000);
    let inode = fs.create(2, "timestamp", false).unwrap();
    fs.write(inode, 0, b"T").unwrap();
    fs.set_times(inode, Some(111), Some(222)).unwrap();
    let info = fs.inode(inode).unwrap();
    assert_eq!(
        (info.accessed, info.modified, info.changed),
        (111, 222, 9000)
    );
    fs.set_clock(10000);
    fs.set_times(inode, None, Some(333)).unwrap();
    fs.shutdown().unwrap();
    drop(fs);
    let mut fs = mount(&device);
    let info = fs.inode(inode).unwrap();
    assert_eq!(
        (info.accessed, info.modified, info.changed),
        (111, 333, 10000)
    );
    fs.set_clock(11000);
    fs.write(inode, 0, b"U").unwrap();
    assert_eq!(fs.inode(inode).unwrap().modified, 11000);
    let writes = device.0.borrow().writes;
    device.0.borrow_mut().fail_write = Some(writes + 1);
    assert_eq!(fs.set_times(inode, Some(1), Some(2)), Err(IO_ERROR));
    assert_eq!(fs.set_times(inode, Some(3), Some(4)), Err(IO_ERROR));
}
#[derive(Clone)]
struct Memory(Rc<RefCell<State>>);
impl BlockDevice for Memory {
    fn sectors(&self) -> u64 {
        (self.0.borrow().bytes.len() / SECTOR) as u64
    }
    fn read_sector(&mut self, sector: u64, output: &mut [u8; SECTOR]) -> Result<(), i64> {
        check_range(self.sectors(), sector, SECTOR)?;
        let start = sector as usize * SECTOR;
        output.copy_from_slice(&self.0.borrow().bytes[start..start + SECTOR]);
        Ok(())
    }
    fn write_sector(&mut self, sector: u64, input: &[u8; SECTOR]) -> Result<(), i64> {
        check_range(self.sectors(), sector, SECTOR)?;
        let mut s = self.0.borrow_mut();
        s.writes += 1;
        if s.fail_write == Some(s.writes) {
            return Err(IO_ERROR);
        }
        let start = sector as usize * SECTOR;
        s.bytes[start..start + SECTOR].copy_from_slice(input);
        Ok(())
    }
    fn flush(&mut self) -> Result<(), i64> {
        if self.0.borrow().fail_flush {
            Err(IO_ERROR)
        } else {
            Ok(())
        }
    }
}
fn p16(bytes: &mut [u8], at: usize, n: u16) {
    bytes[at..at + 2].copy_from_slice(&n.to_le_bytes());
}
fn p32(bytes: &mut [u8], at: usize, n: u32) {
    bytes[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn fixture() -> Memory {
    let mut b = vec![0; 128 * BLOCK];
    let s = &mut b[1024..2048];
    for (at, n) in [
        (0, 128),
        (4, 128),
        (12, 119),
        (16, 118),
        (24, 2),
        (28, 2),
        (32, 128),
        (36, 128),
        (40, 128),
        (76, 1),
        (84, 11),
        (96, 2),
    ] {
        p32(s, at, n);
    }
    p16(s, 56, 0xef53);
    p16(s, 58, 1);
    p16(s, 88, 128);
    let g = &mut b[BLOCK..BLOCK + 32];
    for (at, n) in [(0, 2), (4, 3), (8, 4)] {
        p32(g, at, n);
    }
    p16(g, 12, 119);
    p16(g, 14, 118);
    p16(g, 16, 1);
    for i in 0..9 {
        b[2 * BLOCK + i / 8] |= 1 << (i % 8);
    }
    for i in 0..10 {
        b[3 * BLOCK + i / 8] |= 1 << (i % 8);
    }
    let root = &mut b[4 * BLOCK + 128..4 * BLOCK + 256];
    p16(root, 0, 0x41ed);
    p32(root, 4, BLOCK as u32);
    p16(root, 26, 2);
    p32(root, 28, 8);
    p32(root, 40, 8);
    let dir = &mut b[8 * BLOCK..9 * BLOCK];
    p32(dir, 0, 2);
    p16(dir, 4, 12);
    dir[6] = 1;
    dir[7] = 2;
    dir[8] = b'.';
    p32(dir, 12, 2);
    p16(dir, 16, (BLOCK - 12) as u16);
    dir[18] = 2;
    dir[19] = 2;
    dir[20..22].copy_from_slice(b"..");
    Memory(Rc::new(RefCell::new(State {
        bytes: b,
        writes: 0,
        fail_write: None,
        fail_flush: false,
    })))
}
fn mount(device: &Memory) -> Ext2 {
    Ext2::mount(Box::new(device.clone())).unwrap()
}

#[test]
fn direct_indirect_sparse_truncate_unlink_and_clean_reboot() {
    let device = fixture();
    let mut fs = mount(&device);
    let directory = fs.create(2, "src", true).unwrap();
    let inode = fs.create(directory, "source.c", false).unwrap();
    let bytes: Vec<_> = (0..13 * BLOCK + 17).map(|n| (n % 251) as u8).collect();
    fs.write(inode, 0, &bytes).unwrap();
    fs.write(inode, 16 * BLOCK + 5, b"tail").unwrap();
    let mut output = vec![0; 17 * BLOCK];
    let n = fs.read(inode, 0, &mut output).unwrap();
    assert_eq!(&output[..bytes.len()], bytes.as_slice());
    assert!(output[bytes.len()..16 * BLOCK + 5].iter().all(|b| *b == 0));
    assert_eq!(&output[n - 4..n], b"tail");
    fs.set_executable(inode).unwrap();
    fs.sync().unwrap();
    assert!(
        Ext2::mount(Box::new(device.clone())).is_err(),
        "Live dirty filesystem must not remount"
    );
    fs.shutdown().unwrap();
    drop(fs);
    let mut fs = mount(&device);
    assert!(fs.inode(inode).unwrap().executable);
    assert_eq!(fs.entries(directory).unwrap()[0].inode, inode);
    fs.truncate(inode).unwrap();
    assert_eq!(fs.read(inode, 0, &mut output).unwrap(), 0);
    fs.write(inode, 9, b"again").unwrap();
    fs.unlink(directory, "source.c").unwrap();
    assert_eq!(fs.read(inode, 9, &mut output[..5]).unwrap(), 5);
    fs.release_unlinked(inode).unwrap();
    assert_eq!(fs.create(directory, "reused.c", false).unwrap(), inode);
    fs.shutdown().unwrap();
    drop(fs);
    mount(&device);
}
#[test]
fn corrupt_profiles_capacity_and_metadata_aliases_are_rejected_without_writes() {
    for (offset, value) in [
        (1024 + 56, 0),
        (1024 + 24, 1),
        (1024 + 88, 0),
        (1024 + 92, 1),
        (1024 + 96, 0),
        (1024 + 100, 2),
        (1024 + 58, 0),
        (BLOCK + 8, 5),
        (4 * BLOCK + 128 + 40, 2),
        (8 * BLOCK + 4, 0),
    ] {
        let device = fixture();
        device.0.borrow_mut().bytes[offset] = value;
        let mut fs = Ext2::mount(Box::new(device.clone()));
        // Directory record corruption is checked while importing the namespace.
        if offset == 8 * BLOCK + 4 {
            assert!(fs.as_mut().unwrap().entries(2).is_err());
        } else {
            assert!(fs.is_err(), "Offset {offset}");
        }
        assert_eq!(device.0.borrow().writes, 0);
    }
    let device = fixture();
    device.0.borrow_mut().bytes.truncate(64 * BLOCK);
    assert!(Ext2::mount(Box::new(device)).is_err());
}
#[test]
fn full_disk_and_file_limits_preserve_existing_data_and_reuse_space() {
    let device = fixture();
    let mut fs = mount(&device);
    let file = fs.create(2, "full", false).unwrap();
    fs.write(file, 0, b"keep").unwrap();
    assert_eq!(fs.write(file, usize::MAX, b"overflow"), Err(NO_SPACE));
    assert_eq!(fs.write(file, 0, &vec![7; 128 * BLOCK]), Err(NO_SPACE));
    let mut bytes = [0; 4];
    assert_eq!(fs.read(file, 0, &mut bytes).unwrap(), 4);
    assert_eq!(&bytes, b"keep");
    fs.write(file, 0, &vec![5; 110 * BLOCK]).unwrap();
    let next = fs.create(2, "other", false).unwrap();
    assert_eq!(fs.write(next, 0, &vec![3; 16 * BLOCK]), Err(NO_SPACE));
    fs.truncate(file).unwrap();
    fs.write(next, 0, &vec![3; 16 * BLOCK]).unwrap();
    fs.shutdown().unwrap();
    drop(fs);
    mount(&device);
}
#[test]
fn partial_write_error_poisoning_and_dirty_reboot_are_explicit() {
    let device = fixture();
    let mut fs = mount(&device);
    let inode = fs.create(2, "file", false).unwrap();
    {
        let mut state = device.0.borrow_mut();
        state.fail_write = Some(state.writes + 4);
    }
    assert_eq!(fs.write(inode, 0, b"data"), Err(IO_ERROR));
    device.0.borrow_mut().fail_write = None;
    assert_eq!(fs.write(inode, 0, b"retry"), Err(IO_ERROR));
    assert_eq!(fs.shutdown(), Err(IO_ERROR));
    drop(fs);
    assert!(Ext2::mount(Box::new(device)).is_err());
}
#[test]
fn flush_failure_never_reports_durability_success() {
    let device = fixture();
    let mut fs = mount(&device);
    fs.create(2, "dirty", false).unwrap();
    device.0.borrow_mut().fail_flush = true;
    assert_eq!(fs.sync(), Err(IO_ERROR));
    assert_eq!(fs.shutdown(), Err(IO_ERROR));
    device.0.borrow_mut().fail_flush = false;
    drop(fs);
    assert!(Ext2::mount(Box::new(device)).is_err());
}
#[test]
fn sector_boundaries_and_overflow_are_checked() {
    assert_eq!(check_range(8, 7, 512), Ok(()));
    for (start, length) in [(8, 512), (7, 1024), (u64::MAX, 512), (0, 0), (0, 513)] {
        assert!(check_range(8, start, length).is_err());
    }
}
