// SPDX-License-Identifier: Apache-2.0
//! Create bounded regular ext2 images, attach temporary/persistent disks and verify real reboot data.
//! Every writable image stays below build/; existing images are never implicitly formatted.
use super::*;
use anova_storage::{BlockDevice, SECTOR, check_range, ext2::Ext2};
use std::io::{Read, Seek, SeekFrom, Write};

pub(super) struct Image {
    file: File,
    sectors: u64,
}
impl Image {
    pub(super) fn open(path: &Path) -> Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_file() || !metadata.len().is_multiple_of(SECTOR as u64) {
            return Err("Disk image must be a sector-aligned regular file".into());
        }
        Ok(Self {
            file: fs::OpenOptions::new().read(true).write(true).open(path)?,
            sectors: metadata.len() / 512,
        })
    }
}
impl BlockDevice for Image {
    fn sectors(&self) -> u64 {
        self.sectors
    }
    fn read_sector(
        &mut self,
        sector: u64,
        output: &mut [u8; SECTOR],
    ) -> std::result::Result<(), i64> {
        check_range(self.sectors, sector, SECTOR)?;
        self.file
            .seek(SeekFrom::Start(sector * 512))
            .and_then(|_| self.file.read_exact(output))
            .map_err(|_| anova_abi::IO_ERROR)
    }
    fn write_sector(&mut self, sector: u64, input: &[u8; SECTOR]) -> std::result::Result<(), i64> {
        check_range(self.sectors, sector, SECTOR)?;
        self.file
            .seek(SeekFrom::Start(sector * 512))
            .and_then(|_| self.file.write_all(input))
            .map_err(|_| anova_abi::IO_ERROR)
    }
    fn flush(&mut self) -> std::result::Result<(), i64> {
        self.file.sync_all().map_err(|_| anova_abi::IO_ERROR)
    }
}
pub fn create(root: &Path, path: &Path) -> Result<()> {
    create_profile(root, path, false)
}
pub(super) fn create_profile(root: &Path, path: &Path, linux: bool) -> Result<()> {
    output_path(path)?;
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.set_len(16 * 1024 * 1024)?;
    drop(file);
    // Fixed feature profile, one group, no journal/resize/xattrs/sparse-super/large-file features.
    command(
        Command::new("mke2fs")
            .args([
                "-q",
                "-F",
                "-t",
                "ext2",
                "-b",
                "4096",
                "-I",
                "128",
                "-g",
                "4096",
                "-N",
                "128",
                "-m",
                "0",
                "-O",
                "none,filetype",
                "-E",
                "lazy_itable_init=0",
                "-L",
                "ANOVA-DATA",
            ])
            .arg(path)
            .arg("4096"),
    )?;
    let mut disk = Ext2::mount(Box::new(Image::open(path)?))
        .map_err(|e| format!("New ext2 profile rejected: {e}"))?;
    let result = (|| -> std::result::Result<(), i64> {
        let abi = disk.create(2, ".anova-abi", false)?;
        disk.write(
            abi,
            0,
            if linux {
                b"linux.x86_64.v1\n"
            } else {
                b"anova.native.v1\n"
            },
        )?;
        let src = disk.create(2, "src", true)?;
        let bin = disk.create(2, "bin", true)?;
        if linux {
            return disk.shutdown();
        }
        let source = disk.create(src, "hello.c", false)?;
        disk.write(
            source,
            0,
            b"/* Native source saved on the AnovaOS data disk. */\nint main(void) { return 7; }\n",
        )?;
        let app = disk.create(bin, "hello-c", false)?;
        let bytes =
            fs::read(root.join("build/apps/hello-c.elf")).map_err(|_| anova_abi::IO_ERROR)?;
        disk.write(app, 0, &bytes)?;
        disk.set_executable(app)?;
        disk.shutdown()
    })();
    result.map_err(|e| format!("Cannot populate ext2 image: {e}"))?;
    println!("Created ext2 data image: {}", path.display());
    Ok(())
}
pub fn user_path(root: &Path, value: &str) -> Result<PathBuf> {
    let base = directory(root, "disks")?.canonicalize()?;
    let input = Path::new(value);
    let path = if input.is_absolute() {
        input.to_path_buf()
    } else {
        root.join(input)
    };
    let parent = path
        .parent()
        .ok_or("Disk path has no parent")?
        .canonicalize()?;
    if parent != base {
        return Err("Use a direct regular image file inside build/disks/".into());
    }
    let name = path.file_name().ok_or("Missing image filename")?;
    let result = parent.join(name);
    if result.exists() && !fs::symlink_metadata(&result)?.file_type().is_file() {
        return Err("Disk path must be a regular file, not a link or block device".into());
    }
    if result.to_string_lossy().contains(',') {
        return Err("Disk path cannot contain a comma".into());
    }
    Ok(result)
}
pub fn attach(
    root: &Path,
    vars: &Path,
    cmd: &mut Command,
    path: Option<&Path>,
    read_only: bool,
) -> Result<()> {
    let disk = if let Some(path) = path {
        path.to_path_buf()
    } else {
        let path = vars.join("root.ext2");
        create(root, &path)?;
        path
    };
    checked_file(&disk)?;
    let fault = vars.join("blkdebug.conf");
    let backing = if fault.exists() {
        format!("blkdebug:{}:{}", fault.display(), disk.display())
    } else {
        disk.display().to_string()
    };
    cmd.arg("-drive").arg(format!(
        "if=none,id=anovadisk,format=raw,cache=writeback,werror=report,rerror=report,readonly={},file={}",
        if read_only { "on" } else { "off" },
        backing
    ));
    cmd.args([
        "-device",
        "virtio-blk-pci,drive=anovadisk,disable-legacy=on",
    ]);
    Ok(())
}

pub fn verify(root: &Path) -> Result<()> {
    let iso = image(root, Mode::Normal)?;
    let disks = directory(root, "disks")?;
    let name = format!(
        "persistence-{}.ext2",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    let path = disks.join(name);
    create(root, &path)?;
    session(root,&iso,&path,"disk-save",false,None,&[
        ("anova> ",b"fs-test disk-suite\r"),
        ("[DISK:TEST]",b"edit /persist/src/demo.c\r"),
        ("AnovaOS line editor:",b"a\rint main(void) { return 0; }\r.\rc 1\rint main(void) { return 42; }\ra\r// temporary\r.\rd 2\rp\rw\r"),
        ("[EDIT:SAVED] fsync=ok",b"q\r"),
        ("anova> ",b"install /bin/hello-c /persist/saved-app\r"),
        ("anova> ",b"/persist/saved-app from-disk\r"),
        ("[APP:C] hello from Ring 3; args: /persist/saved-app from-disk; BSS=zero SSE2=ok",b"status\r"),
        ("[SHELL:STATUS] code=7",b"echo durable > /persist/note\r"),
        ("anova> ",b"echo append >> /persist/note\r"),
        ("anova> ",b"echo volatile > /tmp/volatile\r"),
        ("anova> ",b"sync\r"),
        ("anova> ",b"shutdown\r"),
    ],true)?;
    inspect_saved(root, &path)?;
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(&path))?;
    session(
        root,
        &iso,
        &path,
        "disk-reboot",
        false,
        None,
        &[
            ("anova> ", b"fs-test disk-reboot\r"),
            (
                "[DISK:REBOOT] source=exact binary=exact note=exact tmpfs=lost",
                b"/persist/saved-app after-reboot\r",
            ),
            (
                "[APP:C] hello from Ring 3; args: /persist/saved-app after-reboot; BSS=zero SSE2=ok",
                b"status\r",
            ),
            ("[SHELL:STATUS] code=7", b"shutdown\r"),
        ],
        true,
    )?;
    inspect_saved(root, &path)?;
    command(Command::new("e2fsck").args(["-f", "-n"]).arg(&path))?;
    session(
        root,
        &iso,
        &path,
        "disk-read-only",
        true,
        None,
        &[
            ("anova> ", b"echo blocked > /persist/readonly\r"),
            ("shell: echo failed (-11)", b"cat /persist/note\r"),
            ("durable\r\nappend\r\n", b"shutdown\r"),
        ],
        true,
    )?;
    let temporary = Temporary::new(root, "disk-negative")?;
    let bad = temporary.0.join("bad.ext2");
    fs::copy(&path, &bad)?;
    let mut file = fs::OpenOptions::new().write(true).open(&bad)?;
    file.seek(SeekFrom::Start(1024 + 96))?;
    file.write_all(&0x8000_0002u32.to_le_bytes())?;
    file.sync_all()?;
    drop(file);
    session(
        root,
        &iso,
        &bad,
        "disk-bad-feature",
        false,
        None,
        &[
            ("anova> ", b"echo blocked > /persist/bad\r"),
            ("shell: echo failed (-11)", b"shutdown\r"),
        ],
        true,
    )?;
    for (name, event) in [
        ("disk-write-error", "write_aio"),
        ("disk-flush-error", "flush_to_disk"),
    ] {
        let bad = temporary.0.join(format!("{name}.ext2"));
        fs::copy(&path, &bad)?;
        let kind = if event == "write_aio" {
            "write"
        } else {
            "flush"
        };
        let config = format!(
            "[inject-error]\nevent = \"{event}\"\niotype = \"{kind}\"\nerrno = \"5\"\nonce = \"off\"\nimmediately = \"on\"\n"
        );
        let exchanges: &[(&str, &[u8])] = if event == "write_aio" {
            &[
                ("anova> ", b"fs-test disk-error\r"),
                (
                    "[DISK:IOERROR] create=error sync=error retry=error kernel=alive",
                    b"status\r",
                ),
                ("[SHELL:STATUS] code=0", b""),
            ]
        } else {
            &[
                ("anova> ", b"echo blocked > /persist/flush-error\r"),
                ("shell: echo failed (-11)", b"shutdown\r"),
            ]
        };
        session(
            root,
            &iso,
            &bad,
            name,
            false,
            Some(&config),
            exchanges,
            event != "write_aio",
        )?;
    }
    temporary.cleanup()?;
    let result = directory(root, "logs")?.join("persistence.result.txt");
    fs::write(
        result,
        format!(
            "result=Ok\nimage={}\nsource=exact\nbinary=sha256-exact\nboots=2\nacpi_shutdown=true\ne2fsck=clean\nreadonly=reject\nunknown_features=reject\nwrite_error=reported\nflush_error=reported\n",
            path.display()
        ),
    )?;
    println!(
        "Verified editor/source/binary persistence, native execution, clean ACPI shutdown, read-only and device failures; image {}",
        path.display()
    );
    Ok(())
}
fn inspect_saved(root: &Path, path: &Path) -> Result<()> {
    use sha2::{Digest, Sha256};
    let mut disk = Ext2::mount(Box::new(Image::open(path)?))
        .map_err(|e| format!("Saved image failed clean remount: {e}"))?;
    let entries = disk
        .entries(2)
        .map_err(|e| format!("Cannot read root: {e}"))?;
    let find = |entries: &[anova_storage::ext2::Entry], name: &str| -> Result<u32> {
        entries
            .iter()
            .find(|e| &e.name[..e.length] == name.as_bytes())
            .map(|e| e.inode)
            .ok_or_else(|| format!("Missing {name}").into())
    };
    let saved = find(&entries, "saved-app")?;
    let src = find(&entries, "src")?;
    let note = find(&entries, "note")?;
    let source = find(
        &disk
            .entries(src)
            .map_err(|e| format!("Cannot read source dir: {e}"))?,
        "demo.c",
    )?;
    let mut bytes = vec![0; 1024 * 1024];
    let n = disk
        .read(saved, 0, &mut bytes)
        .map_err(|e| format!("Cannot read saved ELF: {e}"))?;
    let expected = fs::read(root.join("build/apps/hello-c.elf"))?;
    if Sha256::digest(&bytes[..n]) != Sha256::digest(&expected) {
        return Err("Saved ELF SHA-256 mismatch".into());
    }
    let n = disk
        .read(source, 0, &mut bytes)
        .map_err(|e| format!("Cannot read edited source: {e}"))?;
    if &bytes[..n] != b"int main(void) { return 42; }\n" {
        return Err("Edited source mismatch".into());
    }
    let n = disk
        .read(note, 0, &mut bytes)
        .map_err(|e| format!("Cannot read redirected note: {e}"))?;
    if &bytes[..n] != b"durable\nappend\n" {
        return Err("Redirected data mismatch".into());
    }
    println!(
        "Host independently verified source bytes and saved ELF SHA-256 {:x}",
        Sha256::digest(&expected)
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)] // One explicit scenario description; not a guest-facing API.
pub(super) fn session(
    root: &Path,
    iso: &Path,
    disk: &Path,
    name: &str,
    readonly: bool,
    fault: Option<&str>,
    exchanges: &[(&str, &[u8])],
    poweroff: bool,
) -> Result<()> {
    session_timeout(
        root, iso, disk, name, readonly, fault, exchanges, poweroff, 60,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn session_timeout(
    root: &Path,
    iso: &Path,
    disk: &Path,
    name: &str,
    readonly: bool,
    fault: Option<&str>,
    exchanges: &[(&str, &[u8])],
    poweroff: bool,
    seconds: u64,
) -> Result<()> {
    let logs = directory(root, "logs")?;
    let serial = logs.join(format!("{name}.serial.log"));
    let diagnostic = logs.join(format!("{name}.qemu.log"));
    output_path(&serial)?;
    output_path(&diagnostic)?;
    let vars = Temporary::new(root, "persistence")?;
    if let Some(fault) = fault {
        fs::write(vars.0.join("blkdebug.conf"), fault)?;
    }
    let mut cmd = qemu_disk(root, iso, &vars.0, false, Some(disk), readonly)?;
    cmd.args(["-serial", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(File::create(&diagnostic)?);
    println!("Running {name}: {cmd:?}");
    let mut guest = Guest(cmd.spawn()?);
    let mut output = transcript::Transcript::capture(
        guest.0.stdout.take().ok_or("Missing UART output pipe")?,
        File::create(&serial)?,
    )?;
    let mut input = guest.0.stdin.take().ok_or("Missing UART pipe")?;
    let start = Instant::now();
    let mut step = 0;
    let mut cursor = 0;
    let transcript = loop {
        output.poll()?;
        let exited = guest.0.try_wait()?;
        if exited.is_some() {
            output.finish()?;
        }
        let text = output.text();
        if step < exchanges.len() && exchanges[step].0 == "[SHELL:STATUS] code=0" {
            let tail = String::from_utf8_lossy(text.as_bytes().get(cursor..).unwrap_or_default());
            if tail.contains("[SHELL:STATUS] code=") && !tail.contains("[SHELL:STATUS] code=0") {
                return Err(format!(
                    "{name}: command failed at step {step}; see {}",
                    serial.display()
                )
                .into());
            }
        }
        if [
            "[PANIC]",
            "[EXCEPTION]",
            "[INIT:FAIL]",
            "[SHELL:FAIL]",
            "[FS:FAIL]",
            "[EDIT:FAIL]",
            "[POWER:FAIL]",
        ]
        .iter()
        .any(|m| text.contains(m))
        {
            return Err(format!("{name}: guest failure; see {}", serial.display()).into());
        }
        if step < exchanges.len()
            && let Some(at) = output.find_from(cursor, exchanges[step].0)
        {
            cursor = at + exchanges[step].0.len();
            input.write_all(exchanges[step].1)?;
            input.flush()?;
            step += 1;
        }
        if let Some(status) = exited {
            if !poweroff
                || !status.success()
                || step != exchanges.len()
                || !text.contains("[POWER:OFF] synced=true clean=true dma_reset=true")
            {
                return Err(format!(
                    "{name}: unexpected exit {status}, step {step}; see {}",
                    serial.display()
                )
                .into());
            }
            break text.into_owned();
        }
        if !poweroff && step == exchanges.len() {
            guest.0.kill()?;
            guest.0.wait()?;
            output.finish()?;
            break output.text().into_owned();
        }
        if start.elapsed() > Duration::from_secs(seconds) {
            return Err(
                format!("{name}: timed out at step {step}; see {}", serial.display()).into(),
            );
        }
        thread::sleep(Duration::from_millis(20));
    };
    if name == "disk-flush-error" {
        if !transcript.contains("[BLK:ERROR] operation=4 status=1")
            || !transcript.contains("[DISK:REJECT] mount error=-21")
            || !transcript.contains("[BLK:RESET] acknowledged=true dma_released=true")
            || transcript.contains("[DISK:MOUNT]")
        {
            return Err("Flush failure did not reject mount and reset DMA".into());
        }
    } else if name == "disk-bad-feature" {
        if !transcript.contains("[DISK:REJECT] mount error=-22")
            || transcript.contains("[DISK:MOUNT]")
        {
            return Err("Unknown ext2 feature was not rejected".into());
        }
    } else if !transcript.contains("[DISK:MOUNT]") || transcript.contains("[DISK:REJECT]") {
        return Err(format!("{name}: missing valid disk mount").into());
    }
    if name == "disk-write-error" && !transcript.contains("[BLK:ERROR] operation=1 status=1") {
        return Err("Write error did not come from a completed virtio request".into());
    }
    drop(guest);
    vars.cleanup()?;
    let evidence = logs.join(format!("{name}.result.txt"));
    output_path(&evidence)?;
    fs::write(
        evidence,
        format!("scenario={name}\nresult=Ok\nsteps={step}\nacpi_shutdown={poweroff}\n"),
    )?;
    println!("Verified {name}; steps={step} acpi_shutdown={poweroff}");
    Ok(())
}
