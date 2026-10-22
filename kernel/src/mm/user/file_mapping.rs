// SPDX-License-Identifier: Apache-2.0
//! Refresh bounded readonly shared-file pages from versioned VFS bytes before user execution.
//! Metadata pins file identity; normal page ownership still controls physical allocation and fork.
use super::*;
use anova_vfs::file_mapping::MappedFile;

#[derive(Clone)]
pub(super) struct FilePage {
    pub address: VirtAddr,
    physical: PhysAddr,
    file: MappedFile,
    offset: usize,
    version: u64,
}

impl UserSpace {
    pub fn reserve_file_pages(&mut self, pages: usize) -> Result<(), Error> {
        self.0
            .borrow_mut()
            .file_pages
            .try_reserve(pages)
            .map_err(|_| Error::OutOfMemory)
    }
    /// Call only after all pages and metadata capacity have been allocated successfully.
    pub fn bind_file_pages(&mut self, start: u64, size: u64, offset: usize, file: MappedFile) {
        let mut space = self.0.borrow_mut();
        for page in (start..start + size).step_by(4096) {
            let physical = space
                .pages
                .iter()
                .find(|(v, _)| v.0 == page)
                .expect("File mapping page is not owned")
                .1
                .address;
            space.file_pages.push(FilePage {
                address: VirtAddr(page),
                physical,
                file: file.clone(),
                offset: offset + (page - start) as usize,
                version: u64::MAX,
            });
        }
    }
    pub fn has_file_pages(&self, start: u64, end: u64) -> bool {
        self.0
            .borrow()
            .file_pages
            .iter()
            .any(|p| (start..end).contains(&p.address.0))
    }
    pub fn refresh_file_pages(&self) {
        let mut space = self.0.borrow_mut();
        if space.file_pages.is_empty() {
            return;
        }
        MEMORY.with(|slot| {
            let manager = slot.as_ref().expect("Memory not initialized");
            for page in &mut space.file_pages {
                if page.version == page.file.version() {
                    continue;
                }
                // SAFETY: This page still belongs to the inactive user root. The BSP is on its
                // permanent scheduler root at IF=0; no user alias can be read or written now.
                // File metadata owns no physical page and is removed whenever its leaf is unmapped.
                let bytes = unsafe {
                    core::slice::from_raw_parts_mut(
                        direct_pointer(manager.direct, page.physical),
                        PAGE_SIZE as usize,
                    )
                };
                page.version = page.file.copy_into(page.offset, bytes);
            }
        });
    }
}
