// SPDX-License-Identifier: Apache-2.0
//! Read checked ext4 extents and optionally stage bounded, non-journal mutations before disk I/O.
//! Default mounts remain readonly; writable admission additionally audits allocation and namespace ownership.
mod audit;
mod directory;
mod write;
use crate::{
    BlockDevice, SECTOR, check_range,
    filesystem::{Capabilities, Entry, Filesystem, Format, Metadata},
};
use alloc::{boxed::Box, vec::Vec};
use anova_abi::*;

pub const BLOCK: usize = 4096;
pub const MAX_FILE: usize = 256 * 1024 * 1024;
pub const MAX_GROUPS: u32 = 128;
const MAX_EXTENTS: usize = 1024;
const MAX_TREE_BLOCKS: usize = 64;

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn put16(b: &mut [u8], at: usize, value: u16) {
    b[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(b: &mut [u8], at: usize, value: u32) {
    b[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn buffer() -> Result<Box<[u8; BLOCK]>, i64> {
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(BLOCK).map_err(|_| NO_MEMORY)?;
    bytes.resize(BLOCK, 0);
    bytes.into_boxed_slice().try_into().map_err(|_| NO_MEMORY)
}
fn read_block(
    device: &mut dyn BlockDevice,
    number: u32,
    bytes: &mut [u8; BLOCK],
) -> Result<(), i64> {
    let sector = u64::from(number) * (BLOCK / SECTOR) as u64;
    check_range(device.sectors(), sector, BLOCK)?;
    for (i, output) in bytes.chunks_exact_mut(SECTOR).enumerate() {
        device.read_sector(sector + i as u64, output.try_into().unwrap())?;
    }
    Ok(())
}
fn power(mut group: u32, base: u32) -> bool {
    while group > 1 && group.is_multiple_of(base) {
        group /= base;
    }
    group == 1
}
fn has_super(group: u32, sparse: bool) -> bool {
    !sparse || group == 0 || power(group, 3) || power(group, 5) || power(group, 7)
}
#[derive(Clone, Copy)]
struct Group {
    start: u32,
    end: u32,
    prefix: u32,
    blocks: u32,
    inodes: u32,
    table: u32,
    table_end: u32,
}
#[derive(Clone, Copy)]
struct Extent {
    logical: u32,
    length: u32,
    physical: u32,
    unwritten: bool,
}
struct Cached {
    info: Metadata,
    raw: [u8; 256],
    extents: Vec<Extent>,
    tree: Vec<u32>,
}

pub struct Ext4 {
    device: Box<dyn BlockDevice>,
    blocks: u32,
    inodes: u32,
    blocks_per_group: u32,
    inodes_per_group: u32,
    groups: Vec<Group>,
    bitmap: Box<[u8; BLOCK]>,
    bitmap_number: Option<u32>,
    cached: Option<Cached>,
    writable: bool,
    dirty: bool,
    failed: bool,
    clock: u32,
    orphans: Vec<u32>,
    transaction: Option<write::Transaction>,
}

impl Ext4 {
    pub fn mount(mut device: Box<dyn BlockDevice>) -> Result<Self, i64> {
        let mut bytes = buffer()?;
        read_block(&mut *device, 0, &mut bytes)?;
        let s = &bytes[1024..2048];
        // No flex/64bit, indexed directories, xattrs, checksums, bigalloc or journal recovery.
        // Journal-bearing images also remain outside this deliberately non-journal profile.
        if u16_at(s, 56) != 0xef53
            || u16_at(s, 58) != 1
            || u32_at(s, 20) != 0
            || u32_at(s, 24) != 2
            || u32_at(s, 28) != 2
            || u32_at(s, 72) != 0
            || u32_at(s, 76) != 1
            || u32_at(s, 84) != 11
            || u16_at(s, 88) != 256
            || u32_at(s, 92) != 0
            || u32_at(s, 96) != 0x42
            || u32_at(s, 100) & !3 != 0
            || u32_at(s, 232) != 0
            || !matches!(u16_at(s, 254), 0 | 32)
            || u32_at(s, 336) != 0
            || u32_at(s, 340) != 0
            || u32_at(s, 344) != 0
        {
            return Err(INVALID_FILESYSTEM);
        }
        let blocks = u32_at(s, 4);
        let inodes = u32_at(s, 0);
        let bpg = u32_at(s, 32);
        let ipg = u32_at(s, 40);
        if !(256..=32768).contains(&bpg)
            || !bpg.is_power_of_two()
            || u32_at(s, 36) != bpg
            || !(16..=32768).contains(&ipg)
            || !ipg.is_multiple_of((BLOCK / 256) as u32)
            || blocks < bpg
            || u64::from(blocks) * 8 > device.sectors()
            || inodes < 16
            || u32_at(s, 12) > blocks
            || u32_at(s, 16) > inodes
        {
            return Err(INVALID_FILESYSTEM);
        }
        let count = blocks.div_ceil(bpg);
        if count > MAX_GROUPS || count.checked_mul(ipg) != Some(inodes) {
            return Err(INVALID_FILESYSTEM);
        }
        let descriptors = (count * 32).div_ceil(BLOCK as u32);
        let table_blocks = ipg / (BLOCK / 256) as u32;
        let sparse = u32_at(s, 100) & 1 != 0;
        let mut groups = Vec::new();
        groups
            .try_reserve_exact(count as usize)
            .map_err(|_| NO_MEMORY)?;
        for i in 0..count {
            read_block(&mut *device, 1 + i * 32 / BLOCK as u32, &mut bytes)?;
            let d = &bytes[(i as usize * 32) % BLOCK..][..32];
            let start = i.checked_mul(bpg).ok_or(INVALID_FILESYSTEM)?;
            let end = start
                .checked_add(bpg)
                .ok_or(INVALID_FILESYSTEM)?
                .min(blocks);
            let prefix = start
                + if has_super(i, sparse) {
                    1 + descriptors
                } else {
                    0
                };
            let bb = u32_at(d, 0);
            let ib = u32_at(d, 4);
            let table = u32_at(d, 8);
            let table_end = table.checked_add(table_blocks).ok_or(INVALID_FILESYSTEM)?;
            if prefix > end
                || bb < prefix
                || bb >= end
                || ib < prefix
                || ib >= end
                || bb == ib
                || table < prefix
                || table_end > end
                || (table..table_end).contains(&bb)
                || (table..table_end).contains(&ib)
                || u16_at(d, 12) as u32 > end - start
                || u16_at(d, 14) as u32 > ipg
                || u16_at(d, 16) as u32 > ipg
                || u16_at(d, 18) & !4 != 0
                || u32_at(d, 20) != 0
                || u32_at(d, 24) != 0
                || u16_at(d, 30) != 0
            {
                return Err(INVALID_FILESYSTEM);
            }
            groups.push(Group {
                start,
                end,
                prefix,
                blocks: bb,
                inodes: ib,
                table,
                table_end,
            });
        }
        let mut fs = Self {
            device,
            blocks,
            inodes,
            blocks_per_group: bpg,
            inodes_per_group: ipg,
            groups,
            bitmap: buffer()?,
            bitmap_number: None,
            cached: None,
            writable: false,
            dirty: false,
            failed: false,
            clock: 0,
            orphans: Vec::new(),
            transaction: None,
        };
        for i in 0..fs.groups.len() {
            let g = fs.groups[i];
            for block in (g.start..g.prefix)
                .chain([g.blocks, g.inodes])
                .chain(g.table..g.table_end)
            {
                if !fs.allocated_block(block)? {
                    return Err(INVALID_FILESYSTEM);
                }
            }
        }
        if !fs.load(2)?.directory {
            return Err(INVALID_FILESYSTEM);
        }
        Ok(fs)
    }
    fn read_block(&mut self, n: u32, output: &mut [u8; BLOCK]) -> Result<(), i64> {
        if self.failed {
            return Err(IO_ERROR);
        }
        if n >= self.blocks {
            return Err(INVALID_FILESYSTEM);
        }
        if let Some(change) = self.transaction.as_ref().and_then(|t| t.find(n)) {
            output.copy_from_slice(&change.bytes[..]);
            return Ok(());
        }
        read_block(&mut *self.device, n, output)
    }
    fn bit(&mut self, bitmap: u32, bit: u32) -> Result<bool, i64> {
        if bit >= (BLOCK * 8) as u32 {
            return Err(INVALID_FILESYSTEM);
        }
        if self.bitmap_number != Some(bitmap) {
            self.bitmap_number = None;
            if let Some(change) = self.transaction.as_ref().and_then(|t| t.find(bitmap)) {
                self.bitmap.copy_from_slice(&change.bytes[..]);
            } else {
                read_block(&mut *self.device, bitmap, &mut self.bitmap)?;
            }
            self.bitmap_number = Some(bitmap);
        }
        Ok(self.bitmap[bit as usize / 8] & (1 << (bit % 8)) != 0)
    }
    fn allocated_block(&mut self, block: u32) -> Result<bool, i64> {
        let group = self
            .groups
            .get((block / self.blocks_per_group) as usize)
            .ok_or(INVALID_FILESYSTEM)?;
        let bitmap = group.blocks;
        self.bit(bitmap, block % self.blocks_per_group)
    }
    fn check_data(&mut self, block: u32) -> Result<(), i64> {
        let g = self
            .groups
            .get((block / self.blocks_per_group) as usize)
            .ok_or(INVALID_FILESYSTEM)?;
        if block >= g.end
            || block < g.prefix
            || block == g.blocks
            || block == g.inodes
            || (g.table..g.table_end).contains(&block)
            || !self.allocated_block(block)?
        {
            return Err(INVALID_FILESYSTEM);
        }
        Ok(())
    }
    fn load(&mut self, number: u32) -> Result<Metadata, i64> {
        if self.failed {
            return Err(IO_ERROR);
        }
        if let Some(cached) = &self.cached
            && cached.info.number == number
        {
            return Ok(cached.info);
        }
        if number == 0 || number > self.inodes {
            return Err(INVALID_FILESYSTEM);
        }
        let index = (number - 1) / self.inodes_per_group;
        let local = (number - 1) % self.inodes_per_group;
        let group = self.groups[index as usize];
        if !self.bit(group.inodes, local)? {
            return Err(INVALID_FILESYSTEM);
        }
        let mut bytes = buffer()?;
        self.read_block(group.table + local / (BLOCK / 256) as u32, &mut bytes)?;
        let raw = &bytes[(local as usize * 256) % BLOCK..][..256];
        let mode = u16_at(raw, 0);
        let directory = mode & 0xf000 == 0x4000;
        let size = u32_at(raw, 4) as usize;
        let links = u16_at(raw, 26);
        if (!directory && mode & 0xf000 != 0x8000)
            || links == 0 && !self.is_orphan(number)
            || !directory && links > 1
            || u32_at(raw, 20) != 0
            || u32_at(raw, 32) != 0x80000
            || u32_at(raw, 104) != 0
            || u32_at(raw, 108) != 0
            || u16_at(raw, 116) != 0
            || u16_at(raw, 118) != 0
            || size > MAX_FILE
            || directory && (size == 0 || !size.is_multiple_of(BLOCK))
            || !matches!(u16_at(raw, 128), 0 | 32)
            || u32_at(raw, 132) != 0
            || u32_at(raw, 136) != 0
            || u32_at(raw, 140) != 0
        {
            return Err(INVALID_FILESYSTEM);
        }
        let info = Metadata {
            number,
            directory,
            size,
            executable: mode & 0o111 != 0,
            permissions: mode & 0o777,
            accessed: u32_at(raw, 8),
            changed: u32_at(raw, 12),
            modified: u32_at(raw, 16),
        };
        let root = &raw[40..100];
        let depth = u16_at(root, 6);
        if depth > 3 {
            return Err(INVALID_FILESYSTEM);
        }
        let mut extents = Vec::new();
        let mut visited = Vec::new();
        self.tree(
            root,
            depth,
            0,
            (size.div_ceil(BLOCK)) as u64,
            &mut extents,
            &mut visited,
        )?;
        let mut allocated = visited.len() as u64;
        for (i, e) in extents.iter().enumerate() {
            allocated += u64::from(e.length);
            for block in e.physical..e.physical + e.length {
                self.check_data(block)?;
                if visited.contains(&block) {
                    return Err(INVALID_FILESYSTEM);
                }
            }
            if extents[..i].iter().any(|old: &Extent| {
                e.physical < old.physical + old.length && old.physical < e.physical + e.length
            }) {
                return Err(INVALID_FILESYSTEM);
            }
        }
        if allocated * 8 != u64::from(u32_at(raw, 28)) {
            return Err(INVALID_FILESYSTEM);
        }
        let raw = raw.try_into().unwrap();
        self.cached = Some(Cached {
            info,
            raw,
            extents,
            tree: visited,
        });
        Ok(info)
    }
    fn tree(
        &mut self,
        bytes: &[u8],
        depth: u16,
        lower: u64,
        upper: u64,
        extents: &mut Vec<Extent>,
        visited: &mut Vec<u32>,
    ) -> Result<(), i64> {
        let count = u16_at(bytes, 2) as usize;
        let maximum = u16_at(bytes, 4) as usize;
        if u16_at(bytes, 0) != 0xf30a
            || u16_at(bytes, 6) != depth
            || maximum != (bytes.len() - 12) / 12
            || count > maximum
            || depth != 0 && count == 0
        {
            return Err(INVALID_FILESYSTEM);
        }
        let mut previous_end = lower;
        for i in 0..count {
            let at = 12 + i * 12;
            let logical = u32_at(bytes, at);
            if u64::from(logical) < previous_end || u64::from(logical) >= upper {
                return Err(INVALID_FILESYSTEM);
            }
            if depth == 0 {
                let length = u16_at(bytes, at + 4);
                let unwritten = length > 32768;
                let length = u32::from(if unwritten { length - 32768 } else { length });
                let physical = u32_at(bytes, at + 8);
                let end = u64::from(logical) + u64::from(length);
                if length == 0
                    || u16_at(bytes, at + 6) != 0
                    || physical == 0
                    || physical
                        .checked_add(length)
                        .is_none_or(|end| end > self.blocks)
                    || end > upper
                    || extents.len() == MAX_EXTENTS
                {
                    return Err(INVALID_FILESYSTEM);
                }
                extents.try_reserve(1).map_err(|_| NO_MEMORY)?;
                extents.push(Extent {
                    logical,
                    physical,
                    length,
                    unwritten,
                });
                previous_end = end;
            } else {
                let block = u32_at(bytes, at + 4);
                if u32_at(bytes, at + 8) != 0
                    || visited.contains(&block)
                    || visited.len() == MAX_TREE_BLOCKS
                {
                    return Err(INVALID_FILESYSTEM);
                }
                self.check_data(block)?;
                visited.try_reserve(1).map_err(|_| NO_MEMORY)?;
                visited.push(block);
                let end = if i + 1 < count {
                    u64::from(u32_at(bytes, at + 12))
                } else {
                    upper
                };
                if end <= u64::from(logical) || end > upper {
                    return Err(INVALID_FILESYSTEM);
                }
                let mut child = buffer()?;
                self.read_block(block, &mut child)?;
                self.tree(
                    &child[..],
                    depth - 1,
                    u64::from(logical),
                    end,
                    extents,
                    visited,
                )?;
                previous_end = end;
            }
        }
        Ok(())
    }
    fn data_block(&self, logical: usize) -> Option<u32> {
        self.cached
            .as_ref()?
            .extents
            .iter()
            .find(|e| logical >= e.logical as usize && logical < (e.logical + e.length) as usize)
            .and_then(|e| (!e.unwritten).then_some(e.physical + logical as u32 - e.logical))
    }
    fn record(bytes: &[u8; BLOCK], at: usize) -> Result<(u32, usize, usize), i64> {
        if at > BLOCK - 8 {
            return Err(INVALID_FILESYSTEM);
        }
        let length = u16_at(bytes, at + 4) as usize;
        let n = bytes[at + 6] as usize;
        if length < 8 || !length.is_multiple_of(4) || at + length > BLOCK || n > length - 8 {
            return Err(INVALID_FILESYSTEM);
        }
        Ok((u32_at(bytes, at), length, n))
    }
}
impl Filesystem for Ext4 {
    fn directory_moves(&self) -> bool {
        self.writable
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            format: Format::Ext4,
            root: 2,
            max_file: MAX_FILE,
            timestamp_nanos: 1_000_000_000,
            read_only: !self.writable,
        }
    }
    fn inode(&mut self, number: u32) -> Result<Metadata, i64> {
        self.load(number)
    }
    fn read(&mut self, number: u32, offset: usize, output: &mut [u8]) -> Result<usize, i64> {
        let info = self.load(number)?;
        if info.directory {
            return Err(IS_DIRECTORY);
        }
        let n = output.len().min(info.size.saturating_sub(offset));
        let mut done = 0;
        let mut bytes = buffer()?;
        while done < n {
            let position = offset + done;
            let take = (BLOCK - position % BLOCK).min(n - done);
            if let Some(block) = self.data_block(position / BLOCK) {
                self.read_block(block, &mut bytes)?;
                output[done..done + take]
                    .copy_from_slice(&bytes[position % BLOCK..position % BLOCK + take]);
            } else {
                output[done..done + take].fill(0);
            }
            done += take;
        }
        Ok(n)
    }
    fn next_entry(&mut self, number: u32, cursor: u64) -> Result<Option<(Entry, u64)>, i64> {
        let info = self.load(number)?;
        if !info.directory {
            return Err(NOT_DIRECTORY);
        }
        let mut position = usize::try_from(cursor).map_err(|_| INVALID_ARGUMENT)?;
        if position > info.size || !position.is_multiple_of(4) {
            return Err(INVALID_ARGUMENT);
        }
        let mut bytes = buffer()?;
        while position < info.size {
            let block = self
                .data_block(position / BLOCK)
                .ok_or(INVALID_FILESYSTEM)?;
            self.read_block(block, &mut bytes)?;
            let mut at = 0;
            while at < position % BLOCK {
                at += Self::record(&bytes, at)?.1;
            }
            if at != position % BLOCK {
                return Err(INVALID_ARGUMENT);
            }
            while at < BLOCK {
                let (inode, length, n) = Self::record(&bytes, at)?;
                let name = &bytes[at + 8..at + 8 + n];
                let kind = bytes[at + 7];
                at += length;
                position += length;
                if inode == 0 {
                    continue;
                }
                if inode > self.inodes
                    || name.is_empty()
                    || name.contains(&0)
                    || name.contains(&b'/')
                    || core::str::from_utf8(name).is_err()
                    || !(1..=2).contains(&kind)
                {
                    return Err(INVALID_FILESYSTEM);
                }
                if name == b"." || name == b".." {
                    continue;
                }
                let mut label = [0; 256];
                label[..n].copy_from_slice(name);
                return Ok(Some((
                    Entry {
                        inode,
                        name: label,
                        length: n,
                    },
                    position as u64,
                )));
            }
        }
        Ok(None)
    }
    fn sync(&mut self) -> Result<(), i64> {
        self.flush_writable()
    }
    fn shutdown(&mut self) -> Result<(), i64> {
        self.shutdown_writable()
    }
    fn stop_device(&mut self) -> Result<(), i64> {
        self.device.stop()
    }
    fn set_clock(&mut self, seconds: u32) {
        self.clock = seconds;
    }
    fn write(&mut self, n: u32, offset: usize, input: &[u8]) -> Result<usize, i64> {
        self.mutate(|s| s.write_inner(n, offset, input))
    }
    fn resize(&mut self, n: u32, length: usize) -> Result<(), i64> {
        self.mutate(|s| s.resize_inner(n, length))
    }
    fn set_executable(&mut self, n: u32) -> Result<(), i64> {
        self.mutate(|s| s.executable_inner(n))
    }
    fn set_permissions(&mut self, n: u32, permissions: u16) -> Result<(), i64> {
        self.mutate(|s| s.permissions_inner(n, permissions))
    }
    fn set_times(
        &mut self,
        n: u32,
        accessed: Option<u32>,
        modified: Option<u32>,
    ) -> Result<(), i64> {
        self.mutate(|s| s.times_inner(n, accessed, modified))
    }
    fn create(&mut self, p: u32, name: &str, directory: bool) -> Result<u32, i64> {
        self.mutate(|s| s.create_inner(p, name, directory))
    }
    fn unlink(&mut self, p: u32, name: &str) -> Result<u32, i64> {
        self.mutate(|s| s.unlink_inner(p, name, false))
    }
    fn rmdir(&mut self, p: u32, name: &str) -> Result<u32, i64> {
        self.mutate(|s| s.unlink_inner(p, name, true))
    }
    fn rename(&mut self, p: u32, name: &str, d: u32, new: &str) -> Result<(), i64> {
        self.mutate(|s| s.rename_inner(p, name, d, new))
    }
    fn release_unlinked(&mut self, n: u32) -> Result<(), i64> {
        self.mutate(|s| s.release_inner(n))
    }
}
