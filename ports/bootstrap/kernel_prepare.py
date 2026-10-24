# SPDX-License-Identifier: Apache-2.0
# Publish exact kernel sources, locked offline crates and inherited bare-metal seed libraries.
"""Preparation never compiles a kernel or substitutes host-built target artifacts."""

import fcntl
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import tomllib


ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / "build/bootstrap-kernel"
GUEST_SOURCE = "/ks/source"


def regular(path):
    if not stat.S_ISREG(path.lstat().st_mode):
        raise RuntimeError("Expected a regular, non-symlink file: " + str(path))
    return path.read_bytes()


def directory(path, create=False):
    if not path.exists() and not path.is_symlink() and create:
        path.mkdir()
    if not stat.S_ISDIR(path.lstat().st_mode):
        raise RuntimeError("Expected a real, non-symlink directory: " + str(path))


def digest(data):
    return hashlib.sha256(data).hexdigest()


def inventory(base):
    """Check every directory as well as files; never traverse redirected output trees."""
    directory(base)
    files = {}
    pending = [base]
    while pending:
        parent = pending.pop()
        for path in sorted(parent.iterdir()):
            mode = path.lstat().st_mode
            if stat.S_ISDIR(mode):
                pending.append(path)
            elif stat.S_ISREG(mode):
                files[path.relative_to(base).as_posix()] = digest(regular(path))
            else:
                raise RuntimeError("Unsafe prepared tree entry: " + str(path))
    return dict(sorted(files.items()))


def authenticated(path, expected):
    if digest(regular(path)) != expected:
        raise RuntimeError("Kernel input checksum mismatch: " + path.name)


def archive_files(path, expected, archive_root):
    """Authenticate before parsing and reject duplicate, escaping or redirected names."""
    authenticated(path, expected)
    files = {}
    directories = set()
    seen = set()
    with tarfile.open(path) as archive:
        for member in archive:
            name = member.name.removesuffix("/") if member.isdir() else member.name
            parts = PurePosixPath(name).parts
            if (not parts or parts[0] != archive_root or name != "/".join(parts)
                    or "\\" in name or any(part in (".", "..") for part in parts)
                    or name in seen or member.size < 0
                    or not (member.isfile() or member.isdir())
                    or member.isfile() and len(parts) == 1):
                raise RuntimeError("Unsafe kernel input archive member: " + member.name)
            seen.add(name)
            parents = ["/".join(parts[:end]) for end in range(1, len(parts))]
            if any(parent in files for parent in parents):
                raise RuntimeError("Conflicting kernel input archive member: " + name)
            if member.isdir():
                if name in files:
                    raise RuntimeError("Conflicting kernel input archive directory: " + name)
                directories.add(name)
                continue
            if name in directories or any(existing.startswith(name + "/") for existing in seen):
                raise RuntimeError("Conflicting kernel input archive file: " + name)
            with archive.extractfile(member) as stream:
                data = stream.read()
            if len(data) != member.size:
                raise RuntimeError("Short kernel input archive member: " + name)
            files[name] = data
            directories.update(parents)
    prefix = archive_root + "/"
    return {name[len(prefix):]: data for name, data in files.items()}


def write_tree(base, files):
    base.mkdir()
    for name, data in sorted(files.items()):
        path = base / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)


def source_inputs():
    names = subprocess.check_output([
        "git", "ls-files", "-z", "--", "kernel", "crates", "linker",
        "Cargo.toml", "Cargo.lock", ".cargo/config.toml", "LICENSE", "NOTICE",
    ], cwd=ROOT).decode().split("\0")
    # Include newly added first-party source files before their delivery commit as well.
    names += subprocess.check_output([
        "git", "ls-files", "--others", "--exclude-standard", "-z", "--",
        "kernel", "crates", "linker",
    ], cwd=ROOT).decode().split("\0")
    result = {}
    for name in sorted(set(names) - {""}):
        path = ROOT / name
        for parent in path.parents:
            if parent == ROOT:
                break
            directory(parent)
        result[name] = regular(path)
    return result


def isolated_manifest(original):
    text = original.decode()
    manifest = tomllib.loads(text)
    members = manifest["workspace"]["members"]
    if "xtask" not in members or 'default-members = ["xtask"]' not in text:
        raise RuntimeError("Kernel workspace isolation requires review of changed root metadata")
    isolated = [member for member in members if member != "xtask"]
    old = "members = " + json.dumps(members)
    if old not in text:
        raise RuntimeError("Kernel workspace member formatting changed; review isolation")
    return text.replace(old, "members = " + json.dumps(isolated), 1).replace(
        'default-members = ["xtask"]', 'default-members = ["kernel"]', 1).encode()


def validate_lock(original, prepared):
    expected = {(item["name"], item["version"], item.get("source")): item.get("checksum")
                for item in tomllib.loads(original.decode())["package"]}
    packages = tomllib.loads(prepared.decode())["package"]
    for package in packages:
        key = (package["name"], package["version"], package.get("source"))
        if key not in expected or expected[key] != package.get("checksum"):
            raise RuntimeError("Isolated kernel lock drifted from the repository: " + package["name"])
    return [package for package in packages if "source" in package]


def registry_archive(package):
    name = f"{package['name']}-{package['version']}.crate"
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
    matches = sorted((cargo_home / "registry/cache").glob("*/" + name))
    if not matches:
        raise RuntimeError("Missing locked crate cache; run cargo fetch --locked: " + name)
    authenticated(matches[0], package["checksum"])
    return matches[0]


def seed_archive(record, verify=False):
    path = WORK / record["name"]
    if path.exists() or path.is_symlink() or verify:
        authenticated(path, record["sha256"])
        return path
    with tempfile.TemporaryDirectory(prefix="download-", dir=WORK) as temporary:
        candidate = Path(temporary) / "input.part"
        subprocess.run([
            "curl", "--fail", "--location", "--proto", "=https", "--tlsv1.2",
            "--retry", "2", "--max-time", "300", record["url"], "-o", str(candidate),
        ], check=True)
        authenticated(candidate, record["sha256"])
        try:
            path.hardlink_to(candidate)
        except FileExistsError:
            authenticated(path, record["sha256"])
    return path


def published():
    manifest = json.loads(regular(WORK / "inputs.json"))
    for name in ["source", "seeds"]:
        if inventory(WORK / name) != manifest[name + "_files_sha256"]:
            raise RuntimeError("Prepared kernel " + name + " inventory changed")
        if directories(WORK / name) != manifest[name + "_directories"]:
            raise RuntimeError("Prepared kernel " + name + " namespace changed")
    return manifest


def directories(base):
    # inventory() already rejects links before this namespace enumeration is used.
    return sorted(path.relative_to(base).as_posix() for path in base.rglob("*") if path.is_dir())


def check_guest_paths(base, prefix):
    entries = list(base.rglob("*"))
    for path in entries:
        name = prefix + "/" + path.relative_to(base).as_posix()
        if len(name.encode()) > 128 or any(len(part.encode()) > 63 for part in name.split("/")):
            raise RuntimeError("Prepared kernel path exceeds initramfs bounds: " + name)
    return len(entries)


def main(argv):
    if argv not in ([], ["--verify"]):
        raise RuntimeError("Usage: kernel_prepare.py [--verify]")
    verify = bool(argv)
    directory(ROOT / "build")
    directory(WORK, create=not verify)
    record = json.loads(regular(ROOT / "ports/bootstrap/kernel-inputs.json"))
    recipe_files = {name: digest(regular(ROOT / name)) for name in [
        "ports/bootstrap/kernel_prepare.py", "ports/bootstrap/kernel-inputs.json",
    ]}
    inputs = source_inputs()
    input_hashes = {name: digest(data) for name, data in inputs.items()}
    if verify:
        manifest = published()
        seed_archive(record, verify=True)
        if (manifest["repository_files_sha256"] != input_hashes
                or manifest["recipe_files_sha256"] != recipe_files):
            raise RuntimeError("Kernel source or preparation recipe changed; prepare a fresh snapshot")
        for package in manifest["registry_packages"]:
            registry_archive(package)
        print("Verified exact kernel source, locked registry inputs and inherited bare-metal seeds")
        return
    lock_path = WORK / "prepare.lock"
    if lock_path.exists() or lock_path.is_symlink():
        regular(lock_path)
    with lock_path.open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if (WORK / "inputs.json").exists() or (WORK / "inputs.json").is_symlink():
            published()
        elif any((WORK / name).exists() or (WORK / name).is_symlink() for name in ["source", "seeds"]):
            raise RuntimeError("Uninventoried kernel preparation outputs already exist")
        archive = seed_archive(record)
        with tempfile.TemporaryDirectory(prefix="prepare-", dir=WORK) as temporary:
            temporary = Path(temporary)
            source = temporary / "source"
            source_files = dict(inputs)
            source_files["Cargo.toml"] = isolated_manifest(inputs["Cargo.toml"])
            # Cargo controls only its generated isolated lock, never the repository lock.
            write_tree(source, source_files)
            subprocess.run([
                "cargo", "generate-lockfile", "--offline", "--manifest-path", str(source / "Cargo.toml"),
            ], cwd=source, check=True)
            packages = validate_lock(inputs["Cargo.lock"], regular(source / "Cargo.lock"))
            vendor = source / "vendor"
            vendor.mkdir()
            for package in packages:
                if package["source"] != "registry+https://github.com/rust-lang/crates.io-index":
                    raise RuntimeError("Unsupported locked kernel dependency source")
                name = f"{package['name']}-{package['version']}"
                files = archive_files(registry_archive(package), package["checksum"], name)
                files[".cargo-checksum.json"] = (json.dumps({
                    "package": package["checksum"],
                    "files": {key: digest(data) for key, data in sorted(files.items())},
                }, sort_keys=True) + "\n").encode()
                write_tree(vendor / name, files)
            config = source / ".cargo/config.toml"
            config.write_bytes(regular(config) + b'\n[source.crates-io]\nreplace-with = "m15-vendor"\n'
                               b'\n[source.m15-vendor]\ndirectory = "vendor"\n\n[net]\noffline = true\n')
            dist = archive_files(archive, record["sha256"], record["root"])
            seed_files = {}
            library_prefix = f"rust-std-{record['target']}/lib/rustlib/{record['target']}/lib/"
            for name, data in dist.items():
                if name.startswith(library_prefix):
                    seed_files["rs/lib/rustlib/" + record["target"] + "/lib/" + name[len(library_prefix):]] = data
                elif "/" not in name and (name.startswith("LICENSE") or name.startswith("COPYRIGHT")):
                    seed_files["rs/licenses/kernel-target/" + name] = data
            for name in ["core", "alloc", "compiler_builtins"]:
                if not any(Path(path).name.startswith("lib" + name + "-") for path in seed_files):
                    raise RuntimeError("Missing inherited bare-metal seed library: " + name)
            seeds = temporary / "seeds"
            write_tree(seeds, seed_files)
            nodes = check_guest_paths(source, GUEST_SOURCE) + check_guest_paths(seeds, "")
            if nodes > 1500:
                raise RuntimeError("Kernel source and seed namespace exceeds the preparation budget")
            manifest = {
                "profile": "m15-kernel-reconstruction-1",
                "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "source_worktree": subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True),
                "repository_files_sha256": input_hashes,
                "recipe_files_sha256": recipe_files,
                "source_files_sha256": inventory(source),
                "seeds_files_sha256": inventory(seeds),
                "source_directories": directories(source),
                "seeds_directories": directories(seeds),
                "registry_packages": packages,
                "inherited_bare_metal_seed": record,
                "source_and_seed_namespace_entries": nodes,
                "generated_metadata": ["Cargo.toml: root workspace excludes xtask and defaults to kernel",
                                       "Cargo.lock: Cargo-generated subset checked against repository versions/checksums",
                                       ".cargo/config.toml: original target flags plus offline vendor replacement",
                                       "vendor/*/.cargo-checksum.json: authenticated complete archive file hashes"],
                "host_participation": "Snapshot exact first-party sources, isolate build metadata, authenticate/extract locked dependencies and inherited target libraries; no target compilation",
                "guest_source_root": GUEST_SOURCE,
                "guest_seed_root": "/rs/lib/rustlib/" + record["target"],
            }
            # Existing generated outputs were checked before replacement. Publish the inventory last.
            for name in ["source", "seeds"]:
                destination = WORK / name
                if destination.exists():
                    destination.rename(temporary / ("previous-" + name))
                (temporary / name).rename(destination)
            candidate = temporary / "inputs.json"
            candidate.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
            candidate.replace(WORK / "inputs.json")
    print("Prepared kernel source and inherited target seeds: " + str(WORK / "inputs.json"))


if __name__ == "__main__":
    main(sys.argv[1:])
