/* SPDX-License-Identifier: Apache-2.0 */
/* Native v1 console/process wire layouts and wrappers; no libc or Linux dependency. */
#ifndef ANOVA_NATIVE_H
#define ANOVA_NATIVE_H
#include <stdint.h>
#include <stddef.h>

#define ANOVA_ABI_VERSION 1
struct anova_info { uint32_t abi_version, size; uint64_t page_size, ticks, process_id; };
_Static_assert(sizeof(struct anova_info) == 32, "Native info layout mismatch");
_Static_assert(offsetof(struct anova_info, process_id) == 24, "Native info offset mismatch");

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
_Static_assert(sizeof(struct anova_exec_request) == 32, "Native exec layout mismatch");
_Static_assert(sizeof(struct anova_wait_status) == 16, "Native wait layout mismatch");
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
__attribute__((noreturn)) static inline void anova_exit(uint32_t status) {
    anova_syscall(1, status, 0, 0);
    __builtin_trap();
}
#endif
