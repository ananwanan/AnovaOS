/* SPDX-License-Identifier: Apache-2.0 */
/* Produce a real native user fault through named, non-inlined frames for ELF symbolization. */
#include <stdint.h>

__attribute__((noinline)) static void m13_crash_leaf(void) {
    volatile uint64_t marker = 13;
    /* Intentional guest-only fault without an invalid C pointer dereference. */
#ifdef M13_INVALID_FRAME
    __asm__ volatile("mov $1, %%rbp; ud2" : "+m"(marker) :: "memory");
#else
    __asm__ volatile("ud2" : "+m"(marker));
#endif
}

__attribute__((noinline)) static void m13_crash_middle(void) {
    m13_crash_leaf();
    __asm__ volatile("" ::: "memory");
}

__attribute__((noinline)) static void m13_crash_entry(void) {
    m13_crash_middle();
    __asm__ volatile("" ::: "memory");
}

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    m13_crash_entry();
    return 99;
}
