// SPDX-License-Identifier: Apache-2.0
//! Sample resource use at scheduler boundaries without allocation or a new syscall ABI.
//! Each scheduler instance starts afresh; only successful shutdown emits its cumulative summary.
//! These observations omit temporary allocations within syscalls and do not establish peak RSS.
//! Elapsed time uses the coarse IRQ-derived clock, which does not charge all kernel execution.
use super::{CAPACITY, Kernel, State, apic, mm, serial};

pub(super) struct Samples {
    started_ns: u64,
    baseline_free_frames: usize,
    min_free_frames: usize,
    heap_peak_bytes: usize,
    live_tasks_peak: usize,
    occupied_slots_peak: usize,
    fd_per_process_peak: usize,
    vfs_node_slots_peak: usize,
    count: u64,
}

impl Samples {
    pub(super) fn new() -> Self {
        let free = mm::user::free_frames();
        Self {
            started_ns: apic::monotonic_ns(),
            baseline_free_frames: free,
            min_free_frames: free,
            heap_peak_bytes: mm::heap_used(),
            live_tasks_peak: 0,
            occupied_slots_peak: 0,
            fd_per_process_peak: 0,
            vfs_node_slots_peak: 0,
            count: 0,
        }
    }

    pub(super) fn report(&self) {
        serial::write(format_args!(
            "[M15:RESOURCES] scope=scheduler-instance sampling=boundaries clock=irq-monotonic samples={} baseline_free_frames={} min_free_frames={} peak_frame_delta={} heap_peak_bytes={} live_tasks_peak={} occupied_slots_peak={} fd_per_process_peak={} vfs_node_slots_peak={} elapsed_ns={}\n",
            self.count,
            self.baseline_free_frames,
            self.min_free_frames,
            self.baseline_free_frames
                .saturating_sub(self.min_free_frames),
            self.heap_peak_bytes,
            self.live_tasks_peak,
            self.occupied_slots_peak,
            self.fd_per_process_peak,
            self.vfs_node_slots_peak,
            apic::monotonic_ns().saturating_sub(self.started_ns),
        ));
    }
}

impl Kernel {
    pub(super) fn sample_resources(&mut self) {
        let mut occupied = 0;
        let mut live = 0;
        for slot in 0..CAPACITY {
            let state = self.policy.entry(slot).state;
            occupied += usize::from(state != State::Empty);
            live += usize::from(!matches!(state, State::Empty | State::Zombie(_)));
        }
        let descriptors = self
            .processes
            .iter()
            .flatten()
            .map(|process| process.files.count())
            .max()
            .unwrap_or(0);
        let r = &mut self.resources;
        r.count = r.count.saturating_add(1);
        r.min_free_frames = r.min_free_frames.min(mm::user::free_frames());
        r.heap_peak_bytes = r.heap_peak_bytes.max(mm::heap_used());
        r.live_tasks_peak = r.live_tasks_peak.max(live);
        r.occupied_slots_peak = r.occupied_slots_peak.max(occupied);
        r.fd_per_process_peak = r.fd_per_process_peak.max(descriptors);
        r.vfs_node_slots_peak = r.vfs_node_slots_peak.max(self.fs.node_slots());
    }
}
