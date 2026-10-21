// SPDX-License-Identifier: Apache-2.0
//! Read and update one-group, 4 KiB/128-byte-inode ext2 with direct/single-indirect regular files.
//! Reject unsupported features and dirty images. Ordered writes fail closed; this is not a journal.
use crate::{BlockDevice, SECTOR, check_range};
use alloc::{boxed::Box, vec::Vec};
use anova_abi::*;

pub const BLOCK: usize = 4096;
pub const MAX_FILE: usize = 1024 * 1024;
const REGULAR: u16 = 0x8000;
const DIRECTORY: u16 = 0x4000;
fn u16_at(b: &[u8], n: usize) -> u16 {
    u16::from_le_bytes(b[n..n + 2].try_into().unwrap())
}
fn u32_at(b: &[u8], n: usize) -> u32 {
    u32::from_le_bytes(b[n..n + 4].try_into().unwrap())
}
fn put16(b: &mut [u8], n: usize, v: u16) {
    b[n..n + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], n: usize, v: u32) {
    b[n..n + 4].copy_from_slice(&v.to_le_bytes());
}

#[derive(Clone, Copy, Debug)]
pub struct Inode {
    pub number: u32,
    pub directory: bool,
    pub size: usize,
    pub executable: bool,
    raw: [u8; 128],
}
pub struct Entry {
    pub inode: u32,
    pub name: [u8; 64],
    pub length: usize,
}
pub struct Ext2 {
    device: Box<dyn BlockDevice>,
    superblock: [u8; 1024],
    group: [u8; 32],
    blocks: u32,
    inodes: u32,
    block_bitmap: u32,
    inode_bitmap: u32,
    inode_table: u32,
    data_start: u32,
    dirty: bool,
    failed: bool,
    writes: u64,
}
impl Ext2 {
    pub fn stop_device(&mut self) -> Result<(), i64> {
        self.device.stop()
    }
    fn mutate<T>(&mut self, operation: impl FnOnce(&mut Self) -> Result<T, i64>) -> Result<T, i64> {
        let before = self.writes;
        let result = operation(self);
        // Allocation/read failures after any published write require recovery, never a clean unmount.
        if result.is_err() && self.writes != before {
            self.failed = true;
        }
        result
    }
    pub fn shutdown(&mut self) -> Result<(), i64> {
        self.mutate(|s| s.shutdown_inner())
    }
    pub fn write(&mut self, number: u32, offset: usize, input: &[u8]) -> Result<usize, i64> {
        self.mutate(|s| s.write_inner(number, offset, input))
    }
    pub fn truncate(&mut self, number: u32) -> Result<(), i64> {
        self.mutate(|s| s.truncate_inner(number))
    }
    pub fn set_executable(&mut self, number: u32) -> Result<(), i64> {
        self.mutate(|s| s.set_executable_inner(number))
    }
    pub fn create(&mut self, parent: u32, name: &str, directory: bool) -> Result<u32, i64> {
        self.mutate(|s| s.create_inner(parent, name, directory))
    }
    pub fn unlink(&mut self, parent: u32, name: &str) -> Result<u32, i64> {
        self.mutate(|s| s.unlink_inner(parent, name))
    }
    pub fn release_unlinked(&mut self, number: u32) -> Result<(), i64> {
        self.mutate(|s| s.release_unlinked_inner(number))
    }

    pub fn mount(mut device: Box<dyn BlockDevice>) -> Result<Self, i64> {
        let mut first = buffer()?;
        read_block(&mut *device, 0, &mut first)?;
        let s = &first[1024..2048];
        // Only the exact image profile is writable; even unknown COMPAT bits are rejected.
        if u16_at(s, 56) != 0xef53
            || u32_at(s, 24) != 2
            || u32_at(s, 28) != 2
            || u32_at(s, 20) != 0
            || u32_at(s, 76) != 1
            || u16_at(s, 88) != 128
            || u32_at(s, 84) != 11
            || u32_at(s, 72) != 0
            || u32_at(s, 92) != 0
            || u32_at(s, 96) != 2
            || u32_at(s, 100) != 0
            || u16_at(s, 58) != 1
            || u32_at(s, 232) != 0
        {
            return Err(INVALID_FILESYSTEM);
        }
        let blocks = u32_at(s, 4);
        let inodes = u32_at(s, 0);
        if !(64..=32768).contains(&blocks)
            || !(16..=4096).contains(&inodes)
            || u32_at(s, 32) < blocks
            || u32_at(s, 32) > 32768
            || u32_at(s, 36) != u32_at(s, 32)
            || u32_at(s, 40) != inodes
            || u32_at(s, 12) > blocks
            || u32_at(s, 16) > inodes
            || u64::from(blocks) * 8 > device.sectors()
        {
            return Err(INVALID_FILESYSTEM);
        }
        let mut superblock = [0; 1024];
        superblock.copy_from_slice(s);
        let mut descriptor = buffer()?;
        read_block(&mut *device, 1, &mut descriptor)?;
        let mut group = [0; 32];
        group.copy_from_slice(&descriptor[..32]);
        let block_bitmap = u32_at(&group, 0);
        let inode_bitmap = u32_at(&group, 4);
        let inode_table = u32_at(&group, 8);
        let data_start = inode_table
            .checked_add((inodes as usize * 128).div_ceil(BLOCK) as u32)
            .ok_or(INVALID_FILESYSTEM)?;
        // Contiguous metadata makes allocation/reservation checks auditable and bounded.
        if block_bitmap != 2
            || inode_bitmap != 3
            || inode_table != 4
            || data_start >= blocks
            || u16_at(&group, 12) as u32 != u32_at(&superblock, 12)
            || u16_at(&group, 14) as u32 != u32_at(&superblock, 16)
        {
            return Err(INVALID_FILESYSTEM);
        }
        let mut fs = Self {
            device,
            superblock,
            group,
            blocks,
            inodes,
            block_bitmap,
            inode_bitmap,
            inode_table,
            data_start,
            dirty: false,
            failed: false,
            writes: 0,
        };
        fs.validate_allocation()?;
        Ok(fs)
    }
    fn read_block(&mut self, block: u32, bytes: &mut [u8; BLOCK]) -> Result<(), i64> {
        if self.failed {
            return Err(IO_ERROR);
        }
        if block >= self.blocks {
            return Err(INVALID_FILESYSTEM);
        }
        read_block(&mut *self.device, block, bytes)
    }
    fn write_block(&mut self, block: u32, bytes: &[u8; BLOCK]) -> Result<(), i64> {
        if self.failed {
            return Err(IO_ERROR);
        }
        if block >= self.blocks {
            return Err(INVALID_FILESYSTEM);
        }
        self.writes = self.writes.wrapping_add(1);
        let result = write_block(&mut *self.device, block, bytes);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    pub fn sync(&mut self) -> Result<(), i64> {
        if self.failed {
            return Err(IO_ERROR);
        }
        let result = self.device.flush();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn store_super(&mut self) -> Result<(), i64> {
        let mut block = buffer()?;
        self.read_block(0, &mut block)?;
        block[1024..2048].copy_from_slice(&self.superblock);
        self.write_block(0, &block)
    }
    fn begin(&mut self) -> Result<(), i64> {
        if self.failed {
            return Err(IO_ERROR);
        }
        if self.device.read_only() {
            return Err(READ_ONLY);
        }
        if !self.dirty {
            put16(&mut self.superblock, 58, 0);
            self.store_super()?;
            self.sync()?;
            self.dirty = true;
        }
        Ok(())
    }
    /// Only call after all open orphaned inodes have been released and writers stopped.
    fn shutdown_inner(&mut self) -> Result<(), i64> {
        self.sync()?;
        if self.dirty {
            put16(&mut self.superblock, 58, 1);
            self.store_super()?;
            self.sync()?;
            self.dirty = false;
        }
        Ok(())
    }
    fn counters(&mut self, blocks: i32, inodes: i32, directories: i32) -> Result<(), i64> {
        let b = i64::from(u32_at(&self.superblock, 12)) + i64::from(blocks);
        let i = i64::from(u32_at(&self.superblock, 16)) + i64::from(inodes);
        let d = i64::from(u16_at(&self.group, 16)) + i64::from(directories);
        if !(0..=i64::from(self.blocks)).contains(&b)
            || !(0..=i64::from(self.inodes)).contains(&i)
            || !(0..=65535).contains(&d)
        {
            return Err(INVALID_FILESYSTEM);
        }
        put32(&mut self.superblock, 12, b as u32);
        put32(&mut self.superblock, 16, i as u32);
        put16(&mut self.group, 12, b as u16);
        put16(&mut self.group, 14, i as u16);
        put16(&mut self.group, 16, d as u16);
        let mut block = buffer()?;
        self.read_block(1, &mut block)?;
        block[..32].copy_from_slice(&self.group);
        self.write_block(1, &block)?;
        self.store_super()
    }
    fn allocated(&mut self, inode: u32) -> Result<bool, i64> {
        if inode == 0 || inode > self.inodes {
            return Err(INVALID_FILESYSTEM);
        }
        let mut bits = buffer()?;
        self.read_block(self.inode_bitmap, &mut bits)?;
        Ok(bit(&*bits, inode - 1))
    }
    pub fn inode(&mut self, number: u32) -> Result<Inode, i64> {
        if !self.allocated(number)? {
            return Err(INVALID_FILESYSTEM);
        }
        let offset = (number as usize - 1) * 128;
        let mut block = buffer()?;
        self.read_block(self.inode_table + (offset / BLOCK) as u32, &mut block)?;
        let mut raw = [0; 128];
        raw.copy_from_slice(&block[offset % BLOCK..offset % BLOCK + 128]);
        let mode = u16_at(&raw, 0);
        let directory = mode & 0xf000 == DIRECTORY;
        let size = u32_at(&raw, 4) as usize;
        if (!directory && mode & 0xf000 != REGULAR)
            || u32_at(&raw, 32) != 0
            || u32_at(&raw, 104) != 0
            || u32_at(&raw, 108) != 0
            || u32_at(&raw, 112) != 0
            || size > if directory { 12 * BLOCK } else { MAX_FILE }
            || directory && (size == 0 || !size.is_multiple_of(BLOCK))
            || u32_at(&raw, 92) != 0
            || u32_at(&raw, 96) != 0
        {
            return Err(INVALID_FILESYSTEM);
        }
        Ok(Inode {
            number,
            directory,
            size,
            executable: mode & 0o111 != 0,
            raw,
        })
    }
    fn store_inode(&mut self, inode: &Inode) -> Result<(), i64> {
        let offset = (inode.number as usize - 1) * 128;
        let address = self.inode_table + (offset / BLOCK) as u32;
        let mut block = buffer()?;
        self.read_block(address, &mut block)?;
        block[offset % BLOCK..offset % BLOCK + 128].copy_from_slice(&inode.raw);
        self.write_block(address, &block)
    }
    fn data_pointer(&mut self, inode: &Inode, index: usize) -> Result<u32, i64> {
        let pointer = if index < 12 {
            u32_at(&inode.raw, 40 + index * 4)
        } else {
            if index >= 12 + BLOCK / 4 {
                return Err(NO_SPACE);
            }
            let indirect = u32_at(&inode.raw, 88);
            if indirect == 0 {
                return Ok(0);
            }
            self.check_data(indirect)?;
            let mut table = buffer()?;
            self.read_block(indirect, &mut table)?;
            u32_at(&*table, (index - 12) * 4)
        };
        if pointer != 0 {
            self.check_data(pointer)?;
        }
        Ok(pointer)
    }
    fn check_data(&self, block: u32) -> Result<(), i64> {
        if block < self.data_start || block >= self.blocks {
            Err(INVALID_FILESYSTEM)
        } else {
            Ok(())
        }
    }
    pub fn read(&mut self, number: u32, offset: usize, output: &mut [u8]) -> Result<usize, i64> {
        let inode = self.inode(number)?;
        if inode.directory {
            return Err(IS_DIRECTORY);
        }
        let n = output.len().min(inode.size.saturating_sub(offset));
        let mut done = 0;
        while done < n {
            let position = offset + done;
            let take = (BLOCK - position % BLOCK).min(n - done);
            let block = self.data_pointer(&inode, position / BLOCK)?;
            if block == 0 {
                output[done..done + take].fill(0);
            } else {
                let mut bytes = buffer()?;
                self.read_block(block, &mut bytes)?;
                output[done..done + take]
                    .copy_from_slice(&bytes[position % BLOCK..position % BLOCK + take]);
            }
            done += take;
        }
        Ok(n)
    }
    pub fn entries(&mut self, number: u32) -> Result<Vec<Entry>, i64> {
        let inode = self.inode(number)?;
        if !inode.directory {
            return Err(NOT_DIRECTORY);
        }
        let mut entries = Vec::new();
        for index in 0..inode.size / BLOCK {
            let block = self.data_pointer(&inode, index)?;
            if block == 0 {
                return Err(INVALID_FILESYSTEM);
            }
            let mut bytes = buffer()?;
            self.read_block(block, &mut bytes)?;
            let mut offset = 0;
            while offset < BLOCK {
                let (child, length, name_len) = directory_record(&bytes, offset)?;
                if child != 0 {
                    let name = &bytes[offset + 8..offset + 8 + name_len];
                    if child > self.inodes
                        || name.contains(&0)
                        || name.contains(&b'/')
                        || name_len == 0
                        || core::str::from_utf8(name).is_err()
                    {
                        return Err(INVALID_FILESYSTEM);
                    }
                    if name != b"." && name != b".." {
                        if name_len > 63 {
                            return Err(TOO_LONG);
                        }
                        if entries.iter().any(|e: &Entry| &e.name[..e.length] == name) {
                            return Err(INVALID_FILESYSTEM);
                        }
                        entries.try_reserve(1).map_err(|_| NO_MEMORY)?;
                        let mut label = [0; 64];
                        label[..name_len].copy_from_slice(name);
                        entries.push(Entry {
                            inode: child,
                            name: label,
                            length: name_len,
                        });
                    }
                }
                offset += length;
            }
        }
        Ok(entries)
    }
    fn allocate_block(&mut self) -> Result<u32, i64> {
        let mut bits = buffer()?;
        self.read_block(self.block_bitmap, &mut bits)?;
        let number = (self.data_start..self.blocks)
            .find(|n| !bit(&*bits, *n))
            .ok_or(NO_SPACE)?;
        self.begin()?;
        // Zero before publishing a pointer, including the final block of sparse writes.
        self.write_block(number, &*buffer()?)?;
        set_bit(&mut *bits, number, true);
        self.write_block(self.block_bitmap, &bits)?;
        self.counters(-1, 0, 0)?;
        Ok(number)
    }
    fn free_block(&mut self, number: u32) -> Result<(), i64> {
        self.check_data(number)?;
        let mut bits = buffer()?;
        self.read_block(self.block_bitmap, &mut bits)?;
        if !bit(&*bits, number) {
            return Err(INVALID_FILESYSTEM);
        }
        set_bit(&mut *bits, number, false);
        self.write_block(self.block_bitmap, &bits)?;
        self.counters(1, 0, 0)
    }
    fn write_inner(&mut self, number: u32, offset: usize, input: &[u8]) -> Result<usize, i64> {
        let mut inode = self.inode(number)?;
        if inode.directory {
            return Err(IS_DIRECTORY);
        }
        let end = offset
            .checked_add(input.len())
            .filter(|n| *n <= MAX_FILE)
            .ok_or(NO_SPACE)?;
        if input.is_empty() {
            return Ok(0);
        }
        let mut missing = 0;
        for index in 0..end.div_ceil(BLOCK) {
            if self.data_pointer(&inode, index)? == 0 {
                missing += 1;
            }
        }
        if end > 12 * BLOCK && u32_at(&inode.raw, 88) == 0 {
            missing += 1;
        }
        if missing > u32_at(&self.superblock, 12) {
            return Err(NO_SPACE);
        }
        self.begin()?;
        // Allocate/zero all holes before publishing length. A failed transfer poisons further I/O.
        for index in 0..end.div_ceil(BLOCK) {
            if self.data_pointer(&inode, index)? != 0 {
                continue;
            }
            if index >= 12 && u32_at(&inode.raw, 88) == 0 {
                let pointer = self.allocate_block()?;
                put32(&mut inode.raw, 88, pointer);
                let sectors = u32_at(&inode.raw, 28) + 8;
                put32(&mut inode.raw, 28, sectors);
            }
            let pointer = self.allocate_block()?;
            if index < 12 {
                put32(&mut inode.raw, 40 + index * 4, pointer);
            } else {
                let mut table = buffer()?;
                let address = u32_at(&inode.raw, 88);
                self.read_block(address, &mut table)?;
                put32(&mut *table, (index - 12) * 4, pointer);
                self.write_block(address, &table)?;
            }
            let sectors = u32_at(&inode.raw, 28) + 8;
            put32(&mut inode.raw, 28, sectors);
        }
        let mut done = 0;
        while done < input.len() {
            let pos = offset + done;
            let n = (BLOCK - pos % BLOCK).min(input.len() - done);
            let address = self.data_pointer(&inode, pos / BLOCK)?;
            let mut block = buffer()?;
            self.read_block(address, &mut block)?;
            block[pos % BLOCK..pos % BLOCK + n].copy_from_slice(&input[done..done + n]);
            self.write_block(address, &block)?;
            done += n;
        }
        self.sync()?;
        inode.size = inode.size.max(end);
        put32(&mut inode.raw, 4, inode.size as u32);
        self.store_inode(&inode)?;
        Ok(done)
    }
    fn truncate_inner(&mut self, number: u32) -> Result<(), i64> {
        let mut inode = self.inode(number)?;
        if inode.directory {
            return Err(IS_DIRECTORY);
        }
        self.begin()?;
        let old = inode;
        inode.size = 0;
        put32(&mut inode.raw, 4, 0);
        put32(&mut inode.raw, 28, 0);
        inode.raw[40..100].fill(0);
        self.store_inode(&inode)?;
        self.sync()?;
        for index in 0..old.size.div_ceil(BLOCK) {
            let block = self.data_pointer(&old, index)?;
            if block != 0 {
                self.free_block(block)?;
            }
        }
        let table = u32_at(&old.raw, 88);
        if table != 0 {
            self.free_block(table)?;
        }
        Ok(())
    }
    fn set_executable_inner(&mut self, number: u32) -> Result<(), i64> {
        let mut inode = self.inode(number)?;
        if inode.directory {
            return Err(IS_DIRECTORY);
        }
        self.begin()?;
        let mode = u16_at(&inode.raw, 0) | 0o111;
        put16(&mut inode.raw, 0, mode);
        self.store_inode(&inode)
    }
    fn create_inner(&mut self, parent: u32, name: &str, directory: bool) -> Result<u32, i64> {
        if name.is_empty()
            || name.len() > 63
            || name == "."
            || name == ".."
            || name.contains(['/', '\0'])
        {
            return Err(INVALID_ARGUMENT);
        }
        if self
            .entries(parent)?
            .iter()
            .any(|e| &e.name[..e.length] == name.as_bytes())
        {
            return Err(ALREADY_EXISTS);
        }
        // Find space before allocating an inode; bounded directories do not grow in M7.
        let parent_inode = self.inode(parent)?;
        let needed = (8 + name.len()).next_multiple_of(4);
        let mut slot = None;
        for index in 0..parent_inode.size / BLOCK {
            let address = self.data_pointer(&parent_inode, index)?;
            let mut block = buffer()?;
            self.read_block(address, &mut block)?;
            let mut at = 0;
            while at < BLOCK {
                let (child, len, n) = directory_record(&block, at)?;
                let used = if child == 0 {
                    0
                } else {
                    (8 + n).next_multiple_of(4)
                };
                if len - used >= needed {
                    slot = Some((address, at, used, len));
                    break;
                }
                at += len;
            }
            if slot.is_some() {
                break;
            }
        }
        let (address, at, used, len) = slot.ok_or(NO_SPACE)?;
        if directory && u32_at(&self.superblock, 12) == 0 {
            return Err(NO_SPACE);
        }
        let mut bitmap = buffer()?;
        self.read_block(self.inode_bitmap, &mut bitmap)?;
        let number = (11..=self.inodes)
            .find(|n| !bit(&*bitmap, *n - 1))
            .ok_or(NO_SPACE)?;
        self.begin()?;
        let mut inode = Inode {
            number,
            directory,
            size: 0,
            executable: false,
            raw: [0; 128],
        };
        put16(
            &mut inode.raw,
            0,
            if directory {
                DIRECTORY | 0o755
            } else {
                REGULAR | 0o644
            },
        );
        put16(&mut inode.raw, 26, if directory { 2 } else { 1 });
        if directory {
            let data = self.allocate_block()?;
            let mut block = buffer()?;
            record(&mut block, 0, 12, number, ".", 2);
            record(&mut block, 12, BLOCK - 12, parent, "..", 2);
            self.write_block(data, &block)?;
            inode.size = BLOCK;
            put32(&mut inode.raw, 4, BLOCK as u32);
            put32(&mut inode.raw, 28, 8);
            put32(&mut inode.raw, 40, data);
        }
        self.store_inode(&inode)?;
        set_bit(&mut *bitmap, number - 1, true);
        self.write_block(self.inode_bitmap, &bitmap)?;
        self.counters(0, -1, i32::from(directory))?;
        if directory {
            let mut p = parent_inode;
            let links = u16_at(&p.raw, 26).checked_add(1).ok_or(NO_SPACE)?;
            put16(&mut p.raw, 26, links);
            self.store_inode(&p)?;
        }
        self.sync()?;
        let mut block = buffer()?;
        self.read_block(address, &mut block)?;
        if used != 0 {
            put16(&mut *block, at + 4, used as u16);
        }
        record(
            &mut block,
            at + used,
            len - used,
            number,
            name,
            if directory { 2 } else { 1 },
        );
        self.write_block(address, &block)?;
        Ok(number)
    }
    fn unlink_inner(&mut self, parent: u32, name: &str) -> Result<u32, i64> {
        let p = self.inode(parent)?;
        for index in 0..p.size / BLOCK {
            let address = self.data_pointer(&p, index)?;
            let mut block = buffer()?;
            self.read_block(address, &mut block)?;
            let mut at = 0;
            while at < BLOCK {
                let (child, length, n) = directory_record(&block, at)?;
                if child != 0 && &block[at + 8..at + 8 + n] == name.as_bytes() {
                    let mut inode = self.inode(child)?;
                    if inode.directory {
                        return Err(IS_DIRECTORY);
                    }
                    self.begin()?;
                    put32(&mut *block, at, 0);
                    self.write_block(address, &block)?;
                    self.sync()?;
                    put16(&mut inode.raw, 26, 0);
                    self.store_inode(&inode)?;
                    return Ok(child);
                }
                at += length;
            }
        }
        Err(NOT_FOUND)
    }
    fn release_unlinked_inner(&mut self, number: u32) -> Result<(), i64> {
        let mut inode = self.inode(number)?;
        if u16_at(&inode.raw, 26) != 0 {
            return Err(INVALID_ARGUMENT);
        }
        self.truncate(number)?;
        self.sync()?;
        inode.raw.fill(0);
        self.store_inode(&inode)?;
        let mut bitmap = buffer()?;
        self.read_block(self.inode_bitmap, &mut bitmap)?;
        set_bit(&mut *bitmap, number - 1, false);
        self.write_block(self.inode_bitmap, &bitmap)?;
        self.counters(0, 1, 0)
    }
    fn validate_allocation(&mut self) -> Result<(), i64> {
        let mut blocks = buffer()?;
        let mut inodes = buffer()?;
        self.read_block(self.block_bitmap, &mut blocks)?;
        self.read_block(self.inode_bitmap, &mut inodes)?;
        if (0..self.data_start).any(|i| !bit(&*blocks, i))
            || (0..10).any(|i| !bit(&*inodes, i))
            || (0..self.blocks).filter(|i| !bit(&*blocks, *i)).count()
                != u32_at(&self.superblock, 12) as usize
            || (0..self.inodes).filter(|i| !bit(&*inodes, *i)).count()
                != u32_at(&self.superblock, 16) as usize
        {
            return Err(INVALID_FILESYSTEM);
        }
        let mut seen = buffer()?;
        let mut directories = 0;
        for number in 2..=self.inodes {
            if number != 2 && number < 11 || !bit(&*inodes, number - 1) {
                continue;
            }
            let inode = self.inode(number)?;
            if u16_at(&inode.raw, 26) == 0 || !inode.directory && u16_at(&inode.raw, 26) != 1 {
                return Err(INVALID_FILESYSTEM);
            }
            directories += u32::from(inode.directory);
            let mut used = 0;
            let mut pointers = Vec::new();
            pointers
                .try_reserve_exact(12 + BLOCK / 4)
                .map_err(|_| NO_MEMORY)?;
            pointers.resize(12 + BLOCK / 4, 0);
            for (index, pointer) in pointers[..12].iter_mut().enumerate() {
                *pointer = u32_at(&inode.raw, 40 + index * 4);
            }
            let table = u32_at(&inode.raw, 88);
            if table != 0 {
                self.check_data(table)?;
                if !bit(&*blocks, table) || bit(&*seen, table) {
                    return Err(INVALID_FILESYSTEM);
                }
                set_bit(&mut *seen, table, true);
                used += 8;
                let mut bytes = buffer()?;
                self.read_block(table, &mut bytes)?;
                for (index, pointer) in pointers[12..].iter_mut().enumerate() {
                    *pointer = u32_at(&*bytes, index * 4);
                }
            }
            for (index, block) in pointers.into_iter().enumerate() {
                if block == 0 {
                    continue;
                }
                self.check_data(block)?;
                // Reject aliases, unallocated blocks and allocations hidden beyond EOF.
                if index >= inode.size.div_ceil(BLOCK)
                    || !bit(&*blocks, block)
                    || bit(&*seen, block)
                {
                    return Err(INVALID_FILESYSTEM);
                }
                set_bit(&mut *seen, block, true);
                used += 8;
            }
            if used != u32_at(&inode.raw, 28) {
                return Err(INVALID_FILESYSTEM);
            }
        }
        if directories != u16_at(&self.group, 16) as u32 {
            return Err(INVALID_FILESYSTEM);
        }
        for block in self.data_start..self.blocks {
            if bit(&*blocks, block) != bit(&*seen, block) {
                return Err(INVALID_FILESYSTEM);
            }
        }
        Ok(())
    }
}
fn bit(bytes: &[u8], index: u32) -> bool {
    bytes[index as usize / 8] & (1 << (index % 8)) != 0
}
fn set_bit(bytes: &mut [u8], index: u32, on: bool) {
    let b = &mut bytes[index as usize / 8];
    let mask = 1 << (index % 8);
    if on {
        *b |= mask;
    } else {
        *b &= !mask;
    }
}
fn directory_record(block: &[u8; BLOCK], at: usize) -> Result<(u32, usize, usize), i64> {
    if at > BLOCK - 8 {
        return Err(INVALID_FILESYSTEM);
    }
    let length = u16_at(block, at + 4) as usize;
    let name = block[at + 6] as usize;
    if length < 8
        || !length.is_multiple_of(4)
        || at + length > BLOCK
        || name > length - 8
        || block[at + 7] > 2
    {
        return Err(INVALID_FILESYSTEM);
    }
    Ok((u32_at(block, at), length, name))
}
fn record(block: &mut [u8; BLOCK], at: usize, length: usize, inode: u32, name: &str, kind: u8) {
    block[at..at + length].fill(0);
    put32(block, at, inode);
    put16(block, at + 4, length as u16);
    block[at + 6] = name.len() as u8;
    block[at + 7] = kind;
    block[at + 8..at + 8 + name.len()].copy_from_slice(name.as_bytes());
}
fn read_block(
    device: &mut dyn BlockDevice,
    block: u32,
    bytes: &mut [u8; BLOCK],
) -> Result<(), i64> {
    let sector = u64::from(block) * 8;
    check_range(device.sectors(), sector, BLOCK)?;
    for (n, part) in bytes.chunks_exact_mut(SECTOR).enumerate() {
        device.read_sector(sector + n as u64, part.try_into().unwrap())?;
    }
    Ok(())
}
fn write_block(device: &mut dyn BlockDevice, block: u32, bytes: &[u8; BLOCK]) -> Result<(), i64> {
    let sector = u64::from(block) * 8;
    check_range(device.sectors(), sector, BLOCK)?;
    for (n, part) in bytes.chunks_exact(SECTOR).enumerate() {
        device.write_sector(sector + n as u64, part.try_into().unwrap())?;
    }
    Ok(())
}

fn buffer() -> Result<Box<[u8; BLOCK]>, i64> {
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(BLOCK).map_err(|_| NO_MEMORY)?;
    bytes.resize(BLOCK, 0);
    bytes.into_boxed_slice().try_into().map_err(|_| NO_MEMORY)
}
