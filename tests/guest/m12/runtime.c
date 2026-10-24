// SPDX-License-Identifier: Apache-2.0
// Verify backend file bounds, filesystem/IPC ownership, timestamps, signals and readiness.
#define _GNU_SOURCE
#include <assert.h>
#include <errno.h>
#include <dirent.h>
#include <fcntl.h>
#include <signal.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/file.h>
#include <sys/syscall.h>
#include <sys/mman.h>
#include <sys/socket.h>
#include <sys/select.h>
#include <sys/stat.h>
#include <poll.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static unsigned char alternate[32768];
static volatile sig_atomic_t handled;
static atomic_int detached_tid, detached_ready;
static volatile sig_atomic_t selected_signal;
static void select_handler(int signal) { (void)signal; selected_signal = 1; }
static void timestamp_checks(void) {
    int fd = open("timestamp-file", O_CREAT | O_RDWR | O_TRUNC, 0600);
    struct stat value;
    assert(fd >= 0 && fstat(fd, &value) == 0 && value.st_size == 0);
    assert(value.st_mtim.tv_nsec == 0 && value.st_ctim.tv_nsec == 0);
    assert(write(fd, "T", 1) == 1 && fstat(fd, &value) == 0);
    assert(value.st_mtim.tv_nsec == 0 && value.st_ctim.tv_nsec == 0);
    const struct timespec written = value.st_mtim;
    struct timespec unchanged[2] = {{.tv_nsec = UTIME_OMIT}, written};
    assert(futimens(fd, unchanged) == 0 && fstat(fd, &value) == 0);
    assert(value.st_mtim.tv_sec == written.tv_sec && value.st_mtim.tv_nsec == written.tv_nsec);
    assert(ftruncate(fd, 16) == 0 && fstat(fd, &value) == 0 && value.st_size == 16);
    assert(value.st_mtim.tv_nsec == 0 && value.st_ctim.tv_nsec == 0);
    assert(ftruncate(fd, 2) == 0 && fstat(fd, &value) == 0 && value.st_size == 2);
    assert(value.st_mtim.tv_nsec == 0 && value.st_ctim.tv_nsec == 0);
    int truncated = open("timestamp-file", O_WRONLY | O_TRUNC);
    assert(truncated >= 0 && close(truncated) == 0);
    assert(fstat(fd, &value) == 0 && value.st_size == 0);
    assert(value.st_mtim.tv_nsec == 0 && value.st_ctim.tv_nsec == 0);
    struct timespec times[2] = {{.tv_sec = 1234, .tv_nsec = 123}, {.tv_sec = 5678, .tv_nsec = 456}};
    assert(futimens(fd, times) == 0);
    assert(fstat(fd, &value) == 0 && value.st_atim.tv_sec == 1234 && value.st_mtim.tv_sec == 5678);
    assert(value.st_atim.tv_nsec == 0 && value.st_mtim.tv_nsec == 0);
    times[0].tv_nsec = UTIME_OMIT; times[1].tv_sec = 6789;
    assert(utimensat(AT_FDCWD, "timestamp-file", times, 0) == 0);
    assert(fstat(fd, &value) == 0 && value.st_atim.tv_sec == 1234 && value.st_mtim.tv_sec == 6789);
    assert(syscall(SYS_utimensat, fd, NULL, (void *)1, 0) == -1 && errno == EFAULT);
    times[1].tv_nsec = 1000000000;
    assert(futimens(fd, times) == -1 && errno == EINVAL);
    times[1].tv_nsec = UTIME_NOW;
    assert(futimens(fd, times) == 0);
    assert(fstat(fd, &value) == 0 && value.st_atim.tv_sec == 1234 && value.st_mtim.tv_sec > 6789);
    struct timeval old[2] = {{.tv_sec = 111}, {.tv_sec = 222}};
    assert(syscall(SYS_futimesat, fd, NULL, old) == 0);
    assert(fstat(fd, &value) == 0 && value.st_atim.tv_sec == 111 && value.st_mtim.tv_sec == 222);
    assert(close(fd) == 0 && unlink("timestamp-file") == 0);
    fd = open("/tmp/m12-timestamp-nanos", O_CREAT | O_EXCL | O_RDWR, 0600);
    assert(fd >= 0 && write(fd, "T", 1) == 1);
    times[0] = (struct timespec){.tv_sec = 1234, .tv_nsec = 123};
    times[1] = (struct timespec){.tv_sec = 5678, .tv_nsec = 456};
    assert(futimens(fd, times) == 0 && fstat(fd, &value) == 0);
    assert(value.st_atim.tv_sec == 1234 && value.st_atim.tv_nsec == 123);
    assert(value.st_mtim.tv_sec == 5678 && value.st_mtim.tv_nsec == 456);
    assert(close(fd) == 0 && unlink("/tmp/m12-timestamp-nanos") == 0);
    puts("[M12:TIMES] real-atime-mtime NOW OMIT bad-pointer invalid-nsec");
}
static void shared_file_checks(void) {
    int fd = open("mapped-file", O_CREAT | O_RDWR | O_TRUNC, 0600);
    assert(fd >= 0 && write(fd, "original", 8) == 8);
    volatile char *a = mmap(NULL, 4096, PROT_READ, MAP_SHARED, fd, 0);
    volatile char *b = mmap(NULL, 4096, PROT_READ, MAP_SHARED, fd, 0);
    assert(a != MAP_FAILED && b != MAP_FAILED && a[0] == 'o' && b[0] == 'o');
    assert(pwrite(fd, "N", 1, 0) == 1 && a[0] == 'N' && b[0] == 'N');
    int readonly = fork(), readonly_status;
    assert(readonly >= 0);
    if (!readonly) { a[0] = 'X'; _exit(99); }
    assert(waitpid(readonly, &readonly_status, 0) == readonly && WIFSIGNALED(readonly_status));
    assert(WTERMSIG(readonly_status) == SIGSEGV && a[0] == 'N' && b[0] == 'N');
    assert(mprotect((void *)a, 4096, PROT_READ | PROT_WRITE) == -1 && errno == EACCES);
    assert(mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0) == MAP_FAILED && errno == ENOTSUP);
    assert(ftruncate(fd, 4) == -1 && errno == EBUSY);
    assert(ftruncate(fd, 16) == 0 && a[10] == 0 && pwrite(fd, "G", 1, 10) == 1 && a[10] == 'G');
    int child = fork(), status;
    assert(child >= 0);
    if (!child) { _exit(pwrite(fd, "C", 1, 0) != 1 || a[0] != 'C'); }
    assert(waitpid(child, &status, 0) == child && status == 0 && a[0] == 'C' && b[0] == 'C');
    assert(close(fd) == 0 && unlink("mapped-file") == 0 && a[0] == 'C');
    fd = open("mapped-file", O_CREAT | O_RDWR | O_TRUNC, 0600);
    assert(fd >= 0 && write(fd, "other", 5) == 5 && a[0] == 'C');
    assert(close(fd) == 0 && unlink("mapped-file") == 0);
    assert(munmap((void *)a, 4096) == 0 && munmap((void *)b, 4096) == 0);
    void *pressure[16];
    int count;
    for (count = 0; count != 16; ++count) {
        char name[32]; snprintf(name, sizeof name, "map-pressure-%d", count);
        fd = open(name, O_CREAT | O_RDWR | O_TRUNC, 0600);
        assert(fd >= 0 && write(fd, "P", 1) == 1);
        pressure[count] = mmap(NULL, 4096, PROT_READ, MAP_SHARED, fd, 0);
        int failure = errno;
        assert(close(fd) == 0 && unlink(name) == 0);
        if (pressure[count] == MAP_FAILED) { assert(failure == ENOMEM); break; }
    }
    assert(count > 0 && count < 16);
    for (int k = 0; k != count; ++k) {
        assert(*(volatile char *)pressure[k] == 'P' && munmap(pressure[k], 4096) == 0);
    }
    fd = open("map-reuse", O_CREAT | O_RDWR | O_TRUNC, 0600);
    assert(fd >= 0 && write(fd, "R", 1) == 1);
    a = mmap(NULL, 4096, PROT_READ, MAP_SHARED, fd, 0);
    assert(a != MAP_FAILED && a[0] == 'R');
    assert(munmap((void *)a, 4096) == 0 && close(fd) == 0 && unlink("map-reuse") == 0);
    puts("[M12:FILEMAP] coherent-readonly fork unlinked-inode real-OOM reuse");
}
static void select_checks(void) {
    int pair[2];
    assert(pipe(pair) == 0);
    fd_set readfds, writefds;
    FD_ZERO(&readfds); FD_ZERO(&writefds);
    FD_SET(pair[0], &readfds); FD_SET(pair[1], &writefds);
    struct timeval timeout = {0};
    assert(syscall(SYS_select, pair[1] + 1, &readfds, &writefds, NULL, &timeout) == 1);
    assert(!FD_ISSET(pair[0], &readfds) && FD_ISSET(pair[1], &writefds));
    FD_ZERO(&readfds); FD_SET(31, &readfds);
    assert(syscall(SYS_select, 32, &readfds, NULL, NULL, &timeout) == -1 && errno == EBADF);
    assert(FD_ISSET(31, &readfds));
    assert(syscall(SYS_select, 33, NULL, NULL, NULL, &timeout) == -1 && errno == EINVAL);
    assert(syscall(SYS_select, 1, (void *)1, NULL, NULL, &timeout) == -1 && errno == EFAULT);
    assert(syscall(SYS_select, 0, NULL, NULL, NULL, (void *)1) == -1 && errno == EFAULT);
    timeout.tv_usec = 1000000;
    assert(syscall(SYS_select, 0, NULL, NULL, NULL, &timeout) == -1 && errno == EINVAL);
    struct timespec before, after;
    assert(clock_gettime(CLOCK_MONOTONIC, &before) == 0);
    timeout = (struct timeval){.tv_usec = 20000};
    assert(syscall(SYS_select, 0, NULL, NULL, NULL, &timeout) == 0);
    assert(clock_gettime(CLOCK_MONOTONIC, &after) == 0);
    assert((after.tv_sec - before.tv_sec) * 1000000000LL + after.tv_nsec - before.tv_nsec >= 20000000);
    assert(timeout.tv_sec == 0 && timeout.tv_usec == 0);
    int child = fork(), status;
    assert(child >= 0);
    if (!child) { usleep(30000); _exit(write(pair[1], "S", 1) != 1); }
    FD_ZERO(&readfds); FD_SET(pair[0], &readfds); timeout = (struct timeval){.tv_sec = 2};
    assert(syscall(SYS_select, pair[0] + 1, &readfds, NULL, NULL, &timeout) == 1);
    char byte;
    assert(FD_ISSET(pair[0], &readfds) && read(pair[0], &byte, 1) == 1 && byte == 'S');
    assert(waitpid(child, &status, 0) == child && status == 0);
    struct sigaction action = {.sa_handler = select_handler, .sa_flags = SA_RESTART}, old;
    sigemptyset(&action.sa_mask);
    assert(sigaction(SIGUSR2, &action, &old) == 0);
    for (int poll_wait = 0; poll_wait != 2; ++poll_wait) {
        selected_signal = 0;
        child = fork(); assert(child >= 0);
        if (!child) { usleep(30000); _exit(kill(getppid(), SIGUSR2) != 0); }
        FD_ZERO(&readfds); FD_SET(pair[0], &readfds); timeout = (struct timeval){.tv_sec = 2};
        long result;
        if (poll_wait) {
            struct pollfd fd = {.fd = pair[0], .events = POLLIN};
            result = poll(&fd, 1, 2000);
        } else result = syscall(SYS_select, pair[0] + 1, &readfds, NULL, NULL, &timeout);
        assert(result == -1 && errno == EINTR && selected_signal);
        assert(FD_ISSET(pair[0], &readfds));
        assert(waitpid(child, &status, 0) == child && status == 0);
    }
    assert(sigaction(SIGUSR2, &old, NULL) == 0 && close(pair[1]) == 0);
    timeout = (struct timeval){0};
    assert(syscall(SYS_select, pair[0] + 1, &readfds, NULL, NULL, &timeout) == 1);
    assert(read(pair[0], &byte, 1) == 0 && close(pair[0]) == 0);
    puts("[M12:SELECT] readiness timeout bad-fd pointers EOF select-poll-EINTR");
}
static void *detached(void *unused) {
    (void)unused;
    atomic_store(&detached_tid, (int)syscall(SYS_gettid));
    while (!atomic_load(&detached_ready)) sched_yield();
    return NULL;
}
static void handler(int signal) {
    (void)signal;
    unsigned char local;
    stack_t current, disabled = {.ss_flags = SS_DISABLE};
    if ((uintptr_t)&local < (uintptr_t)alternate ||
        (uintptr_t)&local >= (uintptr_t)alternate + sizeof alternate ||
        sigaltstack(NULL, &current) || !(current.ss_flags & SS_ONSTACK)) _exit(81);
    errno = 0;
    if (sigaltstack(&disabled, NULL) != -1 || errno != EPERM) _exit(82);
    handled = 1;
}
int main(void) {
    assert(mkdir("remove", 0777) == 0);
    int remove = open("remove", O_RDONLY | O_DIRECTORY | O_NOFOLLOW);
    assert(remove >= 0);
    int member = openat(remove, "member", O_CREAT | O_RDWR, 0644);
    assert(member >= 0 && close(member) == 0);
    assert(rmdir("remove") == -1 && errno == ENOTEMPTY);
    assert(unlinkat(remove, "member", 0) == 0 && rmdir("remove") == 0);
    struct stat removed;
    assert(fstat(remove, &removed) == 0 && S_ISDIR(removed.st_mode));
    assert(openat(remove, "resurrect", O_CREAT | O_RDWR, 0644) == -1 && errno == ENOENT);
    assert(mkdir("remove", 0777) == 0 && close(remove) == 0);
    assert(chdir("remove") == 0 && rmdir("../remove") == -1 && errno == EBUSY);
    assert(chdir("..") == 0 && unlinkat(AT_FDCWD, "remove", AT_REMOVEDIR) == 0);
    for (int k = 0; k < 20; ++k) {
        assert(mkdir("remove", 0777) == 0 && rmdir("remove") == 0);
    }
    puts("[M12:DIRECTORY] nofollow dirfd unlink empty open-inode busy cwd reuse");
    assert(mkdir("names", 0777) == 0);
    char long_name[201], long_path[220];
    memset(long_name, 'n', 200); long_name[200] = 0;
    snprintf(long_path, sizeof long_path, "names/%s", long_name);
    int named = open(long_path, O_RDWR | O_CREAT | O_EXCL, 0644);
    assert(named >= 0 && write(named, "long", 4) == 4 && close(named) == 0);
    DIR *directory = opendir("names");
    assert(directory);
    int found = 0;
    struct dirent *entry;
    while ((entry = readdir(directory))) if (!strcmp(entry->d_name, long_name)) found = 1;
    assert(found && closedir(directory) == 0 && unlink(long_path) == 0 && rmdir("names") == 0);
    char too_long[514];
    memset(too_long, 'n', 256); too_long[256] = 0;
    assert(open(too_long, O_CREAT | O_RDWR, 0644) == -1 && errno == ENAMETOOLONG);
    memset(too_long, 'n', 513); too_long[513] = 0;
    assert(open(too_long, O_RDONLY) == -1 && errno == ENAMETOOLONG);
    puts("[M12:NAMES] long ext2 names getdents and rejected component/path bounds");
    int pair[2];
    assert(socketpair(AF_UNIX, SOCK_SEQPACKET | SOCK_NONBLOCK | SOCK_CLOEXEC, 0, pair) == 0);
    assert((fcntl(pair[0], F_GETFL) & O_NONBLOCK) && (fcntl(pair[1], F_GETFD) & FD_CLOEXEC));
    assert(send(pair[0], "alpha", 5, MSG_NOSIGNAL) == 5 && send(pair[0], "B", 1, 0) == 1);
    char packet[1023];
    assert(recv(pair[1], packet, 2, 0) == 2 && !memcmp(packet, "al", 2));
    assert(recv(pair[1], packet, sizeof packet, 0) == 1 && packet[0] == 'B');
    assert(recv(pair[1], packet, sizeof packet, 0) == -1 && errno == EAGAIN);
    assert(send(pair[1], "reverse", 7, MSG_NOSIGNAL) == 7);
    assert(syscall(SYS_recvfrom, pair[0], (void *)1, 7, 0, NULL, NULL) == -1 && errno == EFAULT);
    assert(recv(pair[0], packet, sizeof packet, 0) == 7 && !memcmp(packet, "reverse", 7));
    memset(packet, 'P', sizeof packet);
    assert(send(pair[0], packet, 1022, 0) == 1022);
    assert(send(pair[0], packet, 1, MSG_DONTWAIT) == -1 && errno == EAGAIN);
    assert(send(pair[0], packet, 1023, MSG_NOSIGNAL) == -1 && errno == EMSGSIZE);
    assert(recv(pair[1], packet, sizeof packet, 0) == 1022);
    assert(send(pair[0], packet, 0, 0) == 0);
    struct pollfd queued = {.fd = pair[1], .events = POLLIN};
    assert(poll(&queued, 1, 0) == 1 && (queued.revents & POLLIN));
    assert(recv(pair[1], packet, 0, 0) == 0);
    assert(recv(pair[1], packet, sizeof packet, 0) == -1 && errno == EAGAIN);
    assert(close(pair[1]) == 0 && recv(pair[0], packet, sizeof packet, 0) == 0);
    assert(send(pair[0], packet, 1, MSG_NOSIGNAL) == -1 && errno == EPIPE);
    assert(close(pair[0]) == 0);
    assert(syscall(SYS_socketpair, AF_UNIX, SOCK_SEQPACKET, 0, (void *)1) == -1 && errno == EFAULT);
    for (int k = 0; k < 20; ++k) {
        assert(socketpair(AF_UNIX, SOCK_SEQPACKET, 0, pair) == 0);
        assert(close(pair[0]) == 0 && close(pair[1]) == 0);
    }
    puts("[M12:PACKET] duplex boundaries truncation backpressure pointers EOF reuse");
    pid_t invalid = fork();
    assert(invalid >= 0);
    if (invalid == 0) {
        unsigned char *code = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
        assert(code != MAP_FAILED);
        const unsigned char unmap_self[] = {0xb8,11,0,0,0,0x0f,0x05,0xc3};
        memcpy(code, unmap_self, sizeof unmap_self);
        assert(mprotect(code, 4096, PROT_READ | PROT_EXEC) == 0);
        signal(SIGSEGV, SIG_IGN);
        sigset_t mask;
        sigemptyset(&mask); sigaddset(&mask, SIGSEGV);
        assert(sigprocmask(SIG_BLOCK, &mask, NULL) == 0);
        // Only this child's own anonymous code page is removed, immediately after its syscall.
        ((void (*)(void *, size_t))code)(code, 4096);
        _exit(83);
    }
    int invalid_status;
    assert(waitpid(invalid, &invalid_status, 0) == invalid);
    assert(WIFSIGNALED(invalid_status) && WTERMSIG(invalid_status) == SIGSEGV);
    puts("[M12:RETURN] ignored blocked SIGSEGV cannot stall invalid return");
    pthread_t worker;
    assert(pthread_create(&worker, NULL, detached, NULL) == 0);
    assert(pthread_detach(worker) == 0);
    while (!atomic_load(&detached_tid)) sched_yield();
    atomic_store(&detached_ready, 1);
    struct timespec start, now;
    assert(clock_gettime(CLOCK_MONOTONIC, &start) == 0);
    for (;;) {
        if (syscall(SYS_tgkill, getpid(), atomic_load(&detached_tid), 0) == -1) {
            assert(errno == ESRCH); break;
        }
        assert(clock_gettime(CLOCK_MONOTONIC, &now) == 0 && now.tv_sec - start.tv_sec < 3);
        sched_yield();
    }
    puts("[M12:DETACHED] self-unmap exit observed");
    stack_t stack = {.ss_sp = alternate, .ss_size = sizeof alternate}, old;
    assert(sigaltstack(NULL, &old) == 0 && (old.ss_flags & SS_DISABLE));
    stack_t small = {.ss_sp = alternate, .ss_size = 1};
    assert(sigaltstack(&small, NULL) == -1 && errno == ENOMEM);
    assert(syscall(SYS_sigaltstack, (void *)1, NULL) == -1 && errno == EFAULT);
    assert(sigaltstack(&stack, NULL) == 0);
    struct sigaction action = {.sa_handler = handler, .sa_flags = SA_ONSTACK};
    sigemptyset(&action.sa_mask);
    assert(sigaction(SIGUSR1, &action, NULL) == 0 && raise(SIGUSR1) == 0 && handled);
    assert(sigaltstack(NULL, &old) == 0 && old.ss_flags == 0 && old.ss_sp == alternate);
    int a = open("rename-a", O_RDWR | O_CREAT | O_TRUNC, 0600);
    int b = open("rename-b", O_RDWR | O_CREAT | O_TRUNC, S_IFREG | 0600);
    assert(b >= 0 && fchmod(b, S_IFREG | 0600) == 0);
    struct stat mode;
    assert(fstat(b, &mode) == 0 && (mode.st_mode & 0777) == 0600);
    assert(fchmod(b, 04755) == -1 && errno == ENOTSUP);
    assert(a >= 0 && b >= 0 && write(a, "new", 3) == 3 && write(b, "old", 3) == 3);
    assert(rename("rename-a", "rename-b") == 0 && access("rename-a", F_OK) == -1);
    char bytes[4] = {0};
    assert(lseek(b, 0, SEEK_SET) == 0 && read(b, bytes, 3) == 3 && !strcmp(bytes, "old"));
    assert(pread(b, bytes, 3, 0) == 3 && !strcmp(bytes, "old") && lseek(b, 0, SEEK_CUR) == 3);
    assert(pwrite(b, "X", 1, 1) == 1 && lseek(b, 0, SEEK_CUR) == 3);
    assert(pread(b, bytes, 3, 0) == 3 && !memcmp(bytes, "oXd", 3));
    assert(pread(b, bytes, 3, 9999) == 0 && lseek(b, 0, SEEK_CUR) == 3);
    assert(ftruncate(b, 2) == 0 && lseek(b, 0, SEEK_CUR) == 3);
    assert(ftruncate(b, 6) == 0 && lseek(b, 0, SEEK_CUR) == 3);
    char grown[6];
    assert(pread(b, grown, 6, 0) == 6 && !memcmp(grown, "oX\0\0\0\0", 6));
    assert(ftruncate(b, 1048577) == 0 && lseek(b, 0, SEEK_CUR) == 3);
    assert(ftruncate(b, 6) == 0 && lseek(b, 0, SEEK_CUR) == 3);
    assert(ftruncate(b, 4194305) == -1 && errno == EFBIG);
    assert(pread(b, grown, 6, 0) == 6 && !memcmp(grown, "oX\0\0\0\0", 6));
    int temporary = open("/tmp/m12-file-bound", O_CREAT | O_EXCL | O_RDWR, 0600);
    assert(temporary >= 0 && write(temporary, "tmp", 3) == 3);
    assert(ftruncate(temporary, 1048577) == -1 && errno == EFBIG);
    assert(fstat(temporary, &mode) == 0 && mode.st_size == 3);
    assert(pread(temporary, bytes, 3, 0) == 3 && !memcmp(bytes, "tmp", 3));
    assert(lseek(temporary, 0, SEEK_CUR) == 3);
    assert(close(temporary) == 0 && unlink("/tmp/m12-file-bound") == 0);
    int c = open("rename-b", O_RDONLY);
    assert(ftruncate(c, 0) == -1 && errno == EBADF);
    assert(syscall(SYS_pread64, c, (void *)1, 1, 0) == -1 && errno == EFAULT);
    assert(lseek(c, 0, SEEK_CUR) == 0);
    assert(c >= 0 && flock(a, LOCK_EX | LOCK_NB) == 0);
    assert(flock(c, LOCK_EX | LOCK_NB) == -1 && errno == EAGAIN);
    int copy = dup(a);
    assert(copy >= 0 && close(a) == 0 && flock(c, LOCK_SH | LOCK_NB) == -1 && errno == EAGAIN);
    assert(close(copy) == 0 && flock(c, LOCK_EX | LOCK_NB) == 0);
    assert(close(c) == 0 && close(b) == 0);
    assert(unlink("rename-b") == 0);
    stack.ss_flags = SS_DISABLE;
    assert(sigaltstack(&stack, NULL) == 0);
    select_checks();
    shared_file_checks();
    timestamp_checks();
    puts("[M12:RUNTIME] altstack delivery return negatives rename open-inode locks");
    return 0;
}
