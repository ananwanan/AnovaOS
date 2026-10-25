// SPDX-License-Identifier: Apache-2.0
// Execute the real guest GCC/G++ and Rust tools to build native GUI clients with a Rust runtime.
// This Linux-musl launcher is build tooling; its output follows the mounted native ABI policy.
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static void run(char *const args[]) {
    pid_t child = fork();
    if (child < 0) { perror("gui-build: fork"); exit(1); }
    if (child == 0) {
        execv(args[0], args);
        perror("gui-build: exec");
        _exit(127);
    }
    int status;
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) || WEXITSTATUS(status)) {
        fputs("gui-build: tool failed; previous executable retained\n", stderr);
        exit(1);
    }
}

int main(int argc, char **argv) {
    if (argc != 2) { fputs("Usage: gui-build c|cpp|rust|note|terminal|contract|fault\n", stderr); return 2; }
    const char *language = argv[1];
    int cpp = strcmp(language, "cpp") == 0;
    int rust = strcmp(language, "rust") == 0 || !strcmp(language, "note") ||
        !strcmp(language, "terminal");
    if (!cpp && !rust && strcmp(language, "c") && strcmp(language, "note") &&
        strcmp(language, "terminal") && strcmp(language, "contract") && strcmp(language, "fault")) return 2;
    setenv("PATH", "/bin:/tc/bin:/rs/bin:/linux", 1);
    setenv("TMPDIR", "/tmp", 1);
    setenv("LC_ALL", "C", 1);
    setenv("RUST_MIN_STACK", "8388608", 1);
    char *runtime[] = {"/bin/rustc", "--edition=2024", "--crate-name=anova_gui_runtime",
        "--crate-type=rlib", "-Cpanic=abort", "-Copt-level=1", "-Cdebuginfo=0",
        "-Crelocation-model=static",
        "/gui/crates/gui-runtime/src/lib.rs", "-o", "/persist/libanova_gui_runtime.rlib", NULL};
    run(runtime);
    char source[64], output[64], pending[80];
    snprintf(source, sizeof(source), "/persist/%s", !strcmp(language, "note") ? "note.rs" :
        !strcmp(language, "terminal") ? "terminal.rs" : rust ? "main.rs" : cpp ? "main.cpp" :
        !strcmp(language, "contract") ? "contract.c" : !strcmp(language, "fault") ? "fault.c" : "main.c");
    snprintf(output, sizeof(output), "/persist/gui-%s", language);
    snprintf(pending, sizeof(pending), "%s.new", output);
    char *cc[] = {cpp ? "/bin/g++" : "/bin/gcc", cpp ? "-std=c++17" : "-std=c11",
        "-Wall", "-Wextra", "-Werror", "-O1", "-ffreestanding", "-fno-stack-protector",
        "-fno-builtin", "-fno-pie", "-fno-exceptions", cpp ? "-fno-rtti" : "-fno-common", "-fno-unwind-tables",
        "-fno-asynchronous-unwind-tables", "-I/gui/include", "-c", source,
        "-o", "/persist/client.o", NULL};
    char *rs[] = {"/bin/rustc", "--edition=2024", "--crate-type=lib", "--emit=obj",
        "-Cpanic=abort", "-Copt-level=1", "-Crelocation-model=static", "--extern",
        "anova_gui_runtime=/persist/libanova_gui_runtime.rlib", source,
        "-o", "/persist/client.o", NULL};
    run(rust ? rs : cc);
    char *link[] = {"/bin/rustc", "--edition=2024", "-Cpanic=abort", "-Copt-level=1",
        "-Crelocation-model=static", "-Clink-self-contained=no",
        "-Clink-arg=-T/gui/native-v1.ld", "-Clink-arg=--undefined=_start",
        "-Clink-arg=--gc-sections", "-Clink-arg=--build-id=none", "--extern",
        "anova_gui_runtime=/persist/libanova_gui_runtime.rlib", "/gui/link.rs",
        "-Clink-arg=/persist/client.o", "-o", pending, NULL};
    run(link);
    if (rename(pending, output)) { perror("gui-build: publish"); return 1; }
    printf("[GUI-SDK:BUILD] language=%s native=true guest=true\n", language);
    return 0;
}
