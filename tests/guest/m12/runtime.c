// SPDX-License-Identifier: Apache-2.0
// Verify bounded names/directories/packets, return faults, signals, detached exit and file ownership.
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
#include <sys/stat.h>
#include <poll.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static unsigned char alternate[32768];
static volatile sig_atomic_t handled;
static atomic_int detached_tid, detached_ready;
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
    assert(ftruncate(b, 1048577) == -1 && errno == EFBIG);
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
    puts("[M12:RUNTIME] altstack delivery return negatives rename open-inode locks");
    return 0;
}
