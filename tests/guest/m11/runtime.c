// SPDX-License-Identifier: Apache-2.0
// Check vfork suspension/shared bytes, exec failure, private file snapshots and permission errors.
#include <errno.h>
#include <dirent.h>
#include <fcntl.h>
#include <stdio.h>
#include <signal.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/resource.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>
#define CHECK(x) do { if (!(x)) { fprintf(stderr,"[M11:FAIL] line=%d errno=%d\n",__LINE__,errno); return 1; } } while(0)
static volatile sig_atomic_t shared;
static volatile sig_atomic_t observed;
static void handler(int number) { (void)number; observed = shared; }
int main(int argc, char **argv) {
    if (argc == 2 && !strcmp(argv[1], "child")) return 23;
    if (argc == 2 && !strcmp(argv[1], "temps")) {
        DIR *dir = opendir("/tmp");
        CHECK(dir);
        struct dirent *entry;
        while ((entry = readdir(dir)))
            CHECK(!strcmp(entry->d_name, ".") || !strcmp(entry->d_name, "..") || !strcmp(entry->d_name, "work"));
        CHECK(closedir(dir) == 0);
        puts("[M11:TEMP] clean");
        return 0;
    }
    signal(SIGUSR1, handler);
    pid_t child = vfork();
    if (child == 0) {
        kill(getppid(), SIGUSR1);
        shared = 17;
        execl("/bin/m11-runtime", "m11-runtime", "child", (char *)0);
        _exit(90);
    }
    int status;
    CHECK(child > 0 && shared == 17 && observed == 17);
    CHECK(waitpid(child, &status, 0) == child && WIFEXITED(status) && WEXITSTATUS(status) == 23);
    child = vfork();
    if (child == 0) {
        shared = 42;
        execl("/absent-m11", "absent", (char *)0);
        _exit(errno == ENOENT ? 7 : 91);
    }
    CHECK(child > 0 && shared == 42);
    CHECK(waitpid(child, &status, 0) == child && WIFEXITED(status) && WEXITSTATUS(status) == 7);
    mode_t old = umask(0077);
    int fd = open("/tmp/m11-object", O_CREAT | O_EXCL | O_RDWR, 0666);
    struct stat info;
    CHECK(fd >= 0 && fstat(fd, &info) == 0 && (info.st_mode & 0777) == 0600);
    CHECK(write(fd, "AB", 2) == 2);
    char *p = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_PRIVATE, fd, 0);
    CHECK(p != MAP_FAILED && p[0] == 'A' && p[1] == 'B' && p[2] == 0);
    CHECK(lseek(fd, 0, SEEK_CUR) == 2);
    CHECK(lseek(fd, 0, SEEK_SET) == 0 && write(fd, "CD", 2) == 2 && p[0] == 'A');
    p[0] = 'X';
    CHECK(lseek(fd, 0, SEEK_SET) == 0);
    char bytes[2];
    CHECK(read(fd, bytes, 2) == 2 && bytes[0] == 'C');
    CHECK(munmap(p, 4096) == 0 && fchmod(fd, 0755) == 0);
    CHECK(fstat(fd, &info) == 0 && (info.st_mode & 0777) == 0755);
    CHECK(fchmod(fd, 04755) == -1 && errno == ENOTSUP);
    CHECK(close(fd) == 0 && unlink("/tmp/m11-object") == 0);
    umask(old);
    struct rlimit limit;
    CHECK(getrlimit(RLIMIT_NOFILE, &limit) == 0 && limit.rlim_cur == 32 && limit.rlim_max == 32);
    puts("[M11:RUNTIME] vfork exec-failure snapshots umask modes limits");
    return 0;
}
