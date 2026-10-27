// SPDX-License-Identifier: Apache-2.0
//! Stage bounded ext4 changes, maintain allocation counters and publish through the selected durability profile.
//! Allocation/read failures discard the overlay; device commit failures poison the mount and retain dirty state.
use super::*;

pub const MAX_TRANSACTION_BLOCKS: usize = 1024;
pub const MAX_DIRECTORY_BLOCKS: usize = 256;

pub(super) struct Change {
    pub number: u32,
    pub bytes: Box<[u8; BLOCK]>,
    pub(super) data: bool,
}
pub(super) struct Transaction {
    pub(super) changes: Vec<Change>,
    pub(super) orphans: Vec<u32>,
    pub(super) hint: u32,
}
impl Transaction {
    pub fn find(&self, n: u32) -> Option<&Change> {
        self.changes.iter().find(|c| c.number == n)
    }
}

impl Ext4 {
    /// Explicit opt-in. The ordinary mount constructor never enables disk mutations.
    pub fn mount_writable(device: Box<dyn BlockDevice>) -> Result<Self, i64> {
        if device.read_only() {
            return Err(READ_ONLY);
        }
        let mut fs = Self::mount_inner(device, true)?;
        fs.audit_writable()?;
        fs.cached = None;
        fs.bitmap_number = None;
        fs.writable = true;
        while let Some(n) = fs.orphans.last().copied() {
            fs.mutate(|s| s.release_inner(n))?;
        }
        Ok(fs)
    }
    pub(super) fn is_orphan(&self, n: u32) -> bool {
        self.transaction
            .as_ref()
            .map_or(&self.orphans, |t| &t.orphans)
            .contains(&n)
    }
    pub(super) fn mutate<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<T, i64>,
    ) -> Result<T, i64> {
        if !self.writable {
            return Err(READ_ONLY);
        }
        if self.failed {
            return Err(IO_ERROR);
        }
        let mut orphans = Vec::new();
        orphans
            .try_reserve_exact(self.orphans.len())
            .map_err(|_| NO_MEMORY)?;
        orphans.extend_from_slice(&self.orphans);
        self.transaction = Some(Transaction {
            changes: Vec::new(),
            orphans,
            hint: 0,
        });
        self.cached = None;
        let result = operation(self).and_then(|value| {
            self.persist_orphans()?;
            Ok(value)
        });
        self.cached = None;
        self.bitmap_number = None;
        let transaction = self.transaction.take().unwrap();
        match result {
            Ok(value) => {
                let counters = transaction
                    .find(0)
                    .map(|c| (u32_at(&c.bytes[..], 1036), u32_at(&c.bytes[..], 1040)));
                self.commit(transaction)?;
                if let Some((free_blocks, free_inodes)) = counters {
                    self.record_usage(free_blocks, free_inodes)?;
                }
                Ok(value)
            }
            Err(error) => Err(error),
        }
    }
    pub(super) fn stage(&mut self, n: u32, bytes: Box<[u8; BLOCK]>, data: bool) -> Result<(), i64> {
        if n >= self.blocks {
            return Err(INVALID_FILESYSTEM);
        }
        let t = self.transaction.as_mut().ok_or(IO_ERROR)?;
        if let Some(c) = t.changes.iter_mut().find(|c| c.number == n) {
            c.bytes = bytes;
            c.data = data;
        } else {
            if t.changes.len() == MAX_TRANSACTION_BLOCKS {
                return Err(NO_MEMORY);
            }
            t.changes.try_reserve(1).map_err(|_| NO_MEMORY)?;
            t.changes.push(Change {
                number: n,
                bytes,
                data,
            });
        }
        self.bitmap_number = None;
        self.cached = None;
        Ok(())
    }
    pub(super) fn write_device(&mut self, n: u32, bytes: &[u8; BLOCK]) -> Result<(), i64> {
        let sector = u64::from(n) * 8;
        check_range(self.device.sectors(), sector, BLOCK)?;
        for (i, input) in bytes.chunks_exact(SECTOR).enumerate() {
            self.device
                .write_sector(sector + i as u64, input.try_into().unwrap())?;
        }
        Ok(())
    }
    fn commit(&mut self, mut t: Transaction) -> Result<(), i64> {
        if t.changes.is_empty() {
            return Ok(());
        }
        if self.journal.is_some() {
            return self.commit_journal(t);
        }
        // Allocate/read everything still needed before the very first device mutation.
        let mut primary = buffer()?;
        self.read_block(0, &mut primary)?;
        put16(&mut primary[..], 1024 + 58, 0);
        if let Some(c) = t.changes.iter_mut().find(|c| c.number == 0) {
            put16(&mut c.bytes[..], 1024 + 58, 0);
        }
        let result = (|| {
            if !self.dirty {
                // The dirty marker is durable before any data, bitmap or inode can change.
                self.write_device(0, &primary)?;
                self.device.flush()?;
            }
            for c in t.changes.iter().filter(|c| c.data) {
                self.write_device(c.number, &c.bytes)?;
            }
            self.device.flush()?;
            for c in t.changes.iter().filter(|c| !c.data && c.number != 0) {
                self.write_device(c.number, &c.bytes)?;
            }
            if let Some(c) = t.find(0) {
                self.write_device(0, &c.bytes)?;
            }
            self.device.flush()
        })();
        if let Err(error) = result {
            self.failed = true;
            self.dirty = true;
            return Err(error);
        }
        self.dirty = true;
        self.orphans = t.orphans;
        Ok(())
    }
    pub(super) fn flush_writable(&mut self) -> Result<(), i64> {
        if self.failed {
            return Err(IO_ERROR);
        }
        if !self.writable {
            return Ok(());
        }
        let result = self.device.flush();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    pub(super) fn shutdown_writable(&mut self) -> Result<(), i64> {
        if !self.orphans.is_empty() {
            return Err(BUSY);
        }
        self.flush_writable()?;
        if !self.dirty {
            return Ok(());
        }
        let mut primary = buffer()?;
        self.read_block(0, &mut primary)?;
        put16(&mut primary[..], 1024 + 58, 1);
        if self.journal.is_some() {
            put32(&mut primary[..], 1024 + 96, 0x42);
        }
        put32(&mut primary[..], 1024 + 48, self.clock);
        let result = self
            .write_device(0, &primary)
            .and_then(|_| self.device.flush());
        if let Err(error) = result {
            // A failed clean-marker write/flush must not be reported as a clean shutdown.
            // Best effort only: an unavailable device cannot promise persisted dirty bytes.
            if self.journal.is_some() {
                put32(&mut primary[..], 1024 + 96, 0x46);
            } else {
                put16(&mut primary[..], 1024 + 58, 0);
            }
            let _ = self.write_device(0, &primary);
            let _ = self.device.flush();
            self.failed = true;
            return Err(error);
        }
        self.dirty = false;
        Ok(())
    }
    pub(super) fn snapshot(&mut self, n: u32) -> Result<Cached, i64> {
        self.load(n)?;
        Ok(self.cached.take().unwrap())
    }
    pub(super) fn store(&mut self, inode: &Cached) -> Result<(), i64> {
        let local = (inode.info.number - 1) % self.inodes_per_group;
        let group = self.groups[((inode.info.number - 1) / self.inodes_per_group) as usize];
        let n = group.table + local / (BLOCK / 256) as u32;
        let mut bytes = buffer()?;
        self.read_block(n, &mut bytes)?;
        let at = local as usize * 256 % BLOCK;
        bytes[at..at + 256].copy_from_slice(&inode.raw);
        self.stage(n, bytes, false)
    }
    pub(super) fn touch(&self, inode: &mut Cached, contents: bool) {
        put32(&mut inode.raw, 12, self.clock);
        if contents {
            put32(&mut inode.raw, 16, self.clock);
        }
    }
    pub(super) fn counters(
        &mut self,
        index: usize,
        blocks: i32,
        inodes: i32,
        dirs: i32,
    ) -> Result<(), i64> {
        fn add(value: u32, change: i32, limit: u32) -> Result<u32, i64> {
            let result = i64::from(value) + i64::from(change);
            if !(0..=i64::from(limit)).contains(&result) {
                return Err(INVALID_FILESYSTEM);
            }
            Ok(result as u32)
        }
        let mut sb = buffer()?;
        self.read_block(0, &mut sb)?;
        let b = add(u32_at(&sb[..], 1036), blocks, self.blocks)?;
        let i = add(u32_at(&sb[..], 1040), inodes, self.inodes)?;
        put32(&mut sb[..], 1036, b);
        put32(&mut sb[..], 1040, i);
        put32(&mut sb[..], 1072, self.clock);
        self.stage(0, sb, false)?;
        let address = 1 + index as u32 * 32 / BLOCK as u32;
        let at = index * 32 % BLOCK;
        let mut desc = buffer()?;
        self.read_block(address, &mut desc)?;
        for (offset, delta, limit) in [
            (12, blocks, self.blocks_per_group),
            (14, inodes, self.inodes_per_group),
            (16, dirs, self.inodes_per_group),
        ] {
            let value = add(u32::from(u16_at(&desc[..], at + offset)), delta, limit)?;
            put16(&mut desc[..], at + offset, value as u16);
        }
        // All inode-table bytes were initialized at image creation; no lazy initialization is admitted.
        put16(&mut desc[..], at + 28, 0);
        self.stage(address, desc, false)
    }
    pub(super) fn allocate_block(&mut self) -> Result<u32, i64> {
        let hint = self.transaction.as_ref().ok_or(IO_ERROR)?.hint;
        let first_group = hint / self.blocks_per_group;
        for pass in 0..=self.groups.len() {
            let index = (first_group as usize + pass) % self.groups.len();
            let g = self.groups[index];
            let mut bits = buffer()?;
            self.read_block(g.blocks, &mut bits)?;
            let start = if pass == 0 {
                hint.max(g.prefix)
            } else {
                g.prefix
            };
            for n in start..g.end {
                let bit = n - g.start;
                if bits[bit as usize / 8] & (1 << (bit % 8)) != 0 {
                    continue;
                }
                if n == g.blocks || n == g.inodes || (g.table..g.table_end).contains(&n) {
                    return Err(INVALID_FILESYSTEM);
                }
                bits[bit as usize / 8] |= 1 << (bit % 8);
                self.stage(g.blocks, bits, false)?;
                self.counters(index, -1, 0, 0)?;
                self.stage(n, buffer()?, true)?;
                self.transaction.as_mut().unwrap().hint = n + 1;
                return Ok(n);
            }
        }
        Err(NO_SPACE)
    }
    pub(super) fn free_block(&mut self, n: u32) -> Result<(), i64> {
        self.check_data(n)?;
        let index = (n / self.blocks_per_group) as usize;
        let g = self.groups[index];
        let bit = n - g.start;
        let mut bits = buffer()?;
        self.read_block(g.blocks, &mut bits)?;
        bits[bit as usize / 8] &= !(1 << (bit % 8));
        self.stage(g.blocks, bits, false)?;
        self.counters(index, 1, 0, 0)
    }
    pub(super) fn allocate_inode(&mut self, directory: bool) -> Result<Cached, i64> {
        for index in 0..self.groups.len() {
            let g = self.groups[index];
            let mut bits = buffer()?;
            self.read_block(g.inodes, &mut bits)?;
            for bit in 0..self.inodes_per_group {
                let n = index as u32 * self.inodes_per_group + bit + 1;
                if n < 11 || bits[bit as usize / 8] & (1 << (bit % 8)) != 0 {
                    continue;
                }
                bits[bit as usize / 8] |= 1 << (bit % 8);
                self.stage(g.inodes, bits, false)?;
                self.counters(index, 0, -1, i32::from(directory))?;
                let permissions = if directory { 0o755 } else { 0o644 };
                let mut raw = [0; 256];
                put16(
                    &mut raw,
                    0,
                    if directory { 0x4000 } else { 0x8000 } | permissions,
                );
                put16(&mut raw, 26, if directory { 2 } else { 1 });
                for at in [8, 12, 16, 144] {
                    put32(&mut raw, at, self.clock);
                }
                put32(&mut raw, 32, 0x80000);
                put16(&mut raw, 40, 0xf30a);
                put16(&mut raw, 44, 4);
                put16(&mut raw, 128, 32);
                return Ok(Cached {
                    info: Metadata {
                        number: n,
                        directory,
                        size: 0,
                        executable: false,
                        permissions,
                        modified: self.clock,
                        accessed: self.clock,
                        changed: self.clock,
                    },
                    raw,
                    extents: Vec::new(),
                    tree: Vec::new(),
                });
            }
        }
        Err(NO_SPACE)
    }
    pub(super) fn normalize(extents: &mut Vec<Extent>) -> Result<(), i64> {
        extents.sort_unstable_by_key(|e| e.logical);
        let mut count = 0;
        for i in 0..extents.len() {
            let e = extents[i];
            if count != 0 {
                let previous = &mut extents[count - 1];
                if previous.logical + previous.length > e.logical {
                    return Err(INVALID_FILESYSTEM);
                }
                let limit = if e.unwritten { 32767 } else { 32768 };
                if previous.logical + previous.length == e.logical
                    && previous.physical + previous.length == e.physical
                    && previous.unwritten == e.unwritten
                    && previous.length + e.length <= limit
                {
                    previous.length += e.length;
                    continue;
                }
            }
            extents[count] = e;
            count += 1;
        }
        extents.truncate(count);
        if count > MAX_EXTENTS {
            return Err(NO_SPACE);
        }
        Ok(())
    }
    pub(super) fn mapping(inode: &Cached, logical: u32) -> Option<Extent> {
        inode
            .extents
            .iter()
            .find(|e| (e.logical..e.logical + e.length).contains(&logical))
            .copied()
    }
    pub(super) fn initialized_mapping(
        inode: &mut Cached,
        logical: u32,
        physical: u32,
    ) -> Result<(), i64> {
        inode.extents.try_reserve(3).map_err(|_| NO_MEMORY)?;
        if let Some(i) = inode
            .extents
            .iter()
            .position(|e| (e.logical..e.logical + e.length).contains(&logical))
        {
            let old = inode.extents.remove(i);
            if old.logical < logical {
                inode.extents.push(Extent {
                    length: logical - old.logical,
                    ..old
                });
            }
            if logical + 1 < old.logical + old.length {
                inode.extents.push(Extent {
                    logical: logical + 1,
                    physical: old.physical + logical + 1 - old.logical,
                    length: old.logical + old.length - logical - 1,
                    ..old
                });
            }
        }
        inode.extents.push(Extent {
            logical,
            physical,
            length: 1,
            unwritten: false,
        });
        Self::normalize(&mut inode.extents)
    }
    pub(super) fn rebuild(&mut self, inode: &mut Cached) -> Result<(), i64> {
        Self::normalize(&mut inode.extents)?;
        for n in inode.tree.drain(..) {
            self.free_block(n)?;
        }
        let leaves = if inode.extents.len() <= 4 {
            0
        } else {
            inode.extents.len().div_ceil((BLOCK - 12) / 12)
        };
        inode
            .tree
            .try_reserve_exact(leaves)
            .map_err(|_| NO_MEMORY)?;
        inode.raw[40..100].fill(0);
        put16(&mut inode.raw, 40, 0xf30a);
        put16(
            &mut inode.raw,
            42,
            if leaves == 0 {
                inode.extents.len()
            } else {
                leaves
            } as u16,
        );
        put16(&mut inode.raw, 44, 4);
        put16(&mut inode.raw, 46, u16::from(leaves != 0));
        fn encode(bytes: &mut [u8], at: usize, e: &Extent) {
            put32(bytes, at, e.logical);
            put16(
                bytes,
                at + 4,
                (e.length + if e.unwritten { 32768 } else { 0 }) as u16,
            );
            put32(bytes, at + 8, e.physical);
        }
        if leaves == 0 {
            for (i, e) in inode.extents.iter().enumerate() {
                encode(&mut inode.raw, 52 + i * 12, e);
            }
        } else {
            for (i, extents) in inode.extents.chunks((BLOCK - 12) / 12).enumerate() {
                let n = self.allocate_block()?;
                let mut bytes = buffer()?;
                put16(&mut bytes[..], 0, 0xf30a);
                put16(&mut bytes[..], 2, extents.len() as u16);
                put16(&mut bytes[..], 4, ((BLOCK - 12) / 12) as u16);
                for (j, e) in extents.iter().enumerate() {
                    encode(&mut bytes[..], 12 + j * 12, e);
                }
                self.stage(n, bytes, false)?;
                put32(&mut inode.raw, 52 + i * 12, extents[0].logical);
                put32(&mut inode.raw, 56 + i * 12, n);
                inode.tree.push(n);
            }
        }
        let blocks: u32 =
            inode.extents.iter().map(|e| e.length).sum::<u32>() + inode.tree.len() as u32;
        put32(&mut inode.raw, 28, blocks * 8);
        put32(&mut inode.raw, 4, inode.info.size as u32);
        Ok(())
    }
    pub(super) fn write_inner(
        &mut self,
        n: u32,
        offset: usize,
        input: &[u8],
    ) -> Result<usize, i64> {
        let mut inode = self.snapshot(n)?;
        if inode.info.directory {
            return Err(IS_DIRECTORY);
        }
        if input.is_empty() {
            return Ok(0);
        }
        let end = offset
            .checked_add(input.len())
            .filter(|e| *e <= MAX_FILE)
            .ok_or(NO_SPACE)?;
        let mut done = 0;
        let mut remap = false;
        if end > inode.info.size {
            self.zero_tail(&inode, inode.info.size)?;
        }
        while done < input.len() {
            let position = offset + done;
            let logical = (position / BLOCK) as u32;
            let take = (BLOCK - position % BLOCK).min(input.len() - done);
            let old = Self::mapping(&inode, logical);
            let physical = match old {
                Some(e) => e.physical + logical - e.logical,
                None => self.allocate_block()?,
            };
            let mut bytes = buffer()?;
            if old.is_some_and(|e| !e.unwritten) {
                self.read_block(physical, &mut bytes)?;
            }
            // Existing last-block padding is not assumed to be zero on an imported image.
            if position > inode.info.size && inode.info.size / BLOCK == position / BLOCK {
                bytes[inode.info.size % BLOCK..position % BLOCK].fill(0);
            }
            bytes[position % BLOCK..position % BLOCK + take]
                .copy_from_slice(&input[done..done + take]);
            self.stage(physical, bytes, true)?;
            if old.is_none_or(|e| e.unwritten) {
                Self::initialized_mapping(&mut inode, logical, physical)?;
                remap = true;
            }
            done += take;
        }
        if end > inode.info.size {
            // If the write starts in a later block, zero the old EOF tail before exposing the hole.
            inode.info.size = end;
        }
        if remap {
            self.rebuild(&mut inode)?;
        } else {
            put32(&mut inode.raw, 4, inode.info.size as u32);
        }
        self.touch(&mut inode, true);
        self.store(&inode)?;
        Ok(input.len())
    }
    fn zero_tail(&mut self, inode: &Cached, length: usize) -> Result<(), i64> {
        if length.is_multiple_of(BLOCK) {
            return Ok(());
        }
        let logical = (length / BLOCK) as u32;
        if let Some(e) = Self::mapping(inode, logical).filter(|e| !e.unwritten) {
            let physical = e.physical + logical - e.logical;
            let mut bytes = buffer()?;
            self.read_block(physical, &mut bytes)?;
            bytes[length % BLOCK..].fill(0);
            self.stage(physical, bytes, true)?;
        }
        Ok(())
    }
    pub(super) fn resize_inner(&mut self, n: u32, length: usize) -> Result<(), i64> {
        let mut inode = self.snapshot(n)?;
        if inode.info.directory {
            return Err(IS_DIRECTORY);
        }
        if length > MAX_FILE {
            return Err(NO_SPACE);
        }
        if length == inode.info.size {
            return Ok(());
        }
        self.zero_tail(&inode, length.min(inode.info.size))?;
        if length < inode.info.size {
            let keep = length.div_ceil(BLOCK) as u32;
            for e in &mut inode.extents {
                let retained = keep.saturating_sub(e.logical).min(e.length);
                for block in e.physical + retained..e.physical + e.length {
                    self.free_block(block)?;
                }
                e.length = retained;
            }
            inode.extents.retain(|e| e.length != 0);
            inode.info.size = length;
            self.rebuild(&mut inode)?;
        } else {
            inode.info.size = length;
            put32(&mut inode.raw, 4, length as u32);
        }
        self.touch(&mut inode, true);
        self.store(&inode)
    }
    pub(super) fn executable_inner(&mut self, n: u32) -> Result<(), i64> {
        let inode = self.snapshot(n)?;
        if inode.info.directory {
            return Err(IS_DIRECTORY);
        }
        self.permissions_inner(n, inode.info.permissions | 0o111)
    }
    pub(super) fn permissions_inner(&mut self, n: u32, permissions: u16) -> Result<(), i64> {
        if permissions & !0o777 != 0 {
            return Err(UNSUPPORTED);
        }
        let mut inode = self.snapshot(n)?;
        let mode = u16_at(&inode.raw, 0) & !0o777;
        put16(&mut inode.raw, 0, mode | permissions);
        self.touch(&mut inode, false);
        self.store(&inode)
    }
    pub(super) fn times_inner(
        &mut self,
        n: u32,
        accessed: Option<u32>,
        modified: Option<u32>,
    ) -> Result<(), i64> {
        let mut inode = self.snapshot(n)?;
        if let Some(time) = accessed {
            put32(&mut inode.raw, 8, time);
        }
        if let Some(time) = modified {
            put32(&mut inode.raw, 16, time);
        }
        self.touch(&mut inode, false);
        self.store(&inode)
    }
    pub(super) fn orphan(&mut self, n: u32) -> Result<(), i64> {
        let mut inode = self.snapshot(n)?;
        let orphans = &mut self.transaction.as_mut().unwrap().orphans;
        if orphans.contains(&n) {
            return Err(INVALID_FILESYSTEM);
        }
        orphans.try_reserve(1).map_err(|_| NO_MEMORY)?;
        orphans.push(n);
        put16(&mut inode.raw, 26, 0);
        self.touch(&mut inode, false);
        self.store(&inode)
    }
    pub(super) fn release_inner(&mut self, n: u32) -> Result<(), i64> {
        if !self.is_orphan(n) {
            return Err(INVALID_ARGUMENT);
        }
        let mut inode = self.snapshot(n)?;
        for e in &inode.extents {
            for block in e.physical..e.physical + e.length {
                self.free_block(block)?;
            }
        }
        for block in &inode.tree {
            self.free_block(*block)?;
        }
        let index = ((n - 1) / self.inodes_per_group) as usize;
        let bit = (n - 1) % self.inodes_per_group;
        let g = self.groups[index];
        let mut bits = buffer()?;
        self.read_block(g.inodes, &mut bits)?;
        if bits[bit as usize / 8] & (1 << (bit % 8)) == 0 {
            return Err(INVALID_FILESYSTEM);
        }
        bits[bit as usize / 8] &= !(1 << (bit % 8));
        self.stage(g.inodes, bits, false)?;
        self.counters(index, 0, 1, -i32::from(inode.info.directory))?;
        inode.raw.fill(0);
        put32(&mut inode.raw, 20, self.clock);
        self.store(&inode)?;
        self.transaction
            .as_mut()
            .unwrap()
            .orphans
            .retain(|old| *old != n);
        Ok(())
    }
}
