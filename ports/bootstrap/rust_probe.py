# SPDX-License-Identifier: Apache-2.0
# Authenticate full Rust/LLVM/Cargo sources and publish bounded pristine feasibility inputs.
"""Audit member metadata without extracting or following links or running build tools."""

import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys
import tarfile
import tempfile
import tomllib


ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / "build/bootstrap-rust"
FILE_LIMIT = 1024 * 1024
LIMITS = {
    "writable_file_bytes": FILE_LIMIT,
    "work_disk_bytes": 16 * 1024 * 1024,
    "work_disk_inodes": 512,
    "vfs_nodes_including_seed_environment": 2048,
    "disk_block_bytes": 4096,
}
SELECTED = {
    "x": "rust-x",
    "x.py": "rust-x.py",
    "src/bootstrap/bootstrap.py": "rust-bootstrap.py",
    "src/stage0": "rust-stage0",
    "Cargo.toml": "rust-Cargo.toml",
    "Cargo.lock": "rust-Cargo.lock",
    "git-commit-hash": "rust-git-commit-hash",
    "LICENSE-APACHE": "rust-LICENSE-APACHE",
    "LICENSE-MIT": "rust-LICENSE-MIT",
    "src/tools/cargo/Cargo.toml": "cargo-Cargo.toml",
    "src/tools/cargo/Cargo.lock": "cargo-Cargo.lock",
    "src/tools/cargo/LICENSE-APACHE": "cargo-LICENSE-APACHE",
    "src/tools/cargo/LICENSE-MIT": "cargo-LICENSE-MIT",
    "src/tools/cargo/LICENSE-THIRD-PARTY": "cargo-LICENSE-THIRD-PARTY",
    "src/llvm-project/llvm/CMakeLists.txt": "llvm-CMakeLists.txt",
    "src/llvm-project/cmake/Modules/LLVMVersion.cmake": "llvm-LLVMVersion.cmake",
    "src/llvm-project/llvm/LICENSE.TXT": "llvm-LICENSE.TXT",
}
COMPONENTS = {
    "llvm-project": "src/llvm-project/",
    "cargo": "src/tools/cargo/",
    "vendor": "vendor/",
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


def digest(path):
    if not stat.S_ISREG(path.lstat().st_mode):
        raise RuntimeError("Expected a regular source archive: " + str(path))
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def authenticated_archive(path, record):
    if digest(path) != record["sha256"]:
        raise RuntimeError("Rust source archive checksum mismatch: " + path.name)


def cached_archive(record, verify):
    """Publish downloads only after authentication; verification never downloads or writes."""
    cache = WORK / "cache"
    directory(cache, create=not verify)
    path = cache / record["name"]
    if path.exists() or path.is_symlink() or verify:
        authenticated_archive(path, record)
        return path
    with tempfile.TemporaryDirectory(prefix="download-", dir=cache) as temporary:
        candidate = Path(temporary) / "source.part"
        subprocess.run([
            "curl", "--fail", "--location", "--proto", "=https", "--tlsv1.2",
            "--retry", "2", "--max-time", "300", record["url"], "-o", str(candidate),
        ], check=True)
        authenticated_archive(candidate, record)
        # A concurrent preparation cannot overwrite an existing cache entry.
        try:
            path.hardlink_to(candidate)
        except FileExistsError:
            authenticated_archive(path, record)
    return path


def link_target(member, root):
    """Validate lexical containment; links are metadata only and never followed."""
    target = member.linkname
    if not target or target.startswith("/") or "\\" in target:
        raise RuntimeError("Unsafe Rust source link: " + member.name)
    parts = list(PurePosixPath(member.name).parts[:-1]) if member.issym() else []
    for part in target.split("/"):
        if part == "..":
            if len(parts) <= 1:
                raise RuntimeError("Escaping Rust source link: " + member.name)
            parts.pop()
        elif part not in ("", "."):
            parts.append(part)
    if not parts or parts[0] != root:
        raise RuntimeError("Escaping Rust source link: " + member.name)
    return "/".join(parts)


def empty_counts():
    return {"regular_files": 0, "directories": 0, "links": 0,
            "logical_file_bytes": 0, "regular_file_data_block_bytes_lower_bound": 0,
            "over_file_limit_count": 0}


def audit_archive(path, record, selected):
    authenticated_archive(path, record)
    root = record["root"]
    seen = {}
    namespace = set()
    parents = set()
    files = {}
    selections = []
    oversized = []
    links = []
    counts = empty_counts()
    components = {name: empty_counts() for name in (*COMPONENTS, "rust-and-other")}
    inventory = hashlib.sha256()
    largest = {"path": "", "size": 0}
    with tarfile.open(path, "r|*") as archive:
        for member in archive:
            canonical = member.name.removesuffix("/") if member.isdir() else member.name
            parts = PurePosixPath(canonical).parts
            if (not parts or parts[0] != root or canonical != "/".join(parts)
                    or any(part in (".", "..") for part in parts) or "\\" in canonical
                    or canonical in seen or member.size < 0
                    or not (member.isfile() or member.isdir() or member.issym() or member.islnk())):
                raise RuntimeError("Unsafe or ambiguous Rust archive member: " + member.name)
            if canonical in parents and not member.isdir():
                raise RuntimeError("Rust archive file conflicts with a directory: " + canonical)
            for end in range(1, len(parts)):
                parent = "/".join(parts[:end])
                if parent in seen and seen[parent] != "directory":
                    raise RuntimeError("Rust archive parent is not a directory: " + canonical)
                parents.add(parent)
            namespace.update("/".join(parts[:end]) for end in range(1, len(parts) + 1))
            relative = "/".join(parts[1:])
            component = next((name for name, prefix in COMPONENTS.items()
                              if relative.startswith(prefix)), "rust-and-other")
            summary = components[component]
            kind = "directory" if member.isdir() else "file" if member.isfile() else "link"
            seen[canonical] = kind
            inventory.update((json.dumps([canonical, kind, member.size, member.linkname],
                                         separators=(",", ":")) + "\n").encode())
            if kind == "directory":
                counts["directories"] += 1
                summary["directories"] += 1
                continue
            if kind == "link":
                destination = link_target(member, root)
                links.append({"path": canonical, "target": member.linkname,
                              "resolved_lexical_target": destination,
                              "kind": "symlink" if member.issym() else "hardlink"})
                counts["links"] += 1
                summary["links"] += 1
                if relative in selected:
                    raise RuntimeError("Selected Rust probe input is not a regular file: " + canonical)
                continue
            rounded = (member.size + 4095) // 4096 * 4096
            for item in (counts, summary):
                item["regular_files"] += 1
                item["logical_file_bytes"] += member.size
                item["regular_file_data_block_bytes_lower_bound"] += rounded
                item["over_file_limit_count"] += int(member.size > FILE_LIMIT)
            item = {"path": canonical, "size": member.size}
            if member.size > largest["size"]:
                largest = item
            if member.size > FILE_LIMIT:
                oversized.append(item)
            if relative not in selected:
                continue
            destination = selected[relative]
            if (not destination or PurePosixPath(destination).name != destination
                    or destination in (".", "..") or destination in files):
                raise RuntimeError("Invalid or duplicate flat probe destination: " + destination)
            if member.size > FILE_LIMIT:
                raise RuntimeError("Selected Rust probe exceeds the guest file limit: " + canonical)
            with archive.extractfile(member) as stream:
                data = stream.read(FILE_LIMIT + 1)
            if len(data) != member.size:
                raise RuntimeError("Truncated Rust probe member: " + canonical)
            files[destination] = data
            selections.append({"path": destination, "size": len(data),
                               "sha256": hashlib.sha256(data).hexdigest(),
                               "archive": record["name"], "member": canonical})
    if set(files) != set(selected.values()):
        raise RuntimeError("Rust archive is missing a selected probe input")
    return {
        **record, "archive_bytes": path.stat().st_size, **counts,
        "namespace_nodes_lower_bound": len(namespace), "largest_file": largest,
        "over_file_limit": sorted(oversized, key=lambda item: item["path"]),
        "link_metadata": links,
        "member_metadata_inventory_sha256": inventory.hexdigest(),
        "components": [{"name": name, **summary} for name, summary in components.items()],
    }, files, selections


def requirements(files, target):
    stage0 = dict(line.split("=", 1) for line in files["rust-stage0"].decode().splitlines()
                  if line and not line.startswith("#") and "=" in line)
    date, version = stage0["compiler_date"], stage0["compiler_version"]
    stage0_inputs = []
    for tool in ("rustc", "cargo", "rust-std"):
        member = f"dist/{date}/{tool}-{version}-{target}.tar.xz"
        stage0_inputs.append({"name": member.rsplit("/", 1)[1],
                              "url": stage0["dist_server"] + "/" + member,
                              "sha256": stage0[member]})
    cargo = tomllib.loads(files["cargo-Cargo.toml"].decode())
    cargo_lock = tomllib.loads(files["cargo-Cargo.lock"].decode())
    llvm = files["llvm-CMakeLists.txt"].decode()
    llvm_version = files["llvm-LLVMVersion.cmake"].decode()
    version_parts = [re.search(r"set\(LLVM_VERSION_" + part + r" (\d+)\)",
                               llvm_version).group(1) for part in ("MAJOR", "MINOR", "PATCH")]
    curl = next(item for item in cargo_lock["package"] if item["name"] == "curl")
    return {
        "rust_release_commit": files["rust-git-commit-hash"].decode().strip(),
        "rustc": {"prescribed_stage0_version": version, "prescribed_stage0_date": date,
                  "prescribed_stage0_inputs": stage0_inputs,
                  "existing_m12_seed_version": "1.90.0",
                  "existing_seed_is_prescribed_stage0": False},
        "llvm": {"version": ".".join(version_parts),
                 "cmake_minimum": re.search(r"cmake_minimum_required\(VERSION ([\d.]+)\)", llvm).group(1),
                 "cxx_standard": int(re.search(r"set\(LLVM_REQUIRED_CXX_STANDARD (\d+)\)", llvm).group(1))},
        "cargo": {"crate_version": cargo["package"]["version"],
                  "package_rust_version": cargo["package"]["rust-version"],
                  "workspace_rust_version": cargo["workspace"]["package"]["rust-version"],
                  "locked_packages": len(cargo_lock["package"]),
                  "locked_registry_packages": sum(item.get("source", "").startswith("registry+")
                                                   for item in cargo_lock["package"]),
                  "curl_probe_dependency": {key: curl[key] for key in ("name", "version", "source", "checksum")}},
        "scope": "Requirements read from unchanged release files; availability and sufficiency need guest probes.",
    }


def publish(report, inputs, verify=False):
    directory(ROOT / "build", create=not verify)
    directory(WORK, create=not verify)
    output = WORK / "source-audit.json"
    expected = (json.dumps(report, indent=2) + "\n").encode()
    inputs = dict(inputs)
    inputs["INPUTS.json"] = (json.dumps({
        "purpose": "Record exact unchanged Rust bootstrap, Cargo, LLVM requirements and notices.",
        "scope": "Source-only subset; not a complete, configured or buildable source workspace.",
        "files": report["selected_inputs"],
    }, indent=2) + "\n").encode()
    inputs["SOURCE.sha256"] = "".join(
        hashlib.sha256(data).hexdigest() + "  " + name + "\n"
        for name, data in sorted(inputs.items())
    ).encode()
    source = WORK / "source"
    if output.exists() or output.is_symlink():
        if regular(output) != expected:
            raise RuntimeError("Rust source audit changed; retain it elsewhere then remove source-audit.json")
    elif verify:
        raise RuntimeError("Missing Rust source audit; run rust_probe.py first")
    if source.exists() or source.is_symlink():
        directory(source)
        if ({item.name for item in source.iterdir()} != set(inputs)
                or any(regular(source / name) != data for name, data in inputs.items())):
            raise RuntimeError("Rust probe source cache changed; retain it elsewhere then remove source/")
    elif verify:
        raise RuntimeError("Missing Rust probe sources; run rust_probe.py first")
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
    directory(ROOT / "build", create=not verify)
    directory(WORK, create=not verify)
    lock_bytes = regular(ROOT / "ports/bootstrap/rust-inputs.json")
    lock = json.loads(lock_bytes)
    if (lock["version"] != "1.90.0" or lock["target"] != "x86_64-unknown-linux-musl"
            or len(lock["sources"]) != 1):
        raise RuntimeError("Unexpected Rust source dependency lock")
    record = lock["sources"][0]
    summary, files, selections = audit_archive(cached_archive(record, verify), record, SELECTED)
    totals = {key: summary[key] for key in (*empty_counts(), "namespace_nodes_lower_bound")}
    report = {
        "purpose": "Audit authenticated Rust, LLVM, Cargo and vendor source metadata against guest capacity.",
        "schema_version": 1, "profile": "m15-rust-1.90.0-feasibility",
        "lock_sha256": hashlib.sha256(lock_bytes).hexdigest(),
        "scope": "Full release archive includes tests/docs/other targets; not a minimum build dependency closure.",
        "host_participation": "Download authenticated source, audit metadata and select pristine files; no compilation.",
        "unverified": "LLVM/rustc/Cargo reconstruction, configured builds, prescribed stage0 execution, exact build peaks.",
        "limits": LIMITS, "archives": [summary], "totals": totals,
        "requirements": requirements(files, lock["target"]),
        "full_source_tree_fits": (
            totals["over_file_limit_count"] == 0
            and totals["regular_file_data_block_bytes_lower_bound"] <= LIMITS["work_disk_bytes"]
            and totals["namespace_nodes_lower_bound"] <= LIMITS["work_disk_inodes"]
            and totals["namespace_nodes_lower_bound"] <= LIMITS["vfs_nodes_including_seed_environment"]),
        "selected_inputs": sorted(selections, key=lambda item: item["path"]),
    }
    publish(report, files, verify)
    print("Verified source-only Rust/LLVM/Cargo probe workspace:", WORK / "source", flush=True)
    print(totals["regular_files"], "files,", totals["logical_file_bytes"], "logical bytes,",
          totals["over_file_limit_count"], "files above", FILE_LIMIT, "bytes", flush=True)
    return report


if __name__ == "__main__":
    if sys.argv[1:] not in ([], ["--verify"]):
        raise SystemExit("Usage: python3 ports/bootstrap/rust_probe.py [--verify]")
    prepare(verify=sys.argv[1:] == ["--verify"])
