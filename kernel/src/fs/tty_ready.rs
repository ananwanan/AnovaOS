// SPDX-License-Identifier: Apache-2.0
//! Share exact PTY progress predicates between writes, poll and blocked-task wakeups.
//! Reserve output room for CRLF and echo expansion; false readiness can spin with IRQs masked.

pub const QUEUE_CAPACITY: usize = 1024;

pub const fn slave_writable(output_count: usize) -> bool {
    output_count <= QUEUE_CAPACITY - 2
}

pub const fn master_writable(
    input_count: usize,
    eof: bool,
    echo: bool,
    output_count: usize,
) -> bool {
    input_count <= 768 && !eof && (!echo || output_count <= QUEUE_CAPACITY - 8)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_free_output_byte_cannot_wake_a_writer_that_reserves_crlf() {
        assert!(slave_writable(0));
        let mut used = QUEUE_CAPACITY - 2;
        assert!(slave_writable(used));
        used += b"\r\n".len();
        assert!(!slave_writable(used));
        used -= 1;
        assert!(!slave_writable(used));
        used -= 1;
        assert!(slave_writable(used));
        assert!(!slave_writable(usize::MAX));
    }

    #[test]
    fn input_eof_and_echo_pressure_require_real_progress_before_waking() {
        assert!(master_writable(768, false, false, QUEUE_CAPACITY));
        assert!(!master_writable(769, false, false, 0));
        assert!(!master_writable(0, true, false, 0));
        assert!(master_writable(0, false, true, QUEUE_CAPACITY - 8));
        assert!(!master_writable(0, false, true, QUEUE_CAPACITY - 7));
        assert!(master_writable(0, false, false, QUEUE_CAPACITY));
        assert!(!master_writable(usize::MAX, false, false, 0));
    }
}
