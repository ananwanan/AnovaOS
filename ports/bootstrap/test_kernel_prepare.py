# SPDX-License-Identifier: Apache-2.0
# Reject changed kernel inputs, unsafe archives, redirected publication and dependency-lock drift.
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest import mock


spec = importlib.util.spec_from_file_location("kernel_prepare", Path(__file__).with_name("kernel_prepare.py"))
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class KernelInputTests(unittest.TestCase):
    def archive(self, directory, members):
        path = Path(directory) / "input.tar.gz"
        with tarfile.open(path, "w:gz") as archive:
            for name, kind, data in members:
                member = tarfile.TarInfo(name)
                member.type = kind
                if kind == tarfile.REGTYPE:
                    member.size = len(data)
                    archive.addfile(member, io.BytesIO(data))
                else:
                    member.linkname = "outside"
                    archive.addfile(member)
        return path, hashlib.sha256(path.read_bytes()).hexdigest()

    def test_authentication_precedes_tar_parsing_and_rejects_redirected_archives(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "invalid"
            path.write_bytes(b"not a tar archive")
            with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
                prepare.archive_files(path, "0" * 64, "crate")
            link = Path(temporary) / "link"
            link.symlink_to(path)
            with self.assertRaisesRegex(RuntimeError, "non-symlink"):
                prepare.authenticated(link, prepare.digest(path.read_bytes()))

    def test_rejects_escape_duplicates_links_devices_and_namespace_conflicts(self):
        cases = [
            [("crate/../escape", tarfile.REGTYPE, b"x")],
            [("/crate/file", tarfile.REGTYPE, b"x")],
            [("crate//file", tarfile.REGTYPE, b"x")],
            [("crate/./file", tarfile.REGTYPE, b"x")],
            [("other/file", tarfile.REGTYPE, b"x")],
            [("crate/back\\slash", tarfile.REGTYPE, b"x")],
            [("crate", tarfile.REGTYPE, b"x")],
            [("crate/file", tarfile.REGTYPE, b"x")] * 2,
            [("crate/link", tarfile.SYMTYPE, b"")],
            [("crate/link", tarfile.LNKTYPE, b"")],
            [("crate/device", tarfile.CHRTYPE, b"")],
            [("crate/fifo", tarfile.FIFOTYPE, b"")],
            [("crate/file", tarfile.REGTYPE, b"x"),
             ("crate/file/child", tarfile.REGTYPE, b"y")],
            [("crate/file/child", tarfile.REGTYPE, b"y"),
             ("crate/file", tarfile.REGTYPE, b"x")],
        ]
        for members in cases:
            with self.subTest(members=members), tempfile.TemporaryDirectory() as temporary:
                path, checksum = self.archive(temporary, members)
                with self.assertRaises(RuntimeError):
                    prepare.archive_files(path, checksum, "crate")
                self.assertEqual(list(Path(temporary).iterdir()), [path])

    def test_valid_archive_preserves_complete_upstream_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            path, checksum = self.archive(temporary, [
                ("crate", tarfile.DIRTYPE, b""),
                ("crate/src/lib.rs", tarfile.REGTYPE, b"#![no_std]\n"),
                ("crate/LICENSE", tarfile.REGTYPE, b"unchanged upstream license\n"),
            ])
            self.assertEqual(prepare.archive_files(path, checksum, "crate"), {
                "src/lib.rs": b"#![no_std]\n", "LICENSE": b"unchanged upstream license\n",
            })

    def test_inventory_rejects_nested_directory_redirect_and_special_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            (base / "nested").symlink_to(base, target_is_directory=True)
            with self.assertRaisesRegex(RuntimeError, "Unsafe prepared tree"):
                prepare.inventory(base)
            (base / "nested").unlink()
            (base / "file").write_bytes(b"data")
            self.assertEqual(prepare.inventory(base), {"file": prepare.digest(b"data")})

    def test_publication_rejects_modified_missing_extra_files_and_empty_directories(self):
        for mutation in ["modified", "missing", "extra", "directory"]:
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                work = Path(temporary)
                manifest = {}
                for name in ["source", "seeds"]:
                    prepare.write_tree(work / name, {"input": b"original"})
                    manifest[name + "_files_sha256"] = prepare.inventory(work / name)
                    manifest[name + "_directories"] = []
                (work / "inputs.json").write_text(json.dumps(manifest))
                with mock.patch.object(prepare, "WORK", work):
                    self.assertEqual(prepare.published(), manifest)
                    if mutation == "modified":
                        (work / "source/input").write_bytes(b"changed")
                    elif mutation == "missing":
                        (work / "source/input").unlink()
                    elif mutation == "extra":
                        (work / "source/extra").write_bytes(b"extra")
                    else:
                        (work / "source/extra").mkdir()
                    with self.assertRaisesRegex(RuntimeError, "changed"):
                        prepare.published()

    def test_isolation_excludes_host_tools_preserving_kernel_profiles(self):
        original = (prepare.ROOT / "Cargo.toml").read_bytes()
        isolated = prepare.isolated_manifest(original).decode()
        self.assertNotIn('"xtask"', isolated)
        self.assertIn('default-members = ["kernel"]', isolated)
        self.assertEqual(isolated.split("[workspace.package]", 1)[1],
                         original.decode().split("[workspace.package]", 1)[1])

    def test_lock_rejects_version_and_checksum_drift(self):
        original = b'''version = 4
[[package]]
name = "dependency"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "trusted"
'''
        self.assertEqual(prepare.validate_lock(original, original)[0]["version"], "1.0.0")
        for changed in [original.replace(b'"1.0.0"', b'"2.0.0"'),
                        original.replace(b'"trusted"', b'"changed"')]:
            with self.assertRaisesRegex(RuntimeError, "lock drifted"):
                prepare.validate_lock(original, changed)

    def test_initramfs_path_budget_rejects_long_components(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            (base / ("x" * 64)).write_bytes(b"x")
            with self.assertRaisesRegex(RuntimeError, "initramfs bounds"):
                prepare.check_guest_paths(base, "/ks/source")

    def test_verification_is_read_only_and_rejects_stale_repository_input(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            work = root / "build/bootstrap-kernel"
            work.mkdir(parents=True)
            scripts = root / "ports/bootstrap"
            scripts.mkdir(parents=True)
            (scripts / "kernel_prepare.py").write_bytes(b"recipe")
            archive = work / "target.tar.xz"
            archive.write_bytes(b"locked seed archive")
            record = {"name": archive.name, "sha256": prepare.digest(archive.read_bytes())}
            (scripts / "kernel-inputs.json").write_text(json.dumps(record))
            manifest = {
                "repository_files_sha256": {"kernel/src/main.rs": prepare.digest(b"source")},
                "recipe_files_sha256": {
                    str(path.relative_to(root)): prepare.digest(path.read_bytes())
                    for path in scripts.iterdir()
                },
                "registry_packages": [],
            }
            for name in ["source", "seeds"]:
                prepare.write_tree(work / name, {"input": b"unchanged"})
                manifest[name + "_files_sha256"] = prepare.inventory(work / name)
                manifest[name + "_directories"] = []
            (work / "inputs.json").write_text(json.dumps(manifest))
            before = prepare.inventory(root)
            before_directories = prepare.directories(root)
            with mock.patch.object(prepare, "ROOT", root), mock.patch.object(prepare, "WORK", work):
                with mock.patch.object(prepare, "source_inputs", return_value={"kernel/src/main.rs": b"source"}):
                    prepare.main(["--verify"])
                self.assertEqual(prepare.inventory(root), before)
                self.assertEqual(prepare.directories(root), before_directories)
                with mock.patch.object(prepare, "source_inputs", return_value={"kernel/src/main.rs": b"changed"}):
                    with self.assertRaisesRegex(RuntimeError, "source or preparation recipe changed"):
                        prepare.main(["--verify"])
                self.assertEqual(prepare.inventory(root), before)


if __name__ == "__main__":
    unittest.main()
