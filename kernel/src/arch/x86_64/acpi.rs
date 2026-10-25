// SPDX-License-Identifier: Apache-2.0
//! Read q35 ACPI tables for interrupts/timing and a restricted constant _S5 soft-off handoff.
use super::paging::EarlyMapper;
use crate::{boot::BootInfo, serial};
use anova_platform::{
    PhysAddr,
    acpi::{self, MAX_ROOT_ENTRIES, Madt, PmTimer},
};
use core::{
    arch::asm,
    sync::atomic::{AtomicU32, Ordering},
};
static SOFT_OFF: AtomicU32 = AtomicU32::new(0);

pub struct Platform {
    pub madt: Madt,
    pub pm_timer: PmTimer,
}

fn read_table<'a>(mapper: &'a mut EarlyMapper<'_>, address: PhysAddr) -> &'a [u8] {
    let length = acpi::table_length(mapper.firmware(address, 36)).expect("Invalid ACPI header");
    mapper.firmware(address, length)
}

pub fn discover(info: &BootInfo, mapper: &mut EarlyMapper<'_>) -> Platform {
    let rsdp = info.rsdp();
    let length = acpi::rsdp_length(mapper.firmware(rsdp, 36)).expect("Invalid RSDP header");
    let (root, width) = acpi::root_address(mapper.firmware(rsdp, length)).expect("Invalid RSDP");
    let mut addresses = [0; MAX_ROOT_ENTRIES];
    let count = acpi::root_entries(read_table(mapper, PhysAddr(root)), width, &mut addresses)
        .expect("Invalid ACPI root");
    let (mut madt, mut pm_timer) = (None, None);
    let mut shutdown = None;
    for address in &addresses[..count] {
        let bytes = read_table(mapper, PhysAddr(*address));
        match &bytes[..4] {
            b"APIC" => {
                assert!(madt.is_none(), "Duplicate MADT");
                madt = Some(acpi::madt(bytes).expect("Unsupported or invalid MADT"));
            }
            b"FACP" => {
                assert!(pm_timer.is_none(), "Duplicate FADT");
                pm_timer = Some(acpi::pm_timer(bytes).expect("Unsupported or invalid FADT timer"));
                shutdown = anova_platform::power::qemu_register(bytes).ok();
            }
            _ => {} // Unneeded tables are not interpreted or executed.
        }
    }
    if let Some((port, dsdt)) = shutdown
        && let Ok(sleep_type) = anova_platform::power::qemu_s5(read_table(mapper, PhysAddr(dsdt)))
    {
        SOFT_OFF.store(
            u32::from(port) | (u32::from(sleep_type) << 16),
            Ordering::Relaxed,
        );
        serial::write(format_args!(
            "[POWER:READY] q35_pm1a={port:#x} sleep_type={sleep_type}\n"
        ));
    }
    let platform = Platform {
        madt: madt.expect("Missing MADT"),
        pm_timer: pm_timer.expect("Missing FADT"),
    };
    serial::write(format_args!(
        "[CPU:ACPI] root={root:#x} width={width} entries={count} lapic={:#x} enabled_cpus={} pm_port={:#x} pm_mask={:#x}\n",
        platform.madt.lapic,
        platform.madt.enabled_cpus,
        platform.pm_timer.port,
        platform.pm_timer.mask
    ));
    platform
}
pub fn can_poweroff() -> bool {
    SOFT_OFF.load(Ordering::Relaxed) != 0
}
pub fn poweroff() -> ! {
    cpu_poweroff(SOFT_OFF.load(Ordering::Relaxed))
}
fn cpu_poweroff(config: u32) -> ! {
    super::cpu::disable_interrupts();
    assert!(config != 0, "Missing q35 soft-off configuration");
    let port = config as u16;
    let kind = (config >> 16) as u16;
    let mut control: u16;
    // SAFETY: The FADT/DSDT parser admitted only q35's System-I/O PM1a register and S5 constant.
    // All writers stopped, ext2 is clean and virtio reset was acknowledged before this call.
    unsafe {
        asm!("in ax,dx",in("dx")port,out("ax")control,options(nomem,nostack,preserves_flags));
        control = (control & !0x3c00) | (kind << 10);
        asm!("out dx,ax",in("dx")port,in("ax")control,options(nomem,nostack,preserves_flags));
    }
    serial::write(format_args!(
        "[POWER:OFF] synced=true clean=true dma_reset=true\n"
    ));
    // SAFETY: Validated ACPI SLP_EN requests q35 power-off only after the durable clean marker.
    unsafe {
        asm!("out dx,ax",in("dx")port,in("ax")(control|1<<13),options(nomem,nostack,preserves_flags));
    }
    // ACPI power removal is asynchronous. The host requires an actual successful QEMU exit;
    // reaching this halt alone never satisfies the shutdown or persistence acceptance gate.
    crate::halt()
}
