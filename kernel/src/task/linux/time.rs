// SPDX-License-Identifier: Apache-2.0
//! Translate Linux utimensat/futimesat into actual VFS/ext2 timestamps with checked user layouts.
//! Preserve omitted fields, resolve dirfds normally and reject unsupported timestamp/flag ranges.
use super::*;

impl Kernel {
    pub(super) fn linux_timestamps(&mut self, i: usize, f: &TrapFrame) -> Result<i64, i64> {
        if f.rax == 280 && f.r10 & !0x100 != 0 {
            return Err(-22);
        }
        let p = self.processes[i].as_ref().unwrap();
        let space = &p.image.as_ref().unwrap().space;
        let (inode, times) = space.with_active(|| -> Result<(u64, [Option<u64>; 2]), i64> {
            let inode = if f.rsi == 0 {
                if f.rax == 280 && f.r10 != 0 {
                    return Err(-22);
                }
                self.fs
                    .descriptor_metadata(&p.files, f.rdi)
                    .map_err(errno)?
                    .0
            } else {
                let mut bytes = [0; anova_vfs::PATH_BYTES + 1];
                let length = string(space, f.rsi, &mut bytes)?;
                let path = core::str::from_utf8(&bytes[..length]).map_err(|_| -22)?;
                let cwd = at_directory(&self.fs, &p.files, p.cwd, f.rdi, path)?;
                if f.rax == 280 && f.r10 & 0x100 != 0 {
                    self.fs.metadata_nofollow(cwd, path).map_err(errno)?.0
                } else {
                    self.fs.metadata(cwd, path).map_err(errno)?.0
                }
            };
            let now = || -> Result<u64, i64> {
                let (seconds, nanos) = crate::arch::x86_64::clock::realtime().ok_or(-5)?;
                seconds
                    .checked_mul(1_000_000_000)
                    .and_then(|s| s.checked_add(nanos))
                    .ok_or(-75)
            };
            if f.rdx == 0 {
                return Ok((inode, [Some(now()?), Some(now()?)]));
            }
            let mut input = [0; 32];
            uaccess::from_user(space, UserAddr(f.rdx), &mut input).map_err(errno)?;
            let mut times = [None; 2];
            for (time, bytes) in times.iter_mut().zip(input.chunks_exact(16)) {
                let seconds = i64::from_le_bytes(bytes[..8].try_into().unwrap());
                let fraction = i64::from_le_bytes(bytes[8..].try_into().unwrap());
                if f.rax == 280 && fraction == 1_073_741_822 {
                    continue;
                }
                if f.rax == 280 && fraction == 1_073_741_823 {
                    *time = Some(now()?);
                    continue;
                }
                let units = if f.rax == 261 {
                    1_000_000
                } else {
                    1_000_000_000
                };
                if !(0..units).contains(&fraction) {
                    return Err(-22);
                }
                if seconds < 0 {
                    return Err(-75);
                }
                *time = Some(
                    (seconds as u64)
                        .checked_mul(1_000_000_000)
                        .and_then(|s| {
                            s.checked_add(fraction as u64 * (1_000_000_000 / units as u64))
                        })
                        .ok_or(-75)?,
                );
            }
            Ok((inode, times))
        })?;
        self.fs
            .set_times(inode, times[0], times[1])
            .map_err(errno)?;
        Ok(0)
    }
}
