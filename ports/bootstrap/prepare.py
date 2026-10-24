# SPDX-License-Identifier: Apache-2.0
# Verify official sources and prepare a bounded, source-only offline guest workspace.
"""No compiler runs here; the acceptance tool must be built by the guest GCC."""

import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tarfile
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "ports/bootstrap"
WORK = ROOT / "build/bootstrap"
LOCK = json.loads((PORT / "inputs.json").read_text())
FILE_LIMIT = 1024 * 1024


def regular(path):
    if path.is_symlink() or not path.is_file():
        raise RuntimeError("Expected a regular, non-symlink file: " + str(path))
    return path.read_bytes()


def checked_archive(path):
    data = regular(path)
    for algorithm in ("sha256", "sha512"):
        if hashlib.new(algorithm, data).hexdigest() != LOCK[algorithm]:
            raise RuntimeError("bzip2 source archive checksum mismatch: " + algorithm)


def source_inputs(archive):
    """Read checked regular members without invoking tar extraction or following links."""
    result = {}
    seen = set()
    with tarfile.open(archive, "r:gz") as source:
        for member in source:
            parts = PurePosixPath(member.name).parts
            if (not parts or parts[0] != LOCK["root"] or len(parts) != 2
                    or member.name != "/".join(parts)
                    or parts[1] in (".", "..") or not member.isfile()
                    or member.name in seen):
                raise RuntimeError("Unexpected bzip2 archive member: " + member.name)
            seen.add(member.name)
            if parts[1] not in LOCK["selected"]:
                continue
            if member.size > FILE_LIMIT:
                raise RuntimeError("Selected source exceeds the guest file limit: " + member.name)
            with source.extractfile(member) as stream:
                data = stream.read(FILE_LIMIT + 1)
            if len(data) != member.size:
                raise RuntimeError("Truncated bzip2 archive member: " + member.name)
            result[parts[1]] = data
    if set(result) != set(LOCK["selected"]):
        raise RuntimeError("bzip2 archive is missing a locked source input")
    result["Makefile.anova"] = regular(PORT / "Makefile.anova")
    result["rejected.c"] = regular(ROOT / "tests/guest/m15/rejected.c")
    result["invalid.bz2"] = b"M15 deliberately invalid compressed input\n"
    return result


def manifest(inputs):
    return {
        "purpose": "Record source-only guest inputs, seed tools and machine/ABI roles.",
        "name": LOCK["name"],
        "version": LOCK["version"],
        "archive": LOCK["archive"],
        "archive_sha256": LOCK["sha256"],
        "archive_sha512": LOCK["sha512"],
        "url": LOCK["url"],
        "license": LOCK["license"],
        "patches": [],
        "build": "AnovaOS x86_64, Linux-musl compatibility personality",
        "host": "AnovaOS x86_64, Linux-musl compatibility personality",
        "target": "x86_64-linux-musl static ET_EXEC",
        "seed_tools": "M11 source-built GCC 11.2.0/binutils 2.37/make 4.4.1/musl 1.2.4",
        "sysroot": "/tc",
        "guest_recipe": "make -f Makefile.anova -j1",
        "host_participation": "Download/check/select files, create work disk, boot and inspect; no bzip2 compilation",
        "source_bytes": sum(map(len, inputs.values())),
        "source_files": len(inputs),
        "largest_source_bytes": max(map(len, inputs.values())),
        "files": [
            {"path": name, "size": len(data), "sha256": hashlib.sha256(data).hexdigest()}
            for name, data in sorted(inputs.items())
        ],
    }


def prepare(verify=False):
    WORK.mkdir(parents=True, exist_ok=True)
    if WORK.is_symlink() or WORK.resolve() != ROOT / "build/bootstrap":
        raise RuntimeError("Bootstrap workspace must be a real directory below build/")
    archive = WORK / LOCK["archive"]
    if not archive.exists() and not archive.is_symlink():
        if verify:
            raise RuntimeError("Missing locked archive; run cargo xtask bootstrap-prepare")
        partial = WORK / (LOCK["archive"] + ".part")
        if partial.exists() or partial.is_symlink():
            raise RuntimeError("Remove stale bootstrap partial download before retrying")
        subprocess.run([
            "curl", "--fail", "--location", "--proto", "=https", "--tlsv1.2",
            "--retry", "2", "--max-time", "180", "-o", str(partial), LOCK["url"],
        ], check=True)
        checked_archive(partial)
        partial.rename(archive)
    checked_archive(archive)
    inputs = source_inputs(archive)
    expected = json.dumps(manifest(inputs), indent=2) + "\n"
    inventory = "".join(
        hashlib.sha256(data).hexdigest() + "  " + name + "\n"
        for name, data in sorted(inputs.items())
    )
    inputs["SOURCE.sha256"] = inventory.encode()
    inputs["INPUTS.json"] = expected.encode()
    destination = WORK / "source"
    if destination.exists() or destination.is_symlink():
        if destination.is_symlink() or not destination.is_dir():
            raise RuntimeError("Bootstrap source cache must be a real directory")
        names = {path.name for path in destination.iterdir()}
        if names != set(inputs) or any(
            regular(destination / name) != data for name, data in inputs.items()
        ):
            raise RuntimeError("Bootstrap source cache changed; retain it elsewhere then remove build/bootstrap/source")
    elif verify:
        raise RuntimeError("Missing prepared sources; run cargo xtask bootstrap-prepare")
    else:
        with tempfile.TemporaryDirectory(prefix="prepare-", dir=WORK) as temporary:
            stage = Path(temporary) / "source"
            stage.mkdir()
            for name, data in inputs.items():
                (stage / name).write_bytes(data)
            stage.rename(destination)
    output = WORK / "inputs.json"
    if verify:
        if regular(output).decode() != expected:
            raise RuntimeError("Bootstrap manifest changed")
    else:
        if output.is_symlink():
            raise RuntimeError("Bootstrap manifest must not be a symlink")
        output.write_text(expected)
    print("Verified bzip2 1.0.8 source-only workspace:", destination)
    print("Guest source files:", len(inputs), "; bytes:", sum(map(len, inputs.values())))


if __name__ == "__main__":
    if sys.argv[1:] not in ([], ["--verify"]):
        raise SystemExit("Usage: python3 ports/bootstrap/prepare.py [--verify]")
    prepare(verify=sys.argv[1:] == ["--verify"])
