# SPDX-License-Identifier: Apache-2.0
# Reject corrupt compiler archives, unsafe metadata and modified or redirected probe caches.
import hashlib
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest import mock


spec = importlib.util.spec_from_file_location("gcc_probe", Path(__file__).with_name("gcc_probe.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class CompilerArchiveTests(unittest.TestCase):
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
        record = [path.name, "https://example.invalid/input.tar.gz",
                  hashlib.sha256(path.read_bytes()).hexdigest(), "gcc-test"]
        return path, record

    def test_checksum_precedes_archive_parsing_and_symlinks_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "input.tar.gz"
            path.write_bytes(b"not a tar archive")
            record = [path.name, "https://example.invalid/input", "0" * 64, "gcc-test"]
            with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
                probe.audit_archive(path, record, {})
            link = Path(directory) / "link.tar.gz"
            link.symlink_to(path)
            with self.assertRaisesRegex(RuntimeError, "regular source archive"):
                probe.audit_archive(link, record, {})

    def test_reject_paths_links_duplicates_and_special_members_without_extraction(self):
        cases = [
            [("gcc-test/../escape", tarfile.REGTYPE, b"x")],
            [("/gcc-test/file", tarfile.REGTYPE, b"x")],
            [("other/file", tarfile.REGTYPE, b"x")],
            [("gcc-test//file", tarfile.REGTYPE, b"x")],
            [("gcc-test/./file", tarfile.REGTYPE, b"x")],
            [("gcc-test/file", tarfile.REGTYPE, b"x")] * 2,
            [("gcc-test/link", tarfile.SYMTYPE, b"file")],
            [("gcc-test/link", tarfile.SYMTYPE, b"../../escape")],
            [("gcc-test/link", tarfile.LNKTYPE, b"gcc-test/file")],
            [("gcc-test/fifo", tarfile.FIFOTYPE, b"")],
            [("gcc-test/device", tarfile.CHRTYPE, b"")],
        ]
        for members in cases:
            with self.subTest(members=members), tempfile.TemporaryDirectory() as directory:
                path, record = self.archive(directory, members)
                with self.assertRaisesRegex(RuntimeError, "Unsafe or ambiguous"):
                    probe.audit_archive(path, record, {})
                self.assertEqual([entry.name for entry in Path(directory).iterdir()], [path.name])

    def test_real_metadata_counts_missing_inputs_and_selected_file_limit(self):
        members = [
            ("gcc-test", tarfile.DIRTYPE, b""),
            ("gcc-test/configure", tarfile.REGTYPE, b"script"),
            ("gcc-test/nested/large", tarfile.REGTYPE, b"123456789"),
        ]
        with tempfile.TemporaryDirectory() as directory:
            path, record = self.archive(directory, members)
            with mock.patch.object(probe, "FILE_LIMIT", 6):
                report, selected, entries = probe.audit_archive(path, record, {"configure": "configure"})
                self.assertEqual(report["regular_files"], 2)
                self.assertEqual(report["directories"], 1)
                self.assertEqual(report["logical_file_bytes"], 15)
                self.assertEqual(report["namespace_nodes_lower_bound"], 4)
                self.assertEqual(report["regular_file_data_block_bytes_lower_bound"], 8192)
                self.assertEqual(report["largest_file"], {"path": "gcc-test/nested/large", "size": 9})
                self.assertEqual(report["over_file_limit"], [{"path": "gcc-test/nested/large", "size": 9}])
                self.assertEqual(selected, {"configure": b"script"})
                self.assertEqual(entries[0]["sha256"], hashlib.sha256(b"script").hexdigest())
                with self.assertRaisesRegex(RuntimeError, "exceeds the guest file limit"):
                    probe.audit_archive(path, record, {"nested/large": "large"})
            with self.assertRaisesRegex(RuntimeError, "missing a selected"):
                probe.audit_archive(path, record, {"absent": "absent"})
            with self.assertRaisesRegex(RuntimeError, "flat probe destination"):
                probe.audit_archive(path, record, {"configure": "../escape"})


class ProbePublicationTests(unittest.TestCase):
    def setup_paths(self, directory):
        root = Path(directory)
        (root / "build").mkdir()
        return root, root / "build/bootstrap-gcc"

    def test_verification_is_readonly_and_rejects_source_or_manifest_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root, work = self.setup_paths(directory)
            with mock.patch.object(probe, "ROOT", root), mock.patch.object(probe, "WORK", work):
                report = {"selected_inputs": []}
                inputs = {"configure": b"the exact source"}
                probe.publish(report, inputs)
                before = {str(path.relative_to(work)): (path.read_bytes(), path.stat().st_mtime_ns)
                          for path in work.rglob("*") if path.is_file()}
                probe.publish(report, inputs, verify=True)
                after = {str(path.relative_to(work)): (path.read_bytes(), path.stat().st_mtime_ns)
                         for path in work.rglob("*") if path.is_file()}
                self.assertEqual(before, after)
                (work / "source/configure").write_bytes(b"changed source")
                with self.assertRaisesRegex(RuntimeError, "source cache changed"):
                    probe.publish(report, inputs)
                (work / "source/configure").write_bytes(inputs["configure"])
                (work / "source-audit.json").write_bytes(b"changed audit")
                with self.assertRaisesRegex(RuntimeError, "source audit changed"):
                    probe.publish(report, inputs)

    def test_reject_symlink_work_report_source_or_file_without_touching_target(self):
        for target in ("work", "report", "source", "file"):
            with self.subTest(target=target), tempfile.TemporaryDirectory() as directory:
                root, work = self.setup_paths(directory)
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
                        for name in ("configure", "INPUTS.json", "SOURCE.sha256"):
                            (work / "source" / name).symlink_to(victim / "data")
                with mock.patch.object(probe, "ROOT", root), mock.patch.object(probe, "WORK", work):
                    with self.assertRaisesRegex(RuntimeError, "non-symlink"):
                        probe.publish({"selected_inputs": []}, {"configure": b"source"})
                self.assertEqual((victim / "data").read_bytes(), b"preserved")
                self.assertEqual([entry.name for entry in victim.iterdir()], ["data"])

    def test_verification_does_not_create_missing_work(self):
        with tempfile.TemporaryDirectory() as directory:
            root, work = self.setup_paths(directory)
            with mock.patch.object(probe, "ROOT", root), mock.patch.object(probe, "WORK", work):
                with self.assertRaises(FileNotFoundError):
                    probe.publish({"selected_inputs": []}, {}, verify=True)
            self.assertFalse(work.exists())


if __name__ == "__main__":
    unittest.main()
