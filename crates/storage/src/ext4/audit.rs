// SPDX-License-Identifier: Apache-2.0
//! Audit the entire bounded writable volume before admitting allocation or metadata mutation.
//! Cross-check ownership, bitmap padding/counters, directory types, parent chains and inode link counts.
use super::write::MAX_DIRECTORY_BLOCKS;
use super::*;

fn zeros<T: Default + Clone>(count: usize) -> Result<Vec<T>, i64> {
    let mut v = Vec::new();
    v.try_reserve_exact(count).map_err(|_| NO_MEMORY)?;
    v.resize(count, T::default());
    Ok(v)
}
fn bit(bytes: &[u8], n: u32) -> bool {
    bytes[n as usize / 8] & (1 << (n % 8)) != 0
}
fn claim(bytes: &mut [u8], n: u32) -> Result<(), i64> {
    if bit(bytes, n) {
        return Err(INVALID_FILESYSTEM);
    }
    bytes[n as usize / 8] |= 1 << (n % 8);
    Ok(())
}
impl Ext4 {
    pub(super) fn audit_writable(&mut self) -> Result<(), i64> {
        // Bound the audit independently of the larger readonly format geometry.
        if self.inodes > 16384 {
            return Err(UNSUPPORTED);
        }
        let mut owned: Vec<u8> = zeros((self.blocks as usize).div_ceil(8))?;
        let mut kinds: Vec<u8> = zeros(self.inodes as usize + 1)?;
        let mut expected: Vec<u8> = zeros(self.inodes as usize + 1)?;
        let mut links: Vec<u16> = zeros(self.inodes as usize + 1)?;
        let mut refs: Vec<u16> = zeros(self.inodes as usize + 1)?;
        let mut parents: Vec<u32> = zeros(self.inodes as usize + 1)?;
        let mut dotdots: Vec<u32> = zeros(self.inodes as usize + 1)?;
        let mut total_free_blocks = 0u32;
        let mut total_free_inodes = 0u32;
        let mut bytes = buffer()?;
        for index in 0..self.groups.len() {
            let g = self.groups[index];
            for block in (g.start..g.prefix)
                .chain([g.blocks, g.inodes])
                .chain(g.table..g.table_end)
            {
                claim(&mut owned, block)?;
            }
            let mut inode_bits = buffer()?;
            self.read_block(g.inodes, &mut inode_bits)?;
            let mut free_inodes = 0;
            let mut directories = 0;
            for local in 0..(BLOCK * 8) as u32 {
                if local >= self.inodes_per_group {
                    if !bit(&inode_bits[..], local) {
                        return Err(INVALID_FILESYSTEM);
                    }
                    continue;
                }
                let n = index as u32 * self.inodes_per_group + local + 1;
                if !bit(&inode_bits[..], local) {
                    if n < 11 {
                        return Err(INVALID_FILESYSTEM);
                    }
                    free_inodes += 1;
                    continue;
                }
                if n < 11 && n != 2 {
                    self.read_block(g.table + local / (BLOCK / 256) as u32, &mut bytes)?;
                    let raw = &bytes[(local as usize * 256) % BLOCK..][..256];
                    // mke2fs stamps the reserved bad-block inode even when it has no bad blocks.
                    if raw
                        .iter()
                        .enumerate()
                        .any(|(i, v)| *v != 0 && !(8..20).contains(&i))
                    {
                        return Err(INVALID_FILESYSTEM);
                    }
                    continue;
                }
                let inode = self.snapshot(n)?;
                let k = if inode.info.directory { 2 } else { 1 };
                kinds[n as usize] = k;
                links[n as usize] = u16_at(&inode.raw, 26);
                // The controlled namespace is root-owned and has no ACLs, special modes or high IDs.
                if u16_at(&inode.raw, 0) & 0o7000 != 0
                    || u16_at(&inode.raw, 2) != 0
                    || u16_at(&inode.raw, 24) != 0
                    || u32_at(&inode.raw, 120) != 0
                {
                    return Err(UNSUPPORTED);
                }
                for block in &inode.tree {
                    claim(&mut owned, *block)?;
                }
                for e in &inode.extents {
                    for block in e.physical..e.physical + e.length {
                        claim(&mut owned, block)?;
                    }
                }
                if !inode.info.directory {
                    continue;
                }
                directories += 1;
                if inode.info.size > MAX_DIRECTORY_BLOCKS * BLOCK {
                    return Err(UNSUPPORTED);
                }
                let mut dot = false;
                let mut dotdot = false;
                let mut names = Vec::new();
                for logical in 0..inode.info.size / BLOCK {
                    let e = Self::mapping(&inode, logical as u32)
                        .filter(|e| !e.unwritten)
                        .ok_or(INVALID_FILESYSTEM)?;
                    let block = e.physical + logical as u32 - e.logical;
                    self.read_block(block, &mut bytes)?;
                    let mut at = 0;
                    while at < BLOCK {
                        let (child, length, count) = Self::record(&bytes, at)?;
                        if child != 0 {
                            let name = &bytes[at + 8..at + 8 + count];
                            let kind = bytes[at + 7];
                            if child > self.inodes
                                || name.is_empty()
                                || name.contains(&0)
                                || name.contains(&b'/')
                                || core::str::from_utf8(name).is_err()
                                || !(1..=2).contains(&kind)
                            {
                                return Err(INVALID_FILESYSTEM);
                            }
                            let c = child as usize;
                            let hash = name.iter().fold(0xcbf29ce484222325u64, |h, b| {
                                (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
                            });
                            names.try_reserve(1).map_err(|_| NO_MEMORY)?;
                            names.push((hash, block, at as u16, count as u8));
                            if expected[c] != 0 && expected[c] != kind {
                                return Err(INVALID_FILESYSTEM);
                            }
                            expected[c] = kind;
                            refs[c] = refs[c].checked_add(1).ok_or(INVALID_FILESYSTEM)?;
                            if name == b"." {
                                if dot || child != n || kind != 2 {
                                    return Err(INVALID_FILESYSTEM);
                                }
                                dot = true;
                            } else if name == b".." {
                                if dotdot || kind != 2 {
                                    return Err(INVALID_FILESYSTEM);
                                }
                                dotdot = true;
                                dotdots[n as usize] = child;
                            } else if kind == 2 {
                                if parents[c] != 0 || child == 2 {
                                    return Err(INVALID_FILESYSTEM);
                                }
                                parents[c] = n;
                            }
                        }
                        at += length;
                    }
                }
                if !dot || !dotdot {
                    return Err(INVALID_FILESYSTEM);
                }
                names.sort_unstable_by_key(|n| n.0);
                let mut previous = buffer()?;
                for i in 1..names.len() {
                    let (hash, block, at, count) = names[i];
                    for &(old_hash, old_block, old_at, old_count) in names[..i].iter().rev() {
                        if old_hash != hash {
                            break;
                        }
                        if old_count != count {
                            continue;
                        }
                        self.read_block(block, &mut bytes)?;
                        self.read_block(old_block, &mut previous)?;
                        if bytes[at as usize + 8..at as usize + 8 + count as usize]
                            == previous[old_at as usize + 8..old_at as usize + 8 + count as usize]
                        {
                            return Err(INVALID_FILESYSTEM);
                        }
                    }
                }
            }
            self.read_block(g.blocks, &mut bytes)?;
            let mut free_blocks = 0;
            for local in 0..(BLOCK * 8) as u32 {
                if local >= g.end - g.start {
                    if !bit(&bytes[..], local) {
                        return Err(INVALID_FILESYSTEM);
                    }
                } else if !bit(&bytes[..], local) {
                    free_blocks += 1;
                }
            }
            self.read_block(1 + index as u32 * 32 / BLOCK as u32, &mut bytes)?;
            let at = index * 32 % BLOCK;
            if u16_at(&bytes[..], at + 12) as u32 != free_blocks
                || u16_at(&bytes[..], at + 14) as u32 != free_inodes
                || u16_at(&bytes[..], at + 16) as u32 != directories
            {
                return Err(INVALID_FILESYSTEM);
            }
            total_free_blocks += free_blocks;
            total_free_inodes += free_inodes;
        }
        // Every allocated block must have exactly one admitted owner, including metadata.
        for index in 0..self.groups.len() {
            let g = self.groups[index];
            self.read_block(g.blocks, &mut bytes)?;
            for n in g.start..g.end {
                if bit(&bytes[..], n - g.start) != bit(&owned, n) {
                    return Err(INVALID_FILESYSTEM);
                }
            }
        }
        parents[2] = 2;
        for n in 1..=self.inodes as usize {
            if expected[n] != kinds[n] || refs[n] != links[n] {
                return Err(INVALID_FILESYSTEM);
            }
            if kinds[n] != 2 {
                continue;
            }
            if parents[n] == 0 || dotdots[n] != parents[n] {
                return Err(INVALID_FILESYSTEM);
            }
            let mut ancestor = n;
            for steps in 0..=self.inodes {
                if ancestor == 2 {
                    break;
                }
                if steps == self.inodes || parents[ancestor] == 0 {
                    return Err(INVALID_FILESYSTEM);
                }
                ancestor = parents[ancestor] as usize;
            }
        }
        self.read_block(0, &mut bytes)?;
        if u32_at(&bytes[..], 1036) != total_free_blocks
            || u32_at(&bytes[..], 1040) != total_free_inodes
        {
            return Err(INVALID_FILESYSTEM);
        }
        Ok(())
    }
}
