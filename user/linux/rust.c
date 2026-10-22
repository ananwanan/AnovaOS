// SPDX-License-Identifier: Apache-2.0
// Launch isolated Rust/Cargo tools with static apps and PIC libraries for the offline macro graph.
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv) {
    const char *name = strrchr(argv[0], '/');
    name = name ? name + 1 : argv[0];
    int cargo = strcmp(name, "cargo") == 0;
    int types = 0, macro_type = 0, library_type = 0;
    for (int k = 1; k < argc; ++k) {
        const char *kind = NULL;
        if (!strcmp(argv[k], "--crate-type") && k + 1 < argc) kind = argv[k+1];
        else if (!strncmp(argv[k], "--crate-type=", 13)) kind = argv[k]+13;
        if (kind) {
            ++types;
            macro_type |= !strcmp(kind, "proc-macro");
            library_type |= !strcmp(kind, "lib") || !strcmp(kind, "rlib");
        }
    }
    const int dynamic_macro = types == 1 && macro_type;
    char *args[96];
    int n = 0;
    if (argc > 70) { fputs("rust: too many arguments\n", stderr); return 2; }
    setenv("PATH", "/bin:/tc/bin:/linux", 1);
    setenv("TMPDIR", "/tmp", 1);
    setenv("LC_ALL", "C", 1);
    setenv("LD_LIBRARY_PATH", "/rs/lib:/lib", 1);
    setenv("RUSTC", "/bin/rustc", 1);
    setenv("CARGO_HOME", "/tmp/cargo-home", 1);
    setenv("CARGO_BUILD_JOBS", "1", 1);
    setenv("CARGO_INCREMENTAL", "0", 1);
    setenv("CARGO_TERM_COLOR", "never", 1);
    setenv("CARGO_TERM_PROGRESS_WHEN", "never", 1);
    setenv("CARGO_NET_OFFLINE", "true", 1);
    setenv("RUST_BACKTRACE", "0", 1);
    args[n++] = cargo ? "/rs/bin/cargo" : "/rs/bin/rustc";
    if (!cargo) {
        args[n++] = "--sysroot=/rs";
        args[n++] = "-Clinker=/rs/bin/rust-lld";
        args[n++] = "-Clinker-flavor=ld.lld";
        args[n++] = "-Clink-arg=--threads=1";
        args[n++] = "-Clink-arg=--no-mmap-output-file";
        args[n++] = "-Ccodegen-units=1";
        args[n++] = "-Cdebuginfo=0";
        args[n++] = "-Cstrip=symbols";
        // Cargo also links host dependency rlibs into the macro shared object.
        args[n++] = (dynamic_macro || library_type)
            ? "-Crelocation-model=pic" : "-Crelocation-model=static";
        if (dynamic_macro) {
            args[n++] = "-Ctarget-feature=-crt-static";
            args[n++] = "-Lnative=/rs/lib";
        }
    }
    for (int k = 1; k < argc; ++k) args[n++] = argv[k];
    args[n] = NULL;
    execv(args[0], args);
    fprintf(stderr, "rust: exec %s: %s\n", args[0], strerror(errno));
    return 127;
}
