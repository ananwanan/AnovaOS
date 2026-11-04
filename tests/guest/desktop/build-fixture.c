// SPDX-License-Identifier: Apache-2.0
// Exercise controlled-job output, actual UD2 and detached-family cleanup in a separate test ISO.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/wait.h>

int main(int argc, char **argv) {
    if (argc != 5 || strcmp(argv[1], "-fdiagnostics-color=never") ||
        strcmp(argv[2], "/persist/build.c") || strcmp(argv[3], "-o") ||
        strcmp(argv[4], "/persist/build-app.next")) return 99;
    FILE *source = fopen(argv[2], "r");
    int mode = source ? fgetc(source) : 'x';
    if (source) fclose(source);
    if (mode == 'c') { __asm__ volatile("ud2"); }
    if (mode == 'l') {
        for (int n = 0; n < 5000; ++n) {
            puts("LONG-OUTPUT-012345678901234567890123456789");
            if (n % 10 == 0) { fflush(stdout); usleep(20000); }
        }
        fflush(stdout);
        return 29;
    }
    if (mode == 'd' || mode == 'w') {
        int fds[2];
        if (pipe(fds)) return 98;
        pid_t pid = fork();
        if (pid == 0) {
            close(fds[0]);
            if (setsid() < 0 || write(fds[1], "R", 1) != 1) _exit(97);
            if (mode == 'w') {
                char output[1024];
                memset(output, 'W', sizeof(output));
                for (;;) {
                    if (write(1, output, sizeof(output)) < 0) _exit(95);
                }
            }
            for (;;) usleep(10000);
        }
        close(fds[1]);
        char ready;
        if (pid < 0 || read(fds[0], &ready, 1) != 1) return 96;
        // A flood must start only in the descendant: parent output could block
        // behind it and prevent the leader from reaching the exit being tested.
        if (mode != 'w') {
            puts("DETACHED-READY");
            fflush(stdout);
        }
        return mode == 'w' ? 0 : 31;
    }
    puts("JOB-COPY-OK!");
    puts("BUILD-SUCCESS exit=0");
    fputs("MERGED-STDERR\n", stderr);
    return 23;
}
