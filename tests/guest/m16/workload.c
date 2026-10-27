// SPDX-License-Identifier: Apache-2.0
// Run a real bzip2 build and storage writer while accepting UART foreground commands and interruption.
// Every response follows actual exec, file I/O or process-group signaling; the host measures UART wall latency.
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#define CHECK(c) do { if (!(c)) { fprintf(stderr, "[M16:WORKLOAD:FAIL] line=%d errno=%d\n", __LINE__, errno); return 1; } } while (0)
static int reap(pid_t pid, int *status) {
    pid_t result;
    do result = waitpid(pid, status, 0); while (result < 0 && errno == EINTR);
    return result == pid ? 0 : -1;
}
static int compiler_log(void) {
    char bytes[4097];
    int fd = open("/persist/build.log", O_RDONLY);
    if (fd < 0) return 0;
    ssize_t n = read(fd, bytes, sizeof(bytes) - 1);
    int result = close(fd);
    if (n <= 0 || result) return 0;
    bytes[n] = 0;
    return strstr(bytes, "gcc ") != NULL;
}
static int save(void) {
    static const char bytes[] = "foreground saved during guest compilation\n";
    int fd = open("/persist/foreground.pending", O_WRONLY | O_CREAT | O_TRUNC, 0640);
    if (fd < 0) return -1;
    if (write(fd, bytes, sizeof(bytes) - 1) != (ssize_t)sizeof(bytes) - 1 || fsync(fd) || close(fd)) return -1;
    return rename("/persist/foreground.pending", "/persist/foreground.txt");
}
static void stress(void) {
    char bytes[4096]; memset(bytes, 0x5a, sizeof(bytes));
    int fd = open("/persist/io-stress", O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (fd < 0) _exit(21);
    for (unsigned n = 0;; ++n) {
        if (lseek(fd, (off_t)(n % 32) * sizeof(bytes), SEEK_SET) < 0
            || write(fd, bytes, sizeof(bytes)) != (ssize_t)sizeof(bytes) || fsync(fd)) _exit(22);
    }
}
int main(int argc, char **argv) {
    CHECK(argc == 2 && (!strcmp(argv[1], "build") || !strcmp(argv[1], "interrupt")));
    int interrupted = !strcmp(argv[1], "interrupt");
    setvbuf(stdout, NULL, _IONBF, 0);
    CHECK(chdir("/persist/bootstrap") == 0);
    pid_t builder = fork(); CHECK(builder >= 0);
    if (!builder) {
        if (setpgid(0, 0)) _exit(10);
        int log = open("/persist/build.log", O_WRONLY | O_CREAT | O_TRUNC, 0644);
        if (log < 0 || dup2(log, 1) != 1 || dup2(log, 2) != 2 || close(log)) _exit(11);
        execl("/bin/make", "make", "-B", "-f", "Makefile.anova", "-j1", (char *)NULL);
        _exit(12);
    }
    pid_t writer = fork(); CHECK(writer >= 0);
    if (!writer) stress();
    int ready = 0;
    for (int retry = 0; retry < 300; ++retry) {
        if (compiler_log()) { ready = 1; break; }
        struct timespec delay = {0, 100000000};
        CHECK(nanosleep(&delay, NULL) == 0);
    }
    CHECK(ready);
    puts("[M16:ACTIVE] compiler_log=true storage_writer=true");
    char command[32]; int status = 0;
    while (fgets(command, sizeof(command), stdin)) {
        if (!strcmp(command, "ping\n")) {
            CHECK(waitpid(builder, &status, WNOHANG) == 0);
            pid_t echo = fork(); CHECK(echo >= 0);
            if (!echo) { execl("/linux/echo", "echo", "[M16:PING] executed=true", (char *)NULL); _exit(13); }
            CHECK(reap(echo, &status) == 0 && status == 0);
        } else if (!strcmp(command, "save\n")) {
            CHECK(waitpid(builder, &status, WNOHANG) == 0 && save() == 0);
            puts("[M16:SAVE] fsync=true rename=true");
        } else if (!strcmp(command, "log\n")) {
            CHECK(waitpid(builder, &status, WNOHANG) == 0 && compiler_log());
            puts("[M16:LOG] compiler=true");
        } else if (!strcmp(command, "interrupt\n") && interrupted) {
            CHECK(kill(-builder, SIGINT) == 0 && reap(builder, &status) == 0 && status != 0);
            puts("[M16:INTERRUPT] group_reaped=true");
            break;
        } else if (!strcmp(command, "wait\n") && !interrupted) {
            CHECK(reap(builder, &status) == 0 && status == 0);
            puts("[M16:BUILD] exit=0");
            break;
        } else CHECK(0);
    }
    CHECK(kill(writer, SIGTERM) == 0 && reap(writer, &status) == 0 && WIFSIGNALED(status));
    CHECK(unlink("/persist/io-stress") == 0);
    puts("[M16:WORKLOAD] storage_writer_reaped=true");
    return 0;
}
