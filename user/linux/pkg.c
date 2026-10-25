/* SPDX-License-Identifier: Apache-2.0 */
/* Validate versioned native/Linux application manifests before persistent writes, then install
 * checksum-locked packages into alternating slots under the explicit disk ABI policy. */
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
#define HEADER_V2 256u
#ifndef ANOVA_PKG_PERSIST
#define ANOVA_PKG_PERSIST "/persist"
#endif
#define ENTRY 96u
#define FILES 4u
static unsigned char archive[LIMIT];
static const char *root = ANOVA_PKG_PERSIST "/pkg";
static size_t header_size;
static int manifest_v2;
static int manifest_linux;
static uint32_t word(const unsigned char *b) {
    return (uint32_t)b[0] | (uint32_t)b[1] << 8 | (uint32_t)b[2] << 16 | (uint32_t)b[3] << 24;
}
static uint64_t wide(const unsigned char *b) {
    return (uint64_t)word(b) | (uint64_t)word(b + 4) << 32;
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
        unsigned value = 0;
        for (unsigned j = 0; j < 2; j++) {
            unsigned char c = (unsigned char)s[k * 2 + j];
            if (c >= '0' && c <= '9')
                value = value * 16 + c - '0';
            else if (c >= 'a' && c <= 'f')
                value = value * 16 + c - 'a' + 10;
            else
                return -1;
        }
        out[k] = (unsigned char)value;
    }
    return 0;
}
static int zero(const unsigned char *b, size_t n) {
    for (size_t k = 0; k < n; k++)
        if (b[k])
            return 0;
    return 1;
}
static int field(const unsigned char *b, size_t n, const char *s) {
    size_t len = strlen(s);
    return len < n && !memcmp(b, s, len) && zero(b + len, n - len);
}
static int number(const char *s, unsigned *out) {
    unsigned value = 0;
    if (!*s)
        return -1;
    for (; *s; s++) {
        unsigned digit = (unsigned char)*s - '0';
        if (digit > 9 || value > (UINT32_MAX - digit) / 10)
            return -1;
        value = value * 10 + digit;
    }
    *out = value;
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
/* The SDK appends this versioned, hash-covered declaration after validating the ELF.
 * It binds the manifest to the selected build target without guessing from ELF OSABI. */
static int executable(const unsigned char *data, size_t n) {
    if (n < 128 || memcmp(data, "\177ELF\2\1\1", 7) || data[18] != 62 || data[19] ||
        data[16] != 2 || data[17] ||
        word(data + 20) != 1 || word(data + 48) || data[52] != 64 || data[53] ||
        data[54] != 56 || data[55])
        return -1;
    const unsigned char *tag = data + n - 64;
    if (memcmp(tag, "ANOVAABI", 8) || memcmp(tag + 8, archive + 64, 32) ||
        !field(tag + 40, 16, "x86_64") || word(tag + 56) != 1 || !zero(tag + 60, 4))
        return -1;
    uint64_t offset = wide(data + 32), entry = wide(data + 24);
    unsigned count = data[56] | (unsigned)data[57] << 8;
    if (offset < 64 || offset > n - 64 || !count || count > 32 ||
        count * 56u > n - 64 - offset)
        return -1;
    uint64_t starts[16], ends[16], addresses[16], offsets[16], file_sizes[16], mem_sizes[16];
    uint64_t memory = 0, tls_file = 0, tls_address = 0, tls_filesz = 0, tls_memsz = 0;
    unsigned tls_count = 0;
    unsigned loads = 0;
    int entry_found = 0, phdr_found = 0;
    for (unsigned k = 0; k < count; k++) {
        const unsigned char *p = data + offset + k * 56u;
        unsigned kind = word(p), flags = word(p + 4);
        uint64_t file = wide(p + 8), address = wide(p + 16), filesz = wide(p + 32),
                 memsz = wide(p + 40), align = wide(p + 48);
        if (file > n - 64 || filesz > n - 64 - file)
            return -1;
        if (kind == 7 && manifest_linux) {
            if (tls_count++ || filesz > memsz || (align > 1 && (align & (align - 1))))
                return -1;
            tls_file = file;
            tls_address = address;
            tls_filesz = filesz;
            tls_memsz = memsz;
            continue;
        }
        if (kind != 1) {
            if (kind == 0 || kind == 4 || kind == 6 ||
                (kind == 0x6474e551 && !(flags & 1)) ||
                (manifest_linux && (kind == 0x6474e550 || kind == 0x6474e552 ||
                                    kind == 0x6474e553)))
                continue;
            return -1;
        }
        if (filesz > memsz || flags & ~7u || !(flags & 4) || (flags & 3) == 3 ||
            (align > 1 && ((align & (align - 1)) || address % align != file % align)) ||
            address % 4096 != file % 4096)
            return -1;
        if (!memsz)
            continue;
        if (address > 0x40000000u || memsz > 0x40000000u - address ||
            address < 0x10000u || loads == 16)
            return -1;
        uint64_t start = address & ~4095ull, end = (address + memsz + 4095) & ~4095ull;
        if (loads && starts[loads - 1] > start)
            return -1;
        for (unsigned j = 0; j < loads; j++)
            if (start < ends[j] && starts[j] < end)
                return -1;
        memory += end - start;
        if (memory > (manifest_linux ? 256u * 1024 * 1024 : 4u * 1024 * 1024))
            return -1;
        starts[loads] = start;
        ends[loads] = end;
        addresses[loads] = address;
        offsets[loads] = file;
        file_sizes[loads] = filesz;
        mem_sizes[loads++] = memsz;
        if ((flags & 1) && entry >= address && entry - address < filesz)
            entry_found = 1;
        if (file <= offset && offset + count * 56u - file <= filesz)
            phdr_found = 1;
    }
    if (tls_count) {
        int contained = 0;
        for (unsigned k = 0; k < loads; k++) {
            if (tls_address >= addresses[k] && tls_address - addresses[k] <= mem_sizes[k] &&
                tls_memsz <= mem_sizes[k] - (tls_address - addresses[k]) &&
                tls_file >= offsets[k] && tls_file - offsets[k] <= file_sizes[k] &&
                tls_filesz <= file_sizes[k] - (tls_file - offsets[k]) &&
                tls_address - addresses[k] == tls_file - offsets[k])
                contained = 1;
        }
        if (!contained)
            return -1;
    }
    return entry_found && (!manifest_linux || phdr_found) ? 0 : -1;
}
static int verify(size_t n, const unsigned char expected[32]) {
    unsigned char hash[32];
    digest(archive, n, hash);
    if (memcmp(hash, expected, 32) || n < HEADER)
        return -1;
    manifest_v2 = !memcmp(archive, "ANOVAPK2", 8);
    if (!manifest_v2 && memcmp(archive, "ANOVAPK1", 8))
        return -1;
    header_size = manifest_v2 ? HEADER_V2 : HEADER;
    if (n < header_size || !safe_name((char *)archive + 8, 24) ||
        !field(archive + 48, 16, "x86_64") || !field(archive + 96, 16, "Apache-2.0"))
        return -1;
    manifest_linux = field(archive + 64, 32, "linux.x86_64.v1");
    if ((!manifest_linux && !field(archive + 64, 32, "anova.native.v1")) ||
        (!manifest_v2 && manifest_linux))
        return -1;
    unsigned count = word(archive + 44);
    if (count == 0 || count > FILES || !word(archive + 36) || word(archive + 40) != 1 ||
        word(archive + 112) != 1 || n < header_size + count * ENTRY)
        return -1;
    if (manifest_v2) {
        unsigned permissions = word(archive + 136), capabilities = word(archive + 140);
        if (!zero(archive + 8 + strlen((char *)archive + 8),
                  24 - strlen((char *)archive + 8)) ||
            !zero(archive + 32, 4) || !zero(archive + 116, 12) || word(archive + 128) != 2 ||
            word(archive + 132) != HEADER_V2 || permissions & ~3u || capabilities & ~(manifest_linux ? 7u : 3u) ||
            !(permissions & 1) || !(capabilities & 1) ||
            ((permissions & 2) && !(capabilities & 2)) ||
            ((permissions & 4) && !(capabilities & 8)) ||
            (manifest_linux && !(capabilities & 4)) ||
            !safe_name((char *)archive + 144, 48) ||
            !zero(archive + 144 + strlen((char *)archive + 144),
                  48 - strlen((char *)archive + 144)) ||
            !field(archive + 192, 32, manifest_linux ? "anova.linux.musl" : "anova.sdk") ||
            !word(archive + 224) || word(archive + 224) > (manifest_linux ? 1204u : 1u) ||
            !zero(archive + 228, 28))
            return -1;
    }
    size_t at = header_size + count * ENTRY;
    int license = 0, entry_found = 0;
    for (unsigned k = 0; k < count; k++) {
        unsigned char *entry = archive + header_size + k * ENTRY;
        unsigned mode = word(entry + 48), size = word(entry + 52);
        if (!safe_name((char *)entry, 48) || size > n - at || (mode != 0644 && mode != 0755))
            return -1;
        for (unsigned j = 0; j < k; j++)
            if (!strcmp((char *)entry, (char *)archive + header_size + j * ENTRY))
                return -1;
        digest(archive + at, size, hash);
        if (memcmp(hash, entry + 56, 32))
            return -1;
        if (!strcmp((char *)entry, "LICENSE") && mode == 0644 && size)
            license = 1;
        if (manifest_v2) {
            if (!zero(entry + strlen((char *)entry), 48 - strlen((char *)entry)) ||
                !zero(entry + 88, 8))
                return -1;
            if (!strcmp((char *)entry, (char *)archive + 144)) {
                if (mode != 0755 || executable(archive + at, size))
                    return -1;
                entry_found = 1;
            } else if (mode != 0644)
                return -1;
        }
        at += size;
    }
    return at == n && license && (!manifest_v2 || entry_found) ? 0 : -1;
}
/* The filesystem assigns one ABI to all persistent executables. Check it before mkdir,
 * cleanup or any write so even a valid archive cannot cross that execution boundary. */
static int target_policy(void) {
    unsigned char policy[32];
    size_t n;
    const char *expected = manifest_linux ? "linux.x86_64.v1\n" : "anova.native.v1\n";
    return read_file(ANOVA_PKG_PERSIST "/.anova-abi", policy, sizeof policy, &n) ||
                   n != strlen(expected) || memcmp(policy, expected, n)
               ? -1
               : 0;
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
    if (verify(n, expected) || target_policy()) {
        puts("[PKG:REJECT] checksum, ABI, target policy, dependency, capability, permission, entry, path or license");
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
    if (directory(root) || directory(ANOVA_PKG_PERSIST "/data")) {
        return -1;
    }
    char path[128];
    snprintf(path, sizeof path, ANOVA_PKG_PERSIST "/data/%.23s", name);
    if (directory(path))
        return -1;
    unsigned slot = 1 - old.slot;
    snprintf(path, sizeof path, "%s/%.23s-%c", root, name, slot ? 'b' : 'a');
    if (directory(path))
        return -1;
    cleanup(name, slot);
    unsigned count = word(archive + 44);
    size_t at = header_size + count * ENTRY;
    for (unsigned k = 0; k < count; k++) {
        unsigned char *entry = archive + header_size + k * ENTRY;
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
    if (write_file(path, archive, header_size + count * ENTRY))
        goto failed;
    struct state next = {
        .generation = old.generation + 1, .version = version, .present = 1, .slot = slot};
    memcpy(next.package, expected, 32);
    if (commit(name, &next))
        goto failed;
    printf("[PKG:INSTALLED] name=%s version=%u slot=%c abi=%s license=Apache-2.0\n",
           name, version, slot ? 'b' : 'a', archive + 64);
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
    if (argc == 4 && (!strcmp(argv[1], "check") || !strcmp(argv[1], "inspect"))) {
        unsigned char expected[32];
        size_t size;
        if (hex(argv[3], expected) || read_file(argv[2], archive, sizeof archive, &size) ||
            verify(size, expected))
            return 1;
        puts("[PKG:CHECKED] integrity=true ABI=true paths=true dependency=true license=true");
        if (!strcmp(argv[1], "inspect")) {
            printf("[PKG:MANIFEST] format=%u name=%s version=%u abi=%s arch=x86_64 sdk=1",
                   manifest_v2 ? 2u : 1u, archive + 8, word(archive + 36), archive + 64);
            if (manifest_v2)
                printf(" entry=%s permissions=%u capabilities=%u dependency=%s>=%u", archive + 144,
                       word(archive + 136), word(archive + 140), archive + 192, word(archive + 224));
            puts("");
        }
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
        unsigned version;
        if (number(argv[3], &version) || locked(argv[2], version, path, expected))
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
    } else if (argc == 4 && (!strcmp(argv[1], "install-file") || !strcmp(argv[1], "fail-file"))) {
        if (!strcmp(argv[1], "fail-file"))
            fail_after = 0;
        if (hex(argv[3], expected) || read_file(argv[2], archive, sizeof archive, &n))
            return 1;
    } else {
        puts("Usage: pkg install NAME VERSION | fetch NAME VERSION PORT | remove NAME | status "
             "NAME | install-file PATH SHA256 | check PATH SHA256 | inspect PATH SHA256");
        return 2;
    }
    return install(n, expected, fail_after) ? 1 : 0;
}
