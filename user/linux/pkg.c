/* SPDX-License-Identifier: Apache-2.0 */
/* Install bounded, hash-locked native packages into alternating persistent slots with checked
 * commits. */
#define _GNU_SOURCE
#include "tls.h"
#include <errno.h>
#include <fcntl.h>
#include <mbedtls/sha256.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#define LIMIT (128 * 1024)
#define HEADER 128u
#define ENTRY 96u
#define FILES 4u
static unsigned char archive[LIMIT];
static const char *root = "/persist/pkg";
static uint32_t word(const unsigned char *b) {
    return (uint32_t)b[0] | (uint32_t)b[1] << 8 | (uint32_t)b[2] << 16 | (uint32_t)b[3] << 24;
}
static void put(unsigned char *b, uint32_t x) {
    for (unsigned k = 0; k < 4; k++)
        b[k] = (unsigned char)(x >> (k * 8));
}
static void digest(const void *b, size_t n, unsigned char out[32]) {
    if (mbedtls_sha256_ret(b, n, out, 0))
        abort();
}
static int hex(const char *s, unsigned char out[32]) {
    if (strlen(s) != 64)
        return -1;
    for (unsigned k = 0; k < 32; k++) {
        char pair[3] = {s[k * 2], s[k * 2 + 1], 0}, *end;
        unsigned long v = strtoul(pair, &end, 16);
        if (*end || v > 255)
            return -1;
        out[k] = (unsigned char)v;
    }
    return 0;
}
static int safe_name(const char *s, size_t limit) {
    size_t n = strnlen(s, limit);
    if (n == 0 || n == limit || !strcmp(s, ".") || strstr(s, ".."))
        return 0;
    for (size_t k = 0; k < n; k++)
        if (!((s[k] >= 'a' && s[k] <= 'z') || (s[k] >= 'A' && s[k] <= 'Z') ||
              (s[k] >= '0' && s[k] <= '9') || s[k] == '-' || s[k] == '_' || s[k] == '.'))
            return 0;
    return 1;
}
static int read_file(const char *path, unsigned char *out, size_t capacity, size_t *size) {
    int fd = open(path, O_RDONLY);
    if (fd < 0)
        return -1;
    size_t n = 0;
    for (;;) {
        unsigned char extra;
        ssize_t k = read(fd, n < capacity ? out + n : &extra, n < capacity ? capacity - n : 1);
        if (k < 0) {
            close(fd);
            return -1;
        }
        if (!k)
            break;
        if (n == capacity) {
            close(fd);
            return -1;
        }
        n += (size_t)k;
    }
    close(fd);
    *size = n;
    return 0;
}
static int write_file(const char *path, const unsigned char *b, size_t n) {
    int fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0)
        return -1;
    size_t at = 0;
    while (at < n) {
        ssize_t k = write(fd, b + at, n - at);
        if (k <= 0) {
            close(fd);
            return -1;
        }
        at += (size_t)k;
    }
    int result = fsync(fd);
    if (close(fd))
        result = -1;
    return result;
}
static int directory(const char *path) { return mkdir(path, 0755) && errno != EEXIST ? -1 : 0; }
static void slot_path(char out[128], const char *name, unsigned slot, const char *leaf) {
    snprintf(out, 128, "%s/%.23s-%c/%.16s", root, name, slot ? 'b' : 'a', leaf);
}
struct state {
    uint32_t generation, version, present, slot;
    unsigned char package[32];
};
static int load_state(const char *name, unsigned slot, struct state *s) {
    char path[128];
    slot_path(path, name, slot, "state");
    unsigned char b[88], hash[32];
    size_t n;
    if (read_file(path, b, sizeof b, &n) || n != sizeof b || memcmp(b, "APSTATE1", 8))
        return -1;
    digest(b, 56, hash);
    if (memcmp(hash, b + 56, 32) || word(b + 20) != slot || word(b + 16) > 1)
        return -1;
    s->generation = word(b + 8);
    s->version = word(b + 12);
    s->present = word(b + 16);
    s->slot = slot;
    memcpy(s->package, b + 24, 32);
    return 0;
}
static int current(const char *name, struct state *out) {
    struct state a, b;
    int has_a = load_state(name, 0, &a) == 0, has_b = load_state(name, 1, &b) == 0;
    if (!has_a && !has_b) {
        memset(out, 0, sizeof *out);
        out->slot = 1;
        return 0;
    }
    if (has_a && has_b && a.generation == b.generation)
        return -1;
    *out = has_a && (!has_b || a.generation > b.generation) ? a : b;
    return 0;
}
static int commit(const char *name, const struct state *s) {
    unsigned char b[88] = {0};
    memcpy(b, "APSTATE1", 8);
    put(b + 8, s->generation);
    put(b + 12, s->version);
    put(b + 16, s->present);
    put(b + 20, s->slot);
    memcpy(b + 24, s->package, 32);
    digest(b, 56, b + 56);
    char path[128];
    slot_path(path, name, s->slot, "state");
    return write_file(path, b, sizeof b);
}
static int verify(size_t n, const unsigned char expected[32]) {
    unsigned char hash[32];
    digest(archive, n, hash);
    if (memcmp(hash, expected, 32) || n < HEADER || memcmp(archive, "ANOVAPK1", 8))
        return -1;
    if (!safe_name((char *)archive + 8, 24) || memcmp(archive + 48, "x86_64\0", 7) ||
        memcmp(archive + 64, "anova.native.v1\0", 16) || memcmp(archive + 96, "Apache-2.0\0", 11))
        return -1;
    unsigned count = word(archive + 44);
    if (count == 0 || count > FILES || word(archive + 40) != 1 || word(archive + 112) != 1 ||
        n < HEADER + count * ENTRY)
        return -1;
    size_t at = HEADER + count * ENTRY;
    int license = 0;
    for (unsigned k = 0; k < count; k++) {
        unsigned char *entry = archive + HEADER + k * ENTRY;
        unsigned mode = word(entry + 48), size = word(entry + 52);
        if (!safe_name((char *)entry, 48) || size > n - at || (mode != 0644 && mode != 0755))
            return -1;
        for (unsigned j = 0; j < k; j++)
            if (!strcmp((char *)entry, (char *)archive + HEADER + j * ENTRY))
                return -1;
        digest(archive + at, size, hash);
        if (memcmp(hash, entry + 56, 32))
            return -1;
        if (!strcmp((char *)entry, "LICENSE") && mode == 0644)
            license = 1;
        at += size;
    }
    return at == n && license ? 0 : -1;
}
static void cleanup(const char *name, unsigned slot) {
    char path[128];
    for (unsigned k = 0; k < FILES; k++) {
        char leaf[16];
        snprintf(leaf, sizeof leaf, "file%u", k);
        slot_path(path, name, slot, leaf);
        unlink(path);
    }
    slot_path(path, name, slot, "manifest");
    unlink(path);
    slot_path(path, name, slot, "state");
    unlink(path);
}
static int install(size_t n, const unsigned char expected[32], int fail_after) {
    if (verify(n, expected)) {
        puts("[PKG:REJECT] checksum, ABI, dependency, path or license");
        return -1;
    }
    const char *name = (char *)archive + 8;
    struct state old;
    if (current(name, &old) || old.generation == UINT32_MAX)
        return -1;
    unsigned version = word(archive + 36);
    if (!version || (old.present && version <= old.version)) {
        puts("[PKG:REJECT] version must increase");
        return -1;
    }
    if (directory(root) || directory("/persist/data")) {
        return -1;
    }
    char path[128];
    snprintf(path, sizeof path, "/persist/data/%.23s", name);
    if (directory(path))
        return -1;
    unsigned slot = 1 - old.slot;
    snprintf(path, sizeof path, "%s/%.23s-%c", root, name, slot ? 'b' : 'a');
    if (directory(path))
        return -1;
    cleanup(name, slot);
    unsigned count = word(archive + 44);
    size_t at = HEADER + count * ENTRY;
    for (unsigned k = 0; k < count; k++) {
        unsigned char *entry = archive + HEADER + k * ENTRY;
        unsigned size = word(entry + 52);
        char leaf[16];
        snprintf(leaf, sizeof leaf, "file%u", k);
        slot_path(path, name, slot, leaf);
        if (write_file(path, archive + at, size) || (word(entry + 48) == 0755 && chmod(path, 0755)))
            goto failed;
        at += size;
        if (fail_after == (int)k)
            goto failed;
    }
    slot_path(path, name, slot, "manifest");
    if (write_file(path, archive, HEADER + count * ENTRY))
        goto failed;
    struct state next = {
        .generation = old.generation + 1, .version = version, .present = 1, .slot = slot};
    memcpy(next.package, expected, 32);
    if (commit(name, &next))
        goto failed;
    printf("[PKG:INSTALLED] name=%s version=%u slot=%c abi=anova.native.v1 license=Apache-2.0\n",
           name, version, slot ? 'b' : 'a');
    return 0;
failed:
    cleanup(name, slot);
    printf("[PKG:ROLLBACK] name=%s retained=%u\n", name, old.present ? old.version : 0);
    return -1;
}
static int locked(const char *name, unsigned version, char path[128], unsigned char hash[32]) {
    if (!safe_name(name, 24))
        return -1;
    snprintf(path, 128, "/repo/%s-%u.apkg", name, version);
    char index[4096];
    size_t n;
    if (read_file("/repo/INDEX", (unsigned char *)index, sizeof(index) - 1, &n))
        return -1;
    index[n] = 0;
    char wanted[64];
    snprintf(wanted, sizeof wanted, "%s-%u.apkg ", name, version);
    for (char *line = strtok(index, "\n"); line; line = strtok(0, "\n"))
        if (!strncmp(line, wanted, strlen(wanted)))
            return hex(line + strlen(wanted), hash);
    return -1;
}
static int status(const char *name) {
    struct state s;
    if (!safe_name(name, 24) || current(name, &s))
        return 1;
    printf("[PKG:STATUS] name=%s installed=%u version=%u slot=%c data=/persist/data/%s\n", name,
           s.present, s.present ? s.version : 0, s.slot ? 'b' : 'a', name);
    return 0;
}
int main(int argc, char **argv) {
    if (argc == 4 && !strcmp(argv[1], "check")) {
        unsigned char expected[32];
        size_t size;
        if (hex(argv[3], expected) || read_file(argv[2], archive, sizeof archive, &size) ||
            verify(size, expected))
            return 1;
        puts("[PKG:CHECKED] integrity=true ABI=true paths=true dependency=true license=true");
        return 0;
    }

    if (argc >= 3 && !strcmp(argv[1], "status"))
        return status(argv[2]);
    if (argc == 3 && !strcmp(argv[1], "remove")) {
        const char *name = argv[2];
        struct state old;
        if (!safe_name(name, 24) || current(name, &old) || !old.present ||
            old.generation == UINT32_MAX)
            return 1;
        struct state next = old;
        next.slot = 1 - old.slot;
        next.generation++;
        next.present = 0;
        char path[128];
        snprintf(path, sizeof path, "%s/%.23s-%c", root, name, next.slot ? 'b' : 'a');
        if (directory(path))
            return 1;
        cleanup(name, next.slot);
        if (commit(name, &next))
            return 1;
        cleanup(name, old.slot);
        printf("[PKG:REMOVED] name=%s user_data=preserved\n", name);
        return 0;
    }
    unsigned char expected[32];
    size_t n = 0;
    char path[128];
    int fail_after = -1;
    if (argc >= 4 && (!strcmp(argv[1], "install") || !strcmp(argv[1], "fetch") ||
                      !strcmp(argv[1], "fail-test"))) {
        unsigned version = (unsigned)strtoul(argv[3], 0, 10);
        if (locked(argv[2], version, path, expected))
            return 1;
        if (!strcmp(argv[1], "fetch")) {
            if (argc != 5 ||
                https_get("repo.anova", (unsigned)strtoul(argv[4], 0, 10), "/etc/pkg-ca.pem",
                          strrchr(path, '/'), archive, sizeof archive, &n))
                return 1;
        } else if (read_file(path, archive, sizeof archive, &n))
            return 1;
        if (!strcmp(argv[1], "fail-test"))
            fail_after = 0;
    } else if (argc == 4 && !strcmp(argv[1], "install-file")) {
        if (hex(argv[3], expected) || read_file(argv[2], archive, sizeof archive, &n))
            return 1;
    } else {
        puts("Usage: pkg install NAME VERSION | fetch NAME VERSION PORT | remove NAME | status "
             "NAME | install-file PATH SHA256");
        return 2;
    }
    return install(n, expected, fail_after) ? 1 : 0;
}
