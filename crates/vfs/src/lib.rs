// SPDX-License-Identifier: Apache-2.0
#![no_std]
//! Bounded VFS mounts, component-wise paths, shared open descriptions and byte-stream pipes.
//! One non-preemptible owner serializes all mutations; no hardware, locks or user pointers.
extern crate alloc;
pub mod archive;
pub mod fd;
#[cfg(test)]
mod tests;
use alloc::vec::Vec;
use anova_abi::*;
pub use fd::Descriptors;

pub const ROOT: usize = 0;
pub const NODE_LIMIT: usize = 96;
pub const OPEN_LIMIT: usize = 128;
pub const PIPE_LIMIT: usize = 16;
pub const PIPE_BYTES: usize = 1024;
pub const FILE_BYTES: usize = 64 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mount {
    Initramfs,
    Tmpfs,
    Devfs,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Console,
    Null,
}
enum Data<'a> {
    Directory,
    Borrowed(&'a [u8], bool),
    Mutable(Vec<u8>),
    Link(&'a str),
    Device(Device),
}
struct Node<'a> {
    parent: usize,
    name: [u8; 64],
    len: usize,
    mount: Mount,
    linked: bool,
    data: Data<'a>,
}
#[derive(Clone, Copy)]
enum Target {
    Node(usize),
    ReadPipe(usize),
    WritePipe(usize),
}
#[derive(Clone, Copy)]
struct Open {
    target: Target,
    offset: usize,
    flags: u64,
    refs: usize,
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
        nodes.try_reserve_exact(NODE_LIMIT).map_err(|_| NO_MEMORY)?;
        nodes.push(Node {
            parent: ROOT,
            name: [0; 64],
            len: 0,
            mount: Mount::Initramfs,
            linked: true,
            data: Data::Directory,
        });
        let mut fs = Self {
            nodes,
            opens: slots(OPEN_LIMIT, || None)?,
            pipes: slots(PIPE_LIMIT, || None)?,
        };
        let mut input = archive::Archive::new(archive)?;
        while let Some(entry) = input.entry()? {
            let data = match entry.kind {
                archive::DIRECTORY => Data::Directory,
                archive::SYMLINK => {
                    Data::Link(core::str::from_utf8(entry.data).map_err(|_| INVALID_ARGUMENT)?)
                }
                kind => Data::Borrowed(entry.data, kind == archive::EXECUTABLE),
            };
            let (parent, name) = fs.parent(ROOT, entry.path)?;
            // Archive paths may never traverse a symlink supplied by an earlier entry.
            let lexical = entry.path.rsplit_once('/').unwrap().0;
            let mut canonical = [0; 128];
            let length = fs.canonical(parent, &mut canonical)?;
            if !lexical.is_empty() && &canonical[..length] != lexical.as_bytes() {
                return Err(INVALID_ARGUMENT);
            }
            fs.insert(parent, name, data, Mount::Initramfs)?;
        }
        fs.mount("/tmp", Mount::Tmpfs)?;
        fs.mount("/dev", Mount::Devfs)?;
        let dev = fs.resolve(ROOT, "/dev")?;
        fs.insert(dev, "console", Data::Device(Device::Console), Mount::Devfs)?;
        fs.insert(dev, "null", Data::Device(Device::Null), Mount::Devfs)?;
        Ok(fs)
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
        if name.is_empty()
            || name.len() > 63
            || name == "."
            || name == ".."
            || name.contains(['/', '\0'])
        {
            return Err(INVALID_ARGUMENT);
        }
        if self.child(parent, name).is_some() {
            return Err(ALREADY_EXISTS);
        }
        let mut label = [0; 64];
        label[..name.len()].copy_from_slice(name.as_bytes());
        let node = Node {
            parent,
            name: label,
            len: name.len(),
            mount,
            linked: true,
            data,
        };
        // Unlinked nodes remain alive until their final open description is closed.
        let reusable = self.nodes.iter().enumerate().position(|(i, n)| {
            !n.linked
                && !self
                    .opens
                    .iter()
                    .flatten()
                    .any(|o| matches!(o.target,Target::Node(id) if id == i))
        });
        if let Some(i) = reusable {
            self.nodes[i] = node;
            Ok(i)
        } else if self.nodes.len() < NODE_LIMIT {
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
    pub fn resolve(&self, cwd: usize, path: &str) -> Result<usize, i64> {
        self.walk(cwd, path, 0)
    }
    fn walk(&self, cwd: usize, path: &str, depth: usize) -> Result<usize, i64> {
        if path.is_empty() || path.len() > 128 || path.contains('\0') {
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
                    let next = self.child(at, name).ok_or(NOT_FOUND)?;
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
    fn parent<'p>(&self, cwd: usize, path: &'p str) -> Result<(usize, &'p str), i64> {
        if path.is_empty() || path.len() > 128 || path.contains('\0') || path.ends_with('/') {
            return Err(INVALID_ARGUMENT);
        }
        let (base, name) = path.rsplit_once('/').unwrap_or((".", path));
        let parent = self.resolve(cwd, if base.is_empty() { "/" } else { base })?;
        if !matches!(self.nodes[parent].data, Data::Directory) {
            return Err(NOT_DIRECTORY);
        }
        if name == "." || name == ".." || name.len() > 63 {
            return Err(INVALID_ARGUMENT);
        }
        Ok((parent, name))
    }
    pub fn canonical(&self, mut node: usize, output: &mut [u8]) -> Result<usize, i64> {
        let mut reversed = [0; 128];
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
    pub fn chdir(&self, cwd: usize, path: &str) -> Result<usize, i64> {
        let node = self.resolve(cwd, path)?;
        if matches!(self.nodes[node].data, Data::Directory) {
            Ok(node)
        } else {
            Err(NOT_DIRECTORY)
        }
    }
    pub fn mkdir(&mut self, cwd: usize, path: &str) -> Result<(), i64> {
        let (parent, name) = self.parent(cwd, path)?;
        if self.nodes[parent].mount != Mount::Tmpfs {
            return Err(READ_ONLY);
        }
        self.insert(parent, name, Data::Directory, Mount::Tmpfs)
            .map(|_| ())
    }
    pub fn unlink(&mut self, cwd: usize, path: &str) -> Result<(), i64> {
        let (parent, name) = self.parent(cwd, path)?;
        let node = self.child(parent, name).ok_or(NOT_FOUND)?;
        if self.nodes[node].mount != Mount::Tmpfs {
            return Err(READ_ONLY);
        }
        if matches!(self.nodes[node].data, Data::Directory) {
            return Err(IS_DIRECTORY);
        }
        self.nodes[node].linked = false;
        if !self
            .opens
            .iter()
            .flatten()
            .any(|o| matches!(o.target, Target::Node(n) if n == node))
        {
            self.nodes[node].data = Data::Mutable(Vec::new());
        }
        Ok(())
    }
    pub fn executable(&self, cwd: usize, path: &str) -> Result<&'a [u8], i64> {
        let node = self.resolve(cwd, path)?;
        match self.nodes[node].data {
            Data::Borrowed(bytes, true) => Ok(bytes),
            _ => Err(PERMISSION_DENIED),
        }
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
        {
            return Err(INVALID_ARGUMENT);
        }
        let fd = fds.free()?;
        if self.opens.iter().all(Option::is_some) {
            return Err(FILE_LIMIT);
        }
        let node = match self.resolve(cwd, path) {
            Ok(id) => id,
            Err(NOT_FOUND) if flags & OPEN_CREATE != 0 => {
                let (parent, name) = self.parent(cwd, path)?;
                if flags & OPEN_DIRECTORY != 0 {
                    return Err(NOT_DIRECTORY);
                }
                if self.nodes[parent].mount != Mount::Tmpfs {
                    return Err(READ_ONLY);
                }
                self.insert(parent, name, Data::Mutable(Vec::new()), Mount::Tmpfs)?
            }
            Err(e) => return Err(e),
        };
        if flags & OPEN_DIRECTORY != 0 && !matches!(self.nodes[node].data, Data::Directory) {
            return Err(NOT_DIRECTORY);
        }
        if flags & OPEN_WRITE != 0 {
            match self.nodes[node].data {
                Data::Directory => return Err(IS_DIRECTORY),
                Data::Borrowed(..) | Data::Link(..) => return Err(READ_ONLY),
                _ => {}
            }
        }
        if flags & OPEN_TRUNCATE != 0 {
            if let Data::Mutable(ref mut bytes) = self.nodes[node].data {
                bytes.clear();
            } else {
                return Err(INVALID_ARGUMENT);
            }
        }
        let id = self.allocate(Open {
            target: Target::Node(node),
            offset: 0,
            flags,
            refs: 1,
        })?;
        fds.slots[fd] = Some(id);
        Ok(fd as u64)
    }
    pub fn inherit(&mut self, parent: &Descriptors) -> Descriptors {
        for id in parent.slots.iter().flatten() {
            self.opens[*id].as_mut().unwrap().refs += 1;
        }
        Descriptors {
            slots: parent.slots,
        }
    }
    pub fn close(&mut self, fds: &mut Descriptors, fd: u64) -> Result<(), i64> {
        let id = fds.handle(fd)?;
        fds.slots[fd as usize] = None;
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
                Target::Node(n) if !self.nodes[n].linked => {
                    if !self.opens.iter().enumerate().any(|(other, o)| {
                        other != id
                            && o.is_some_and(
                                |o| matches!(o.target, Target::Node(node) if node == n),
                            )
                    }) {
                        self.nodes[n].data = Data::Mutable(Vec::new());
                    }
                }
                _ => {}
            }
            self.opens[id] = None;
        }
        Ok(())
    }
    pub fn clear(&mut self, fds: &mut Descriptors) {
        for i in 0..fd::FD_LIMIT {
            if fds.slots[i].is_some() {
                self.close(fds, i as u64).unwrap();
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
        fds.slots[target as usize] = Some(id);
        Ok(target)
    }
    pub fn pipe(&mut self, fds: &mut Descriptors) -> Result<[u64; 2], i64> {
        let mut free = fds
            .slots
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
        })?;
        let b = self.allocate(Open {
            target: Target::WritePipe(p),
            offset: 0,
            flags: OPEN_WRITE,
            refs: 1,
        })?;
        self.pipes[p] = Some(Pipe {
            bytes: [0; PIPE_BYTES],
            head: 0,
            len: 0,
            readers: 1,
            writers: 1,
        });
        fds.slots[r] = Some(a);
        fds.slots[w] = Some(b);
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
            _ => true,
        }
    }
    pub fn read(&mut self, fds: &Descriptors, fd: u64, output: &mut [u8]) -> Result<usize, i64> {
        self.device(fds, fd, false)?;
        let id = fds.handle(fd)?;
        let open = self.opens[id].unwrap();
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
            Target::Node(n) => match &mut self.nodes[n].data {
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
                    return Ok(input.len());
                }
                Data::Device(Device::Null) => return Ok(input.len()),
                _ => return Err(READ_ONLY),
            },
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
            .filter(|n| *n >= 0 && *n <= length.max(FILE_BYTES) as i64)
            .ok_or(INVALID_ARGUMENT)? as usize;
        self.opens[id].as_mut().unwrap().offset = offset;
        Ok(offset)
    }
    pub fn readdir(&mut self, fds: &Descriptors, fd: u64) -> Result<Option<DirEntry>, i64> {
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
        let Some((i, node)) = self
            .nodes
            .iter()
            .enumerate()
            .skip(open.offset.max(1))
            .find(|(_, node)| node.linked && node.parent == n)
        else {
            return Ok(None);
        };
        self.opens[id].as_mut().unwrap().offset = i + 1;
        let kind = match node.data {
            Data::Directory => 2,
            Data::Link(_) => 3,
            Data::Device(_) => 4,
            _ => 1,
        };
        Ok(Some(DirEntry {
            inode: i as u64,
            kind,
            name_len: node.len as u32,
            name: node.name,
        }))
    }
    pub fn live_objects(&self) -> (usize, usize) {
        (
            self.opens.iter().flatten().count(),
            self.pipes.iter().flatten().count(),
        )
    }
}
