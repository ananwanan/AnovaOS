/* SPDX-License-Identifier: Apache-2.0 */
/* Exercise actual IPv4 TCP/UDP/DNS, readiness, fd inheritance and hostile user-buffer boundaries.
 */
#define _GNU_SOURCE
#include "dns.h"
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/syscall.h>
#include <sys/time.h>
#include <sys/wait.h>
#include <unistd.h>
#define CHECK(x)                                                                                   \
    do {                                                                                           \
        if (!(x)) {                                                                                \
            printf("[M10:FAIL] line=%d errno=%d\n", __LINE__, errno);                              \
            return 1;                                                                              \
        }                                                                                          \
    } while (0)
static struct sockaddr_in endpoint(unsigned port) {
    struct sockaddr_in a = {.sin_family = AF_INET, .sin_port = htons(port)};
    inet_pton(AF_INET, "10.0.2.2", &a.sin_addr);
    return a;
}
static int waitfd(int fd, int events, int ms) {
    struct pollfd p = {.fd = fd, .events = events};
    return poll(&p, 1, ms) > 0 && (p.revents & (events | POLLERR | POLLHUP));
}
static volatile sig_atomic_t caught;
static void handler(int n) { caught = n; }
int main(int argc, char **argv) {
    CHECK(socket(AF_UNIX, SOCK_STREAM, 0) == -1 && errno == EAFNOSUPPORT);
    CHECK(syscall(SYS_socketpair, AF_UNIX, SOCK_STREAM, 0, 0) == -1 && errno == ESOCKTNOSUPPORT);
    unsigned base = argc > 1 ? (unsigned)strtoul(argv[1], 0, 10) : 18080;
    int fd = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK | SOCK_CLOEXEC, 0);
    CHECK(fd >= 0);
    CHECK(fcntl(fd, F_GETFD) == FD_CLOEXEC);
    struct sockaddr_in a = endpoint(base);
    CHECK(connect(fd, (void *)&a, sizeof a) == -1 && errno == EINPROGRESS);
    CHECK(waitfd(fd, POLLOUT, 5000));
    int error = 123;
    socklen_t size = sizeof error;
    CHECK(getsockopt(fd, SOL_SOCKET, SO_ERROR, &error, &size) == 0 && error == 0);
    int alias = dup(fd);
    CHECK(alias >= 0);
    CHECK(close(fd) == 0);
    fd = alias;
    CHECK(send(fd, "M10-tcp", 7, MSG_NOSIGNAL) == 7);
    CHECK(waitfd(fd, POLLIN, 3000));
    char b[8192];
    CHECK(recv(fd, b, sizeof b, 0) == 7 && !memcmp(b, "M10-tcp", 7));
    size = sizeof a;
    CHECK(getpeername(fd, (void *)&a, &size) == 0 && ntohs(a.sin_port) == base);
    CHECK(recv(fd, b, sizeof b, MSG_DONTWAIT) == -1 && errno == EAGAIN);
    CHECK(fcntl(fd, F_SETFL, 0) == 0);
    struct timeval timeout = {.tv_usec = 100000};
    CHECK(setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof timeout) == 0);
    CHECK(recv(fd, b, sizeof b, 0) == -1 && errno == EAGAIN);
    CHECK(shutdown(fd, SHUT_WR) == 0);
    CHECK(waitfd(fd, POLLIN, 3000));
    CHECK(read(fd, b, sizeof b) == 0);
    CHECK(close(fd) == 0);
    puts("[M10:TCP] echo=true nonblocking=true poll=true dup=true timeout=true eof=true");
    fd = socket(AF_INET, SOCK_DGRAM | SOCK_NONBLOCK, 0);
    CHECK(fd >= 0);
    a = endpoint(base + 1);
    CHECK(sendto(fd, "M10-udp", 7, 0, (void *)&a, sizeof a) == 7);
    CHECK(waitfd(fd, POLLIN, 3000));
    CHECK(syscall(SYS_recvfrom, fd, (void *)1, 7, 0, 0, 0) == -1 && errno == EFAULT);
    size = sizeof a;
    CHECK(recvfrom(fd, b, sizeof b, 0, (void *)&a, &size) == 7 && !memcmp(b, "M10-udp", 7));
    CHECK(ntohs(a.sin_port) == base + 1);
    CHECK(sendto(fd, b, 1473, 0, (void *)&a, sizeof a) == -1 && errno == EMSGSIZE);
    CHECK(close(fd) == 0);
    puts("[M10:UDP] echo=true source=true EFAULT_retains_datagram=true MTU=true");
    fd = socket(AF_INET, SOCK_DGRAM, 0);
    CHECK(fd >= 0);
    a = endpoint(base + 2);
    unsigned char query[] = {0x6d, 0x10, 1,   0, 0,   1,   0,   0,   0,   0, 0, 0, 4, 'r',
                             'e',  'p',  'o', 5, 'a', 'n', 'o', 'v', 'a', 0, 0, 1, 0, 1};
    CHECK(sendto(fd, query, sizeof query, 0, (void *)&a, sizeof a) == sizeof query);
    CHECK(waitfd(fd, POLLIN, 3000));
    ssize_t n = recv(fd, b, sizeof b, 0);
    CHECK(n == (ssize_t)sizeof(query) + 16);
    CHECK(!memcmp(b, query, 2) && ((unsigned char)b[2] & 0x80) && b[7] == 1);
    CHECK(!memcmp(b + n - 4, "\x0a\x00\x02\x02", 4));
    CHECK(close(fd) == 0);
    unsigned char ip[4];
    CHECK(dns_resolve("repo.anova", "10.0.2.2", base + 2, ip) == 0 &&
          !memcmp(ip, "\x0a\x00\x02\x02", 4));
    CHECK(dns_answer((unsigned char *)b, (size_t)n, 0x6d11, ip) == -1);
    b[12] = (char)0xc0;
    b[13] = 12;
    CHECK(dns_answer((unsigned char *)b, (size_t)n, 0x6d10, ip) == -1);
    CHECK(dns_resolve("repo.anova", "10.0.2.2", base + 6, ip) == -1);
    puts("[M10:DNS] query=true controlled_A=true transaction_id=true compression_cycle=reject "
         "timeout=true");
    fd = socket(AF_INET, SOCK_DGRAM, 0);
    CHECK(fd >= 0);
    a = endpoint(base + 1);
    CHECK(connect(fd, (void *)&a, sizeof a) == 0);
    struct sigaction sa = {.sa_handler = handler};
    sigemptyset(&sa.sa_mask);
    CHECK(sigaction(SIGUSR1, &sa, 0) == 0);
    pid_t parent = getpid(), child = fork();
    CHECK(child >= 0);
    if (child == 0) {
        usleep(30000);
        kill(parent, SIGUSR1);
        _exit(0);
    }
    CHECK(read(fd, b, 1) == -1 && errno == EINTR && caught == SIGUSR1);
    int status;
    CHECK(waitpid(child, &status, 0) == child && status == 0);
    CHECK(close(fd) == 0);
    puts("[M10:SIGNAL] blocked_socket=true EINTR=true fork_close=true");
    int sockets[8];
    for (int k = 0; k < 8; k++) {
        sockets[k] = socket(AF_INET, SOCK_DGRAM, 0);
        CHECK(sockets[k] >= 0);
    }
    CHECK(socket(AF_INET, SOCK_DGRAM, 0) == -1 && errno == EMFILE);
    for (int k = 0; k < 8; k++)
        CHECK(close(sockets[k]) == 0);
    for (int k = 0; k < 32; k++) {
        fd = socket(AF_INET, SOCK_DGRAM, 0);
        CHECK(fd >= 0);
        CHECK(close(fd) == 0);
    }
    puts("[M10:SOCKETS] bounded=true reuse=32 cleanup=true");
    fd = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK, 0);
    CHECK(fd >= 0);
    a = endpoint(base + 4);
    CHECK(connect(fd, (void *)&a, sizeof a) == -1 && errno == EINPROGRESS);
    CHECK(waitfd(fd, POLLOUT, 5000));
    size = sizeof error;
    CHECK(getsockopt(fd, SOL_SOCKET, SO_ERROR, &error, &size) == 0 && error == ECONNREFUSED);
    CHECK(close(fd) == 0);
    puts("[M10:PASS] network=true refusal=true");
    return 0;
}
