// SPDX-License-Identifier: Apache-2.0
//! Test VFS paths, shared file/socket descriptor ownership, pipe shutdown and bounded rollback.
use super::*;
use alloc::vec;
#[test]
fn removed_directories_retain_open_identity_and_reclaim_after_final_close() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut fds = Descriptors::default();
    fs.mkdir(ROOT, "/tmp/remove").unwrap();
    let node = fs.chdir(ROOT, "/tmp/remove").unwrap();
    let directory = fs
        .open(&mut fds, ROOT, "/tmp/remove", OPEN_READ | OPEN_DIRECTORY)
        .unwrap();
    let child = fs
        .open(&mut fds, node, "child", OPEN_CREATE | OPEN_WRITE)
        .unwrap();
    assert_eq!(fs.rmdir(ROOT, "/tmp/remove", &[]), Err(NOT_EMPTY));
    fs.unlink(node, "child").unwrap();
    fs.close(&mut fds, child).unwrap();
    assert_eq!(fs.rmdir(ROOT, "/tmp/remove", &[node]), Err(BUSY));
    fs.rmdir(ROOT, "/tmp/remove", &[]).unwrap();
    assert!(fs.peek_readdir(&fds, directory).unwrap().is_none());
    assert_eq!(fs.mkdir(node, "resurrect"), Err(NOT_FOUND));
    fs.mkdir(ROOT, "/tmp/remove").unwrap();
    assert_ne!(fs.chdir(ROOT, "/tmp/remove").unwrap(), node);
    fs.close(&mut fds, directory).unwrap();
    fs.rmdir(ROOT, "/tmp/remove", &[]).unwrap();
    assert_eq!(fs.rmdir(ROOT, "/tmp", &[]), Err(BUSY));
    fs.mkdir(ROOT, "/tmp/parent").unwrap();
    fs.mkdir(ROOT, "/tmp/parent/child").unwrap();
    let held = fs
        .open(
            &mut fds,
            ROOT,
            "/tmp/parent/child",
            OPEN_READ | OPEN_DIRECTORY,
        )
        .unwrap();
    fs.rmdir(ROOT, "/tmp/parent/child", &[]).unwrap();
    assert_eq!(fs.rmdir(ROOT, "/tmp/parent", &[]), Err(BUSY));
    fs.close(&mut fds, held).unwrap();
    fs.rmdir(ROOT, "/tmp/parent", &[]).unwrap();
    for _ in 0..NODE_LIMIT + 1 {
        fs.mkdir(ROOT, "/tmp/reuse").unwrap();
        fs.rmdir(ROOT, "/tmp/reuse", &[]).unwrap();
    }
    fs.clear(&mut fds);
    assert_eq!(fs.live_objects(), (0, 0));
}
#[test]
fn extended_names_keep_native_wire_records_bounded_and_failed_reads_unconsumed() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut fds = Descriptors::default();
    fs.mkdir(ROOT, "/tmp/names").unwrap();
    let name = "n".repeat(NAME_BYTES);
    let path = alloc::format!("/tmp/names/{name}");
    let file = fs
        .open(&mut fds, ROOT, &path, OPEN_CREATE | OPEN_WRITE)
        .unwrap();
    fs.write(&fds, file, b"long name").unwrap();
    let directory = fs
        .open(&mut fds, ROOT, "/tmp/names", OPEN_READ | OPEN_DIRECTORY)
        .unwrap();
    assert!(matches!(fs.readdir(&fds, directory), Err(TOO_LONG)));
    let entry = fs.peek_readdir(&fds, directory).unwrap().unwrap();
    assert_eq!(&entry.name[..entry.name_len as usize], name.as_bytes());
    fs.advance_readdir(&fds, directory).unwrap();
    assert!(fs.peek_readdir(&fds, directory).unwrap().is_none());
    assert_eq!(
        fs.open(
            &mut fds,
            ROOT,
            &alloc::format!("{path}x"),
            OPEN_CREATE | OPEN_WRITE
        ),
        Err(TOO_LONG)
    );
    assert_eq!(
        fs.metadata(ROOT, &"n".repeat(PATH_BYTES + 1)),
        Err(TOO_LONG)
    );
    fs.clear(&mut fds);
    assert_eq!(fs.live_objects(), (0, 0));
}
#[test]
fn packet_pairs_preserve_messages_duplex_backpressure_and_last_close() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut fds = Descriptors::default();
    let [a, b] = fs.packet_pair(&mut fds).unwrap();
    fs.write(&fds, a, b"first").unwrap();
    fs.write(&fds, a, b"next").unwrap();
    let mut short = [0; 2];
    assert_eq!(fs.read(&fds, b, &mut short).unwrap(), 2);
    assert_eq!(&short, b"fi");
    let mut bytes = [0; PACKET_BYTES];
    assert_eq!(fs.read(&fds, b, &mut bytes).unwrap(), 4);
    assert_eq!(&bytes[..4], b"next");
    assert_eq!(fs.read(&fds, b, &mut bytes), Err(WOULD_BLOCK));
    fs.write(&fds, b, b"reverse").unwrap();
    assert_eq!(fs.read(&fds, a, &mut bytes).unwrap(), 7);
    assert_eq!(&bytes[..7], b"reverse");
    fs.write(&fds, a, &bytes).unwrap();
    assert_eq!(fs.write(&fds, a, b"x"), Err(WOULD_BLOCK));
    assert_eq!(fs.read(&fds, b, &mut bytes).unwrap(), PACKET_BYTES);
    fs.write(&fds, a, &[]).unwrap();
    assert!(fs.readiness(&fds, b).unwrap().0);
    assert_eq!(fs.read(&fds, b, &mut []).unwrap(), 0);
    assert_eq!(fs.read(&fds, b, &mut bytes), Err(WOULD_BLOCK));
    let mut child = fs.inherit(&fds).unwrap();
    fs.clear(&mut fds);
    assert_eq!(fs.read(&child, a, &mut bytes), Err(WOULD_BLOCK));
    fs.close(&mut child, b).unwrap();
    assert_eq!(fs.read(&child, a, &mut bytes).unwrap(), 0);
    assert_eq!(fs.write(&child, a, b"x"), Err(BROKEN_PIPE));
    fs.clear(&mut child);
    assert_eq!(fs.live_objects(), (0, 0));
    for _ in 0..PIPE_LIMIT / 2 {
        fs.packet_pair(&mut fds).unwrap();
    }
    assert_eq!(fs.packet_pair(&mut fds), Err(FILE_LIMIT));
    assert_eq!(fds.count(), PIPE_LIMIT);
    fs.clear(&mut fds);
    assert_eq!(fs.live_objects(), (0, 0));
}
#[test]
fn rename_replacement_preserves_open_bytes_and_description_lock_lifetime() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut fds = Descriptors::default();
    let a = fs
        .open(
            &mut fds,
            ROOT,
            "/tmp/a",
            OPEN_READ | OPEN_WRITE | OPEN_CREATE,
        )
        .unwrap();
    let b = fs
        .open(
            &mut fds,
            ROOT,
            "/tmp/b",
            OPEN_READ | OPEN_WRITE | OPEN_CREATE,
        )
        .unwrap();
    fs.write(&fds, a, b"new").unwrap();
    fs.write_at(&fds, a, 1, b"E").unwrap();
    assert_eq!(fs.seek(&fds, a, 0, 1).unwrap(), 3);
    let mut positioned = [0; 3];
    assert_eq!(fs.read_at(&fds, a, 0, &mut positioned).unwrap(), 3);
    assert_eq!(&positioned, b"nEw");
    assert_eq!(fs.seek(&fds, a, 0, 1).unwrap(), 3);
    fs.write(&fds, b, b"old").unwrap();
    fs.seek(&fds, b, 0, 0).unwrap();
    fs.rename(ROOT, "/tmp/a", "/tmp/b").unwrap();
    assert_eq!(fs.metadata(ROOT, "/tmp/a"), Err(NOT_FOUND));
    let new = fs.open(&mut fds, ROOT, "/tmp/b", OPEN_READ).unwrap();
    let mut bytes = [0; 3];
    assert_eq!(fs.read(&fds, b, &mut bytes).unwrap(), 3);
    assert_eq!(&bytes, b"old");
    fs.set_len(&fds, b, 2).unwrap();
    fs.set_len(&fds, b, 6).unwrap();
    assert_eq!(fs.seek(&fds, b, 0, 1).unwrap(), 3);
    let mut grown = [0xff; 6];
    assert_eq!(fs.read_at(&fds, b, 0, &mut grown).unwrap(), 6);
    assert_eq!(&grown, b"ol\0\0\0\0");
    assert_eq!(fs.set_len(&fds, b, FILE_BYTES + 1), Err(NO_SPACE));
    assert_eq!(fs.set_len(&fds, new, 0), Err(BAD_DESCRIPTOR));
    fs.try_lock(&fds, a, 2).unwrap();
    assert_eq!(fs.try_lock(&fds, new, 1), Err(WOULD_BLOCK));
    let dup = fs.dup2(&mut fds, a, 20).unwrap();
    fs.close(&mut fds, a).unwrap();
    assert_eq!(fs.try_lock(&fds, new, 2), Err(WOULD_BLOCK));
    fs.close(&mut fds, dup).unwrap();
    fs.try_lock(&fds, new, 2).unwrap();
    assert_eq!(fs.rename(ROOT, "/tmp/b", "/etc/text"), Err(READ_ONLY));
    fs.mkdir(ROOT, "/tmp/staging").unwrap();
    let directory = fs.chdir(ROOT, "/tmp/staging").unwrap();
    fs.mkdir(ROOT, "/tmp/staging/child").unwrap();
    fs.mkdir(ROOT, "/tmp/other").unwrap();
    fs.rename(ROOT, "/tmp/staging", "/tmp/installed").unwrap();
    assert_eq!(fs.chdir(ROOT, "/tmp/installed").unwrap(), directory);
    assert_eq!(fs.metadata(ROOT, "/tmp/staging"), Err(NOT_FOUND));
    assert!(fs.chdir(ROOT, "/tmp/installed/child").is_ok());
    assert_eq!(
        fs.rename(ROOT, "/tmp/installed", "/tmp/other"),
        Err(UNSUPPORTED)
    );
    assert_eq!(
        fs.rename(ROOT, "/tmp/installed", "/tmp/other/sub"),
        Err(UNSUPPORTED)
    );
    fs.clear(&mut fds);
    assert_eq!(fs.live_objects(), (0, 0));
}
#[test]
fn shared_thread_table_publishes_close_and_releases_only_on_last_owner() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut parent = fs.standard().unwrap();
    let mut thread = parent.share().unwrap();
    let fd = fs.open(&mut thread, ROOT, "/etc/text", OPEN_READ).unwrap();
    assert!(parent.contains(fd));
    fs.close(&mut parent, fd).unwrap();
    assert!(!thread.contains(fd));
    fs.clear(&mut parent);
    assert_eq!(thread.count(), 3);
    fs.clear(&mut thread);
    assert_eq!(fs.live_objects(), (0, 0));
}
#[test]
fn pty_allocation_lock_and_final_open_lifetime_are_bounded() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut fds = Descriptors::default();
    let master = fs
        .open(&mut fds, ROOT, "/dev/ptmx", OPEN_READ | OPEN_WRITE)
        .unwrap();
    assert_eq!(
        fs.device(&fds, master, false),
        Ok(Some(Device::PtyMaster(0)))
    );
    assert_eq!(
        fs.open(&mut fds, ROOT, "/dev/pts/0", OPEN_READ),
        Err(IO_ERROR)
    );
    fs.pty_lock(0, false);
    let slave = fs.open(&mut fds, ROOT, "/dev/pts/0", OPEN_READ).unwrap();
    let duplicate = fs.dup2(&mut fds, master, 12).unwrap();
    fs.close(&mut fds, master).unwrap();
    assert!(fs.pty_present(0, true));
    fs.close(&mut fds, duplicate).unwrap();
    assert!(!fs.pty_present(0, true));
    fs.close(&mut fds, slave).unwrap();
    fs.clear(&mut fds);
    assert_eq!(fs.live_objects(), (0, 0));
}
fn image() -> Vec<u8> {
    let entries: &[(u8, &str, &[u8])] = &[
        (2, "/bin", b""),
        (2, "/tmp", b""),
        (2, "/dev", b""),
        (2, "/etc", b""),
        (1, "/etc/text", b"abcdef"),
        (4, "/bin/app", b"ELF"),
        (3, "/text", b"etc/text"),
        (3, "/loop", b"loop"),
        (3, "/tmp-link", b"/tmp"),
    ];
    let mut bytes = Vec::from(&b"ANOVAFS1"[..]);
    bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (kind, path, data) in entries {
        bytes.extend_from_slice(&[*kind, 0]);
        bytes.extend_from_slice(&(path.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u64).to_le_bytes());
        bytes.extend_from_slice(path.as_bytes());
        bytes.extend_from_slice(data);
    }
    bytes
}
#[test]
fn component_walk_enforces_directory_mount_symlink_and_root_boundaries() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    assert_eq!(fs.resolve(ROOT, "/text"), fs.resolve(ROOT, "/etc/text"));
    assert_eq!(fs.resolve(ROOT, "/loop"), Err(LINK_LOOP));
    assert_eq!(fs.resolve(ROOT, "/etc/text/../text"), Err(NOT_DIRECTORY));
    assert_eq!(fs.resolve(ROOT, "/etc/text/"), Err(NOT_DIRECTORY));
    assert_eq!(fs.resolve(ROOT, "/../../.."), Ok(ROOT));
    assert_eq!(fs.resolve(ROOT, ""), Err(INVALID_ARGUMENT));
    fs.mkdir(ROOT, "/tmp-link/work").unwrap();
    let cwd = fs.chdir(ROOT, "/tmp/work").unwrap();
    fs.mkdir(cwd, "sub").unwrap();
    assert_eq!(fs.resolve(cwd, "sub/.."), Ok(cwd));
    let mut path = [0; 128];
    let n = fs.canonical(cwd, &mut path).unwrap();
    assert_eq!(&path[..n], b"/tmp/work");
    assert_eq!(fs.mkdir(ROOT, "/etc/new"), Err(READ_ONLY));
    assert_eq!(fs.executable(ROOT, "/bin/app"), Ok(&b"ELF"[..]));
    assert_eq!(fs.executable(ROOT, "/etc/text"), Err(PERMISSION_DENIED));
}
#[test]
fn independent_opens_dup_and_fork_have_precise_shared_offsets_and_lifetimes() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut p = fs.standard().unwrap();
    let a = fs.open(&mut p, ROOT, "/etc/text", OPEN_READ).unwrap();
    let b = fs.open(&mut p, ROOT, "/etc/text", OPEN_READ).unwrap();
    fs.dup2(&mut p, a, 10).unwrap();
    let mut c = fs.inherit(&p).unwrap();
    let mut out = [0; 2];
    fs.read(&p, a, &mut out).unwrap();
    assert_eq!(&out, b"ab");
    fs.read(&c, 10, &mut out).unwrap();
    assert_eq!(&out, b"cd");
    fs.read(&p, a, &mut out).unwrap();
    assert_eq!(&out, b"ef");
    fs.read(&p, b, &mut out).unwrap();
    assert_eq!(&out, b"ab");
    fs.seek(&p, a, -2, 2).unwrap();
    fs.read(&c, a, &mut out).unwrap();
    assert_eq!(&out, b"ef");
    assert_eq!(fs.seek(&p, a, -99, 0), Err(INVALID_ARGUMENT));
    assert_eq!(fs.seek(&p, 1, 0, 0), Err(NOT_SEEKABLE));
    fs.close(&mut c, 1).unwrap();
    assert!(p.contains(1));
    assert!(!c.contains(1));
    assert_eq!(fs.dup2(&mut c, 99, 0), Err(BAD_DESCRIPTOR));
    fs.clear(&mut p);
    assert!(fs.live_objects().0 > 0);
    fs.clear(&mut c);
    assert_eq!(fs.live_objects(), (0, 0));
}
#[test]
fn tmpfs_sparse_append_truncate_and_unlink_keep_existing_opens_alive() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut p = Descriptors::default();
    let a = fs
        .open(&mut p, ROOT, "/tmp/f", OPEN_READ | OPEN_WRITE | OPEN_CREATE)
        .unwrap();
    fs.seek(&p, a, 3, 0).unwrap();
    fs.write(&p, a, b"a").unwrap();
    let b = fs
        .open(&mut p, ROOT, "/tmp/f", OPEN_READ | OPEN_WRITE | OPEN_APPEND)
        .unwrap();
    fs.seek(&p, b, 0, 0).unwrap();
    fs.write(&p, b, b"bc").unwrap();
    fs.seek(&p, a, 0, 0).unwrap();
    let mut out = [0xff; 6];
    assert_eq!(fs.read(&p, a, &mut out), Ok(6));
    assert_eq!(&out, b"\0\0\0abc");
    assert_eq!(fs.write(&p, b, &vec![1; FILE_BYTES + 1]), Err(NO_SPACE));
    fs.unlink(ROOT, "/tmp/f").unwrap();
    assert_eq!(fs.resolve(ROOT, "/tmp/f"), Err(NOT_FOUND));
    fs.close(&mut p, a).unwrap();
    fs.seek(&p, b, 3, 0).unwrap();
    assert_eq!(fs.read(&p, b, &mut out), Ok(3));
    assert_eq!(&out[..3], b"abc");
    let c = fs
        .open(&mut p, ROOT, "/tmp/f", OPEN_WRITE | OPEN_CREATE)
        .unwrap();
    fs.write(&p, c, b"new").unwrap();
    fs.close(&mut p, b).unwrap();
    fs.clear(&mut p);
    assert_eq!(fs.live_objects(), (0, 0));
    let d = fs
        .open(&mut p, ROOT, "/tmp/f", OPEN_WRITE | OPEN_TRUNCATE)
        .unwrap();
    fs.close(&mut p, d).unwrap();
    let r = fs.open(&mut p, ROOT, "/tmp/f", OPEN_READ).unwrap();
    assert_eq!(fs.read(&p, r, &mut out), Ok(0));
    fs.clear(&mut p);
}
#[test]
fn pipes_block_wrap_drain_eof_and_report_broken_reader_after_final_reference() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut p = Descriptors::default();
    let [r, w] = fs.pipe(&mut p).unwrap();
    let mut c = fs.inherit(&p).unwrap();
    fs.dup2(&mut c, w, 9).unwrap();
    let mut out = [0; PIPE_BYTES];
    assert_eq!(fs.read(&p, r, &mut out), Err(WOULD_BLOCK));
    assert!(!fs.ready(&p, r, false));
    assert_eq!(fs.write(&p, w, &vec![7; PIPE_BYTES + 10]), Ok(PIPE_BYTES));
    assert_eq!(fs.write(&p, w, b"x"), Err(WOULD_BLOCK));
    assert_eq!(fs.read(&p, r, &mut out[..700]), Ok(700));
    fs.write(&c, w, &vec![8; 700]).unwrap();
    assert_eq!(fs.read(&p, r, &mut out), Ok(PIPE_BYTES));
    assert_eq!(&out[..324], &[7; 324]);
    assert_eq!(&out[324..], &[8; 700]);
    fs.close(&mut p, w).unwrap();
    fs.close(&mut c, w).unwrap();
    assert_eq!(fs.read(&p, r, &mut out), Err(WOULD_BLOCK));
    fs.close(&mut c, 9).unwrap();
    assert!(fs.ready(&p, r, false));
    assert_eq!(fs.read(&p, r, &mut out), Ok(0));
    fs.clear(&mut p);
    fs.clear(&mut c);
    assert_eq!(fs.live_objects(), (0, 0));
    let [r, w] = fs.pipe(&mut p).unwrap();
    fs.close(&mut p, r).unwrap();
    assert_eq!(fs.write(&p, w, b"x"), Err(BROKEN_PIPE));
    assert_eq!(fs.write(&p, w, b""), Ok(0));
    fs.clear(&mut p);
}
#[test]
fn descriptor_and_inode_exhaustion_roll_back_and_reuse_without_leaks() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut p = Descriptors::default();
    for fd in 0..fd::FD_LIMIT {
        assert_eq!(
            fs.open(&mut p, ROOT, "/dev/null", OPEN_WRITE),
            Ok(fd as u64)
        );
    }
    let before = fs.live_objects();
    assert_eq!(fs.pipe(&mut p), Err(FILE_LIMIT));
    assert_eq!(
        fs.open(
            &mut p,
            ROOT,
            "/tmp/must-not-exist",
            OPEN_WRITE | OPEN_CREATE
        ),
        Err(FILE_LIMIT)
    );
    assert_eq!(fs.resolve(ROOT, "/tmp/must-not-exist"), Err(NOT_FOUND));
    assert_eq!(fs.live_objects(), before);
    fs.clear(&mut p);
    for _ in 0..200 {
        let [r, w] = fs.pipe(&mut p).unwrap();
        fs.close(&mut p, r).unwrap();
        fs.close(&mut p, w).unwrap();
    }
    let mut n = 0;
    loop {
        let mut name = [b'x'; 63];
        name[0] = (n % 26) as u8 + b'a';
        name[1] = ((n / 26) % 26) as u8 + b'a';
        name[2] = (n / (26 * 26)) as u8 + b'a';
        let path = core::str::from_utf8(&name).unwrap();
        let cwd = fs.resolve(ROOT, "/tmp").unwrap();
        match fs.mkdir(cwd, path) {
            Ok(()) => n += 1,
            Err(NO_SPACE) => break,
            e => panic!("Unexpected capacity result: {e:?}"),
        }
    }
    assert!(n > 50);
    assert_eq!(fs.live_objects(), (0, 0));
}

#[test]
fn global_open_and_pipe_limits_reject_new_objects_and_recover_after_close() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut tables: [Descriptors; 4] = core::array::from_fn(|_| Descriptors::default());
    for table in &mut tables {
        for _ in 0..fd::FD_LIMIT {
            fs.open(table, ROOT, "/dev/null", OPEN_READ).unwrap();
        }
    }
    let mut extra = Descriptors::default();
    assert_eq!(fs.live_objects(), (OPEN_LIMIT, 0));
    assert_eq!(
        fs.open(
            &mut extra,
            ROOT,
            "/tmp/unpublished",
            OPEN_CREATE | OPEN_WRITE
        ),
        Err(FILE_LIMIT)
    );
    assert_eq!(fs.resolve(ROOT, "/tmp/unpublished"), Err(NOT_FOUND));
    assert_eq!(fs.pipe(&mut extra), Err(FILE_LIMIT));
    for table in &mut tables {
        fs.clear(table);
    }
    for _ in 0..PIPE_LIMIT {
        fs.pipe(&mut tables[0]).unwrap();
    }
    assert_eq!(fs.live_objects(), (PIPE_LIMIT * 2, PIPE_LIMIT));
    assert_eq!(fs.pipe(&mut extra), Err(FILE_LIMIT));
    fs.clear(&mut tables[0]);
    assert_eq!(fs.live_objects(), (0, 0));
    fs.pipe(&mut extra).unwrap();
    fs.clear(&mut extra);
    assert_eq!(fs.live_objects(), (0, 0));
}

#[test]
fn descriptor_exec_flags_are_per_slot_and_inherited_without_losing_shared_offsets() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut parent = Descriptors::default();
    let fd = fs.open(&mut parent, ROOT, "/etc/text", OPEN_READ).unwrap();
    parent.set_close_on_exec(fd, true).unwrap();
    fs.dup2(&mut parent, fd, 9).unwrap();
    assert!(!parent.close_on_exec(9).unwrap());
    let mut child = fs.inherit(&parent).unwrap();
    fs.close_exec(&mut child);
    assert!(!child.contains(fd));
    assert!(child.contains(9) && parent.contains(fd));
    let mut bytes = [0; 3];
    assert_eq!(fs.read(&parent, fd, &mut bytes), Ok(3));
    assert_eq!(&bytes, b"abc");
    assert_eq!(fs.read(&child, 9, &mut bytes), Ok(3));
    assert_eq!(&bytes, b"def");
    fs.clear(&mut parent);
    fs.clear(&mut child);
    assert_eq!(fs.live_objects(), (0, 0));
}

#[test]
fn directory_peek_does_not_consume_and_device_truncate_does_not_create_storage() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut fds = Descriptors::default();
    let directory = fs
        .open(&mut fds, ROOT, "/etc", OPEN_READ | OPEN_DIRECTORY)
        .unwrap();
    let a = fs.peek_readdir(&fds, directory).unwrap().unwrap();
    let b = fs.peek_readdir(&fds, directory).unwrap().unwrap();
    assert_eq!(a.inode, b.inode);
    assert_eq!(fs.readdir(&fds, directory).unwrap().unwrap().inode, a.inode);
    assert!(fs.peek_readdir(&fds, directory).unwrap().is_none());
    let null = fs
        .open(
            &mut fds,
            ROOT,
            "/dev/null",
            OPEN_WRITE | OPEN_TRUNCATE | OPEN_CREATE,
        )
        .unwrap();
    assert_eq!(fs.write(&fds, null, b"discard"), Ok(7));
    assert_eq!(fs.descriptor_metadata(&fds, null).unwrap().2, 0);
    assert_eq!(fs.metadata_nofollow(ROOT, "/text").unwrap().1, 3);
    assert_eq!(fs.metadata(ROOT, "/text").unwrap().1, 1);
    fs.clear(&mut fds);
}

#[test]
fn socket_description_survives_dup_fork_exec_and_shared_thread_close() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut parent = fs.standard().unwrap();
    let socket = fs.socket(&mut parent, 7).unwrap();
    parent.set_close_on_exec(socket, true).unwrap();
    let alias = fs.dup2(&mut parent, socket, 10).unwrap();
    fs.set_nonblocking(&parent, alias, true).unwrap();
    assert_eq!(fs.nonblocking(&parent, socket), Ok(true));
    let mut child = fs.inherit(&parent).unwrap();
    fs.close_exec(&mut parent);
    assert!(!parent.contains(socket));
    assert_eq!(fs.socket_id(&child, socket), Ok(Some(7)));
    fs.clear(&mut parent);
    assert!(fs.socket_live(7));
    let mut thread = child.share().unwrap();
    fs.close(&mut thread, alias).unwrap();
    assert!(!child.contains(alias));
    fs.clear(&mut child);
    assert!(fs.socket_live(7));
    fs.clear(&mut thread);
    assert!(!fs.socket_live(7));
    assert_eq!(fs.live_objects(), (0, 0));
}

#[test]
fn private_snapshots_preserve_offsets_modes_and_modification_times() {
    let image = image();
    let mut fs = Vfs::new(&image).unwrap();
    let mut files = fs.standard().unwrap();
    fs.set_clock(1_000_000_010);
    let fd = fs
        .open(
            &mut files,
            ROOT,
            "/tmp/object",
            OPEN_CREATE | OPEN_READ | OPEN_WRITE,
        )
        .unwrap();
    fs.fchmod(&files, fd, 0o600).unwrap();
    fs.write(&files, fd, b"object-one").unwrap();
    let inode = fs.metadata(ROOT, "/tmp/object").unwrap().0;
    assert_eq!(fs.attributes(inode), (0o600, 1_000_000_010));
    let mapping = fs.mapping_bytes(&files, fd, 0, 4096).unwrap();
    assert_eq!(fs.seek(&files, fd, 0, 1), Ok(10));
    fs.set_clock(2_000_000_020);
    fs.seek(&files, fd, 0, 0).unwrap();
    fs.write(&files, fd, b"object-two").unwrap();
    fs.chmod(ROOT, "/tmp/object", 0o755).unwrap();
    assert_eq!(&*mapping, b"object-one");
    assert_eq!(fs.attributes(inode), (0o755, 2_000_000_020));
    assert_eq!(fs.mapping_bytes(&files, fd, 11, 1), Err(INVALID_ARGUMENT));
    assert_eq!(fs.fchmod(&files, fd, 0o4755), Err(UNSUPPORTED));
    fs.close(&mut files, fd).unwrap();
}
