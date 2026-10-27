// SPDX-License-Identifier: Apache-2.0
// Pin tmpfs directory identities to exhaust the real VFS quota, then verify rollback and reuse.+// Run twice in one process so leaked cache slots cannot be hidden by a reboot.
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <sys/stat.h>
#include <unistd.h>

#define CHECK(c) do { if (!(c)) { fprintf(stderr, "[M16:VFS:FAIL] line=%d errno=%d\n", __LINE__, errno); return -1; } } while (0)

static int exhaust(void) {
    CHECK(mkdir("/tmp/m16-vfs", 0700) == 0);
    int count;
    char path[64];
    for (count = 0; count < 2048; ++count) {
        snprintf(path, sizeof(path), "/tmp/m16-vfs/d%04d", count);
        if (mkdir(path, 0700)) {
            CHECK(errno == ENOSPC);
            break;
        }
    }
    CHECK(count >= 32 && count < 2048);
    CHECK(open("/tmp/m16-vfs/rejected", O_CREAT | O_WRONLY, 0600) == -1
          && errno == ENOSPC);
    for (int index = count - 1; index >= 0; --index) {
        snprintf(path, sizeof(path), "/tmp/m16-vfs/d%04d", index);
        CHECK(rmdir(path) == 0);
    }
    struct stat metadata;
    CHECK(stat("/tmp/m16-vfs/rejected", &metadata) == -1 && errno == ENOENT);
    int fd = open("/tmp/m16-vfs/reused", O_CREAT | O_EXCL | O_WRONLY, 0600);
    CHECK(fd >= 0 && write(fd, "reused", 6) == 6 && close(fd) == 0);
    CHECK(unlink("/tmp/m16-vfs/reused") == 0 && rmdir("/tmp/m16-vfs") == 0);
    return count;
}

int main(void) {
    int first = exhaust();
    if (first < 0) return 1;
    int second = exhaust();
    if (second < 0 || first != second) return 1;
    printf("[M16:VFS] directories=%d repeated=%d rollback=true reuse=true\n", first, second);
    return 0;
}
