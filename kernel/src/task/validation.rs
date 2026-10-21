// SPDX-License-Identifier: Apache-2.0
//! Exercise the real loader/syscall/fault boundaries and verify exact reclamation.
use super::*;
use alloc::vec::Vec;

pub fn run(rust: &[u8]) {
    mm::uaccess::verify_recovery();
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
}
