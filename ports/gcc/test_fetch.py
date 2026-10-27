# SPDX-License-Identifier: Apache-2.0
# Verify transport fallback, digest rejection, cache admission and failed-download cleanup.
import hashlib
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from fetch import fetch


class FetchTests(unittest.TestCase):
    def setUp(self):
        self.work = tempfile.TemporaryDirectory()
        self.addCleanup(self.work.cleanup)
        self.path = Path(self.work.name) / "seed.tgz"
        self.digest = hashlib.sha256(b"locked seed").hexdigest()
        self.urls = ["https://primary.invalid/seed", "https://archive.invalid/seed"]

    def downloaded(self, argv, check):
        Path(argv[argv.index("-o") + 1]).write_bytes(b"locked seed")

    def test_transport_failure_uses_locked_mirror_and_cleans_partial(self):
        def run(argv, check):
            if argv[-1] == self.urls[0]:
                Path(argv[argv.index("-o") + 1]).write_bytes(b"partial")
                raise subprocess.CalledProcessError(28, argv)
            self.downloaded(argv, check)
        with patch("fetch.subprocess.run", side_effect=run) as call:
            fetch(self.path, self.urls, self.digest)
        self.assertEqual(call.call_count, 2)
        self.assertEqual(self.path.read_bytes(), b"locked seed")
        self.assertEqual(list(self.path.parent.iterdir()), [self.path])

    def test_digest_failure_is_fatal_and_does_not_try_a_mirror(self):
        def corrupt(argv, check):
            Path(argv[argv.index("-o") + 1]).write_bytes(b"corrupt")
        with patch("fetch.subprocess.run", side_effect=corrupt) as call:
            with self.assertRaisesRegex(RuntimeError, "checksum"):
                fetch(self.path, self.urls, self.digest)
        self.assertEqual(call.call_count, 1)
        self.assertFalse(self.path.exists())
        self.assertEqual(list(self.path.parent.iterdir()), [])

    def test_existing_corrupt_archive_is_preserved_and_rejected(self):
        self.path.write_bytes(b"corrupt")
        with patch("fetch.subprocess.run") as call:
            with self.assertRaisesRegex(RuntimeError, "checksum"):
                fetch(self.path, self.urls, self.digest)
            call.assert_not_called()
        self.assertEqual(self.path.read_bytes(), b"corrupt")

    def test_verified_cache_is_copied_without_network(self):
        cache = self.path.parent / "cache"
        cache.mkdir()
        source = cache / self.path.name
        source.write_bytes(b"locked seed")
        with patch("fetch.subprocess.run") as call:
            fetch(self.path, self.urls, self.digest, cache)
            call.assert_not_called()
        self.assertEqual(self.path.read_bytes(), source.read_bytes())

    def test_symlink_cache_and_destination_are_rejected(self):
        source = self.path.parent / "target"
        source.write_bytes(b"locked seed")
        self.path.symlink_to(source)
        with self.assertRaisesRegex(RuntimeError, "regular"):
            fetch(self.path, self.urls, self.digest)

    def test_all_transports_fail_without_publishing(self):
        with patch("fetch.subprocess.run", side_effect=subprocess.CalledProcessError(28, "curl")):
            with self.assertRaisesRegex(RuntimeError, "transports"):
                fetch(self.path, self.urls, self.digest)
        self.assertEqual(list(self.path.parent.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
