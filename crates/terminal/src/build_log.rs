// SPDX-License-Identifier: Apache-2.0
//! Fixed compiler-attempt records and raw merged-output tails, independent of task-slot reuse.
//! Evict oldest bytes/records explicitly; bounded line iteration never allocates or parses status.

use anova_abi::ExitStatus;
pub const RECORDS: usize = 4;
pub const CAPACITY: usize = 8192;
pub const COLUMNS: usize = 50;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Empty,
    Starting,
    Running,
    Draining,
    Finished,
    StartFailed,
    CaptureFailed,
}

pub struct Record {
    pub attempt: u64,
    pub id: u64,
    pub phase: Phase,
    pub status: Option<ExitStatus>,
    pub error: i64,
    pub published: bool,
    pub incomplete: bool,
    pub dropped: u64,
    bytes: [u8; CAPACITY],
    start: usize,
    length: usize,
}
impl Record {
    const fn new() -> Self {
        Self {
            attempt: 0,
            id: 0,
            phase: Phase::Empty,
            status: None,
            error: 0,
            published: false,
            incomplete: false,
            dropped: 0,
            bytes: [0; CAPACITY],
            start: 0,
            length: 0,
        }
    }
    pub fn append(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.length == CAPACITY {
                self.start = (self.start + 1) % CAPACITY;
                self.dropped = self.dropped.saturating_add(1);
            } else {
                self.length += 1;
            }
            self.bytes[(self.start + self.length - 1) % CAPACITY] = byte;
        }
    }
    pub fn len(&self) -> usize {
        self.length
    }
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
    pub fn lines(&self) -> Lines<'_> {
        Lines {
            record: self,
            offset: 0,
        }
    }
}
pub struct Lines<'a> {
    record: &'a Record,
    offset: usize,
}
impl Iterator for Lines<'_> {
    type Item = ([u8; COLUMNS], usize);
    fn next(&mut self) -> Option<Self::Item> {
        if self.offset == self.record.length {
            return None;
        }
        let mut line = [b' '; COLUMNS];
        let mut n = 0;
        while self.offset < self.record.length && n < COLUMNS {
            let b = self.record.bytes[(self.record.start + self.offset) % CAPACITY];
            self.offset += 1;
            if b == b'\n' {
                break;
            }
            if b != b'\r' {
                line[n] = if b.is_ascii_graphic() || b == b' ' {
                    b
                } else {
                    b'?'
                };
                n += 1;
            }
        }
        Some((line, n))
    }
}
pub struct History {
    pub records: [Record; RECORDS],
    pub attempts: u64,
    pub evicted: u64,
}
impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}
impl History {
    pub const fn new() -> Self {
        Self {
            records: [const { Record::new() }; RECORDS],
            attempts: 0,
            evicted: 0,
        }
    }
    pub fn begin(&mut self) -> Option<usize> {
        let attempt = self.attempts.checked_add(1)?;
        let index = ((attempt - 1) % RECORDS as u64) as usize;
        if self.records[index].attempt != 0 {
            self.evicted += 1;
        }
        let r = &mut self.records[index];
        // Reset metadata in place; no large stack temporary and no stale bytes exposed.
        r.attempt = attempt;
        r.id = 0;
        r.phase = Phase::Starting;
        r.status = None;
        r.error = 0;
        r.published = false;
        r.incomplete = false;
        r.dropped = 0;
        r.start = 0;
        r.length = 0;
        self.attempts = attempt;
        Some(index)
    }
    pub fn neighbor(&self, index: usize, newer: bool) -> usize {
        let current = self.records[index].attempt;
        self.records
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r.attempt != 0
                    && if newer {
                        r.attempt > current
                    } else {
                        r.attempt < current
                    }
            })
            .min_by_key(|(_, r)| r.attempt.abs_diff(current))
            .map_or(index, |(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    #[test]
    fn raw_tail_is_bounded_and_reports_exact_eviction() {
        let mut h = History::new();
        let i = h.begin().unwrap();
        let r = &mut h.records[i];
        for n in 0..CAPACITY + 1031 {
            r.append(&[(n % 93 + 33) as u8]);
        }
        assert_eq!(r.len(), CAPACITY);
        assert_eq!(r.dropped, 1031);
        for n in 0..CAPACITY {
            assert_eq!(
                r.bytes[(r.start + n) % CAPACITY],
                ((n + 1031) % 93 + 33) as u8
            );
        }
        assert!(size_of::<History>() < 34000);
    }
    #[test]
    fn records_never_inherit_status_or_identity_and_evict_explicitly() {
        let mut h = History::new();
        let old = h.begin().unwrap();
        h.records[old].id = 19;
        h.records[old].status = Some(ExitStatus { reason: 0, code: 1 });
        h.records[old].append(b"failed\n");
        let second = h.begin().unwrap();
        assert_eq!(h.records[old].status.unwrap().code, 1);
        assert_eq!(h.neighbor(second, false), old);
        assert_eq!(h.neighbor(old, true), second);
        for _ in 0..3 {
            h.begin().unwrap();
        }
        assert_eq!(h.evicted, 1);
        assert_eq!(h.records[old].attempt, 5);
        assert_eq!(h.records[old].id, 0);
        assert_eq!(h.records[old].status, None);
        assert_eq!(h.records[old].len(), 0);
        h.attempts = u64::MAX;
        assert_eq!(h.begin(), None);
    }
    #[test]
    fn rendering_wraps_and_sanitizes_without_interpreting_output_as_status() {
        let mut r = Record::new();
        r.append(b"OK exit=0\r\n\x1b[31m\t");
        r.append(&[b'x'; 60]);
        let lines: std::vec::Vec<_> = r.lines().collect();
        assert_eq!(&lines[0].0[..lines[0].1], b"OK exit=0");
        assert_eq!(&lines[1].0[..6], b"?[31m?");
        assert_eq!(lines[1].1, 50);
        assert_eq!(lines[2].1, 16);
        assert_eq!(r.status, None);
    }
}
