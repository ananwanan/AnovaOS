// SPDX-License-Identifier: Apache-2.0
//! Interpret the guest transcript and isa-debug-exit result together.

#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    Timeout,
    Panic,
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
    // isa-debug-exit transforms the guest value as (value << 1) | 1.
    if code != Some(33) {
        return Err(Failure::Exit(code));
    }
    let mut remaining = log;
    for marker in [
        "[BOOT:ENTRY]",
        "[BOOT:PROTOCOL]",
        "[BOOT:MEMORY]",
        "[BOOT:OK]",
        "[TEST:PASS]",
    ] {
        let Some(index) = remaining.find(marker) else {
            return Err(Failure::Markers);
        };
        remaining = &remaining[index + marker.len()..];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const GOOD: &str = "[BOOT:ENTRY]\n[BOOT:PROTOCOL]\n[BOOT:MEMORY]\n[BOOT:OK]\n[TEST:PASS]\n";

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
    }
}
