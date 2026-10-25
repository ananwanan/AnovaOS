// SPDX-License-Identifier: Apache-2.0
//! Runtime invariants and isolated QEMU stress/fault validation.
use super::*;
use alloc::vec::Vec;
#[cfg(feature = "qemu-test")]
use alloc::{boxed::Box, string::String};

pub fn verify() {
    MEMORY.with(|slot| verify_memory(slot.as_mut().expect("Memory manager unavailable")));
    #[cfg(feature = "qemu-test")]
    {
        MEMORY.with(|slot| verify_frames(slot.as_mut().expect("Memory manager unavailable")));
        verify_heap();
    }
    serial::write(format_args!(
        "[MM:READY] owned stack, frames, tables and heap verified\n"
    ));
}

fn verify_memory(manager: &mut MemoryManager) {
    let stack: u64;
    // SAFETY: Only read RSP; no memory is dereferenced by the instruction.
    unsafe { asm!("mov {}, rsp", out(reg) stack, options(nomem, nostack, preserves_flags)) };
    assert!(
        (STACK_BOTTOM..STACK_TOP).contains(&stack),
        "Still executing on bootloader stack"
    );
    let store = Store {
        frames: &mut manager.frames,
        direct: manager.direct,
        boot: None,
        active: true,
    };
    assert_eq!(
        manager.tables.query(VirtAddr(STACK_GUARD), &store),
        Err(Error::NotMapped)
    );
    assert_eq!(
        manager.tables.query(VirtAddr(STACK_TOP), &store),
        Err(Error::NotMapped)
    );
    let mut visited = 0;
    manager.tables.visit_tables(&store, |frame| {
        assert_eq!(
            store.frames.owner(frame),
            Ok(Some(Owner::PageTable)),
            "Borrowed table remains live"
        );
        visited += 1;
    });
    assert_eq!(
        visited,
        manager.tables.table_count(),
        "Table ownership count mismatch"
    );
    let (root, cr0, cr4): (u64, u64, u64);
    // SAFETY: Read only architectural control registers on the BSP at CPL0.
    unsafe {
        asm!("mov {}, cr3", out(reg) root, options(nomem, nostack, preserves_flags));
        asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack, preserves_flags));
        asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
    }
    assert_eq!(
        root & anova_memory::paging::ADDRESS,
        manager.tables.root().0,
        "Wrong active root"
    );
    assert!(
        cr0 & (1 << 16) != 0,
        "Supervisor write protection is disabled"
    );
    assert_eq!(
        cr4 & ((1 << 7) | (1 << 12) | (1 << 17)),
        0,
        "PGE/LA57/PCID must be disabled"
    );
    assert!(
        cpu::read_msr(0xc000_0080) & (1 << 11) != 0,
        "NX is disabled"
    );
    let text = manager
        .tables
        .query(VirtAddr(crate::_start as *const () as u64), &store)
        .expect("Missing kernel text");
    assert!(
        text.executable && !text.writable && !text.user,
        "Invalid kernel text permissions"
    );
    for address in [STACK_BOTTOM, HEAP_BASE] {
        let mapping = manager
            .tables
            .query(VirtAddr(address), &store)
            .expect("Missing owned RAM mapping");
        assert!(
            mapping.writable && !mapping.executable && !mapping.user && mapping.cache_index == 0,
            "Invalid owned RAM permissions"
        );
        let direct = manager
            .tables
            .query(VirtAddr(manager.direct.0 + mapping.physical.0), &store)
            .expect("Owned RAM is missing its direct alias");
        assert!(
            !direct.executable && !direct.user && direct.cache_index == 0,
            "Unsafe direct RAM alias"
        );
    }
    for address in [LAPIC_WINDOW, IOAPIC_WINDOW] {
        let mapping = manager
            .tables
            .query(VirtAddr(address), &store)
            .expect("Missing APIC mapping");
        assert!(
            mapping.writable && !mapping.executable && !mapping.user && mapping.cache_index == 3,
            "Invalid MMIO cache attributes"
        );
    }
    serial::write(format_args!(
        "[MM:STACK] rsp={stack:#x} bottom={STACK_BOTTOM:#x} top={STACK_TOP:#x} guards=2 RSP0=owned\n"
    ));
}

#[cfg(feature = "qemu-test")]
fn verify_frames(manager: &mut MemoryManager) {
    let before = manager.frames.stats();
    let probe = manager
        .frames
        .allocate(Owner::Test, Zone::Any)
        .expect("No test frame");
    assert_eq!(
        manager.frames.release(probe, Owner::Heap),
        Err(Error::WrongOwner)
    );
    manager.frames.release(probe, Owner::Test).unwrap();
    assert_eq!(
        manager.frames.release(probe, Owner::Test),
        Err(Error::NotAllocated)
    );
    assert_eq!(
        manager.frames.release(PhysAddr(probe.0 + 1), Owner::Test),
        Err(Error::Alignment)
    );
    assert!(matches!(
        manager.frames.release(PhysAddr(0), Owner::Test),
        Err(Error::InvalidAddress | Error::Reserved)
    ));
    let reserved = (0..before.total)
        .map(|index| manager.frames.frame_at(index).unwrap())
        .find(|frame| manager.frames.owner(*frame) == Err(Error::Reserved))
        .expect("Missing metadata reservation");
    assert_eq!(
        manager.frames.release(reserved, Owner::Test),
        Err(Error::Reserved)
    );
    let tables_before = manager.tables.table_count();
    for round in 0..256 {
        let address = VirtAddr(TEST_BASE + (round % 32) * PAGE_SIZE);
        let frame = manager
            .map_page(address, Owner::Test)
            .expect("Map stress allocation failed");
        let value = 0x1234_5678_0000_0000 | round;
        // SAFETY: map_page installed an exclusive zeroed RW page at address.
        // Both accesses are aligned and remain inside it; direct alias checks
        // the actual physical target. No references alias the volatile writes.
        unsafe {
            (address.0 as *mut u64).write_volatile(value);
            assert_eq!(
                direct_pointer(manager.direct, frame)
                    .cast::<u64>()
                    .read_volatile(),
                value
            );
        }
        assert_eq!(
            manager.unmap_page(address, Owner::Heap),
            Err(Error::WrongOwner)
        );
        assert_eq!(manager.unmap_page(address, Owner::Test), Ok(frame));
    }
    assert_eq!(
        manager.tables.table_count(),
        tables_before,
        "Empty page tables leaked"
    );
    assert_eq!(
        manager.frames.stats(),
        before,
        "Mapping stress leaked frames"
    );

    // Exhaust real free RAM without retaining an auxiliary heap list. Ownership
    // tags allow a subsequent scan to return exactly the test-owned frames.
    let payload = manager.frames.allocate(Owner::Test, Zone::Any).unwrap();
    while manager.frames.stats().free > 1 {
        manager.frames.allocate(Owner::Test, Zone::Any).unwrap();
    }
    let mut store = Store {
        frames: &mut manager.frames,
        direct: manager.direct,
        boot: None,
        active: true,
    };
    assert_eq!(
        manager.tables.map(
            VirtAddr(ROLLBACK_BASE),
            payload,
            Permissions::DATA,
            &mut store
        ),
        Err(Error::OutOfMemory)
    );
    assert_eq!(
        manager.tables.query(VirtAddr(ROLLBACK_BASE), &store),
        Err(Error::NotMapped)
    );
    assert_eq!(
        manager.tables.table_count(),
        tables_before,
        "Failed mapping leaked tables"
    );
    assert_eq!(store.frames.stats().free, 1, "OOM rollback leaked a frame");
    store.frames.allocate(Owner::Test, Zone::Any).unwrap();
    assert_eq!(
        store.frames.allocate(Owner::Test, Zone::Any),
        Err(Error::OutOfMemory)
    );
    for index in 0..store.frames.stats().total {
        let frame = store.frames.frame_at(index).unwrap();
        if store.frames.owner(frame) == Ok(Some(Owner::Test)) {
            store.frames.release(frame, Owner::Test).unwrap();
        }
    }
    assert_eq!(
        store.frames.stats(),
        before,
        "Exhaustion recovery leaked RAM"
    );
    serial::write(format_args!(
        "[MM:TEST] mappings=256 free={} reserved={} invalid_free=rejected frame_oom=recovered map_oom=rolled_back\n",
        before.free, before.reserved
    ));
}

#[cfg(feature = "qemu-test")]
fn verify_heap() {
    let before = HEAP.with(|heap| heap.used());
    {
        let mut boxes = Vec::new();
        for index in 0..128_u64 {
            boxes.push(Box::new([index; 32]));
        }
        let mut message = String::from("AnovaOS ");
        message.push_str("owned heap");
        assert_eq!(message, "AnovaOS owned heap");
        for (index, block) in boxes.iter().enumerate() {
            assert!(block.iter().all(|value| *value == index as u64));
        }
        let mut large = Vec::<u8>::new();
        assert!(
            large.try_reserve_exact(HEAP_BYTES * 2).is_err(),
            "Oversized heap allocation succeeded"
        );
    }
    assert_eq!(
        HEAP.with(|heap| heap.used()),
        before,
        "alloc containers leaked heap blocks"
    );
    let layout = Layout::from_size_align(HEAP_BYTES, HEAP_BYTES).unwrap();
    // SAFETY: GlobalAlloc provides an exclusive aligned block or null. Free
    // uses its original layout after all prior containers have been dropped.
    unsafe {
        let pointer = alloc::alloc::alloc(layout);
        assert_eq!(pointer as u64, HEAP_BASE, "Heap failed to coalesce");
        alloc::alloc::dealloc(pointer, layout);
    }
    serial::write(format_args!(
        "[MM:ALLOC] Box Vec String stress=128 coalesced=true fallible_oom=recovered used={}\n",
        HEAP.with(|heap| heap.used())
    ));
}

pub fn inject_fault() {
    if cfg!(feature = "test-oom") {
        serial::write(format_args!("[TEST:INJECT] oom\n"));
        // Stable no_std alloc calls the default allocation error handler, which
        // invokes our allocation-free panic path. black_box keeps a real request.
        let value = Vec::<u8>::with_capacity(core::hint::black_box(HEAP_BYTES * 2));
        core::hint::black_box(value);
        panic!("OOM injection returned");
    }
    if cfg!(feature = "test-stack-guard") {
        serial::write(format_args!(
            "[TEST:INJECT] stack-guard address={STACK_GUARD:#x}\n"
        ));
        // SAFETY: Deliberate write to the owned stack's absent lower guard in
        // an isolated test image; #PF must arrive on the independent IST2.
        unsafe { asm!("mov byte ptr [{}], 0", in(reg) STACK_GUARD, options(nostack)) };
        panic!("Stack guard injection returned");
    }
    if cfg!(feature = "test-read-only")
        || cfg!(feature = "test-no-execute")
        || cfg!(feature = "test-direct-no-execute")
        || cfg!(feature = "test-unmapped")
    {
        MEMORY.with(|slot| {
            let manager = slot.as_mut().expect("Missing memory manager");
            let address = VirtAddr(TEST_BASE);
            let frame = manager
                .map_page(address, Owner::Test)
                .expect("Fault probe mapping failed");
            if cfg!(feature = "test-read-only") {
                let mut store = Store {
                    frames: &mut manager.frames,
                    direct: manager.direct,
                    boot: None,
                    active: true,
                };
                manager
                    .tables
                    .protect(address, Permissions::READ_ONLY, &mut store)
                    .unwrap();
                serial::write(format_args!(
                    "[TEST:INJECT] read-only address={TEST_BASE:#x}\n"
                ));
                // SAFETY: Deliberate supervisor write to a present read-only
                // page; CR0.WP must make this a protection #PF, not a success.
                unsafe { asm!("mov byte ptr [{}], 0", in(reg) TEST_BASE, options(nostack)) };
            } else if cfg!(feature = "test-no-execute") || cfg!(feature = "test-direct-no-execute")
            {
                // SAFETY: The mapped probe is exclusive RW/NX RAM. Store a RET
                // without making it executable, then attempt a real fetch. The
                // independent #PF path never takes the held memory-manager lock.
                unsafe { (TEST_BASE as *mut u8).write_volatile(0xc3) };
                let (name, target) = if cfg!(feature = "test-direct-no-execute") {
                    (
                        "direct-no-execute",
                        manager
                            .direct
                            .0
                            .checked_add(frame.0)
                            .expect("Direct alias overflow"),
                    )
                } else {
                    ("no-execute", TEST_BASE)
                };
                serial::write(format_args!("[TEST:INJECT] {name} address={target:#x}\n"));
                // SAFETY: Deliberate isolated NX fault. Use the C call ABI so
                // a broken NX implementation cannot silently corrupt registers.
                unsafe { asm!("call rax", in("rax") target, clobber_abi("C")) };
            } else {
                manager.unmap_page(address, Owner::Test).unwrap();
                serial::write(format_args!(
                    "[TEST:INJECT] unmapped address={TEST_BASE:#x}\n"
                ));
                // SAFETY: Deliberate access after a checked unmap/TLB flush.
                unsafe { asm!("mov byte ptr [{}], 0", in(reg) TEST_BASE, options(nostack)) };
            }
            panic!("Memory protection injection returned");
        });
    }
}
