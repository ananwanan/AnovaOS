// SPDX-License-Identifier: Apache-2.0
//! Borrow retained Limine data. M3 allocates only USABLE RAM; bootloader data,
//! original stacks/tables, modules, firmware and the early pool remain reserved.
//! Retained native ELF modules support isolation tests; a tagged archive supplies the M6 root.
use limine::{
    BaseRevision,
    firmware_type::FirmwareType,
    memory_map::{Entry, EntryType},
    paging::Mode,
    request::*,
};

use crate::serial;
use anova_platform::{PhysAddr, VirtAddr};

#[used]
#[unsafe(link_section = ".limine_requests_start")]
static START: RequestsStartMarker = RequestsStartMarker::new();

macro_rules! request {
    ($name:ident: $ty:ty = $value:expr) => {
        #[used]
        #[unsafe(link_section = ".limine_requests")]
        static $name: $ty = $value;
    };
}

request!(BASE: BaseRevision = BaseRevision::with_revision(3));
request!(LOADER: BootloaderInfoRequest = BootloaderInfoRequest::new());
request!(FIRMWARE: FirmwareTypeRequest = FirmwareTypeRequest::new());
request!(STACK: StackSizeRequest = StackSizeRequest::new().with_size(64 * 1024));
request!(PAGING: PagingModeRequest = PagingModeRequest::new().with_mode(Mode::FOUR_LEVEL).with_min_mode(Mode::FOUR_LEVEL).with_max_mode(Mode::FOUR_LEVEL));
request!(MEMORY: MemoryMapRequest = MemoryMapRequest::new());
request!(HHDM: HhdmRequest = HhdmRequest::new());
request!(EXECUTABLE: ExecutableAddressRequest = ExecutableAddressRequest::new());
request!(RSDP: RsdpRequest = RsdpRequest::new());
request!(FRAMEBUFFER: FramebufferRequest = FramebufferRequest::new());
request!(MODULES: ModuleRequest = ModuleRequest::new());

#[used]
#[unsafe(link_section = ".limine_requests_end")]
static END: RequestsEndMarker = RequestsEndMarker::new();

pub struct BootInfo {
    entries: &'static [&'static Entry],
    hhdm: VirtAddr,
    kernel_base: PhysAddr,
    usable_bytes: u64,
}

pub fn validate() -> BootInfo {
    assert!(BASE.is_supported(), "Unsupported Limine base revision 3");
    assert_eq!(
        BASE.loaded_revision(),
        Some(3),
        "Unexpected Limine base revision"
    );
    let loader = LOADER.get_response().expect("Missing bootloader identity");
    serial::write(format_args!(
        "[BOOT:PROTOCOL] {} {} base=3\n",
        loader.name(),
        loader.version()
    ));
    assert!(
        FIRMWARE
            .get_response()
            .is_some_and(|r| r.firmware_type() == FirmwareType::UEFI_64),
        "UEFI64 firmware required"
    );
    assert!(
        PAGING
            .get_response()
            .is_some_and(|r| r.mode() == Mode::FOUR_LEVEL),
        "Four-level paging required"
    );
    let hhdm = HHDM.get_response().expect("Missing HHDM").offset();
    assert!(
        hhdm >= 0xffff_8000_0000_0000 && hhdm & 0xfff == 0,
        "Invalid HHDM offset"
    );
    let executable = EXECUTABLE
        .get_response()
        .expect("Missing executable address");
    assert_eq!(
        executable.virtual_base(),
        0xffff_ffff_8000_0000,
        "Unexpected kernel virtual base"
    );

    let entries = MEMORY.get_response().expect("Missing memory map").entries();
    assert!(!entries.is_empty(), "Empty memory map");
    let mut previous_base = 0;
    let mut usable_bytes = 0_u64;
    let mut kernel_reserved = false;
    for entry in entries {
        assert!(
            entry.length != 0 && entry.base >= previous_base,
            "Invalid or unsorted memory map entry"
        );
        let end = entry
            .base
            .checked_add(entry.length)
            .expect("Memory map overflow");
        previous_base = entry.base;
        if entry.entry_type == EntryType::USABLE {
            usable_bytes = usable_bytes
                .checked_add(entry.length)
                .expect("Usable memory overflow");
        }
        if entry.entry_type == EntryType::EXECUTABLE_AND_MODULES
            && entry.base <= executable.physical_base()
            && executable.physical_base() < end
        {
            kernel_reserved = true;
        }
    }
    // Reserved entries may overlap each other under the Limine protocol.
    // Only usable and bootloader-reclaimable entries must be disjoint from all
    // other entries; do not reject a conforming map for reserved overlaps.
    for (index, entry) in entries.iter().enumerate() {
        if entry.entry_type == EntryType::USABLE
            || entry.entry_type == EntryType::BOOTLOADER_RECLAIMABLE
        {
            assert_eq!(
                (entry.base | entry.length) & 0xfff,
                0,
                "Unaligned RAM range"
            );
            for (other_index, other) in entries.iter().enumerate() {
                assert!(
                    index == other_index
                        || entry.base >= other.base + other.length
                        || other.base >= entry.base + entry.length,
                    "Overlapping allocatable RAM range"
                );
            }
        }
    }
    assert!(
        usable_bytes > 0 && kernel_reserved,
        "Missing usable memory or kernel reservation"
    );
    BootInfo {
        entries,
        hhdm: VirtAddr(hhdm),
        kernel_base: PhysAddr(executable.physical_base()),
        usable_bytes,
    }
}

impl BootInfo {
    pub fn entries(&self) -> &[&Entry] {
        self.entries
    }

    pub fn direct_map(&self) -> VirtAddr {
        self.hhdm
    }

    pub fn rsdp(&self) -> PhysAddr {
        PhysAddr(
            RSDP.get_response()
                .expect("M2 requires ACPI RSDP")
                .address() as u64,
        )
    }

    pub fn kernel_physical(&self, address: VirtAddr) -> PhysAddr {
        let offset = address
            .0
            .checked_sub(0xffff_ffff_8000_0000)
            .expect("Not a kernel address");
        let physical = PhysAddr(
            self.kernel_base
                .0
                .checked_add(offset)
                .expect("Kernel address overflow"),
        );
        assert!(
            self.entries.iter().any(
                |entry| entry.entry_type == EntryType::EXECUTABLE_AND_MODULES
                    && physical.0 >= entry.base
                    && physical.0 < entry.base + entry.length
            ),
            "Kernel address lacks reservation"
        );
        physical
    }

    pub fn table_pointer(&self, physical: PhysAddr) -> *const u64 {
        assert_eq!(physical.0 & 0xfff, 0, "Unaligned page table");
        assert!(
            self.entries.iter().any(|entry| {
                (entry.entry_type == EntryType::BOOTLOADER_RECLAIMABLE
                    || entry.entry_type == EntryType::EXECUTABLE_AND_MODULES)
                    && physical.0 >= entry.base
                    && physical
                        .0
                        .checked_add(4096)
                        .is_some_and(|end| end <= entry.base + entry.length)
            }),
            "Page table is outside retained, HHDM-mapped memory"
        );
        self.hhdm.0.checked_add(physical.0).expect("HHDM overflow") as *const u64
    }

    pub fn check_firmware(&self, physical: PhysAddr, length: usize) {
        let end = physical
            .0
            .checked_add(length as u64)
            .expect("Firmware range overflow");
        assert!(
            length > 0
                && self.entries.iter().any(|entry| {
                    (entry.entry_type == EntryType::ACPI_RECLAIMABLE
                        || entry.entry_type == EntryType::ACPI_NVS
                        || entry.entry_type == EntryType::BOOTLOADER_RECLAIMABLE
                        || entry.entry_type == EntryType::RESERVED)
                        && physical.0 >= entry.base
                        && end <= entry.base + entry.length
                }),
            "ACPI range is outside reserved firmware memory"
        );
    }

    pub fn check_mmio(&self, physical: PhysAddr) {
        let end = physical.0.checked_add(4096).expect("MMIO range overflow");
        assert!(
            self.entries.iter().all(|entry| {
                let mapped_ram = entry.entry_type == EntryType::USABLE
                    || entry.entry_type == EntryType::BOOTLOADER_RECLAIMABLE
                    || entry.entry_type == EntryType::EXECUTABLE_AND_MODULES
                    || entry.entry_type == EntryType::FRAMEBUFFER
                    || entry.entry_type == EntryType::ACPI_RECLAIMABLE
                    || entry.entry_type == EntryType::ACPI_NVS;
                !mapped_ram || end <= entry.base || physical.0 >= entry.base + entry.length
            }),
            "Device page overlaps RAM, firmware, or a framebuffer"
        );
    }

    pub fn report(&self) {
        serial::write(format_args!(
            "[BOOT:MEMORY] entries={} usable_bytes={} hhdm={:#x} kernel_phys={:#x}\n",
            self.entries.len(),
            self.usable_bytes,
            self.hhdm.0,
            self.kernel_base.0
        ));
        for entry in self.entries {
            if entry.entry_type != EntryType::USABLE {
                serial::write(format_args!(
                    "[BOOT:RESERVE] {:#x}..{:#x} {}\n",
                    entry.base,
                    entry.base + entry.length,
                    kind(entry.entry_type)
                ));
            }
        }
        serial::write(format_args!(
            "[BOOT:OWNERSHIP] boot data and non-usable memory reserved; M3 claims only usable RAM\n"
        ));
        if let Some(rsdp) = RSDP.get_response() {
            // Base revision 3 returns a physical address. Do not dereference it
            // until M2 implements ACPI mapping and table checksum validation.
            // UEFI supplies an address, not a legacy 16-byte scan boundary.
            assert!(
                rsdp.address() != 0 && rsdp.address().checked_add(20).is_some(),
                "Invalid RSDP address"
            );
            serial::write(format_args!(
                "[BOOT:ACPI] rsdp_phys={:#x}\n",
                rsdp.address()
            ));
        } else {
            serial::write(format_args!("[BOOT:ACPI] unavailable (optional in M1)\n"));
        }
        let mut count = 0;
        if let Some(response) = FRAMEBUFFER.get_response() {
            for fb in response.framebuffers() {
                assert!(
                    fb.width() > 0 && fb.height() > 0 && fb.bpp() > 0,
                    "Invalid framebuffer dimensions"
                );
                let row_bytes = fb
                    .width()
                    .checked_mul(u64::from(fb.bpp()))
                    .and_then(|v| v.checked_add(7))
                    .expect("Framebuffer width overflow")
                    / 8;
                assert!(fb.pitch() >= row_bytes, "Invalid framebuffer pitch");
                fb.pitch()
                    .checked_mul(fb.height())
                    .expect("Framebuffer size overflow");
                serial::write(format_args!(
                    "[BOOT:FRAMEBUFFER] {}x{} pitch={} bpp={} (not written)\n",
                    fb.width(),
                    fb.height(),
                    fb.pitch(),
                    fb.bpp()
                ));
                count += 1;
            }
        }
        if count == 0 {
            serial::write(format_args!("[BOOT:FRAMEBUFFER] unavailable (optional)\n"));
        }
        let modules = MODULES.get_response().map_or(0, |response| {
            for module in response.modules() {
                let physical = (module.addr() as u64)
                    .checked_sub(self.hhdm.0)
                    .expect("Module is outside HHDM");
                let end = physical
                    .checked_add(module.size())
                    .expect("Module range overflow");
                assert!(
                    module.size() > 0
                        && self.entries.iter().any(|entry| {
                            entry.entry_type == EntryType::EXECUTABLE_AND_MODULES
                                && physical >= entry.base
                                && end <= entry.base + entry.length
                        }),
                    "Module has no kernel/module reservation"
                );
            }
            response.modules().len()
        });
        serial::write(format_args!(
            "[BOOT:MODULES] count={modules}; retained, not executed\n"
        ));
    }
}

fn kind(value: EntryType) -> &'static str {
    match value {
        EntryType::RESERVED => "reserved",
        EntryType::ACPI_RECLAIMABLE => "acpi-reclaimable",
        EntryType::ACPI_NVS => "acpi-nvs",
        EntryType::BAD_MEMORY => "bad-memory",
        EntryType::BOOTLOADER_RECLAIMABLE => "bootloader-reclaimable",
        EntryType::EXECUTABLE_AND_MODULES => "kernel-and-modules",
        EntryType::FRAMEBUFFER => "framebuffer",
        _ => "unknown-reserved",
    }
}

/// Select an explicit image personality from the locked ISO manifest, never OSABI.
pub fn native_module(name: &str) -> &'static [u8] {
    retained_module(b"anova.native.v1:", name.as_bytes())
}

pub fn initramfs() -> &'static [u8] {
    retained_module(b"anova.initramfs.v1", b"")
}

fn retained_module(prefix: &[u8], name: &[u8]) -> &'static [u8] {
    let info = validate();
    let response = MODULES
        .get_response()
        .expect("Missing native application modules");
    let mut selected = None;
    for module in response.modules() {
        let command = module.string().to_bytes();
        let Some(label) = command.strip_prefix(prefix) else {
            continue;
        };
        if label != name {
            continue;
        }
        assert!(selected.is_none(), "Duplicate native module identity");
        let physical = (module.addr() as u64)
            .checked_sub(info.hhdm.0)
            .expect("Module outside HHDM");
        let end = physical
            .checked_add(module.size())
            .expect("Module overflow");
        let length = usize::try_from(module.size()).expect("Module size overflow");
        assert!(
            length > 0
                && length <= 128 * 1024 * 1024
                && info.entries.iter().any(|entry| entry.entry_type
                    == EntryType::EXECUTABLE_AND_MODULES
                    && physical >= entry.base
                    && end <= entry.base + entry.length),
            "Module has no complete retained reservation"
        );
        // SAFETY: Bootloader module bytes are bounded by a retained, read-only
        // borrow of kernel/module RAM in HHDM. Their backing is never reclaimed.
        selected = Some(unsafe { core::slice::from_raw_parts(module.addr(), length) });
    }
    selected.expect("Required native module not found in image manifest")
}
