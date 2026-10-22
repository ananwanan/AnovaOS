// SPDX-License-Identifier: Apache-2.0
//! Map bounded anonymous/file-backed Linux VMAs and enforce mprotect, guard pages and W^X.
//! All page-table edits happen on the inactive BSP scheduler root; no mutable borrow spans usercopy.
use super::*;
use anova_memory::paging::Permissions;
const START: u64 = 0x5000_0000;
const END: u64 = 0xc000_0000;
fn permissions(prot: u64) -> Result<Option<Permissions>, i64> {
    if prot & !7 != 0 || prot & 6 == 6 {
        return Err(-13);
    }
    if prot == 0 {
        return Ok(None);
    }
    Ok(Some(Permissions {
        writable: prot & 2 != 0,
        executable: prot & 4 != 0,
        ..Permissions::READ_ONLY
    }))
}
impl Kernel {
    pub(in crate::task) fn linux_memory(&mut self, i: usize, f: &TrapFrame) -> Result<i64, i64> {
        if f.rax == 12 {
            let image = self.process_mut(i).image.as_mut().unwrap();
            let old = image.brk;
            if !(0x4000_0000..=0x4800_0000).contains(&f.rdi) {
                return Ok(old as i64);
            }
            let start = (old + 4095) & !4095;
            let end = (f.rdi + 4095) & !4095;
            if end > start && image.space.map_anonymous(start, end).is_err() {
                return Ok(old as i64);
            }
            for page in (end..start).step_by(4096) {
                image.space.unmap(page).map_err(|_| -12)?;
            }
            let group = self.process(i).thread.tgid;
            for p in self
                .processes
                .iter_mut()
                .flatten()
                .filter(|p| p.thread.tgid == group)
            {
                if let Some(image) = p.image.as_mut() {
                    image.brk = f.rdi;
                }
            }
            return Ok(f.rdi as i64);
        }
        let size = f
            .rsi
            .checked_add(4095)
            .map(|n| n & !4095)
            .filter(|n| *n > 0 && *n <= 256 * 1024 * 1024)
            .ok_or(-22)?;
        let end = f.rdi.checked_add(size).ok_or(-22)?;
        if f.rax == 10 {
            let perms = permissions(f.rdx)?;
            if f.rdi & 4095 != 0 {
                return Err(-22);
            }
            if perms.is_some_and(|p| p.writable)
                && self
                    .process(i)
                    .image
                    .as_ref()
                    .unwrap()
                    .space
                    .has_file_pages(f.rdi, end)
            {
                return Err(-13);
            }
            self.process_mut(i)
                .image
                .as_mut()
                .unwrap()
                .space
                .protect(f.rdi, end, perms)
                .map_err(|_| -12)?;
            return Ok(0);
        }
        if f.rax == 11 {
            if f.rdi & 4095 != 0 || f.rdi < START || end > END {
                return Err(-22);
            }
            for page in (f.rdi..end).step_by(4096) {
                self.process_mut(i)
                    .image
                    .as_mut()
                    .unwrap()
                    .space
                    .unmap(page)
                    .map_err(|_| -22)?;
            }
            return Ok(0);
        }
        let perms = permissions(f.rdx)?;
        let anonymous = f.r10 & 0x20 != 0;
        let fixed = f.r10 & 0x10 != 0;
        let shared = f.r10 & 3 == 1;
        // Linux MAP_STACK is a hint; the same eager mapping and guard protections apply.
        if f.r10 & !0x21033 != 0
            || !matches!(f.r10 & 3, 1 | 2)
            || (shared && fixed)
            || f.r9 & 4095 != 0
        {
            return Err(-22);
        }
        // musl deliberately removes the initial brk guard from CPL3 accessibility.
        if f.rdx == 0
            && f.r10 == 0x32
            && f.rdi >= 0x4000_0000
            && end <= (self.process(i).image.as_ref().unwrap().brk + 4095) & !4095
        {
            self.process_mut(i)
                .image
                .as_mut()
                .unwrap()
                .space
                .protect(f.rdi, end, None)
                .map_err(|_| -22)?;
            return Ok(f.rdi as i64);
        }
        if anonymous && (f.r8 != u64::MAX || f.r9 != 0) {
            return Err(-22);
        }
        if shared && !anonymous && f.rdx & 2 != 0 {
            return Err(-95);
        }
        let files = &self.processes[i].as_ref().unwrap().files;
        let file = if shared && !anonymous {
            Some(
                self.fs
                    .shared_file_mapping(files, f.r8, f.r9, size)
                    .map_err(errno)?,
            )
        } else {
            None
        };
        let data = if anonymous || file.is_some() {
            None
        } else {
            Some(
                self.fs
                    .mapping_bytes(files, f.r8, f.r9, size)
                    .map_err(errno)?,
            )
        };
        let image = self.process_mut(i).image.as_mut().unwrap();
        if file.is_some() {
            image
                .space
                .reserve_file_pages((size / 4096) as usize)
                .map_err(|_| -12)?;
        }
        let start = if fixed {
            if f.rdi & 4095 != 0 || f.rdi < START || end > END {
                return Err(-22);
            }
            f.rdi
        } else {
            image.space.find_gap(START, END, size).ok_or(-12)?
        };
        if fixed
            && (start..start + size)
                .step_by(4096)
                .any(|a| image.space.shared_backing(a))
        {
            return Err(-95);
        }
        // Existing MAP_FIXED backing is retained until all missing pages are allocated.
        let mut created = alloc::vec::Vec::new();
        created
            .try_reserve_exact((size / 4096) as usize)
            .map_err(|_| -12)?;
        for page in (start..start + size).step_by(4096) {
            if !image.space.mapped(page) {
                if image.space.map(page, Permissions::DATA).is_err() {
                    for old in created {
                        image.space.unmap(old).map_err(|_| -12)?;
                    }
                    return Err(-12);
                }
                created.push(page);
            }
        }
        let zero = [0; 4096];
        for page in (start..start + size).step_by(4096) {
            image.space.initialize(page, &zero).map_err(|_| -14)?;
        }
        if let Some(data) = data {
            image.space.initialize(start, &data).map_err(|_| -14)?;
        }
        if shared {
            image.space.share_mapping(start, start + size);
        }
        if let Some(file) = file {
            image
                .space
                .bind_file_pages(start, size, f.r9 as usize, file);
            image.space.refresh_file_pages();
        }
        image
            .space
            .protect(start, start + size, perms)
            .map_err(|_| -13)?;
        if !fixed {
            image.space.commit_mapping_hint(start + size);
        }
        Ok(start as i64)
    }
}
