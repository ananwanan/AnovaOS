// SPDX-License-Identifier: Apache-2.0
// Construct the fixed AnovaOS UEFI optical image from independently supplied regular files.
// Stream ECMA-119 sectors and an El Torito boot catalog to a recoverable publication candidate.
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

enum {
    SECTOR = 2048,
    CHUNK = 4096,
    MAX_PATH = 511,
    MAX_FILE = 16 * 1024 * 1024,
    MAX_IMAGE = 32 * 1024 * 1024,
    EFI_BYTES = 2949120,
    PRIMARY = 16,
    BOOT_RECORD,
    TERMINATOR,
    LITTLE_PATHS,
    BIG_PATHS,
    CATALOG,
    DIRECTORIES,
};

// Directories are in ECMA-119 path-table order, by depth, parent, then identifier.
enum {ROOT, BOOT, EFI, APPS, LIMINE, EFI_BOOT, DIRECTORY_COUNT};
static const struct directory {
    unsigned parent;
    const char *iso_name;
    const char *name;
    unsigned links;
} directories[] = {
    {ROOT, "", "", 4},
    {ROOT, "BOOT", "boot", 4},
    {ROOT, "EFI", "EFI", 3},
    {BOOT, "APPS", "apps", 2},
    {BOOT, "LIMINE", "limine", 2},
    {EFI, "BOOT", "BOOT", 2},
};
_Static_assert(DIRECTORY_COUNT == sizeof directories / sizeof *directories,
               "Every path-table directory needs a layout entry");

// Entries in each directory follow ISO identifier order. Rock Ridge names preserve case.
enum {KERNEL = 1, LIMINE_LICENSE = 3, BOOT_IMAGE = 5};
static const struct input {
    unsigned parent;
    const char *iso_name;
    const char *name;
} inputs[] = {
    {BOOT, "INITRAMFS.ANOVAFS;1", "initramfs.anovafs"},
    {BOOT, "KERNEL.ELF;1", "kernel.elf"},
    {EFI_BOOT, "BOOTX64.EFI;1", "BOOTX64.EFI"},
    {LIMINE, "LICENSE.;1", "LICENSE"},
    {LIMINE, "LIMINE.CONF;1", "limine.conf"},
    {LIMINE, "LIMINE_UEFI_CD.BIN;1", "limine-uefi-cd.bin"},
    {APPS, "COMPOSITOR.ELF;1", "compositor.elf"},
    {APPS, "EDIT.ELF;1", "edit.elf"},
    {APPS, "FS_TEST.ELF;1", "fs-test.elf"},
    {APPS, "GUI_CLIENT.ELF;1", "gui-client.elf"},
    {APPS, "GUI_RUN.ELF;1", "gui-run.elf"},
    {APPS, "GUI_TEST.ELF;1", "gui-test.elf"},
    {APPS, "HELLO_C.ELF;1", "hello-c.elf"},
    {APPS, "HELLO_RUST.ELF;1", "hello-rust.elf"},
    {APPS, "INIT.ELF;1", "init.elf"},
    {APPS, "MANIFEST.TOML;1", "manifest.toml"},
    {APPS, "PROBE.ELF;1", "probe.elf"},
    {APPS, "PROCESS_TEST.ELF;1", "process-test.elf"},
    {APPS, "SHELL.ELF;1", "shell.elf"},
};
#define INPUT_COUNT (sizeof inputs / sizeof *inputs)
_Static_assert(INPUT_COUNT + 4 <= 32, "Image exceeds the guest FD budget");

struct file {
    int fd;
    uint32_t first;
    uint32_t size;
};

struct stream {
    int fd;
    size_t used;
    uint8_t data[CHUNK];
};

static int failure(const char *operation, const char *path) {
    fprintf(stderr, "m18-image: %s %s: %s\n", operation, path, strerror(errno));
    return -1;
}

static void le16(uint8_t *p, uint16_t value) {
    p[0] = (uint8_t)value;
    p[1] = (uint8_t)(value >> 8);
}

static void le32(uint8_t *p, uint32_t value) {
    for (unsigned i = 0; i < 4; ++i) p[i] = (uint8_t)(value >> (i * 8));
}

static void be32(uint8_t *p, uint32_t value) {
    for (unsigned i = 0; i < 4; ++i) p[3 - i] = (uint8_t)(value >> (i * 8));
}

static void both16(uint8_t *p, uint16_t value) {
    le16(p, value);
    p[2] = p[1];
    p[3] = p[0];
}

static void both32(uint8_t *p, uint32_t value) {
    le32(p, value);
    be32(p + 4, value);
}

static uint8_t *system_use(uint8_t *p, const char *code, uint8_t size) {
    memcpy(p, code, 2);
    p[2] = size;
    p[3] = 1;
    return p + 4;
}

static size_t record(uint8_t *p, uint32_t first, uint32_t size, int directory,
                     uint32_t links, const char *identifier, size_t length, const char *name,
                     int root_self, int rock_ridge) {
    static const uint8_t timestamp[] = {126, 1, 1, 0, 0, 0, 0};
    both32(p + 2, first);
    both32(p + 10, size);
    memcpy(p + 18, timestamp, sizeof timestamp);
    p[25] = directory ? 2 : 0;
    both16(p + 28, 1);
    p[32] = (uint8_t)length;
    memcpy(p + 33, identifier, length);
    uint8_t *next = p + 33 + length + (length % 2 == 0);
    if (root_self) {
        uint8_t *field = system_use(next, "SP", 7);
        *field++ = 0xbe;
        *field++ = 0xef;
        *field++ = 0;
        static const char extension[] = "RRIP_1991A";
        static const char description[] = "ROCK RIDGE INTERCHANGE PROTOCOL";
        static const char source[] = "ROCK RIDGE TECHNICAL WORKING GROUP";
        size_t extension_size = sizeof extension - 1;
        size_t description_size = sizeof description - 1;
        size_t source_size = sizeof source - 1;
        field = system_use(field, "ER", (uint8_t)(8 + extension_size + description_size + source_size));
        *field++ = (uint8_t)extension_size;
        *field++ = (uint8_t)description_size;
        *field++ = (uint8_t)source_size;
        *field++ = 1;
        memcpy(field, extension, extension_size);
        memcpy(field + extension_size, description, description_size);
        memcpy(field + extension_size + description_size, source, source_size);
        next = field + extension_size + description_size + source_size;
    }
    if (rock_ridge) {
        uint8_t *field = system_use(next, "RR", 5);
        *field++ = name ? 0x09 : 0x01; // PX and optional NM.
        field = system_use(field, "PX", 36);
        both32(field, directory ? 0040755 : 0100644);
        both32(field + 8, links);
        next = field + 32; // UID and GID remain zero.
        if (name) {
            size_t name_size = strlen(name);
            field = system_use(next, "NM", (uint8_t)(5 + name_size));
            *field++ = 0;
            memcpy(field, name, name_size);
            next = field + name_size;
        }
    }
    if ((next - p) % 2) ++next;
    p[0] = (uint8_t)(next - p);
    return (size_t)(next - p);
}

static int dir_sector(uint8_t sector[SECTOR], unsigned number, const struct file *files) {
    unsigned parent = directories[number].parent;
    size_t at = record(sector, DIRECTORIES + number, SECTOR, 1, directories[number].links,
                       "\0", 1, NULL, number == ROOT, 1);
    at += record(sector + at, DIRECTORIES + parent, SECTOR, 1, directories[parent].links,
                 "\1", 1, NULL, 0, 1);
    unsigned child_dir = 1;
    size_t child_file = 0;
    for (;;) {
        while (child_dir < DIRECTORY_COUNT && directories[child_dir].parent != number) ++child_dir;
        while (child_file < INPUT_COUNT && inputs[child_file].parent != number) ++child_file;
        if (child_dir == DIRECTORY_COUNT && child_file == INPUT_COUNT) return 0;
        uint8_t bytes[256] = {0};
        size_t size;
        if (child_file == INPUT_COUNT || (child_dir < DIRECTORY_COUNT
            && strcmp(directories[child_dir].iso_name, inputs[child_file].iso_name) < 0)) {
            const struct directory *entry = &directories[child_dir++];
            size = record(bytes, DIRECTORIES + child_dir - 1, SECTOR, 1, entry->links,
                          entry->iso_name, strlen(entry->iso_name), entry->name, 0, 1);
        } else {
            const struct file *file = &files[child_file];
            const struct input *entry = &inputs[child_file++];
            size = record(bytes, file->first, file->size, 0, 1,
                          entry->iso_name, strlen(entry->iso_name), entry->name, 0, 1);
        }
        if (size > SECTOR - at) {
            errno = EOVERFLOW;
            return -1;
        }
        memcpy(sector + at, bytes, size);
        at += size;
    }
}

static size_t path_table(uint8_t sector[SECTOR], int big_endian) {
    size_t at = 0;
    for (unsigned i = 0; i < DIRECTORY_COUNT; ++i) {
        const struct directory *entry = &directories[i];
        size_t length = i == ROOT ? 1 : strlen(entry->iso_name);
        uint8_t *p = sector + at;
        p[0] = (uint8_t)length;
        if (big_endian) {
            be32(p + 2, DIRECTORIES + i);
            p[7] = (uint8_t)(entry->parent + 1);
        } else {
            le32(p + 2, DIRECTORIES + i);
            p[6] = (uint8_t)(entry->parent + 1);
        }
        memcpy(p + 8, entry->iso_name, length);
        at += 8 + length + length % 2;
    }
    return at;
}

static void descriptor(uint8_t sector[SECTOR], uint8_t type) {
    sector[0] = type;
    memcpy(sector + 1, "CD001", 5);
    sector[6] = 1;
}

static void primary(uint8_t sector[SECTOR], uint32_t sectors) {
    uint8_t paths[SECTOR] = {0};
    descriptor(sector, 1);
    memset(sector + 8, ' ', 64);
    memcpy(sector + 40, "ANOVAOS", 7);
    both32(sector + 80, sectors);
    both16(sector + 120, 1); // Volume set size.
    both16(sector + 124, 1); // Volume sequence number.
    both16(sector + 128, SECTOR);
    both32(sector + 132, (uint32_t)path_table(paths, 0));
    le32(sector + 140, LITTLE_PATHS);
    be32(sector + 148, BIG_PATHS);
    record(sector + 156, DIRECTORIES, SECTOR, 1, 1, "\0", 1, NULL, 0, 0);
    memset(sector + 190, ' ', 623);
    memcpy(sector + 446, "ANOVAOS", 7);
    memcpy(sector + 813, "2026010100000000", 16);
    memcpy(sector + 830, "2026010100000000", 16);
    memcpy(sector + 847, "0000000000000000", 16);
    memcpy(sector + 864, "0000000000000000", 16);
    sector[881] = 1; // File structure version.
}

static void catalog(uint8_t sector[SECTOR], const struct file *efi) {
    sector[0] = 1;
    sector[1] = 0xef; // UEFI platform.
    memcpy(sector + 4, "AnovaOS", 7);
    sector[30] = 0x55;
    sector[31] = 0xaa;
    uint16_t checksum = 0;
    for (size_t i = 0; i < 32; i += 2) {
        checksum += (uint16_t)(sector[i] | (sector[i + 1] << 8));
    }
    le16(sector + 28, (uint16_t)-checksum);
    sector[32] = 0x88; // Bootable, no emulation.
    le16(sector + 38, (uint16_t)(efi->size / 512));
    le32(sector + 40, efi->first);
}

static int flush(struct stream *output) {
    size_t at = 0;
    while (at < output->used) {
        ssize_t written = write(output->fd, output->data + at, output->used - at);
        if (written < 0 && errno == EINTR) continue;
        if (written <= 0) {
            if (written == 0) errno = EIO;
            return -1;
        }
        at += (size_t)written;
    }
    output->used = 0;
    return 0;
}

static int emit(struct stream *output, const uint8_t *data, size_t size) {
    while (size) {
        size_t copy = sizeof output->data - output->used;
        if (copy > size) copy = size;
        memcpy(output->data + output->used, data, copy);
        output->used += copy;
        data += copy;
        size -= copy;
        if (output->used == sizeof output->data && flush(output)) return -1;
    }
    return 0;
}

static ssize_t read_chunk(int fd, uint8_t *bytes, size_t size) {
    ssize_t length;
    do length = read(fd, bytes, size); while (length < 0 && errno == EINTR);
    return length;
}

static int write_image(struct stream *output, const struct file *files, uint32_t sectors) {
    uint8_t sector[SECTOR];
    // ECMA-119 leaves the 16 system sectors before the descriptors unused in this profile.
    if (lseek(output->fd, PRIMARY * SECTOR, SEEK_SET) != PRIMARY * SECTOR) return -1;
    for (unsigned i = PRIMARY; i < DIRECTORIES + DIRECTORY_COUNT; ++i) {
        memset(sector, 0, sizeof sector);
        switch (i) {
        case PRIMARY: primary(sector, sectors); break;
        case BOOT_RECORD:
            descriptor(sector, 0);
            memcpy(sector + 7, "EL TORITO SPECIFICATION", 23);
            le32(sector + 71, CATALOG);
            break;
        case TERMINATOR: descriptor(sector, 255); break;
        case LITTLE_PATHS: path_table(sector, 0); break;
        case BIG_PATHS: path_table(sector, 1); break;
        case CATALOG: catalog(sector, &files[BOOT_IMAGE]); break;
        default:
            if (dir_sector(sector, i - DIRECTORIES, files)) return -1;
        }
        if (emit(output, sector, sizeof sector)) return -1;
    }
    static const uint8_t zeros[SECTOR];
    uint8_t bytes[CHUNK];
    for (size_t i = 0; i < INPUT_COUNT; ++i) {
        const struct file *file = &files[i];
        uint32_t left = file->size;
        while (left) {
            ssize_t length = read_chunk(file->fd, bytes, left < CHUNK ? left : CHUNK);
            if (length <= 0) {
                if (length == 0) errno = EIO;
                return -1;
            }
            if (emit(output, bytes, (size_t)length)) return -1;
            left -= (uint32_t)length;
        }
        ssize_t extra = read_chunk(file->fd, bytes, 1);
        if (extra != 0) {
            if (extra > 0) errno = EIO;
            return -1;
        }
        size_t padding = (SECTOR - file->size % SECTOR) % SECTOR;
        if (emit(output, zeros, padding)) return -1;
    }
    return flush(output);
}

static int open_inputs(struct file files[INPUT_COUNT], const char *kernel, const char *base,
                       const char *output, uint32_t *sectors) {
    struct stat previous;
    int present = lstat(output, &previous) == 0;
    if (!present && errno != ENOENT) return failure("inspect output", output);
    if (present && S_ISLNK(previous.st_mode)) {
        errno = ELOOP;
        return failure("reject redirected output", output);
    }
    *sectors = DIRECTORIES + DIRECTORY_COUNT;
    for (size_t i = 0; i < INPUT_COUNT; ++i) {
        char joined[MAX_PATH + 1];
        const char *path = kernel;
        if (i != KERNEL) {
            const char *name = i == LIMINE_LICENSE ? "LIMINE-LICENSE" : inputs[i].name;
            int length = snprintf(joined, sizeof joined, "%s/%s", base, name);
            if (length < 0 || (size_t)length >= sizeof joined) {
                errno = ENAMETOOLONG;
                return failure("construct input path", base);
            }
            path = joined;
        }
        struct stat info;
        files[i].fd = open(path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK);
        if (files[i].fd < 0) return failure("open input", path);
        if (fstat(files[i].fd, &info)) return failure("inspect input", path);
        if (!S_ISREG(info.st_mode) || info.st_size <= 0 || info.st_size > MAX_FILE
            || (i == BOOT_IMAGE && info.st_size != EFI_BYTES)) {
            errno = EINVAL;
            return failure("reject input type or size", path);
        }
        if (present && previous.st_dev == info.st_dev && previous.st_ino == info.st_ino) {
            errno = EINVAL;
            return failure("reject output alias of input", path);
        }
        files[i].size = (uint32_t)info.st_size;
        files[i].first = *sectors;
        *sectors += (files[i].size + SECTOR - 1) / SECTOR;
        if (*sectors > MAX_IMAGE / SECTOR) {
            errno = EFBIG;
            return failure("reject image size", output);
        }
    }
    return 0;
}

static int candidate_path(char path[MAX_PATH + 1], const char *output) {
    int length = snprintf(path, MAX_PATH + 1, "%s.candidate", output);
    if (length >= 0 && length <= MAX_PATH) return 0;
    errno = ENAMETOOLONG;
    return failure("construct candidate path", output);
}

static int cleanup(const char *output) {
    char candidate[MAX_PATH + 1];
    if (candidate_path(candidate, output)) return -1;
    struct stat info;
    if (lstat(candidate, &info)) {
        if (errno == ENOENT) return 0;
        return failure("inspect stale candidate", candidate);
    }
    if (!S_ISREG(info.st_mode)) {
        errno = EINVAL;
        return failure("reject non-regular candidate", candidate);
    }
    if (unlink(candidate)) return failure("remove stale candidate", candidate);
    printf("[M18:IMAGE:CLEANUP] path=%s\n", candidate);
    return 0;
}

static int publish(const struct file *files, uint32_t sectors, const char *output) {
    char candidate[MAX_PATH + 1];
    if (candidate_path(candidate, output)) return -1;
    struct stream stream = {.fd = open(candidate, O_WRONLY | O_CREAT | O_EXCL, 0600)};
    if (stream.fd < 0) return failure("create candidate", candidate);
    printf("[M18:IMAGE:WRITE] bytes=%u path=%s\n", sectors * SECTOR, candidate);
    fflush(stdout);
    int result = 0;
    if (write_image(&stream, files, sectors)) result = failure("stream image", candidate);
    struct stat info;
    if (!result && fstat(stream.fd, &info)) result = failure("inspect candidate", candidate);
    if (!result && info.st_size != (off_t)sectors * SECTOR) {
        errno = EIO;
        result = failure("reject candidate size", candidate);
    }
    if (!result && fsync(stream.fd)) result = failure("sync candidate", candidate);
    if (close(stream.fd) && !result) result = failure("close candidate", candidate);
    if (!result && rename(candidate, output)) result = failure("publish candidate", output);
    if (result) {
        if (unlink(candidate)) failure("remove rejected candidate", candidate);
        return result;
    }
    printf("[M18:IMAGE:PUBLISHED] bytes=%u path=%s\n", sectors * SECTOR, output);
    return 0;
}

int main(int argc, char *argv[]) {
    if (argc == 3 && !strcmp(argv[1], "cleanup")) return cleanup(argv[2]) ? 1 : 0;
    if (argc != 4 || !argv[1][0] || !argv[2][0] || !argv[3][0]) {
        fputs("Usage: m18-image KERNEL INPUT_DIRECTORY OUTPUT_ISO\n"
              "       m18-image cleanup OUTPUT_ISO\n", stderr);
        return 2;
    }
    struct file files[INPUT_COUNT];
    for (size_t i = 0; i < INPUT_COUNT; ++i) files[i].fd = -1;
    uint32_t sectors = 0;
    int result = open_inputs(files, argv[1], argv[2], argv[3], &sectors);
    if (!result) result = publish(files, sectors, argv[3]);
    for (size_t i = 0; i < INPUT_COUNT; ++i) {
        if (files[i].fd >= 0) close(files[i].fd);
    }
    return result ? 1 : 0;
}
