// SPDX-License-Identifier: Apache-2.0
//! Read only the firmware tables needed by the q35 BSP interrupt baseline.
use super::paging::EarlyMapper;
use crate::{boot::BootInfo, serial};
use anova_platform::{
    PhysAddr,
    acpi::{self, MAX_ROOT_ENTRIES, Madt, PmTimer},
};

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
            }
            _ => {} // Unneeded tables are not interpreted or executed.
        }
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
