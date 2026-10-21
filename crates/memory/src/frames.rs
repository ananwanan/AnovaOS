// SPDX-License-Identifier: Apache-2.0
//! Only explicitly supplied usable RAM enters the allocator. All other memory
//! stays reserved, including holes and every bootloader-reclaimable byte.
use crate::{Error, PAGE_SIZE, PHYSICAL_LIMIT};
use anova_platform::PhysAddr;

pub const MAX_REGIONS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysRange {
    pub start: PhysAddr,
    pub length: u64,
}

impl PhysRange {
    pub fn end(self) -> Result<u64, Error> {
        let end = self
            .start
            .0
            .checked_add(self.length)
            .ok_or(Error::Overflow)?;
        if self.length == 0 || end > PHYSICAL_LIMIT {
            return Err(Error::InvalidRange);
        }
        Ok(end)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Owner {
    PageTable = 2,
    Stack = 3,
    Heap = 4,
    Test = 5,
    User = 6,
    ProcessStack = 7,
    Dma = 8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Any,
    Dma32,
}

#[derive(Clone, Copy, Default)]
struct Region {
    start: u64,
    pages: usize,
    index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
    pub total: usize,
    pub reserved: usize,
    pub free: usize,
}

pub struct FrameAllocator<'a> {
    regions: [Region; MAX_REGIONS],
    count: usize,
    states: &'a mut [u8],
    cursor: usize,
    stats: Stats,
}

pub fn metadata_bytes(ranges: &[PhysRange]) -> Result<usize, Error> {
    if ranges.is_empty() || ranges.len() > MAX_REGIONS {
        return Err(Error::Capacity);
    }
    let mut previous_end = 0;
    let mut total = 0_usize;
    for range in ranges {
        let end = range.end()?;
        if (range.start.0 | range.length) & (PAGE_SIZE - 1) != 0 {
            return Err(Error::Alignment);
        }
        if range.start.0 < previous_end {
            return Err(Error::Overlap);
        }
        previous_end = end;
        total = total
            .checked_add(usize::try_from(range.length / PAGE_SIZE).map_err(|_| Error::Capacity)?)
            .ok_or(Error::Overflow)?;
    }
    Ok(total)
}

impl<'a> FrameAllocator<'a> {
    pub fn new(
        ranges: &[PhysRange],
        reservations: &[PhysRange],
        states: &'a mut [u8],
    ) -> Result<Self, Error> {
        let total = metadata_bytes(ranges)?;
        if states.len() < total {
            return Err(Error::Capacity);
        }
        for reservation in reservations {
            reservation.end()?;
        }
        let mut allocator = Self {
            regions: [Region::default(); MAX_REGIONS],
            count: ranges.len(),
            states: &mut states[..total],
            cursor: 0,
            stats: Stats {
                total,
                reserved: 0,
                free: total,
            },
        };
        allocator.states.fill(0);
        let mut index = 0;
        for (slot, range) in ranges.iter().enumerate() {
            let pages = (range.length / PAGE_SIZE) as usize;
            allocator.regions[slot] = Region {
                start: range.start.0,
                pages,
                index,
            };
            for page in 0..pages {
                let physical = range.start.0 + page as u64 * PAGE_SIZE;
                if physical == 0
                    || reservations.iter().any(|r| {
                        physical < r.start.0 + r.length && r.start.0 < physical + PAGE_SIZE
                    })
                {
                    allocator.states[index + page] = 1;
                    allocator.stats.reserved += 1;
                    allocator.stats.free -= 1;
                }
            }
            index += pages;
        }
        Ok(allocator)
    }

    fn index(&self, frame: PhysAddr) -> Result<usize, Error> {
        if frame.0 & (PAGE_SIZE - 1) != 0 {
            return Err(Error::Alignment);
        }
        self.regions[..self.count]
            .iter()
            .find_map(|region| {
                frame
                    .0
                    .checked_sub(region.start)
                    .filter(|offset| *offset / PAGE_SIZE < region.pages as u64)
                    .map(|offset| region.index + (offset / PAGE_SIZE) as usize)
            })
            .ok_or(Error::InvalidAddress)
    }

    pub fn frame_at(&self, index: usize) -> Option<PhysAddr> {
        self.regions[..self.count].iter().find_map(|region| {
            index
                .checked_sub(region.index)
                .filter(|page| *page < region.pages)
                .map(|page| PhysAddr(region.start + page as u64 * PAGE_SIZE))
        })
    }

    pub fn allocate(&mut self, owner: Owner, zone: Zone) -> Result<PhysAddr, Error> {
        for step in 0..self.states.len() {
            let index = (self.cursor + step) % self.states.len();
            if self.states[index] != 0 {
                continue;
            }
            let frame = self.frame_at(index).ok_or(Error::InvalidAddress)?;
            if zone == Zone::Dma32 && frame.0 >= 1 << 32 {
                continue;
            }
            self.states[index] = owner as u8;
            self.cursor = (index + 1) % self.states.len();
            self.stats.free -= 1;
            return Ok(frame);
        }
        Err(Error::OutOfMemory)
    }

    pub fn release(&mut self, frame: PhysAddr, owner: Owner) -> Result<(), Error> {
        let index = self.index(frame)?;
        match self.states[index] {
            0 => return Err(Error::NotAllocated),
            1 => return Err(Error::Reserved),
            state if state != owner as u8 => return Err(Error::WrongOwner),
            _ => {}
        }
        self.states[index] = 0;
        self.stats.free += 1;
        Ok(())
    }

    pub fn owner(&self, frame: PhysAddr) -> Result<Option<Owner>, Error> {
        match self.states[self.index(frame)?] {
            0 => Ok(None),
            1 => Err(Error::Reserved),
            2 => Ok(Some(Owner::PageTable)),
            3 => Ok(Some(Owner::Stack)),
            4 => Ok(Some(Owner::Heap)),
            5 => Ok(Some(Owner::Test)),
            6 => Ok(Some(Owner::User)),
            7 => Ok(Some(Owner::ProcessStack)),
            8 => Ok(Some(Owner::Dma)),
            _ => Err(Error::WrongOwner),
        }
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start: u64, pages: u64) -> PhysRange {
        PhysRange {
            start: PhysAddr(start),
            length: pages * PAGE_SIZE,
        }
    }

    #[test]
    fn exhaustion_reuse_reservations_and_invalid_releases() {
        let mut states = [0; 8];
        let ranges = [range(0, 4), range(0x10000, 4)];
        // A partial-page reservation excludes both intersected frames.
        let reserved = [PhysRange {
            start: PhysAddr(0x10001),
            length: 4096,
        }];
        let mut frames = FrameAllocator::new(&ranges, &reserved, &mut states).unwrap();
        assert_eq!(
            frames.stats(),
            Stats {
                total: 8,
                reserved: 3,
                free: 5
            }
        );
        let mut allocated = [PhysAddr(0); 5];
        for frame in &mut allocated {
            *frame = frames.allocate(Owner::Test, Zone::Any).unwrap();
            assert!(![0, 0x10000, 0x11000].contains(&frame.0));
        }
        assert_eq!(
            frames.allocate(Owner::Test, Zone::Any),
            Err(Error::OutOfMemory)
        );
        for (address, error) in [
            (0, Error::Reserved),
            (1, Error::Alignment),
            (0x9000, Error::InvalidAddress),
        ] {
            assert_eq!(frames.release(PhysAddr(address), Owner::Test), Err(error));
        }
        assert_eq!(
            frames.release(allocated[0], Owner::Heap),
            Err(Error::WrongOwner)
        );
        frames.release(allocated[0], Owner::Test).unwrap();
        assert_eq!(
            frames.release(allocated[0], Owner::Test),
            Err(Error::NotAllocated)
        );
        assert_eq!(
            frames.allocate(Owner::Heap, Zone::Any).unwrap(),
            allocated[0]
        );
    }

    #[test]
    fn validates_bounds_without_touching_metadata_on_failure() {
        let mut states = [77; 8];
        for ranges in [
            [range(4096, 2), range(8192, 2)],
            [range(4097, 1), range(16384, 1)],
            [range(PHYSICAL_LIMIT, 1), range(4096, 1)],
        ] {
            assert!(FrameAllocator::new(&ranges, &[], &mut states).is_err());
            assert_eq!(states, [77; 8]);
        }
        assert!(
            metadata_bytes(&[PhysRange {
                start: PhysAddr(u64::MAX - 10),
                length: 20
            }])
            .is_err()
        );
        assert!(FrameAllocator::new(&[range(4096, 9)], &[], &mut states).is_err());
        assert!(metadata_bytes(&[]).is_err());
    }

    #[test]
    fn dma32_never_returns_high_memory() {
        let mut states = [0; 3];
        let mut frames =
            FrameAllocator::new(&[range((1 << 32) - PAGE_SIZE, 3)], &[], &mut states).unwrap();
        assert_eq!(
            frames.allocate(Owner::Test, Zone::Dma32).unwrap(),
            PhysAddr((1 << 32) - PAGE_SIZE)
        );
        assert_eq!(
            frames.allocate(Owner::Test, Zone::Dma32),
            Err(Error::OutOfMemory)
        );
        assert_eq!(
            frames.allocate(Owner::Test, Zone::Any).unwrap(),
            PhysAddr(1 << 32)
        );
    }
}
