// SPDX-License-Identifier: Apache-2.0
//! Prepare PCI handoffs, attach polled storage/network and optionally own scheduler-polled PS/2.
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
use anova_storage::ext2::Ext2;
use anova_vfs::Vfs;
use core::cell::UnsafeCell;

struct Prepared(UnsafeCell<Option<(pci::VirtioConfig, PmTimer)>>);
// SAFETY: Only the BSP accesses this boot handoff with IF=0; no IRQ or AP touches it.
unsafe impl Sync for Prepared {}
static PREPARED: Prepared = Prepared(UnsafeCell::new(None));

pub fn prepare(mapper: &mut EarlyMapper<'_>, timer: PmTimer) {
    assert_eq!(cpu::flags() & (1 << 9), 0, "PCI preparation requires IF=0");
    network::prepare(mapper, timer);
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
    let readonly = anova_storage::BlockDevice::read_only(&device);
    device.verify_boundaries()?;
    let disk = Ext2::mount(Box::new(device))?;
    fs.mount_disk(disk)?;
    serial::write(format_args!(
        "[DISK:MOUNT] /persist=ext2:{} profile=4k-single-group native=anova.native.v1\n",
        if readonly { "ro" } else { "rw" }
    ));
    Ok(())
}
