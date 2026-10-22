// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Validate bounded native ET_EXEC and Linux ET_EXEC/ET_DYN images before allocation.
//! Linux interpreters and TLS are checked here; user-space loaders perform dynamic relocations.

pub mod stack;
use anova_abi::USER_MIN;

pub const PAGE: u64 = 4096;
pub const IMAGE_LIMIT: u64 = 0x4000_0000;
pub const MAX_SEGMENTS: usize = 16;
pub const MAX_MEMORY: u64 = 4 * 1024 * 1024;
pub const LINUX_MAX_MEMORY: u64 = 128 * 1024 * 1024;
pub const LINUX_MAX_FILE: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Header,
    Bounds,
    Unsupported,
    Permissions,
    Address,
    Overlap,
    Capacity,
    Entry,
    Arguments,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Segment {
    pub address: u64,
    pub file_offset: usize,
    pub file_size: usize,
    pub memory_size: u64,
    pub writable: bool,
    pub executable: bool,
}

impl Segment {
    pub fn page_start(self) -> u64 {
        self.address & !(PAGE - 1)
    }
    pub fn page_end(self) -> u64 {
        (self.address + self.memory_size + PAGE - 1) & !(PAGE - 1)
    }
}

#[derive(Debug)]
pub struct Image {
    pub entry: u64,
    pub phdr: u64,
    pub phnum: u64,
    segments: [Segment; MAX_SEGMENTS],
    count: usize,
}

impl Image {
    pub fn segments(&self) -> &[Segment] {
        &self.segments[..self.count]
    }
}

fn integer(bytes: &[u8], offset: usize, length: usize) -> Result<u64, Error> {
    let field = bytes
        .get(offset..offset.checked_add(length).ok_or(Error::Bounds)?)
        .ok_or(Error::Bounds)?;
    let mut value = 0;
    for (i, byte) in field.iter().enumerate() {
        value |= u64::from(*byte) << (i * 8);
    }
    Ok(value)
}

pub fn parse(bytes: &[u8]) -> Result<Image, Error> {
    parse_personality(bytes, false)
}

pub fn parse_linux(bytes: &[u8]) -> Result<Image, Error> {
    parse_linux_at(bytes, 0x1000_0000)
}
pub fn parse_linux_at(bytes: &[u8], bias: u64) -> Result<Image, Error> {
    let mut image = parse_personality(bytes, true)?;
    if integer(bytes, 16, 2)? == 3 {
        if bias & 4095 != 0 || bias < USER_MIN {
            return Err(Error::Address);
        }
        image.entry = image.entry.checked_add(bias).ok_or(Error::Address)?;
        image.phdr = image.phdr.checked_add(bias).ok_or(Error::Address)?;
        for segment in &mut image.segments[..image.count] {
            segment.address = segment.address.checked_add(bias).ok_or(Error::Address)?;
            if segment
                .address
                .checked_add(segment.memory_size)
                .is_none_or(|end| end > IMAGE_LIMIT)
            {
                return Err(Error::Address);
            }
        }
    }
    Ok(image)
}

/// Return only a bounded, absolute, NUL-terminated interpreter path from a validated ELF.
pub fn interpreter(bytes: &[u8]) -> Result<Option<&str>, Error> {
    if bytes.len() < 64
        || bytes[..7] != *b"\x7fELF\x02\x01\x01"
        || integer(bytes, 18, 2)? != 62
        || integer(bytes, 54, 2)? != 56
    {
        return Err(Error::Header);
    }
    let offset = usize::try_from(integer(bytes, 32, 8)?).map_err(|_| Error::Bounds)?;
    let count = integer(bytes, 56, 2)? as usize;
    if count == 0
        || count > 32
        || offset < 64
        || offset
            .checked_add(count * 56)
            .is_none_or(|end| end > bytes.len())
    {
        return Err(Error::Bounds);
    }
    let mut found = None;
    for header in bytes[offset..offset + count * 56].chunks_exact(56) {
        if integer(header, 0, 4)? != 3 {
            continue;
        }
        if found.is_some() {
            return Err(Error::Unsupported);
        }
        let start = integer(header, 8, 8)? as usize;
        let size = integer(header, 32, 8)? as usize;
        let data = bytes
            .get(start..start.checked_add(size).ok_or(Error::Bounds)?)
            .ok_or(Error::Bounds)?;
        if !(2..=128).contains(&size)
            || data[0] != b'/'
            || data[size - 1] != 0
            || data[..size - 1].contains(&0)
        {
            return Err(Error::Bounds);
        }
        found = Some(core::str::from_utf8(&data[..size - 1]).map_err(|_| Error::Header)?);
    }
    Ok(found)
}

fn parse_personality(bytes: &[u8], linux: bool) -> Result<Image, Error> {
    let file_limit = if linux {
        LINUX_MAX_FILE
    } else {
        8 * 1024 * 1024
    };
    if bytes.len() < 64 || bytes.len() > file_limit || bytes[..7] != *b"\x7fELF\x02\x01\x01" {
        return Err(Error::Header);
    }
    let at = |offset, length| integer(bytes, offset, length);
    let dynamic = linux && at(16, 2)? == 3;
    if (!dynamic && at(16, 2)? != 2)
        || at(18, 2)? != 62
        || at(20, 4)? != 1
        || at(48, 4)? != 0
        || at(52, 2)? != 64
        || at(54, 2)? != 56
    {
        return Err(Error::Unsupported);
    }
    let offset = usize::try_from(at(32, 8)?).map_err(|_| Error::Bounds)?;
    let count = at(56, 2)? as usize;
    if count == 0
        || count > 32
        || offset < 64
        || offset
            .checked_add(count * 56)
            .is_none_or(|end| end > bytes.len())
    {
        return Err(Error::Bounds);
    }
    let mut image = Image {
        entry: at(24, 8)?,
        phdr: 0,
        phnum: count as u64,
        segments: [Segment::default(); MAX_SEGMENTS],
        count: 0,
    };
    let mut memory = 0_u64;
    let mut tls = None;
    for header in bytes[offset..offset + count * 56].chunks_exact(56) {
        let p = |offset, length| integer(header, offset, length);
        let (kind, flags) = (p(0, 4)?, p(4, 4)?);
        let (file, address, filesz, memsz, align) =
            (p(8, 8)?, p(16, 8)?, p(32, 8)?, p(40, 8)?, p(48, 8)?);
        if file
            .checked_add(filesz)
            .is_none_or(|end| end > bytes.len() as u64)
        {
            return Err(Error::Bounds);
        }
        match kind {
            0 | 4 | 6 => continue, // NULL, NOTE and PHDR metadata need no runtime action.
            2 | 3 | 0x6474_e550 | 0x6474_e553 if linux => continue,
            0x6474_e551 if flags & 1 == 0 => continue, // Non-executable GNU_STACK.
            7 if linux => {
                if tls.is_some() || filesz > memsz || (align > 1 && !align.is_power_of_two()) {
                    return Err(Error::Bounds);
                }
                tls = Some((file, address, filesz, memsz));
                continue;
            }
            0x6474_e552 if linux => continue, // Static libc handles its own relocation; RELRO protection is not yet enforced.
            1 => {}
            _ => return Err(Error::Unsupported),
        }
        if filesz > memsz {
            return Err(Error::Bounds);
        }
        if flags & !7 != 0 || flags & 4 == 0 || flags & 3 == 3 {
            return Err(Error::Permissions);
        }
        if align > 1 && (!align.is_power_of_two() || address % align != file % align)
            || address % PAGE != file % PAGE
        {
            return Err(Error::Address);
        }
        if memsz == 0 {
            continue;
        }
        let end = address.checked_add(memsz).ok_or(Error::Address)?;
        if (!dynamic && address < USER_MIN) || end > IMAGE_LIMIT {
            return Err(Error::Address);
        }
        if file <= offset as u64 && file + filesz >= (offset + count * 56) as u64 {
            image.phdr = address
                .checked_add(offset as u64 - file)
                .ok_or(Error::Address)?;
        }
        let segment = Segment {
            address,
            file_offset: file as usize,
            file_size: filesz as usize,
            memory_size: memsz,
            writable: flags & 2 != 0,
            executable: flags & 1 != 0,
        };
        if image.segments().iter().any(|other| {
            segment.page_start() < other.page_end() && other.page_start() < segment.page_end()
        }) {
            return Err(Error::Overlap);
        }
        if image
            .segments()
            .last()
            .is_some_and(|last| last.address > address)
        {
            return Err(Error::Address);
        }
        memory = memory
            .checked_add(segment.page_end() - segment.page_start())
            .ok_or(Error::Capacity)?;
        let memory_limit = if linux { LINUX_MAX_MEMORY } else { MAX_MEMORY };
        if image.count == MAX_SEGMENTS || memory > memory_limit {
            return Err(Error::Capacity);
        }
        image.segments[image.count] = segment;
        image.count += 1;
    }
    if linux && image.phdr == 0 {
        return Err(Error::Unsupported);
    }
    if let Some((file, address, filesz, memsz)) = tls {
        let end = address.checked_add(memsz).ok_or(Error::Address)?;
        if !image.segments().iter().any(|s| {
            address >= s.address
                && end <= s.address + s.memory_size
                && file >= s.file_offset as u64
                && file + filesz <= (s.file_offset + s.file_size) as u64
                && address - s.address == file - s.file_offset as u64
        }) {
            return Err(Error::Bounds);
        }
    }
    if !image.segments().iter().any(|segment| {
        segment.executable
            && image.entry >= segment.address
            && image.entry < segment.address + segment.file_size as u64
    }) {
        return Err(Error::Entry);
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::{vec, vec::Vec};
    fn put(bytes: &mut [u8], offset: usize, value: u64, count: usize) {
        bytes[offset..offset + count].copy_from_slice(&value.to_le_bytes()[..count]);
    }
    fn valid() -> Vec<u8> {
        let mut bytes = vec![0; 8192];
        bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        for (offset, value, count) in [
            (16, 2, 2),
            (18, 62, 2),
            (20, 1, 4),
            (24, 0x400000, 8),
            (32, 64, 8),
            (52, 64, 2),
            (54, 56, 2),
            (56, 1, 2),
            (64, 1, 4),
            (68, 5, 4),
            (72, 4096, 8),
            (80, 0x400000, 8),
            (96, 16, 8),
            (104, 4096, 8),
            (112, 4096, 8),
        ] {
            put(&mut bytes, offset, value, count);
        }
        bytes
    }
    #[test]
    fn linux_pie_bias_and_interpreter_bounds_are_checked_before_loading() {
        let mut bytes = valid();
        put(&mut bytes, 16, 3, 2);
        put(&mut bytes, 24, 0x1000, 8);
        put(&mut bytes, 72, 0, 8);
        put(&mut bytes, 80, 0, 8);
        put(&mut bytes, 96, 8192, 8);
        put(&mut bytes, 104, 8192, 8);
        let image = parse_linux_at(&bytes, 0x20000000).unwrap();
        assert_eq!(image.entry, 0x20001000);
        assert_eq!(image.phdr, 0x20000040);
        assert_eq!(
            parse_linux_at(&bytes, u64::MAX).unwrap_err(),
            Error::Address
        );
        put(&mut bytes, 56, 2, 2);
        put(&mut bytes, 120, 3, 4);
        put(&mut bytes, 128, 512, 8);
        put(&mut bytes, 152, 11, 8);
        bytes[512..523].copy_from_slice(b"/lib/ld.so\0");
        assert_eq!(interpreter(&bytes), Ok(Some("/lib/ld.so")));
        bytes[522] = b'x';
        assert_eq!(interpreter(&bytes), Err(Error::Bounds));
        bytes[522] = 0;
        bytes[512] = b'x';
        assert_eq!(interpreter(&bytes), Err(Error::Bounds));
    }
    #[test]
    fn accepts_file_backed_entry_and_zero_fill_extent() {
        let image = parse(&valid()).unwrap();
        assert_eq!(image.segments()[0].file_size, 16);
        assert_eq!(image.segments()[0].memory_size, 4096);
    }
    #[test]
    fn rejects_truncation_dynamic_tls_permissions_and_bad_entry() {
        let original = valid();
        for length in [0, 6, 63, 65, 4096] {
            assert!(parse(&original[..length]).is_err());
        }
        for (offset, value, count, error) in [
            (16, 3, 2, Error::Unsupported),
            (18, 3, 2, Error::Unsupported),
            (64, 2, 4, Error::Unsupported),
            (64, 3, 4, Error::Unsupported),
            (64, 7, 4, Error::Unsupported),
            (68, 7, 4, Error::Permissions),
            (68, 1, 4, Error::Permissions),
            (96, 4097, 8, Error::Bounds),
            (72, u64::MAX, 8, Error::Bounds),
            (80, u64::MAX - 1, 8, Error::Address),
            (80, 0xffff800000000000, 8, Error::Address),
            (112, 3, 8, Error::Address),
            (24, 0x400010, 8, Error::Entry),
            (32, u64::MAX, 8, Error::Bounds),
            (56, 33, 2, Error::Bounds),
            (104, MAX_MEMORY + 1, 8, Error::Capacity),
        ] {
            let mut bytes = original.clone();
            put(&mut bytes, offset, value, count);
            assert_eq!(
                parse(&bytes).unwrap_err(),
                error,
                "offset {offset} value {value}"
            );
        }
    }
    #[test]
    fn linux_requires_mapped_headers_and_valid_static_tls_but_native_rejects_tls() {
        let mut bytes = valid();
        assert_eq!(parse_linux(&bytes).unwrap_err(), Error::Unsupported);
        put(&mut bytes, 72, 0, 8);
        put(&mut bytes, 96, 8192, 8);
        put(&mut bytes, 104, 8192, 8);
        put(&mut bytes, 56, 2, 2);
        for (offset, value, count) in [
            (120, 7, 4),
            (124, 4, 4),
            (128, 4096, 8),
            (136, 0x401000, 8),
            (152, 4, 8),
            (160, 32, 8),
            (168, 4, 8),
        ] {
            put(&mut bytes, offset, value, count);
        }
        assert_eq!(parse(&bytes).unwrap_err(), Error::Unsupported);
        let image = parse_linux(&bytes).unwrap();
        assert_eq!(image.phdr, 0x400040);
        assert_eq!(image.phnum, 2);
        put(&mut bytes, 160, 8192, 8);
        assert_eq!(parse_linux(&bytes).unwrap_err(), Error::Bounds);
        put(&mut bytes, 160, 2, 8);
        assert_eq!(parse_linux(&bytes).unwrap_err(), Error::Bounds);
    }
    #[test]
    fn rejects_page_overlap_before_mapping_or_allocation() {
        let mut bytes = valid();
        put(&mut bytes, 56, 2, 2);
        let header: [u8; 56] = bytes[64..120].try_into().unwrap();
        bytes[120..176].copy_from_slice(&header);
        assert_eq!(parse(&bytes).unwrap_err(), Error::Overlap);
        put(&mut bytes, 136, 0x401000, 8);
        assert_eq!(parse(&bytes).unwrap().segments().len(), 2);
    }
}
