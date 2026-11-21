#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Independently inspect the bounded guest image writer and exercise publication failures.
"""Test the same static musl writer used by the guest, with host-only fixture files."""

import hashlib
import os
from pathlib import Path
import resource
import signal
import struct
import subprocess
import tempfile
import unittest


APPS = (
    "hello-rust", "hello-c", "probe", "process-test", "init", "shell",
    "fs-test", "edit", "compositor", "gui-client", "gui-test", "gui-run",
)
EFI_BYTES = 2_949_120
SECTOR = 2048


class ImageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tool_dir = tempfile.TemporaryDirectory(prefix="anova-m18-image-tool-")
        cls.addClassCleanup(cls.tool_dir.cleanup)
        cls.tool = Path(cls.tool_dir.name) / "m18-image"
        subprocess.run(
            ["musl-gcc", "-O2", "-static", "-no-pie", "-Wall", "-Wextra", "-Werror",
             "-Wl,--build-id=none", Path(__file__).with_name("image-build.c"),
             "-o", cls.tool],
            check=True,
            timeout=30,
        )

    def setUp(self):
        work = tempfile.TemporaryDirectory(prefix="anova-m18-image-test-")
        self.addCleanup(work.cleanup)
        self.path = Path(work.name)
        self.inputs = self.path / "inputs"
        self.inputs.mkdir()
        self.kernel = self.path / "kernel.elf"
        self.output = self.path / "anova.iso"
        self.candidate = self.path / "anova.iso.candidate"
        self.previous = b"previous published bytes\n"

        self.kernel.write_bytes(b"\x7fELF\x02\x01\x01" + bytes(range(256)) * 11)
        self.layout = {
            "initramfs.anovafs": "boot/initramfs.anovafs",
            "LIMINE-LICENSE": "boot/limine/LICENSE",
            "limine.conf": "boot/limine/limine.conf",
            "BOOTX64.EFI": "EFI/BOOT/BOOTX64.EFI",
            "limine-uefi-cd.bin": "boot/limine/limine-uefi-cd.bin",
            "manifest.toml": "boot/apps/manifest.toml",
        }
        self.layout.update({f"{name}.elf": f"boot/apps/{name}.elf" for name in APPS})
        for index, name in enumerate(self.layout):
            (self.inputs / name).write_bytes(name.encode() * (index + 1))
        (self.inputs / "initramfs.anovafs").write_bytes(bytes(range(256)) * 29 + b"end")
        with (self.inputs / "limine-uefi-cd.bin").open("ab") as efi:
            efi.truncate(EFI_BYTES)

    def run_writer(self, output=None, **options):
        return subprocess.run(
            [self.tool, self.kernel, self.inputs, output or self.output],
            capture_output=True,
            text=True,
            timeout=30,
            **options,
        )

    def reject(self, expected, **options):
        self.output.write_bytes(self.previous)
        result = self.run_writer(**options)
        self.assertEqual(result.returncode, 1, result)
        self.assertIn(expected, result.stderr)
        self.assertNotIn("[M18:IMAGE:PUBLISHED]", result.stdout)
        self.assertEqual(self.output.read_bytes(), self.previous)
        self.assertFalse(self.candidate.exists())

    def test_report_extract_and_repeat_exact_image(self):
        for output in (self.output, self.path / "repeat.iso"):
            result = self.run_writer(output)
            self.assertEqual(result.returncode, 0, result)
            self.assertIn("[M18:IMAGE:PUBLISHED]", result.stdout)
            self.assertFalse(Path(f"{output}.candidate").exists())
        first = self.output.read_bytes()
        second = (self.path / "repeat.iso").read_bytes()
        self.assertEqual(hashlib.sha256(first).digest(), hashlib.sha256(second).digest())
        self.assertEqual(first, second)
        self.assertEqual(len(first) % SECTOR, 0)

        extracted = self.path / "extracted"
        result = subprocess.run(
            ["xorriso", "-osirrox", "on", "-indev", f"stdio:{self.output}",
             "-report_el_torito", "plain", "-extract", "/", extracted],
            check=True, capture_output=True, text=True, timeout=30,
        )
        self.assertRegex(result.stdout, r"El Torito boot img\s*:\s*1\s+UEFI\s+y\s+none")
        expected_paths = set(self.layout.values()) | {"boot/kernel.elf"}
        self.assertEqual(
            {str(path.relative_to(extracted)) for path in extracted.rglob("*") if path.is_file()},
            expected_paths,
        )
        for source, destination in self.layout.items():
            self.assertEqual((self.inputs / source).read_bytes(), (extracted / destination).read_bytes())
        self.assertEqual(self.kernel.read_bytes(), (extracted / "boot/kernel.elf").read_bytes())

        catalog_offset = struct.unpack_from("<I", first, 17 * SECTOR + 71)[0] * SECTOR
        catalog = first[catalog_offset:catalog_offset + 64]
        self.assertEqual(sum(struct.unpack_from("<16H", catalog)) % 65536, 0)
        self.assertEqual(catalog[30:34], b"\x55\xaa\x88\x00")
        load_count, boot_lba = struct.unpack_from("<HI", catalog, 38)
        self.assertEqual(load_count * 512, EFI_BYTES)
        self.assertEqual(
            first[boot_lba * SECTOR:boot_lba * SECTOR + EFI_BYTES],
            (self.inputs / "limine-uefi-cd.bin").read_bytes(),
        )

    def test_invalid_and_oversized_inputs(self):
        config = self.inputs / "limine.conf"
        config.unlink()
        self.reject("open input")
        config.symlink_to(self.kernel)
        self.reject("open input")
        config.unlink()
        os.mkfifo(config)
        self.reject("reject input type or size")
        config.unlink()
        config.write_bytes(b"limited\n")
        with (self.inputs / "initramfs.anovafs").open("ab") as archive:
            archive.truncate(16 * 1024 * 1024 + 1)
        self.reject("reject input type or size")

    def test_candidate_collision_and_recovery(self):
        self.output.write_bytes(self.previous)
        self.candidate.write_bytes(b"old unpublished bytes\n")
        result = self.run_writer()
        self.assertEqual(result.returncode, 1, result)
        self.assertIn("create candidate", result.stderr)
        self.assertEqual(self.candidate.read_bytes(), b"old unpublished bytes\n")
        self.assertEqual(self.output.read_bytes(), self.previous)
        cleanup = subprocess.run(
            [self.tool, "cleanup", self.output], check=True, capture_output=True, text=True, timeout=30,
        )
        self.assertIn("[M18:IMAGE:CLEANUP]", cleanup.stdout)
        self.assertFalse(self.candidate.exists())
        self.assertEqual(self.output.read_bytes(), self.previous)
        result = self.run_writer()
        self.assertEqual(result.returncode, 0, result)

    def test_publish_fails_on_existing_directory(self):
        self.output.mkdir()
        user_file = self.output / "persisted.txt"
        user_file.write_bytes(self.previous)
        result = self.run_writer()
        self.assertEqual(result.returncode, 1, result)
        self.assertIn("[M18:IMAGE:WRITE]", result.stdout)
        self.assertIn("publish candidate", result.stderr)
        self.assertEqual(user_file.read_bytes(), self.previous)
        self.assertFalse(self.candidate.exists())

    def test_file_limit_failure_preserves_previous_image(self):
        def restrict_file_size():
            signal.signal(signal.SIGXFSZ, signal.SIG_IGN)
            resource.setrlimit(resource.RLIMIT_FSIZE, (8 * SECTOR, 8 * SECTOR))

        self.reject("stream image", preexec_fn=restrict_file_size, restore_signals=False)

    def test_output_alias_cannot_replace_kernel(self):
        previous_kernel = self.kernel.read_bytes()
        for output in (self.kernel, self.output):
            if output == self.output:
                output.hardlink_to(self.kernel)
            result = self.run_writer(output)
            self.assertEqual(result.returncode, 1, result)
            self.assertIn("reject output alias of input", result.stderr)
            self.assertEqual(self.kernel.read_bytes(), previous_kernel)


if __name__ == "__main__":
    unittest.main()
