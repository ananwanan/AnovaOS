// SPDX-License-Identifier: Apache-2.0
// Measure missing GCC build tools and the unchanged configure entry point in the guest.
// This host-built diagnostic supplies no reconstructed compiler or dependency library.
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#define LOG_LIMIT (256 * 1024)

static int fail(const char *operation) {
    fprintf(stderr, "[M15:GCC:FAIL] operation=%s errno=%d\n", operation, errno);
    return -1;
}

static int write_all(int fd, const void *buffer, size_t length) {
    const unsigned char *bytes = buffer;
    while (length) {
        ssize_t size = write(fd, bytes, length);
        if (size < 0 && errno == EINTR) continue;
        if (size <= 0) {
            if (!size) errno = EIO;
            return -1;
        }
        bytes += size;
        length -= (size_t)size;
    }
    return 0;
}

static int wait_child(pid_t child, int *status) {
    pid_t result;
    do result = waitpid(child, status, 0); while (result < 0 && errno == EINTR);
    if (result != child) return fail("wait-child");
    return 0;
}

static int missing_tool(const char *name) {
    int channel[2];
    if (pipe(channel)) return fail("tool-pipe");
    pid_t child = fork();
    if (child < 0) {
        int saved = errno;
        if (close(channel[0])) fail("tool-close-reader");
        if (close(channel[1])) fail("tool-close-writer");
        errno = saved;
        return fail("tool-fork");
    }
    if (!child) {
        if (close(channel[0])) _exit(126);
        char *arguments[] = {(char *)name, "--version", NULL};
        execvp(name, arguments);
        int rejected = errno;
        if (write_all(channel[1], &rejected, sizeof rejected)) _exit(126);
        if (close(channel[1])) _exit(126);
        _exit(127);
    }
    int result = 0;
    if (close(channel[1])) result = fail("tool-close-writer");
    unsigned char evidence[sizeof(int) + 1];
    size_t used = 0;
    while (used < sizeof evidence) {
        ssize_t size = read(channel[0], evidence + used, sizeof evidence - used);
        if (size < 0 && errno == EINTR) continue;
        if (size < 0) {
            result = fail("tool-read-errno");
            break;
        }
        if (!size) break;
        used += (size_t)size;
    }
    if (close(channel[0])) result = fail("tool-close-reader");
    int status = 0;
    if (wait_child(child, &status)) result = -1;
    int rejected = 0;
    if (used == sizeof rejected) memcpy(&rejected, evidence, sizeof rejected);
    if (used != sizeof rejected || rejected != ENOENT || !WIFEXITED(status)
        || WEXITSTATUS(status) != 127) {
        fprintf(stderr, "[M15:GCC:FAIL] operation=missing-tool name=%s "
                "exec_errno=%d bytes=%zu wait_status=%d\n", name, rejected, used, status);
        result = -1;
    }
    if (!result) printf("[M15:GCC:TOOL] name=%s exec_errno=%d\n", name, rejected);
    return result;
}

static int configure_probe(void) {
    int output = open("configure.log", O_WRONLY | O_CREAT | O_TRUNC, 0600);
    if (output < 0) return fail("configure-open-log");
    int channel[2];
    if (pipe(channel)) {
        int saved = errno;
        if (close(output)) fail("configure-close-log");
        errno = saved;
        return fail("configure-pipe");
    }
    pid_t child = fork();
    if (child < 0) {
        int saved = errno;
        if (close(channel[0])) fail("configure-close-reader");
        if (close(channel[1])) fail("configure-close-writer");
        if (close(output)) fail("configure-close-log");
        errno = saved;
        return fail("configure-fork");
    }
    if (!child) {
        if (close(channel[0]) || close(output)) _exit(126);
        if (dup2(channel[1], STDOUT_FILENO) < 0 || dup2(channel[1], STDERR_FILENO) < 0)
            _exit(126);
        if (close(channel[1])) _exit(126);
        char *arguments[] = {
            "/linux/sh", "./configure", "--srcdir=.", "--build=x86_64-linux-musl",
            "--host=x86_64-linux-musl", "--target=x86_64-linux-musl",
            "--disable-bootstrap", "--enable-languages=c,c++", "--disable-multilib", NULL,
        };
        execv(arguments[0], arguments);
        dprintf(STDERR_FILENO, "[M15:GCC:FAIL] operation=configure-exec errno=%d\n", errno);
        _exit(127);
    }
    int result = 0;
    if (close(channel[1])) result = fail("configure-close-writer");
    static char evidence[LOG_LIMIT + 1];
    size_t used = 0;
    int killed = 0;
    for (;;) {
        char buffer[4096];
        ssize_t size = read(channel[0], buffer, sizeof buffer);
        if (size < 0 && errno == EINTR) continue;
        if (size < 0) {
            result = fail("configure-read-output");
            break;
        }
        if (!size) break;
        if ((size_t)size > LOG_LIMIT - used) {
            errno = EFBIG;
            result = fail("configure-log-limit");
            if (kill(child, SIGKILL) && errno != ESRCH) fail("configure-kill-overflow");
            killed = 1;
            break;
        }
        memcpy(evidence + used, buffer, (size_t)size);
        used += (size_t)size;
        if (write_all(output, buffer, (size_t)size)) {
            result = fail("configure-write-log");
            break;
        }
    }
    if (result && !killed && kill(child, SIGKILL) && errno != ESRCH)
        fail("configure-kill-failed-capture");
    if (close(channel[0])) result = fail("configure-close-reader");
    if (close(output)) result = fail("configure-close-log");
    int status = 0;
    if (wait_child(child, &status)) result = -1;
    evidence[used] = '\0';
    if (!WIFEXITED(status) || !WEXITSTATUS(status)
        || !strstr(evidence, "sed: not found") || strstr(evidence, "cannot find sources")) {
        errno = 0;
        result = fail("configure-expected-tool-rejection");
        fprintf(stderr, "[M15:GCC:FAIL] configure_wait_status=%d log_bytes=%zu\n", status, used);
    }
    if (!result) printf("[M15:GCC:CONFIGURE] exit=%d selected-entry-point=blocked\n",
                        WEXITSTATUS(status));
    return result;
}

int main(void) {
    if (setenv("PATH", "/bin:/tc/bin:/linux", 1) || setenv("CONFIG_SHELL", "/linux/sh", 1)
        || setenv("LC_ALL", "C", 1)) {
        fail("set-environment");
        return 1;
    }
    static const char *const tools[] = {"sed", "awk", "grep", "expr", "tr", "sort", "cmp", "mv", "ln"};
    for (size_t index = 0; index < sizeof tools / sizeof tools[0]; ++index) {
        if (missing_tool(tools[index])) return 1;
    }
    return configure_probe() ? 1 : 0;
}
