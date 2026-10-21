// SPDX-License-Identifier: Apache-2.0
//! Interpret the guest transcript and isa-debug-exit result together.

#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    Timeout,
    Panic,
    Exception,
    Exit(Option<i32>),
    Markers,
}

pub fn assess(log: &str, code: Option<i32>, timed_out: bool) -> Result<(), Failure> {
    if timed_out {
        return Err(Failure::Timeout);
    }
    if log.contains("[PANIC]") {
        return Err(Failure::Panic);
    }
    if log.contains("[EXCEPTION]") {
        return Err(Failure::Exception);
    }
    // isa-debug-exit transforms the guest value as (value << 1) | 1.
    if code != Some(33) {
        return Err(Failure::Exit(code));
    }
    let mut remaining = log;
    for marker in [
        "[BOOT:ENTRY]",
        "[BOOT:PROTOCOL]",
        "[BOOT:MEMORY]",
        "[CPU:PAGING]",
        "[CPU:TABLES]",
        "[TRAP:BREAKPOINT]",
        "[CPU:ACPI]",
        "[CPU:IOAPIC]",
        "[CPU:APIC]",
        "[MM:FRAMES]",
        "[MM:TABLES]",
        "[MM:HEAP]",
        "[MM:STACK]",
        "[MM:TEST]",
        "[MM:ALLOC]",
        "[MM:READY]",
        "[CPU:TIMER]",
        "[TRAP:BREAKPOINT]",
        "[USER:CPU]",
        "[APP:RUST]",
        "[USER:EXIT] pid=1 status=0",
        "[APP:C]",
        "[USER:EXIT] pid=2 status=7",
        "[USER:COPYFAULT] write=false",
        "[USER:COPYFAULT] write=true",
        "[USER:LOADER]",
        "[USER:TEST]",
        "[USER:READY]",
        "[BOOT:OK]",
        "[TEST:PASS]",
    ] {
        let Some(index) = remaining.find(marker) else {
            return Err(Failure::Markers);
        };
        remaining = &remaining[index + marker.len()..];
    }
    let timer = log
        .lines()
        .find(|line| line.starts_with("[CPU:TIMER]"))
        .ok_or(Failure::Markers)?;
    let ticks = field(timer, "ticks")
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or(Failure::Markers)?;
    let nanoseconds = field(timer, "monotonic_ns")
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or(Failure::Markers)?;
    if ticks < 8
        || ticks.checked_mul(10_000_000) != Some(nanoseconds)
        || !log.contains("saved_if=0 return=iretq")
        || !log.contains("saved_if=1 return=iretq")
    {
        return Err(Failure::Markers);
    }
    let tables = log
        .lines()
        .find(|line| line.starts_with("[MM:TABLES]"))
        .ok_or(Failure::Markers)?;
    let memory = log
        .lines()
        .find(|line| line.starts_with("[MM:TEST]"))
        .ok_or(Failure::Markers)?;
    let heap = log
        .lines()
        .find(|line| line.starts_with("[MM:ALLOC]"))
        .ok_or(Failure::Markers)?;
    if field(tables, "boot_table_refs") != Some("0")
        || field(tables, "NX") != Some("on")
        || field(tables, "WP") != Some("on")
        || field(tables, "direct_map") != Some("NX")
        || field(memory, "mappings") != Some("256")
        || field(memory, "invalid_free") != Some("rejected")
        || field(memory, "frame_oom") != Some("recovered")
        || field(memory, "map_oom") != Some("rolled_back")
        || field(heap, "coalesced") != Some("true")
        || field(heap, "fallible_oom") != Some("recovered")
        || field(heap, "used") != Some("0")
    {
        return Err(Failure::Markers);
    }
    let user = log
        .lines()
        .find(|line| line.starts_with("[USER:TEST]"))
        .ok_or(Failure::Markers)?;
    if field(user, "cases") != Some("11")
        || field(user, "repeated") != Some("8")
        || field(user, "pointers") != Some("rejected")
        || field(user, "syscalls") != Some("checked")
        || field(user, "faults") != Some("contained")
        || field(user, "frames_restored") != Some("true")
        || field(user, "user_irqs")
            .and_then(|v| v.parse::<u64>().ok())
            .is_none_or(|n| n == 0)
        || !log.contains("args: hello-rust AnovaOS ring3; BSS=zero x87/SSE2=ok")
        || !log.contains("args: hello-c native-v1; BSS=zero SSE2=ok")
        || !log.contains("partial_oom=rolled_back")
        || log.contains("[APP:FAIL]")
    {
        return Err(Failure::Markers);
    }
    for case in [
        "syscalls",
        "timer",
        "page-fault",
        "kernel-read",
        "kernel-write",
        "stack-guard",
        "invalid-opcode",
        "privileged",
        "bad-rsp",
        "read-only",
        "no-execute",
    ] {
        if log
            .matches(&format!("[USER:CASE] name={case} passed=true"))
            .count()
            != 1
        {
            return Err(Failure::Markers);
        }
    }
    Ok(())
}

pub struct Fault {
    pub name: &'static str,
    pub vector: u64,
    pub error: &'static str,
    pub ist: u8,
}

fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_whitespace().find_map(|word| {
        let (name, value) = word.split_once('=')?;
        (name == key).then_some(value)
    })
}

pub fn reached_fault(log: &str, code: Option<i32>, fault: Fault) -> bool {
    if code != Some(37) || log.contains("[PANIC]") || log.contains("[TEST:PASS]") {
        return false;
    }
    let injection = format!("[TEST:INJECT] {}", fault.name);
    let Some((before, after)) = log.split_once(&injection) else {
        return false;
    };
    if !before.contains("[CPU:TABLES]") {
        return false;
    }
    let Some(exception) = after.lines().find(|line| line.starts_with("[EXCEPTION]")) else {
        return false;
    };
    let values_match = field(exception, "vector").and_then(|n| n.parse().ok())
        == Some(fault.vector)
        && field(exception, "name")
            == Some(if fault.vector == 14 {
                "page-fault"
            } else {
                fault.name
            })
        && field(exception, "error") == Some(fault.error)
        && field(exception, "ist").and_then(|n| n.parse().ok()) == Some(fault.ist)
        && field(exception, "current_if") == Some("0");
    let injected_line = after.lines().next().unwrap_or_default();
    let address_matches = match fault.vector {
        14 => {
            field(injected_line, "address")
                .is_some_and(|address| field(exception, "cr2") == Some(address))
                && log.contains(match fault.error {
                    "0x3" => "[PAGE:ERROR] present=1 write=1 user=0 reserved=0 instruction=0",
                    "0x11" => "[PAGE:ERROR] present=1 write=0 user=0 reserved=0 instruction=1",
                    _ => "[PAGE:ERROR] present=0 write=1 user=0 reserved=0 instruction=0",
                })
        }
        8 => field(injected_line, "guard")
            .and_then(|g| u64::from_str_radix(g.strip_prefix("0x")?, 16).ok())
            .zip(
                field(exception, "rsp")
                    .and_then(|g| u64::from_str_radix(g.strip_prefix("0x")?, 16).ok()),
            )
            .is_some_and(|(guard, rsp)| guard.checked_add(4096) == Some(rsp)),
        13 => log.contains("[GP:ERROR] external=0 table=0 selector_index=8191"),
        _ => true,
    };
    let m3 = matches!(
        fault.name,
        "stack-guard" | "read-only" | "no-execute" | "direct-no-execute" | "unmapped"
    );
    values_match
        && address_matches
        && log.contains("[REGISTERS]")
        && (!m3 || before.contains("[MM:READY]"))
}

pub fn reached_oom(log: &str, code: Option<i32>) -> bool {
    let Some((before, after)) = log.split_once("[TEST:INJECT] oom") else {
        return false;
    };
    code == Some(35)
        && before.contains("[MM:READY]")
        && after.contains("[MM:OOM] size=4194304")
        && after.contains("[PANIC]")
        && after.contains("memory allocation of 4194304 bytes failed")
        && !log.contains("[TEST:PASS]")
        && !log.contains("[EXCEPTION]")
}

#[cfg(test)]
mod tests {
    use super::*;
    const GOOD: &str = concat!(
        "[BOOT:ENTRY]\n[BOOT:PROTOCOL]\n[BOOT:MEMORY]\n[CPU:PAGING]\n[CPU:TABLES]\n[TRAP:BREAKPOINT] saved_if=0 return=iretq\n[CPU:ACPI]\n[CPU:IOAPIC]\n[CPU:APIC]\n[MM:FRAMES]\n[MM:TABLES] boot_table_refs=0 NX=on WP=on direct_map=NX\n[MM:HEAP]\n[MM:STACK]\n[MM:TEST] mappings=256 invalid_free=rejected frame_oom=recovered map_oom=rolled_back\n[MM:ALLOC] coalesced=true fallible_oom=recovered used=0\n[MM:READY]\n[CPU:TIMER] ticks=8 monotonic_ns=80000000\n[TRAP:BREAKPOINT] saved_if=1 return=iretq\n",
        "[USER:CPU] syscall=true return=iretq\n[APP:RUST] args: hello-rust AnovaOS ring3; BSS=zero x87/SSE2=ok\n[USER:EXIT] pid=1 status=0\n[APP:C] args: hello-c native-v1; BSS=zero SSE2=ok\n[USER:EXIT] pid=2 status=7\n[USER:COPYFAULT] write=false recovered=true\n[USER:COPYFAULT] write=true recovered=true\n[USER:LOADER] partial_oom=rolled_back\n[USER:CASE] name=syscalls passed=true\n[USER:CASE] name=timer passed=true\n[USER:CASE] name=page-fault passed=true\n[USER:CASE] name=kernel-read passed=true\n[USER:CASE] name=kernel-write passed=true\n[USER:CASE] name=stack-guard passed=true\n[USER:CASE] name=invalid-opcode passed=true\n[USER:CASE] name=privileged passed=true\n[USER:CASE] name=bad-rsp passed=true\n[USER:CASE] name=read-only passed=true\n[USER:CASE] name=no-execute passed=true\n[USER:TEST] cases=11 repeated=8 pointers=rejected syscalls=checked faults=contained frames_restored=true user_irqs=1\n[USER:READY]\n[BOOT:OK]\n[TEST:PASS]\n",
    );

    #[test]
    fn requires_both_ordered_stages_and_debug_exit() {
        assert_eq!(assess(GOOD, Some(33), false), Ok(()));
        assert_eq!(assess(GOOD, Some(0), false), Err(Failure::Exit(Some(0))));
        assert_eq!(assess(GOOD, None, false), Err(Failure::Exit(None)));
        assert_eq!(
            assess("[BOOT:ENTRY]", Some(33), false),
            Err(Failure::Markers)
        );
        assert_eq!(
            assess(
                "[TEST:PASS][BOOT:OK][BOOT:MEMORY][BOOT:PROTOCOL][BOOT:ENTRY]",
                Some(33),
                false
            ),
            Err(Failure::Markers)
        );
    }

    #[test]
    fn rejects_panic_and_timeout_even_with_success_markers() {
        assert_eq!(
            assess(&format!("{GOOD}[PANIC]"), Some(33), false),
            Err(Failure::Panic)
        );
        assert_eq!(assess(GOOD, Some(33), true), Err(Failure::Timeout));
        assert_eq!(assess("[PANIC]", Some(35), false), Err(Failure::Panic));
        assert_eq!(
            assess(&format!("{GOOD}[EXCEPTION]"), Some(33), false),
            Err(Failure::Exception)
        );
        assert_eq!(
            assess(&GOOD.replace("ticks=8", "ticks=1"), Some(33), false),
            Err(Failure::Markers)
        );
    }

    #[test]
    fn fault_gate_requires_real_vector_ist_address_and_exit() {
        let log = "[CPU:TABLES]\n[TEST:INJECT] page-fault address=0xffffffff80025000\n[EXCEPTION] vector=14 name=page-fault error=0x2 cr2=0xffffffff80025000 current_if=0 ist=2\n[REGISTERS]\n[PAGE:ERROR] present=0 write=1 user=0 reserved=0 instruction=0\n";
        let fault = || Fault {
            name: "page-fault",
            vector: 14,
            error: "0x2",
            ist: 2,
        };
        assert!(reached_fault(log, Some(37), fault()));
        assert!(!reached_fault(log, Some(33), fault()));
        for (old, new) in [
            ("vector=14", "vector=8"),
            ("ist=2", "ist=0"),
            ("cr2=0xffffffff80025000", "cr2=0x0"),
            ("[TEST:INJECT]", "[TEST:NOTREACHED]"),
        ] {
            assert!(!reached_fault(&log.replace(old, new), Some(37), fault()));
        }
    }

    #[test]
    fn m3_gate_requires_memory_checks_and_actual_oom_panic() {
        for (old, new) in [
            ("[MM:READY]", "[MM:NOTREADY]"),
            ("boot_table_refs=0", "boot_table_refs=1"),
            ("WP=on", "WP=off"),
            ("coalesced=true", "coalesced=false"),
            ("used=0", "used=16"),
        ] {
            assert_eq!(
                assess(&GOOD.replace(old, new), Some(33), false),
                Err(Failure::Markers)
            );
        }
        let log = "[MM:READY]\n[TEST:INJECT] oom\n[MM:OOM] size=4194304 align=1 used=0 capacity=2097152\n[PANIC] memory allocation of 4194304 bytes failed";
        assert!(reached_oom(log, Some(35)));
        assert!(!reached_oom(log, Some(33)));
        assert!(!reached_oom(
            &log.replace("memory allocation of", "unrelated panic"),
            Some(35)
        ));
    }

    #[test]
    fn m4_gate_rejects_missing_isolation_or_reclamation_evidence() {
        for (old, new) in [
            ("[USER:READY]", "[USER:NOTREADY]"),
            ("partial_oom=rolled_back", "partial_oom=leaked"),
            ("user_irqs=1", "user_irqs=0"),
            ("frames_restored=true", "frames_restored=false"),
            (
                "name=kernel-write passed=true",
                "name=kernel-write passed=false",
            ),
            ("[USER:COPYFAULT] write=false", "[USER:NOTCOPY] write=false"),
            ("BSS=zero x87/SSE2=ok", "BSS=bad"),
        ] {
            assert_eq!(
                assess(&GOOD.replace(old, new), Some(33), false),
                Err(Failure::Markers)
            );
        }
    }
}
