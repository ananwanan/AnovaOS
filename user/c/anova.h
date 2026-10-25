/* SPDX-License-Identifier: Apache-2.0 */
/* Frozen native v1 wire layouts, stable errors and C/C++ syscall wrappers; no libc. */
#ifndef ANOVA_NATIVE_H
#define ANOVA_NATIVE_H
#include <stdint.h>
#include <stddef.h>

#define ANOVA_ABI_VERSION 1
#define ANOVA_SDK_VERSION "1.0.0"
#define ANOVA_ABI_PERSONALITY "anova.native.v1"
#define ANOVA_MAX_ARGS 64
#define ANOVA_MAX_ARGUMENT_BYTES 1023
#ifdef __cplusplus
#define ANOVA_STATIC_ASSERT static_assert
#else
#define ANOVA_STATIC_ASSERT _Static_assert
#endif
#define ANOVA_BAD_ADDRESS (-1)
#define ANOVA_UNSUPPORTED (-2)
#define ANOVA_INVALID_ARGUMENT (-3)
#define ANOVA_TOO_LONG (-4)
#define ANOVA_NO_CHILD (-5)
#define ANOVA_NO_MEMORY (-6)
#define ANOVA_NOT_FOUND (-7)
#define ANOVA_PROCESS_LIMIT (-8)
#define ANOVA_BAD_DESCRIPTOR (-9)
#define ANOVA_FILE_LIMIT (-10)
#define ANOVA_READ_ONLY (-11)
#define ANOVA_NOT_DIRECTORY (-12)
#define ANOVA_IS_DIRECTORY (-13)
#define ANOVA_ALREADY_EXISTS (-14)
#define ANOVA_WOULD_BLOCK (-15)
#define ANOVA_BROKEN_PIPE (-16)
#define ANOVA_NOT_SEEKABLE (-17)
#define ANOVA_NO_SPACE (-18)
#define ANOVA_LINK_LOOP (-19)
#define ANOVA_PERMISSION_DENIED (-20)
#define ANOVA_IO_ERROR (-21)
#define ANOVA_INVALID_FILESYSTEM (-22)
#define ANOVA_DEVICE_TIMEOUT (-23)
#define ANOVA_BUSY (-24)
#define ANOVA_NOT_EMPTY (-25)
#define ANOVA_OPEN_READ 1
#define ANOVA_OPEN_WRITE 2
#define ANOVA_OPEN_CREATE 4
#define ANOVA_OPEN_TRUNCATE 8
#define ANOVA_OPEN_APPEND 16
#define ANOVA_OPEN_DIRECTORY 32
#define ANOVA_OPEN_EXCLUSIVE 64

/* Every negative value is an error; unknown values retain their original number. */
static inline const char *anova_error_name(int64_t value) {
    static const char *const names[] = {
        "bad_address", "unsupported", "invalid_argument", "too_long", "no_child",
        "no_memory", "not_found", "process_limit", "bad_descriptor", "file_limit",
        "read_only", "not_directory", "is_directory", "already_exists", "would_block",
        "broken_pipe", "not_seekable", "no_space", "link_loop", "permission_denied",
        "io_error", "invalid_filesystem", "device_timeout", "busy", "not_empty"
    };
    return value < 0 && value >= -25 ? names[(size_t)(-value - 1)] :
        value < 0 ? "unknown_native_error" : "success";
}
struct anova_info { uint32_t abi_version, size; uint64_t page_size, ticks, process_id; };
ANOVA_STATIC_ASSERT(sizeof(struct anova_info) == 32, "Native info layout mismatch");
ANOVA_STATIC_ASSERT(offsetof(struct anova_info, process_id) == 24, "Native info offset mismatch");

static inline int64_t anova_syscall(uint64_t number, uint64_t a, uint64_t b, uint64_t c) {
    int64_t result;
    __asm__ volatile ("syscall" : "=a"(result) : "a"(number), "D"(a), "S"(b), "d"(c) : "rcx", "r11", "memory", "cc");
    return result;
}
static inline int64_t anova_write(const char *bytes, size_t length) {
    return anova_syscall(0, 1, (uintptr_t)bytes, length);
}
static inline int64_t anova_get_info(struct anova_info *info) {
    return anova_syscall(2, (uintptr_t)info, sizeof(*info), 0);
}
struct anova_exec_request { uint64_t name, name_len, argument, argument_len; };
struct anova_wait_status { uint64_t pid; uint32_t reason, code; };
ANOVA_STATIC_ASSERT(sizeof(struct anova_exec_request) == 32, "Native exec layout mismatch");
ANOVA_STATIC_ASSERT(sizeof(struct anova_wait_status) == 16, "Native wait layout mismatch");
struct anova_dir_entry { uint64_t inode; uint32_t kind, name_len; uint8_t name[64]; };
ANOVA_STATIC_ASSERT(sizeof(struct anova_dir_entry) == 80, "Native directory layout mismatch");
ANOVA_STATIC_ASSERT(offsetof(struct anova_dir_entry, name) == 16, "Native directory offset mismatch");
static inline int64_t anova_yield(void) { return anova_syscall(3, 0, 0, 0); }
static inline int64_t anova_sleep(uint64_t ticks) { return anova_syscall(4, ticks, 0, 0); }
static inline int64_t anova_spawn(const struct anova_exec_request *request) {
    return anova_syscall(5, (uintptr_t)request, sizeof(*request), 0);
}
static inline int64_t anova_exec(const struct anova_exec_request *request) {
    return anova_syscall(6, (uintptr_t)request, sizeof(*request), 0);
}
static inline int64_t anova_wait(uint64_t pid, struct anova_wait_status *status) {
    return anova_syscall(7, pid, (uintptr_t)status, sizeof(*status));
}
static inline int64_t anova_fork(void) { return anova_syscall(8, 0, 0, 0); }
static inline int64_t anova_close(uint64_t fd) { return anova_syscall(9, fd, 0, 0); }
static inline int64_t anova_read(uint64_t fd, void *bytes, size_t length) {
    return anova_syscall(10, fd, (uintptr_t)bytes, length);
}
static inline int64_t anova_write_fd(uint64_t fd, const void *bytes, size_t length) {
    return anova_syscall(0, fd, (uintptr_t)bytes, length);
}
static inline int64_t anova_write_all(uint64_t fd, const void *bytes, size_t length) {
    const uint8_t *cursor = (const uint8_t *)bytes;
    while (length != 0) {
        size_t part = length < 4096 ? length : 4096;
        int64_t result = anova_write_fd(fd, cursor, part);
        if (result < 0) return result;
        if (result == 0 || (uint64_t)result > part) return ANOVA_INVALID_ARGUMENT;
        cursor += result;
        length -= (size_t)result;
    }
    return 0;
}
static inline int64_t anova_open(const char *path, size_t length, uint64_t flags) {
    return anova_syscall(11, (uintptr_t)path, length, flags);
}
static inline int64_t anova_seek(uint64_t fd, int64_t offset, uint64_t whence) {
    return anova_syscall(12, fd, (uint64_t)offset, whence);
}
static inline int64_t anova_dup2(uint64_t source, uint64_t target) {
    return anova_syscall(13, source, target, 0);
}
static inline int64_t anova_pipe(uint64_t fds[2]) {
    return anova_syscall(14, (uintptr_t)fds, 16, 0);
}
static inline int64_t anova_readdir(uint64_t fd, struct anova_dir_entry *entry) {
    return anova_syscall(15, fd, (uintptr_t)entry, sizeof(*entry));
}
static inline int64_t anova_mkdir(const char *path, size_t length) {
    return anova_syscall(16, (uintptr_t)path, length, 0);
}
static inline int64_t anova_chdir(const char *path, size_t length) {
    return anova_syscall(17, (uintptr_t)path, length, 0);
}
static inline int64_t anova_getcwd(char *output, size_t length) {
    return anova_syscall(18, (uintptr_t)output, length, 0);
}
static inline int64_t anova_unlink(const char *path, size_t length) {
    return anova_syscall(19, (uintptr_t)path, length, 0);
}
static inline int64_t anova_sync(void) { return anova_syscall(20, 0, 0, 0); }
static inline int64_t anova_fsync(uint64_t fd) { return anova_syscall(21, fd, 0, 0); }
static inline int64_t anova_shutdown(void) { return anova_syscall(22, 0, 0, 0); }
static inline int64_t anova_set_executable(const char *path, size_t length) {
    return anova_syscall(23, (uintptr_t)path, length, 0);
}
__attribute__((noreturn)) static inline void anova_exit(uint32_t status) {
    anova_syscall(1, status, 0, 0);
    __builtin_trap();
}
#endif
