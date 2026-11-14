// SPDX-License-Identifier: Apache-2.0
//! Fixed native monitoring records for BSP resource snapshots and permission-scoped tasks.
//! Observing a task never consumes its wait status. A PID identifies one task lifecycle.

use crate::ExitStatus;

pub const SYSCALL: u64 = 0x105;
pub const VERSION: u32 = 1;
pub const RESOURCES: u32 = 0;
pub const TASKS: u32 = 1;
pub const STOP: u32 = 2;
pub const MAX_TASKS: usize = 32;

pub const READY: u32 = 0;
pub const RUNNING: u32 = 1;
pub const SLEEPING: u32 = 2;
pub const WAITING: u32 = 3;
pub const BLOCKED: u32 = 4;
pub const STOPPED: u32 = 5;
pub const ZOMBIE: u32 = 6;
pub const STATES: usize = 7;

pub const NATIVE: u32 = 1;
pub const LINUX_PROCESS: u32 = 2;
pub const LINUX_THREAD: u32 = 3;
pub const CAN_STOP: u32 = 1;

pub const DISK_READ_ONLY: u32 = 1;
pub const DISK_FAILED: u32 = 2;
pub const DISK_JOURNALED: u32 = 4;

/// Current allocator and service counters at one scheduler boundary.
/// Disk format zero means absent. Free disk counts are unavailable after a fatal mount error.
/// GUI occupancy includes disconnected entries awaiting compositor teardown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Resources {
    pub ticks: u64,
    pub page_bytes: u64,
    pub frames_total: u64,
    pub frames_free: u64,
    pub frames_reserved: u64,
    pub heap_bytes: u64,
    pub heap_used: u64,
    pub gui_session: u64,
    pub tasks_total: u32,
    pub tasks: [u32; STATES],
    pub nodes_total: u32,
    pub nodes_used: u32,
    pub node_slots: u32,
    pub opens_total: u32,
    pub opens_used: u32,
    pub fds_per_process: u32,
    pub ptys_total: u32,
    pub ptys_used: u32,
    pub ptys_managed: u32,
    pub gui_total: u32,
    pub gui_used: u32,
    pub gui_connected: u32,
    pub disk_format: u32,
    pub disk_flags: u32,
    pub block_bytes: u32,
    pub blocks_total: u32,
    pub blocks_free: u32,
    pub inodes_total: u32,
    pub inodes_free: u32,
    pub reserved: u32,
}

/// State and identity for the caller, its immediate children or a family owned through its PTY.
/// Exit status is populated only for a zombie. `family` is zero outside a tagged PTY.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Task {
    pub pid: u64,
    pub parent: u64,
    pub group: u64,
    pub started_tick: u64,
    pub family: u64,
    pub kind: u32,
    pub state: u32,
    pub flags: u32,
    pub exit: ExitStatus,
    pub reserved: u32,
}

pub const RESOURCE_BYTES: usize = 176;
pub const TASK_BYTES: usize = 64;
const _: () = assert!(size_of::<Resources>() == RESOURCE_BYTES);
const _: () = assert!(size_of::<Task>() == TASK_BYTES);

impl Resources {
    pub fn bytes(&self) -> &[u8; RESOURCE_BYTES] {
        // SAFETY: The C layout has consecutive integer fields and no padding. The sizes
        // above match the sum of field widths, and all fields are initialized.
        unsafe { &*(core::ptr::from_ref(self).cast()) }
    }
}

impl Task {
    pub fn bytes(&self) -> &[u8; TASK_BYTES] {
        // SAFETY: The C layout consists of five u64s and six u32s, including ExitStatus.
        // Each field is initialized and the size assertion rules out padding.
        unsafe { &*(core::ptr::from_ref(self).cast()) }
    }
}

/// For queries RSI is a writable buffer and RDX is its exact byte length.
/// STOP takes an exact PID in RSI and zero in RDX. It returns 0 for a stop and
/// 1 if an owned native child already exited. wait/POLL still owns reaping.
pub const fn command(operation: u32) -> u64 {
    (VERSION as u64) << 32 | operation as u64
}

pub const fn decode(command: u64) -> Option<u32> {
    if command >> 32 == VERSION as u64 && command as u32 <= STOP {
        Some(command as u32)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::offset_of;

    #[test]
    fn version_and_layout_are_fixed() {
        for operation in RESOURCES..=STOP {
            assert_eq!(decode(command(operation)), Some(operation));
        }
        for invalid in [0, command(STOP + 1), 2 << 32, u64::MAX] {
            assert_eq!(decode(invalid), None);
        }
        assert_eq!(align_of::<Resources>(), 8);
        assert_eq!(offset_of!(Resources, tasks), 68);
        assert_eq!(offset_of!(Resources, disk_format), 144);
        assert_eq!(offset_of!(Resources, reserved), 172);
        assert_eq!(align_of::<Task>(), 8);
        assert_eq!(offset_of!(Task, family), 32);
        assert_eq!(offset_of!(Task, exit), 52);
        let task = Task {
            pid: 0x0102_0304_0506_0708,
            exit: ExitStatus {
                reason: 3,
                code: 15,
            },
            ..Task::default()
        };
        assert_eq!(&task.bytes()[..8], &task.pid.to_le_bytes());
        assert_eq!(&task.bytes()[52..60], &[3, 0, 0, 0, 15, 0, 0, 0]);
    }
}
