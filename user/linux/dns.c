/* SPDX-License-Identifier: Apache-2.0 */
/* Resolve DNS A records with checked compression, transaction/source validation and finite
 * deadlines. */
#define _GNU_SOURCE
#include "dns.h"
#include <arpa/inet.h>
#include <errno.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/random.h>
#include <sys/socket.h>
#include <unistd.h>
static unsigned word(const unsigned char *b) { return (unsigned)b[0] * 256 + b[1]; }
static int skip_name(const unsigned char *b, size_t n, size_t *at) {
    size_t cursor = *at, after = 0;
    unsigned steps = 0, total = 0;
    while (cursor < n && steps++ < 128) {
        unsigned size = b[cursor++];
        if (!size) {
            *at = after ? after : cursor;
            return 0;
        }
        if ((size & 0xc0) == 0xc0) {
            if (cursor >= n)
                return -1;
            size_t next = ((size & 63u) << 8) | b[cursor++];
            if (next >= n)
                return -1;
            if (!after)
                after = cursor;
            cursor = next;
        } else if (size > 63 || size > n - cursor || (total += size + 1) > 254) {
            return -1;
        } else {
            cursor += size;
        }
    }
    return -1;
}
int dns_answer(const unsigned char *b, size_t n, uint16_t transaction, unsigned char ip[4]) {
    if (n < 12 || word(b) != transaction || (word(b + 2) & 0xfa0f) != 0x8000 || word(b + 4) != 1 ||
        word(b + 6) > 32)
        return -1;
    size_t at = 12;
    if (skip_name(b, n, &at) || n - at < 4 || word(b + at) != 1 || word(b + at + 2) != 1)
        return -1;
    at += 4;
    for (unsigned k = 0; k < word(b + 6); k++) {
        if (skip_name(b, n, &at) || n - at < 10)
            return -1;
        unsigned type = word(b + at), klass = word(b + at + 2), size = word(b + at + 8);
        at += 10;
        if (size > n - at)
            return -1;
        if (type == 1 && klass == 1 && size == 4) {
            memcpy(ip, b + at, 4);
            return 0;
        }
        at += size;
    }
    return -1;
}
int dns_resolve(const char *name, const char *server, unsigned port, unsigned char ip[4]) {
    unsigned char query[512] = {0}, response[512];
    uint16_t transaction;
    if (getrandom(&transaction, sizeof transaction, 0) != sizeof transaction)
        return -1;
    query[0] = (unsigned char)(transaction >> 8);
    query[1] = (unsigned char)transaction;
    query[2] = 1;
    query[5] = 1;
    size_t at = 12;
    const char *label = name;
    if (!*name || strlen(name) > 253 || port == 0 || port > 65535)
        return -1;
    while (*label) {
        const char *dot = strchr(label, '.');
        size_t n = dot ? (size_t)(dot - label) : strlen(label);
        if (!n || n > 63 || at + 1 + n + 5 > sizeof query)
            return -1;
        query[at++] = (unsigned char)n;
        memcpy(query + at, label, n);
        at += n;
        if (!dot)
            break;
        label = dot + 1;
        if (!*label)
            return -1;
    }
    query[at++] = 0;
    query[at++] = 0;
    query[at++] = 1;
    query[at++] = 0;
    query[at++] = 1;
    int fd = socket(AF_INET, SOCK_DGRAM | SOCK_NONBLOCK, 0);
    if (fd < 0)
        return -1;
    struct sockaddr_in peer = {.sin_family = AF_INET, .sin_port = htons((uint16_t)port)};
    int result = -1;
    if (inet_pton(AF_INET, server, &peer.sin_addr) != 1)
        goto done;
    if (sendto(fd, query, at, 0, (void *)&peer, sizeof peer) != (ssize_t)at)
        goto done;
    struct pollfd p = {.fd = fd, .events = POLLIN};
    if (poll(&p, 1, 2000) != 1)
        goto done;
    struct sockaddr_in sender;
    socklen_t len = sizeof sender;
    ssize_t n = recvfrom(fd, response, sizeof response, 0, (void *)&sender, &len);
    if (n < 0 || len != sizeof sender || sender.sin_port != peer.sin_port ||
        sender.sin_addr.s_addr != peer.sin_addr.s_addr)
        goto done;
    /* The answer must repeat the exact question; this rejects unrelated responses sharing an ID. */
    if ((size_t)n < at || memcmp(response + 12, query + 12, at - 12))
        goto done;
    result = dns_answer(response, (size_t)n, transaction, ip);
done:
    close(fd);
    return result;
}
#ifdef ANOVA_DNS_COMMAND
int main(int argc, char **argv) {
    unsigned char ip[4];
    if (argc < 2 || argc > 4)
        return 2;
    if (dns_resolve(argv[1], argc >= 3 ? argv[2] : "10.0.2.3",
                    argc == 4 ? (unsigned)strtoul(argv[3], 0, 10) : 53, ip)) {
        puts("dns: lookup failed or timed out");
        return 1;
    }
    printf("%s %u.%u.%u.%u\n", argv[1], ip[0], ip[1], ip[2], ip[3]);
    return 0;
}
#endif
