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
        "[CPU:TIMER]",
        "[TRAP:BREAKPOINT]",
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
        && field(exception, "name") == Some(fault.name)
        && field(exception, "error") == Some(fault.error)
        && field(exception, "ist").and_then(|n| n.parse().ok()) == Some(fault.ist)
        && field(exception, "current_if") == Some("0");
    let injected_line = after.lines().next().unwrap_or_default();
    let address_matches = match fault.vector {
        14 => {
            field(injected_line, "address")
                .is_some_and(|address| field(exception, "cr2") == Some(address))
                && log.contains("[PAGE:ERROR] present=0 write=1 user=0 reserved=0 instruction=0")
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
    values_match && address_matches && log.contains("[REGISTERS]")
}

#[cfg(test)]
mod tests {
    use super::*;
    const GOOD: &str = "[BOOT:ENTRY]\n[BOOT:PROTOCOL]\n[BOOT:MEMORY]\n[CPU:PAGING]\n[CPU:TABLES]\n[TRAP:BREAKPOINT] saved_if=0 return=iretq\n[CPU:ACPI]\n[CPU:IOAPIC]\n[CPU:APIC]\n[CPU:TIMER] ticks=8 monotonic_ns=80000000\n[TRAP:BREAKPOINT] saved_if=1 return=iretq\n[BOOT:OK]\n[TEST:PASS]\n";

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
}
