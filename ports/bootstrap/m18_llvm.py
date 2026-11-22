# SPDX-License-Identifier: Apache-2.0
# Select and verify the exact stand-alone LLVM demangler closure from the locked Rust release.
"""Prepare pristine LLVMDemangle inputs without configuring or compiling on the host."""

import hashlib
import json
from pathlib import PurePosixPath
import tarfile
import tempfile

from rust_probe import ROOT, WORK as RUST_WORK, cached_archive, directory, regular

LOCK = ROOT / "ports/bootstrap/rust-inputs.json"
WORK = ROOT / "build/m18-llvm"
LLVM_GITLINK = "e8a2ffcf322f45b8dce82c65ab27a3e2430a6b51"
FILES = {
    "LICENSE.TXT": "8d85c1057d742e597985c7d4e6320b015a9139385cff4cbae06ffc0ebe89afee",
    "include/llvm/Demangle/Demangle.h": "71215603824cd1055c6057727bac6af38b30f01c32f8f12f3f6f2571f3dcb237",
    "include/llvm/Demangle/DemangleConfig.h": "5e6942efe35e16ad2422fbf9a07de1750d8f28f564aa98da23aec36d877a9dad",
    "include/llvm/Demangle/ItaniumDemangle.h": "8c93a30a837ebb2eee11102184217904d2fef9f996a066e028a4e37610a428e1",
    "include/llvm/Demangle/ItaniumNodes.def": "e29522ac3d0a77da44f6299707b2c67af730f5748bd348ff33a3f4ae070f08f4",
    "include/llvm/Demangle/MicrosoftDemangle.h": "d48e9f6723fbf37d7c207e0edd4ed40553a69f084261e11ca97c65f04eb3c163",
    "include/llvm/Demangle/MicrosoftDemangleNodes.h": "1a923b5268d7b464ca479c9d8e60596bd81c7cc38a0d95341b5beac0cd9896bc",
    "include/llvm/Demangle/StringViewExtras.h": "7bbf7bc462ea8e17c574137cc4a62f146f7c6bf4aa25563df001feca5d8e235c",
    "include/llvm/Demangle/Utility.h": "6af6caa2346503ed84d8559fe76ea445799b12f684681dd1fb1b5d437a8d1321",
    "lib/Demangle/CMakeLists.txt": "8e60b3c4a348398f526d6aa754e5c9292c2066c2d7c5548c2534586887dad2a9",
    "lib/Demangle/DLangDemangle.cpp": "a2a5bad5775ec211eb14199e4c935a06f44f5715496e6526513f710de0a7df80",
    "lib/Demangle/Demangle.cpp": "1bbf5ea03cc8ff3ffab4f7fd210bdb3ad793fbfa62efb7169bcf03bbf352be23",
    "lib/Demangle/ItaniumDemangle.cpp": "d370004e7e7b4524c0e929019111e5bc86c777f2cee14e81d5bfe93497dc1c84",
    "lib/Demangle/MicrosoftDemangle.cpp": "21ea972fdfb5516afff853583da389e497acc13fbb4063b0c5aeb501853bcd68",
    "lib/Demangle/MicrosoftDemangleNodes.cpp": "ddeb6c5eafbfdcd6698b29936a59e477a9824242cb6cc89d58bd6d06e4391448",
    "lib/Demangle/RustDemangle.cpp": "b9955aaa8cc9fd1fe0f57e7b81df63521fc3545bc063a421a0dfdc5f298734ec",
}
MAX_SELECTED_FILE = 256 * 1024


def digest(data):
    return hashlib.sha256(data).hexdigest()


def extract(archive_path, prefix, destination):
    found = set()
    with tarfile.open(archive_path, "r|xz") as archive:
        for member in archive:
            name = member.name.removeprefix(prefix)
            if name == member.name or name not in FILES:
                continue
            if name in found or not member.isfile() or not 0 < member.size <= MAX_SELECTED_FILE:
                raise RuntimeError(f"Invalid selected LLVM archive member: {member.name}")
            with archive.extractfile(member) as stream:
                data = stream.read(MAX_SELECTED_FILE + 1)
            if len(data) != member.size or digest(data) != FILES[name]:
                raise RuntimeError(f"Digest mismatch for the selected LLVM member: {member.name}")
            path = destination / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            found.add(name)
    if found != FILES.keys():
        raise RuntimeError(f"Missing selected LLVM members: {sorted(FILES.keys() - found)}")


def check_tree(base, report):
    directory(base)
    expected = set(FILES) | {"INPUTS.json"}
    directories = {parent for name in FILES for parent in PurePosixPath(name).parents}
    for parent in directories:
        directory(base / parent)
    paths = {path.relative_to(base).as_posix() for path in base.rglob("*")}
    if paths != expected | {str(path) for path in directories if path.parts}:
        raise RuntimeError("LLVM preparation contains unexpected or missing paths")
    for name, sha256 in FILES.items():
        if digest(regular(base / name)) != sha256:
            raise RuntimeError(f"LLVM prepared source changed: {name}")
    if regular(base / "INPUTS.json") != report:
        raise RuntimeError("LLVM preparation manifest changed")


def main():
    lock_bytes = regular(LOCK)
    lock = json.loads(lock_bytes)
    if lock["version"] != "1.90.0" or len(lock["sources"]) != 1:
        raise RuntimeError("Unexpected Rust release source lock")
    record = lock["sources"][0]
    prefix = record["root"] + "/src/llvm-project/llvm/"
    directory(ROOT / "build", create=True)
    directory(WORK, create=True)
    directory(RUST_WORK, create=True)
    archive = cached_archive(record, False)
    report = (json.dumps({
        "purpose": "Record the pinned Rust-fork LLVMDemangle source closure and upstream notice.",
        "source_archive": record,
        "lock_sha256": digest(lock_bytes),
        "llvm_gitlink": LLVM_GITLINK,
        "llvm_version": "20.1.8",
        "member_prefix": prefix,
        "files_sha256": FILES,
    }, indent=2) + "\n").encode()
    source = WORK / "source"
    if source.exists() or source.is_symlink():
        check_tree(source, report)
    else:
        with tempfile.TemporaryDirectory(prefix="select-", dir=WORK) as temporary:
            stage = Path(temporary) / "source"
            stage.mkdir()
            extract(archive, prefix, stage)
            (stage / "INPUTS.json").write_bytes(report)
            check_tree(stage, report)
            stage.rename(source)
    print("Verified pinned upstream LLVMDemangle inputs:", source)


if __name__ == "__main__":
    main()
