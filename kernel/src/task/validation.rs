// SPDX-License-Identifier: Apache-2.0
//! Verify loader/syscall/fault containment plus transactional process/fork/exec failures.
use super::*;
use alloc::vec::Vec;

pub fn run(rust: &[u8]) {
    mm::uaccess::verify_recovery();
    let frames = mm::user::free_frames();
    let heap = mm::heap_used();
    {
        let mut space = mm::user::UserSpace::new().unwrap();
        let before = mm::user::free_frames();
        mm::user::set_test_budget(2);
        assert!(space.map_anonymous(0x5000_0000, 0x5000_4000).is_err());
        mm::user::set_test_budget(usize::MAX);
        assert_eq!(
            before,
            mm::user::free_frames(),
            "Anonymous OOM leaked frames"
        );
        assert!(!space.accessible(UserAddr(0x5000_0000), 1, false, false));
        space.map_anonymous(0x5000_0000, 0x5000_2000).unwrap();
        assert!(space.map_anonymous(0x4fff_f000, 0x5000_1000).is_err());
        assert!(!space.accessible(UserAddr(0x4fff_f000), 1, false, false));
    }
    assert_eq!(frames, mm::user::free_frames());
    assert_eq!(heap, mm::heap_used());
    serial::write(format_args!(
        "[LINUX:MEMORY] partial_oom=rolled_back overlap=retained frames_restored=true heap_restored=true\n"
    ));
    let before = mm::user::free_frames();
    assert!(matches!(
        loader::load(b"bad ELF", PERSONALITY, &["bad"]),
        Err(loader::LoadError::Elf(_))
    ));
    assert!(matches!(
        loader::load(rust, "linux", &["bad"]),
        Err(loader::LoadError::Personality)
    ));
    let mut bad = Vec::from(rust);
    bad[16] = 3;
    assert!(loader::load(&bad, PERSONALITY, &["bad"]).is_err());
    assert!(loader::load(rust, PERSONALITY, &["bad\0argument"]).is_err());
    mm::user::set_test_budget(3);
    assert!(matches!(
        loader::load(rust, PERSONALITY, &["oom"]),
        Err(loader::LoadError::Memory(anova_memory::Error::OutOfMemory))
    ));
    mm::user::set_test_budget(usize::MAX);
    assert_eq!(
        mm::user::free_frames(),
        before,
        "Failed ELF load leaked frames"
    );
    serial::write(format_args!(
        "[USER:LOADER] malformed=rejected personality=explicit partial_oom=rolled_back\n"
    ));
    let probe = boot::native_module("probe");
    let fault = |vector, error, address| Outcome::Fault {
        vector,
        error,
        address,
    };
    for (index, (case, expected)) in [
        ("syscalls", Outcome::Exit(0)),
        ("timer", Outcome::Exit(0)),
        ("page-fault", fault(14, 4, 0)),
        ("kernel-read", fault(14, 5, 0xffff_ffff_8000_0000)),
        ("kernel-write", fault(14, 7, 0xffff_ffff_8000_0000)),
        (
            "stack-guard",
            fault(14, 4, anova_abi::STACK_TOP - anova_abi::STACK_BYTES - 1),
        ),
        ("invalid-opcode", fault(6, 0, 0)),
        ("privileged", fault(13, 0, 0)),
        ("bad-rsp", Outcome::BadReturn),
    ]
    .into_iter()
    .enumerate()
    {
        checked_run(probe, &["probe", case], index as u64 + 3, expected);
        serial::write(format_args!("[USER:CASE] name={case} passed=true\n"));
    }
    // Addresses of probe text/BSS are link-dependent; still require the exact
    // hardware vector and permission error before accepting fault containment.
    for (index, (case, error)) in [("read-only", 7), ("no-execute", 21)]
        .into_iter()
        .enumerate()
    {
        let before = mm::user::free_frames();
        assert!(
            matches!(super::run(probe,&["probe",case],index as u64+12).unwrap(),
            Outcome::Fault { vector:14,error:e,address } if e==error && (anova_abi::USER_MIN..anova_elf::IMAGE_LIMIT).contains(&address))
        );
        assert_eq!(
            mm::user::free_frames(),
            before,
            "Faulted process leaked pages"
        );
        serial::write(format_args!(
            "[USER:CASE] name={case} passed=true frames_restored=true\n"
        ));
    }
    for pid in 14..22 {
        checked_run(
            rust,
            &["hello-rust", "AnovaOS", "ring3"],
            pid,
            Outcome::Exit(0),
        );
    }
    let ticks = USER_IRQS.load(Ordering::Relaxed);
    assert!(ticks > 0, "No timer IRQ interrupted Ring 3");
    serial::write(format_args!(
        "[USER:TEST] cases=11 repeated=8 pointers=rejected syscalls=checked faults=contained frames_restored=true user_irqs={ticks}\n"
    ));
    lifecycle_oom(rust);
}

fn lifecycle_oom(rust: &[u8]) {
    let frames = mm::user::free_frames();
    let heap = mm::heap_used();
    {
        let mut kernel = Kernel::new(100).unwrap();
        let parent = kernel
            .add(rust, &["hello-rust", "AnovaOS", "ring3"], 0)
            .unwrap();
        kernel
            .fs
            .close(&mut kernel.processes[parent].as_mut().unwrap().files, 1)
            .unwrap();
        let root = kernel.process(parent).image.as_ref().unwrap().space.root();
        let before = mm::user::free_frames();
        let heap = mm::heap_used();
        for operation in 0..3 {
            mm::user::set_test_budget(3);
            let error = match operation {
                0 => kernel.fork(parent).map(|_| ()),
                1 => kernel.exec(parent, rust, &["new-image"]),
                _ => kernel.add(rust, &["child"], 100).map(|_| ()),
            };
            mm::user::set_test_budget(usize::MAX);
            assert_eq!(
                error,
                Err(NO_MEMORY),
                "Partial process allocation must fail"
            );
            assert_eq!(
                mm::user::free_frames(),
                before,
                "Failed lifecycle leaked pages"
            );
            assert_eq!(
                mm::heap_used(),
                heap,
                "Failed lifecycle leaked heap storage"
            );
            assert_eq!(
                kernel.process(parent).image.as_ref().unwrap().space.root(),
                root
            );
            assert!(
                !kernel.process(parent).files.contains(1),
                "Failed exec replaced descriptors"
            );
            assert!((1..CAPACITY).all(|i| kernel.policy.entry(i).state == State::Empty));
        }
    }
    assert_eq!(mm::user::free_frames(), frames);
    assert_eq!(mm::heap_used(), heap);
    serial::write(format_args!(
        "[PROC:OOM] spawn=rolled_back fork=rolled_back exec=retained frames_restored=true heap_restored=true\n"
    ));
}
