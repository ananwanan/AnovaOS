#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Build standalone native SDK projects with isolated user flags and an explicit ABI trailer.
import os
from pathlib import Path
import subprocess
import sys


def run(arguments, env=None):
    print("Running " + " ".join(str(value) for value in arguments), flush=True)
    return subprocess.run(arguments, check=True, env=env)


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in ("c", "cpp", "rust"):
        raise SystemExit("Usage: python3 SDK/build.py <c|cpp|rust> PROJECT")
    language = sys.argv[1]
    sdk = Path(__file__).resolve().parent
    project = Path(sys.argv[2]).resolve(strict=True)
    temporary = project / "app.elf.new"
    output = project / "app.elf"
    for path in (temporary, output, project / ".build"):
        if path.is_symlink():
            raise SystemExit("Refusing symlink output: " + str(path))
    (project / ".build").mkdir(exist_ok=True)
    environment = os.environ.copy()
    # Host and kernel target settings must not enter application code generation.
    for key in list(environment):
        if key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET") or (
            key.startswith("CARGO_TARGET_") and key.endswith("_RUSTFLAGS")
        ):
            del environment[key]
    if language == "rust":
        if not (project / "Cargo.lock").is_file():
            raise SystemExit("Missing Cargo.lock; run cargo generate-lockfile --offline first")
        target = "x86_64-unknown-linux-gnu"
        sysroot = subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip()
        linker = Path(sysroot) / "lib/rustlib" / target / "bin/rust-lld"
        flags = [
            "-C", "linker=" + str(linker), "-C", "linker-flavor=ld.lld",
            "-C", "link-arg=-T" + str(sdk / "native-v1.ld"),
            "-C", "link-arg=--undefined=_start", "-C", "link-arg=--build-id=none",
            "-C", "relocation-model=static", "-C", "code-model=small",
            "-C", "no-redzone=no", "-C", "target-cpu=x86-64",
            "-C", "target-feature=-avx,-avx2", "-C", "force-frame-pointers=yes",
            "-C", "panic=abort",
        ]
        environment["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
        build = project / ".build/rust"
        run(["cargo", "build", "--offline", "--locked", "--release", "--target", target,
             "--manifest-path", project / "Cargo.toml", "--target-dir", build], environment)
        temporary.write_bytes((build / target / "release/anova-app").read_bytes())
    else:
        flags = [
            "g++" if language == "cpp" else "gcc",
            "-std=c++17" if language == "cpp" else "-std=c11",
            "-Wall", "-Wextra", "-Werror", "-O1", "-g", "-fno-omit-frame-pointer",
            "-fno-optimize-sibling-calls",
            "-ffreestanding", "-fno-stack-protector", "-fno-pie", "-fno-builtin",
            "-static", "-nostdlib", "-no-pie", "-m64", "-march=x86-64",
            "-mno-avx", "-mno-avx2", "-Wl,--build-id=none", "-Wl,-z,noexecstack",
            "-Wl,-T," + str(sdk / "native-v1.ld"), "-I" + str(sdk / "include"),
        ]
        if language == "cpp":
            flags += ["-fno-exceptions", "-fno-rtti", "-fno-use-cxa-atexit",
                      "-fno-threadsafe-statics"]
        run(flags + [sdk / "crt.S", project / ("main.cpp" if language == "cpp" else "main.c"),
                     "-o", temporary], environment)
    # This explicit build contract is metadata, not publisher authentication.
    trailer = (b"ANOVAABI" + b"anova.native.v1".ljust(32, b"\0") +
               b"x86_64".ljust(16, b"\0") + (1).to_bytes(4, "little") + b"\0" * 4)
    with temporary.open("ab") as binary:
        binary.write(trailer)
    temporary.replace(output)
    print("Built native SDK v1 ELF: " + str(output))


if __name__ == "__main__":
    main()
