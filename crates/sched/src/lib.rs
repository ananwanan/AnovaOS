// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Bounded BSP scheduling policy and parent/child lifecycle without hardware or allocation.
//! The kernel owns contexts/resources; this policy never switches stacks or takes locks.
use anova_abi::{ExitStatus, NO_CHILD, PROCESS_LIMIT};

pub const CAPACITY: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Empty,
    Ready,
    Running,
    Sleeping(u64),
    Waiting(u64),
    Blocked,
    Zombie(ExitStatus),
}

#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub pid: u64,
    pub parent: u64,
    pub state: State,
}

pub struct Scheduler {
    entries: [Entry; CAPACITY],
    next_pid: u64,
    cursor: usize,
}

impl Scheduler {
    pub fn new(first_pid: u64) -> Self {
        assert!(first_pid > 0);
        Self {
            entries: [Entry {
                pid: 0,
                parent: 0,
                state: State::Empty,
            }; CAPACITY],
            next_pid: first_pid,
            cursor: 0,
        }
    }

    pub fn entry(&self, index: usize) -> Entry {
        self.entries[index]
    }

    pub fn insert(&mut self, parent: u64) -> Result<usize, i64> {
        let index = self
            .entries
            .iter()
            .position(|e| e.state == State::Empty)
            .ok_or(PROCESS_LIMIT)?;
        if self.next_pid > i64::MAX as u64 {
            return Err(PROCESS_LIMIT);
        }
        let next = self.next_pid.checked_add(1).ok_or(PROCESS_LIMIT)?;
        self.entries[index] = Entry {
            pid: self.next_pid,
            parent,
            state: State::Ready,
        };
        self.next_pid = next;
        Ok(index)
    }

    /// Roll back an unpublished child after resource allocation fails. PIDs are not reused.
    pub fn remove(&mut self, index: usize) {
        self.entries[index].state = State::Empty;
    }

    /// Commit exec from a nonleader after the kernel has removed all sibling entries.
    pub fn exec_identity(&mut self, index: usize, pid: u64, parent: u64) {
        assert!(
            !self
                .entries
                .iter()
                .enumerate()
                .any(|(k, e)| k != index && e.state != State::Empty && e.pid == pid)
        );
        self.entries[index].pid = pid;
        self.entries[index].parent = parent;
    }

    pub fn ready(&mut self, index: usize) {
        self.entries[index].state = State::Ready;
    }

    pub fn block(&mut self, index: usize) {
        self.entries[index].state = State::Blocked;
    }

    pub fn sleep(&mut self, index: usize, deadline: u64) {
        self.entries[index].state = State::Sleeping(deadline);
    }

    pub fn select(&mut self, now: u64) -> Option<usize> {
        for entry in &mut self.entries {
            if matches!(entry.state, State::Sleeping(deadline) if now >= deadline) {
                entry.state = State::Ready;
            }
        }
        for offset in 0..CAPACITY {
            let index = (self.cursor + offset) % CAPACITY;
            if self.entries[index].state == State::Ready {
                self.entries[index].state = State::Running;
                self.cursor = (index + 1) % CAPACITY;
                return Some(index);
            }
        }
        None
    }

    /// pid=0 waits for any child. A status remains a zombie until the kernel copies it out.
    pub fn wait(&mut self, parent: usize, pid: u64) -> Result<Option<usize>, i64> {
        let parent_pid = self.entries[parent].pid;
        let mut found = false;
        for (index, child) in self.entries.iter().enumerate() {
            if child.state != State::Empty
                && child.parent == parent_pid
                && (pid == 0 || pid == child.pid)
            {
                if matches!(child.state, State::Zombie(_)) {
                    return Ok(Some(index));
                }
                found = true;
            }
        }
        if !found {
            return Err(NO_CHILD);
        }
        self.entries[parent].state = State::Waiting(pid);
        Ok(None)
    }

    pub fn exit(&mut self, index: usize, status: ExitStatus) {
        let pid = self.entries[index].pid;
        let parent = self.entries[index].parent;
        self.entries[index].state = State::Zombie(status);
        for entry in &mut self.entries {
            if entry.parent == pid {
                entry.parent = 0;
            }
            if entry.pid == parent
                && matches!(entry.state, State::Waiting(target) if target == 0 || target == pid)
            {
                entry.state = State::Ready;
            }
        }
    }

    pub fn live(&self) -> bool {
        self.entries
            .iter()
            .any(|e| !matches!(e.state, State::Empty | State::Zombie(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const OK: ExitStatus = ExitStatus { reason: 0, code: 7 };

    #[test]
    fn round_robin_bounds_starvation_and_sleep_uses_deadlines() {
        let mut s = Scheduler::new(1);
        let a = s.insert(0).unwrap();
        let b = s.insert(0).unwrap();
        let c = s.insert(0).unwrap();
        for _ in 0..100 {
            for expected in [a, b, c] {
                let i = s.select(10).unwrap();
                assert_eq!(i, expected);
                s.ready(i);
            }
        }
        assert_eq!(s.select(10), Some(a));
        s.sleep(a, 12);
        s.sleep(b, 11);
        s.sleep(c, 13);
        assert_eq!(s.select(10), None);
        assert_eq!(s.select(11), Some(b));
        s.sleep(b, 20);
        assert_eq!(s.select(12), Some(a));
    }

    #[test]
    fn wait_has_no_lost_wakeup_and_rejects_nonchildren() {
        let mut s = Scheduler::new(1);
        let p = s.insert(0).unwrap();
        let c = s.insert(1).unwrap();
        let other = s.insert(0).unwrap();
        assert_eq!(s.wait(p, 3), Err(NO_CHILD));
        assert_eq!(s.wait(p, 2), Ok(None));
        assert_eq!(s.entry(p).state, State::Waiting(2));
        s.exit(other, OK);
        assert_eq!(s.entry(p).state, State::Waiting(2));
        s.exit(c, OK);
        assert_eq!(s.entry(p).state, State::Ready);
        assert_eq!(s.wait(p, 2), Ok(Some(c)));
        assert_eq!(s.wait(p, 0), Ok(Some(c)));
        s.remove(c);
        assert_eq!(s.wait(p, 0), Err(NO_CHILD));
    }

    #[test]
    fn blocked_io_requires_an_explicit_readiness_wakeup() {
        let mut s = Scheduler::new(1);
        let p = s.insert(0).unwrap();
        s.block(p);
        assert_eq!(s.select(1000), None);
        s.ready(p);
        assert_eq!(s.select(1000), Some(p));
    }

    #[test]
    fn orphans_are_adopted_and_exhausted_slots_are_reusable() {
        let mut s = Scheduler::new(1);
        let p = s.insert(0).unwrap();
        let c = s.insert(1).unwrap();
        s.exit(p, OK);
        assert_eq!(s.entry(c).parent, 0);
        for _ in 2..CAPACITY {
            s.insert(0).unwrap();
        }
        assert_eq!(s.insert(0), Err(PROCESS_LIMIT));
        s.remove(p);
        assert_eq!(s.insert(0), Ok(p));
        assert_eq!(s.entry(p).pid, 33);
        let mut end = Scheduler::new(u64::MAX);
        assert_eq!(end.insert(0), Err(PROCESS_LIMIT));
        assert!(!end.live());
    }
}
