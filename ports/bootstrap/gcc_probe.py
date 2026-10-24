# SPDX-License-Identifier: Apache-2.0
# Audit locked compiler sources and publish only bounded GCC configure-startup inputs.
"""No extraction or compiler runs here; full-tree bounds are not minimum build needs."""

import hashlib
import json
from pathlib import Path, PurePosixPath
import stat
import sys
import tarfile
import tempfile


ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / "build/bootstrap-gcc"
CACHE = ROOT / "build/toolchain"
# Preserve the accepted M15.3/M15.4 assessment capacity profile for comparable audits.
# M15.5 raises current persistent files to 4 MiB; these full trees still exceed disk/node bounds.
FILE_LIMIT = 1024 * 1024
LIMITS = {
    "writable_file_bytes": FILE_LIMIT,
    "work_disk_bytes": 16 * 1024 * 1024,
    "work_disk_inodes": 512,
    "vfs_nodes_including_seed_environment": 2048,
    "disk_block_bytes": 4096,
}
SELECTED = {
    "gcc-11.2.0": {
        "configure": "configure",
        "move-if-change": "move-if-change",
        "config.guess": "config.guess",
        "config.sub": "config.sub",
        "install-sh": "install-sh",
        "COPYING3": "gcc-COPYING3",
        "INSTALL/prerequisites.html": "gcc-prerequisites.html",
        "INSTALL/gfdl.html": "gcc-gfdl.html",
    },
    "gmp-6.2.1": {
        "COPYING.LESSERv3": "gmp-COPYING.LESSERv3",
        "COPYINGv3": "gmp-COPYINGv3",
    },
    "mpfr-4.1.0": {
        "COPYING.LESSER": "mpfr-COPYING.LESSER",
        "COPYING": "mpfr-COPYING",
    },
    "mpc-1.2.1": {"COPYING.LESSER": "mpc-COPYING.LESSER"},
}


def regular(path):
    if not stat.S_ISREG(path.lstat().st_mode):
        raise RuntimeError("Expected a regular, non-symlink file: " + str(path))
    return path.read_bytes()


def directory(path, create=False):
    if not path.exists() and not path.is_symlink() and create:
        path.mkdir()
    if not stat.S_ISDIR(path.lstat().st_mode):
        raise RuntimeError("Expected a real, non-symlink directory: " + str(path))


def checked_archive(path, expected):
    if not stat.S_ISREG(path.lstat().st_mode):
        raise RuntimeError("Expected a regular source archive: " + str(path))
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    if digest.hexdigest() != expected:
        raise RuntimeError("Compiler source archive checksum mismatch: " + path.name)


def audit_archive(path, record, selected):
    """Scan authenticated member metadata without extracting or following any links."""
    name, url, expected, root = record
    checked_archive(path, expected)
    seen = set()
    namespace = set()
    files = {}
    selected_records = []
    oversized = []
    counts = {"regular_files": 0, "directories": 0, "links": 0}
    logical = 0
    rounded = 0
    largest = {"path": "", "size": 0}
    inventory = hashlib.sha256()
    with tarfile.open(path, "r|*") as archive:
        for member in archive:
            # tar directories may end in '/', but no interior normalization is accepted.
            canonical = member.name.removesuffix("/") if member.isdir() else member.name
            parts = PurePosixPath(canonical).parts
            if (not parts or parts[0] != root or canonical != "/".join(parts)
                    or any(part in (".", "..") for part in parts)
                    or canonical in seen or member.size < 0
                    or not (member.isfile() or member.isdir())):
                raise RuntimeError("Unsafe or ambiguous compiler archive member: " + member.name)
            seen.add(canonical)
            for end in range(1, len(parts) + 1):
                namespace.add("/".join(parts[:end]))
            kind = "file" if member.isfile() else "directory"
            inventory.update(json.dumps([canonical, kind, member.size], separators=(",", ":")).encode())
            inventory.update(b"\n")
            if member.isdir():
                counts["directories"] += 1
                continue
            counts["regular_files"] += 1
            logical += member.size
            rounded += (member.size + 4095) // 4096 * 4096
            item = {"path": canonical, "size": member.size}
            if member.size > largest["size"]:
                largest = item
            if member.size > FILE_LIMIT:
                oversized.append(item)
            relative = "/".join(parts[1:])
            if relative not in selected:
                continue
            if member.size > FILE_LIMIT:
                raise RuntimeError("Selected compiler probe exceeds the guest file limit: " + canonical)
            with archive.extractfile(member) as stream:
                data = stream.read(FILE_LIMIT + 1)
            if len(data) != member.size:
                raise RuntimeError("Truncated compiler probe member: " + canonical)
            destination = selected[relative]
            if (not destination or PurePosixPath(destination).name != destination
                    or destination in (".", "..") or destination in files):
                raise RuntimeError("Invalid or duplicate flat probe destination: " + destination)
            files[destination] = data
            selected_records.append({
                "path": destination, "size": len(data),
                "sha256": hashlib.sha256(data).hexdigest(),
                "archive": name, "member": canonical,
            })
    if set(files) != set(selected.values()):
        raise RuntimeError("Compiler archive is missing a selected probe input: " + name)
    return {
        "name": name, "root": root, "url": url, "sha256": expected,
        "archive_bytes": path.stat().st_size, **counts,
        "logical_file_bytes": logical,
        "regular_file_data_block_bytes_lower_bound": rounded,
        "namespace_nodes_lower_bound": len(namespace),
        "largest_file": largest,
        "over_file_limit": sorted(oversized, key=lambda item: item["path"]),
        "member_metadata_inventory_sha256": inventory.hexdigest(),
    }, files, selected_records


def publish(report, inputs, verify=False):
    """Refuse changed caches and symlinks; verification performs no filesystem writes."""
    directory(ROOT / "build", create=not verify)
    directory(WORK, create=not verify)
    output = WORK / "source-audit.json"
    expected = (json.dumps(report, indent=2) + "\n").encode()
    inputs = dict(inputs)
    inputs["INPUTS.json"] = (json.dumps({
        "purpose": "Record exact unchanged upstream configure-startup inputs and notices.",
        "scope": "Source-only subset; neither a complete GCC source tree nor a configured build.",
        "files": report["selected_inputs"],
    }, indent=2) + "\n").encode()
    inputs["SOURCE.sha256"] = "".join(
        hashlib.sha256(data).hexdigest() + "  " + name + "\n"
        for name, data in sorted(inputs.items())
    ).encode()
    source = WORK / "source"
    # Check every existing destination before publishing any new output.
    if output.exists() or output.is_symlink():
        if regular(output) != expected:
            raise RuntimeError("Compiler source audit changed; retain it elsewhere then remove source-audit.json")
    elif verify:
        raise RuntimeError("Missing compiler source audit; run gcc_probe.py first")
    if source.exists() or source.is_symlink():
        directory(source)
        if ({item.name for item in source.iterdir()} != set(inputs)
                or any(regular(source / name) != data for name, data in inputs.items())):
            raise RuntimeError("Compiler probe source cache changed; retain it elsewhere then remove source/")
    elif verify:
        raise RuntimeError("Missing compiler probe sources; run gcc_probe.py first")
    else:
        with tempfile.TemporaryDirectory(prefix="prepare-", dir=WORK) as temporary:
            stage = Path(temporary) / "source"
            stage.mkdir()
            for name, data in inputs.items():
                (stage / name).write_bytes(data)
            stage.rename(source)
    if not verify and not output.exists():
        with output.open("xb") as stream:
            stream.write(expected)


def prepare(verify=False):
    directory(ROOT / "build")
    directory(CACHE)
    lock = json.loads(regular(ROOT / "ports/gcc/inputs.json"))
    records = [record for record in lock["sources"] if record[0] != "bootstrap.tgz"]
    if {record[3] for record in records} != {
        "gcc-11.2.0", "binutils-2.37", "make-4.4.1", "musl-1.2.4",
        "gmp-6.2.1", "mpfr-4.1.0", "mpc-1.2.1",
    } or len(records) != 7:
        raise RuntimeError("Unexpected compiler source dependency lock")
    archives = []
    files = {}
    selections = []
    for record in records:
        summary, selected, entries = audit_archive(CACHE / record[0], record, SELECTED.get(record[3], {}))
        archives.append(summary)
        if set(files) & set(selected):
            raise RuntimeError("Duplicate compiler probe filename across archives")
        files.update(selected)
        selections.extend(entries)
        print("Verified", record[0], ":", summary["regular_files"], "files,",
              summary["logical_file_bytes"], "logical bytes,",
              len(summary["over_file_limit"]), "files above", FILE_LIMIT, "bytes", flush=True)
    totals = {
        key: sum(archive[key] for archive in archives)
        for key in ("regular_files", "directories", "links", "logical_file_bytes",
                    "regular_file_data_block_bytes_lower_bound", "namespace_nodes_lower_bound")
    }
    totals["over_file_limit_count"] = sum(len(archive["over_file_limit"]) for archive in archives)
    report = {
        "purpose": "Audit authenticated unpruned compiler source trees against bounded guest capacity.",
        "schema_version": 1,
        "profile": "m15-gcc-11.2.0-feasibility",
        "lock_sha256": hashlib.sha256(regular(ROOT / "ports/gcc/inputs.json")).hexdigest(),
        "scope": "Full release source archives including tests/docs; not a minimum build dependency closure.",
        "host_participation": "Read cached locked archives and select startup scripts/notices; no extraction or compilation.",
        "unverified": "Guest GCC reconstruction, configured source build, stage2/stage3, exact peak build memory/time/disk.",
        "limits": LIMITS,
        "archives": archives,
        "totals": totals,
        "full_source_tree_fits": (
            totals["over_file_limit_count"] == 0
            and totals["logical_file_bytes"] <= LIMITS["work_disk_bytes"]
            and totals["namespace_nodes_lower_bound"] <= LIMITS["work_disk_inodes"]
            and totals["namespace_nodes_lower_bound"] <= LIMITS["vfs_nodes_including_seed_environment"]
        ),
        "selected_inputs": sorted(selections, key=lambda entry: entry["path"]),
    }
    publish(report, files, verify)
    print("Verified source-only GCC startup probe workspace:", WORK / "source", flush=True)
    return report


if __name__ == "__main__":
    if sys.argv[1:] not in ([], ["--verify"]):
        raise SystemExit("Usage: python3 ports/bootstrap/gcc_probe.py [--verify]")
    prepare(verify=sys.argv[1:] == ["--verify"])
