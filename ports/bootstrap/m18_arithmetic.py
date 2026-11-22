# SPDX-License-Identifier: Apache-2.0
# Build and verify the locked host-prepared GCC arithmetic headers and static musl libraries.
"""Publish the arithmetic development inputs for the independent M18 GCC guest profile."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
LOCK = ROOT / "ports/gcc/inputs.json"
WORK = ROOT / "build/m18-arithmetic"
CACHE = ROOT / "build/toolchain"
sys.path.insert(0, str(LOCK.parent))
from fetch import fetch, verify  # Reuse M11 acquisition and strict checksum verification.

PROJECTS = {
    "gmp-6.2.1": ("gmp", ["COPYING.LESSERv3", "COPYINGv3"]),
    "mpfr-4.1.0": ("mpfr", ["COPYING.LESSER", "COPYING"]),
    "mpc-1.2.1": ("mpc", ["COPYING.LESSER"]),
}
TARGET = "x86_64-linux-musl"
SEED = "x86_64-linux-musl-native"
FLAGS = "-O2 -g0 -fcf-protection=none"
LDFLAGS = "-static -s"
TIMEOUT_SECONDS = 600
FILES = [
    "include/gmp.h", "include/mpfr.h", "include/mpf2mpfr.h", "include/mpc.h",
    "lib/libgmp.a", "lib/libmpfr.a", "lib/libmpc.a",
    *(f"licenses/{project}-{license_name}"
      for project, (_, licenses) in PROJECTS.items() for license_name in licenses),
]


def digest(path):
    if path.is_symlink() or not path.is_file():
        raise RuntimeError(f"Expected a regular file: {path}")
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def directory(path, create=False):
    if create:
        path.mkdir(exist_ok=True)
    if path.is_symlink() or not path.is_dir():
        raise RuntimeError(f"Expected a real directory: {path}")


def input_records(lock):
    wanted = {SEED, *PROJECTS}
    records = [record for record in lock["sources"] if record[3] in wanted]
    if len(records) != len(wanted) or {record[3] for record in records} != wanted:
        raise RuntimeError("Unexpected GCC arithmetic or musl bootstrap source lock")
    return records


def identity(records):
    return {
        "purpose": "Identify host-built Linux-musl arithmetic development inputs supplied to AnovaOS.",
        "host_participation": "Host builds GMP, MPFR and MPC; guest consumes static archives and headers.",
        "target_abi": TARGET,
        "source_archives": [dict(zip(("name", "url", "sha256", "root"), item)) for item in records],
        "recipe_sha256": digest(Path(__file__)),
        "lock_sha256": digest(LOCK),
        "fetch_sha256": digest(LOCK.parent / "fetch.py"),
    }


def inventory(package):
    for name in ("include", "lib", "licenses"):
        directory(package / name)
    expected = {"include", "lib", "licenses", "INPUTS.json"}
    if {path.name for path in package.iterdir()} != expected:
        raise RuntimeError("Unexpected arithmetic package directories or metadata")
    paths = [path for name in sorted(expected - {"INPUTS.json"}) for path in (package / name).iterdir()]
    if {path.relative_to(package).as_posix() for path in paths} != set(FILES):
        raise RuntimeError("Unexpected arithmetic development file inventory")
    return {path.relative_to(package).as_posix(): {"sha256": digest(path), "bytes": path.stat().st_size}
            for path in sorted(paths)}


def check_package(package, records):
    directory(package)
    manifest = package / "INPUTS.json"
    digest(manifest)
    observed = json.loads(manifest.read_text(encoding="utf-8"))
    expected = identity(records)
    expected["files"] = inventory(package)
    for key, value in expected.items():
        if observed.get(key) != value:
            raise RuntimeError(f"Arithmetic package changed ({key}); retain it before rebuilding")


def extract(archive, source, area):
    def filter_member(member, destination):
        # The locked musl seed contains this one guest-absolute interpreter link.
        if (source == SEED and member.issym() and member.name == SEED + "/lib/ld-musl-x86_64.so.1"
                and member.linkname == "/lib/libc.so"):
            member = member.replace(linkname="libc.so")
        return tarfile.data_filter(member, destination)

    with tarfile.open(archive) as tar:
        tar.extractall(area, filter=filter_member)


def run(args, cwd, env, log):
    print("Building arithmetic dependencies:", cwd.name, args[:2], flush=True)
    with log.open("ab") as output:
        with subprocess.Popen(args, cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT,
                              start_new_session=True) as process:
            try:
                status = process.wait(timeout=TIMEOUT_SECONDS)
            except (subprocess.TimeoutExpired, KeyboardInterrupt):
                os.killpg(process.pid, signal.SIGKILL)
                raise
        if status != 0:
            raise RuntimeError(f"Arithmetic dependency build exited {status}: see {log}")


def build(package, records):
    area = Path(tempfile.mkdtemp(prefix="attempt-", dir=WORK))
    print("Arithmetic dependency build logs:", area, flush=True)
    for name, _, _, source in records:
        extract(CACHE / name, source, area)
    compiler = area / SEED / "bin"
    build_name = subprocess.check_output(["gcc", "-dumpmachine"], text=True, timeout=15).strip()
    seed_name = subprocess.check_output([compiler / "gcc", "-dumpmachine"], text=True, timeout=15).strip()
    if not build_name.startswith("x86_64-") or seed_name != TARGET:
        raise RuntimeError(f"Unexpected arithmetic build/host machines: {build_name}, {seed_name}")
    deps = area / "deps"
    env = {"PATH": f"{compiler}:{os.defpath}", "LC_ALL": "C", "CONFIG_SITE": "/dev/null",
           "CC": str(compiler / "gcc"), "AR": str(compiler / "ar"), "NM": str(compiler / "nm"),
           "RANLIB": str(compiler / "ranlib"), "STRIP": str(compiler / "strip"),
           "CFLAGS": f"{FLAGS} -ffile-prefix-map={area}=/m18-arithmetic-build", "LDFLAGS": LDFLAGS}
    common = [f"--build={build_name}", f"--host={TARGET}", f"--prefix={deps}", "--disable-shared"]
    options = {
        "gmp-6.2.1": ["--disable-assembly"],
        "mpfr-4.1.0": [f"--with-gmp={deps}"],
        "mpc-1.2.1": [f"--with-gmp={deps}", f"--with-mpfr={deps}"],
    }
    jobs = min(4, len(os.sched_getaffinity(0)))
    for source, (project, _) in PROJECTS.items():
        cwd = area / f"build-{project}"
        cwd.mkdir()
        log = area / f"{project}.log"
        run([area / source / "configure", *common, *options[source]], cwd, env, log)
        run(["make", f"-j{jobs}"], cwd, env, log)
        run(["make", "install"], cwd, env, log)
    stage = area / "package"
    stage.mkdir()
    for name in ("include", "lib", "licenses"):
        (stage / name).mkdir()
    for path in FILES:
        if not path.startswith("licenses/"):
            shutil.copyfile(deps / path, stage / path)
    for source, (_, licenses) in PROJECTS.items():
        for name in licenses:
            shutil.copyfile(area / source / name, stage / "licenses" / f"{source}-{name}")
    manifest = stage / "INPUTS.json"
    manifest.touch()
    report = identity(records)
    report.update(build_name=build_name, configure_flags=common, project_flags=options,
                  cflags=env["CFLAGS"], ldflags=LDFLAGS, make_jobs=jobs,
                  per_command_timeout_seconds=TIMEOUT_SECONDS, build_logs=str(area.relative_to(ROOT)),
                  files=inventory(stage))
    manifest.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    check_package(stage, records)
    stage.rename(package)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", action="store_true", help="Verify an existing arithmetic package")
    args = parser.parse_args()
    lock = json.loads(LOCK.read_text(encoding="utf-8"))
    records = input_records(lock)
    directory(ROOT / "build", create=not args.verify)
    directory(CACHE, create=not args.verify)
    directory(WORK, create=not args.verify)
    package = WORK / "package"
    for name, url, sha256, _ in records:
        archive = CACHE / name
        if args.verify or package.exists() or package.is_symlink():
            verify(archive, sha256)
        else:
            fetch(archive, [url, *lock.get("mirrors", {}).get(name, [])], sha256,
                  os.environ.get("ANOVA_SOURCE_CACHE"))
    if args.verify or package.exists() or package.is_symlink():
        check_package(package, records)
    else:
        build(package, records)
    print("Verified host-prepared GMP, MPFR and MPC development inputs:", package, flush=True)


if __name__ == "__main__":
    main()
