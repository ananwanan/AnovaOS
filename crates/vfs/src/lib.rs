// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Bounded mounts with immutable ABI policy, shared files/maps, packet pairs, pipes and PTYs.
//! Cached timestamps retain the backing filesystem's supported precision.
//! Persistent directories stream from a backend; inactive file nodes are reclaimable under pressure.
//! Directory/cwd identities, open files, mappings and the ABI policy are pinned within fixed quotas.
//! One non-preemptible owner serializes all mutations; no hardware, locks or user pointers.
extern crate alloc;
pub mod archive;
pub mod fd;
pub mod file_mapping;
#[cfg(test)]
mod foundation_tests;
#[cfg(test)]
mod tests;
use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::vec::Vec;
use anova_abi::*;
use anova_storage::filesystem::{Entry as DiskEntry, Filesystem};
pub use fd::Descriptors;
use file_mapping::MappedFile;

pub const ROOT: usize = 0;
const PERSISTENT_ID_BASE: u64 = 1 << 32;
// Leave room for PTY/library nodes and the independent real heap-exhaustion workload.
pub const NODE_LIMIT: usize = 2048;
pub const OPEN_LIMIT: usize = 128;
pub const PIPE_LIMIT: usize = 16;
pub const PIPE_BYTES: usize = 1024;
pub const PACKET_BYTES: usize = PIPE_BYTES - 2;
// Internal/Linux bounds; native path and directory-record adapters retain their v1 capacities.
pub const PATH_BYTES: usize = 512;
pub const NAME_BYTES: usize = 255;
pub const FILE_BYTES: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mount {
    Initramfs,
    Tmpfs,
    Devfs,
    Persistent,
}
impl Mount {
    // Cached metadata must use the same precision as the persistent inode format.
    fn timestamp(self, nanos: u64) -> u64 {
        if self == Self::Persistent {
            nanos / 1_000_000_000 * 1_000_000_000
        } else {
            nanos
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Console,
    Null,
    Ptmx,
    PtyMaster(usize),
    PtySlave(usize),
}
enum Data<'a> {
    Directory,
    Borrowed(&'a [u8], bool),
    Mutable(Vec<u8>),
    Link(&'a str),
    Device(Device),
    Persistent { size: usize, executable: bool },
}
struct Node<'a> {
    parent: usize,
    name: [u8; NAME_BYTES + 1],
    len: usize,
    mount: Mount,
    linked: bool,
    data: Data<'a>,
    inode: Option<u32>,
    permissions: u16,
    modified: u64,
    accessed: u64,
    changed: u64,
    mapping: Option<MappedFile>,
}
/// Internal directory names are independent of the fixed-size native v1 wire record.
pub struct DirectoryEntry {
    pub inode: u64,
    pub kind: u32,
    pub name_len: u32,
    pub name: [u8; NAME_BYTES + 1],
    pub next_offset: usize,
}
#[derive(Clone, Copy)]
enum Target {
    Node(usize),
    ReadPipe(usize),
    WritePipe(usize),
    Packet { input: usize, output: usize },
    Socket(usize),
}
#[derive(Clone, Copy)]
struct Open {
    target: Target,
    offset: usize,
    flags: u64,
    refs: usize,
    lock: u8,
}
struct Pipe {
    bytes: [u8; PIPE_BYTES],
    head: usize,
    len: usize,
    readers: usize,
    writers: usize,
}
pub struct Vfs<'a> {
    nodes: Vec<Node<'a>>,
    opens: Vec<Option<Open>>,
    pipes: Vec<Option<Pipe>>,
    disk: Option<Box<dyn Filesystem>>,
    // This inode owns the mount-time executable ABI policy and cannot change while mounted.
    policy_node: Option<usize>,
    linux: [bool; NODE_LIMIT],
    pty_unlocked: [bool; 4],
    clock: u64,
    mapping_cache: bool,
}
fn slots<T>(n: usize, mut f: impl FnMut() -> T) -> Result<Vec<T>, i64> {
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|_| NO_MEMORY)?;
    v.resize_with(n, &mut f);
    Ok(v)
}
impl<'a> Vfs<'a> {
    pub fn new(archive: &'a [u8]) -> Result<Self, i64> {
        let mut nodes = Vec::new();
        nodes.try_reserve_exact(32).map_err(|_| NO_MEMORY)?;
        nodes.push(Node {
            parent: ROOT,
            name: [0; NAME_BYTES + 1],
            len: 0,
            mount: Mount::Initramfs,
            linked: true,
            data: Data::Directory,
            inode: None,
            permissions: 0o755,
            modified: 0,
            accessed: 0,
            changed: 0,
            mapping: None,
        });
        let mut fs = Self {
            nodes,
            opens: slots(OPEN_LIMIT, || None)?,
            pipes: slots(PIPE_LIMIT, || None)?,
            disk: None,
            policy_node: None,
            linux: [false; NODE_LIMIT],
            pty_unlocked: [false; 4],
            clock: 0,
            mapping_cache: false,
        };
        let mut input = archive::Archive::new(archive)?;
        while let Some(entry) = input.entry()? {
            let data = match entry.kind {
                archive::DIRECTORY => Data::Directory,
                archive::SYMLINK => {
                    Data::Link(core::str::from_utf8(entry.data).map_err(|_| INVALID_ARGUMENT)?)
                }
                kind => Data::Borrowed(
                    entry.data,
                    matches!(kind, archive::EXECUTABLE | archive::LINUX_EXECUTABLE),
                ),
            };
            let (parent, name) = fs.parent(ROOT, entry.path)?;
            // Archive paths may never traverse a symlink supplied by an earlier entry.
            let lexical = entry.path.rsplit_once('/').unwrap().0;
            let mut canonical = [0; PATH_BYTES];
            let length = fs.canonical(parent, &mut canonical)?;
            if !lexical.is_empty() && &canonical[..length] != lexical.as_bytes() {
                return Err(INVALID_ARGUMENT);
            }
            let index = fs.insert(parent, name, data, Mount::Initramfs)?;
            fs.linux[index] = entry.kind == archive::LINUX_EXECUTABLE;
        }
        fs.mount("/tmp", Mount::Tmpfs)?;
        fs.mount("/dev", Mount::Devfs)?;
        let dev = fs.resolve(ROOT, "/dev")?;
        fs.insert(dev, "console", Data::Device(Device::Console), Mount::Devfs)?;
        fs.insert(dev, "null", Data::Device(Device::Null), Mount::Devfs)?;
        fs.insert(dev, "ptmx", Data::Device(Device::Ptmx), Mount::Devfs)?;
        let pts = fs.insert(dev, "pts", Data::Directory, Mount::Devfs)?;
        for (n, (master, slave)) in [("ptm0", "0"), ("ptm1", "1"), ("ptm2", "2"), ("ptm3", "3")]
            .iter()
            .enumerate()
        {
            fs.insert(
                dev,
                master,
                Data::Device(Device::PtyMaster(n)),
                Mount::Devfs,
            )?;
            fs.insert(pts, slave, Data::Device(Device::PtySlave(n)), Mount::Devfs)?;
        }
        Ok(fs)
    }
    /// Publish only the mount root and immutable ABI marker; all other names load on demand.
    pub fn mount_disk<F: Filesystem + 'static>(&mut self, disk: F) -> Result<(), i64> {
        self.mount_backend(anova_storage::try_box(disk)?)
    }
    pub fn mount_backend(&mut self, mut disk: Box<dyn Filesystem>) -> Result<(), i64> {
        if self.disk.is_some() {
            return Err(INVALID_ARGUMENT);
        }
        let root = self.resolve(ROOT, "/persist")?;
        let caps = disk.capabilities();
        if caps.timestamp_nanos != 1_000_000_000 {
            return Err(UNSUPPORTED);
        }
        let info = disk.inode(caps.root)?;
        if !info.directory {
            return Err(INVALID_FILESYSTEM);
        }
        let marker = disk
            .lookup(caps.root, ".anova-abi")?
            .ok_or(INVALID_FILESYSTEM)?;
        let policy = disk.inode(marker.inode)?;
        if policy.directory || policy.size > 32 {
            return Err(INVALID_FILESYSTEM);
        }
        let mut bytes = [0; 32];
        let n = disk.read(marker.inode, 0, &mut bytes)?;
        if n != policy.size {
            return Err(INVALID_FILESYSTEM);
        }
        let linux = match &bytes[..n] {
            b"anova.native.v1\n" => false,
            b"linux.x86_64.v1\n" => true,
            _ => return Err(INVALID_FILESYSTEM),
        };
        self.ensure_node_slot(&[])?;
        self.mount("/persist", Mount::Persistent)?;
        self.nodes[root].inode = Some(caps.root);
        self.linux[root] = linux;
        self.disk = Some(disk);
        match self.cache_entry(root, marker, &[]) {
            Ok(manifest) => {
                self.policy_node = Some(manifest);
                self.nodes[root].permissions = info.permissions;
                self.nodes[root].modified = u64::from(info.modified) * 1_000_000_000;
                self.nodes[root].accessed = u64::from(info.accessed) * 1_000_000_000;
                self.nodes[root].changed = u64::from(info.changed) * 1_000_000_000;
                Ok(())
            }
            Err(error) => {
                self.disk = None;
                self.nodes[root].mount = Mount::Initramfs;
                self.nodes[root].inode = None;
                self.linux[root] = false;
                Err(error)
            }
        }
    }
    fn held(&self, index: usize) -> bool {
        self.nodes[index]
            .mapping
            .as_ref()
            .is_some_and(MappedFile::active)
            || self
                .opens
                .iter()
                .flatten()
                .any(|o| matches!(o.target, Target::Node(n) if n == index))
    }
    fn reusable(&self, index: usize) -> bool {
        !self.nodes[index].linked && self.nodes[index].inode.is_none() && !self.held(index)
    }
    /// Directory identities may be held as cwd outside the VFS and are never evicted.
    fn ensure_node_slot(&mut self, protected: &[usize]) -> Result<(), i64> {
        self.collect_mappings()?;
        if (0..self.nodes.len()).any(|i| self.reusable(i)) {
            return Ok(());
        }
        if self.nodes.len() < NODE_LIMIT {
            self.nodes.try_reserve(1).map_err(|_| NO_MEMORY)?;
            return Ok(());
        }
        let index = (0..self.nodes.len())
            .find(|&i| {
                self.nodes[i].linked
                    && matches!(self.nodes[i].data, Data::Persistent { .. })
                    && self.policy_node != Some(i)
                    && !protected.contains(&i)
                    && !self.held(i)
            })
            .ok_or(NO_SPACE)?;
        // Eviction drops only cache state. It must never release the live on-disk inode.
        self.nodes[index].linked = false;
        self.nodes[index].inode = None;
        self.nodes[index].data = Data::Mutable(Vec::new());
        self.nodes[index].mapping = None;
        Ok(())
    }
    fn cache_entry(
        &mut self,
        parent: usize,
        entry: DiskEntry,
        protected: &[usize],
    ) -> Result<usize, i64> {
        // This profile has no hard-link API. Reject cycles and cached metadata aliases.
        if self.nodes.iter().any(|n| n.inode == Some(entry.inode)) {
            return Err(INVALID_FILESYSTEM);
        }
        let info = self.disk.as_mut().ok_or(IO_ERROR)?.inode(entry.inode)?;
        let name =
            core::str::from_utf8(&entry.name[..entry.length]).map_err(|_| INVALID_FILESYSTEM)?;
        if name == "." || name == ".." || name.contains(['/', '\0']) {
            return Err(INVALID_FILESYSTEM);
        }
        self.ensure_node_slot(protected)?;
        let data = if info.directory {
            Data::Directory
        } else {
            Data::Persistent {
                size: info.size,
                executable: info.executable,
            }
        };
        let node = self.insert(parent, name, data, Mount::Persistent)?;
        self.nodes[node].inode = Some(info.number);
        self.nodes[node].permissions = info.permissions;
        self.nodes[node].modified = u64::from(info.modified) * 1_000_000_000;
        self.nodes[node].accessed = u64::from(info.accessed) * 1_000_000_000;
        self.nodes[node].changed = u64::from(info.changed) * 1_000_000_000;
        Ok(node)
    }
    fn lookup_child(
        &mut self,
        parent: usize,
        name: &str,
        protected: &[usize],
    ) -> Result<Option<usize>, i64> {
        if let Some(index) = self.child(parent, name) {
            return Ok(Some(index));
        }
        if self.nodes[parent].mount != Mount::Persistent {
            return Ok(None);
        }
        let number = self.nodes[parent].inode.ok_or(INVALID_FILESYSTEM)?;
        let entry = self.disk.as_mut().ok_or(IO_ERROR)?.lookup(number, name)?;
        entry
            .map(|entry| self.cache_entry(parent, entry, protected))
            .transpose()
    }
    fn identity(&self, index: usize) -> u64 {
        self.nodes[index]
            .inode
            .map_or(index as u64, |n| PERSISTENT_ID_BASE + u64::from(n))
    }
    fn identity_node(&self, identity: u64) -> Option<usize> {
        if identity < PERSISTENT_ID_BASE {
            return ((identity as usize) < self.nodes.len()).then_some(identity as usize);
        }
        let number = u32::try_from(identity - PERSISTENT_ID_BASE).ok()?;
        self.nodes.iter().position(|n| n.inode == Some(number))
    }
    fn touch_node(&mut self, node: usize, contents: bool) {
        if self.nodes[node].mount == Mount::Persistent
            && self
                .disk
                .as_ref()
                .is_some_and(|d| d.capabilities().format == anova_storage::filesystem::Format::Ext2)
        {
            // Preserve the independently accepted ext2 metadata/cache subset.
            return;
        }
        let timestamp = self.nodes[node].mount.timestamp(self.clock);
        self.nodes[node].changed = timestamp;
        if contents {
            self.nodes[node].modified = timestamp;
        }
    }
    fn persistent_limit(&self) -> usize {
        self.disk.as_ref().map_or(0, |d| d.capabilities().max_file)
    }
    fn persistent_writable(&self) -> bool {
        self.disk
            .as_ref()
            .is_some_and(|d| !d.capabilities().read_only)
    }
    pub fn sync(&mut self) -> Result<(), i64> {
        self.collect_mappings()?;
        match &mut self.disk {
            Some(disk) => disk.sync(),
            None => Ok(()),
        }
    }
    pub fn set_clock(&mut self, nanos: u64) {
        self.clock = nanos;
        if let Some(disk) = self.disk.as_mut() {
            disk.set_clock((nanos / 1_000_000_000) as u32);
        }
    }
    pub fn attributes(&self, inode: u64) -> (u16, u64) {
        self.identity_node(inode)
            .and_then(|n| self.nodes.get(n))
            .map_or((0o600, 0), |n| (n.permissions, n.modified))
    }
    pub fn times(&self, inode: u64) -> [u64; 3] {
        self.identity_node(inode)
            .and_then(|n| self.nodes.get(n))
            .map_or([0; 3], |n| [n.accessed, n.modified, n.changed])
    }
    /// Set checked regular-file/directory timestamps; persistent ext2 stores whole seconds.
    pub fn set_times(
        &mut self,
        inode: u64,
        accessed: Option<u64>,
        modified: Option<u64>,
    ) -> Result<(), i64> {
        let index = self.identity_node(inode).ok_or(INVALID_ARGUMENT)?;
        if accessed.is_none() && modified.is_none() {
            return Ok(());
        }
        self.mutable_node(index)?;
        let node = self.nodes.get_mut(index).ok_or(INVALID_ARGUMENT)?;
        if accessed.is_none() && modified.is_none() {
            return Ok(());
        }
        if !matches!(node.mount, Mount::Tmpfs | Mount::Persistent) {
            return Err(READ_ONLY);
        }
        let seconds = |time: u64| u32::try_from(time / 1_000_000_000).map_err(|_| INVALID_ARGUMENT);
        if let Some(number) = node.inode {
            self.disk.as_mut().ok_or(IO_ERROR)?.set_times(
                number,
                accessed.map(seconds).transpose()?,
                modified.map(seconds).transpose()?,
            )?;
        }
        if let Some(time) = accessed {
            node.accessed = node.mount.timestamp(time);
        }
        if let Some(time) = modified {
            node.modified = node.mount.timestamp(time);
        }
        node.changed = node.mount.timestamp(self.clock);
        Ok(())
    }
    pub fn fsync(&mut self, files: &Descriptors, fd: u64) -> Result<(), i64> {
        let open = self.opens[files.handle(fd)?].unwrap();
        match open.target {
            Target::Node(n) if !matches!(self.nodes[n].data, Data::Device(_)) => self.sync(),
            _ => Err(INVALID_ARGUMENT),
        }
    }
    pub fn shutdown(&mut self) -> Result<(), i64> {
        self.collect_mappings()?;
        if self.nodes.iter().any(|n| n.inode.is_some() && !n.linked) {
            return Err(IO_ERROR);
        }
        match &mut self.disk {
            Some(disk) => disk.shutdown(),
            None => Ok(()),
        }
    }
    /// Call only after successful shutdown; device Drop resets the queue and releases DMA.
    pub fn detach_disk(&mut self) -> Result<(), i64> {
        if let Some(disk) = &mut self.disk {
            disk.stop_device()?;
        }
        self.disk = None;
        Ok(())
    }
    pub fn set_executable(&mut self, cwd: usize, path: &str) -> Result<(), i64> {
        let node = self.resolve(cwd, path)?;
        self.mutable_node(node)?;
        let number = self.nodes[node].inode.ok_or(READ_ONLY)?;
        if !matches!(self.nodes[node].data, Data::Persistent { .. }) {
            return Err(IS_DIRECTORY);
        }
        self.disk.as_mut().ok_or(IO_ERROR)?.set_executable(number)?;
        if let Data::Persistent {
            ref mut executable, ..
        } = self.nodes[node].data
        {
            *executable = true;
        }
        self.nodes[node].permissions |= 0o111;
        self.touch_node(node, false);
        Ok(())
    }
    fn chmod_node(&mut self, node: usize, permissions: u16) -> Result<(), i64> {
        self.mutable_node(node)?;
        if permissions & !0o777 != 0 {
            return Err(UNSUPPORTED);
        }
        let inode = self.nodes[node].inode;
        let mount = self.nodes[node].mount;
        match self.nodes[node].data {
            Data::Persistent {
                ref mut executable, ..
            } => {
                self.disk
                    .as_mut()
                    .ok_or(IO_ERROR)?
                    .set_permissions(inode.unwrap(), permissions)?;
                *executable = permissions & 0o111 != 0;
            }
            Data::Directory if inode.is_some() => {
                self.disk
                    .as_mut()
                    .ok_or(IO_ERROR)?
                    .set_permissions(inode.unwrap(), permissions)?;
            }
            Data::Mutable(_) => {}
            Data::Directory if mount == Mount::Tmpfs => {}
            _ => return Err(READ_ONLY),
        }
        self.nodes[node].permissions = permissions;
        self.touch_node(node, false);
        Ok(())
    }
    pub fn chmod(&mut self, cwd: usize, path: &str, permissions: u16) -> Result<(), i64> {
        let node = self.resolve(cwd, path)?;
        self.chmod_node(node, permissions)
    }
    pub fn fchmod(&mut self, fds: &Descriptors, fd: u64, permissions: u16) -> Result<(), i64> {
        let Target::Node(n) = self.opens[fds.handle(fd)?].unwrap().target else {
            return Err(INVALID_ARGUMENT);
        };
        self.chmod_node(n, permissions)
    }
    pub fn load_executable(&mut self, cwd: usize, path: &str) -> Result<Cow<'a, [u8]>, i64> {
        let node = self.resolve(cwd, path)?;
        match self.nodes[node].data {
            Data::Borrowed(bytes, true) => Ok(Cow::Borrowed(bytes)),
            Data::Persistent {
                size,
                executable: true,
            } => {
                let mut bytes = Vec::new();
                bytes.try_reserve_exact(size).map_err(|_| NO_MEMORY)?;
                bytes.resize(size, 0);
                let n = self.disk.as_mut().ok_or(IO_ERROR)?.read(
                    self.nodes[node].inode.unwrap(),
                    0,
                    &mut bytes,
                )?;
                if n != size {
                    return Err(IO_ERROR);
                }
                Ok(Cow::Owned(bytes))
            }
            _ => Err(PERMISSION_DENIED),
        }
    }
    fn mount(&mut self, path: &str, mount: Mount) -> Result<(), i64> {
        let node = self.resolve(ROOT, path)?;
        if self.nodes[node].mount != Mount::Initramfs
            || !matches!(self.nodes[node].data, Data::Directory)
            || self
                .nodes
                .iter()
                .enumerate()
                .any(|(i, n)| i != node && n.linked && n.parent == node)
        {
            return Err(INVALID_ARGUMENT);
        }
        self.nodes[node].mount = mount;
        Ok(())
    }
    fn insert(
        &mut self,
        parent: usize,
        name: &str,
        data: Data<'a>,
        mount: Mount,
    ) -> Result<usize, i64> {
        self.ensure_node_slot(&[])?;
        if name.is_empty()
            || name.len() > NAME_BYTES
            || name == "."
            || name == ".."
            || name.contains(['/', '\0'])
        {
            return Err(INVALID_ARGUMENT);
        }
        if self.child(parent, name).is_some() {
            return Err(ALREADY_EXISTS);
        }
        let mut label = [0; NAME_BYTES + 1];
        label[..name.len()].copy_from_slice(name.as_bytes());
        let permissions = match &data {
            Data::Directory => 0o755,
            Data::Device(_) => 0o666,
            Data::Link(_) => 0o777,
            Data::Borrowed(_, true) => 0o555,
            Data::Borrowed(_, false) => 0o444,
            _ => 0o644,
        };
        let node = Node {
            parent,
            name: label,
            len: name.len(),
            mount,
            linked: true,
            data,
            inode: None,
            permissions,
            modified: mount.timestamp(self.clock),
            accessed: mount.timestamp(self.clock),
            changed: mount.timestamp(self.clock),
            mapping: None,
        };
        // Unlinked nodes remain alive until their final open description is closed.
        let reusable = self.nodes.iter().enumerate().position(|(i, n)| {
            !n.linked
                && n.inode.is_none()
                && !n.mapping.as_ref().is_some_and(MappedFile::active)
                && !self
                    .opens
                    .iter()
                    .flatten()
                    .any(|o| matches!(o.target,Target::Node(id) if id == i))
        });
        if let Some(i) = reusable {
            self.nodes[i] = node;
            self.linux[i] = self.linux[parent];
            Ok(i)
        } else if self.nodes.len() < NODE_LIMIT {
            self.linux[self.nodes.len()] = self.linux[parent];
            self.nodes.push(node);
            Ok(self.nodes.len() - 1)
        } else {
            Err(NO_SPACE)
        }
    }
    fn child(&self, parent: usize, name: &str) -> Option<usize> {
        self.nodes
            .iter()
            .enumerate()
            .find(|(i, n)| {
                *i != ROOT && n.linked && n.parent == parent && &n.name[..n.len] == name.as_bytes()
            })
            .map(|(i, _)| i)
    }
    pub fn resolve(&mut self, cwd: usize, path: &str) -> Result<usize, i64> {
        self.walk(cwd, path, 0)
    }
    fn walk(&mut self, cwd: usize, path: &str, depth: usize) -> Result<usize, i64> {
        if path.len() > PATH_BYTES {
            return Err(TOO_LONG);
        }
        if path.is_empty() || path.contains('\0') {
            return Err(INVALID_ARGUMENT);
        }
        if depth > 8 {
            return Err(LINK_LOOP);
        }
        let mut at = if path.starts_with('/') { ROOT } else { cwd };
        let mut parts = path.split('/').peekable();
        while let Some(part) = parts.next() {
            if part.is_empty() {
                continue;
            }
            if !matches!(self.nodes[at].data, Data::Directory) {
                return Err(NOT_DIRECTORY);
            }
            match part {
                "." => {}
                ".." => at = self.nodes[at].parent,
                name => {
                    if name.len() > NAME_BYTES {
                        return Err(TOO_LONG);
                    }
                    let next = self.lookup_child(at, name, &[])?.ok_or(NOT_FOUND)?;
                    if let Data::Link(target) = self.nodes[next].data {
                        at = self.walk(at, target, depth + 1)?;
                    } else {
                        at = next;
                    }
                }
            }
            if parts.peek().is_some() && !matches!(self.nodes[at].data, Data::Directory) {
                return Err(NOT_DIRECTORY);
            }
        }
        Ok(at)
    }
    fn parent<'p>(&mut self, cwd: usize, path: &'p str) -> Result<(usize, &'p str), i64> {
        if path.len() > PATH_BYTES {
            return Err(TOO_LONG);
        }
        if path.is_empty() || path.contains('\0') || path.ends_with('/') {
            return Err(INVALID_ARGUMENT);
        }
        let (base, name) = path.rsplit_once('/').unwrap_or((".", path));
        let parent = self.resolve(cwd, if base.is_empty() { "/" } else { base })?;
        if !matches!(self.nodes[parent].data, Data::Directory) {
            return Err(NOT_DIRECTORY);
        }
        if !self.nodes[parent].linked {
            return Err(NOT_FOUND);
        }
        if name.len() > NAME_BYTES {
            return Err(TOO_LONG);
        }
        if name == "." || name == ".." {
            return Err(INVALID_ARGUMENT);
        }
        Ok((parent, name))
    }
    pub fn canonical(&self, mut node: usize, output: &mut [u8]) -> Result<usize, i64> {
        let mut reversed = [0; PATH_BYTES];
        let mut count = 0;
        while node != ROOT {
            let n = &self.nodes[node];
            if count + n.len + 1 > reversed.len() {
                return Err(TOO_LONG);
            }
            for c in n.name[..n.len].iter().rev() {
                reversed[count] = *c;
                count += 1;
            }
            reversed[count] = b'/';
            count += 1;
            node = n.parent;
        }
        if count == 0 {
            reversed[0] = b'/';
            count = 1;
        }
        if output.len() < count {
            return Err(TOO_LONG);
        }
        for i in 0..count {
            output[i] = reversed[count - 1 - i];
        }
        Ok(count)
    }
    pub fn chdir(&mut self, cwd: usize, path: &str) -> Result<usize, i64> {
        let node = self.resolve(cwd, path)?;
        if matches!(self.nodes[node].data, Data::Directory) {
            Ok(node)
        } else {
            Err(NOT_DIRECTORY)
        }
    }
    pub fn mkdir(&mut self, cwd: usize, path: &str) -> Result<(), i64> {
        let (parent, name) = self.parent(cwd, path)?;
        if self.nodes[parent].mount == Mount::Persistent {
            self.ensure_node_slot(&[])?;
            let inode = self.disk.as_mut().ok_or(IO_ERROR)?.create(
                self.nodes[parent].inode.unwrap(),
                name,
                true,
            )?;
            let node = self.insert(parent, name, Data::Directory, Mount::Persistent)?;
            self.nodes[node].inode = Some(inode);
            self.touch_node(parent, true);
            return Ok(());
        }
        if self.nodes[parent].mount != Mount::Tmpfs {
            return Err(READ_ONLY);
        }
        self.insert(parent, name, Data::Directory, Mount::Tmpfs)
            .map(|_| ())
    }
    pub fn directory(&self, fds: &Descriptors, fd: u64) -> Result<usize, i64> {
        let Target::Node(node) = self.opens[fds.handle(fd)?].unwrap().target else {
            return Err(NOT_DIRECTORY);
        };
        if !matches!(self.nodes[node].data, Data::Directory) {
            return Err(NOT_DIRECTORY);
        }
        Ok(node)
    }
    /// Remove an empty directory, retaining an open inode until final close.
    /// Callers supply live cwd references; this bounded profile rejects those removals as busy.
    pub fn rmdir(&mut self, cwd: usize, path: &str, cwd_references: &[usize]) -> Result<(), i64> {
        let (parent, name) = self.parent(cwd, path)?;
        let node = self.lookup_child(parent, name, &[])?.ok_or(NOT_FOUND)?;
        if !matches!(self.nodes[node].data, Data::Directory) {
            return Err(NOT_DIRECTORY);
        }
        if !matches!(self.nodes[node].mount, Mount::Tmpfs | Mount::Persistent) {
            return Err(READ_ONLY);
        }
        if self.nodes[node].mount != self.nodes[parent].mount {
            return Err(BUSY);
        }
        if cwd_references.contains(&node) {
            return Err(BUSY);
        }
        if self.nodes[node].mount == Mount::Persistent
            && self
                .disk
                .as_mut()
                .ok_or(IO_ERROR)?
                .next_entry(self.nodes[node].inode.unwrap(), 0)?
                .is_some()
        {
            return Err(NOT_EMPTY);
        }
        if self.nodes.iter().any(|n| n.linked && n.parent == node) {
            return Err(NOT_EMPTY);
        }
        // Keep the parent identity stable while an already removed child directory is open.
        if self.nodes.iter().enumerate().any(|(child, n)| {
            n.parent == node
                && matches!(n.data, Data::Directory)
                && self
                    .opens
                    .iter()
                    .flatten()
                    .any(|o| matches!(o.target, Target::Node(k) if k == child))
        }) {
            return Err(BUSY);
        }
        if self.nodes[node].mount == Mount::Persistent {
            self.disk
                .as_mut()
                .ok_or(IO_ERROR)?
                .rmdir(self.nodes[parent].inode.unwrap(), name)?;
        }
        self.nodes[node].linked = false;
        self.touch_node(parent, true);
        self.touch_node(node, false);
        if !self
            .opens
            .iter()
            .flatten()
            .any(|o| matches!(o.target, Target::Node(n) if n == node))
            && !self.nodes[node]
                .mapping
                .as_ref()
                .is_some_and(MappedFile::active)
        {
            self.reap_after_unlink(node)?;
        }
        Ok(())
    }
    pub fn unlink(&mut self, cwd: usize, path: &str) -> Result<(), i64> {
        let (parent, name) = self.parent(cwd, path)?;
        let node = self.lookup_child(parent, name, &[])?.ok_or(NOT_FOUND)?;
        self.mutable_node(node)?;
        if !matches!(self.nodes[node].mount, Mount::Tmpfs | Mount::Persistent) {
            return Err(READ_ONLY);
        }
        if matches!(self.nodes[node].data, Data::Directory) {
            return Err(IS_DIRECTORY);
        }
        if self.nodes[node].mount == Mount::Persistent {
            self.disk
                .as_mut()
                .ok_or(IO_ERROR)?
                .unlink(self.nodes[parent].inode.unwrap(), name)?;
        }
        self.nodes[node].linked = false;
        self.touch_node(parent, true);
        self.touch_node(node, false);
        if !self
            .opens
            .iter()
            .flatten()
            .any(|o| matches!(o.target, Target::Node(n) if n == node))
            && !self.nodes[node]
                .mapping
                .as_ref()
                .is_some_and(MappedFile::active)
        {
            self.reap_after_unlink(node)?;
        }
        Ok(())
    }
    /// Rename with the backend's directory-move contract; callers with live cwd references supply them below.
    pub fn rename(&mut self, cwd: usize, from: &str, to: &str) -> Result<(), i64> {
        self.rename_with_cwds(cwd, from, to, &[])
    }
    /// Audited ext4 supports directory moves/replacement; ext2 and tmpfs retain their original subset.
    pub fn rename_with_cwds(
        &mut self,
        cwd: usize,
        from: &str,
        to: &str,
        cwd_references: &[usize],
    ) -> Result<(), i64> {
        let (old_parent, old_name) = self.parent(cwd, from)?;
        let (parent, name) = self.parent(cwd, to)?;
        let node = self
            .lookup_child(old_parent, old_name, &[])?
            .ok_or(NOT_FOUND)?;
        self.mutable_node(node)?;
        let mount = self.nodes[node].mount;
        if !matches!(mount, Mount::Tmpfs | Mount::Persistent) || self.nodes[parent].mount != mount {
            return Err(READ_ONLY);
        }
        let target = self.lookup_child(parent, name, &[node])?;
        if let Some(target) = target {
            self.mutable_node(target)?;
        }
        if target == Some(node) {
            return Ok(());
        }
        let directory = matches!(self.nodes[node].data, Data::Directory);
        let directory_moves =
            mount == Mount::Persistent && self.disk.as_ref().is_some_and(|d| d.directory_moves());
        if directory && (parent != old_parent || target.is_some()) && !directory_moves {
            return Err(UNSUPPORTED);
        }
        if directory_moves && directory {
            let mut ancestor = parent;
            loop {
                if ancestor == node {
                    return Err(INVALID_ARGUMENT);
                }
                if ancestor == ROOT {
                    break;
                }
                ancestor = self.nodes[ancestor].parent;
            }
        }
        if let Some(target) = target {
            let target_directory = matches!(self.nodes[target].data, Data::Directory);
            if target_directory != directory {
                return Err(if target_directory {
                    IS_DIRECTORY
                } else {
                    NOT_DIRECTORY
                });
            }
            if target_directory {
                if cwd_references.contains(&target) {
                    return Err(BUSY);
                }
                if self.nodes.iter().any(|n| n.linked && n.parent == target)
                    || self
                        .disk
                        .as_mut()
                        .ok_or(IO_ERROR)?
                        .next_entry(self.nodes[target].inode.unwrap(), 0)?
                        .is_some()
                {
                    return Err(NOT_EMPTY);
                }
                if self.nodes.iter().enumerate().any(|(child, n)| {
                    n.parent == target
                        && matches!(n.data, Data::Directory)
                        && self
                            .opens
                            .iter()
                            .flatten()
                            .any(|o| matches!(o.target, Target::Node(k) if k == child))
                }) {
                    return Err(BUSY);
                }
            }
        }
        if mount == Mount::Persistent {
            self.disk.as_mut().ok_or(IO_ERROR)?.rename(
                self.nodes[old_parent].inode.unwrap(),
                old_name,
                self.nodes[parent].inode.unwrap(),
                name,
            )?;
        }
        if let Some(target) = target {
            self.nodes[target].linked = false;
            if !self
                .opens
                .iter()
                .flatten()
                .any(|o| matches!(o.target, Target::Node(n) if n == target))
                && !self.nodes[target]
                    .mapping
                    .as_ref()
                    .is_some_and(MappedFile::active)
            {
                self.mapping_cache = true;
            }
        }
        let n = &mut self.nodes[node];
        n.parent = parent;
        n.name.fill(0);
        n.name[..name.len()].copy_from_slice(name.as_bytes());
        n.len = name.len();
        self.touch_node(node, false);
        self.touch_node(old_parent, true);
        self.touch_node(parent, true);
        if let Err(error) = self.collect_mappings()
            && error != NO_MEMORY
        {
            return Err(error);
        }
        Ok(())
    }
    /// Advisory locks belong to open descriptions, survive dup/fork and disappear on final close.
    pub fn try_lock(&mut self, fds: &Descriptors, fd: u64, mode: u8) -> Result<(), i64> {
        if mode > 2 {
            return Err(INVALID_ARGUMENT);
        }
        let handle = fds.handle(fd)?;
        let open = self.opens[handle].ok_or(BAD_DESCRIPTOR)?;
        let Target::Node(node) = open.target else {
            return Err(UNSUPPORTED);
        };
        if matches!(self.nodes[node].data, Data::Directory | Data::Device(_)) {
            return Err(UNSUPPORTED);
        }
        // Converting flock modes releases the previous mode before testing contention.
        if open.lock == mode {
            return Ok(());
        }
        self.opens[handle].as_mut().unwrap().lock = 0;
        if mode != 0
            && self.opens.iter().enumerate().any(|(k, o)| {
                k != handle
                    && o.is_some_and(|o| {
                        matches!(o.target, Target::Node(n) if n == node)
                            && o.lock != 0
                            && (mode == 2 || o.lock == 2)
                    })
            })
        {
            return Err(WOULD_BLOCK);
        }
        self.opens[handle].as_mut().unwrap().lock = mode;
        Ok(())
    }
    /// Report queued pipe bytes or remaining regular-file bytes without consuming the open offset.
    pub fn available_bytes(&self, fds: &Descriptors, fd: u64) -> Result<usize, i64> {
        let open = self.opens[fds.handle(fd)?].ok_or(BAD_DESCRIPTOR)?;
        match open.target {
            Target::ReadPipe(n) | Target::WritePipe(n) => {
                Ok(self.pipes[n].as_ref().ok_or(BAD_DESCRIPTOR)?.len)
            }
            Target::Packet { input, .. } => {
                let p = self.pipes[input].as_ref().unwrap();
                Ok(if p.len == 0 {
                    0
                } else {
                    Self::packet_length(p)
                })
            }
            Target::Node(n) => match &self.nodes[n].data {
                Data::Borrowed(b, _) => Ok(b.len().saturating_sub(open.offset)),
                Data::Mutable(b) => Ok(b.len().saturating_sub(open.offset)),
                Data::Persistent { size, .. } => Ok(size.saturating_sub(open.offset)),
                _ => Err(UNSUPPORTED),
            },
            _ => Err(UNSUPPORTED),
        }
    }
    /// Positioned regular-file I/O never publishes a changed shared open-description offset.
    /// The single BSP owner cannot yield while the temporary internal cursor is installed.
    pub fn read_at(
        &mut self,
        fds: &Descriptors,
        fd: u64,
        offset: u64,
        output: &mut [u8],
    ) -> Result<usize, i64> {
        let handle = fds.handle(fd)?;
        let old = self.opens[handle].ok_or(BAD_DESCRIPTOR)?;
        if !matches!(old.target, Target::Node(n) if matches!(self.nodes[n].data, Data::Borrowed(..) | Data::Mutable(_) | Data::Persistent {..}))
        {
            return Err(NOT_SEEKABLE);
        }
        self.opens[handle].as_mut().unwrap().offset =
            usize::try_from(offset).map_err(|_| INVALID_ARGUMENT)?;
        let result = self.read(fds, fd, output);
        self.opens[handle].as_mut().unwrap().offset = old.offset;
        result
    }
    pub fn write_at(
        &mut self,
        fds: &Descriptors,
        fd: u64,
        offset: u64,
        input: &[u8],
    ) -> Result<usize, i64> {
        let handle = fds.handle(fd)?;
        let old = self.opens[handle].ok_or(BAD_DESCRIPTOR)?;
        if !matches!(old.target, Target::Node(n) if matches!(self.nodes[n].data, Data::Borrowed(..) | Data::Mutable(_) | Data::Persistent {..}))
        {
            return Err(NOT_SEEKABLE);
        }
        self.opens[handle].as_mut().unwrap().offset =
            usize::try_from(offset).map_err(|_| INVALID_ARGUMENT)?;
        let result = self.write(fds, fd, input);
        self.opens[handle].as_mut().unwrap().offset = old.offset;
        result
    }
    /// Return the backing limit of a writable regular file, independently of tmpfs/cache limits.
    pub fn writable_file_limit(&self, fds: &Descriptors, fd: u64) -> Result<usize, i64> {
        self.device(fds, fd, true)?;
        let open = self.opens[fds.handle(fd)?].ok_or(BAD_DESCRIPTOR)?;
        let Target::Node(node) = open.target else {
            return Err(INVALID_ARGUMENT);
        };
        self.mutable_node(node)?;
        match self.nodes[node].data {
            Data::Mutable(_) => Ok(FILE_BYTES),
            Data::Persistent { .. } => Ok(self.persistent_limit()),
            Data::Directory => Err(IS_DIRECTORY),
            _ => Err(READ_ONLY),
        }
    }
    /// Set regular-file length without modifying any shared open-description offset.
    pub fn set_len(&mut self, fds: &Descriptors, fd: u64, length: usize) -> Result<(), i64> {
        self.device(fds, fd, true)?;
        let open = self.opens[fds.handle(fd)?].ok_or(BAD_DESCRIPTOR)?;
        let Target::Node(node) = open.target else {
            return Err(INVALID_ARGUMENT);
        };
        self.mutable_node(node)?;
        let inode = self.nodes[node].inode;
        if self.nodes[node]
            .mapping
            .as_ref()
            .is_some_and(|m| m.active() && length < m.length())
        {
            return Err(BUSY);
        }
        if length <= self.writable_file_limit(fds, fd)? {
            self.prepare_mapping_growth(node, length)?;
        }
        match &mut self.nodes[node].data {
            Data::Mutable(bytes) => {
                if length > FILE_BYTES {
                    return Err(NO_SPACE);
                }
                if length > bytes.len() {
                    bytes
                        .try_reserve(length - bytes.len())
                        .map_err(|_| NO_MEMORY)?;
                }
                bytes.resize(length, 0);
            }
            Data::Persistent { size, .. } => {
                if *size == length {
                    // The backing resize is a no-op; keep its cached inode timestamps unchanged.
                    return Ok(());
                }
                self.disk
                    .as_mut()
                    .ok_or(IO_ERROR)?
                    .resize(inode.unwrap(), length)?;
                *size = length;
            }
            Data::Directory => return Err(IS_DIRECTORY),
            _ => return Err(READ_ONLY),
        }
        let timestamp = self.nodes[node].mount.timestamp(self.clock);
        self.nodes[node].modified = timestamp;
        self.nodes[node].changed = timestamp;
        if let Some(mapping) = &self.nodes[node].mapping {
            mapping.resize(length);
        }
        Ok(())
    }
    /// A live shared cache cannot cover larger files; idle caches can be discarded before growth.
    fn prepare_mapping_growth(&mut self, node: usize, length: usize) -> Result<(), i64> {
        if length > FILE_BYTES {
            if self.nodes[node]
                .mapping
                .as_ref()
                .is_some_and(MappedFile::active)
            {
                return Err(BUSY);
            }
            self.nodes[node].mapping = None;
        }
        Ok(())
    }
    pub fn executable(&mut self, cwd: usize, path: &str) -> Result<&'a [u8], i64> {
        let node = self.resolve(cwd, path)?;
        match self.nodes[node].data {
            Data::Borrowed(bytes, true) => Ok(bytes),
            _ => Err(PERMISSION_DENIED),
        }
    }
    /// Image policy comes from the archive record, never an ELF-header heuristic.
    pub fn linux_executable(&mut self, cwd: usize, path: &str) -> Result<bool, i64> {
        let node = self.resolve(cwd, path)?;
        Ok(self.linux[node])
    }
    /// Return backend-neutral inode, kind, size and executable metadata.
    pub fn metadata(&mut self, cwd: usize, path: &str) -> Result<(u64, u32, usize, bool), i64> {
        let node = self.resolve(cwd, path)?;
        self.node_metadata(node)
    }
    pub fn writable(&mut self, cwd: usize, path: &str) -> Result<bool, i64> {
        let n = self.resolve(cwd, path)?;
        Ok(self.policy_node != Some(n)
            && (self.nodes[n].mount == Mount::Tmpfs
                || self.nodes[n].mount == Mount::Persistent && self.persistent_writable()))
    }
    fn mutable_node(&self, node: usize) -> Result<(), i64> {
        if self.policy_node == Some(node)
            || self.nodes[node].mount == Mount::Persistent && !self.persistent_writable()
        {
            Err(READ_ONLY)
        } else {
            Ok(())
        }
    }
    pub fn read_link(&mut self, cwd: usize, path: &str) -> Result<&'a str, i64> {
        let (parent, name) = self.parent(cwd, path)?;
        let node = self.lookup_child(parent, name, &[])?.ok_or(NOT_FOUND)?;
        match self.nodes[node].data {
            Data::Link(target) => Ok(target),
            _ => Err(INVALID_ARGUMENT),
        }
    }
    pub fn metadata_nofollow(
        &mut self,
        cwd: usize,
        path: &str,
    ) -> Result<(u64, u32, usize, bool), i64> {
        if path.ends_with('/') || path == "." || path == ".." {
            return self.metadata(cwd, path);
        }
        let (parent, name) = self.parent(cwd, path)?;
        let node = self.lookup_child(parent, name, &[])?.ok_or(NOT_FOUND)?;
        self.node_metadata(node)
    }
    fn node_metadata(&self, n: usize) -> Result<(u64, u32, usize, bool), i64> {
        let (kind, size, executable) = match &self.nodes[n].data {
            Data::Directory => (2, 0, true),
            Data::Borrowed(b, x) => (1, b.len(), *x),
            Data::Mutable(b) => (1, b.len(), false),
            Data::Persistent { size, executable } => (1, *size, *executable),
            Data::Link(b) => (3, b.len(), false),
            Data::Device(_) => (4, 0, false),
        };
        Ok((self.identity(n), kind, size, executable))
    }
    pub fn descriptor_metadata(
        &self,
        fds: &Descriptors,
        fd: u64,
    ) -> Result<(u64, u32, usize, bool), i64> {
        match self.opens[fds.handle(fd)?].unwrap().target {
            Target::Node(n) => self.node_metadata(n),
            Target::Socket(id) => Ok(((NODE_LIMIT + PIPE_LIMIT + id) as u64, 6, 0, false)),
            Target::Packet { input, .. } => Ok(((NODE_LIMIT + input) as u64, 6, 0, false)),
            Target::ReadPipe(p) | Target::WritePipe(p) => {
                Ok(((NODE_LIMIT + p) as u64, 5, 0, false))
            }
        }
    }

    /// Snapshot private file mappings independently of the open offset; no writeback is implied.
    pub fn mapping_bytes(
        &mut self,
        fds: &Descriptors,
        fd: u64,
        offset: u64,
        length: u64,
    ) -> Result<Cow<'a, [u8]>, i64> {
        let open = self.opens[fds.handle(fd)?].unwrap();
        if open.flags & OPEN_READ == 0 {
            return Err(BAD_DESCRIPTOR);
        }
        let Target::Node(node) = open.target else {
            return Err(INVALID_ARGUMENT);
        };
        let start = usize::try_from(offset).map_err(|_| INVALID_ARGUMENT)?;
        let size = self.node_metadata(node)?.2;
        if start > size {
            return Err(INVALID_ARGUMENT);
        }
        let end = size.min(start.saturating_add(length as usize));
        if let Data::Borrowed(bytes, _) = self.nodes[node].data {
            return Ok(Cow::Borrowed(&bytes[start..end]));
        }
        if !matches!(
            self.nodes[node].data,
            Data::Mutable(_) | Data::Persistent { .. }
        ) {
            return Err(UNSUPPORTED);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(end - start)
            .map_err(|_| NO_MEMORY)?;
        bytes.resize(end - start, 0);
        match &self.nodes[node].data {
            Data::Mutable(data) => bytes.copy_from_slice(&data[start..end]),
            Data::Persistent { .. } => {
                let n = self.disk.as_mut().ok_or(IO_ERROR)?.read(
                    self.nodes[node].inode.unwrap(),
                    start,
                    &mut bytes,
                )?;
                if n != bytes.len() {
                    return Err(IO_ERROR);
                }
            }
            _ => unreachable!(),
        }
        Ok(Cow::Owned(bytes))
    }
    /// Readonly shared maps retain file identity and observe later successful descriptor writes.
    /// Shrinking a live mapped file and writable shared maps remain outside this bounded profile.
    pub fn shared_file_mapping(
        &mut self,
        fds: &Descriptors,
        fd: u64,
        offset: u64,
        size: u64,
    ) -> Result<MappedFile, i64> {
        let open = self.opens[fds.handle(fd)?].ok_or(BAD_DESCRIPTOR)?;
        if open.flags & OPEN_READ == 0 {
            return Err(BAD_DESCRIPTOR);
        }
        let Target::Node(node) = open.target else {
            return Err(INVALID_ARGUMENT);
        };
        let (_, kind, length, _) = self.node_metadata(node)?;
        if kind != 1 || length > FILE_BYTES {
            return Err(UNSUPPORTED);
        }
        if offset >= length as u64
            || offset
                .checked_add(size)
                .is_none_or(|end| end > (length as u64).next_multiple_of(4096))
        {
            return Err(INVALID_ARGUMENT);
        }
        if self.nodes[node].mapping.is_none() {
            let bytes = self.mapping_bytes(fds, fd, 0, length as u64)?;
            self.nodes[node].mapping = Some(MappedFile::new(&bytes)?);
        }
        self.mapping_cache = true;
        Ok(self.nodes[node].mapping.as_ref().unwrap().clone())
    }
    /// Namespace publication succeeded; deferred allocation-only reclamation cannot turn it into failure.
    fn reap_after_unlink(&mut self, node: usize) -> Result<(), i64> {
        match self.reap_node(node) {
            Err(NO_MEMORY) => Ok(()),
            result => result,
        }
    }
    /// Keep an orphan's identity pinned if fallible final-close reclamation needs a later retry.
    fn reap_node(&mut self, node: usize) -> Result<(), i64> {
        if let Some(inode) = self.nodes[node].inode {
            if let Err(error) = self.disk.as_mut().ok_or(IO_ERROR)?.release_unlinked(inode) {
                self.mapping_cache = true;
                return Err(error);
            }
            self.nodes[node].inode = None;
        }
        self.nodes[node].data = Data::Mutable(Vec::new());
        Ok(())
    }
    /// Drop inactive maps and retry orphan reclamation after allocation failures or final unmap.
    pub fn collect_mappings(&mut self) -> Result<(), i64> {
        if !self.mapping_cache {
            return Ok(());
        }
        self.mapping_cache = false;
        for node in 0..self.nodes.len() {
            if self.nodes[node]
                .mapping
                .as_ref()
                .is_some_and(MappedFile::active)
            {
                self.mapping_cache = true;
                continue;
            }
            self.nodes[node].mapping = None;
            if !self.nodes[node].linked && !self.held(node) {
                self.reap_node(node)?;
            }
        }
        Ok(())
    }
    pub fn pty_present(&self, n: usize, master: bool) -> bool {
        self.opens.iter().flatten().any(|o|match o.target {
            Target::Node(node)=>matches!(self.nodes[node].data,Data::Device(d) if d==if master {Device::PtyMaster(n)} else {Device::PtySlave(n)}),_=>false
        })
    }
    pub fn pty_lock(&mut self, n: usize, locked: bool) {
        self.pty_unlocked[n] = !locked;
    }
    /// Publish an opaque socket in the same shared open-description namespace as files.
    pub fn socket(&mut self, fds: &mut Descriptors, socket: usize) -> Result<u64, i64> {
        let fd = fds.free()?;
        fds.ensure()?;
        let id = self.allocate(Open {
            target: Target::Socket(socket),
            offset: 0,
            flags: OPEN_READ | OPEN_WRITE,
            refs: 1,
            lock: 0,
        })?;
        fds.table.as_ref().unwrap().borrow_mut().slots[fd] = Some(id);
        Ok(fd as u64)
    }
    pub fn socket_id(&self, fds: &Descriptors, fd: u64) -> Result<Option<usize>, i64> {
        Ok(match self.opens[fds.handle(fd)?].unwrap().target {
            Target::Socket(id) => Some(id),
            _ => None,
        })
    }
    pub fn socket_live(&self, socket: usize) -> bool {
        self.opens
            .iter()
            .flatten()
            .any(|o| matches!(o.target, Target::Socket(id) if id == socket))
    }
    /// A bounded full-duplex sequenced-packet pair owns two queues from the ordinary pipe pool.
    /// Headers preserve message boundaries; a short receive discards the rest of that message.
    pub fn packet_pair(&mut self, fds: &mut Descriptors) -> Result<[u64; 2], i64> {
        fds.ensure()?;
        let slots = fds.slots();
        let mut free = slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_none())
            .map(|(i, _)| i);
        let descriptors = [
            free.next().ok_or(FILE_LIMIT)?,
            free.next().ok_or(FILE_LIMIT)?,
        ];
        if self.opens.iter().filter(|s| s.is_none()).count() < 2 {
            return Err(FILE_LIMIT);
        }
        let mut free = self
            .pipes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_none())
            .map(|(i, _)| i);
        let a = free.next().ok_or(FILE_LIMIT)?;
        let b = free.next().ok_or(FILE_LIMIT)?;
        // All capacity checks precede publication; the BSP cannot interleave another allocator.
        for p in [a, b] {
            self.pipes[p] = Some(Pipe {
                bytes: [0; PIPE_BYTES],
                head: 0,
                len: 0,
                readers: 1,
                writers: 1,
            });
        }
        for (fd, input, output) in [(descriptors[0], a, b), (descriptors[1], b, a)] {
            let id = self.allocate(Open {
                target: Target::Packet { input, output },
                offset: 0,
                flags: OPEN_READ | OPEN_WRITE,
                refs: 1,
                lock: 0,
            })?;
            fds.table.as_ref().unwrap().borrow_mut().slots[fd] = Some(id);
        }
        Ok([descriptors[0] as u64, descriptors[1] as u64])
    }
    pub fn is_packet(&self, fds: &Descriptors, fd: u64) -> bool {
        fds.handle(fd)
            .ok()
            .is_some_and(|id| matches!(self.opens[id].unwrap().target, Target::Packet { .. }))
    }
    fn packet_length(p: &Pipe) -> usize {
        u16::from_le_bytes([p.bytes[p.head], p.bytes[(p.head + 1) % PIPE_BYTES]]) as usize
    }
    pub fn readiness(&self, fds: &Descriptors, fd: u64) -> Result<(bool, bool, bool, bool), i64> {
        let open = self.opens[fds.handle(fd)?].unwrap();
        Ok(match open.target {
            Target::ReadPipe(p) => {
                let p = self.pipes[p].as_ref().unwrap();
                (p.len > 0 || p.writers == 0, false, p.writers == 0, false)
            }
            Target::WritePipe(p) => {
                let p = self.pipes[p].as_ref().unwrap();
                (false, p.len < PIPE_BYTES, false, p.readers == 0)
            }
            Target::Packet { input, output } => {
                let a = self.pipes[input].as_ref().unwrap();
                let b = self.pipes[output].as_ref().unwrap();
                (
                    a.len != 0 || a.writers == 0,
                    b.len <= PACKET_BYTES,
                    a.writers == 0,
                    b.readers == 0,
                )
            }
            _ => (
                open.flags & OPEN_READ != 0,
                open.flags & OPEN_WRITE != 0,
                false,
                false,
            ),
        })
    }
    pub fn status_flags(&self, fds: &Descriptors, fd: u64) -> Result<u64, i64> {
        Ok(self.opens[fds.handle(fd)?].unwrap().flags)
    }
    pub fn set_append(&mut self, fds: &Descriptors, fd: u64, append: bool) -> Result<(), i64> {
        let open = self.opens[fds.handle(fd)?].as_mut().unwrap();
        open.flags = (open.flags & !OPEN_APPEND) | if append { OPEN_APPEND } else { 0 };
        Ok(())
    }
    pub fn nonblocking(&self, fds: &Descriptors, fd: u64) -> Result<bool, i64> {
        Ok(self.opens[fds.handle(fd)?].unwrap().flags & (1 << 20) != 0)
    }
    pub fn set_nonblocking(&mut self, fds: &Descriptors, fd: u64, value: bool) -> Result<(), i64> {
        let open = self.opens[fds.handle(fd)?].as_mut().unwrap();
        open.flags = (open.flags & !(1 << 20)) | if value { 1 << 20 } else { 0 };
        Ok(())
    }
    pub fn standard(&mut self) -> Result<Descriptors, i64> {
        let mut fds = Descriptors::default();
        for flags in [OPEN_READ, OPEN_WRITE, OPEN_WRITE] {
            if let Err(e) = self.open(&mut fds, ROOT, "/dev/console", flags) {
                self.clear(&mut fds);
                return Err(e);
            }
        }
        Ok(fds)
    }
    fn allocate(&mut self, open: Open) -> Result<usize, i64> {
        let id = self
            .opens
            .iter()
            .position(Option::is_none)
            .ok_or(FILE_LIMIT)?;
        self.opens[id] = Some(open);
        Ok(id)
    }
    pub fn open(
        &mut self,
        fds: &mut Descriptors,
        cwd: usize,
        path: &str,
        flags: u64,
    ) -> Result<u64, i64> {
        if flags & !OPEN_FLAGS != 0
            || flags & (OPEN_READ | OPEN_WRITE) == 0
            || flags & (OPEN_TRUNCATE | OPEN_APPEND) != 0 && flags & OPEN_WRITE == 0
            || flags & OPEN_EXCLUSIVE != 0 && flags & OPEN_CREATE == 0
        {
            return Err(INVALID_ARGUMENT);
        }
        let fd = fds.free()?;
        fds.ensure()?;
        if self.opens.iter().all(Option::is_some) {
            return Err(FILE_LIMIT);
        }
        let mut node = match self.resolve(cwd, path) {
            Ok(_) if flags & OPEN_EXCLUSIVE != 0 => return Err(ALREADY_EXISTS),
            Ok(id) => id,
            Err(NOT_FOUND) if flags & OPEN_CREATE != 0 => {
                let (parent, name) = self.parent(cwd, path)?;
                if flags & OPEN_DIRECTORY != 0 {
                    return Err(NOT_DIRECTORY);
                }
                if !matches!(self.nodes[parent].mount, Mount::Tmpfs | Mount::Persistent) {
                    return Err(READ_ONLY);
                }
                self.ensure_node_slot(&[])?;
                if self.nodes[parent].mount == Mount::Persistent {
                    let inode = self.disk.as_mut().ok_or(IO_ERROR)?.create(
                        self.nodes[parent].inode.unwrap(),
                        name,
                        false,
                    )?;
                    let node = self.insert(
                        parent,
                        name,
                        Data::Persistent {
                            size: 0,
                            executable: false,
                        },
                        Mount::Persistent,
                    )?;
                    self.nodes[node].inode = Some(inode);
                    self.touch_node(parent, true);
                    node
                } else {
                    self.insert(parent, name, Data::Mutable(Vec::new()), Mount::Tmpfs)?
                }
            }
            Err(e) => return Err(e),
        };
        if matches!(self.nodes[node].data, Data::Device(Device::Ptmx)) {
            let n = (0..4)
                .find(|&n| !self.pty_present(n, true))
                .ok_or(FILE_LIMIT)?;
            node = self
                .nodes
                .iter()
                .position(|v| matches!(v.data,Data::Device(Device::PtyMaster(k)) if k==n))
                .ok_or(NOT_FOUND)?;
            self.pty_unlocked[n] = false;
        }
        if let Data::Device(Device::PtySlave(n)) = self.nodes[node].data
            && (!self.pty_present(n, true) || !self.pty_unlocked[n])
        {
            return Err(IO_ERROR);
        }
        if flags & OPEN_DIRECTORY != 0 && !matches!(self.nodes[node].data, Data::Directory) {
            return Err(NOT_DIRECTORY);
        }
        if flags & OPEN_WRITE != 0 {
            self.mutable_node(node)?;
            match self.nodes[node].data {
                Data::Directory => return Err(IS_DIRECTORY),
                Data::Borrowed(..) | Data::Link(..) => return Err(READ_ONLY),
                _ => {}
            }
        }
        if flags & OPEN_TRUNCATE != 0 {
            if self.nodes[node]
                .mapping
                .as_ref()
                .is_some_and(MappedFile::active)
            {
                return Err(BUSY);
            }
            let inode = self.nodes[node].inode;
            if let Data::Mutable(ref mut bytes) = self.nodes[node].data {
                bytes.clear();
            } else if let Data::Persistent { ref mut size, .. } = self.nodes[node].data {
                self.disk
                    .as_mut()
                    .ok_or(IO_ERROR)?
                    .truncate(inode.unwrap())?;
                *size = 0;
            } else if !matches!(self.nodes[node].data, Data::Device(_)) {
                return Err(INVALID_ARGUMENT);
            }
            let timestamp = self.nodes[node].mount.timestamp(self.clock);
            self.nodes[node].modified = timestamp;
            self.nodes[node].changed = timestamp;
            if let Some(mapping) = &self.nodes[node].mapping {
                mapping.resize(0);
            }
        }
        let id = self.allocate(Open {
            target: Target::Node(node),
            offset: 0,
            flags,
            refs: 1,
            lock: 0,
        })?;
        fds.table.as_ref().unwrap().borrow_mut().slots[fd] = Some(id);
        Ok(fd as u64)
    }
    pub fn inherit(&mut self, parent: &Descriptors) -> Result<Descriptors, i64> {
        let Some(table) = parent.table.as_ref() else {
            return Ok(Descriptors::default());
        };
        let table = *table.borrow();
        let inherited = Descriptors::from_table(table)?;
        for id in table.slots.iter().flatten() {
            self.opens[*id].as_mut().unwrap().refs += 1;
        }
        Ok(inherited)
    }
    pub fn close(&mut self, fds: &mut Descriptors, fd: u64) -> Result<(), i64> {
        let mut cleanup = Ok(());
        let id = fds.handle(fd)?;
        fds.table.as_ref().unwrap().borrow_mut().slots[fd as usize] = None;
        fds.table.as_ref().unwrap().borrow_mut().close_on_exec[fd as usize] = false;
        let open = self.opens[id].as_mut().unwrap();
        open.refs -= 1;
        if open.refs == 0 {
            match open.target {
                Target::ReadPipe(p) | Target::WritePipe(p) => {
                    let pipe = self.pipes[p].as_mut().unwrap();
                    if matches!(open.target, Target::ReadPipe(_)) {
                        pipe.readers -= 1;
                    } else {
                        pipe.writers -= 1;
                    }
                    if pipe.readers == 0 && pipe.writers == 0 {
                        self.pipes[p] = None;
                    }
                }
                Target::Packet { input, output } => {
                    self.pipes[input].as_mut().unwrap().readers -= 1;
                    self.pipes[output].as_mut().unwrap().writers -= 1;
                    for p in [input, output] {
                        let pipe = self.pipes[p].as_ref().unwrap();
                        if pipe.readers == 0 && pipe.writers == 0 {
                            self.pipes[p] = None;
                        }
                    }
                }
                Target::Node(n) if !self.nodes[n].linked => {
                    if !self.opens.iter().enumerate().any(|(other, o)| {
                        other != id
                            && o.is_some_and(
                                |o| matches!(o.target, Target::Node(node) if node == n),
                            )
                    }) && !self.nodes[n]
                        .mapping
                        .as_ref()
                        .is_some_and(MappedFile::active)
                    {
                        cleanup = self.reap_node(n);
                    }
                }
                _ => {}
            }
            self.opens[id] = None;
        }
        cleanup
    }
    pub fn close_exec(&mut self, fds: &mut Descriptors) {
        if fds.table.is_none() {
            return;
        }
        for i in 0..fd::FD_LIMIT {
            let close = fds.table.as_ref().unwrap().borrow().close_on_exec[i];
            if close {
                let _ = self.close(fds, i as u64);
            }
        }
    }
    pub fn clear(&mut self, fds: &mut Descriptors) {
        if fds.table.is_none() {
            return;
        }
        if fds.shared() {
            *fds = Descriptors::default();
            return;
        }
        for i in 0..fd::FD_LIMIT {
            let present = fds.table.as_ref().unwrap().borrow().slots[i].is_some();
            if present {
                // Descriptor ownership is released even if a poisoned disk reports a cleanup error.
                let _ = self.close(fds, i as u64);
            }
        }
    }
    pub fn dup2(&mut self, fds: &mut Descriptors, source: u64, target: u64) -> Result<u64, i64> {
        let id = fds.handle(source)?;
        if target >= fd::FD_LIMIT as u64 {
            return Err(BAD_DESCRIPTOR);
        }
        if source == target {
            return Ok(target);
        }
        if fds.contains(target) {
            self.close(fds, target)?;
        }
        self.opens[id].as_mut().unwrap().refs += 1;
        fds.table.as_ref().unwrap().borrow_mut().slots[target as usize] = Some(id);
        fds.table.as_ref().unwrap().borrow_mut().close_on_exec[target as usize] = false;
        Ok(target)
    }
    pub fn pipe(&mut self, fds: &mut Descriptors) -> Result<[u64; 2], i64> {
        fds.ensure()?;
        let slots = fds.slots();
        let mut free = slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_none())
            .map(|(i, _)| i);
        let r = free.next().ok_or(FILE_LIMIT)?;
        let w = free.next().ok_or(FILE_LIMIT)?;
        if self.opens.iter().filter(|s| s.is_none()).count() < 2 {
            return Err(FILE_LIMIT);
        }
        let p = self
            .pipes
            .iter()
            .position(Option::is_none)
            .ok_or(FILE_LIMIT)?;
        let a = self.allocate(Open {
            target: Target::ReadPipe(p),
            offset: 0,
            flags: OPEN_READ,
            refs: 1,
            lock: 0,
        })?;
        let b = self.allocate(Open {
            target: Target::WritePipe(p),
            offset: 0,
            flags: OPEN_WRITE,
            refs: 1,
            lock: 0,
        })?;
        self.pipes[p] = Some(Pipe {
            bytes: [0; PIPE_BYTES],
            head: 0,
            len: 0,
            readers: 1,
            writers: 1,
        });
        fds.table.as_ref().unwrap().borrow_mut().slots[r] = Some(a);
        fds.table.as_ref().unwrap().borrow_mut().slots[w] = Some(b);
        Ok([r as u64, w as u64])
    }
    pub fn device(&self, fds: &Descriptors, fd: u64, writing: bool) -> Result<Option<Device>, i64> {
        let open = self.opens[fds.handle(fd)?].unwrap();
        if open.flags & if writing { OPEN_WRITE } else { OPEN_READ } == 0 {
            return Err(BAD_DESCRIPTOR);
        }
        Ok(match open.target {
            Target::Node(n) => match self.nodes[n].data {
                Data::Device(d) => Some(d),
                _ => None,
            },
            _ => None,
        })
    }
    pub fn ready(&self, fds: &Descriptors, fd: u64, writing: bool) -> bool {
        let Ok(id) = fds.handle(fd) else {
            return true;
        };
        match self.opens[id].unwrap().target {
            Target::ReadPipe(p) if !writing => {
                let p = self.pipes[p].as_ref().unwrap();
                p.len > 0 || p.writers == 0
            }
            Target::WritePipe(p) if writing => {
                let p = self.pipes[p].as_ref().unwrap();
                p.len < PIPE_BYTES || p.readers == 0
            }
            Target::Packet { input, output } => {
                let p = self.pipes[if writing { output } else { input }]
                    .as_ref()
                    .unwrap();
                if writing {
                    p.len == 0 || p.readers == 0
                } else {
                    p.len != 0 || p.writers == 0
                }
            }
            _ => true,
        }
    }
    pub fn read(&mut self, fds: &Descriptors, fd: u64, output: &mut [u8]) -> Result<usize, i64> {
        self.device(fds, fd, false)?;
        let id = fds.handle(fd)?;
        let open = self.opens[id].unwrap();
        if let Target::Packet { input, .. } = open.target {
            let p = self.pipes[input].as_mut().unwrap();
            if p.len == 0 {
                return if p.writers == 0 {
                    Ok(0)
                } else {
                    Err(WOULD_BLOCK)
                };
            }
            let length = Self::packet_length(p);
            let count = length.min(output.len());
            for (i, byte) in output[..count].iter_mut().enumerate() {
                *byte = p.bytes[(p.head + 2 + i) % PIPE_BYTES];
            }
            p.head = (p.head + length + 2) % PIPE_BYTES;
            p.len -= length + 2;
            return Ok(count);
        }
        if output.is_empty() {
            return Ok(0);
        }
        let count = match open.target {
            Target::ReadPipe(p) => {
                let p = self.pipes[p].as_mut().unwrap();
                if p.len == 0 && p.writers > 0 {
                    return Err(WOULD_BLOCK);
                }
                let n = output.len().min(p.len);
                for c in &mut output[..n] {
                    *c = p.bytes[p.head];
                    p.head = (p.head + 1) % PIPE_BYTES;
                    p.len -= 1;
                }
                n
            }
            Target::Node(n) => {
                if let Data::Persistent { .. } = self.nodes[n].data {
                    let count = self.disk.as_mut().ok_or(IO_ERROR)?.read(
                        self.nodes[n].inode.unwrap(),
                        open.offset,
                        output,
                    )?;
                    self.opens[id].as_mut().unwrap().offset += count;
                    return Ok(count);
                }
                let bytes = match &self.nodes[n].data {
                    Data::Borrowed(bytes, _) => *bytes,
                    Data::Mutable(bytes) => bytes,
                    Data::Device(Device::Null) => return Ok(0),
                    Data::Directory => return Err(IS_DIRECTORY),
                    _ => return Err(INVALID_ARGUMENT),
                };
                let start = open.offset.min(bytes.len());
                let n = output.len().min(bytes.len() - start);
                output[..n].copy_from_slice(&bytes[start..start + n]);
                n
            }
            _ => return Err(BAD_DESCRIPTOR),
        };
        if matches!(open.target, Target::Node(_)) {
            self.opens[id].as_mut().unwrap().offset += count;
        }
        Ok(count)
    }
    pub fn write(&mut self, fds: &Descriptors, fd: u64, input: &[u8]) -> Result<usize, i64> {
        self.device(fds, fd, true)?;
        let id = fds.handle(fd)?;
        let open = self.opens[id].unwrap();
        if let Target::Packet { output, .. } = open.target {
            let p = self.pipes[output].as_mut().unwrap();
            if input.len() > PACKET_BYTES {
                return Err(NO_SPACE);
            }
            if p.readers == 0 {
                return Err(BROKEN_PIPE);
            }
            if input.len() + 2 > PIPE_BYTES - p.len {
                return Err(WOULD_BLOCK);
            }
            for byte in (input.len() as u16).to_le_bytes().iter().chain(input) {
                p.bytes[(p.head + p.len) % PIPE_BYTES] = *byte;
                p.len += 1;
            }
            return Ok(input.len());
        }
        if input.is_empty() {
            return Ok(0);
        }
        let count = match open.target {
            Target::WritePipe(p) => {
                let p = self.pipes[p].as_mut().unwrap();
                if p.readers == 0 {
                    return Err(BROKEN_PIPE);
                }
                if p.len == PIPE_BYTES {
                    return Err(WOULD_BLOCK);
                }
                let n = input.len().min(PIPE_BYTES - p.len);
                for c in &input[..n] {
                    p.bytes[(p.head + p.len) % PIPE_BYTES] = *c;
                    p.len += 1;
                }
                n
            }
            Target::Node(n) => {
                self.mutable_node(n)?;
                let inode = self.nodes[n].inode;
                if let Data::Persistent { size, .. } = self.nodes[n].data {
                    let offset = if open.flags & OPEN_APPEND != 0 {
                        size
                    } else {
                        open.offset
                    };
                    let end = offset.checked_add(input.len()).ok_or(NO_SPACE)?;
                    if end <= self.persistent_limit() {
                        self.prepare_mapping_growth(n, end)?;
                    }
                }
                match &mut self.nodes[n].data {
                    Data::Persistent { size, .. } => {
                        let offset = if open.flags & OPEN_APPEND != 0 {
                            *size
                        } else {
                            open.offset
                        };
                        let count = self.disk.as_mut().ok_or(IO_ERROR)?.write(
                            inode.unwrap(),
                            offset,
                            input,
                        )?;
                        self.opens[id].as_mut().unwrap().offset = offset + count;
                        *size = (*size).max(offset + count);
                        let timestamp = self.nodes[n].mount.timestamp(self.clock);
                        self.nodes[n].modified = timestamp;
                        self.nodes[n].changed = timestamp;
                        if let Some(mapping) = &self.nodes[n].mapping {
                            mapping.write(offset, &input[..count]);
                        }
                        return Ok(count);
                    }
                    Data::Mutable(bytes) => {
                        let offset = if open.flags & OPEN_APPEND != 0 {
                            bytes.len()
                        } else {
                            open.offset
                        };
                        let end = offset
                            .checked_add(input.len())
                            .filter(|e| *e <= FILE_BYTES)
                            .ok_or(NO_SPACE)?;
                        if end > bytes.len() {
                            bytes
                                .try_reserve(end - bytes.len())
                                .map_err(|_| NO_MEMORY)?;
                            bytes.resize(end, 0);
                        }
                        bytes[offset..end].copy_from_slice(input);
                        self.opens[id].as_mut().unwrap().offset = end;
                        let timestamp = self.nodes[n].mount.timestamp(self.clock);
                        self.nodes[n].modified = timestamp;
                        self.nodes[n].changed = timestamp;
                        if let Some(mapping) = &self.nodes[n].mapping {
                            mapping.write(offset, input);
                        }
                        return Ok(input.len());
                    }
                    Data::Device(Device::Null) => return Ok(input.len()),
                    _ => return Err(READ_ONLY),
                }
            }
            _ => return Err(BAD_DESCRIPTOR),
        };
        Ok(count)
    }
    pub fn seek(
        &mut self,
        fds: &Descriptors,
        fd: u64,
        delta: i64,
        whence: u64,
    ) -> Result<usize, i64> {
        let id = fds.handle(fd)?;
        let open = self.opens[id].unwrap();
        let Target::Node(n) = open.target else {
            return Err(NOT_SEEKABLE);
        };
        let length = match &self.nodes[n].data {
            Data::Borrowed(b, _) => b.len(),
            Data::Mutable(b) => b.len(),
            Data::Persistent { size, .. } => *size,
            _ => return Err(NOT_SEEKABLE),
        };
        let base = match whence {
            0 => 0,
            1 => open.offset,
            2 => length,
            _ => return Err(INVALID_ARGUMENT),
        };
        let offset = (base as i64)
            .checked_add(delta)
            .filter(|offset| {
                *offset >= 0
                    && *offset
                        <= length.max(if self.nodes[n].mount == Mount::Persistent {
                            self.persistent_limit()
                        } else {
                            FILE_BYTES
                        }) as i64
            })
            .ok_or(INVALID_ARGUMENT)? as usize;
        self.opens[id].as_mut().unwrap().offset = offset;
        Ok(offset)
    }
    pub fn readdir(&mut self, fds: &Descriptors, fd: u64) -> Result<Option<DirEntry>, i64> {
        let entry = self.peek_readdir(fds, fd)?;
        let Some(entry) = entry else {
            return Ok(None);
        };
        let mut native = DirEntry::default();
        let n = entry.name_len as usize;
        if n >= native.name.len() {
            return Err(TOO_LONG);
        }
        native.inode = entry.inode;
        native.kind = entry.kind;
        native.name_len = entry.name_len;
        native.name[..n].copy_from_slice(&entry.name[..n]);
        self.advance_readdir(fds, fd)?;
        Ok(Some(native))
    }
    pub fn advance_readdir(&mut self, fds: &Descriptors, fd: u64) -> Result<(), i64> {
        if let Some(entry) = self.peek_readdir(fds, fd)? {
            self.opens[fds.handle(fd)?].as_mut().unwrap().offset = entry.next_offset;
        }
        Ok(())
    }
    /// Inspect the next record without consuming it before an ABI buffer has been validated.
    pub fn peek_readdir(
        &mut self,
        fds: &Descriptors,
        fd: u64,
    ) -> Result<Option<DirectoryEntry>, i64> {
        let id = fds.handle(fd)?;
        let open = self.opens[id].unwrap();
        if open.flags & OPEN_READ == 0 {
            return Err(BAD_DESCRIPTOR);
        }
        let Target::Node(n) = open.target else {
            return Err(NOT_DIRECTORY);
        };
        if !matches!(self.nodes[n].data, Data::Directory) {
            return Err(NOT_DIRECTORY);
        }
        if self.nodes[n].mount == Mount::Persistent {
            let Some((entry, next)) = self
                .disk
                .as_mut()
                .ok_or(IO_ERROR)?
                .next_entry(self.nodes[n].inode.unwrap(), open.offset as u64)?
            else {
                return Ok(None);
            };
            let info = self.disk.as_mut().ok_or(IO_ERROR)?.inode(entry.inode)?;
            return Ok(Some(DirectoryEntry {
                inode: PERSISTENT_ID_BASE + u64::from(entry.inode),
                kind: if info.directory { 2 } else { 1 },
                name_len: entry.length as u32,
                name: entry.name,
                next_offset: usize::try_from(next).map_err(|_| INVALID_FILESYSTEM)?,
            }));
        }
        let Some((i, node)) = self
            .nodes
            .iter()
            .enumerate()
            .skip(open.offset.max(1))
            .find(|(_, node)| node.linked && node.parent == n)
        else {
            return Ok(None);
        };
        let kind = match node.data {
            Data::Directory => 2,
            Data::Link(_) => 3,
            Data::Device(_) => 4,
            _ => 1,
        };
        Ok(Some(DirectoryEntry {
            inode: i as u64,
            kind,
            name_len: node.len as u32,
            name: node.name,
            next_offset: i + 1,
        }))
    }
    pub fn live_objects(&self) -> (usize, usize) {
        (
            self.opens.iter().flatten().count(),
            self.pipes.iter().flatten().count(),
        )
    }
    /// Allocated namespace slots, including unlinked slots retained for later reuse.
    /// This measures the vector's high-water extent, not just currently linked names.
    pub fn node_slots(&self) -> usize {
        self.nodes.len()
    }
}
