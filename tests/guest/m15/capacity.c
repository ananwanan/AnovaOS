// SPDX-License-Identifier: Apache-2.0
// Exercise file, descriptor and task limits, then prove rejection leaves resources usable.
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

static int fail(const char *operation) {
    fprintf(stderr, "[M15:CAPACITY:FAIL] operation=%s errno=%d\n", operation, errno);
    return -1;
}

static int file_limit(const char *path) {
    static const char sentinel[] = "M15 file intact\n";
    char saved[sizeof sentinel];
    struct stat metadata;
    int result = -1;
    int fd = open(path, O_CREAT | O_EXCL | O_RDWR, 0600);
    if (fd < 0) return fail("create-scratch");
    if (write(fd, sentinel, sizeof sentinel - 1) != (ssize_t)(sizeof sentinel - 1)) {
        fail("write-sentinel");
        goto cleanup;
    }
    errno = 0;
    if (ftruncate(fd, 1024 * 1024 + 1) != -1 || errno != EFBIG) {
        fail("reject-file-limit");
        goto cleanup;
    }
    if (fstat(fd, &metadata) || metadata.st_size != (off_t)(sizeof sentinel - 1)
        || lseek(fd, 0, SEEK_SET) != 0
        || read(fd, saved, sizeof saved) != (ssize_t)(sizeof sentinel - 1)
        || memcmp(saved, sentinel, sizeof sentinel - 1)) {
        fail("sentinel-preserved");
        goto cleanup;
    }
    result = 0;
cleanup:
    if (close(fd)) result = fail("close-scratch");
    if (unlink(path)) result = fail("remove-scratch");
    return result;
}

static int descriptor_limit(void) {
    int descriptors[64];
    int count = 0;
    int rejected = 0;
    while (count < (int)(sizeof descriptors / sizeof descriptors[0])) {
        int fd = open("/dev/null", O_RDONLY);
        if (fd < 0) {
            rejected = errno == EMFILE;
            break;
        }
        descriptors[count++] = fd;
    }
    int result = count > 0 && rejected ? count : fail("reject-descriptor-limit");
    while (count) {
        if (close(descriptors[--count])) result = fail("close-descriptor");
    }
    int fd = open("/dev/null", O_RDONLY);
    if (fd < 0) return fail("descriptor-recovery");
    if (close(fd)) return fail("close-recovered-descriptor");
    return result;
}

static int reap(pid_t child) {
    int status;
    pid_t result;
    do result = waitpid(child, &status, 0); while (result < 0 && errno == EINTR);
    if (result != child || !WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        return fail("reap-child");
    }
    return 0;
}

static int task_limit(void) {
    int barrier[2];
    pid_t children[40];
    int count = 0;
    int rejected = 0;
    if (pipe(barrier)) return fail("create-child-barrier");
    while (count < (int)(sizeof children / sizeof children[0])) {
        pid_t child = fork();
        if (child < 0) {
            rejected = errno == EAGAIN;
            break;
        }
        if (child == 0) {
            char byte;
            ssize_t size;
            if (close(barrier[1])) _exit(1);
            do size = read(barrier[0], &byte, 1); while (size < 0 && errno == EINTR);
            int closed = close(barrier[0]);
            _exit(size == 0 && closed == 0 ? 0 : 1);
        }
        children[count++] = child;
    }
    int result = count > 0 && rejected ? count : fail("reject-task-limit");
    /* Every child waits for EOF. Closing both parent references releases all children. */
    if (close(barrier[0])) result = fail("close-parent-reader");
    if (close(barrier[1])) result = fail("release-children");
    for (int index = 0; index < count; ++index) {
        if (reap(children[index])) result = -1;
    }
    pid_t child = fork();
    if (child < 0) return fail("task-recovery");
    if (child == 0) _exit(0);
    if (reap(child)) return -1;
    return result;
}

int main(int argc, char **argv) {
    if (argc > 2) {
        fputs("Usage: m15-capacity [SCRATCH_PATH]\n", stderr);
        return 2;
    }
    if (file_limit(argc == 2 ? argv[1] : "/persist/m15-capacity") < 0) return 1;
    int descriptors = descriptor_limit();
    if (descriptors < 0) return 1;
    int children = task_limit();
    if (children < 0) return 1;
    printf("[M15:CAPACITY] files=efbig-preserved fds=%d children=%d recovery=ok\n",
           descriptors, children);
    return 0;
}
