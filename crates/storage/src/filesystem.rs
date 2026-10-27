// SPDX-License-Identifier: Apache-2.0
//! Define persistent volume capabilities, stable inode identities and bounded directory cursors.
//! Device ownership stays in each backend; unsupported mutations fail before touching the disk.
use crate::ext2::Ext2;
use alloc::boxed::Box;
use anova_abi::{INVALID_FILESYSTEM, READ_ONLY};

/// Select by an explicit on-disk extent feature; never reinterpret a failed ext4 mount as ext2.
pub fn mount(mut device: Box<dyn crate::BlockDevice>) -> Result<Box<dyn Filesystem>, i64> {
    let mut superblock = [0; crate::SECTOR];
    device.read_sector(2, &mut superblock)?;
    let incompat = u32::from_le_bytes(superblock[96..100].try_into().unwrap());
    if incompat & 0x40 != 0 {
        Ok(crate::try_box(crate::ext4::Ext4::mount(device)?)?)
    } else {
        Ok(crate::try_box(Ext2::mount(device)?)?)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Ext2,
    Ext4,
}

#[derive(Clone, Copy, Debug)]
pub struct Capabilities {
    pub format: Format,
    pub root: u32,
    pub max_file: usize,
    pub timestamp_nanos: u64,
    pub read_only: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Metadata {
    pub number: u32,
    pub directory: bool,
    pub size: usize,
    pub executable: bool,
    pub permissions: u16,
    pub modified: u32,
    pub accessed: u32,
    pub changed: u32,
}

pub struct Entry {
    pub inode: u32,
    pub name: [u8; 256],
    pub length: usize,
}

/// Cursors are backend byte positions, independent of the VFS node cache and native wire ABI.
pub trait Filesystem {
    fn capabilities(&self) -> Capabilities;
    fn inode(&mut self, number: u32) -> Result<Metadata, i64>;
    fn read(&mut self, number: u32, offset: usize, output: &mut [u8]) -> Result<usize, i64>;
    fn next_entry(&mut self, number: u32, cursor: u64) -> Result<Option<(Entry, u64)>, i64>;
    fn lookup(&mut self, parent: u32, name: &str) -> Result<Option<Entry>, i64> {
        let mut cursor = 0;
        let mut found = None;
        while let Some((entry, next)) = self.next_entry(parent, cursor)? {
            if next <= cursor {
                return Err(INVALID_FILESYSTEM);
            }
            cursor = next;
            if &entry.name[..entry.length] == name.as_bytes() {
                if found.is_some() {
                    return Err(INVALID_FILESYSTEM);
                }
                found = Some(entry);
            }
        }
        Ok(found)
    }
    fn set_clock(&mut self, _seconds: u32) {}
    fn sync(&mut self) -> Result<(), i64>;
    fn shutdown(&mut self) -> Result<(), i64>;
    fn stop_device(&mut self) -> Result<(), i64>;
    fn write(&mut self, _number: u32, _offset: usize, _input: &[u8]) -> Result<usize, i64> {
        Err(READ_ONLY)
    }
    fn truncate(&mut self, number: u32) -> Result<(), i64> {
        self.resize(number, 0)
    }
    fn resize(&mut self, _number: u32, _length: usize) -> Result<(), i64> {
        Err(READ_ONLY)
    }
    fn set_executable(&mut self, _number: u32) -> Result<(), i64> {
        Err(READ_ONLY)
    }
    fn set_permissions(&mut self, _number: u32, _permissions: u16) -> Result<(), i64> {
        Err(READ_ONLY)
    }
    fn set_times(
        &mut self,
        _number: u32,
        _accessed: Option<u32>,
        _modified: Option<u32>,
    ) -> Result<(), i64> {
        Err(READ_ONLY)
    }
    fn create(&mut self, _parent: u32, _name: &str, _directory: bool) -> Result<u32, i64> {
        Err(READ_ONLY)
    }
    fn unlink(&mut self, _parent: u32, _name: &str) -> Result<u32, i64> {
        Err(READ_ONLY)
    }
    fn rmdir(&mut self, _parent: u32, _name: &str) -> Result<u32, i64> {
        Err(READ_ONLY)
    }
    fn rename(
        &mut self,
        _parent: u32,
        _name: &str,
        _destination: u32,
        _new: &str,
    ) -> Result<(), i64> {
        Err(READ_ONLY)
    }
    fn release_unlinked(&mut self, _number: u32) -> Result<(), i64> {
        Err(READ_ONLY)
    }
}

impl Filesystem for Ext2 {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            format: Format::Ext2,
            root: 2,
            max_file: crate::ext2::MAX_FILE,
            timestamp_nanos: 1_000_000_000,
            read_only: self.read_only(),
        }
    }
    fn inode(&mut self, number: u32) -> Result<Metadata, i64> {
        let n = Ext2::inode(self, number)?;
        Ok(Metadata {
            number: n.number,
            directory: n.directory,
            size: n.size,
            executable: n.executable,
            permissions: n.permissions,
            modified: n.modified,
            accessed: n.accessed,
            changed: n.changed,
        })
    }
    fn read(&mut self, n: u32, offset: usize, output: &mut [u8]) -> Result<usize, i64> {
        Ext2::read(self, n, offset, output)
    }
    fn next_entry(&mut self, n: u32, cursor: u64) -> Result<Option<(Entry, u64)>, i64> {
        Ext2::next_entry(self, n, cursor)
    }
    fn set_clock(&mut self, seconds: u32) {
        Ext2::set_clock(self, seconds);
    }
    fn sync(&mut self) -> Result<(), i64> {
        Ext2::sync(self)
    }
    fn shutdown(&mut self) -> Result<(), i64> {
        Ext2::shutdown(self)
    }
    fn stop_device(&mut self) -> Result<(), i64> {
        Ext2::stop_device(self)
    }
    fn write(&mut self, n: u32, offset: usize, input: &[u8]) -> Result<usize, i64> {
        Ext2::write(self, n, offset, input)
    }
    fn truncate(&mut self, n: u32) -> Result<(), i64> {
        Ext2::truncate(self, n)
    }
    fn resize(&mut self, n: u32, length: usize) -> Result<(), i64> {
        Ext2::resize(self, n, length)
    }
    fn set_executable(&mut self, n: u32) -> Result<(), i64> {
        Ext2::set_executable(self, n)
    }
    fn set_permissions(&mut self, n: u32, permissions: u16) -> Result<(), i64> {
        Ext2::set_permissions(self, n, permissions)
    }
    fn set_times(
        &mut self,
        n: u32,
        accessed: Option<u32>,
        modified: Option<u32>,
    ) -> Result<(), i64> {
        Ext2::set_times(self, n, accessed, modified)
    }
    fn create(&mut self, p: u32, name: &str, directory: bool) -> Result<u32, i64> {
        Ext2::create(self, p, name, directory)
    }
    fn unlink(&mut self, p: u32, name: &str) -> Result<u32, i64> {
        Ext2::unlink(self, p, name)
    }
    fn rmdir(&mut self, p: u32, name: &str) -> Result<u32, i64> {
        Ext2::rmdir(self, p, name)
    }
    fn rename(&mut self, p: u32, name: &str, d: u32, new: &str) -> Result<(), i64> {
        Ext2::rename(self, p, name, d, new)
    }
    fn release_unlinked(&mut self, n: u32) -> Result<(), i64> {
        Ext2::release_unlinked(self, n)
    }
}
