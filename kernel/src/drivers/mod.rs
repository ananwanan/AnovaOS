// SPDX-License-Identifier: Apache-2.0
//! Prepare polled devices and explicitly opt into audited non-journal ext4 writes by kernel feature.
#[cfg(feature = "gpu")]
pub mod gpu;
#[cfg(feature = "graphics")]
pub mod input;
pub mod network;
pub mod pci;
pub mod virtio;
use crate::{
    arch::x86_64::{cpu, paging::EarlyMapper},
    serial,
};
use alloc::boxed::Box;
use anova_platform::acpi::PmTimer;
use anova_storage::filesystem::{self, Format};
use anova_vfs::Vfs;
use core::cell::UnsafeCell;

struct Prepared(UnsafeCell<Option<(pci::VirtioConfig, PmTimer)>>);
// SAFETY: Only the BSP accesses this boot handoff with IF=0; no IRQ or AP touches it.
unsafe impl Sync for Prepared {}
static PREPARED: Prepared = Prepared(UnsafeCell::new(None));

pub fn prepare(mapper: &mut EarlyMapper<'_>, timer: PmTimer) {
    assert_eq!(cpu::flags() & (1 << 9), 0, "PCI preparation requires IF=0");
    network::prepare(mapper, timer);
    #[cfg(feature = "gpu")]
    gpu::prepare(mapper, timer);
    match pci::discover(mapper, false) {
        Ok(config) => {
            // SAFETY: Sole BSP boot writer; the handoff is consumed once in attach().
            unsafe {
                *PREPARED.0.get() = config.map(|c| (c, timer));
            }
        }
        Err(error) => serial::write(format_args!("[DISK:REJECT] PCI error={error}\n")),
    }
}
pub fn attach(fs: &mut Vfs<'_>) -> Result<(), i64> {
    assert_eq!(cpu::flags() & (1 << 9), 0, "Disk attach requires IF=0");
    // SAFETY: The BSP owns the handoff, with scheduling disabled. take prevents a second driver.
    let prepared = unsafe { (&mut *PREPARED.0.get()).take() };
    let Some((config, timer)) = prepared else {
        serial::write(format_args!("[DISK:ABSENT] initramfs-only session\n"));
        return Ok(());
    };
    let mut device = virtio::Block::new(config, timer)?;
    device.verify_boundaries()?;
    let disk = filesystem::mount_with_ext4_writes(Box::new(device), cfg!(feature = "ext4-write"))?;
    let caps = disk.capabilities();
    fs.mount_backend(disk)?;
    serial::write(format_args!(
        "[DISK:MOUNT] /persist={}:{} profile={} native=anova.native.v1\n",
        if caps.format == Format::Ext2 {
            "ext2"
        } else {
            "ext4"
        },
        if caps.read_only { "ro" } else { "rw" },
        if caps.format == Format::Ext2 {
            "4k-single-group"
        } else if caps.read_only {
            "4k-extents-readonly"
        } else {
            "4k-extents-write-v1"
        }
    ));
    Ok(())
}
