// SPDX-License-Identifier: Apache-2.0
//! Migrate a retained readonly ext2 image to fresh JBD2 ext4 and accept actual builds with UART latency budgets.
//! Compare paths, bytes, modes and second-resolution times; retain source disks, inventories and failed outputs.
use super::*;
use anova_storage::{
    BlockDevice, SECTOR,
    ext2::Ext2,
    ext4::Ext4,
    filesystem::{Filesystem, Metadata},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom, Write},
};

struct Source {
    file: File,
    sectors: u64,
}
impl Source {
    fn open(path: &Path) -> Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_file() || !metadata.len().is_multiple_of(512) {
            return Err("Migration source must be a regular disk image".into());
        }
        Ok(Self {
            file: File::open(path)?,
            sectors: metadata.len() / 512,
        })
    }
}
impl BlockDevice for Source {
    fn sectors(&self) -> u64 {
        self.sectors
    }
    fn read_only(&self) -> bool {
        true
    }
    fn read_sector(
        &mut self,
        sector: u64,
        output: &mut [u8; SECTOR],
    ) -> std::result::Result<(), i64> {
        anova_storage::check_range(self.sectors, sector, SECTOR)?;
        self.file
            .seek(SeekFrom::Start(sector * 512))
            .and_then(|_| self.file.read_exact(output))
            .map_err(|_| anova_abi::IO_ERROR)
    }
    fn write_sector(&mut self, _: u64, _: &[u8; SECTOR]) -> std::result::Result<(), i64> {
        Err(anova_abi::READ_ONLY)
    }
    fn flush(&mut self) -> std::result::Result<(), i64> {
        Ok(())
    }
}
fn storage(e: i64) -> Box<dyn Error> {
    format!("M16 storage error: {e}").into()
}
fn inventory(
    fs: &mut dyn Filesystem,
    parent: u32,
    prefix: &str,
    result: &mut BTreeMap<String, Value>,
    depth: usize,
) -> Result<()> {
    if depth > 64 || result.len() > 16384 {
        return Err("Migration inventory limit exceeded".into());
    }
    let mut cursor = 0;
    while let Some((entry, next)) = fs.next_entry(parent, cursor).map_err(storage)? {
        if next <= cursor {
            return Err("Invalid migration cursor".into());
        }
        cursor = next;
        let name = std::str::from_utf8(&entry.name[..entry.length])?;
        if matches!(name, "." | "..") {
            continue;
        }
        let path = format!("{prefix}/{name}");
        let info = fs.inode(entry.inode).map_err(storage)?;
        let hash = if info.directory {
            None
        } else {
            let mut bytes = vec![0; info.size];
            if fs.read(entry.inode, 0, &mut bytes).map_err(storage)? != bytes.len() {
                return Err("Short migration read".into());
            }
            Some(bootstrap::hash(&bytes))
        };
        result.insert(path.clone(), json!({"directory":info.directory,"mode":info.permissions,"atime":info.accessed,"mtime":info.modified,"ctime":info.changed,"sha256":hash}));
        if info.directory {
            inventory(fs, entry.inode, &path, result, depth + 1)?;
        }
    }
    Ok(())
}
fn copy_tree(
    source: &mut dyn Filesystem,
    destination: &mut Ext4,
    p: u32,
    d: u32,
    depth: usize,
) -> Result<()> {
    if depth > 64 {
        return Err("Migration tree depth exceeded".into());
    }
    let mut cursor = 0;
    while let Some((entry, next)) = source.next_entry(p, cursor).map_err(storage)? {
        if next <= cursor {
            return Err("Invalid migration source cursor".into());
        }
        cursor = next;
        let name = std::str::from_utf8(&entry.name[..entry.length])?;
        if matches!(name, "." | "..") {
            continue;
        }
        let info = source.inode(entry.inode).map_err(storage)?;
        let node = if let Some(existing) = destination.lookup(d, name).map_err(storage)? {
            if destination
                .inode(existing.inode)
                .map_err(storage)?
                .directory
                != info.directory
            {
                return Err("Migration type mismatch".into());
            }
            existing.inode
        } else {
            destination
                .create(d, name, info.directory)
                .map_err(storage)?
        };
        if info.directory {
            copy_tree(source, destination, entry.inode, node, depth + 1)?;
        } else {
            destination.resize(node, 0).map_err(storage)?;
            let mut bytes = vec![0; 64 * 1024];
            let mut offset = 0;
            while offset < info.size {
                let take = bytes.len().min(info.size - offset);
                if source
                    .read(entry.inode, offset, &mut bytes[..take])
                    .map_err(storage)?
                    != take
                    || destination
                        .write(node, offset, &bytes[..take])
                        .map_err(storage)?
                        != take
                {
                    return Err("Short migration transfer".into());
                }
                offset += take;
            }
        }
        restore_metadata(destination, node, info)?;
    }
    Ok(())
}
fn restore_metadata(fs: &mut Ext4, n: u32, info: Metadata) -> Result<()> {
    fs.set_clock(info.changed);
    fs.set_permissions(n, info.permissions).map_err(storage)?;
    fs.set_times(n, Some(info.accessed), Some(info.modified))
        .map_err(storage)
}
pub fn migrate(root: &Path, source: &Path, destination: &Path) -> Result<Value> {
    let before = bootstrap::hash(&fs::read(source)?);
    foundation_journal::check(source)?;
    let mut source_fs = Ext2::mount(Box::new(Source::open(source)?)).map_err(storage)?;
    let mut files = BTreeMap::new();
    inventory(&mut source_fs, 2, "", &mut files, 0)?;
    let abi = source_fs
        .lookup(2, ".anova-abi")
        .map_err(storage)?
        .ok_or("Missing migration ABI marker")?
        .inode;
    let mut marker = [0; 64];
    let length = source_fs.read(abi, 0, &mut marker).map_err(storage)?;
    let linux = match &marker[..length] {
        b"linux.x86_64.v1\n" => true,
        b"anova.native.v1\n" => false,
        _ => return Err("Unsupported migration ABI marker".into()),
    };
    foundation_journal::create(root, destination, linux)?;
    let mut dest =
        Ext4::mount_writable(Box::new(disk::Image::open(destination)?)).map_err(storage)?;
    copy_tree(&mut source_fs, &mut dest, 2, 2, 0)?;
    restore_metadata(
        &mut dest,
        2,
        Filesystem::inode(&mut source_fs, 2).map_err(storage)?,
    )?;
    dest.shutdown().map_err(storage)?;
    drop(dest);
    foundation_journal::check(destination)?;
    let mut dest = Ext4::mount(Box::new(Source::open(destination)?)).map_err(storage)?;
    let mut migrated = BTreeMap::new();
    inventory(&mut dest, 2, "", &mut migrated, 0)?;
    if files != migrated || before != bootstrap::hash(&fs::read(source)?) {
        return Err("Migration inventory/source preservation mismatch".into());
    }
    let report = json!({"source":source,"source_sha256":before,"destination":destination,"destination_sha256":bootstrap::hash(&fs::read(destination)?),"abi":std::str::from_utf8(&marker[..length])?,"files":files});
    fs::write(
        destination.with_extension("migration.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}

// Budgets are declared before the run and measured with host wall time over the actual UART path.
const INPUT_MS: u128 = 3000;
const SAVE_MS: u128 = 5000;
const INTERRUPT_MS: u128 = 5000;
const BUILD_SECONDS: u64 = 1200;
fn responsive(root: &Path, iso: &Path, path: &Path, name: &str, interrupt: bool) -> Result<Value> {
    let logs = directory(root, "logs")?;
    let vars = Temporary::new(root, "m16-responsive")?;
    let serial = logs.join(format!("{name}.serial.log"));
    let mut command = qemu_disk(root, iso, &vars.0, false, Some(path), false)?;
    command
        .args(["-serial", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(File::create(logs.join(format!("{name}.qemu.log")))?);
    let mut guest = Guest(command.spawn()?);
    let mut output = transcript::Transcript::capture(
        guest.0.stdout.take().ok_or("Missing response UART")?,
        File::create(&serial)?,
    )?;
    let mut input = guest.0.stdin.take().ok_or("Missing response UART input")?;
    let mut cursor = 0;
    let mut measurements = BTreeMap::new();
    let start = Instant::now();
    let mut exchange = |marker: &str, request: &[u8], expected: &str, budget: u128| -> Result<()> {
        let ready = Instant::now();
        loop {
            output.poll()?;
            if let Some(at) = output.find_from(cursor, marker) {
                cursor = at + marker.len();
                break;
            }
            if guest.0.try_wait()?.is_some() || ready.elapsed().as_secs() > 120 {
                return Err(
                    format!("Missing responsive marker {marker}: {}", serial.display()).into(),
                );
            }
            thread::sleep(Duration::from_millis(5));
        }
        let sent = Instant::now();
        input.write_all(request)?;
        input.flush()?;
        loop {
            output.poll()?;
            if output.text().contains("[M16:WORKLOAD:FAIL]") || output.text().contains("[PANIC]") {
                return Err(format!("Workload failed: {}", serial.display()).into());
            }
            if let Some(at) = output.find_from(cursor, expected) {
                cursor = at + expected.len();
                let elapsed = sent.elapsed().as_millis();
                if elapsed > budget {
                    return Err(
                        format!("UART budget exceeded: {expected} {elapsed}>{budget} ms").into(),
                    );
                }
                measurements.insert(expected.to_owned(), elapsed);
                return Ok(());
            }
            if guest.0.try_wait()?.is_some() || sent.elapsed().as_millis() > budget {
                return Err(format!("Response timed out: {expected}: {}", serial.display()).into());
            }
            thread::sleep(Duration::from_millis(5));
        }
    };
    exchange(
        "anova> ",
        if interrupt {
            b"m16-workload interrupt\r"
        } else {
            b"m16-workload build\r"
        },
        "[M16:ACTIVE] compiler_log=true storage_writer=true",
        120_000,
    )?;
    exchange("", b"ping\n", "[M16:PING] executed=true", INPUT_MS)?;
    exchange("", b"save\n", "[M16:SAVE] fsync=true rename=true", SAVE_MS)?;
    exchange("", b"log\n", "[M16:LOG] compiler=true", INPUT_MS)?;
    exchange(
        "",
        if interrupt { b"interrupt\n" } else { b"wait\n" },
        if interrupt {
            "[M16:INTERRUPT] group_reaped=true"
        } else {
            "[M16:BUILD] exit=0"
        },
        if interrupt {
            INTERRUPT_MS
        } else {
            u128::from(BUILD_SECONDS) * 1000
        },
    )?;
    exchange("anova> ", b"status\r", "[SHELL:STATUS] code=0", INPUT_MS)?;
    exchange(
        "anova> ",
        b"shutdown\r",
        "[POWER:OFF] synced=true clean=true dma_reset=true",
        SAVE_MS,
    )?;
    let deadline = Instant::now();
    loop {
        if let Some(status) = guest.0.try_wait()? {
            if !status.success() {
                return Err("Responsive guest poweroff failed".into());
            }
            break;
        }
        if deadline.elapsed().as_secs() > 10 {
            return Err("Responsive guest did not exit".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
    output.finish()?;
    vars.cleanup()?;
    let serial = output.text();
    let resources = bootstrap::resource_summary(&serial)?;
    if resources["heap_peak_bytes"].parse::<u64>()? > 16 * 1024 * 1024 {
        return Err("Workload exceeded the declared heap budget".into());
    }
    foundation_journal::check(path)?;
    Ok(
        json!({"uart_wall_ms":measurements,"wall_ms":start.elapsed().as_millis(),"resources":resources}),
    )
}
pub fn verify(root: &Path) -> Result<()> {
    let identity = foundation::source_identity(root)?;
    command(Command::new("python3").arg(root.join("ports/bootstrap/prepare.py")))?;
    let built = foundation_journal::image_profile(root, true)?;
    let work = directory(root, "foundation")?.join(format!(
        "migration-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir(&work)?;
    let iso = work.join("anova-migration.iso");
    fs::copy(built, &iso)?;
    let source = work.join("source.ext2");
    disk::create_profile(root, &source, true)?;
    let mut old = Ext2::mount(Box::new(disk::Image::open(&source)?)).map_err(storage)?;
    let parent = old.create(2, "bootstrap", true).map_err(storage)?;
    for entry in fs::read_dir(root.join("build/bootstrap/source"))? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            return Err("Unexpected prepared build input".into());
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Non-UTF8 build input")?;
        let n = old.create(parent, &name, false).map_err(storage)?;
        old.write(n, 0, &fs::read(entry.path())?).map_err(storage)?;
    }
    old.shutdown().map_err(storage)?;
    drop(old);
    let destination = work.join("work.ext4");
    let migration = migrate(root, &source, &destination)?;
    disk::session_timeout(
        root,
        &iso,
        &source,
        "foundation-ext2-recovery",
        true,
        None,
        &[
            ("anova> ", b"cat /persist/.anova-abi\r"),
            ("linux.x86_64.v1\r\n", b"shutdown\r"),
        ],
        true,
        120,
    )?;
    let budgets = json!({"foreground_ms":INPUT_MS,"save_ms":SAVE_MS,"interrupt_ms":INTERRUPT_MS,"build_seconds":BUILD_SECONDS,"heap_bytes":16777216,"vfs_slots":2048,"tasks":32,"fds":32,"cpus":1,"ram_bytes":1073741824u64,"accelerator":"tcg"});
    fs::write(
        work.join("budgets.json"),
        serde_json::to_vec_pretty(&budgets)?,
    )?;
    let build = responsive(root, &iso, &destination, "foundation-build", false)?;
    let mut disk = Ext4::mount(Box::new(Source::open(&destination)?)).map_err(storage)?;
    let workspace = disk
        .lookup(2, "bootstrap")
        .map_err(storage)?
        .ok_or("Missing migrated workspace")?
        .inode;
    let tool = disk
        .lookup(workspace, "bzip2")
        .map_err(storage)?
        .ok_or("Missing guest-built bzip2")?
        .inode;
    let info = disk.inode(tool).map_err(storage)?;
    let mut elf = vec![0; info.size];
    disk.read(tool, 0, &mut elf).map_err(storage)?;
    anova_elf::parse_linux(&elf).map_err(|e| format!("Guest-built bzip2 ELF: {e:?}"))?;
    if !info.executable {
        return Err("Guest-built tool is not executable".into());
    }
    fs::write(work.join("bzip2.elf"), &elf)?;
    drop(disk);
    let interrupt = responsive(root, &iso, &destination, "foundation-interrupt", true)?;
    let mut steps = Vec::new();
    bootstrap::checked(&mut steps, "cd /persist/bootstrap", 0);
    bootstrap::checked(
        &mut steps,
        "./bzip2 -1 -kc sample1.ref > sample1.out.bz2",
        0,
    );
    bootstrap::checked(&mut steps, "./bzip2 -dc sample1.out.bz2 > roundtrip", 0);
    bootstrap::checked(&mut steps, "make -f Makefile.anova fail", 2);
    bootstrap::checked(&mut steps, "./bzip2 -t sample1.out.bz2", 0);
    steps.push(("anova> ".into(), b"shutdown\r".to_vec()));
    let exchanges: Vec<_> = steps
        .iter()
        .map(|(m, b)| (m.as_str(), b.as_slice()))
        .collect();
    disk::session_timeout(
        root,
        &iso,
        &destination,
        "foundation-migration-reboot",
        false,
        None,
        &exchanges,
        true,
        BUILD_SECONDS,
    )?;
    let rejection =
        fs::read_to_string(root.join("build/logs/foundation-migration-reboot.serial.log"))?;
    if !rejection.contains("rejected.c:")
        || !rejection.contains("error: #error M15_intentional_build_rejection")
    {
        return Err("The failed build did not reach the deliberate compiler rejection".into());
    }
    foundation_journal::check(&destination)?;
    let mut disk = Ext4::mount(Box::new(Source::open(&destination)?)).map_err(storage)?;
    let mut final_files = BTreeMap::new();
    inventory(&mut disk, 2, "", &mut final_files, 0)?;
    if final_files
        .get("/bootstrap/bzip2")
        .and_then(|v| v["sha256"].as_str())
        != Some(bootstrap::hash(&elf).as_str())
    {
        return Err("Interrupted/failed build changed the accepted executable".into());
    }
    if final_files["/bootstrap/roundtrip"]["sha256"]
        != final_files["/bootstrap/sample1.ref"]["sha256"]
        || final_files["/bootstrap/sample1.out.bz2"]["sha256"]
            != final_files["/bootstrap/sample1.bz2"]["sha256"]
    {
        return Err("Reconstructed bzip2 bytes differ from independent official fixtures".into());
    }
    if final_files["/foreground.txt"]["sha256"]
        != bootstrap::hash(b"foreground saved during guest compilation\n")
    {
        return Err("Foreground save did not survive reboot".into());
    }
    for (path, expected) in migration["files"]
        .as_object()
        .ok_or("Missing migration inputs")?
    {
        if expected["directory"] == false && final_files[path]["sha256"] != expected["sha256"] {
            return Err(format!("Migrated source changed during the build: {path}").into());
        }
    }
    if migration["source_sha256"] != bootstrap::hash(&fs::read(&source)?) {
        return Err("Migration source changed".into());
    }
    for name in [
        "foundation-ext2-recovery",
        "foundation-build",
        "foundation-interrupt",
        "foundation-migration-reboot",
    ] {
        for suffix in ["serial.log", "qemu.log"] {
            fs::copy(
                root.join(format!("build/logs/{name}.{suffix}")),
                work.join(format!("{name}.{suffix}")),
            )?;
        }
    }
    if foundation::source_identity(root)? != identity {
        return Err("Migration sources changed during acceptance".into());
    }
    fs::write(
        work.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"profile":"m16-bzip2-jbd2-v1","implementation_sha256":identity,"migration":migration,"budgets":budgets,"build":build,"interrupt":interrupt,"final_files":final_files,"tool_sha256":bootstrap::hash(&elf),"iso_sha256":bootstrap::hash(&fs::read(iso)?),"compiler_bootstrap":false,"default_ext2_preserved":true}),
        )?,
    )?;
    println!(
        "Verified fresh-disk migration, real bzip2 build, interruption and UART budgets: {}",
        work.display()
    );
    Ok(())
}
