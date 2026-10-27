// SPDX-License-Identifier: Apache-2.0
//! Implement bounded internal JBD2 checksum-v3 transactions, validated replay and persistent orphan recovery.
//! A single transaction journals data and metadata, flushes its commit, checkpoints and retires before reuse.
use super::write::{Change, MAX_TRANSACTION_BLOCKS, Transaction};
use super::*;

const MAGIC: u32 = 0xc03b3998;
const TAGS: usize = (BLOCK - 12 - 16 - 4) / 16;
const CHECKSUM_V3: u32 = 0x10;

fn be(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
}
fn set(b: &mut [u8], at: usize, value: u32) {
    b[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
const fn crc_table() -> [u32; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut bit = 0;
        while bit < 8 {
            c = (c >> 1) ^ if c & 1 != 0 { 0x82f63b78 } else { 0 };
            bit += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}
fn crc(mut seed: u32, bytes: &[u8]) -> u32 {
    const TABLE: [u32; 256] = crc_table();
    for b in bytes {
        seed = TABLE[((seed ^ u32::from(*b)) & 255) as usize] ^ (seed >> 8);
    }
    seed
}
fn checksum(bytes: &mut [u8], at: usize, seed: u32) {
    set(bytes, at, 0);
    let sum = crc(seed, bytes);
    set(bytes, at, sum);
}
fn verified(bytes: &mut [u8], at: usize, seed: u32) -> bool {
    let expected = be(bytes, at);
    checksum(bytes, at, seed);
    expected == be(bytes, at)
}
fn header(bytes: &mut [u8], kind: u32, sequence: u32) {
    set(bytes, 0, MAGIC);
    set(bytes, 4, kind);
    set(bytes, 8, sequence);
}
pub(super) struct Journal {
    pub blocks: Vec<u32>,
    superblock: Box<[u8; BLOCK]>,
    sequence: u32,
    seed: u32,
}
impl Journal {
    fn superblock(&self, output: &mut [u8; BLOCK], start: u32, sequence: u32) {
        output.copy_from_slice(&self.superblock[..]);
        set(output, 24, sequence);
        set(output, 28, start);
        set(output, 40, CHECKSUM_V3 | 1);
        output[80] = 4;
        checksum(&mut output[..1024], 252, !0);
    }
}
impl Ext4 {
    pub(super) fn open_journal(&mut self, uuid: [u8; 16], recovery: bool) -> Result<Journal, i64> {
        // The journal map is immutable, inline and independent of recoverable namespace metadata.
        let mut bytes = buffer()?;
        self.read_block(self.groups[0].table, &mut bytes)?;
        let raw = &bytes[7 * 256..8 * 256];
        let size = u32_at(raw, 4);
        let count = u16_at(raw, 42) as usize;
        if u16_at(raw, 0) != 0x8180
            || u16_at(raw, 26) != 1
            || u32_at(raw, 32) != 0x80000
            || u32_at(raw, 108) != 0
            || !(1024 * BLOCK as u32..=8192 * BLOCK as u32).contains(&size)
            || !size.is_multiple_of(BLOCK as u32)
            || u16_at(raw, 40) != 0xf30a
            || u16_at(raw, 44) != 4
            || u16_at(raw, 46) != 0
            || !(1..=4).contains(&count)
            || u32_at(raw, 28) != size / SECTOR as u32
        {
            return Err(UNSUPPORTED);
        }
        let mut blocks = Vec::new();
        blocks
            .try_reserve_exact(size as usize / BLOCK)
            .map_err(|_| NO_MEMORY)?;
        for i in 0..count {
            let at = 52 + i * 12;
            let logical = u32_at(raw, at);
            let length = u16_at(raw, at + 4) as u32;
            let physical = u32_at(raw, at + 8);
            if logical as usize != blocks.len()
                || length == 0
                || length > 32768
                || u16_at(raw, at + 6) != 0
                || physical
                    .checked_add(length)
                    .is_none_or(|end| end > self.blocks)
                || blocks.len() + length as usize > size as usize / BLOCK
            {
                return Err(INVALID_FILESYSTEM);
            }
            for n in physical..physical + length {
                self.check_data(n)?;
                if blocks.contains(&n) {
                    return Err(INVALID_FILESYSTEM);
                }
                blocks.push(n);
            }
        }
        if blocks.len() != size as usize / BLOCK {
            return Err(INVALID_FILESYSTEM);
        }
        self.read_block(blocks[0], &mut bytes)?;
        let features = be(&bytes[..], 40);
        if be(&bytes[..], 0) != MAGIC
            || be(&bytes[..], 4) != 4
            || be(&bytes[..], 8) != 0
            || be(&bytes[..], 12) != BLOCK as u32
            || be(&bytes[..], 16) as usize != blocks.len()
            || be(&bytes[..], 20) != 1
            || be(&bytes[..], 24) == 0
            || !matches!(be(&bytes[..], 28), 0 | 1)
            || be(&bytes[..], 32) != 0
            || be(&bytes[..], 36) != 0
            || features & !(CHECKSUM_V3 | 1) != 0
            || be(&bytes[..], 44) != 0
            || bytes[48..64] != uuid
            || be(&bytes[..], 64) != 1
            || be(&bytes[..], 68) != 0
            || be(&bytes[..], 84) != 0
            || !recovery && be(&bytes[..], 28) != 0
        {
            return Err(INVALID_FILESYSTEM);
        }
        if features & CHECKSUM_V3 != 0 {
            if bytes[80] != 4 || !verified(&mut bytes[..1024], 252, !0) {
                return Err(INVALID_FILESYSTEM);
            }
        } else if be(&bytes[..], 28) != 0 {
            // Only an empty, clean mkfs journal may be upgraded to checksum-v3 on first write.
            return Err(UNSUPPORTED);
        }
        let sequence = be(&bytes[..], 24);
        Ok(Journal {
            blocks,
            superblock: bytes,
            sequence,
            seed: crc(!0, &uuid),
        })
    }

    pub(super) fn commit_journal(&mut self, mut t: Transaction) -> Result<(), i64> {
        let mut primary = buffer()?;
        self.read_block(0, &mut primary)?;
        put32(&mut primary[..], 1120, 0x46);
        put16(&mut primary[..], 1082, 1);
        if let Some(c) = t.changes.iter_mut().find(|c| c.number == 0) {
            put32(&mut c.bytes[..], 1120, 0x46);
            put16(&mut c.bytes[..], 1082, 1);
        }
        let j = self.journal.as_ref().unwrap();
        if t.changes.len() + t.changes.len().div_ceil(TAGS) + 2 > j.blocks.len() {
            return Err(NO_MEMORY);
        }
        let sequence = j.sequence;
        // Every buffer and descriptor is allocated and checksummed before the first write.
        let mut descriptors = Vec::new();
        descriptors
            .try_reserve_exact(t.changes.len().div_ceil(TAGS))
            .map_err(|_| NO_MEMORY)?;
        let mut scratch = buffer()?;
        for chunk in t.changes.chunks(TAGS) {
            let mut d = buffer()?;
            header(&mut d[..], 1, sequence);
            let mut at = 12;
            for (i, c) in chunk.iter().enumerate() {
                if c.number >= self.blocks || j.blocks.contains(&c.number) {
                    return Err(INVALID_FILESYSTEM);
                }
                scratch.copy_from_slice(&c.bytes[..]);
                let escape = be(&scratch[..], 0) == MAGIC;
                if escape {
                    set(&mut scratch[..], 0, 0);
                }
                set(&mut d[..], at, c.number);
                set(
                    &mut d[..],
                    at + 4,
                    u32::from(escape)
                        | if i == 0 { 0 } else { 2 }
                        | if i + 1 == chunk.len() { 8 } else { 0 },
                );
                set(
                    &mut d[..],
                    at + 12,
                    crc(crc(j.seed, &sequence.to_be_bytes()), &scratch[..]),
                );
                at += 16;
                if i == 0 {
                    d[at..at + 16].copy_from_slice(&j.superblock[48..64]);
                    at += 16;
                }
            }
            checksum(&mut d[..], BLOCK - 4, j.seed);
            descriptors.push(d);
        }
        let mut commit = buffer()?;
        header(&mut commit[..], 2, sequence);
        checksum(&mut commit[..], 16, j.seed);
        let mut active = buffer()?;
        let mut empty = buffer()?;
        let next = sequence.wrapping_add(1);
        if next == 0 {
            return Err(UNSUPPORTED);
        }
        j.superblock(&mut active, 1, sequence);
        j.superblock(&mut empty, 0, next);
        let j = self.journal.take().unwrap();
        let result = (|| {
            self.write_device(0, &primary)?;
            self.device.flush()?;
            self.write_device(j.blocks[0], &active)?;
            self.device.flush()?;
            let mut position = 1;
            for (d, chunk) in descriptors.iter().zip(t.changes.chunks(TAGS)) {
                self.write_device(j.blocks[position], d)?;
                position += 1;
                for c in chunk {
                    scratch.copy_from_slice(&c.bytes[..]);
                    if be(&scratch[..], 0) == MAGIC {
                        set(&mut scratch[..], 0, 0);
                    }
                    self.write_device(j.blocks[position], &scratch)?;
                    position += 1;
                }
            }
            self.device.flush()?;
            self.write_device(j.blocks[position], &commit)?;
            self.device.flush()?;
            // Only a durable commit permits changes at home addresses, including existing file data.
            for c in &t.changes {
                self.write_device(c.number, &c.bytes)?;
            }
            self.device.flush()?;
            self.write_device(j.blocks[0], &empty)?;
            self.device.flush()
        })();
        self.journal = Some(j);
        self.dirty = true;
        if let Err(e) = result {
            self.failed = true;
            return Err(e);
        }
        let j = self.journal.as_mut().unwrap();
        j.sequence = next;
        j.superblock = empty;
        self.orphans = t.orphans;
        Ok(())
    }

    pub(super) fn recover_journal(&mut self) -> Result<(), i64> {
        let j = self.journal.as_ref().ok_or(INVALID_FILESYSTEM)?;
        let mut bytes = buffer()?;
        let mut commit = None;
        if be(&j.superblock[..], 28) != 0 {
            // Find the commit independently: an incomplete descriptor/data prefix is discardable.
            for position in 1..j.blocks.len().min(MAX_TRANSACTION_BLOCKS + 8) {
                read_block(&mut *self.device, j.blocks[position], &mut bytes)?;
                if be(&bytes[..], 0) == MAGIC
                    && be(&bytes[..], 4) == 2
                    && be(&bytes[..], 8) == j.sequence
                {
                    if commit.is_some() || !verified(&mut bytes[..], 16, j.seed) {
                        return Err(INVALID_FILESYSTEM);
                    }
                    commit = Some(position);
                }
            }
        }
        let mut changes: Vec<Change> = Vec::new();
        let mut revoked = Vec::new();
        if let Some(end) = commit {
            let mut position = 1;
            while position < end {
                read_block(&mut *self.device, j.blocks[position], &mut bytes)?;
                if be(&bytes[..], 0) != MAGIC
                    || be(&bytes[..], 8) != j.sequence
                    || !verified(&mut bytes[..], BLOCK - 4, j.seed)
                {
                    return Err(INVALID_FILESYSTEM);
                }
                position += 1;
                match be(&bytes[..], 4) {
                    1 => {
                        let mut at = 12;
                        let mut first = true;
                        loop {
                            if at + 16 > BLOCK - 4
                                || position >= end
                                || changes.len() == MAX_TRANSACTION_BLOCKS
                            {
                                return Err(INVALID_FILESYSTEM);
                            }
                            let number = be(&bytes[..], at);
                            let flags = be(&bytes[..], at + 4);
                            let sum = be(&bytes[..], at + 12);
                            if flags & !11 != 0
                                || be(&bytes[..], at + 8) != 0
                                || number >= self.blocks
                                || j.blocks.contains(&number)
                                || changes.iter().any(|c| c.number == number)
                                || first && flags & 2 != 0
                            {
                                return Err(INVALID_FILESYSTEM);
                            }
                            at += 16;
                            if flags & 2 == 0 {
                                if at + 16 > BLOCK - 4 || bytes[at..at + 16] != j.superblock[48..64]
                                {
                                    return Err(INVALID_FILESYSTEM);
                                }
                                at += 16;
                            }
                            first = false;
                            let mut data = buffer()?;
                            read_block(&mut *self.device, j.blocks[position], &mut data)?;
                            position += 1;
                            if crc(crc(j.seed, &j.sequence.to_be_bytes()), &data[..]) != sum {
                                return Err(INVALID_FILESYSTEM);
                            }
                            if flags & 1 != 0 {
                                if be(&data[..], 0) != 0 {
                                    return Err(INVALID_FILESYSTEM);
                                }
                                set(&mut data[..], 0, MAGIC);
                            }
                            changes.try_reserve(1).map_err(|_| NO_MEMORY)?;
                            changes.push(Change {
                                number,
                                bytes: data,
                                data: false,
                            });
                            if flags & 8 != 0 {
                                break;
                            }
                        }
                    }
                    5 => {
                        let count = be(&bytes[..], 12) as usize;
                        if !(16..=BLOCK - 4).contains(&count)
                            || !count.is_multiple_of(4)
                            || revoked.len() + (count - 16) / 4 > MAX_TRANSACTION_BLOCKS
                        {
                            return Err(INVALID_FILESYSTEM);
                        }
                        revoked
                            .try_reserve((count - 16) / 4)
                            .map_err(|_| NO_MEMORY)?;
                        for at in (16..count).step_by(4) {
                            let n = be(&bytes[..], at);
                            if n >= self.blocks || j.blocks.contains(&n) {
                                return Err(INVALID_FILESYSTEM);
                            }
                            revoked.push(n);
                        }
                    }
                    _ => return Err(UNSUPPORTED),
                }
            }
        }
        changes.retain(|c| !revoked.contains(&c.number));
        let mut empty = buffer()?;
        let next = if commit.is_some() {
            j.sequence.wrapping_add(1)
        } else {
            j.sequence
        };
        if next == 0 {
            return Err(UNSUPPORTED);
        }
        j.superblock(&mut empty, 0, next);
        // Audit the recovered overlay before publishing any of its home blocks.
        self.transaction = Some(Transaction {
            changes,
            orphans: Vec::new(),
            hint: 0,
        });
        self.bitmap_number = None;
        self.cached = None;
        self.validate_replay_geometry()?;
        self.load_orphans()?;
        self.audit_writable()?;
        let t = self.transaction.take().unwrap();
        self.cached = None;
        self.bitmap_number = None;
        let j = self.journal.take().unwrap();
        for c in &t.changes {
            self.write_device(c.number, &c.bytes)?;
        }
        self.device.flush()?;
        self.write_device(j.blocks[0], &empty)?;
        self.device.flush()?;
        // Clearing recovery is safe only after the replayed image and retired journal are durable.
        read_block(&mut *self.device, 0, &mut bytes)?;
        put32(&mut bytes[..], 1120, 0x42);
        self.write_device(0, &bytes)?;
        self.device.flush()?;
        self.journal = Some(Journal {
            superblock: empty,
            sequence: next,
            ..j
        });
        Ok(())
    }

    fn validate_replay_geometry(&mut self) -> Result<(), i64> {
        // Replay must not replace the immutable geometry/map used to validate its target addresses.
        let mut old = buffer()?;
        let t = self.transaction.as_ref().ok_or(INVALID_FILESYSTEM)?;
        if let Some(c) = t.find(0) {
            read_block(&mut *self.device, 0, &mut old)?;
            for i in 0..BLOCK {
                if !matches!(i, 1036..1044 | 1072..1076 | 1082..1084 | 1120..1124 | 1256..1260)
                    && old[i] != c.bytes[i]
                {
                    return Err(INVALID_FILESYSTEM);
                }
            }
            if u32_at(&c.bytes[..], 1120) != 0x46 || u16_at(&c.bytes[..], 1082) != 1 {
                return Err(INVALID_FILESYSTEM);
            }
        }
        if let Some(c) = t.find(self.groups[0].table) {
            read_block(&mut *self.device, c.number, &mut old)?;
            if old[7 * 256..8 * 256] != c.bytes[7 * 256..8 * 256] {
                return Err(INVALID_FILESYSTEM);
            }
        }
        for i in 0..self.groups.len() {
            let n = 1 + i as u32 * 32 / BLOCK as u32;
            if let Some(c) = t.find(n) {
                read_block(&mut *self.device, n, &mut old)?;
                let at = i * 32 % BLOCK;
                if old[at..at + 12] != c.bytes[at..at + 12]
                    || old[at + 18..at + 28] != c.bytes[at + 18..at + 28]
                    || old[at + 30..at + 32] != c.bytes[at + 30..at + 32]
                {
                    return Err(INVALID_FILESYSTEM);
                }
            }
        }
        Ok(())
    }

    pub(super) fn load_orphans(&mut self) -> Result<(), i64> {
        if self.journal.is_none() {
            return Ok(());
        }
        let mut bytes = buffer()?;
        self.read_block(0, &mut bytes)?;
        let mut n = u32_at(&bytes[..], 1024 + 232);
        self.orphans.clear();
        while n != 0 {
            if !(11..=self.inodes).contains(&n) || self.orphans.contains(&n) {
                return Err(INVALID_FILESYSTEM);
            }
            self.orphans.try_reserve(1).map_err(|_| NO_MEMORY)?;
            self.orphans.push(n);
            let local = (n - 1) % self.inodes_per_group;
            let g = self.groups[((n - 1) / self.inodes_per_group) as usize];
            self.read_block(g.table + local / 16, &mut bytes)?;
            let at = local as usize % 16 * 256;
            if u16_at(&bytes[..], at + 26) != 0 {
                return Err(INVALID_FILESYSTEM);
            }
            n = u32_at(&bytes[..], at + 20);
        }
        if let Some(t) = self.transaction.as_mut() {
            t.orphans.clear();
            t.orphans
                .try_reserve_exact(self.orphans.len())
                .map_err(|_| NO_MEMORY)?;
            t.orphans.extend_from_slice(&self.orphans);
        }
        Ok(())
    }
    pub(super) fn persist_orphans(&mut self) -> Result<(), i64> {
        if self.journal.is_none() || self.transaction.as_ref().unwrap().orphans == self.orphans {
            return Ok(());
        }
        let count = self.transaction.as_ref().unwrap().orphans.len();
        let mut sb = buffer()?;
        self.read_block(0, &mut sb)?;
        put32(
            &mut sb[..],
            1256,
            self.transaction
                .as_ref()
                .unwrap()
                .orphans
                .first()
                .copied()
                .unwrap_or(0),
        );
        self.stage(0, sb, false)?;
        for i in 0..count {
            let t = self.transaction.as_ref().unwrap();
            let n = t.orphans[i];
            let next = t.orphans.get(i + 1).copied().unwrap_or(0);
            let mut inode = self.snapshot(n)?;
            put32(&mut inode.raw, 20, next);
            self.store(&inode)?;
        }
        Ok(())
    }
}
