// SPDX-License-Identifier: Apache-2.0
// Produce independent stdout/stderr, long output, SIGWINCH and interrupt/cleanup workloads.
#define _POSIX_C_SOURCE 200809L
#include <signal.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static volatile sig_atomic_t resized;
static void winch(int signal) { (void)signal; resized = 1; }
static void size(void) {
    struct winsize window;
    if (ioctl(0, TIOCGWINSZ, &window) != 0) _exit(90);
    printf("SIZE rows=%u columns=%u\n", window.ws_row, window.ws_col);
    fflush(stdout);
}

int main(int argc, char **argv) {
    const char *mode = argc > 1 ? argv[1] : "streams";
    if (!strcmp(mode, "streams")) {
        fputs("\033[2J\033[H\033[32mstdout-17\n", stdout); fflush(stdout);
        fputs("\033[31mstderr-17\n\033[0m", stderr); return 23;
    }
    if (!strcmp(mode, "size")) { size(); return 0; }
    if (!strcmp(mode, "jobs")) {
        int status;
        pid_t child = fork(); if (child < 0) return 95;
        if (child == 0) {
            struct timespec delay = {0, 50000000}; nanosleep(&delay, NULL);
            raise(SIGSTOP); _exit(0);
        }
        if (waitpid(child, &status, WUNTRACED) != child || !WIFSTOPPED(status) || WSTOPSIG(status) != SIGSTOP) return 96;
        if (waitpid(child, &status, WUNTRACED | WNOHANG) != 0) return 97;
        if (kill(child, SIGCONT) != 0 || waitpid(child, &status, 0) != child || status != 0) return 98;
        puts("JOBS-OK"); return 0;
    }
    if (!strcmp(mode, "family")) {
        int ready[2]; if (pipe(ready) != 0) return 91;
        pid_t child = fork(); if (child < 0) return 91;
        if (child == 0) {
            close(ready[0]);
            if (setsid() < 0 || write(ready[1], "R", 1) != 1) _exit(92);
            close(ready[1]);
            for (;;) { struct timespec delay = {1, 0}; nanosleep(&delay, NULL); }
        }
        close(ready[1]); char byte;
        if (read(ready[0], &byte, 1) != 1 || byte != 'R') return 92;
        close(ready[0]);
        printf("FAMILY child=%ld\n", (long)child); fflush(stdout); return 0;
    }
    struct sigaction action = {0};
    action.sa_handler = winch; sigemptyset(&action.sa_mask);
    if (sigaction(SIGWINCH, &action, NULL) != 0) return 93;
    size();
    if (!strcmp(mode, "spin")) {
        puts("SPIN-READY"); fflush(stdout);
        for (;;) { struct timespec delay = {0, 20000000}; nanosleep(&delay, NULL); if (resized) { resized = 0; size(); } }
    }
    if (!strcmp(mode, "flood")) {
        int seen = 0;
        for (unsigned n = 0; n < 600; ++n) {
            printf("line-%04u\n", n); fflush(stdout);
            if (resized) { resized = 0; seen = 1; size(); }
            if (n % 10 == 0) { struct timespec delay = {0, 100000000}; nanosleep(&delay, NULL); }
        }
        puts("END-17"); if (seen) puts("WINCH-SEEN"); size(); return seen ? 0 : 94;
    }
    return 2;
}
