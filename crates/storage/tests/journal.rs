// SPDX-License-Identifier: Apache-2.0
//! Exercise actual mkfs JBD2 images with sector/flush power cuts, volatile caches, corruption and e2fsck replay.
//! Retain baselines, failed snapshots and event inventories under build/journal-host for independent inspection.
use anova_abi::*;
use anova_storage::{BlockDevice, SECTOR, ext4::Ext4, filesystem::Filesystem};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::{cell::RefCell, fs, path::PathBuf, process::Command, rc::Rc};

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
// SAFETY: Successful allocations preserve System's layout/ownership contract; injected failures return null.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if deny() {
            std::ptr::null_mut()
        } else {
            // SAFETY: GlobalAlloc supplies a valid allocation layout, unchanged here.
            unsafe { System.alloc(layout) }
        }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if deny() {
            std::ptr::null_mut()
        } else {
            // SAFETY: The original allocation layout is forwarded to System.
            unsafe { System.alloc_zeroed(layout) }
        }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if deny() {
            std::ptr::null_mut()
        } else {
            // SAFETY: The live pointer, old layout and new size satisfy the caller's realloc contract.
            unsafe { System.realloc(p, layout, size) }
        }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        // SAFETY: All successful allocations came from System with this same layout.
        unsafe { System.dealloc(p, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

#[derive(Clone)]
struct Media {
    durable: Vec<u8>,
    pending: Vec<(usize, [u8; SECTOR])>,
    cache: bool,
    cut: Option<usize>,
    events: usize,
    stopped: bool,
    tear: bool,
    trace: Vec<String>,
    record: bool,
}
impl Media {
    fn new(bytes: Vec<u8>, cache: bool) -> Self {
        Self {
            durable: bytes,
            pending: Vec::new(),
            cache,
            cut: None,
            events: 0,
            stopped: false,
            tear: false,
            trace: Vec::new(),
            record: false,
        }
    }
    fn event(&mut self, name: String) -> bool {
        self.trace.push(name);
        let cut = self.cut == Some(self.events);
        self.events += 1;
        self.stopped |= cut;
        self.stopped
    }
}
struct Disk(Rc<RefCell<Media>>);
impl BlockDevice for Disk {
    fn sectors(&self) -> u64 {
        self.0.borrow().durable.len() as u64 / SECTOR as u64
    }
    fn read_sector(&mut self, n: u64, out: &mut [u8; SECTOR]) -> Result<(), i64> {
        let m = self.0.borrow();
        if m.stopped {
            return Err(IO_ERROR);
        }
        let at = n as usize * SECTOR;
        if at + SECTOR > m.durable.len() {
            return Err(IO_ERROR);
        }
        if let Some((_, bytes)) = m.pending.iter().rev().find(|(offset, _)| *offset == at) {
            out.copy_from_slice(bytes);
        } else {
            out.copy_from_slice(&m.durable[at..at + SECTOR]);
        }
        Ok(())
    }
    fn write_sector(&mut self, n: u64, bytes: &[u8; SECTOR]) -> Result<(), i64> {
        // The device's host-only trace may allocate; the first device write ends preflight injection.
        FAIL_ALLOC.with(|v| v.set(None));
        let mut m = self.0.borrow_mut();
        if m.stopped {
            return Err(IO_ERROR);
        }
        let at = n as usize * SECTOR;
        if at + SECTOR > m.durable.len() {
            return Err(IO_ERROR);
        }
        let event = if m.record {
            format!(
                "write {n} {}",
                bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
            )
        } else {
            format!("write {n}")
        };
        if m.event(event) {
            if m.tear {
                m.durable[at..at + SECTOR / 2].copy_from_slice(&bytes[..SECTOR / 2]);
            }
            return Err(IO_ERROR);
        }
        if m.cache {
            m.pending.push((at, *bytes));
        } else {
            m.durable[at..at + SECTOR].copy_from_slice(bytes);
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), i64> {
        let mut m = self.0.borrow_mut();
        if m.stopped || m.event("flush".into()) {
            return Err(IO_ERROR);
        }
        for (at, bytes) in std::mem::take(&mut m.pending) {
            m.durable[at..at + SECTOR].copy_from_slice(&bytes);
        }
        Ok(())
    }
}
fn mount(m: &Rc<RefCell<Media>>) -> Result<Ext4, i64> {
    Ext4::mount_writable(Box::new(Disk(m.clone())))
}
fn output(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/journal-host")
        .join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    path
}
fn fixture(name: &str) -> Vec<u8> {
    let path = output(&format!("{name}/mkfs.ext4"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::File::create(&path)
        .unwrap()
        .set_len(16 * 1024 * 1024)
        .unwrap();
    let o = Command::new("mke2fs")
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
            "128",
            "-m",
            "0",
            "-O",
            "none,extent,filetype,sparse_super,large_file,has_journal",
            "-J",
            "size=4",
            "-E",
            "lazy_itable_init=0",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    fs::read(path).unwrap()
}
fn check(name: &str, bytes: &[u8], repair: bool) -> Vec<u8> {
    let path = output(name);
    fs::write(&path, bytes).unwrap();
    let o = Command::new("e2fsck")
        .args(if repair { ["-f", "-y"] } else { ["-f", "-n"] })
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        o.status.success() || repair && o.status.code() == Some(1),
        "e2fsck {}: {}{}",
        path.display(),
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    fs::read(path).unwrap()
}
fn contents(fs: &mut Ext4, name: &str) -> Option<Vec<u8>> {
    let entry = fs.lookup(2, name).unwrap()?;
    let info = fs.inode(entry.inode).unwrap();
    let mut bytes = vec![0; info.size];
    assert_eq!(fs.read(entry.inode, 0, &mut bytes), Ok(bytes.len()));
    Some(bytes)
}
fn baseline(name: &str) -> (Rc<RefCell<Media>>, u32) {
    let m = Rc::new(RefCell::new(Media::new(fixture(name), false)));
    let mut fs = mount(&m).unwrap();
    let n = fs.create(2, "old", false).unwrap();
    fs.write(n, 0, b"old persisted bytes").unwrap();
    fs.shutdown().unwrap();
    drop(fs);
    (m, n)
}

#[test]
fn journal_roundtrip_orphans_and_independent_fsck() {
    let (m, n) = baseline("roundtrip");
    let mut fs = mount(&m).unwrap();
    let escaped = [0xc0, 0x3b, 0x39, 0x98, 1, 2, 3, 4];
    fs.write(n, 0, &escaped).unwrap();
    fs.unlink(2, "old").unwrap();
    assert_eq!(fs.read(n, 0, &mut [0; 8]), Ok(8));
    fs.sync().unwrap();
    drop(fs);
    let mut fs = mount(&m).unwrap();
    assert!(fs.lookup(2, "old").unwrap().is_none());
    assert!(fs.inode(n).is_err());
    fs.shutdown().unwrap();
    check("roundtrip/recovered.ext4", &m.borrow().durable, false);
}

#[test]
fn every_sector_and_flush_cut_preserves_atomic_states() {
    let (base, n) = baseline("matrix");
    let original = base.borrow().durable.clone();
    fs::write(output("matrix/baseline.ext4"), &original).unwrap();
    let mut inventory = String::new();
    for operation in ["create", "write", "truncate", "rename", "unlink", "release"] {
        let mut initial = original.clone();
        if operation == "release" {
            let m = Rc::new(RefCell::new(Media::new(initial, false)));
            let mut fs = mount(&m).unwrap();
            fs.unlink(2, "old").unwrap();
            drop(fs);
            initial = m.borrow().durable.clone();
        }
        let perform = |fs: &mut Ext4| -> Result<(), i64> {
            match operation {
                "create" => fs.create(2, "created", false).map(|_| ()),
                "write" => fs.write(n, 0, b"new committed bytes").map(|_| ()),
                "truncate" => fs.resize(n, 3),
                "rename" => fs.rename(2, "old", 2, "new"),
                "unlink" => fs.unlink(2, "old").map(|_| ()),
                "release" => fs.release_unlinked(n),
                _ => unreachable!(),
            }
        };
        // Release an existing orphan in the live mount, without startup auto-reclamation.
        let run = |m: &Rc<RefCell<Media>>,
                   cut: Option<usize>,
                   cache: bool,
                   tear: bool|
         -> Result<(), i64> {
            if operation == "release" {
                *m.borrow_mut() = Media::new(original.clone(), cache);
            }
            let mut fs = mount(m)?;
            if operation == "release" {
                fs.unlink(2, "old")?;
            }
            {
                let mut disk = m.borrow_mut();
                disk.events = 0;
                disk.trace.clear();
                disk.cut = cut;
                disk.tear = tear;
                disk.record = cut.is_none();
            }
            let result = perform(&mut fs);
            if result.is_err() {
                assert_eq!(fs.sync(), Err(IO_ERROR));
            }
            result
        };
        let reference = Rc::new(RefCell::new(Media::new(initial.clone(), false)));
        run(&reference, None, false, false).unwrap();
        let events = reference.borrow().events;
        fs::write(
            output(&format!("matrix/{operation}-baseline.ext4")),
            &initial,
        )
        .unwrap();
        fs::write(
            output(&format!("matrix/{operation}-events.txt")),
            reference.borrow().trace.join("\n"),
        )
        .unwrap();
        for (cache, tear) in [(false, false), (true, false), (false, true)] {
            for cut in 0..events {
                let m = Rc::new(RefCell::new(Media::new(initial.clone(), cache)));
                assert_eq!(
                    run(&m, Some(cut), cache, tear),
                    Err(IO_ERROR),
                    "{operation} {cut}"
                );
                let failed = m.borrow().durable.clone();
                if cut == events / 2 {
                    let failed_path =
                        output(&format!("matrix/{operation}-{cache}-{tear}-failed.ext4"));
                    fs::write(&failed_path, &failed).unwrap();
                    let reconstructed = output(&format!(
                        "matrix/{operation}-{cache}-{tear}-reconstructed.ext4"
                    ));
                    if reconstructed.exists() {
                        fs::remove_file(&reconstructed).unwrap();
                    }
                    let mut command = Command::new("python3");
                    command
                        .arg(
                            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                                .join("../../scripts/reconstruct_journal_cut.py"),
                        )
                        .arg(output(&format!("matrix/{operation}-baseline.ext4")))
                        .arg(output(&format!("matrix/{operation}-events.txt")))
                        .arg(cut.to_string())
                        .arg(&reconstructed);
                    if cache {
                        command.arg("--cache");
                    }
                    if tear {
                        command.arg("--tear");
                    }
                    assert!(command.status().unwrap().success());
                    assert_eq!(fs::read(&reconstructed).unwrap(), failed);
                    fs::remove_file(reconstructed).unwrap();
                }
                {
                    let mut disk = m.borrow_mut();
                    disk.stopped = false;
                    disk.cut = None;
                    disk.pending.clear();
                }
                let mut fs = mount(&m).unwrap_or_else(|e| {
                    panic!("{operation} cut={cut} cache={cache} tear={tear}: {e}")
                });
                let old = contents(&mut fs, "old");
                let new = contents(&mut fs, "new");
                match operation {
                    "create" => {
                        assert_eq!(old.as_deref(), Some(b"old persisted bytes".as_slice()));
                        if let Some(bytes) = contents(&mut fs, "created") {
                            assert!(bytes.is_empty());
                        }
                    }
                    "write" => assert!(
                        old.as_deref() == Some(b"old persisted bytes")
                            || old.as_deref() == Some(b"new committed bytes")
                    ),
                    "truncate" => assert!(
                        old.as_deref() == Some(b"old persisted bytes")
                            || old.as_deref() == Some(b"old")
                    ),
                    "rename" => assert!(
                        (old.is_some() && new.is_none())
                            || (old.is_none() && new.as_deref() == Some(b"old persisted bytes"))
                    ),
                    "unlink" => {
                        assert!(old.is_none() || old.as_deref() == Some(b"old persisted bytes"))
                    }
                    "release" => assert!(old.is_none()),
                    _ => unreachable!(),
                }
                fs.shutdown().unwrap();
                // Independent allocation, namespace and orphan checks for every recoverable cut.
                check("matrix/last-recovered.ext4", &m.borrow().durable, false);
                inventory.push_str(&format!(
                    "{operation} cache={cache} tear={tear} cut={cut} replay=ok fsck=clean\n"
                ));
            }
        }
    }
    fs::write(output("matrix/results.txt"), inventory).unwrap();
}

#[test]
fn independent_replay_corruption_and_repeated_recovery_cuts() {
    let (base, n) = baseline("replay");
    let original = base.borrow().durable.clone();
    let run = |cut: Option<usize>| {
        let m = Rc::new(RefCell::new(Media::new(original.clone(), false)));
        let mut fs = mount(&m).unwrap();
        m.borrow_mut().cut = cut;
        let result = fs.write(n, 0, b"new committed bytes");
        drop(fs);
        (m, result)
    };
    let (reference, result) = run(None);
    result.unwrap();
    let flushes: Vec<_> = reference
        .borrow()
        .trace
        .iter()
        .enumerate()
        .filter(|(_, s)| s.as_str() == "flush")
        .map(|(i, _)| i)
        .collect();
    let (failed, result) = run(Some(flushes[3] + 1));
    assert_eq!(result, Err(IO_ERROR));
    let failed = failed.borrow().durable.clone();
    fs::write(output("replay/committed-before-checkpoint.ext4"), &failed).unwrap();
    let repaired = check("replay/e2fsck-replayed.ext4", &failed, true);
    let m = Rc::new(RefCell::new(Media::new(repaired, false)));
    let mut fs = mount(&m).unwrap();
    assert_eq!(
        contents(&mut fs, "old").as_deref(),
        Some(b"new committed bytes".as_slice())
    );
    fs.shutdown().unwrap();

    let recovering = Rc::new(RefCell::new(Media::new(failed.clone(), false)));
    mount(&recovering).unwrap();
    let count = recovering.borrow().events;
    for cut in 0..count {
        let m = Rc::new(RefCell::new(Media::new(failed.clone(), false)));
        m.borrow_mut().cut = Some(cut);
        assert!(mount(&m).is_err(), "recovery cut={cut}");
        {
            let mut d = m.borrow_mut();
            d.stopped = false;
            d.cut = None;
        }
        let mut fs = mount(&m).unwrap();
        assert_eq!(
            contents(&mut fs, "old").as_deref(),
            Some(b"new committed bytes".as_slice())
        );
        fs.shutdown().unwrap();
        check("replay/repeated-recovery.ext4", &m.borrow().durable, false);
    }
    // The first journal extent is in the inline journal inode, whose table block is group descriptor +8.
    let table = u32::from_le_bytes(failed[4096 + 8..4096 + 12].try_into().unwrap()) as usize;
    let journal = u32::from_le_bytes(
        failed[table * 4096 + 7 * 256 + 60..table * 4096 + 7 * 256 + 64]
            .try_into()
            .unwrap(),
    ) as usize;
    let mut positions = vec![(journal + 1) * 4096 + 4092, (journal + 2) * 4096 + 30];
    for position in journal + 1..journal + 100 {
        let at = position * 4096;
        if failed[at..at + 8] == [0xc0, 0x3b, 0x39, 0x98, 0, 0, 0, 2] {
            positions.push(at + 16);
            break;
        }
    }
    assert_eq!(positions.len(), 3);
    for (case, at) in positions.into_iter().enumerate() {
        let mut corrupt = failed.clone();
        corrupt[at] ^= 1;
        fs::write(output(&format!("replay/corrupt-{case}.ext4")), &corrupt).unwrap();
        let m = Rc::new(RefCell::new(Media::new(corrupt.clone(), false)));
        assert!(matches!(mount(&m), Err(INVALID_FILESYSTEM)));
        assert_eq!(
            m.borrow().durable,
            corrupt,
            "corruption must be rejected without writes"
        );
        assert_eq!(m.borrow().events, 0);
    }
    // A readonly mount cannot perform hidden recovery writes.
    let m = Rc::new(RefCell::new(Media::new(failed, false)));
    assert!(Ext4::mount(Box::new(Disk(m.clone()))).is_err());
    assert_eq!(m.borrow().events, 0);
}

fn reference_crc(mut seed: u32, bytes: &[u8]) -> u32 {
    for b in bytes {
        seed ^= u32::from(*b);
        for _ in 0..8 {
            seed = (seed >> 1) ^ if seed & 1 != 0 { 0x82f63b78 } else { 0 };
        }
    }
    seed
}
fn journal_start(bytes: &[u8]) -> usize {
    let table = u32::from_le_bytes(bytes[4104..4108].try_into().unwrap()) as usize;
    u32::from_le_bytes(
        bytes[table * 4096 + 7 * 256 + 60..table * 4096 + 7 * 256 + 64]
            .try_into()
            .unwrap(),
    ) as usize
}
fn pending_write(name: &str, input: &[u8]) -> Vec<u8> {
    let (m, n) = baseline(name);
    let original = m.borrow().durable.clone();
    let mut fs = mount(&m).unwrap();
    m.borrow_mut().events = 0;
    m.borrow_mut().trace.clear();
    fs.write(n, 0, input).unwrap();
    let cut = m
        .borrow()
        .trace
        .iter()
        .enumerate()
        .filter(|(_, s)| s.as_str() == "flush")
        .nth(3)
        .unwrap()
        .0
        + 1;
    drop(fs);
    let m = Rc::new(RefCell::new(Media::new(original, false)));
    let mut fs = mount(&m).unwrap();
    m.borrow_mut().cut = Some(cut);
    assert_eq!(fs.write(n, 0, input), Err(IO_ERROR));
    drop(fs);
    m.borrow().durable.clone()
}
#[test]
fn multi_descriptor_escape_and_revoke_interoperate_with_e2fsck() {
    let mut input = vec![0x5a; 300 * 4096];
    for chunk in input.chunks_exact_mut(4096) {
        chunk[..4].copy_from_slice(&[0xc0, 0x3b, 0x39, 0x98]);
    }
    let failed = pending_write("large", &input);
    fs::write(output("large/failed.ext4"), &failed).unwrap();
    for (name, bytes) in [
        ("native", failed.clone()),
        ("e2fsck", check("large/fsck.ext4", &failed, true)),
    ] {
        let m = Rc::new(RefCell::new(Media::new(bytes, false)));
        let mut fs = mount(&m).unwrap();
        assert_eq!(contents(&mut fs, "old").unwrap(), input, "{name}");
        fs.shutdown().unwrap();
        check(&format!("large/{name}.ext4"), &m.borrow().durable, false);
    }
    let mut failed = pending_write("revoke", b"new committed bytes");
    let journal = journal_start(&failed);
    let seed = reference_crc(!0, &failed[journal * 4096 + 48..journal * 4096 + 64]);
    let descriptor = (journal + 1) * 4096;
    let block = u32::from_be_bytes(failed[descriptor + 12..descriptor + 16].try_into().unwrap());
    // For an existing-file overwrite the first tag is its data, which this valid revoke cancels.
    let end = (journal + 2..journal + 100)
        .find(|n| failed[n * 4096..n * 4096 + 8] == [0xc0, 0x3b, 0x39, 0x98, 0, 0, 0, 2])
        .unwrap()
        * 4096;
    let commit = failed[end..end + 4096].to_vec();
    failed[end + 4096..end + 8192].copy_from_slice(&commit);
    failed[end..end + 4096].fill(0);
    failed[end..end + 12].copy_from_slice(&commit[..12]);
    failed[end + 4..end + 8].copy_from_slice(&5u32.to_be_bytes());
    failed[end + 12..end + 16].copy_from_slice(&20u32.to_be_bytes());
    failed[end + 16..end + 20].copy_from_slice(&block.to_be_bytes());
    let sum = reference_crc(seed, &failed[end..end + 4096]);
    failed[end + 4092..end + 4096].copy_from_slice(&sum.to_be_bytes());
    fs::write(output("revoke/failed.ext4"), &failed).unwrap();
    for bytes in [failed.clone(), check("revoke/fsck.ext4", &failed, true)] {
        let m = Rc::new(RefCell::new(Media::new(bytes, false)));
        let mut fs = mount(&m).unwrap();
        assert_eq!(
            contents(&mut fs, "old").as_deref(),
            Some(b"old persisted bytes".as_slice())
        );
        fs.shutdown().unwrap();
        check("revoke/recovered.ext4", &m.borrow().durable, false);
    }
}

#[test]
fn journal_preflight_allocation_failures_leave_the_mount_retryable() {
    let (base, n) = baseline("oom");
    let original = base.borrow().durable.clone();
    let mut allocations = 0;
    for index in 0..1000 {
        let m = Rc::new(RefCell::new(Media::new(original.clone(), false)));
        let mut fs = mount(&m).unwrap();
        FAIL_ALLOC.with(|v| v.set(Some(index)));
        let result = fs.write(n, 0, b"new committed bytes");
        FAIL_ALLOC.with(|v| v.set(None));
        if result.is_ok() {
            allocations = index;
            break;
        }
        assert_eq!(result, Err(NO_MEMORY), "allocation {index}");
        assert_eq!(m.borrow().durable, original);
        assert_eq!(
            contents(&mut fs, "old").as_deref(),
            Some(b"old persisted bytes".as_slice())
        );
        fs.write(n, 0, b"new committed bytes").unwrap();
        fs.shutdown().unwrap();
    }
    assert!(allocations > 10 && allocations < 1000);
    fs::write(
        output("oom/results.txt"),
        format!("preflight_allocations={allocations} rollback=exact retry=ok\n"),
    )
    .unwrap();
}
