// SPDX-License-Identifier: Apache-2.0
//! Compare real e2fsprogs extent images and sparse bytes; inject malformed metadata and device errors.
//! Readonly devices count attempted writes, so rejection and successful reads must be non-mutating.
use anova_abi::*;
use anova_storage::{
    BlockDevice, SECTOR, check_range,
    ext4::{BLOCK, Ext4},
    filesystem::Filesystem,
};
use std::{
    cell::Cell,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    process::Command,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Disk {
    file: File,
    sectors: u64,
    writes: Rc<Cell<usize>>,
    fail: Rc<Cell<bool>>,
}
impl BlockDevice for Disk {
    fn sectors(&self) -> u64 {
        self.sectors
    }
    fn read_sector(&mut self, n: u64, output: &mut [u8; SECTOR]) -> Result<(), i64> {
        check_range(self.sectors, n, SECTOR)?;
        if self.fail.get() {
            return Err(IO_ERROR);
        }
        self.file
            .seek(SeekFrom::Start(n * SECTOR as u64))
            .and_then(|_| self.file.read_exact(output))
            .map_err(|_| IO_ERROR)
    }
    fn write_sector(&mut self, _: u64, _: &[u8; SECTOR]) -> Result<(), i64> {
        self.writes.set(self.writes.get() + 1);
        Err(READ_ONLY)
    }
    fn flush(&mut self) -> Result<(), i64> {
        self.writes.set(self.writes.get() + 1);
        Err(READ_ONLY)
    }
}
struct Fixture {
    work: PathBuf,
    image: PathBuf,
    writes: Rc<Cell<usize>>,
    fail: Rc<Cell<bool>>,
}
static NEXT: AtomicUsize = AtomicUsize::new(0);
impl Fixture {
    fn new(large: bool) -> Self {
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/ext4-host")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        let source = work.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join(".anova-abi"), b"anova.native.v1\n").unwrap();
        fs::write(source.join("sample"), b"ext4 exact sample\n").unwrap();
        fs::create_dir(source.join("nested")).unwrap();
        fs::write(source.join("nested/child"), b"child bytes").unwrap();
        if large {
            let mut file = File::create(source.join("large")).unwrap();
            for n in 0..5120 {
                file.write_all(&[1 + (n % 251) as u8; BLOCK]).unwrap();
            }
            let mut sparse = File::create(source.join("sparse")).unwrap();
            sparse.set_len(200 * 1024 * 1024).unwrap();
            sparse.write_all(b"first").unwrap();
            sparse
                .seek(SeekFrom::Start(199 * 1024 * 1024 + 17))
                .unwrap();
            sparse.write_all(b"last").unwrap();
        }
        let image = work.join("disk.ext4");
        File::create(&image)
            .unwrap()
            .set_len(64 * 1024 * 1024)
            .unwrap();
        let output = Command::new("mke2fs")
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
                "1024",
                "-N",
                "8192",
                "-m",
                "0",
                "-O",
                "none,extent,filetype,sparse_super,large_file",
                "-E",
                "lazy_itable_init=0",
                "-d",
            ])
            .arg(source)
            .arg(&image)
            .output()
            .expect("e2fsprogs is required for ext4 acceptance");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Self {
            work,
            image,
            writes: Rc::new(Cell::new(0)),
            fail: Rc::new(Cell::new(false)),
        }
    }
    fn device(&self) -> Box<dyn BlockDevice> {
        let file = File::open(&self.image).unwrap();
        let sectors = file.metadata().unwrap().len() / SECTOR as u64;
        Box::new(Disk {
            file,
            sectors,
            writes: self.writes.clone(),
            fail: self.fail.clone(),
        })
    }
    fn mount(&self) -> Ext4 {
        Ext4::mount(self.device()).unwrap()
    }
    fn patch(&self, offset: u64, bytes: &[u8]) -> Vec<u8> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.image)
            .unwrap();
        file.seek(SeekFrom::Start(offset)).unwrap();
        let mut old = vec![0; bytes.len()];
        file.read_exact(&mut old).unwrap();
        file.seek(SeekFrom::Start(offset)).unwrap();
        file.write_all(bytes).unwrap();
        old
    }
    fn clean(&self) {
        let output = Command::new("e2fsck")
            .args(["-f", "-n"])
            .arg(&self.image)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(self.writes.get(), 0);
    }
    fn inode_offset(&self, number: u32) -> u64 {
        let mut file = File::open(&self.image).unwrap();
        let mut bytes = [0; 4];
        file.seek(SeekFrom::Start(1024 + 40)).unwrap();
        file.read_exact(&mut bytes).unwrap();
        let per_group = u32::from_le_bytes(bytes);
        file.seek(SeekFrom::Start(
            BLOCK as u64 + u64::from((number - 1) / per_group) * 32 + 8,
        ))
        .unwrap();
        file.read_exact(&mut bytes).unwrap();
        u64::from(u32::from_le_bytes(bytes)) * BLOCK as u64
            + u64::from((number - 1) % per_group) * 256
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(&self.work).unwrap();
        }
    }
}

#[test]
fn multi_group_external_extents_large_and_sparse_bytes_match_host() {
    let f = Fixture::new(true);
    f.clean();
    let mut disk = f.mount();
    let large = disk.lookup(2, "large").unwrap().unwrap().inode;
    assert_eq!(disk.inode(large).unwrap().size, 20 * 1024 * 1024);
    let layout = Command::new("debugfs")
        .args(["-R", "stat /large"])
        .arg(&f.image)
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&layout.stdout).contains("ETB0"),
        "Fixture must use an external extent block"
    );
    let mut original = File::open(f.work.join("source/large")).unwrap();
    let mut actual = vec![0; 137_173];
    let mut expected = vec![0; actual.len()];
    let mut offset = 0;
    loop {
        let n = original.read(&mut expected).unwrap();
        assert_eq!(disk.read(large, offset, &mut actual), Ok(n));
        assert_eq!(&actual[..n], &expected[..n]);
        if n == 0 {
            break;
        }
        offset += n;
    }
    let sparse = disk.lookup(2, "sparse").unwrap().unwrap().inode;
    assert_eq!(disk.inode(sparse).unwrap().size, 200 * 1024 * 1024);
    for at in [
        0,
        4093,
        128 * 1024 * 1024,
        199 * 1024 * 1024 + 13,
        200 * 1024 * 1024 - 16,
    ] {
        let mut expected = [0; 16];
        let mut actual = [9; 16];
        let mut source = File::open(f.work.join("source/sparse")).unwrap();
        source.seek(SeekFrom::Start(at)).unwrap();
        source.read_exact(&mut expected).unwrap();
        assert_eq!(disk.read(sparse, at as usize, &mut actual), Ok(16));
        assert_eq!(actual, expected);
    }
    disk.shutdown().unwrap();
    drop(disk);
    f.clean();
}

#[test]
fn readonly_semantics_directory_cursors_and_io_errors_do_not_mutate() {
    let f = Fixture::new(false);
    let before = fs::read(&f.image).unwrap();
    let mut disk = f.mount();
    let file = disk.lookup(2, "sample").unwrap().unwrap().inode;
    assert!(disk.capabilities().read_only);
    assert_eq!(disk.write(file, 0, b"bad"), Err(READ_ONLY));
    assert_eq!(disk.create(2, "bad", false), Err(READ_ONLY));
    assert_eq!(disk.rename(2, "sample", 2, "bad"), Err(READ_ONLY));
    assert_eq!(disk.resize(file, 0), Err(READ_ONLY));
    assert_eq!(disk.unlink(2, "sample"), Err(READ_ONLY));
    assert_eq!(disk.next_entry(2, 4).err(), Some(INVALID_ARGUMENT));
    let mut cursor = 0;
    let mut names = Vec::new();
    while let Some((entry, next)) = disk.next_entry(2, cursor).unwrap() {
        assert!(next > cursor);
        cursor = next;
        names.push(String::from_utf8(entry.name[..entry.length].to_vec()).unwrap());
    }
    names.sort();
    assert_eq!(names, [".anova-abi", "lost+found", "nested", "sample"]);
    let nested = disk.lookup(2, "nested").unwrap().unwrap().inode;
    let child = disk.lookup(nested, "child").unwrap().unwrap().inode;
    let mut bytes = [0; 32];
    assert_eq!(disk.read(child, 0, &mut bytes), Ok(11));
    assert_eq!(&bytes[..11], b"child bytes");
    f.fail.set(true);
    assert_eq!(disk.read(child, 0, &mut bytes), Err(IO_ERROR));
    f.fail.set(false);
    disk.sync().unwrap();
    drop(disk);
    assert_eq!(fs::read(&f.image).unwrap(), before);
    f.clean();
}

#[test]
fn unknown_features_dirty_journal_geometry_and_metadata_aliases_are_rejected() {
    let f = Fixture::new(false);
    for (at, bytes) in [
        (1024 + 58, 0u16.to_le_bytes().to_vec()),
        (1024 + 96, 0x46u32.to_le_bytes().to_vec()),
        (1024 + 96, 0x10042u32.to_le_bytes().to_vec()),
        (1024 + 100, 0x403u32.to_le_bytes().to_vec()),
        (1024 + 92, 0x24u32.to_le_bytes().to_vec()),
        (1024 + 32, 0u32.to_le_bytes().to_vec()),
        (1024 + 4, u32::MAX.to_le_bytes().to_vec()),
        (1024 + 40, u32::MAX.to_le_bytes().to_vec()),
        (1024 + 88, 128u16.to_le_bytes().to_vec()),
        (BLOCK + 8, 2u32.to_le_bytes().to_vec()),
        (BLOCK + 18, 2u16.to_le_bytes().to_vec()),
    ] {
        let old = f.patch(at as u64, &bytes);
        assert!(
            Ext4::mount(f.device()).is_err(),
            "Mutation at {at} was accepted"
        );
        f.patch(at as u64, &old);
    }
    let mut image = File::open(&f.image).unwrap();
    let mut descriptor = [0; 32];
    image.seek(SeekFrom::Start(BLOCK as u64)).unwrap();
    image.read_exact(&mut descriptor).unwrap();
    let table = u32::from_le_bytes(descriptor[8..12].try_into().unwrap()) as usize * BLOCK;
    let bitmap = u32::from_le_bytes(descriptor[..4].try_into().unwrap());
    // Root extent addresses its own block bitmap: allocated, in range, but never file data.
    let at = table + 256 + 60;
    let old = f.patch(at as u64, &bitmap.to_le_bytes());
    assert!(Ext4::mount(f.device()).is_err());
    f.patch(at as u64, &old);
    f.clean();
}

#[test]
fn extent_cycles_overlaps_and_unallocated_references_are_rejected() {
    let f = Fixture::new(true);
    let mut disk = f.mount();
    let number = disk.lookup(2, "large").unwrap().unwrap().inode;
    drop(disk);
    let inode = f.inode_offset(number);
    let mut file = File::open(&f.image).unwrap();
    let mut root = [0; 60];
    file.seek(SeekFrom::Start(inode + 40)).unwrap();
    file.read_exact(&mut root).unwrap();
    assert_eq!(u16::from_le_bytes(root[6..8].try_into().unwrap()), 1);
    let leaf = u32::from_le_bytes(root[16..20].try_into().unwrap());
    let leaf_offset = u64::from(leaf) * BLOCK as u64;
    let old = f.patch(leaf_offset + 24, &0u32.to_le_bytes());
    assert!(f.mount().inode(number).is_err());
    f.patch(leaf_offset + 24, &old);
    let depth = f.patch(inode + 46, &2u16.to_le_bytes());
    let mut cycle = [0; 24];
    cycle[0..2].copy_from_slice(&0xf30au16.to_le_bytes());
    cycle[2..4].copy_from_slice(&1u16.to_le_bytes());
    cycle[4..6].copy_from_slice(&340u16.to_le_bytes());
    cycle[6..8].copy_from_slice(&1u16.to_le_bytes());
    cycle[16..20].copy_from_slice(&leaf.to_le_bytes());
    let old = f.patch(leaf_offset, &cycle);
    assert!(f.mount().inode(number).is_err());
    f.patch(leaf_offset, &old);
    f.patch(inode + 46, &depth);
    let mut first = [0; 4];
    file.seek(SeekFrom::Start(leaf_offset + 20)).unwrap();
    file.read_exact(&mut first).unwrap();
    let physical = u32::from_le_bytes(first);
    let group = physical / 1024;
    file.seek(SeekFrom::Start(BLOCK as u64 + u64::from(group) * 32))
        .unwrap();
    file.read_exact(&mut first).unwrap();
    let bitmap = u64::from(u32::from_le_bytes(first)) * BLOCK as u64;
    let bit_offset = bitmap + u64::from(physical % 1024 / 8);
    let mut old = [0; 1];
    file.seek(SeekFrom::Start(bit_offset)).unwrap();
    file.read_exact(&mut old).unwrap();
    f.patch(bit_offset, &[old[0] & !(1 << (physical % 8))]);
    assert!(f.mount().inode(number).is_err());
    f.patch(bit_offset, &old);
    f.clean();
}

#[test]
fn unwritten_extents_never_expose_existing_device_bytes() {
    let f = Fixture::new(false);
    for command in [
        "write /dev/null /unwritten",
        "fallocate /unwritten 0 0",
        "set_inode_field /unwritten size 4096",
    ] {
        let output = Command::new("debugfs")
            .args(["-w", "-R", command])
            .arg(&f.image)
            .output()
            .unwrap();
        assert!(output.status.success());
    }
    let mut disk = f.mount();
    let number = disk.lookup(2, "unwritten").unwrap().unwrap().inode;
    drop(disk);
    let mut file = File::open(&f.image).unwrap();
    let mut extent = [0; 12];
    file.seek(SeekFrom::Start(f.inode_offset(number) + 52))
        .unwrap();
    file.read_exact(&mut extent).unwrap();
    assert_eq!(u16::from_le_bytes(extent[4..6].try_into().unwrap()), 32769);
    let block = u32::from_le_bytes(extent[8..12].try_into().unwrap());
    f.patch(u64::from(block) * BLOCK as u64, &[0xa5; BLOCK]);
    let mut disk = f.mount();
    let mut bytes = [0xff; BLOCK];
    assert_eq!(disk.read(number, 0, &mut bytes), Ok(BLOCK));
    assert_eq!(bytes, [0; BLOCK]);
    drop(disk);
    f.clean();
}
