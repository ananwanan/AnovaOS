/* SPDX-License-Identifier: Apache-2.0 */
/* Native v1 C wire layout and syscall wrappers; no libc or Linux ABI dependency. */
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
__attribute__((noreturn)) static inline void anova_exit(uint32_t status) {
    anova_syscall(1, status, 0, 0);
    __builtin_trap();
}
#endif
