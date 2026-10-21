// SPDX-License-Identifier: Apache-2.0
//! Strict single-group ext2 direct-block reader and bounded replacement writer.
//! M7 will add allocation, path traversal and the live mount. Existing data writes
//! precede inode-size publication and a required device flush; ext2 has no journal.
use crate::{
    Error,
    block::{self, BlockDevice},
};

const MAX_DIRECT: usize = 12;
const MAX_BLOCK: usize = 4096;
const ROOT_INODE: u32 = 2;
const FILETYPE: u32 = 2;
const SPARSE_SUPER: u32 = 1;
const LARGE_FILE: u32 = 2;

fn le16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}
fn le32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[derive(Clone, Copy)]
struct Inode {
    offset: u64,
    size: u32,
    mode: u16,
    blocks: [u32; MAX_DIRECT],
}

pub struct Ext2<D> {
    device: D,
    block_size: u32,
    blocks: u32,
    inodes: u32,
    inode_size: u16,
    table_block: u32,
    first_data: u32,
    first_ino: u32,
    filetype: bool,
}

impl<D: BlockDevice> Ext2<D> {
    pub fn mount(mut device: D) -> Result<Self, Error> {
        let mut sb = [0; 1024];
        block::read_at(&mut device, 1024, &mut sb)?;
        if le16(&sb, 56) != 0xef53 || le16(&sb, 58) != 1 {
            return Err(Error::Corrupt);
        }
        let inodes = le32(&sb, 0);
        let blocks = le32(&sb, 4);
        let first_data = le32(&sb, 20);
        let log = le32(&sb, 24);
        if log > 2 || blocks == 0 || inodes < 11 {
            return Err(Error::Unsupported);
        }
        let block_size = 1024u32 << log;
        if first_data != u32::from(block_size == 1024)
            || le32(&sb, 32) < blocks - first_data
            || le32(&sb, 40) < inodes
            || blocks - first_data > block_size * 8
            || inodes > block_size * 8
        {
            return Err(Error::Unsupported);
        }
        let revision = le32(&sb, 76);
        if revision > 1 {
            return Err(Error::Unsupported);
        }
        let inode_size = if revision == 0 { 128 } else { le16(&sb, 88) };
        let first_ino = if revision == 0 { 11 } else { le32(&sb, 84) };
        if inode_size < 128
            || !inode_size.is_power_of_two()
            || u32::from(inode_size) > block_size
            || first_ino < 11
        {
            return Err(Error::Unsupported);
        }
        let compat = if revision == 0 { 0 } else { le32(&sb, 92) };
        let incompat = if revision == 0 { 0 } else { le32(&sb, 96) };
        let ro = if revision == 0 { 0 } else { le32(&sb, 100) };
        if compat != 0 || incompat & !FILETYPE != 0 || ro & !(SPARSE_SUPER | LARGE_FILE) != 0 {
            return Err(Error::Unsupported);
        }
        let bytes = u64::from(blocks) * u64::from(block_size);
        if bytes > device.sectors().checked_mul(512).ok_or(Error::Overflow)? {
            return Err(Error::OutOfRange);
        }
        let descriptor = if block_size == 1024 { 2_u64 } else { 1_u64 } * u64::from(block_size);
        let mut gd = [0; 32];
        block::read_at(&mut device, descriptor, &mut gd)?;
        let bitmap = le32(&gd, 0);
        let inode_bitmap = le32(&gd, 4);
        let table_block = le32(&gd, 8);
        let table_bytes = u64::from(inodes) * u64::from(inode_size);
        let table_blocks = table_bytes.div_ceil(block_size as u64);
        if bitmap <= first_data
            || bitmap >= blocks
            || inode_bitmap <= first_data
            || inode_bitmap >= blocks
            || table_block <= first_data
            || u64::from(table_block) + table_blocks > u64::from(blocks)
        {
            return Err(Error::Corrupt);
        }
        let fs = Self {
            device,
            block_size,
            blocks,
            inodes,
            inode_size,
            table_block,
            first_data,
            first_ino,
            filetype: incompat & FILETYPE != 0,
        };
        let mut fs = fs;
        let root = fs.read_inode(ROOT_INODE)?;
        if root.mode & 0xf000 != 0x4000 || root.size == 0 {
            return Err(Error::Corrupt);
        }
        Ok(fs)
    }

    pub fn into_device(self) -> D {
        self.device
    }
    pub fn block_size(&self) -> u32 {
        self.block_size
    }
    pub fn total_blocks(&self) -> u32 {
        self.blocks
    }

    fn block_offset(&self, number: u32) -> Result<u64, Error> {
        if number <= self.first_data || number >= self.blocks {
            return Err(Error::Corrupt);
        }
        Ok(u64::from(number) * u64::from(self.block_size))
    }

    fn read_inode(&mut self, number: u32) -> Result<Inode, Error> {
        if number == 0 || number > self.inodes {
            return Err(Error::Corrupt);
        }
        let offset = u64::from(self.table_block) * u64::from(self.block_size)
            + u64::from(number - 1) * u64::from(self.inode_size);
        let mut data = [0; 128];
        block::read_at(&mut self.device, offset, &mut data)?;
        let mut blocks = [0; MAX_DIRECT];
        for (i, value) in blocks.iter_mut().enumerate() {
            *value = le32(&data, 40 + i * 4);
        }
        let extra = [le32(&data, 88), le32(&data, 92), le32(&data, 96)];
        // No indirect pointers are supported, even when the file fits in direct blocks.
        if extra.iter().any(|value| *value != 0) || le32(&data, 108) != 0 {
            return Err(Error::Unsupported);
        }
        Ok(Inode {
            offset,
            size: le32(&data, 4),
            mode: le16(&data, 0),
            blocks,
        })
    }

    fn read_inode_file(&mut self, inode: &Inode, output: &mut [u8]) -> Result<usize, Error> {
        let length = inode.size as usize;
        if length > MAX_DIRECT * self.block_size as usize {
            return Err(Error::Unsupported);
        }
        if output.len() < length {
            return Err(Error::TooLarge);
        }
        let mut written = 0;
        for number in inode.blocks.iter().copied() {
            if written >= length {
                break;
            }
            let n = (length - written).min(self.block_size as usize);
            if number == 0 {
                output[written..written + n].fill(0);
            } else {
                let offset = self.block_offset(number)?;
                block::read_at(&mut self.device, offset, &mut output[written..written + n])?;
            }
            written += n;
        }
        if written != length {
            return Err(Error::Corrupt);
        }
        Ok(written)
    }

    /// Resolve only immediate root entries, excluding symlinks and directory traversal.
    pub fn lookup_root(&mut self, name: &str) -> Result<u32, Error> {
        if name.is_empty() || name.len() > 63 || name.contains(['/', '\0']) {
            return Err(Error::Unsupported);
        }
        let root = self.read_inode(ROOT_INODE)?;
        let mut data = [0; MAX_DIRECT * MAX_BLOCK];
        let len = self.read_inode_file(&root, &mut data)?;
        let mut offset = 0;
        while offset < len {
            if len - offset < 8 {
                return Err(Error::Corrupt);
            }
            let ino = le32(&data, offset);
            let rec = le16(&data, offset + 4) as usize;
            let count = if self.filetype {
                data[offset + 6] as usize
            } else {
                le16(&data, offset + 6) as usize
            };
            if rec < 8 || rec % 4 != 0 || rec > len - offset || count > rec - 8 {
                return Err(Error::Corrupt);
            }
            if count == name.len() && &data[offset + 8..offset + 8 + count] == name.as_bytes() {
                return if ino == 0 {
                    Err(Error::NotFound)
                } else {
                    Ok(ino)
                };
            }
            offset += rec;
        }
        Err(Error::NotFound)
    }

    pub fn read_file(&mut self, name: &str, output: &mut [u8]) -> Result<usize, Error> {
        let number = self.lookup_root(name)?;
        let inode = self.read_inode(number)?;
        if inode.mode & 0xf000 != 0x8000 {
            return Err(Error::NotFile);
        }
        self.read_inode_file(&inode, output)
    }

    /// Replace an existing regular file inside already allocated direct blocks.
    /// Refuse growth needing new blocks; keep ext2 bitmap and inode allocation unchanged.
    pub fn overwrite(&mut self, name: &str, input: &[u8]) -> Result<(), Error> {
        let number = self.lookup_root(name)?;
        if number < self.first_ino {
            return Err(Error::Unsupported);
        }
        let inode = self.read_inode(number)?;
        if inode.mode & 0xf000 != 0x8000 {
            return Err(Error::NotFile);
        }
        if input.len() > MAX_DIRECT * self.block_size as usize {
            return Err(Error::TooLarge);
        }
        let needed = input.len().div_ceil(self.block_size as usize);
        for number in &inode.blocks[..needed] {
            if *number == 0 {
                return Err(Error::NoSpace);
            }
            self.block_offset(*number)?;
        }
        let mut copied = 0;
        for number in inode.blocks.iter().copied().take(needed) {
            let take = (input.len() - copied).min(self.block_size as usize);
            block::write_at(
                &mut self.device,
                u64::from(number) * u64::from(self.block_size),
                &input[copied..copied + take],
            )?;
            copied += take;
        }
        let size = u32::try_from(input.len()).map_err(|_| Error::TooLarge)?;
        block::write_at(&mut self.device, inode.offset + 4, &size.to_le_bytes())?;
        self.device.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::vec;

    struct Image(std::vec::Vec<u8>);
    impl BlockDevice for Image {
        fn sectors(&self) -> u64 {
            (self.0.len() / 512) as u64
        }
        fn read_sector(&mut self, lba: u64, bytes: &mut [u8; 512]) -> Result<(), Error> {
            block::check(self.sectors(), lba, 1)?;
            let n = lba as usize * 512;
            bytes.copy_from_slice(&self.0[n..n + 512]);
            Ok(())
        }
        fn write_sector(&mut self, lba: u64, bytes: &[u8; 512]) -> Result<(), Error> {
            block::check(self.sectors(), lba, 1)?;
            let n = lba as usize * 512;
            self.0[n..n + 512].copy_from_slice(bytes);
            Ok(())
        }
        fn flush(&mut self) -> Result<(), Error> {
            Ok(())
        }
    }
    fn set16(out: &mut [u8], at: usize, value: u16) {
        out[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn set32(out: &mut [u8], at: usize, value: u32) {
        out[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn fixture() -> Image {
        let mut bytes = vec![0u8; 32 * 4096];
        let sb = 1024;
        set32(&mut bytes, sb, 16); // inodes
        set32(&mut bytes, sb + 4, 32); // filesystem blocks
        set32(&mut bytes, sb + 20, 0);
        set32(&mut bytes, sb + 24, 2);
        set32(&mut bytes, sb + 32, 32);
        set32(&mut bytes, sb + 40, 16);
        set16(&mut bytes, sb + 56, 0xef53);
        set16(&mut bytes, sb + 58, 1);
        set32(&mut bytes, sb + 76, 1);
        set32(&mut bytes, sb + 84, 11);
        set16(&mut bytes, sb + 88, 128);
        set32(&mut bytes, sb + 96, FILETYPE);
        set32(&mut bytes, 4096, 3);
        set32(&mut bytes, 4100, 4);
        set32(&mut bytes, 4104, 5);
        let root = 5 * 4096 + 128;
        set16(&mut bytes, root, 0x41ed);
        set32(&mut bytes, root + 4, 4096);
        set32(&mut bytes, root + 40, 9);
        let file = 5 * 4096 + 11 * 128;
        set16(&mut bytes, file, 0x81a4);
        set32(&mut bytes, file + 4, 5);
        set32(&mut bytes, file + 40, 10);
        let dir = 9 * 4096;
        set32(&mut bytes, dir, 2);
        set16(&mut bytes, dir + 4, 12);
        bytes[dir + 6] = 1;
        bytes[dir + 8] = b'.';
        set32(&mut bytes, dir + 12, 2);
        set16(&mut bytes, dir + 16, 12);
        bytes[dir + 18] = 2;
        bytes[dir + 20..dir + 22].copy_from_slice(b"..");
        set32(&mut bytes, dir + 24, 12);
        set16(&mut bytes, dir + 28, 4096 - 24);
        bytes[dir + 30] = 4;
        bytes[dir + 32..dir + 36].copy_from_slice(b"note");
        bytes[10 * 4096..10 * 4096 + 5].copy_from_slice(b"hello");
        Image(bytes)
    }

    #[test]
    fn reads_and_overwrites_an_existing_file_with_flush() {
        let mut fs = Ext2::mount(fixture()).unwrap();
        assert_eq!(fs.block_size(), 4096);
        let mut out = [0; 24];
        assert_eq!(fs.read_file("note", &mut out).unwrap(), 5);
        assert_eq!(&out[..5], b"hello");
        fs.overwrite("note", b"persisted").unwrap();
        let device = fs.into_device();
        let mut after = Ext2::mount(device).unwrap();
        assert_eq!(after.read_file("note", &mut out).unwrap(), 9);
        assert_eq!(&out[..9], b"persisted");
        assert_eq!(after.overwrite("missing", b"x"), Err(Error::NotFound));
    }

    #[test]
    fn reject_unknown_features_and_corrupt_directory_records() {
        let mut image = fixture();
        set32(&mut image.0, 1024 + 96, 0x80);
        assert!(matches!(Ext2::mount(image), Err(Error::Unsupported)));
        let mut image = fixture();
        set16(&mut image.0, 9 * 4096 + 4, 0);
        let mut fs = Ext2::mount(image).unwrap();
        assert_eq!(fs.lookup_root("note"), Err(Error::Corrupt));
    }

    #[test]
    fn reject_out_of_bounds_device_and_unsupported_growth() {
        let mut image = fixture();
        image.0.truncate(4096);
        assert!(matches!(Ext2::mount(image), Err(Error::OutOfRange)));
        let mut fs = Ext2::mount(fixture()).unwrap();
        assert_eq!(fs.overwrite("note", &[b'X'; 4097]), Err(Error::NoSpace));
    }
}
