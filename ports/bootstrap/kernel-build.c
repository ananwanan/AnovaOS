// SPDX-License-Identifier: Apache-2.0
// Run isolated guest kernel builds and publish a checked ELF only after successful compilation.
// This host-built driver is a recipe/diagnostic; Cargo, rustc and rust-lld construct the kernel.
// ANOVA_EXT4 selects the separately accepted writable filesystem feature for M16 work disks.
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#define WORK "/persist/kernel"
#define KERNEL WORK "/kernel.elf"
#define CANDIDATE WORK "/kernel.candidate"
#define TARGET WORK "/target/x86_64-unknown-none/release/anova-kernel"
#define FILE_LIMIT (4 * 1024 * 1024)
#define LOG_LIMIT (512 * 1024)
#ifdef ANOVA_EXT4
#define GUEST_FEATURES "--features=ext4-write,bootstrap-metrics"
#else
#define GUEST_FEATURES "--features=bootstrap-metrics"
#endif

static int fail(const char *operation) {
    fprintf(stderr, "[M15:KERNEL:FAIL] operation=%s errno=%d\n", operation, errno);
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

static int read_exact_at(int fd, off_t offset, void *buffer, size_t length) {
    if (lseek(fd, offset, SEEK_SET) != offset) return -1;
    unsigned char *bytes = buffer;
    while (length) {
        ssize_t size = read(fd, bytes, length);
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

static uint16_t u16(const unsigned char *bytes) {
    return (uint16_t)bytes[0] | (uint16_t)bytes[1] << 8;
}

static uint32_t u32(const unsigned char *bytes) {
    return (uint32_t)bytes[0] | (uint32_t)bytes[1] << 8
        | (uint32_t)bytes[2] << 16 | (uint32_t)bytes[3] << 24;
}

static uint64_t u64(const unsigned char *bytes) {
    return (uint64_t)u32(bytes) | (uint64_t)u32(bytes + 4) << 32;
}

static int checked_kernel(const char *path, off_t *length) {
    int fd = open(path, O_RDONLY);
    if (fd < 0) return fail("open-kernel");
    int result = -1;
    struct stat metadata;
    unsigned char header[64];
    if (fstat(fd, &metadata) || !S_ISREG(metadata.st_mode)
        || metadata.st_size < 64 || metadata.st_size > FILE_LIMIT
        || read_exact_at(fd, 0, header, sizeof header)) {
        fail("kernel-size-header");
        goto cleanup;
    }
    if (memcmp(header, "\177ELF\002\001\001", 7) || u16(header + 16) != 2
        || u16(header + 18) != 62 || u16(header + 54) != 56) {
        fail("kernel-static-x86-64");
        goto cleanup;
    }
    uint64_t entry = u64(header + 24);
    uint64_t offset = u64(header + 32);
    unsigned count = u16(header + 56);
    if (!count || count > 128 || offset > (uint64_t)metadata.st_size
        || (uint64_t)count * 56 > (uint64_t)metadata.st_size - offset) {
        fail("kernel-program-headers");
        goto cleanup;
    }
    int executable_entry = 0;
    for (unsigned index = 0; index < count; ++index) {
        unsigned char program[56];
        if (read_exact_at(fd, (off_t)(offset + (uint64_t)index * 56),
                          program, sizeof program)) {
            fail("kernel-program-read");
            goto cleanup;
        }
        uint32_t kind = u32(program);
        if (kind == 2 || kind == 3) {
            fail("kernel-dynamic-segment");
            goto cleanup;
        }
        if (kind != 1) continue;
        uint32_t flags = u32(program + 4);
        uint64_t file = u64(program + 8), address = u64(program + 16);
        uint64_t filesz = u64(program + 32), memsz = u64(program + 40);
        if (filesz > memsz || file > (uint64_t)metadata.st_size
            || filesz > (uint64_t)metadata.st_size - file
            || address < UINT64_C(0xffffffff80000000) || memsz > UINT64_MAX - address
            || (flags & 3) == 3) {
            fail("kernel-load-segment");
            goto cleanup;
        }
        executable_entry |= (flags & 1) && entry >= address && entry - address < memsz;
    }
    if (!executable_entry) {
        fail("kernel-executable-entry");
        goto cleanup;
    }
    *length = metadata.st_size;
    result = 0;
cleanup:
    if (close(fd)) result = fail("close-kernel");
    return result;
}

static int directory(const char *path) {
    if (!mkdir(path, 0755)) return 0;
    if (errno != EEXIST) return fail("create-directory");
    struct stat metadata;
    if (stat(path, &metadata) || !S_ISDIR(metadata.st_mode)) return fail("check-directory");
    return 0;
}

static int environment(const char *identity) {
    static const char *const settings[][2] = {
        {"PATH", "/bin:/linux"},
        {"LD_LIBRARY_PATH", "/rs/lib:/lib"},
        {"LC_ALL", "C"},
        {"TMPDIR", WORK "/tmp"},
        {"RUSTC", "/bin/rustc"},
        {"RUST_MIN_STACK", "8388608"},
        {"RUST_BACKTRACE", "0"},
        {"RUSTFLAGS", "-Crelocation-model=static -Ccode-model=kernel -Cno-redzone=yes "
                      "-Copt-level=z -Cpanic=abort -Ccodegen-units=1 -Cdebuginfo=0 -Cstrip=symbols"},
        {"CARGO_HOME", WORK "/cargo-home"},
        {"CARGO_BUILD_JOBS", "1"},
        {"CARGO_INCREMENTAL", "0"},
        {"CARGO_NET_OFFLINE", "true"},
        {"CARGO_TERM_COLOR", "never"},
        {"CARGO_TERM_PROGRESS_WHEN", "never"},
        {"CARGO_PROFILE_RELEASE_OPT_LEVEL", "z"},
        {"CARGO_PROFILE_RELEASE_DEBUG", "0"},
        {"CARGO_PROFILE_RELEASE_STRIP", "symbols"},
        {"CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "1"},
        {"CARGO_PROFILE_RELEASE_INCREMENTAL", "false"},
    };
    for (size_t index = 0; index < sizeof settings / sizeof settings[0]; ++index)
        if (setenv(settings[index][0], settings[index][1], 1)) return fail("set-environment");
    if (unsetenv("CARGO_ENCODED_RUSTFLAGS") || unsetenv("RUSTC_WRAPPER")
        || unsetenv("RUSTC_WORKSPACE_WRAPPER")
        || setenv("ANOVA_KERNEL_BUILD_ID", identity, 1)) return fail("set-build-identity");
    return 0;
}

static int execute(char *const arguments[], const char *log, int expected) {
    int output = open(log, O_CREAT | O_TRUNC | O_WRONLY, 0600);
    if (output < 0) return fail("open-log");
    int channel[2];
    if (pipe(channel)) {
        int saved = errno;
        close(output);
        errno = saved;
        return fail("capture-pipe");
    }
    fflush(NULL);
    pid_t child = fork();
    if (child < 0) {
        int saved = errno;
        close(channel[0]);
        close(channel[1]);
        close(output);
        errno = saved;
        return fail("build-fork");
    }
    if (!child) {
        if (close(channel[0]) || close(output)
            || dup2(channel[1], STDOUT_FILENO) < 0
            || dup2(channel[1], STDERR_FILENO) < 0 || close(channel[1])) _exit(126);
        execv(arguments[0], arguments);
        dprintf(STDERR_FILENO, "[M15:KERNEL:FAIL] operation=exec-build errno=%d\n", errno);
        _exit(127);
    }
    int result = 0;
    if (close(channel[1])) result = fail("capture-close-writer");
    size_t used = 0;
    for (;;) {
        char buffer[4096];
        ssize_t size = read(channel[0], buffer, sizeof buffer);
        if (size < 0 && errno == EINTR) continue;
        if (size < 0) {
            result = fail("capture-read");
            break;
        }
        if (!size) break;
        if ((size_t)size > LOG_LIMIT - used) {
            errno = EFBIG;
            result = fail("capture-log-limit");
            break;
        }
        used += (size_t)size;
        if (write_all(output, buffer, (size_t)size)
            || write_all(STDOUT_FILENO, buffer, (size_t)size)) {
            result = fail("capture-write");
            break;
        }
    }
    if (result && kill(child, SIGKILL) && errno != ESRCH) fail("capture-kill");
    if (close(channel[0])) result = fail("capture-close-reader");
    if (fsync(output)) result = fail("capture-sync-log");
    if (close(output)) result = fail("capture-close-log");
    int status = 0;
    pid_t waited;
    do waited = waitpid(child, &status, 0); while (waited < 0 && errno == EINTR);
    if (waited != child) return fail("build-wait");
    if (!WIFEXITED(status) || WEXITSTATUS(status) != expected) {
        fprintf(stderr, "[M15:KERNEL:FAIL] operation=build-status expected=%d wait_status=%d\n",
                expected, status);
        return -1;
    }
    return result;
}

static int publish(void) {
    off_t expected;
    if (checked_kernel(TARGET, &expected)) return -1;
    int source = open(TARGET, O_RDONLY);
    if (source < 0) return fail("open-built-kernel");
    int destination = open(CANDIDATE, O_CREAT | O_EXCL | O_WRONLY, 0600);
    if (destination < 0) {
        int saved = errno;
        close(source);
        errno = saved;
        return fail("open-candidate");
    }
    int result = 0;
    off_t copied = 0;
    for (;;) {
        char buffer[4096];
        ssize_t size = read(source, buffer, sizeof buffer);
        if (size < 0 && errno == EINTR) continue;
        if (size < 0) {
            result = fail("read-built-kernel");
            break;
        }
        if (!size) break;
        if (size > expected - copied || write_all(destination, buffer, (size_t)size)) {
            result = fail("copy-candidate");
            break;
        }
        copied += size;
    }
    if (copied != expected) result = fail("candidate-copy-size");
    if (close(source)) result = fail("close-built-kernel");
    if (fsync(destination)) result = fail("sync-candidate");
    if (close(destination)) result = fail("close-candidate");
    off_t actual;
    if (!result && checked_kernel(CANDIDATE, &actual)) result = -1;
    if (!result && actual != expected) result = fail("candidate-checked-size");
    if (!result && rename(CANDIDATE, KERNEL)) result = fail("publish-kernel");
    if (result && unlink(CANDIDATE) && errno != ENOENT) fail("remove-rejected-candidate");
    if (!result) printf("[M15:KERNEL:PUBLISHED] bytes=%lld path=%s\n",
                        (long long)actual, KERNEL);
    return result;
}

static int build(const char *identity) {
    char *arguments[] = {"/bin/cargo", "build", "--offline", "--locked", "--release",
        "--target=x86_64-unknown-none", GUEST_FEATURES,
        "--manifest-path=/ks/source/Cargo.toml",
        "-p", "anova-kernel", "--target-dir=" WORK "/target", NULL};
    printf("[M15:KERNEL:BUILD] id=%s target=x86_64-unknown-none seed=1.90.0\n", identity);
    if (execute(arguments, WORK "/build.log", 0) || publish()) return -1;
    printf("[M15:KERNEL:BUILT] id=%s exit=0\n", identity);
    return 0;
}

static int failure(const char *identity) {
    off_t before, after;
    if (checked_kernel(KERNEL, &before)) return -1;
    static const char invalid[] = "fn main() { let value = ; }\n";
    int fd = open(WORK "/bad.rs", O_CREAT | O_TRUNC | O_WRONLY, 0600);
    if (fd < 0) return fail("open-rejected-source");
    int result = write_all(fd, invalid, sizeof invalid - 1);
    if (fsync(fd)) result = fail("sync-rejected-source");
    if (close(fd)) result = fail("close-rejected-source");
    if (result) return fail("write-rejected-source");
    char *arguments[] = {"/bin/rustc", "--edition=2024", WORK "/bad.rs", "-o", CANDIDATE, NULL};
    if (execute(arguments, WORK "/failure.log", 1)) return -1;
    struct stat candidate;
    errno = 0;
    if (!stat(CANDIDATE, &candidate) || errno != ENOENT) return fail("unexpected-failure-output");
    if (checked_kernel(KERNEL, &after) || before != after) return fail("retained-kernel-size");
    printf("[M15:KERNEL:REJECTED] id=%s compiler_exit=1 retained_bytes=%lld\n",
           identity, (long long)after);
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 3 || (strcmp(argv[1], "build") && strcmp(argv[1], "failure"))
        || strlen(argv[2]) != 64 || strspn(argv[2], "0123456789abcdef") != 64) {
        fputs("Usage: m15-kernel-build {build|failure} SHA256_ID\n", stderr);
        return 2;
    }
    if (directory(WORK) || directory(WORK "/tmp") || directory(WORK "/cargo-home")
        || chdir("/ks/source") || environment(argv[2])) return 1;
    if (unlink(CANDIDATE) && errno != ENOENT) {
        fail("remove-stale-candidate");
        return 1;
    }
    return (strcmp(argv[1], "build") ? failure(argv[2]) : build(argv[2])) ? 1 : 0;
}
