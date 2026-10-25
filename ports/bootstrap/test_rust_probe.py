# SPDX-License-Identifier: Apache-2.0
# Reject unauthenticated Rust archives, unsafe metadata and altered or redirected source caches.
import hashlib
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest import mock


spec = importlib.util.spec_from_file_location("rust_probe", Path(__file__).with_name("rust_probe.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class RustArchiveTests(unittest.TestCase):
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
                    member.linkname = data.decode()
                    archive.addfile(member)
        record = {"name": path.name, "url": "https://example.invalid/input.tar.gz",
                  "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "root": "rust-test"}
        return path, record

    def test_authentication_precedes_parsing_and_symlinks_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "input.tar.gz"
            path.write_bytes(b"not a tar archive")
            record = {"name": path.name, "sha256": "0" * 64, "root": "rust-test"}
            with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
                probe.audit_archive(path, record, {})
            link = Path(directory) / "link.tar.gz"
            link.symlink_to(path)
            with self.assertRaisesRegex(RuntimeError, "regular source archive"):
                probe.audit_archive(link, record, {})

    def test_reject_traversal_ambiguity_devices_and_directory_conflicts(self):
        cases = [
            [("rust-test/../escape", tarfile.REGTYPE, b"x")],
            [("/rust-test/file", tarfile.REGTYPE, b"x")],
            [("other/file", tarfile.REGTYPE, b"x")],
            [("rust-test//file", tarfile.REGTYPE, b"x")],
            [("rust-test/./file", tarfile.REGTYPE, b"x")],
            [("rust-test/file", tarfile.REGTYPE, b"x")] * 2,
            [("rust-test/fifo", tarfile.FIFOTYPE, b"")],
            [("rust-test/device", tarfile.CHRTYPE, b"")],
            [("rust-test/sub", tarfile.REGTYPE, b"x"),
             ("rust-test/sub/file", tarfile.REGTYPE, b"x")],
            [("rust-test/sub/file", tarfile.REGTYPE, b"x"),
             ("rust-test/sub", tarfile.REGTYPE, b"x")],
            [("rust-test/link", tarfile.SYMTYPE, b"sub"),
             ("rust-test/link/file", tarfile.REGTYPE, b"x")],
        ]
        for members in cases:
            with self.subTest(members=members), tempfile.TemporaryDirectory() as directory:
                path, record = self.archive(directory, members)
                with self.assertRaises(RuntimeError):
                    probe.audit_archive(path, record, {})
                self.assertEqual([entry.name for entry in Path(directory).iterdir()], [path.name])

    def test_safe_links_are_metadata_only_and_selected_links_fail(self):
        members = [
            ("rust-test/sub/file", tarfile.REGTYPE, b"script"),
            ("rust-test/sub/link", tarfile.SYMTYPE, b"../sub/file"),
            ("rust-test/hardlink", tarfile.LNKTYPE, b"rust-test/sub/file"),
        ]
        with tempfile.TemporaryDirectory() as directory:
            path, record = self.archive(directory, members)
            report, files, _ = probe.audit_archive(path, record, {"sub/file": "file"})
            self.assertEqual(report["links"], 2)
            self.assertEqual(report["regular_files"], 1)
            self.assertEqual(report["logical_file_bytes"], 6)
            self.assertEqual(files, {"file": b"script"})
            self.assertEqual(report["link_metadata"][0]["resolved_lexical_target"], "rust-test/sub/file")
            self.assertEqual([item.name for item in Path(directory).iterdir()], [path.name])
            with self.assertRaisesRegex(RuntimeError, "not a regular file"):
                probe.audit_archive(path, record, {"sub/link": "link"})

    def test_escaping_links_are_rejected_without_following(self):
        cases = [
            ("rust-test/link", tarfile.SYMTYPE, b"../outside"),
            ("rust-test/sub/link", tarfile.SYMTYPE, b"../../outside"),
            ("rust-test/link", tarfile.SYMTYPE, b"/outside"),
            ("rust-test/link", tarfile.SYMTYPE, b"..\\outside"),
            ("rust-test/link", tarfile.LNKTYPE, b"other/file"),
        ]
        for member in cases:
            with self.subTest(member=member), tempfile.TemporaryDirectory() as directory:
                path, record = self.archive(directory, [member])
                with self.assertRaisesRegex(RuntimeError, "(Unsafe|Escaping) Rust source link"):
                    probe.audit_archive(path, record, {})

    def test_capacity_counts_and_selected_limits_and_missing_inputs(self):
        members = [
            ("rust-test", tarfile.DIRTYPE, b""),
            ("rust-test/x.py", tarfile.REGTYPE, b"script"),
            ("rust-test/src/llvm-project/llvm/large", tarfile.REGTYPE, b"123456789"),
            ("rust-test/src/tools/cargo/Cargo.toml", tarfile.REGTYPE, b"cargo"),
        ]
        with tempfile.TemporaryDirectory() as directory:
            path, record = self.archive(directory, members)
            with mock.patch.object(probe, "FILE_LIMIT", 6):
                report, files, selections = probe.audit_archive(path, record, {"x.py": "rust-x.py"})
                self.assertEqual(report["regular_files"], 3)
                self.assertEqual(report["logical_file_bytes"], 20)
                self.assertEqual(report["regular_file_data_block_bytes_lower_bound"], 12288)
                self.assertEqual(report["over_file_limit_count"], 1)
                self.assertEqual(report["namespace_nodes_lower_bound"], 9)
                components = {item["name"]: item for item in report["components"]}
                self.assertEqual(components["llvm-project"]["logical_file_bytes"], 9)
                self.assertEqual(components["cargo"]["regular_files"], 1)
                self.assertEqual(files, {"rust-x.py": b"script"})
                self.assertEqual(selections[0]["sha256"], hashlib.sha256(b"script").hexdigest())
                with self.assertRaisesRegex(RuntimeError, "exceeds the guest file limit"):
                    probe.audit_archive(path, record, {"src/llvm-project/llvm/large": "large"})
            with self.assertRaisesRegex(RuntimeError, "missing a selected"):
                probe.audit_archive(path, record, {"absent": "absent"})
            with self.assertRaisesRegex(RuntimeError, "flat probe destination"):
                probe.audit_archive(path, record, {"x.py": "../escape"})


class RustPublicationTests(unittest.TestCase):
    def paths(self, directory):
        root = Path(directory)
        (root / "build").mkdir()
        return root, root / "build/bootstrap-rust"

    def test_verify_is_readonly_and_changes_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root, work = self.paths(directory)
            with mock.patch.object(probe, "ROOT", root), mock.patch.object(probe, "WORK", work):
                report = {"selected_inputs": []}
                inputs = {"rust-x.py": b"unchanged source"}
                probe.publish(report, inputs)
                before = {str(path.relative_to(work)): (path.read_bytes(), path.stat().st_mtime_ns)
                          for path in work.rglob("*") if path.is_file()}
                probe.publish(report, inputs, verify=True)
                after = {str(path.relative_to(work)): (path.read_bytes(), path.stat().st_mtime_ns)
                         for path in work.rglob("*") if path.is_file()}
                self.assertEqual(before, after)
                (work / "source/rust-x.py").write_bytes(b"changed")
                with self.assertRaisesRegex(RuntimeError, "source cache changed"):
                    probe.publish(report, inputs)
                (work / "source/rust-x.py").write_bytes(inputs["rust-x.py"])
                (work / "source-audit.json").write_bytes(b"changed")
                with self.assertRaisesRegex(RuntimeError, "source audit changed"):
                    probe.publish(report, inputs)

    def test_symlink_publication_paths_preserve_targets(self):
        for target in ("work", "report", "source", "file"):
            with self.subTest(target=target), tempfile.TemporaryDirectory() as directory:
                root, work = self.paths(directory)
                victim = root / "victim"
                victim.mkdir()
                (victim / "data").write_bytes(b"preserved")
                if target == "work":
                    work.symlink_to(victim, target_is_directory=True)
                else:
                    work.mkdir()
                    if target == "report":
                        (work / "source-audit.json").symlink_to(victim / "missing")
                    elif target == "source":
                        (work / "source").symlink_to(victim, target_is_directory=True)
                    else:
                        (work / "source").mkdir()
                        for name in ("rust-x.py", "INPUTS.json", "SOURCE.sha256"):
                            (work / "source" / name).symlink_to(victim / "data")
                with mock.patch.object(probe, "ROOT", root), mock.patch.object(probe, "WORK", work):
                    with self.assertRaisesRegex(RuntimeError, "non-symlink"):
                        probe.publish({"selected_inputs": []}, {"rust-x.py": b"source"})
                self.assertEqual((victim / "data").read_bytes(), b"preserved")
                self.assertEqual([entry.name for entry in victim.iterdir()], ["data"])

    def test_verify_never_creates_work_or_downloads(self):
        with tempfile.TemporaryDirectory() as directory:
            root, work = self.paths(directory)
            with mock.patch.object(probe, "ROOT", root), mock.patch.object(probe, "WORK", work):
                with self.assertRaises(FileNotFoundError):
                    probe.publish({"selected_inputs": []}, {}, verify=True)
                self.assertFalse(work.exists())
                work.mkdir()
                (work / "cache").mkdir()
                record = {"name": "missing.tar.xz", "sha256": "0" * 64}
                with mock.patch.object(probe.subprocess, "run") as run:
                    with self.assertRaises(FileNotFoundError):
                        probe.cached_archive(record, verify=True)
                    run.assert_not_called()
                self.assertEqual([item.name for item in work.iterdir()], ["cache"])

    def test_cache_symlinks_and_failed_download_do_not_publish(self):
        with tempfile.TemporaryDirectory() as directory:
            root, work = self.paths(directory)
            work.mkdir()
            victim = root / "victim"
            victim.mkdir()
            (work / "cache").symlink_to(victim, target_is_directory=True)
            record = {"name": "source.tar.xz", "url": "https://example.invalid/source",
                      "sha256": "0" * 64}
            with mock.patch.object(probe, "WORK", work):
                with self.assertRaisesRegex(RuntimeError, "non-symlink"):
                    probe.cached_archive(record, verify=False)
                self.assertEqual(list(victim.iterdir()), [])
                (work / "cache").unlink()

                def bad_download(argv, check):
                    Path(argv[-1]).write_bytes(b"corrupt download")

                with mock.patch.object(probe.subprocess, "run", side_effect=bad_download):
                    with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
                        probe.cached_archive(record, verify=False)
                self.assertEqual(list((work / "cache").iterdir()), [])

    def test_verified_download_is_cached_and_existing_corruption_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root, work = self.paths(directory)
            work.mkdir()
            data = b"authenticated source archive"
            record = {"name": "source.tar.xz", "url": "https://example.invalid/source",
                      "sha256": hashlib.sha256(data).hexdigest()}

            def good_download(argv, check):
                Path(argv[-1]).write_bytes(data)

            with mock.patch.object(probe, "WORK", work):
                with mock.patch.object(probe.subprocess, "run", side_effect=good_download) as run:
                    cached = probe.cached_archive(record, verify=False)
                    self.assertEqual(cached.read_bytes(), data)
                    self.assertEqual(cached.stat().st_nlink, 1)
                    self.assertEqual([item.name for item in cached.parent.iterdir()], [record["name"]])
                    probe.cached_archive(record, verify=True)
                    probe.cached_archive(record, verify=False)
                    self.assertEqual(run.call_count, 1)
                    cached.write_bytes(b"tampered archive")
                    with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
                        probe.cached_archive(record, verify=False)
                    self.assertEqual(run.call_count, 1)


if __name__ == "__main__":
    unittest.main()
