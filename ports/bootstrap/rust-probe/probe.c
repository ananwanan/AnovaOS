// SPDX-License-Identifier: Apache-2.0
// Diagnose actual missing bootstrap tools and Cargo dependency availability inside the guest.
// This host-built diagnostic does not reconstruct Rust, LLVM, Cargo or their dependencies.
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
    fprintf(stderr, "[M15:RUST:FAIL] operation=%s errno=%d\n", operation, errno);
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

static int missing_program(char *const arguments[], const char *kind, const char *identity) {
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
        execvp(arguments[0], arguments);
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
        fprintf(stderr, "[M15:RUST:FAIL] operation=missing-tool name=%s "
                "exec_errno=%d bytes=%zu wait_status=%d\n",
                arguments[0], rejected, used, status);
        result = -1;
    }
    if (!result) printf("[M15:RUST:%s] %s exec_errno=%d\n", kind, identity, rejected);
    return result;
}

static int cargo_dependency_probe(void) {
    int output = open("cargo-dependency.log", O_WRONLY | O_CREAT | O_TRUNC, 0600);
    if (output < 0) return fail("cargo-open-log");
    int channel[2];
    if (pipe(channel)) {
        int saved = errno;
        if (close(output)) fail("cargo-close-log");
        errno = saved;
        return fail("cargo-pipe");
    }
    pid_t child = fork();
    if (child < 0) {
        int saved = errno;
        if (close(channel[0])) fail("cargo-close-reader");
        if (close(channel[1])) fail("cargo-close-writer");
        if (close(output)) fail("cargo-close-log");
        errno = saved;
        return fail("cargo-fork");
    }
    if (!child) {
        if (close(channel[0]) || close(output)) _exit(126);
        if (dup2(channel[1], STDOUT_FILENO) < 0 || dup2(channel[1], STDERR_FILENO) < 0)
            _exit(126);
        if (close(channel[1])) _exit(126);
        char *arguments[] = {"cargo", "check", "--offline", "--manifest-path",
                             "cargo-dependency/Cargo.toml", NULL};
        execvp(arguments[0], arguments);
        dprintf(STDERR_FILENO, "[M15:RUST:FAIL] operation=cargo-exec errno=%d\n", errno);
        _exit(127);
    }
    int result = 0;
    if (close(channel[1])) result = fail("cargo-close-writer");
    static char evidence[LOG_LIMIT + 1];
    size_t used = 0;
    int killed = 0;
    for (;;) {
        char buffer[4096];
        ssize_t size = read(channel[0], buffer, sizeof buffer);
        if (size < 0 && errno == EINTR) continue;
        if (size < 0) {
            result = fail("cargo-read-output");
            break;
        }
        if (!size) break;
        if ((size_t)size > LOG_LIMIT - used) {
            errno = EFBIG;
            result = fail("cargo-log-limit");
            if (kill(child, SIGKILL) && errno != ESRCH) fail("cargo-kill-overflow");
            killed = 1;
            break;
        }
        memcpy(evidence + used, buffer, (size_t)size);
        used += (size_t)size;
        if (write_all(output, buffer, (size_t)size)) {
            result = fail("cargo-write-log");
            break;
        }
    }
    if (result && !killed && kill(child, SIGKILL) && errno != ESRCH)
        fail("cargo-kill-failed-capture");
    if (close(channel[0])) result = fail("cargo-close-reader");
    if (close(output)) result = fail("cargo-close-log");
    int status = 0;
    if (wait_child(child, &status)) result = -1;
    evidence[used] = '\0';
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 101
        || !strstr(evidence, "no matching package named `curl` found")
        || !strstr(evidence, "/rs/vendor")) {
        errno = 0;
        result = fail("cargo-expected-dependency-rejection");
        fprintf(stderr, "[M15:RUST:FAIL] cargo_wait_status=%d log_bytes=%zu\n", status, used);
        if (used) fprintf(stderr, "%s", evidence);
    }
    if (!result)
        printf("[M15:CARGO:DEPENDENCY] name=curl version=0.4.48 exit=101 offline-vendor=missing\n");
    return result;
}

int main(void) {
    if (setenv("PATH", "/bin:/tc/bin:/linux", 1) || setenv("LC_ALL", "C", 1)) {
        fail("set-environment");
        return 1;
    }
    static const char *const tools[] = {"python3", "cmake", "ninja", "git", "perl", "pkg-config"};
    for (size_t index = 0; index < sizeof tools / sizeof tools[0]; ++index) {
        char *arguments[] = {(char *)tools[index], "--version", NULL};
        char identity[64];
        int length = snprintf(identity, sizeof identity, "name=%s", tools[index]);
        if (length < 0 || (size_t)length >= sizeof identity) return 1;
        if (missing_program(arguments, "TOOL", identity)) return 1;
    }
    char *bootstrap[] = {"python3", "rust-x.py", "--help", NULL};
    if (missing_program(bootstrap, "BOOTSTRAP", "entry=rust-x.py")) return 1;
    return cargo_dependency_probe() ? 1 : 0;
}
