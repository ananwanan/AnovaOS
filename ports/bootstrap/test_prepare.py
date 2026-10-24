# SPDX-License-Identifier: Apache-2.0
# Check archive integrity and reject dangerous or ambiguous source-member layouts.
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest import mock


spec = importlib.util.spec_from_file_location("bootstrap_prepare", Path(__file__).with_name("prepare.py"))
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class SourceArchiveTests(unittest.TestCase):
    def make_archive(self, directory, names):
        path = Path(directory) / "input.tar.gz"
        with tarfile.open(path, "w:gz") as archive:
            for name, kind in names:
                member = tarfile.TarInfo(name)
                member.type = kind
                if kind == tarfile.REGTYPE:
                    member.size = 1
                    archive.addfile(member, io.BytesIO(b"x"))
                else:
                    member.linkname = "out-of-tree"
                    archive.addfile(member)
        return path

    def test_reject_modified_archive_before_reading_members(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "input.tar.gz"
            path.write_bytes(b"not the pinned archive")
            with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
                prepare.checked_archive(path)

    def test_reject_traversal_symlink_duplicate_and_foreign_root(self):
        cases = [
            [("bzip2-1.0.8/../escape", tarfile.REGTYPE)],
            [("bzip2-1.0.8/link", tarfile.SYMTYPE)],
            [("bzip2-1.0.8/link", tarfile.LNKTYPE)],
            [("other/file", tarfile.REGTYPE)],
            [("bzip2-1.0.8//bzlib.c", tarfile.REGTYPE)],
            [("bzip2-1.0.8/./bzlib.c", tarfile.REGTYPE)],
            [("bzip2-1.0.8/bzlib.c", tarfile.REGTYPE)] * 2,
        ]
        for members in cases:
            with self.subTest(members=members), tempfile.TemporaryDirectory() as directory:
                path = self.make_archive(directory, members)
                with self.assertRaisesRegex(RuntimeError, "Unexpected bzip2 archive member"):
                    prepare.source_inputs(path)
                self.assertEqual(sorted(p.name for p in Path(directory).iterdir()), ["input.tar.gz"])

    def test_reject_missing_locked_source_and_oversized_member(self):
        with tempfile.TemporaryDirectory() as directory:
            path = self.make_archive(directory, [("bzip2-1.0.8/README", tarfile.REGTYPE)])
            with self.assertRaisesRegex(RuntimeError, "missing a locked source"):
                prepare.source_inputs(path)
            with mock.patch.object(prepare, "FILE_LIMIT", 0):
                with self.assertRaisesRegex(RuntimeError, "exceeds the guest file limit"):
                    prepare.source_inputs(path)


if __name__ == "__main__":
    unittest.main()
