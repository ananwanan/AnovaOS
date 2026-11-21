# SPDX-License-Identifier: Apache-2.0
# Select pristine GCC configure and x86_64 C/C++ build inputs from the locked release.
"""Stage the entry or stage-one profile without configuring or compiling on the host."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import tarfile
import tempfile

from gcc_probe import ROOT, checked_archive, directory, regular


ENTRY = {
    "COPYING3",
    "Makefile.in",
    "config.guess",
    "config.sub",
    "configure",
    "gcc/BASE-VER",
    "gcc/DEV-PHASE",
    "gcc/configure",
    "gcc/cp/config-lang.in",
    "install-sh",
    "mkinstalldirs",
    "move-if-change",
}

# The all-gcc graph uses these host libraries and fixincludes. In-tree libgcc
# contributes the coverage utility inputs, while the target runtimes come from /tc.
# Disabled D and LTO still supply inputs to the shared generators and option table.
STAGE1_REQUIRED = ENTRY | {
    "ABOUT-NLS", "COPYING", "COPYING.LIB", "COPYING3.LIB", "COPYING.RUNTIME",
    "LAST_UPDATED", "README", "Makefile.def", "Makefile.tpl", "ar-lib", "compile",
    "config-ml.in", "config.rpath", "depcomp", "libtool-ldflags", "libtool.m4",
    "ltgcc.m4", "ltmain.sh", "ltoptions.m4", "ltsugar.m4", "ltversion.m4",
    "missing", "mkdep", "multilib.am", "symlink-tree",
    "test-driver", "ylwrap",
    "c++tools/resolver.cc", "c++tools/resolver.h", "contrib/texi2pod.pl",
    "gcc/Makefile.in", "gcc/config.gcc", "gcc/config.host", "gcc/tree.c",
    "gcc/ada/gcc-interface/ada-tree.def", "gcc/ada/gcc-interface/lang.opt",
    "gcc/brig/lang.opt", "gcc/d/Make-lang.in", "gcc/d/config-lang.in", "gcc/d/d-builtins.cc",
    "gcc/d/d-lang.cc", "gcc/d/d-target.def", "gcc/d/d-tree.def", "gcc/d/d-tree.h",
    "gcc/d/lang.opt", "gcc/d/typeinfo.cc",
    "gcc/fortran/lang.opt",
    "gcc/go/lang.opt", "gcc/lto/Make-lang.in", "gcc/lto/config-lang.in", "gcc/lto/lang.opt",
    "gcc/lto/lto-common.c", "gcc/lto/lto-common.h", "gcc/lto/lto-dump.c", "gcc/lto/lto-lang.c",
    "gcc/lto/lto-tree.h", "gcc/lto/lto.c", "gcc/lto/lto.h", "gcc/objc/objc-tree.def",
    "gcc/common.md", "gcc/gengtype-lex.c", "gcc/gengtype-parse.c", "gcc/match.pd",
    "gcc/config/i386/i386.md", "gcc/cp/Make-lang.in", "gcc/cp/cfns.h",
    "include/gdb/gdb-index.h",
    "fixincludes/Makefile.in", "libbacktrace/Makefile.in", "libcody/Makefile.in",
    "libcpp/Makefile.in", "libdecnumber/Makefile.in", "libiberty/Makefile.in",
    "libiberty/testsuite/Makefile.in",
    "zlib/Makefile.in", "intl/Makefile.in",
    "libgcc/libgcov-driver.c", "libgcc/libgcov-driver-system.c", "libgcc/libgcov-merge.c",
    "libgcc/libgcov-util.c", "libgcc/libgcov.h",
}
STAGE1_FLAT_DIRS = {
    "config", "fixincludes", "gcc", "include", "intl", "libbacktrace", "libcody",
    "libcpp", "libdecnumber", "libiberty", "zlib",
}
STAGE1_TREES = (
    "gcc/analyzer/", "gcc/c/", "gcc/c-family/", "gcc/common/", "gcc/config/i386/",
    "gcc/cp/", "gcc/doc/", "gcc/ginclude/", "gcc/rtl-ssa/", "gcc/testsuite/selftests/",
    "libcpp/include/",
    "libdecnumber/bid/", "libdecnumber/dpd/", "libiberty/config/",
)
FILE_LIMIT = 4 * 1024 * 1024
BLOCK_BYTES = 4096
SOURCE_PATH = re.compile(r"[a-zA-Z0-9._+\-/]+")


def stage1_file(relative):
    if relative in STAGE1_REQUIRED:
        return True
    parts = PurePosixPath(relative).parts
    if not parts or parts[-1].startswith("ChangeLog"):
        return False
    return (len(parts) == 2 and parts[0] in STAGE1_FLAT_DIRS
            or len(parts) == 3 and parts[:2] == ("gcc", "config")
            or relative.startswith(STAGE1_TREES))


def prepare(output, profile="entry"):
    build = ROOT / "build"
    directory(build)
    directory(output.parent)
    run = output.parent
    if profile == "stage1" and run.name == "disk-seed":
        run = run.parent
        directory(run)
    if run.parent != build or output.exists() or output.is_symlink():
        raise RuntimeError("Expected fresh M18 source output inside a run directory below build/")
    lock = regular(ROOT / "ports/gcc/inputs.json")
    record, = [row for row in json.loads(lock)["sources"] if row[3] == "gcc-11.2.0"]
    name, url, digest, prefix = record
    cache = build / "toolchain"
    directory(cache)
    archive = cache / name
    checked_archive(archive, digest)

    required = ENTRY if profile == "entry" else STAGE1_REQUIRED
    selected = {}
    report = {
        "purpose": "Record the exact upstream GCC configure and make-entry inputs.",
        "scope": "Selected entry files only; GCC compiler translation units are excluded.",
        "archive": {"name": name, "url": url, "sha256": digest},
        "lock_sha256": hashlib.sha256(lock).hexdigest(),
    }
    with tempfile.TemporaryDirectory(prefix="gcc-source-", dir=output.parent) as temporary:
        stage = Path(temporary) / "tree"
        stage.mkdir()
        with tarfile.open(archive, "r|xz") as source:
            for member in source:
                relative = member.name.removeprefix(prefix + "/")
                if relative == member.name:
                    continue
                picked = relative in ENTRY if profile == "entry" else stage1_file(relative)
                if not picked or (member.isdir() and relative not in required):
                    continue
                path = PurePosixPath(relative)
                if (not SOURCE_PATH.fullmatch(relative) or relative != path.as_posix()
                        or path.is_absolute() or ".." in path.parts
                        or relative in selected or not member.isfile()
                        or not 0 <= member.size <= FILE_LIMIT):
                    raise RuntimeError("Invalid selected GCC member: " + member.name)
                with source.extractfile(member) as stream:
                    data = stream.read(FILE_LIMIT + 1)
                if len(data) != member.size:
                    raise RuntimeError("Short GCC source member: " + member.name)
                destination = stage / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(data)
                entry = {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
                if profile == "stage1":
                    mode = 0o755 if member.mode & 0o111 else 0o644
                    destination.chmod(mode)
                    # Preserve the release's generator-input order (for example, lex .l/.c).
                    os.utime(destination, (member.mtime, member.mtime))
                    entry |= {"mode": f"{mode:04o}", "mtime": member.mtime}
                selected[relative] = entry
        if required - selected.keys():
            raise RuntimeError("Missing selected GCC members: " + str(sorted(required - selected)))

        report["files"] = dict(sorted(selected.items()))
        if profile == "stage1":
            parents = {parent for name in selected for parent in PurePosixPath(name).parents}
            report["purpose"] = "Record the exact staged GCC 11.2.0 C/C++ all-gcc inputs."
            report["scope"] = "x86_64 Linux-musl stage-one source and release-generated inputs."
            report["profile"] = profile
            report["source_data"] = {
                "bytes": sum(file["bytes"] for file in selected.values()),
                "blocks_4096_excluding_metadata_and_manifest": sum(
                    (file["bytes"] + BLOCK_BYTES - 1) // BLOCK_BYTES for file in selected.values()
                ),
                "files": len(selected),
                "directories_including_source_root": len(parents),
                "inodes_including_manifest": len(selected) + len(parents) + 1,
            }
        (stage / "INPUTS.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        stage.rename(output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--profile", choices=("entry", "stage1"), default="entry")
    options = parser.parse_args()
    prepare(options.output.absolute(), options.profile)
