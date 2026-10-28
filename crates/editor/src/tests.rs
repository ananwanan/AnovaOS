// SPDX-License-Identifier: Apache-2.0
//! Exercise exact editor bytes, bounded input, conflicts and each publication failure stage.

use super::*;
use anova_abi::{ALREADY_EXISTS, IO_ERROR, NO_SPACE};
use std::{collections::BTreeMap, string::String, vec::Vec};

#[derive(Default)]
struct Disk {
    files: BTreeMap<String, Vec<u8>>,
    step: usize,
    fail: Option<(usize, i64)>,
}
impl Disk {
    fn fixture() -> Self {
        let mut disk = Self::default();
        disk.files
            .insert(String::from("/persist/test.c"), b"old\n".to_vec());
        disk
    }
    fn operation(&mut self) -> Result<(), i64> {
        self.step += 1;
        match self.fail {
            Some((step, error)) if step == self.step => Err(error),
            _ => Ok(()),
        }
    }
}
impl Fs for Disk {
    fn read(&mut self, path: &str, output: &mut [u8]) -> Result<usize, i64> {
        self.operation()?;
        let bytes = self.files.get(path).ok_or(NOT_FOUND)?;
        if bytes.len() > output.len() {
            return Err(TOO_LONG);
        }
        output[..bytes.len()].copy_from_slice(bytes);
        Ok(bytes.len())
    }
    fn write_new(&mut self, path: &str, bytes: &[u8]) -> Result<(), i64> {
        self.operation()?;
        if self.files.contains_key(path) {
            return Err(ALREADY_EXISTS);
        }
        self.files.insert(String::from(path), Vec::new());
        // Separate actual exclusive creation, partial write, completion and flush failures.
        self.operation()?;
        let middle = bytes.len() / 2;
        self.files
            .get_mut(path)
            .unwrap()
            .extend_from_slice(&bytes[..middle]);
        self.operation()?;
        self.files
            .get_mut(path)
            .unwrap()
            .extend_from_slice(&bytes[middle..]);
        self.operation()?;
        Ok(())
    }
    fn rename(&mut self, from: &str, to: &str) -> Result<(), i64> {
        if self.files.contains_key(to) {
            return Err(ALREADY_EXISTS);
        }
        self.replace(from, to)
    }
    fn replace(&mut self, from: &str, to: &str) -> Result<(), i64> {
        self.operation()?;
        let bytes = self.files.remove(from).ok_or(NOT_FOUND)?;
        self.files.insert(String::from(to), bytes);
        Ok(())
    }
    fn remove(&mut self, path: &str) -> Result<(), i64> {
        self.operation()?;
        self.files.remove(path).ok_or(NOT_FOUND).map(|_| ())
    }
    fn sync_parent(&mut self, _: &str) -> Result<(), i64> {
        self.operation()
    }
}

fn edited(disk: &mut Disk) -> Editor {
    let mut editor = Editor::new();
    editor.open(disk, "/persist/test.c", false).unwrap();
    editor.move_end();
    editor.insert(b" new").unwrap();
    disk.step = 0;
    editor
}

#[test]
fn saves_and_reopens_exact_c_cpp_rust_bytes_including_crlf_and_tabs() {
    let mut disk = Disk::default();
    let mut editor = Editor::new();
    for (path, source) in [
        (
            "/persist/test.c",
            &b"int main(void) {\r\n\treturn 7;\r\n}\r\n"[..],
        ),
        (
            "/persist/test.cpp",
            &b"extern \"C\" int main() { return 8; }\n"[..],
        ),
        (
            "/persist/test.rs",
            &b"fn main() { println!(\"M17\"); }\n"[..],
        ),
    ] {
        editor.create(path, false).unwrap();
        editor.insert(source).unwrap();
        assert!(editor.dirty());
        assert_eq!(
            editor.save(&mut disk, false),
            Ok(SaveOutcome {
                backup_retained: false
            })
        );
        assert!(!editor.dirty());
        editor.close(false).unwrap();
        editor.open(&mut disk, path, false).unwrap();
        assert_eq!(editor.bytes(), source);
    }
}

#[test]
fn all_prepublication_and_flush_errors_keep_dirty_and_a_complete_prior_copy() {
    // Steps: read, temporary create/write/write/fsync, backup create/write/write/fsync,
    // backup parent flush, replace, publication parent flush, optional backup cleanup.
    for step in 1..=12 {
        for error in [IO_ERROR, NO_SPACE] {
            let mut disk = Disk::fixture();
            let mut editor = edited(&mut disk);
            let pending = editor.bytes().to_vec();
            disk.fail = Some((step, error));
            assert_eq!(
                editor.save(&mut disk, false),
                Err(Error::Io(error)),
                "step {step}"
            );
            assert!(editor.dirty(), "step {step}");
            assert_eq!(editor.bytes(), pending);
            let target = disk.files.get("/persist/test.c").unwrap();
            if target != b"old\n" {
                assert_eq!(target, &pending);
                assert_eq!(disk.files.get("/persist/test.c.m17-bak").unwrap(), b"old\n");
            }
        }
    }
}

#[test]
fn backup_cleanup_error_is_explicit_success_and_leaves_recovery_bytes() {
    let mut disk = Disk::fixture();
    let mut editor = edited(&mut disk);
    disk.fail = Some((13, IO_ERROR));
    assert_eq!(
        editor.save(&mut disk, false),
        Ok(SaveOutcome {
            backup_retained: true
        })
    );
    assert!(!editor.dirty());
    assert_eq!(disk.files.get("/persist/test.c.m17-bak").unwrap(), b"old\n");
}

#[test]
fn unsaved_navigation_and_close_are_blocked_without_altering_the_buffer() {
    let mut disk = Disk::fixture();
    let mut editor = edited(&mut disk);
    let pending = editor.bytes().to_vec();
    assert_eq!(
        editor.open(&mut disk, "/persist/test.c", false),
        Err(Error::Unsaved)
    );
    assert_eq!(editor.create("/persist/new.rs", false), Err(Error::Unsaved));
    assert_eq!(editor.close(false), Err(Error::Unsaved));
    assert_eq!(
        editor.rename(&mut disk, "/persist/new.c", false),
        Err(Error::Unsaved)
    );
    assert_eq!(editor.delete_file(&mut disk, false), Err(Error::Unsaved));
    assert_eq!(editor.bytes(), pending);
    editor.open(&mut disk, "/persist/test.c", true).unwrap();
    assert_eq!(editor.bytes(), b"old\n");
    assert!(!editor.dirty());
}

#[test]
fn external_overwrites_deletes_and_new_file_collisions_need_explicit_confirmation() {
    let mut disk = Disk::fixture();
    let mut editor = edited(&mut disk);
    disk.files
        .insert(String::from("/persist/test.c"), b"external\n".to_vec());
    assert_eq!(editor.save(&mut disk, false), Err(Error::Conflict));
    assert_eq!(disk.files.get("/persist/test.c").unwrap(), b"external\n");
    editor.save(&mut disk, true).unwrap();
    disk.files.remove("/persist/test.c");
    editor.insert(b"!").unwrap();
    assert_eq!(editor.save(&mut disk, false), Err(Error::Conflict));
    editor.save(&mut disk, true).unwrap();
    editor.create("/persist/test.c", true).unwrap();
    assert_eq!(editor.save(&mut disk, false), Err(Error::Conflict));
}

#[test]
fn recovery_siblings_and_rename_targets_are_never_silently_overwritten() {
    for suffix in [".m17-tmp", ".m17-bak"] {
        let mut disk = Disk::fixture();
        let mut editor = edited(&mut disk);
        let recovery = std::format!("/persist/test.c{suffix}");
        disk.files.insert(recovery.clone(), b"recovery".to_vec());
        assert_eq!(
            editor.save(&mut disk, false),
            Err(Error::Io(ALREADY_EXISTS))
        );
        assert!(editor.dirty());
        assert_eq!(disk.files.get(&recovery).unwrap(), b"recovery");
        assert_eq!(disk.files.get("/persist/test.c").unwrap(), b"old\n");
    }
    let mut disk = Disk::fixture();
    let mut editor = Editor::new();
    editor.open(&mut disk, "/persist/test.c", false).unwrap();
    disk.files
        .insert(String::from("/persist/taken.c"), b"other".to_vec());
    assert_eq!(
        editor.rename(&mut disk, "/persist/taken.c", false),
        Err(Error::Io(ALREADY_EXISTS))
    );
    assert_eq!(editor.path(), "/persist/test.c");
    editor
        .rename(&mut disk, "/persist/renamed.c", false)
        .unwrap();
    assert_eq!(editor.path(), "/persist/renamed.c");
    editor.delete_file(&mut disk, false).unwrap();
    assert!(!editor.is_open());
}

#[test]
fn limits_and_invalid_inputs_preserve_existing_buffer_and_storage() {
    let mut disk = Disk::fixture();
    let mut editor = edited(&mut disk);
    for path in [
        "/tmp/a.c",
        "/persist/../a",
        "/persist//a",
        "/persist/a/",
        "/persist/a.m17-bak",
    ] {
        assert_eq!(editor.create(path, true), Err(Error::InvalidPath));
    }
    assert_eq!(editor.insert(b"\0"), Err(Error::InvalidText));
    editor.create("/persist/bound.rs", true).unwrap();
    editor.insert(&[b'x'; LIMIT]).unwrap();
    assert_eq!(editor.insert(b"x"), Err(Error::TooLong));
    editor.save(&mut disk, false).unwrap();
    disk.files.insert(
        String::from("/persist/large.rs"),
        std::vec![b'x'; LIMIT + 1],
    );
    assert_eq!(
        editor.open(&mut disk, "/persist/large.rs", false),
        Err(Error::TooLong)
    );
    assert_eq!(editor.bytes().len(), LIMIT);
    disk.files
        .insert(String::from("/persist/binary.rs"), std::vec![0xff]);
    assert_eq!(
        editor.open(&mut disk, "/persist/binary.rs", false),
        Err(Error::InvalidText)
    );
    assert_eq!(editor.path(), "/persist/bound.rs");
}

#[test]
fn cursor_edits_and_undo_to_saved_bytes_update_dirty_exactly() {
    let mut disk = Disk::fixture();
    let mut editor = Editor::new();
    editor.open(&mut disk, "/persist/test.c", false).unwrap();
    editor.insert(b"X").unwrap();
    editor.backspace();
    assert!(!editor.dirty());
    editor.move_down();
    assert_eq!(editor.cursor(), 4);
    editor.move_up();
    assert_eq!(editor.cursor(), 0);
    editor.move_right();
    editor.move_up();
    assert_eq!(editor.cursor(), 1);
    editor.delete();
    assert_eq!(editor.bytes(), b"od\n");
    editor.insert(b"l").unwrap();
    assert_eq!(editor.bytes(), b"old\n");
    assert!(!editor.dirty());
}
