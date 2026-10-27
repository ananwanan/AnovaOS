// SPDX-License-Identifier: Apache-2.0
//! Maintain linear ext4 directory records, parent links and deferred deletion identities.
//! Rename replacement and directory moves share one preflight overlay, including dot-dot and cycle checks.
use super::write::MAX_DIRECTORY_BLOCKS;
use super::*;

#[derive(Clone, Copy)]
struct Slot {
    block: u32,
    at: usize,
    inode: u32,
}

fn name_valid(name: &str) -> Result<(), i64> {
    if name.is_empty()
        || name.len() > 255
        || matches!(name, "." | "..")
        || name.contains(['/', '\0'])
    {
        return Err(INVALID_ARGUMENT);
    }
    Ok(())
}
fn entry(bytes: &mut [u8], at: usize, length: usize, n: u32, name: &str, directory: bool) {
    bytes[at..at + length].fill(0);
    put32(bytes, at, n);
    put16(bytes, at + 4, length as u16);
    bytes[at + 6] = name.len() as u8;
    bytes[at + 7] = if directory { 2 } else { 1 };
    bytes[at + 8..at + 8 + name.len()].copy_from_slice(name.as_bytes());
}
impl Ext4 {
    fn find_slot(&mut self, parent: u32, name: &str) -> Result<Option<Slot>, i64> {
        let directory = self.snapshot(parent)?;
        if !directory.info.directory {
            return Err(NOT_DIRECTORY);
        }
        let mut found = None;
        let mut bytes = buffer()?;
        for logical in 0..directory.info.size / BLOCK {
            let e = Self::mapping(&directory, logical as u32)
                .filter(|e| !e.unwritten)
                .ok_or(INVALID_FILESYSTEM)?;
            let block = e.physical + logical as u32 - e.logical;
            self.read_block(block, &mut bytes)?;
            let mut at = 0;
            while at < BLOCK {
                let (n, length, count) = Self::record(&bytes, at)?;
                if n != 0 && &bytes[at + 8..at + 8 + count] == name.as_bytes() {
                    if found.is_some() {
                        return Err(INVALID_FILESYSTEM);
                    }
                    found = Some(Slot {
                        block,
                        at,
                        inode: n,
                    });
                }
                at += length;
            }
        }
        Ok(found)
    }
    fn insert_entry(
        &mut self,
        parent: u32,
        name: &str,
        n: u32,
        directory: bool,
    ) -> Result<(), i64> {
        let mut inode = self.snapshot(parent)?;
        if !inode.info.directory {
            return Err(NOT_DIRECTORY);
        }
        if self.is_orphan(parent) {
            return Err(NOT_FOUND);
        }
        let needed = (8 + name.len()).next_multiple_of(4);
        let mut bytes = buffer()?;
        for logical in 0..inode.info.size / BLOCK {
            let e = Self::mapping(&inode, logical as u32)
                .filter(|e| !e.unwritten)
                .ok_or(INVALID_FILESYSTEM)?;
            let block = e.physical + logical as u32 - e.logical;
            self.read_block(block, &mut bytes)?;
            let mut at = 0;
            while at < BLOCK {
                let (child, length, count) = Self::record(&bytes, at)?;
                let used = if child == 0 {
                    0
                } else {
                    (8 + count).next_multiple_of(4)
                };
                if length - used >= needed {
                    if used != 0 {
                        put16(&mut bytes[..], at + 4, used as u16);
                    }
                    entry(&mut bytes[..], at + used, length - used, n, name, directory);
                    self.stage(block, bytes, false)?;
                    self.touch(&mut inode, true);
                    return self.store(&inode);
                }
                at += length;
            }
        }
        let logical = inode.info.size / BLOCK;
        if logical >= MAX_DIRECTORY_BLOCKS {
            return Err(NO_SPACE);
        }
        let block = self.allocate_block()?;
        Self::initialized_mapping(&mut inode, logical as u32, block)?;
        inode.info.size += BLOCK;
        self.rebuild(&mut inode)?;
        bytes.fill(0);
        entry(&mut bytes[..], 0, BLOCK, n, name, directory);
        self.stage(block, bytes, false)?;
        self.touch(&mut inode, true);
        self.store(&inode)
    }
    fn remove_entry(&mut self, parent: u32, name: &str) -> Result<u32, i64> {
        let slot = self.find_slot(parent, name)?.ok_or(NOT_FOUND)?;
        let mut bytes = buffer()?;
        self.read_block(slot.block, &mut bytes)?;
        put32(&mut bytes[..], slot.at, 0);
        self.stage(slot.block, bytes, false)?;
        let mut inode = self.snapshot(parent)?;
        self.touch(&mut inode, true);
        self.store(&inode)?;
        Ok(slot.inode)
    }
    fn parent_links(&mut self, n: u32, delta: i32) -> Result<(), i64> {
        let mut inode = self.snapshot(n)?;
        let count = i32::from(u16_at(&inode.raw, 26)) + delta;
        if !(2..=65000).contains(&count) {
            return Err(NO_SPACE);
        }
        put16(&mut inode.raw, 26, count as u16);
        self.touch(&mut inode, true);
        self.store(&inode)
    }
    pub(super) fn create_inner(
        &mut self,
        parent: u32,
        name: &str,
        directory: bool,
    ) -> Result<u32, i64> {
        name_valid(name)?;
        if self.find_slot(parent, name)?.is_some() {
            return Err(ALREADY_EXISTS);
        }
        let mut inode = self.allocate_inode(directory)?;
        let n = inode.info.number;
        if directory {
            let block = self.allocate_block()?;
            Self::initialized_mapping(&mut inode, 0, block)?;
            inode.info.size = BLOCK;
            self.rebuild(&mut inode)?;
            let mut bytes = buffer()?;
            entry(&mut bytes[..], 0, 12, n, ".", true);
            entry(&mut bytes[..], 12, BLOCK - 12, parent, "..", true);
            self.stage(block, bytes, false)?;
            self.parent_links(parent, 1)?;
        }
        self.store(&inode)?;
        self.insert_entry(parent, name, n, directory)?;
        Ok(n)
    }
    pub(super) fn unlink_inner(
        &mut self,
        parent: u32,
        name: &str,
        directory: bool,
    ) -> Result<u32, i64> {
        name_valid(name)?;
        let n = self.find_slot(parent, name)?.ok_or(NOT_FOUND)?.inode;
        let info = self.load(n)?;
        if info.directory != directory {
            return Err(if directory {
                NOT_DIRECTORY
            } else {
                IS_DIRECTORY
            });
        }
        if directory {
            if self.next_entry(n, 0)?.is_some() {
                return Err(NOT_EMPTY);
            }
            self.parent_links(parent, -1)?;
        }
        self.remove_entry(parent, name)?;
        self.orphan(n)?;
        Ok(n)
    }
    pub(super) fn rename_inner(
        &mut self,
        parent: u32,
        name: &str,
        destination: u32,
        new: &str,
    ) -> Result<(), i64> {
        name_valid(name)?;
        name_valid(new)?;
        let n = self.find_slot(parent, name)?.ok_or(NOT_FOUND)?.inode;
        let source = self.load(n)?;
        let target = self.find_slot(destination, new)?;
        if target.is_some_and(|s| s.inode == n) {
            return Ok(());
        }
        if source.directory {
            let mut ancestor = destination;
            let mut reached_root = false;
            for _ in 0..self.inodes {
                if ancestor == n {
                    return Err(INVALID_ARGUMENT);
                }
                if ancestor == 2 {
                    reached_root = true;
                    break;
                }
                ancestor = self
                    .find_slot(ancestor, "..")?
                    .ok_or(INVALID_FILESYSTEM)?
                    .inode;
            }
            if !reached_root {
                return Err(INVALID_FILESYSTEM);
            }
        }
        if let Some(target) = target {
            let old = self.load(target.inode)?;
            if old.directory != source.directory {
                return Err(if old.directory {
                    IS_DIRECTORY
                } else {
                    NOT_DIRECTORY
                });
            }
            if old.directory && self.next_entry(target.inode, 0)?.is_some() {
                return Err(NOT_EMPTY);
            }
            self.unlink_inner(destination, new, old.directory)?;
        }
        self.remove_entry(parent, name)?;
        self.insert_entry(destination, new, n, source.directory)?;
        if source.directory && parent != destination {
            let dotdot = self.find_slot(n, "..")?.ok_or(INVALID_FILESYSTEM)?;
            let mut bytes = buffer()?;
            self.read_block(dotdot.block, &mut bytes)?;
            put32(&mut bytes[..], dotdot.at, destination);
            self.stage(dotdot.block, bytes, false)?;
            self.parent_links(parent, -1)?;
            self.parent_links(destination, 1)?;
        }
        let mut inode = self.snapshot(n)?;
        self.touch(&mut inode, false);
        self.store(&inode)
    }
}
