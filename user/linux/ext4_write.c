// SPDX-License-Identifier: Apache-2.0
// Exercise writable ext4 through real Linux/musl syscalls, fork, mappings and persisted metadata.
// Failure messages include a source line and errno so host automation cannot accept a banner alone.
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#define CHECK(c) do { if (!(c)) { printf("[M16:WRITE:FAIL] line=%d errno=%d\n", __LINE__, errno); return 1; } } while (0)
static int exact(int fd, const void *data, size_t length) {
    const char *p = data;
    while (length) {
        ssize_t n = write(fd, p, length);
        if (n <= 0) return -1;
        p += n; length -= (size_t)n;
    }
    return 0;
}
static int suite(void) {
    CHECK(mkdir("/persist/a", 0755) == 0);
    CHECK(mkdir("/persist/b", 0755) == 0);
    CHECK(mkdir("/persist/a/sub", 0755) == 0);
    int fd = open("/persist/a/sub/file", O_CREAT | O_EXCL | O_RDWR, 0644);
    CHECK(fd >= 0);
    CHECK(exact(fd, "old bytes", 9) == 0);
    CHECK(lseek(fd, 200 * 1024 * 1024 + 17, SEEK_SET) == 200 * 1024 * 1024 + 17);
    CHECK(exact(fd, "tail", 4) == 0);
    char zero[4096], got[4096]; memset(zero, 0, sizeof(zero));
    CHECK(pread(fd, got, sizeof(got), 64 * 1024 * 1024) == sizeof(got));
    CHECK(memcmp(got, zero, sizeof(got)) == 0);
    CHECK(ftruncate(fd, 3) == 0 && ftruncate(fd, 8192) == 0);
    CHECK(pread(fd, got, sizeof(got), 3) == sizeof(got));
    CHECK(memcmp(got, zero, sizeof(got)) == 0);
    CHECK(fchmod(fd, 0751) == 0);
    struct timespec times[2] = {{1234567890, 0}, {1234567891, 0}};
    CHECK(utimensat(AT_FDCWD, "/persist/a/sub/file", times, 0) == 0);
    struct stat st;
    CHECK(fstat(fd, &st) == 0 && (st.st_mode & 0777) == 0751);
    CHECK(st.st_atim.tv_sec == 1234567890 && st.st_mtim.tv_sec == 1234567891);
    CHECK(chdir("/persist/a/sub") == 0);
    CHECK(rename("/persist/a/sub", "/persist/b/moved") == 0);
    char cwd[128]; CHECK(getcwd(cwd, sizeof(cwd)) && strcmp(cwd, "/persist/b/moved") == 0);
    CHECK(rename("/persist/b", "/persist/b/moved/cycle") == -1 && errno == EINVAL);
    CHECK(chdir("/") == 0);
    int old = open("/persist/b/moved/target", O_CREAT | O_RDWR, 0644);
    CHECK(old >= 0 && exact(old, "retained", 8) == 0);
    CHECK(rename("/persist/b/moved/file", "/persist/b/moved/target") == 0);
    CHECK(pread(old, got, 8, 0) == 8 && memcmp(got, "retained", 8) == 0);
    CHECK(exact(old, "!", 1) == 0 && close(old) == 0);
    void *map = mmap(NULL, 4096, PROT_READ, MAP_SHARED, fd, 0);
    CHECK(map != MAP_FAILED && memcmp(map, "old", 3) == 0);
    CHECK(pwrite(fd, "NEW", 3, 0) == 3 && memcmp(map, "NEW", 3) == 0);
    CHECK(unlink("/persist/b/moved/target") == 0);
    CHECK(open("/persist/b/moved/target", O_RDONLY) == -1 && errno == ENOENT);
    int fresh = open("/persist/b/moved/target", O_CREAT | O_RDWR, 0644);
    CHECK(fresh >= 0 && exact(fresh, "new identity", 12) == 0 && close(fresh) == 0);
    CHECK(lseek(fd, 0, SEEK_SET) == 0);
    int shared = dup(fd); CHECK(shared >= 0);
    pid_t child = fork(); CHECK(child >= 0);
    if (!child) {
        char c = 0;
        _exit(read(shared, &c, 1) == 1 && c == 'N' ? 0 : 9);
    }
    int status = 0; CHECK(waitpid(child, &status, 0) == child && status == 0);
    CHECK(read(fd, got, 2) == 2 && memcmp(got, "EW", 2) == 0);
    CHECK(close(shared) == 0 && close(fd) == 0);
    CHECK(memcmp(map, "NEW", 3) == 0 && munmap(map, 4096) == 0);
    CHECK(mkdir("/persist/a/empty", 0755) == 0 && mkdir("/persist/b/empty", 0755) == 0);
    int dir = open("/persist/b/empty", O_RDONLY | O_DIRECTORY); CHECK(dir >= 0);
    CHECK(rename("/persist/a/empty", "/persist/b/empty") == 0 && close(dir) == 0);
    CHECK(rmdir("/persist/b/empty") == 0);
    CHECK(rmdir("/persist/b/moved") == -1 && errno == ENOTEMPTY);
    CHECK(unlink("/persist/b/moved/target") == 0 && rmdir("/persist/b/moved") == 0);
    CHECK(rmdir("/persist/a") == 0 && rmdir("/persist/b") == 0);
    fd = open("/persist/metadata", O_CREAT | O_RDWR, 0644); CHECK(fd >= 0);
    CHECK(exact(fd, "metadata bytes\n", 15) == 0 && fchmod(fd, 0751) == 0);
    CHECK(utimensat(AT_FDCWD, "/persist/metadata", times, 0) == 0 && fsync(fd) == 0 && close(fd) == 0);
    puts("[M16:WRITE] sparse=zero resize=exact rename=checked orphan=retained maps=coherent fork=shared metadata=exact");
    return 0;
}
static int reboot_check(void) {
    int fd = open("/persist/metadata", O_RDONLY); CHECK(fd >= 0);
    char got[32]; CHECK(read(fd, got, sizeof(got)) == 15 && memcmp(got, "metadata bytes\n", 15) == 0);
    struct stat st; CHECK(fstat(fd, &st) == 0 && (st.st_mode & 0777) == 0751);
    CHECK(st.st_atim.tv_sec == 1234567890 && st.st_mtim.tv_sec == 1234567891);
    CHECK(close(fd) == 0);
    puts("[M16:REBOOT] bytes=exact mode=0751 times=exact"); return 0;
}
static int full(void) {
    int fd = open("/persist/full", O_CREAT | O_RDWR, 0644); CHECK(fd >= 0);
    char data[4096]; memset(data, 'F', sizeof(data));
    off_t size = 0;
    for (;;) {
        ssize_t n = write(fd, data, sizeof(data));
        if (n == -1) { CHECK(errno == ENOSPC); break; }
        CHECK(n > 0); size += n;
    }
    struct stat st; CHECK(fstat(fd, &st) == 0 && st.st_size == size);
    CHECK(write(fd, data, sizeof(data)) == -1 && errno == ENOSPC);
    CHECK(lseek(fd, 0, SEEK_CUR) == size);
    CHECK(ftruncate(fd, 0) == 0 && exact(fd, "reuse", 5) == 0);
    CHECK(close(fd) == 0 && unlink("/persist/full") == 0);
    puts("[M16:ENOSPC] bytes=retained offset=retained blocks=reused"); return 0;
}
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    if (!strcmp(argv[1], "suite")) return suite();
    if (!strcmp(argv[1], "reboot")) return reboot_check();
    if (!strcmp(argv[1], "full")) return full();
    return 2;
}
