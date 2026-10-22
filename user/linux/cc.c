// SPDX-License-Identifier: Apache-2.0
// Configure the offline guest C/C++ profile and exec the real GCC/G++/make tools.
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv) {
    const char *name = strrchr(argv[0], '/');
    name = name ? name + 1 : argv[0];
    char *args[96];
    int n = 0;
    const int make = strcmp(name, "make") == 0;
    const char *tool = make ? "/tc/bin/make" :
        strcmp(name, "g++") == 0 ? "/tc/bin/g++" : "/tc/bin/gcc";
    if (argc > 70) { fputs("toolchain: too many arguments\n", stderr); return 2; }
    setenv("PATH", "/bin:/tc/bin:/linux", 1);
    setenv("TMPDIR", "/tmp", 1);
    setenv("LC_ALL", "C", 1);
    args[n++] = (char *)tool;
    if (make) args[n++] = "SHELL=/linux/sh";
    else {
        args[n++] = "-static";
        args[n++] = "-no-pie";
        args[n++] = "-fno-pie";
        args[n++] = "-fcf-protection=none";
        args[n++] = "-Wl,--build-id=none";
        args[n++] = "--sysroot=/tc";
        args[n++] = "--param=ggc-min-expand=100";
        args[n++] = "--param=ggc-min-heapsize=32768";
    }
    for (int k = 1; k < argc; ++k) args[n++] = argv[k];
    args[n] = NULL;
    execv(tool, args);
    fprintf(stderr, "toolchain: exec %s: %s\n", tool, strerror(errno));
    return 127;
}
