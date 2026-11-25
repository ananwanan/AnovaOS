// SPDX-License-Identifier: Apache-2.0
//! Capture early desktop kernel messages until init and publish one bounded boot snapshot.
//! Only ordinary BSP logging appends. Fatal handlers bypass capture and never perform disk I/O.

use crate::serial;
use anova_abi::{IO_ERROR, NOT_FOUND, OPEN_CREATE, OPEN_EXCLUSIVE, OPEN_WRITE};
use anova_vfs::{Descriptors, ROOT, Vfs};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};

const CAPACITY: usize = 32 * 1024;
const STAGING: &str = "/persist/logs/boot.tmp";

struct Ring {
    bytes: [u8; CAPACITY],
    next: usize,
    wrapped: bool,
}

struct Capture(UnsafeCell<Ring>);
// SAFETY: Only the BSP writes with interrupts disabled. NMI and fatal writers bypass this
// object. The one-way ACTIVE transition ends all mutable access before publishing a slice.
unsafe impl Sync for Capture {}
static CAPTURE: Capture = Capture(UnsafeCell::new(Ring {
    bytes: [0; CAPACITY],
    next: 0,
    wrapped: false,
}));
static ACTIVE: AtomicBool = AtomicBool::new(true);

pub fn append(bytes: &[u8]) {
    if !ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    // SAFETY: Called by the normal serial writer at IF=0 on the single BSP, before sealing.
    // No formatting, nested logging or hardware access takes place during this borrow.
    let ring = unsafe { &mut *CAPTURE.0.get() };
    for &byte in bytes {
        ring.bytes[ring.next] = byte;
        ring.next += 1;
        if ring.next == CAPACITY {
            ring.next = 0;
            ring.wrapped = true;
        }
    }
}

pub fn publish(fs: &mut Vfs<'_>) {
    if !ACTIVE.swap(false, Ordering::Relaxed) {
        return;
    }
    // SAFETY: The BSP is at IF=0. All writers now skip CAPTURE permanently, and this is
    // the only successful seal. The static slice stays valid for the init filesystem.
    let ring = unsafe { &mut *CAPTURE.0.get() };
    let bytes = if ring.wrapped {
        ring.bytes.rotate_left(ring.next);
        &ring.bytes[..]
    } else {
        &ring.bytes[..ring.next]
    };
    if let Err(error) = fs.publish_boot_log(bytes) {
        serial::write(format_args!("[BOOTLOG:RAM_ERROR] error={error}\n"));
    }
    match save(fs, bytes) {
        Ok(Some(path)) => serial::write(format_args!("[BOOTLOG:SAVED] path={path}\n")),
        Ok(None) => {}
        Err(error) => serial::write(format_args!("[BOOTLOG:DISK_ERROR] error={error}\n")),
    }
}

fn save(fs: &mut Vfs<'_>, bytes: &[u8]) -> Result<Option<&'static str>, i64> {
    if !fs.writable(ROOT, "/persist")? {
        return Ok(None);
    }
    // An existing directory opts this disk into capture. Never follow a link as the opt-in.
    match fs.metadata_nofollow(ROOT, "/persist/logs") {
        Err(NOT_FOUND) => return Ok(None),
        Ok((_, 2, _, _)) => {}
        Ok(_) => return Err(anova_abi::NOT_DIRECTORY),
        Err(error) => return Err(error),
    }
    let paths = [
        "/persist/logs/boot-1.log",
        "/persist/logs/boot-2.log",
        "/persist/logs/boot-3.log",
        "/persist/logs/boot-4.log",
    ];
    for path in paths {
        match fs.metadata_nofollow(ROOT, path) {
            Ok(_) => continue,
            Err(NOT_FOUND) => {}
            Err(error) => return Err(error),
        }
        let mut fds = Descriptors::default();
        let fd = fs.open(
            &mut fds,
            ROOT,
            STAGING,
            OPEN_CREATE | OPEN_EXCLUSIVE | OPEN_WRITE,
        )?;
        let written = (|| {
            let mut remaining = bytes;
            while !remaining.is_empty() {
                let count = fs.write(&fds, fd, remaining)?;
                if count == 0 {
                    return Err(IO_ERROR);
                }
                remaining = &remaining[count..];
            }
            fs.fsync(&fds, fd)
        })();
        let closed = fs.close(&mut fds, fd);
        if let Err(error) = written
            .and(closed)
            .and_then(|()| fs.rename_file(ROOT, STAGING, path, false))
        {
            // No user process can race this boot-owned exclusive creation.
            let _ = fs.unlink(ROOT, STAGING);
            return Err(error);
        }
        fs.sync()?;
        return Ok(Some(path));
    }
    Err(anova_abi::NO_SPACE)
}
