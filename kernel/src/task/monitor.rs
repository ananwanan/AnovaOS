// SPDX-License-Identifier: Apache-2.0
//! Sample bounded BSP-owned state without allocation, disk I/O or scheduling.
//! Inspect only self, direct children and owned tagged families; stop native children only.

use super::*;
use anova_abi::monitor as m;
use anova_storage::filesystem::Format;
use mm::uaccess;

const _: () = assert!(CAPACITY == m::MAX_TASKS);

impl Kernel {
    fn resources(&self) -> m::Resources {
        let frames = mm::user::frame_usage();
        let (heap_bytes, heap_used) = mm::heap_usage();
        let (opens_used, _) = self.fs.live_objects();
        let (ptys_total, ptys_used) = self.fs.pty_master_usage();
        #[cfg(not(feature = "gui"))]
        let (gui_total, gui_used, gui_connected, gui_session) = (0, 0, 0, 0);
        #[cfg(feature = "gui")]
        let (gui_total, gui_used, gui_connected, gui_session) = self.gui.usage();
        let mut r = m::Resources {
            ticks: apic::ticks(),
            page_bytes: anova_memory::PAGE_SIZE,
            frames_total: frames.total as u64,
            frames_free: frames.free as u64,
            frames_reserved: frames.reserved as u64,
            heap_bytes: heap_bytes as u64,
            heap_used: heap_used as u64,
            tasks_total: CAPACITY as u32,
            nodes_total: anova_vfs::NODE_LIMIT as u32,
            nodes_used: self.fs.occupied_nodes() as u32,
            node_slots: self.fs.node_slots() as u32,
            opens_total: anova_vfs::OPEN_LIMIT as u32,
            opens_used: opens_used as u32,
            fds_per_process: anova_vfs::fd::FD_LIMIT as u32,
            ptys_total: ptys_total as u32,
            ptys_used: ptys_used as u32,
            ptys_managed: self.ptys.iter().flatten().count() as u32,
            gui_total: gui_total as u32,
            gui_used: gui_used as u32,
            gui_connected: gui_connected as u32,
            gui_session,
            ..m::Resources::default()
        };
        for index in 0..CAPACITY {
            if let Some(state) = self.monitor_state(index) {
                r.tasks[state as usize] += 1;
            }
        }
        if let Some(disk) = self.fs.storage_space() {
            r.disk_format = match disk.format {
                Format::Ext2 => 2,
                Format::Ext4 => 4,
            };
            r.block_bytes = disk.block_bytes;
            r.blocks_total = disk.blocks;
            r.inodes_total = disk.inodes;
            if disk.read_only {
                r.disk_flags |= m::DISK_READ_ONLY;
            }
            if disk.journaled {
                r.disk_flags |= m::DISK_JOURNALED;
            }
            if let Some((blocks, inodes)) = disk.free {
                r.blocks_free = blocks;
                r.inodes_free = inodes;
            } else {
                r.disk_flags |= m::DISK_FAILED;
            }
        }
        r
    }

    fn monitor_state(&self, index: usize) -> Option<u32> {
        let process = self.processes[index].as_ref()?;
        Some(match self.policy.entry(index).state {
            State::Empty => return None,
            State::Zombie(_) => m::ZOMBIE,
            _ if process.thread.stopped => m::STOPPED,
            State::Ready => m::READY,
            State::Running => m::RUNNING,
            State::Sleeping(_) => m::SLEEPING,
            State::Waiting(_) => m::WAITING,
            State::Blocked => m::BLOCKED,
        })
    }

    fn monitor_task(&self, owner: u64, index: usize) -> Option<m::Task> {
        let state = self.monitor_state(index)?;
        let entry = self.policy.entry(index);
        let process = self.process(index);
        let group = process.thread.tgid;
        let direct = group == entry.pid && entry.parent == owner;
        let in_child_group = group != entry.pid
            && self.policy.entries().iter().any(|leader| {
                leader.state != State::Empty && leader.pid == group && leader.parent == owner
            });
        if entry.pid != owner
            && !direct
            && !in_child_group
            && !process
                .pty_member
                .is_some_and(|id| self.owns_pty_family(owner, id))
        {
            return None;
        }
        let kind = if !process.linux {
            m::NATIVE
        } else if group == entry.pid {
            m::LINUX_PROCESS
        } else {
            m::LINUX_THREAD
        };
        let exit = match entry.state {
            State::Zombie(status) => status,
            _ => ExitStatus::default(),
        };
        Some(m::Task {
            pid: entry.pid,
            parent: entry.parent,
            group,
            started_tick: process.started_tick,
            family: process.pty_member.unwrap_or(0),
            kind,
            state,
            flags: if direct && kind == m::NATIVE && state != m::ZOMBIE {
                m::CAN_STOP
            } else {
                0
            },
            exit,
            reserved: 0,
        })
    }

    pub(super) fn monitor(&mut self, caller: usize, f: &TrapFrame) -> Result<i64, i64> {
        let op = m::decode(f.rdi).ok_or(INVALID_ARGUMENT)?;
        if op == m::STOP {
            if f.rdx != 0 {
                return Err(INVALID_ARGUMENT);
            }
            let child = self.policy.child(caller, f.rsi)?;
            let process = self.process(child);
            if process.linux || process.thread.tgid != f.rsi {
                return Err(UNSUPPORTED);
            }
            if matches!(self.policy.entry(child).state, State::Zombie(_)) {
                return Ok(1);
            }
            self.finish(child, Outcome::Signal(15));
            return Ok(0);
        }
        let space = &self.process(caller).image.as_ref().unwrap().space;
        let output = UserAddr(f.rsi);
        if op == m::RESOURCES {
            if f.rdx != m::RESOURCE_BYTES as u64 {
                return Err(INVALID_ARGUMENT);
            }
            let resources = self.resources();
            space.with_active(|| uaccess::to_user(space, output, resources.bytes()))?;
            return Ok(0);
        }
        if f.rdx != (m::MAX_TASKS * m::TASK_BYTES) as u64 {
            return Err(INVALID_ARGUMENT);
        }
        let mut bytes = [0; m::MAX_TASKS * m::TASK_BYTES];
        let owner = self.policy.entry(caller).pid;
        let mut count = 0;
        for task in (0..CAPACITY).filter_map(|i| self.monitor_task(owner, i)) {
            bytes[count..count + m::TASK_BYTES].copy_from_slice(task.bytes());
            count += m::TASK_BYTES;
        }
        space.with_active(|| uaccess::to_user(space, output, &bytes))?;
        Ok((count / m::TASK_BYTES) as i64)
    }
}
