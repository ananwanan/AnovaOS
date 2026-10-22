// SPDX-License-Identifier: Apache-2.0
//! Adapt bounded Linux select sets to real file/socket readiness, deadlines and signal interruption.
//! Store imported descriptor masks while sleeping; never retain a borrowed user buffer or a lock.
use super::*;

impl Kernel {
    pub(super) fn linux_select(&mut self, i: usize, f: &TrapFrame) -> Result<i64, i64> {
        if f.rdi > 32 {
            return Err(-22);
        }
        let space = &self.process(i).image.as_ref().unwrap().space;
        let mut sets = [0; 3];
        let deadline = space.with_active(|| -> Result<Option<u64>, i64> {
            if f.rdi != 0 {
                let mask = (1u64 << f.rdi) - 1;
                for (set, address) in sets.iter_mut().zip([f.rsi, f.rdx, f.r10]) {
                    if address == 0 {
                        continue;
                    }
                    let mut bytes = [0; 8];
                    uaccess::from_user(space, UserAddr(address), &mut bytes).map_err(errno)?;
                    if !space.accessible(UserAddr(address), 8, true, false) {
                        return Err(-14);
                    }
                    *set = (u64::from_le_bytes(bytes) & mask) as u32;
                }
            }
            if f.r8 == 0 {
                return Ok(None);
            }
            let mut bytes = [0; 16];
            uaccess::from_user(space, UserAddr(f.r8), &mut bytes).map_err(errno)?;
            if !space.accessible(UserAddr(f.r8), 16, true, false) {
                return Err(-14);
            }
            let seconds = i64::from_le_bytes(bytes[..8].try_into().unwrap());
            let micros = i64::from_le_bytes(bytes[8..].try_into().unwrap());
            if seconds < 0 || !(0..1_000_000).contains(&micros) {
                return Err(-22);
            }
            let ticks = (seconds as u64)
                .checked_mul(100)
                .and_then(|s| s.checked_add((micros as u64).div_ceil(10_000)))
                .and_then(|s| apic::ticks().checked_add(s))
                .ok_or(-22)?;
            Ok(Some(ticks))
        })?;
        if let Some(result) = self.select_result(i, f, sets, deadline)? {
            return Ok(result);
        }
        self.process_mut(i).thread.blocked = Some(threads::Blocked::Select {
            frame: *f,
            sets,
            deadline,
        });
        self.policy.block(i);
        Ok(0)
    }

    pub(super) fn select_result(
        &self,
        i: usize,
        f: &TrapFrame,
        sets: [u32; 3],
        deadline: Option<u64>,
    ) -> Result<Option<i64>, i64> {
        let p = self.process(i);
        let mut ready = [0u32; 3];
        for fd in 0..f.rdi {
            let bit = 1u32 << fd;
            if sets.iter().all(|set| set & bit == 0) {
                continue;
            }
            if !p.files.contains(fd) {
                return Err(-9);
            }
            let (mut read, mut write, hup, error) =
                match self.fs.socket_id(&p.files, fd).map_err(errno)? {
                    Some(id) => self.network.as_ref().ok_or(-100)?.readiness(id)?,
                    None => self.fs.readiness(&p.files, fd).map_err(errno)?,
                };
            let device = self
                .fs
                .device(&p.files, fd, false)
                .or_else(|_| self.fs.device(&p.files, fd, true))
                .ok()
                .flatten();
            if let Some(device) = device.filter(|d| self.terminal.is_terminal(*d)) {
                read = self.terminal.endpoint_ready(device, false, &self.fs);
                write = self.terminal.endpoint_ready(device, true, &self.fs);
            }
            if read || hup || error {
                ready[0] |= sets[0] & bit;
            }
            if write || hup || error {
                ready[1] |= sets[1] & bit;
            }
            // No accepted descriptor has an urgent/OOB channel. Ordinary errors use read/write.
        }
        let count = ready.iter().map(|set| set.count_ones() as i64).sum();
        if count == 0 && deadline.is_none_or(|d| apic::ticks() < d) {
            return Ok(None);
        }
        let space = &p.image.as_ref().unwrap().space;
        space.with_active(|| -> Result<(), i64> {
            // Validate every output before publishing any set; mappings are pinned on this BSP.
            if f.rdi != 0 {
                for address in [f.rsi, f.rdx, f.r10].into_iter().filter(|a| *a != 0) {
                    if !space.accessible(UserAddr(address), 8, true, false) {
                        return Err(-14);
                    }
                }
                for (set, address) in ready.iter().zip([f.rsi, f.rdx, f.r10]) {
                    if address != 0 {
                        uaccess::to_user(space, UserAddr(address), &(*set as u64).to_le_bytes())
                            .map_err(errno)?;
                    }
                }
            }
            Ok(())
        })?;
        remaining(space, f.r8, deadline)?;
        Ok(Some(count))
    }
}

pub(super) fn remaining(
    space: &mm::user::UserSpace,
    address: u64,
    deadline: Option<u64>,
) -> Result<(), i64> {
    if address == 0 {
        return Ok(());
    }
    let left = deadline
        .unwrap_or(apic::ticks())
        .saturating_sub(apic::ticks());
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(&(left / 100).to_le_bytes());
    bytes[8..].copy_from_slice(&((left % 100) * 10_000).to_le_bytes());
    space
        .with_active(|| uaccess::to_user(space, UserAddr(address), &bytes))
        .map_err(errno)
}
